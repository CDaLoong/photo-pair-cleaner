use crate::operation_plan::OperationSyncPreference;
use crate::rating_rules::{RatingRule, RuleMemberKind};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

pub(crate) const HISTORY_DIR: &str = "rating-operations";
const MANIFEST_FILE: &str = "manifest.json";
const RECOVERY_FILE: &str = "recoveries.jsonl";
const HISTORY_VERSION: u8 = 1;
const MAX_HISTORY_FILE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum OrganizerAction {
    Copy,
    Move,
    Quarantine,
    Trash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum OrganizerGroupStatus {
    Success,
    Failed,
    Partial,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileFingerprint {
    pub(crate) size_bytes: u64,
    pub(crate) modified_ms: Option<u64>,
    pub(crate) sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OperationMemberRecord {
    pub(crate) kind: RuleMemberKind,
    pub(crate) source_path: String,
    pub(crate) target_path: String,
    pub(crate) expected_size_bytes: u64,
    pub(crate) expected_modified_ms: Option<u64>,
    pub(crate) target_snapshot: Option<FileFingerprint>,
    pub(crate) message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OperationGroupRecord {
    pub(crate) group_id: String,
    pub(crate) relative_stem: String,
    pub(crate) action: OrganizerAction,
    pub(crate) status: OrganizerGroupStatus,
    pub(crate) message: String,
    pub(crate) members: Vec<OperationMemberRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OperationManifest {
    version: u8,
    pub(crate) operation_id: String,
    pub(crate) plan_id: String,
    pub(crate) root: String,
    pub(crate) created_at_ms: u64,
    pub(crate) rules: Vec<RatingRule>,
    pub(crate) sync: OperationSyncPreference,
    pub(crate) groups: Vec<OperationGroupRecord>,
}

impl OperationManifest {
    pub(crate) fn new(
        operation_id: String,
        plan_id: String,
        root: String,
        created_at_ms: u64,
        rules: Vec<RatingRule>,
        sync: OperationSyncPreference,
        groups: Vec<OperationGroupRecord>,
    ) -> Self {
        Self {
            version: HISTORY_VERSION,
            operation_id,
            plan_id,
            root,
            created_at_ms,
            rules,
            sync,
            groups,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum RecoveryKind {
    RestoreMove,
    UndoCopy,
    RestoreQuarantine,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecoveryMemberResult {
    pub(crate) source_path: String,
    pub(crate) target_path: String,
    pub(crate) success: bool,
    pub(crate) message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecoveryRecord {
    pub(crate) operation_id: String,
    pub(crate) group_id: String,
    pub(crate) kind: RecoveryKind,
    pub(crate) created_at_ms: u64,
    pub(crate) status: OrganizerGroupStatus,
    pub(crate) message: String,
    pub(crate) members: Vec<RecoveryMemberResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OperationHistoryEntry {
    pub(crate) manifest: OperationManifest,
    pub(crate) recoveries: Vec<RecoveryRecord>,
    pub(crate) recoverable_groups: usize,
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
}

fn ensure_directory(path: &Path, label: &str) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            Err(format!("{label}不是可信文件夹"))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(path).map_err(|error| format!("无法创建{label}：{error}"))
        }
        Err(error) => Err(format!("无法检查{label}：{error}")),
    }
}

fn history_root(app_data_dir: &Path) -> Result<PathBuf, String> {
    ensure_directory(app_data_dir, "应用数据目录")?;
    let root = app_data_dir.join(HISTORY_DIR);
    ensure_directory(&root, "评分整理历史目录")?;
    Ok(root)
}

fn operation_dir(app_data_dir: &Path, operation_id: &str) -> Result<PathBuf, String> {
    if !valid_id(operation_id) {
        return Err("评分整理操作编号不合法".to_string());
    }
    let path = history_root(app_data_dir)?.join(operation_id);
    let metadata = fs::symlink_metadata(&path)
        .map_err(|error| format!("评分整理历史不存在或不可访问：{error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("评分整理操作目录不是可信文件夹".to_string());
    }
    Ok(path)
}

fn read_bounded_file(path: &Path, label: &str) -> Result<Vec<u8>, String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| format!("无法检查{label}：{error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!("{label}不是可信普通文件"));
    }
    if metadata.len() > MAX_HISTORY_FILE_BYTES {
        return Err(format!("{label}超过 16 MiB 上限"));
    }
    fs::read(path).map_err(|error| format!("无法读取{label}：{error}"))
}

fn validate_manifest(manifest: &OperationManifest, directory_name: &str) -> Result<(), String> {
    if manifest.version != HISTORY_VERSION {
        return Err(format!("不支持评分整理历史版本 {}", manifest.version));
    }
    if manifest.operation_id != directory_name || !valid_id(&manifest.operation_id) {
        return Err("评分整理历史编号与目录不一致".to_string());
    }
    if manifest.plan_id.trim().is_empty() || manifest.root.trim().is_empty() {
        return Err("评分整理历史缺少计划或根目录".to_string());
    }
    let mut groups = HashSet::with_capacity(manifest.groups.len());
    if manifest
        .groups
        .iter()
        .any(|group| group.group_id.trim().is_empty() || !groups.insert(group.group_id.as_str()))
    {
        return Err("评分整理历史包含空白或重复照片组".to_string());
    }
    Ok(())
}

fn read_manifest(directory: &Path) -> Result<OperationManifest, String> {
    let name = directory
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "评分整理操作目录名称无效".to_string())?;
    let bytes = read_bounded_file(&directory.join(MANIFEST_FILE), "评分整理历史清单")?;
    let manifest: OperationManifest = serde_json::from_slice(&bytes)
        .map_err(|error| format!("评分整理历史清单已损坏：{error}"))?;
    validate_manifest(&manifest, name)?;
    Ok(manifest)
}

fn read_recoveries(directory: &Path) -> Result<Vec<RecoveryRecord>, String> {
    let path = directory.join(RECOVERY_FILE);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("无法检查评分整理恢复记录：{error}")),
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err("评分整理恢复记录不是可信普通文件".to_string());
            }
            if metadata.len() > MAX_HISTORY_FILE_BYTES {
                return Err("评分整理恢复记录超过 16 MiB 上限".to_string());
            }
        }
    }
    BufReader::new(File::open(&path).map_err(|error| format!("无法打开评分整理恢复记录：{error}"))?)
        .lines()
        .enumerate()
        .map(|(index, line)| {
            let line = line.map_err(|error| format!("无法读取评分整理恢复记录：{error}"))?;
            serde_json::from_str(&line)
                .map_err(|error| format!("评分整理恢复记录第 {} 行无效：{error}", index + 1))
        })
        .collect()
}

fn expected_recovery_kind(action: OrganizerAction) -> Option<RecoveryKind> {
    match action {
        OrganizerAction::Copy => Some(RecoveryKind::UndoCopy),
        OrganizerAction::Move => Some(RecoveryKind::RestoreMove),
        OrganizerAction::Quarantine => Some(RecoveryKind::RestoreQuarantine),
        OrganizerAction::Trash => None,
    }
}

fn validate_recoveries(
    manifest: &OperationManifest,
    recoveries: &[RecoveryRecord],
) -> Result<(), String> {
    for recovery in recoveries {
        if recovery.operation_id != manifest.operation_id {
            return Err("恢复记录与评分整理操作不一致".to_string());
        }
        let group = manifest
            .groups
            .iter()
            .find(|group| group.group_id == recovery.group_id)
            .ok_or_else(|| "恢复记录指向未知照片组".to_string())?;
        let expected = expected_recovery_kind(group.action)
            .ok_or_else(|| "系统回收站操作不支持应用内恢复".to_string())?;
        if recovery.kind != expected {
            return Err("恢复记录类型与原操作不一致".to_string());
        }
    }
    Ok(())
}

pub(crate) fn persist_manifest(
    app_data_dir: &Path,
    manifest: &OperationManifest,
) -> Result<(), String> {
    validate_manifest(manifest, &manifest.operation_id)?;
    let root = history_root(app_data_dir)?;
    let directory = root.join(&manifest.operation_id);
    fs::create_dir(&directory).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            "评分整理操作历史已存在，不会覆盖".to_string()
        } else {
            format!("无法创建评分整理操作历史：{error}")
        }
    })?;
    let result = (|| {
        let bytes = serde_json::to_vec_pretty(manifest)
            .map_err(|error| format!("无法序列化评分整理历史：{error}"))?;
        if bytes.len() as u64 > MAX_HISTORY_FILE_BYTES {
            return Err("评分整理历史清单超过 16 MiB 上限".to_string());
        }
        let mut temporary = tempfile::NamedTempFile::new_in(&directory)
            .map_err(|error| format!("无法创建评分整理历史临时文件：{error}"))?;
        temporary
            .write_all(&bytes)
            .and_then(|_| temporary.as_file_mut().sync_all())
            .map_err(|error| format!("无法写入评分整理历史：{error}"))?;
        temporary
            .persist_noclobber(directory.join(MANIFEST_FILE))
            .map_err(|error| format!("无法保存评分整理历史：{}", error.error))?;
        File::open(&directory)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| format!("无法同步评分整理历史目录：{error}"))
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&directory);
    }
    result
}

