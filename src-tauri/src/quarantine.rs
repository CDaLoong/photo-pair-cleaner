use crate::fs_util::{self, modified_ms};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

pub(crate) const QUARANTINE_DIR: &str = ".framepair-quarantine";
const MANIFEST_FILE: &str = "manifest.jsonl";
const RESTORED_FILE: &str = "restored.jsonl";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuarantineRecord {
    pub operation_id: String,
    pub relative_path: String,
    pub quarantined_path: PathBuf,
    pub size_bytes: u64,
    pub modified_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QuarantineOperation {
    pub operation_id: String,
    pub created_at_ms: u64,
    pub moved: usize,
    pub recoverable: usize,
    pub restored: usize,
    pub manifest_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RestoreResult {
    pub relative_path: String,
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RestoreMarker {
    relative_path: String,
}

fn canonical_root(raw_root: &Path) -> Result<PathBuf, String> {
    fs_util::canonical_directory(raw_root, "RAW 源目录")
}

fn safe_relative_path(value: &Path) -> Result<PathBuf, String> {
    fs_util::safe_relative_path(value, "隔离文件路径")?;
    // 否则一次恢复就能把隔离区自身搬进隔离区，形成无法收敛的嵌套。
    if value
        .components()
        .next()
        .is_some_and(|component| component.as_os_str() == QUARANTINE_DIR)
    {
        return Err("不能把隔离目录自身作为处理目标".to_string());
    }
    Ok(value.to_path_buf())
}

pub(crate) fn operation_root(raw_root: &Path, operation_id: &str) -> Result<PathBuf, String> {
    if operation_id.is_empty()
        || !operation_id
            .chars()
            .all(|value| value.is_ascii_alphanumeric() || value == '-')
    {
        return Err("隔离操作编号不合法".to_string());
    }
    let root = canonical_root(raw_root)?;
    let quarantine_root = root.join(QUARANTINE_DIR);
    if quarantine_root.exists() {
        let metadata = fs::symlink_metadata(&quarantine_root)
            .map_err(|error| format!("无法检查隔离目录：{error}"))?;
        if metadata.file_type().is_symlink() {
            return Err("隔离目录不能是符号链接".to_string());
        }
        if !metadata.is_dir() {
            return Err("隔离目录路径被其他文件占用".to_string());
        }
    }
    let operation_root = quarantine_root.join(operation_id);
    match fs::symlink_metadata(&operation_root) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err("隔离操作目录不能是符号链接".to_string());
        }
        Ok(metadata) if !metadata.is_dir() => {
            return Err("隔离操作目录路径被其他文件占用".to_string());
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("无法检查隔离操作目录：{error}")),
    }
    Ok(operation_root)
}

fn append_json_line<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let line =
        serde_json::to_string(value).map_err(|error| format!("无法序列化隔离记录：{error}"))?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("无法打开隔离记录：{error}"))?;
    writeln!(file, "{line}").map_err(|error| format!("无法写入隔离记录：{error}"))?;
    file.sync_data()
        .map_err(|error| format!("无法同步隔离记录：{error}"))
}

fn read_json_lines<T>(path: &Path) -> Result<Vec<T>, String>
where
    T: for<'de> Deserialize<'de>,
{
    let file = fs::File::open(path).map_err(|error| format!("无法打开隔离记录：{error}"))?;
    BufReader::new(file)
        .lines()
        .enumerate()
        .map(|(index, line)| {
            let line = line.map_err(|error| format!("无法读取隔离记录：{error}"))?;
            serde_json::from_str(&line)
                .map_err(|error| format!("隔离记录第 {} 行无效：{error}", index + 1))
        })
        .collect()
}

fn read_manifest(operation_root: &Path) -> Result<Vec<QuarantineRecord>, String> {
    read_json_lines(&operation_root.join(MANIFEST_FILE))
}

fn restored_paths(operation_root: &Path) -> Result<HashSet<String>, String> {
    let path = operation_root.join(RESTORED_FILE);
    if !path.exists() {
        return Ok(HashSet::new());
    }
    Ok(read_json_lines::<RestoreMarker>(&path)?
        .into_iter()
        .map(|marker| marker.relative_path)
        .collect())
}

