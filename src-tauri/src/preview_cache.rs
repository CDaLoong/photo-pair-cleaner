use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

const CACHE_DATABASE_FILE: &str = "preview-cache-v2.sqlite3";
const LEGACY_INDEX_FILE: &str = "preview-cache-index-v1.json";
const ACCESS_FLUSH_COUNT: usize = 64;
const PRUNE_TARGET_PERCENT: u64 = 90;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreviewCacheStats {
    pub(crate) entry_count: usize,
    pub(crate) size_bytes: u64,
    pub(crate) budget_bytes: u64,
    pub(crate) max_entries: usize,
}

#[derive(Debug, Clone)]
struct PendingAccess {
    size_bytes: u64,
    max_edge: u32,
    last_access_ms: u64,
}

struct PreviewCacheState {
    connection: Connection,
    pending_accesses: HashMap<String, PendingAccess>,
    entry_count: usize,
    size_bytes: u64,
}

pub(crate) struct PreviewCache {
    root: PathBuf,
    budget_bytes: u64,
    max_entries: usize,
    state: Mutex<PreviewCacheState>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyIndex {
    schema_version: u8,
    entries: HashMap<String, LegacyEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyEntry {
    size_bytes: u64,
    last_access_ms: u64,
    #[serde(default)]
    max_edge: u32,
}

static PREVIEW_CACHES: LazyLock<Mutex<HashMap<PathBuf, Arc<PreviewCache>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn cache_error(context: &str, error: impl std::fmt::Display) -> String {
    format!("{context}：{error}")
}

fn sqlite_integer(value: u64) -> Result<i64, String> {
    i64::try_from(value).map_err(|_| "预览缓存数值超出了 SQLite 范围".to_string())
}

fn unsigned_integer(value: i64) -> u64 {
    value.max(0) as u64
}

fn validated_relative_path(value: &str) -> Result<PathBuf, String> {
    let path = Path::new(value);
    if value.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("缓存路径必须是安全相对路径".to_string());
    }
    Ok(path.to_path_buf())
}

fn configure_connection(connection: &Connection) -> Result<(), String> {
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|error| cache_error("无法设置预览缓存等待时间", error))?;
    connection
        .pragma_update(None, "journal_mode", "WAL")
        .map_err(|error| cache_error("无法启用预览缓存 WAL", error))?;
    connection
        .pragma_update(None, "synchronous", "NORMAL")
        .map_err(|error| cache_error("无法设置预览缓存同步模式", error))?;
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS preview_entries (
                relative_path TEXT PRIMARY KEY,
                size_bytes INTEGER NOT NULL CHECK(size_bytes >= 0),
                max_edge INTEGER NOT NULL CHECK(max_edge >= 0),
                last_access_ms INTEGER NOT NULL CHECK(last_access_ms >= 0)
            );
            CREATE INDEX IF NOT EXISTS preview_entries_lru
                ON preview_entries(last_access_ms, relative_path);",
        )
        .map_err(|error| cache_error("无法初始化预览缓存数据库", error))?;
    Ok(())
}

