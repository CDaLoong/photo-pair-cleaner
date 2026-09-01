use crate::fs_util;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const DATABASE_VERSION: u8 = 1;
const MAX_DATABASE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RatingsDatabase {
    version: u8,
    #[serde(default)]
    roots: BTreeMap<String, BTreeMap<String, u8>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RatingUpdate {
    pub(crate) asset_id: String,
    pub(crate) rating: u8,
}

fn canonical_root(root: &Path) -> Result<PathBuf, String> {
    fs_util::canonical_directory(root, "照片目录")
}

fn safe_relative_path(value: &str) -> Result<PathBuf, String> {
    let path = fs_util::safe_relative_path_str(value, "评分路径")?;
    // 评分只对成对清理认可的照片本体有意义，XMP 边车和其它文件不参与。
    if !crate::formats::is_reference(&path) && !crate::formats::is_raw(&path) {
        return Err("只能为受支持的 JPG/RAW 照片评分".to_string());
    }
    Ok(path)
}

fn root_key(root: &Path) -> String {
    root.to_string_lossy().replace('\\', "/")
}

fn asset_id(relative_path: &Path) -> String {
    relative_path
        .with_extension("")
        .to_string_lossy()
        .replace('\\', "/")
        .to_lowercase()
}

fn read_database(database_path: &Path) -> Result<RatingsDatabase, String> {
    if !database_path.exists() {
        return Ok(RatingsDatabase {
            version: DATABASE_VERSION,
            ..RatingsDatabase::default()
        });
    }
    let metadata = fs::symlink_metadata(database_path)
        .map_err(|error| format!("无法读取评分数据库信息：{error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("评分数据库不是可信普通文件".to_string());
    }
    if metadata.len() > MAX_DATABASE_BYTES {
        return Err("评分数据库超过 16 MiB 上限".to_string());
    }
    let input = fs::read(database_path).map_err(|error| format!("无法读取评分数据库：{error}"))?;
    let database: RatingsDatabase =
        serde_json::from_slice(&input).map_err(|error| format!("评分数据库已损坏：{error}"))?;
    if database.version != DATABASE_VERSION {
        return Err(format!("不支持评分数据库版本 {}", database.version));
    }
    Ok(database)
}

fn temporary_path(database_path: &Path) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    database_path.with_file_name(format!(
        ".photo-ratings-{}-{sequence}.tmp",
        std::process::id(),
    ))
}

fn write_database(database_path: &Path, database: &RatingsDatabase) -> Result<(), String> {
    let parent = database_path
        .parent()
        .ok_or_else(|| "无法确定评分数据库目录".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("无法创建评分数据库目录：{error}"))?;
    let bytes = serde_json::to_vec(database).map_err(|error| format!("无法序列化评分：{error}"))?;
    let temporary_path = temporary_path(database_path);
    let mut temporary = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary_path)
        .map_err(|error| format!("无法创建临时评分数据库：{error}"))?;
    if let Err(error) = temporary
        .write_all(&bytes)
        .and_then(|_| temporary.sync_all())
    {
        let _ = fs::remove_file(&temporary_path);
        return Err(format!("无法写入评分数据库：{error}"));
    }
    drop(temporary);

    #[cfg(target_os = "windows")]
    if database_path.exists() {
        fs::remove_file(database_path).map_err(|error| format!("无法更新评分数据库：{error}"))?;
    }
    if let Err(error) = fs::rename(&temporary_path, database_path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(format!("无法完成评分数据库写入：{error}"));
    }
    Ok(())
}

pub(crate) fn load_ratings(
    database_path: &Path,
    root: &Path,
) -> Result<HashMap<String, u8>, String> {
    let root = canonical_root(root)?;
    let database = read_database(database_path)?;
    Ok(database
        .roots
        .get(&root_key(&root))
        .map(|ratings| {
            ratings
                .iter()
                .map(|(key, rating)| (key.clone(), *rating))
                .collect()
        })
        .unwrap_or_default())
}

pub(crate) fn set_rating(
    database_path: &Path,
    root: &Path,
    relative_path: &str,
    rating: u8,
) -> Result<RatingUpdate, String> {
    if rating > 5 {
        return Err("照片评分必须在 0 到 5 星之间".to_string());
    }
    let root = canonical_root(root)?;
    let relative = safe_relative_path(relative_path)?;
    let path = fs::canonicalize(root.join(&relative))
        .map_err(|error| format!("评分照片不存在或不可访问：{error}"))?;
    if !path.starts_with(&root) || !path.is_file() {
        return Err("评分照片超出了所选目录".to_string());
    }

    let asset_id = asset_id(&relative);
    let root_key = root_key(&root);
    let mut database = read_database(database_path)?;
    let ratings = database.roots.entry(root_key.clone()).or_default();
    if rating == 0 {
        ratings.remove(&asset_id);
        if ratings.is_empty() {
            database.roots.remove(&root_key);
        }
    } else {
        ratings.insert(asset_id.clone(), rating);
    }
    write_database(database_path, &database)?;
    Ok(RatingUpdate { asset_id, rating })
}