pub(crate) fn move_file(
    raw_root: &Path,
    operation_id: &str,
    relative_path: &Path,
) -> Result<QuarantineRecord, String> {
    let root = canonical_root(raw_root)?;
    let relative = safe_relative_path(relative_path)?;
    let source = fs::canonicalize(root.join(&relative))
        .map_err(|error| format!("待隔离文件不可访问：{error}"))?;
    if !source.starts_with(&root) {
        return Err("待隔离文件超出了 RAW 源目录".to_string());
    }
    let metadata = fs::metadata(&source).map_err(|error| format!("无法读取文件信息：{error}"))?;
    if !metadata.is_file() {
        return Err("待隔离目标不是普通文件".to_string());
    }

    let operation_root = operation_root(&root, operation_id)?;
    let target = operation_root.join(&relative);
    if target.exists() {
        return Err("隔离目标已存在，不会覆盖".to_string());
    }
    let target_parent = target
        .parent()
        .ok_or_else(|| "无法确定隔离目标目录".to_string())?;
    fs::create_dir_all(target_parent).map_err(|error| format!("无法创建隔离目录：{error}"))?;
    let canonical_parent =
        fs::canonicalize(target_parent).map_err(|error| format!("无法校验隔离目录：{error}"))?;
    let canonical_operation_root = fs::canonicalize(&operation_root)
        .map_err(|error| format!("无法校验隔离操作目录：{error}"))?;
    if !canonical_parent.starts_with(&canonical_operation_root) {
        return Err("隔离目标解析后超出了操作目录".to_string());
    }

    fs::rename(&source, &target).map_err(|error| format!("移入隔离区失败：{error}"))?;
    let record = QuarantineRecord {
        operation_id: operation_id.to_string(),
        relative_path: relative.to_string_lossy().replace('\\', "/"),
        quarantined_path: target,
        size_bytes: metadata.len(),
        modified_ms: modified_ms(&metadata),
    };
    if let Err(error) = append_json_line(&operation_root.join(MANIFEST_FILE), &record) {
        return match fs::rename(&record.quarantined_path, &source) {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(format!("{error}；文件已移动但回滚失败：{rollback_error}")),
        };
    }
    Ok(record)
}

pub(crate) fn restore_file(raw_root: &Path, record: &QuarantineRecord) -> Result<(), String> {
    let root = canonical_root(raw_root)?;
    let relative = safe_relative_path(Path::new(&record.relative_path))?;
    let expected_operation_root = operation_root(&root, &record.operation_id)?;
    let source = fs::canonicalize(&record.quarantined_path)
        .map_err(|error| format!("隔离文件不可访问：{error}"))?;
    let expected_source = expected_operation_root.join(&relative);
    if source != fs::canonicalize(&expected_source).map_err(|error| error.to_string())?
        || !source.starts_with(&expected_operation_root)
    {
        return Err("隔离文件路径与恢复记录不一致".to_string());
    }
    let metadata = fs::metadata(&source).map_err(|error| format!("无法读取隔离文件：{error}"))?;
    if metadata.len() != record.size_bytes || modified_ms(&metadata) != record.modified_ms {
        return Err("隔离文件在移动后发生变化，不会自动恢复".to_string());
    }

    let destination = root.join(&relative);
    if destination.exists() {
        return Err("原位置已有同名文件，不会覆盖".to_string());
    }
    let destination_parent = destination
        .parent()
        .ok_or_else(|| "无法确定恢复目标目录".to_string())?;
    fs::create_dir_all(destination_parent)
        .map_err(|error| format!("无法创建恢复目标目录：{error}"))?;
    let canonical_parent = fs::canonicalize(destination_parent)
        .map_err(|error| format!("无法校验恢复目标目录：{error}"))?;
    if !canonical_parent.starts_with(&root) {
        return Err("恢复目标解析后超出了 RAW 源目录".to_string());
    }

    fs::rename(&source, &destination).map_err(|error| format!("恢复隔离文件失败：{error}"))?;
    let marker = RestoreMarker {
        relative_path: record.relative_path.clone(),
    };
    if let Err(error) = append_json_line(&expected_operation_root.join(RESTORED_FILE), &marker) {
        return match fs::rename(&destination, &source) {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(format!(
                "{error}；文件已恢复但记录失败，且回滚失败：{rollback_error}"
            )),
        };
    }
    Ok(())
}