fn migrate_legacy_index(root: &Path, connection: &mut Connection) -> Result<(), String> {
    let legacy_path = root.join(LEGACY_INDEX_FILE);
    if !legacy_path.is_file() {
        return Ok(());
    }
    let bytes =
        fs::read(&legacy_path).map_err(|error| cache_error("无法读取旧版预览缓存索引", error))?;
    let legacy: LegacyIndex = match serde_json::from_slice::<LegacyIndex>(&bytes) {
        Ok(legacy) if legacy.schema_version == 1 => legacy,
        Ok(_) | Err(_) => {
            let invalid_path = root.join(format!("{LEGACY_INDEX_FILE}.invalid"));
            if invalid_path.exists() {
                fs::remove_file(&invalid_path)
                    .map_err(|error| cache_error("无法替换损坏缓存索引", error))?;
            }
            fs::rename(legacy_path, invalid_path)
                .map_err(|error| cache_error("无法隔离损坏缓存索引", error))?;
            return Ok(());
        }
    };

    let transaction = connection
        .transaction()
        .map_err(|error| cache_error("无法开始旧版缓存迁移", error))?;
    for (relative_path, entry) in legacy.entries {
        if validated_relative_path(&relative_path).is_err() || !root.join(&relative_path).is_file()
        {
            continue;
        }
        let size_bytes = sqlite_integer(entry.size_bytes)?;
        let last_access_ms = sqlite_integer(entry.last_access_ms)?;
        transaction
            .execute(
                "INSERT INTO preview_entries (
                    relative_path, size_bytes, max_edge, last_access_ms
                 ) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(relative_path) DO NOTHING",
                params![relative_path, size_bytes, entry.max_edge, last_access_ms],
            )
            .map_err(|error| cache_error("无法迁移旧版预览缓存项", error))?;
    }
    transaction
        .commit()
        .map_err(|error| cache_error("无法提交旧版缓存迁移", error))?;

    let migrated_path = root.join(format!("{LEGACY_INDEX_FILE}.migrated"));
    if migrated_path.exists() {
        fs::remove_file(&migrated_path)
            .map_err(|error| cache_error("无法替换旧版缓存迁移标记", error))?;
    }
    fs::rename(legacy_path, migrated_path)
        .map_err(|error| cache_error("无法完成旧版缓存索引迁移", error))?;
    Ok(())
}

fn database_totals(connection: &Connection) -> Result<(usize, u64), String> {
    connection
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(size_bytes), 0) FROM preview_entries",
            [],
            |row| {
                let count = row.get::<_, i64>(0)?.max(0) as usize;
                let bytes = unsigned_integer(row.get::<_, i64>(1)?);
                Ok((count, bytes))
            },
        )
        .map_err(|error| cache_error("无法统计预览缓存", error))
}

impl PreviewCache {
    pub(crate) fn open(root: &Path, budget_bytes: u64, max_entries: usize) -> Result<Self, String> {
        if let Ok(metadata) = fs::symlink_metadata(root)
            && metadata.file_type().is_symlink()
        {
            return Err("预览缓存目录不能是符号链接".to_string());
        }
        fs::create_dir_all(root).map_err(|error| cache_error("无法创建预览缓存目录", error))?;
        let mut connection = Connection::open(root.join(CACHE_DATABASE_FILE))
            .map_err(|error| cache_error("无法打开预览缓存数据库", error))?;
        configure_connection(&connection)?;
        migrate_legacy_index(root, &mut connection)?;
        let (entry_count, size_bytes) = database_totals(&connection)?;
        Ok(Self {
            root: root.to_path_buf(),
            budget_bytes: budget_bytes.max(1),
            max_entries: max_entries.max(1),
            state: Mutex::new(PreviewCacheState {
                connection,
                pending_accesses: HashMap::new(),
                entry_count,
                size_bytes,
            }),
        })
    }