pub(crate) fn append_recovery(
    app_data_dir: &Path,
    recovery: &RecoveryRecord,
) -> Result<(), String> {
    let directory = operation_dir(app_data_dir, &recovery.operation_id)?;
    let manifest = read_manifest(&directory)?;
    let existing = read_recoveries(&directory)?;
    validate_recoveries(&manifest, &existing)?;
    validate_recoveries(&manifest, std::slice::from_ref(recovery))?;
    if existing.iter().any(|record| {
        record.group_id == recovery.group_id && record.status == OrganizerGroupStatus::Success
    }) {
        return Err("该照片组已经完成恢复或撤销".to_string());
    }
    let line = serde_json::to_string(recovery)
        .map_err(|error| format!("无法序列化评分整理恢复记录：{error}"))?;
    let path = directory.join(RECOVERY_FILE);
    let mut options = OpenOptions::new();
    options.append(true).write(true);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err("评分整理恢复记录不是可信普通文件".to_string());
        }
        Ok(metadata) if metadata.len() > MAX_HISTORY_FILE_BYTES => {
            return Err("评分整理恢复记录超过 16 MiB 上限".to_string());
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            options.create_new(true);
        }
        Err(error) => return Err(format!("无法检查评分整理恢复记录：{error}")),
    }
    let mut file = options
        .open(&path)
        .map_err(|error| format!("无法打开评分整理恢复记录：{error}"))?;
    writeln!(file, "{line}").map_err(|error| format!("无法写入评分整理恢复记录：{error}"))?;
    file.sync_data()
        .map_err(|error| format!("无法同步评分整理恢复记录：{error}"))
}