pub(crate) fn list_operations(raw_root: &Path) -> Result<Vec<QuarantineOperation>, String> {
    let root = canonical_root(raw_root)?;
    let quarantine_root = root.join(QUARANTINE_DIR);
    if !quarantine_root.exists() {
        return Ok(Vec::new());
    }
    let metadata = fs::symlink_metadata(&quarantine_root)
        .map_err(|error| format!("无法检查隔离目录：{error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("隔离目录不是可信文件夹".to_string());
    }

    let mut operations = Vec::new();
    for entry in
        fs::read_dir(&quarantine_root).map_err(|error| format!("无法读取隔离目录：{error}"))?
    {
        let entry = entry.map_err(|error| format!("无法读取隔离操作：{error}"))?;
        let operation_id = entry.file_name().to_string_lossy().into_owned();
        let operation_root = operation_root(&root, &operation_id)?;
        if !operation_root.join(MANIFEST_FILE).is_file() {
            continue;
        }
        let records = read_manifest(&operation_root)?;
        if records
            .iter()
            .any(|record| record.operation_id != operation_id)
        {
            return Err(format!("隔离操作 {operation_id} 的记录编号不一致"));
        }
        let restored = restored_paths(&operation_root)?;
        let recoverable = records
            .iter()
            .filter(|record| operation_root.join(&record.relative_path).is_file())
            .count();
        operations.push(QuarantineOperation {
            created_at_ms: operation_id
                .split('-')
                .next()
                .and_then(|value| value.parse().ok())
                .unwrap_or_default(),
            operation_id,
            moved: records.len(),
            recoverable,
            restored: restored.len(),
            manifest_path: operation_root
                .join(MANIFEST_FILE)
                .to_string_lossy()
                .into_owned(),
        });
    }
    operations.sort_by(|left, right| right.created_at_ms.cmp(&left.created_at_ms));
    Ok(operations)
}

pub(crate) fn restore_operation(
    raw_root: &Path,
    operation_id: &str,
) -> Result<Vec<RestoreResult>, String> {
    let root = canonical_root(raw_root)?;
    let operation_root = operation_root(&root, operation_id)?;
    let records = read_manifest(&operation_root)?;
    let mut results = Vec::new();
    for record in records {
        let expected_source = operation_root.join(&record.relative_path);
        if !expected_source.exists() {
            continue;
        }
        match restore_file(&root, &record) {
            Ok(()) => results.push(RestoreResult {
                relative_path: record.relative_path,
                success: true,
                message: "已恢复到原位置".to_string(),
            }),
            Err(message) => results.push(RestoreResult {
                relative_path: record.relative_path,
                success: false,
                message,
            }),
        }
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn quarantine_preserves_relative_paths_and_restores_without_overwrite() {
        let temp = tempfile::tempdir().expect("temp root");
        let raw_root = temp.path().join("raw");
        let source = raw_root.join("day/a.NEF");
        std::fs::create_dir_all(source.parent().expect("source parent")).expect("source dir");
        std::fs::write(&source, b"raw").expect("source file");

        let record =
            move_file(&raw_root, "operation-1", Path::new("day/a.NEF")).expect("quarantine move");
        assert!(!source.exists());
        assert!(record.quarantined_path.ends_with("operation-1/day/a.NEF"));

        restore_file(&raw_root, &record).expect("restore");
        assert!(source.exists());
        assert!(!record.quarantined_path.exists());
    }

    #[test]
    fn quarantine_rejects_symlink_root() {
        let temp = tempfile::tempdir().expect("temp root");
        let raw_root = temp.path().join("raw");
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&raw_root).expect("raw root");
        std::fs::create_dir_all(&outside).expect("outside root");

        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, raw_root.join(".framepair-quarantine"))
            .expect("quarantine symlink");
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&outside, raw_root.join(".framepair-quarantine"))
            .expect("quarantine symlink");

        assert!(operation_root(&raw_root, "operation-1").is_err());
    }

    #[test]
    fn restore_rejects_an_existing_destination() {
        let temp = tempfile::tempdir().expect("temp root");
        let raw_root = temp.path().join("raw");
        let source = raw_root.join("a.NEF");
        std::fs::create_dir_all(&raw_root).expect("raw root");
        std::fs::write(&source, b"first").expect("source file");
        let record =
            move_file(&raw_root, "operation-1", Path::new("a.NEF")).expect("quarantine move");
        std::fs::write(&source, b"replacement").expect("replacement file");

        assert!(restore_file(&raw_root, &record).is_err());
        assert_eq!(std::fs::read(&source).expect("destination"), b"replacement");
        assert!(record.quarantined_path.exists());
    }

    #[test]
    fn quarantine_rejects_parent_traversal() {
        let temp = tempfile::tempdir().expect("temp root");
        let raw_root = temp.path().join("raw");
        std::fs::create_dir_all(&raw_root).expect("raw root");

        assert!(move_file(&raw_root, "operation-1", Path::new("../outside.NEF")).is_err());
    }

    #[test]
    fn quarantine_rejects_a_symlinked_operation_directory() {
        let temp = tempfile::tempdir().expect("temp root");
        let raw_root = temp.path().join("raw");
        let source = raw_root.join("a.NEF");
        let quarantine_root = raw_root.join(QUARANTINE_DIR);
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&quarantine_root).expect("quarantine root");
        std::fs::create_dir_all(&outside).expect("outside root");
        std::fs::write(&source, b"raw").expect("source file");

        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, quarantine_root.join("operation-1"))
            .expect("operation symlink");
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&outside, quarantine_root.join("operation-1"))
            .expect("operation symlink");

        assert!(move_file(&raw_root, "operation-1", Path::new("a.NEF")).is_err());
        assert!(source.exists());
        assert!(!outside.join("a.NEF").exists());
    }

    #[test]
    fn manifest_history_survives_restore() {
        let temp = tempfile::tempdir().expect("temp root");
        let raw_root = temp.path().join("raw");
        std::fs::create_dir_all(&raw_root).expect("raw root");
        std::fs::write(raw_root.join("a.NEF"), b"raw").expect("source file");
        move_file(&raw_root, "1000-1-1", Path::new("a.NEF")).expect("quarantine move");

        let operations = list_operations(&raw_root).expect("operation history");
        assert_eq!(operations.len(), 1);
        assert_eq!(operations[0].operation_id, "1000-1-1");
        assert_eq!(operations[0].moved, 1);
        assert_eq!(operations[0].recoverable, 1);
        assert_eq!(operations[0].restored, 0);

        let results = restore_operation(&raw_root, "1000-1-1").expect("restore operation");
        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        assert!(raw_root.join("a.NEF").exists());

        let operations = list_operations(&raw_root).expect("updated history");
        assert_eq!(operations[0].recoverable, 0);
        assert_eq!(operations[0].restored, 1);
    }
}