    pub(crate) fn record_generated(
        &self,
        relative_path: &str,
        size_bytes: u64,
        max_edge: u32,
        last_access_ms: u64,
    ) -> Result<(), String> {
        validated_relative_path(relative_path)?;
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let sqlite_size_bytes = sqlite_integer(size_bytes)?;
        let sqlite_last_access_ms = sqlite_integer(last_access_ms)?;
        state.pending_accesses.remove(relative_path);
        let old_size = state
            .connection
            .query_row(
                "SELECT size_bytes FROM preview_entries WHERE relative_path = ?1",
                [relative_path],
                |row| row.get::<_, i64>(0).map(unsigned_integer),
            )
            .optional()
            .map_err(|error| cache_error("无法读取预览缓存项", error))?;
        state
            .connection
            .execute(
                "INSERT INTO preview_entries (
                    relative_path, size_bytes, max_edge, last_access_ms
                 ) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(relative_path) DO UPDATE SET
                    size_bytes = excluded.size_bytes,
                    max_edge = excluded.max_edge,
                    last_access_ms = excluded.last_access_ms",
                params![
                    relative_path,
                    sqlite_size_bytes,
                    max_edge,
                    sqlite_last_access_ms
                ],
            )
            .map_err(|error| cache_error("无法记录预览缓存项", error))?;
        match old_size {
            Some(previous) => {
                state.size_bytes = state
                    .size_bytes
                    .saturating_sub(previous)
                    .saturating_add(size_bytes);
            }
            None => {
                state.entry_count = state.entry_count.saturating_add(1);
                state.size_bytes = state.size_bytes.saturating_add(size_bytes);
            }
        }
        self.prune_locked(&mut state, Some(relative_path))
    }

    pub(crate) fn record_access(
        &self,
        relative_path: &str,
        size_bytes: u64,
        max_edge: u32,
        last_access_ms: u64,
    ) -> Result<(), String> {
        validated_relative_path(relative_path)?;
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.pending_accesses.insert(
            relative_path.to_string(),
            PendingAccess {
                size_bytes,
                max_edge,
                last_access_ms,
            },
        );
        if state.pending_accesses.len() >= ACCESS_FLUSH_COUNT {
            Self::flush_locked(&mut state)?;
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) fn flush_accesses(&self) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Self::flush_locked(&mut state)
    }

    fn flush_locked(state: &mut PreviewCacheState) -> Result<(), String> {
        if state.pending_accesses.is_empty() {
            return Ok(());
        }
        let pending = std::mem::take(&mut state.pending_accesses);
        let transaction = state
            .connection
            .transaction()
            .map_err(|error| cache_error("无法开始预览缓存访问事务", error))?;
        let mut inserted = 0usize;
        let mut added_bytes = 0u64;
        let mut removed_bytes = 0u64;
        for (relative_path, access) in &pending {
            let sqlite_size_bytes = sqlite_integer(access.size_bytes)?;
            let sqlite_last_access_ms = sqlite_integer(access.last_access_ms)?;
            let old_size = transaction
                .query_row(
                    "SELECT size_bytes FROM preview_entries WHERE relative_path = ?1",
                    [relative_path],
                    |row| row.get::<_, i64>(0).map(unsigned_integer),
                )
                .optional()
                .map_err(|error| cache_error("无法读取待更新缓存项", error))?;
            transaction
                .execute(
                    "INSERT INTO preview_entries (
                        relative_path, size_bytes, max_edge, last_access_ms
                     ) VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(relative_path) DO UPDATE SET
                        size_bytes = excluded.size_bytes,
                        max_edge = excluded.max_edge,
                        last_access_ms = excluded.last_access_ms",
                    params![
                        relative_path,
                        sqlite_size_bytes,
                        access.max_edge,
                        sqlite_last_access_ms
                    ],
                )
                .map_err(|error| cache_error("无法更新预览缓存访问时间", error))?;
            match old_size {
                Some(previous_size) if access.size_bytes >= previous_size => {
                    added_bytes =
                        added_bytes.saturating_add(access.size_bytes.saturating_sub(previous_size));
                }
                Some(previous_size) => {
                    removed_bytes = removed_bytes
                        .saturating_add(previous_size.saturating_sub(access.size_bytes));
                }
                None => {
                    inserted = inserted.saturating_add(1);
                    added_bytes = added_bytes.saturating_add(access.size_bytes);
                }
            }
        }
        transaction
            .commit()
            .map_err(|error| cache_error("无法提交预览缓存访问事务", error))?;
        state.entry_count = state.entry_count.saturating_add(inserted);
        state.size_bytes = state
            .size_bytes
            .saturating_sub(removed_bytes)
            .saturating_add(added_bytes);
        Ok(())
    }

    pub(crate) fn remove_missing(&self, relative_path: &str) -> Result<(), String> {
        validated_relative_path(relative_path)?;
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.pending_accesses.remove(relative_path);
        let old_size = state
            .connection
            .query_row(
                "SELECT size_bytes FROM preview_entries WHERE relative_path = ?1",
                [relative_path],
                |row| row.get::<_, i64>(0).map(unsigned_integer),
            )
            .optional()
            .map_err(|error| cache_error("无法读取缺失预览缓存项", error))?;
        if let Some(size_bytes) = old_size {
            state
                .connection
                .execute(
                    "DELETE FROM preview_entries WHERE relative_path = ?1",
                    [relative_path],
                )
                .map_err(|error| cache_error("无法删除缺失预览缓存项", error))?;
            state.entry_count = state.entry_count.saturating_sub(1);
            state.size_bytes = state.size_bytes.saturating_sub(size_bytes);
        }
        Ok(())
    }

    fn prune_locked(
        &self,
        state: &mut PreviewCacheState,
        protected_path: Option<&str>,
    ) -> Result<(), String> {
        if state.size_bytes <= self.budget_bytes && state.entry_count <= self.max_entries {
            return Ok(());
        }
        let target_bytes = self
            .budget_bytes
            .saturating_mul(PRUNE_TARGET_PERCENT)
            .checked_div(100)
            .unwrap_or(self.budget_bytes)
            .max(1);
        let target_entries = self
            .max_entries
            .saturating_mul(PRUNE_TARGET_PERCENT as usize)
            .checked_div(100)
            .unwrap_or(self.max_entries)
            .max(1);

        while state.size_bytes > target_bytes || state.entry_count > target_entries {
            let candidates = {
                let mut statement = state
                    .connection
                    .prepare(
                        "SELECT relative_path, size_bytes
                         FROM preview_entries
                         WHERE (?1 IS NULL OR relative_path != ?1)
                         ORDER BY last_access_ms ASC, relative_path ASC
                         LIMIT 64",
                    )
                    .map_err(|error| cache_error("无法准备预览缓存淘汰", error))?;
                let rows = statement
                    .query_map([protected_path], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            unsigned_integer(row.get::<_, i64>(1)?),
                        ))
                    })
                    .map_err(|error| cache_error("无法查询预览缓存淘汰项", error))?;
                rows.collect::<Result<Vec<_>, _>>()
                    .map_err(|error| cache_error("无法读取预览缓存淘汰项", error))?
            };
            if candidates.is_empty() {
                break;
            }
            let mut removed_any = false;
            for (relative_path, size_bytes) in candidates {
                if state.size_bytes <= target_bytes && state.entry_count <= target_entries {
                    break;
                }
                match fs::remove_file(self.root.join(&relative_path)) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(_) => continue,
                }
                state
                    .connection
                    .execute(
                        "DELETE FROM preview_entries WHERE relative_path = ?1",
                        [&relative_path],
                    )
                    .map_err(|error| cache_error("无法提交预览缓存淘汰", error))?;
                state.entry_count = state.entry_count.saturating_sub(1);
                state.size_bytes = state.size_bytes.saturating_sub(size_bytes);
                removed_any = true;
            }
            if !removed_any {
                break;
            }
        }
        Ok(())
    }

    pub(crate) fn stats(&self) -> Result<PreviewCacheStats, String> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Self::flush_locked(&mut state)?;
        Ok(PreviewCacheStats {
            entry_count: state.entry_count,
            size_bytes: state.size_bytes,
            budget_bytes: self.budget_bytes,
            max_entries: self.max_entries,
        })
    }

    #[allow(dead_code)]
    pub(crate) fn pending_access_count(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pending_accesses
            .len()
    }

    #[allow(dead_code)]
    pub(crate) fn last_access_ms(&self, relative_path: &str) -> Result<Option<u64>, String> {
        validated_relative_path(relative_path)?;
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .connection
            .query_row(
                "SELECT last_access_ms FROM preview_entries WHERE relative_path = ?1",
                [relative_path],
                |row| row.get::<_, i64>(0).map(unsigned_integer),
            )
            .optional()
            .map_err(|error| cache_error("无法读取预览缓存访问时间", error))
    }
}

pub(crate) fn cache_for(
    root: &Path,
    budget_bytes: u64,
    max_entries: usize,
) -> Result<Arc<PreviewCache>, String> {
    let key = root.to_path_buf();
    let mut caches = PREVIEW_CACHES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(cache) = caches.get(&key) {
        return Ok(Arc::clone(cache));
    }
    let cache = Arc::new(PreviewCache::open(root, budget_bytes, max_entries)?);
    caches.insert(key, Arc::clone(&cache));
    Ok(cache)
}
