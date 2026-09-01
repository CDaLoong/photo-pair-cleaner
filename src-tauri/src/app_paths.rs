//! 应用层的路径解析与校验。
//!
//! 这里的每个函数都在「用户选中的根目录」和「前端传来的相对路径」之间设卡：
//! 先过 fs_util 的通用安全校验，再叠加本应用的额外规则（受支持的扩展名、
//! 解析后必须仍在根目录内）。绕过这一层直接拼接路径就是越权漏洞。

use crate::fs_util::canonical_directory_from_input;
use crate::{formats, fs_util};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub(crate) fn safe_relative_path(value: &str) -> Result<PathBuf, String> {
    fs_util::safe_relative_path_str(value, "文件路径")
}

pub(crate) fn resolve_scan_item_path(
    raw_root: &str,
    relative_path: &str,
) -> Result<PathBuf, String> {
    let root = canonical_directory_from_input(raw_root, "RAW 源目录")?;
    let relative = safe_relative_path(relative_path)?;
    let path = fs::canonicalize(root.join(relative))
        .map_err(|error| format!("文件已不存在或不可访问：{error}"))?;
    if !path.starts_with(&root) {
        return Err("文件解析后超出了 RAW 源目录".to_string());
    }
    Ok(path)
}

pub(crate) fn resolve_photo_asset_path(root: &str, relative_path: &str) -> Result<PathBuf, String> {
    let root = canonical_directory_from_input(root, "照片目录")?;
    let relative = safe_relative_path(relative_path)?;
    if !formats::is_reference(&relative) && !formats::is_raw(&relative) {
        return Err("只能使用受支持的 JPG/RAW 照片".to_string());
    }
    let path = fs::canonicalize(root.join(relative))
        .map_err(|error| format!("照片已不存在或不可访问：{error}"))?;
    if !path.starts_with(&root) || !path.is_file() {
        return Err("照片解析后超出了所选目录".to_string());
    }
    Ok(path)
}

pub(crate) fn validate_operation_log_path(log_root: &Path, value: &str) -> Result<PathBuf, String> {
    let root = fs::canonicalize(log_root).map_err(|error| format!("日志目录不可访问：{error}"))?;
    let path = fs::canonicalize(value).map_err(|error| format!("操作日志不可访问：{error}"))?;
    if !path.starts_with(&root)
        || path.file_name().and_then(|name| name.to_str()) != Some("operations.jsonl")
    {
        return Err("操作日志路径不在应用日志目录中".to_string());
    }
    Ok(path)
}

pub(crate) fn write_audit_manifest(paths: &[String], destination: &Path) -> Result<(), String> {
    if formats::extension_of(destination) != "txt" {
        return Err("审计清单必须保存为 .txt 文件".to_string());
    }
    if paths.iter().any(|path| path.contains(['\r', '\n'])) {
        return Err("审计路径包含换行符，无法导出为逐行清单".to_string());
    }
    let parent = destination
        .parent()
        .ok_or_else(|| "无法确定审计清单保存目录".to_string())?;
    if !parent.is_dir() {
        return Err("审计清单保存目录不存在".to_string());
    }
    if let Ok(metadata) = fs::symlink_metadata(destination)
        && (metadata.file_type().is_symlink() || !metadata.is_file())
    {
        return Err("审计清单目标不是可信普通文件".to_string());
    }
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(destination)
        .map_err(|error| format!("无法创建审计清单：{error}"))?;
    for path in paths {
        writeln!(file, "{path}").map_err(|error| format!("无法写入审计清单：{error}"))?;
    }
    file.sync_data()
        .map_err(|error| format!("无法同步审计清单：{error}"))
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs_util::modified_ms;
    use crate::pair_cleanup::{CleanupCandidate, validate_delete_candidate};

    #[test]
    fn writes_a_utf8_audit_manifest_and_rejects_other_extensions() {
        let temp = tempfile::tempdir().expect("temp directory");
        let destination = temp.path().join("unmatched.txt");
        write_audit_manifest(
            &["day/orphan.JPG".to_string(), "第二天/照片.jpeg".to_string()],
            &destination,
        )
        .expect("manifest export");
        assert_eq!(
            fs::read_to_string(destination).expect("manifest contents"),
            "day/orphan.JPG\n第二天/照片.jpeg\n"
        );
        assert!(
            write_audit_manifest(
                &["day/orphan.JPG".to_string()],
                &temp.path().join("bad.csv")
            )
            .is_err()
        );
    }

    #[test]
    fn validates_dropped_directories_and_rejects_files() {
        let temp = tempfile::tempdir().expect("temp directory");
        let directory = temp.path().join("photos");
        let file = temp.path().join("photo.NEF");
        fs::create_dir_all(&directory).expect("photo directory");
        fs::write(&file, b"raw").expect("raw file");

        let canonical = canonical_directory_from_input(&directory.to_string_lossy(), "拖入路径")
            .expect("directory should be accepted");
        assert_eq!(
            canonical,
            fs::canonicalize(directory).expect("canonical path")
        );
        assert!(canonical_directory_from_input(&file.to_string_lossy(), "拖入路径").is_err());
    }

    #[test]
    fn rejects_path_traversal_and_unexpected_extensions() {
        assert!(safe_relative_path("../outside.NEF").is_err());
        assert!(safe_relative_path("/absolute.NEF").is_err());

        let temp = tempfile::tempdir().expect("temp directory");
        let file = temp.path().join("notes.txt");
        fs::write(&file, b"do not delete").expect("file");
        let metadata = fs::metadata(&file).expect("metadata");
        let candidate = CleanupCandidate {
            relative_path: "notes.txt".to_string(),
            expected_size_bytes: metadata.len(),
            expected_modified_ms: modified_ms(&metadata),
        };
        let root = fs::canonicalize(temp.path()).expect("canonical root");
        assert!(validate_delete_candidate(&root, &candidate).is_err());
    }

    #[test]
    fn reveal_path_resolution_stays_inside_raw_root() {
        let temp = tempfile::tempdir().expect("temp directory");
        let raw_root = temp.path().join("RAW");
        fs::create_dir_all(&raw_root).expect("raw directory");
        fs::write(raw_root.join("photo.NEF"), b"raw").expect("raw file");

        let raw_root_value = raw_root.to_string_lossy().into_owned();
        let resolved = resolve_scan_item_path(&raw_root_value, "photo.NEF").expect("safe path");
        assert_eq!(
            resolved,
            fs::canonicalize(raw_root.join("photo.NEF")).expect("canonical")
        );
        assert!(resolve_scan_item_path(&raw_root_value, "../outside.NEF").is_err());
    }

    #[test]
    fn operation_log_must_use_expected_file_inside_log_root() {
        let temp = tempfile::tempdir().expect("temp directory");
        let log_root = temp.path().join("logs");
        fs::create_dir_all(&log_root).expect("log directory");
        let operation_log = log_root.join("operations.jsonl");
        let other_log = log_root.join("other.jsonl");
        fs::write(&operation_log, b"{}").expect("operation log");
        fs::write(&other_log, b"{}").expect("other log");

        assert!(validate_operation_log_path(&log_root, &operation_log.to_string_lossy()).is_ok());
        assert!(validate_operation_log_path(&log_root, &other_log.to_string_lossy()).is_err());
    }
}