pub(crate) fn load_operation(
    app_data_dir: &Path,
    operation_id: &str,
) -> Result<OperationHistoryEntry, String> {
    let directory = operation_dir(app_data_dir, operation_id)?;
    let manifest = read_manifest(&directory)?;
    let recoveries = read_recoveries(&directory)?;
    validate_recoveries(&manifest, &recoveries)?;
    let recovered = recoveries
        .iter()
        .filter(|record| record.status == OrganizerGroupStatus::Success)
        .map(|record| record.group_id.as_str())
        .collect::<HashSet<_>>();
    let recoverable_groups = manifest
        .groups
        .iter()
        .filter(|group| {
            matches!(
                group.status,
                OrganizerGroupStatus::Success | OrganizerGroupStatus::Partial
            ) && expected_recovery_kind(group.action).is_some()
                && !recovered.contains(group.group_id.as_str())
        })
        .count();
    Ok(OperationHistoryEntry {
        manifest,
        recoveries,
        recoverable_groups,
    })
}

pub(crate) fn list_operations(app_data_dir: &Path) -> Result<Vec<OperationHistoryEntry>, String> {
    let root = history_root(app_data_dir)?;
    let mut history = Vec::new();
    for entry in fs::read_dir(root).map_err(|error| format!("无法读取评分整理历史：{error}"))?
    {
        let entry = entry.map_err(|error| format!("无法读取评分整理历史条目：{error}"))?;
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| format!("无法检查评分整理历史条目：{error}"))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("评分整理历史包含不可信条目".to_string());
        }
        let operation_id = entry
            .file_name()
            .into_string()
            .map_err(|_| "评分整理历史目录名称无效".to_string())?;
        history.push(load_operation(app_data_dir, &operation_id)?);
    }
    history.sort_by(|left, right| {
        right
            .manifest
            .created_at_ms
            .cmp(&left.manifest.created_at_ms)
            .then_with(|| right.manifest.operation_id.cmp(&left.manifest.operation_id))
    });
    Ok(history)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operation_plan::OperationSyncPreference;
    use crate::rating_rules::{RatingCondition, RatingRule, RuleAction, RuleMemberKind};
    use tempfile::tempdir;

    fn rule() -> RatingRule {
        RatingRule {
            id: "rule-1".to_string(),
            name: "三星移动".to_string(),
            enabled: true,
            condition: RatingCondition::Equal { rating: 3 },
            member_scope: vec![RuleMemberKind::Jpeg, RuleMemberKind::Raw],
            action: RuleAction::Move,
            destination: Some("/target".to_string()),
            preserve_relative_path: true,
        }
    }

    fn group(group_id: &str, action: OrganizerAction) -> OperationGroupRecord {
        OperationGroupRecord {
            group_id: group_id.to_string(),
            relative_stem: format!("album/{group_id}"),
            action,
            status: OrganizerGroupStatus::Success,
            message: "处理完成".to_string(),
            members: vec![OperationMemberRecord {
                kind: RuleMemberKind::Jpeg,
                source_path: format!("/source/{group_id}.jpg"),
                target_path: format!("/target/{group_id}.jpg"),
                expected_size_bytes: 4,
                expected_modified_ms: Some(100),
                target_snapshot: Some(FileFingerprint {
                    size_bytes: 4,
                    modified_ms: Some(200),
                    sha256: "abcd".to_string(),
                }),
                message: "已移动".to_string(),
            }],
        }
    }

    fn manifest(operation_id: &str, created_at_ms: u64) -> OperationManifest {
        OperationManifest::new(
            operation_id.to_string(),
            "plan-1".to_string(),
            "/source".to_string(),
            created_at_ms,
            vec![rule()],
            OperationSyncPreference::default(),
            vec![group("move-group", OrganizerAction::Move)],
        )
    }

    #[test]
    fn operation_history_persists_immutable_manifest_and_lists_newest_first() {
        let data = tempdir().expect("app data");
        persist_manifest(data.path(), &manifest("operation-1", 100)).expect("persist first");
        persist_manifest(data.path(), &manifest("operation-2", 200)).expect("persist second");

        assert!(persist_manifest(data.path(), &manifest("operation-1", 300)).is_err());
        let history = list_operations(data.path()).expect("list operations");
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].manifest.operation_id, "operation-2");
        assert_eq!(history[1].manifest.rules, vec![rule()]);
        assert_eq!(history[1].recoverable_groups, 1);
        assert!(history[1].recoveries.is_empty());
    }

    #[test]
    fn operation_history_appends_recovery_without_rewriting_manifest() {
        let data = tempdir().expect("app data");
        let original = manifest("operation-1", 100);
        persist_manifest(data.path(), &original).expect("persist manifest");
        append_recovery(
            data.path(),
            &RecoveryRecord {
                operation_id: "operation-1".to_string(),
                group_id: "move-group".to_string(),
                kind: RecoveryKind::RestoreMove,
                created_at_ms: 200,
                status: OrganizerGroupStatus::Success,
                message: "已恢复".to_string(),
                members: vec![RecoveryMemberResult {
                    source_path: "/source/move-group.jpg".to_string(),
                    target_path: "/target/move-group.jpg".to_string(),
                    success: true,
                    message: "已恢复到原位置".to_string(),
                }],
            },
        )
        .expect("append recovery");

        let history = list_operations(data.path()).expect("list operations");
        assert_eq!(history[0].manifest, original);
        assert_eq!(history[0].recoveries.len(), 1);
        assert_eq!(history[0].recoverable_groups, 0);
        assert!(
            append_recovery(
                data.path(),
                &RecoveryRecord {
                    operation_id: "operation-1".to_string(),
                    group_id: "missing".to_string(),
                    kind: RecoveryKind::RestoreMove,
                    created_at_ms: 300,
                    status: OrganizerGroupStatus::Failed,
                    message: "失败".to_string(),
                    members: Vec::new(),
                },
            )
            .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn operation_history_rejects_symlinked_operation_directories() {
        use std::os::unix::fs::symlink;

        let data = tempdir().expect("app data");
        let outside = tempdir().expect("outside");
        let history_root = data.path().join(HISTORY_DIR);
        std::fs::create_dir_all(&history_root).expect("history root");
        symlink(outside.path(), history_root.join("operation-1")).expect("symlink");

        assert!(list_operations(data.path()).is_err());
    }

    #[test]
    fn cleanup_history_recovers_quarantine_but_never_system_trash() {
        let data = tempdir().expect("app data");
        let cleanup = OperationManifest::new(
            "cleanup-1".to_string(),
            "plan-1".to_string(),
            "/source".to_string(),
            100,
            vec![rule()],
            OperationSyncPreference::default(),
            vec![
                group("quarantine-group", OrganizerAction::Quarantine),
                group("trash-group", OrganizerAction::Trash),
            ],
        );
        persist_manifest(data.path(), &cleanup).expect("persist cleanup history");

        let history = list_operations(data.path()).expect("list cleanup history");
        assert_eq!(history[0].recoverable_groups, 1);
        append_recovery(
            data.path(),
            &RecoveryRecord {
                operation_id: "cleanup-1".to_string(),
                group_id: "quarantine-group".to_string(),
                kind: RecoveryKind::RestoreQuarantine,
                created_at_ms: 200,
                status: OrganizerGroupStatus::Success,
                message: "已恢复隔离".to_string(),
                members: Vec::new(),
            },
        )
        .expect("append quarantine recovery");
        assert_eq!(
            list_operations(data.path()).expect("updated cleanup history")[0]
                .recoverable_groups,
            0
        );

        assert!(
            append_recovery(
                data.path(),
                &RecoveryRecord {
                    operation_id: "cleanup-1".to_string(),
                    group_id: "trash-group".to_string(),
                    kind: RecoveryKind::RestoreQuarantine,
                    created_at_ms: 300,
                    status: OrganizerGroupStatus::Failed,
                    message: "系统回收站不可在应用内恢复".to_string(),
                    members: Vec::new(),
                },
            )
            .is_err()
        );
    }

    #[test]
    fn existing_copy_and_move_action_names_remain_stable() {
        assert_eq!(
            serde_json::to_string(&OrganizerAction::Copy).expect("serialize copy"),
            "\"copy\""
        );
        assert_eq!(
            serde_json::to_string(&OrganizerAction::Move).expect("serialize move"),
            "\"move\""
        );
    }
}
