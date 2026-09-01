//! 成对清理领域：把扫描结果转成一次性授权计划并执行。
//!
//! 安全约定：前端永远只提交 plan_id 和勾选项，绝不提交文件路径或动作类型。
//! 路径与动作都由后端从已授权的计划里取，且每份计划只能被消费一次。

use crate::app_paths::safe_relative_path;
use crate::fs_util::{canonical_directory_from_input, modified_ms};
use crate::safety::{CleanupPlan, FileSnapshot};
use crate::{formats, quarantine};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupCandidate {
    pub relative_path: String,
    pub expected_size_bytes: u64,
    pub expected_modified_ms: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupRequest {
    pub plan_id: String,
    pub raw_root: String,
    pub destination: CleanupDestination,
    pub items: Vec<CleanupCandidate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CleanupDestination {
    Trash,
    Quarantine,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupResult {
    pub relative_path: String,
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupSummary {
    pub succeeded: usize,
    pub failed: usize,
    pub destination: CleanupDestination,
    pub operation_id: Option<String>,
    pub quarantine_path: Option<String>,
    pub results: Vec<CleanupResult>,
    pub log_path: Option<String>,
    pub log_warning: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreSummary {
    pub succeeded: usize,
    pub failed: usize,
    pub results: Vec<CleanupResult>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OperationLogRecord<'a> {
    pub(crate) timestamp: String,
    pub(crate) raw_root: &'a str,
    pub(crate) relative_path: &'a str,
    pub(crate) destination: CleanupDestination,
    pub(crate) success: bool,
    pub(crate) message: &'a str,
}

pub(crate) fn validate_delete_candidate(
    raw_root: &Path,
    candidate: &CleanupCandidate,
) -> Result<PathBuf, String> {
    let relative = safe_relative_path(&candidate.relative_path)?;
    let extension = formats::extension_of(&relative);
    if !formats::is_raw(&relative) && !formats::is_sidecar(&relative) {
        return Err(format!("不允许处理 .{extension} 文件"));
    }

    let path = fs::canonicalize(raw_root.join(relative))
        .map_err(|error| format!("文件已不存在或不可访问：{error}"))?;
    if !path.starts_with(raw_root) {
        return Err("文件解析后超出了 RAW 源目录".to_string());
    }
    let metadata = fs::metadata(&path).map_err(|error| format!("无法读取文件信息：{error}"))?;
    if !metadata.is_file() {
        return Err("目标不是普通文件".to_string());
    }
    if metadata.len() != candidate.expected_size_bytes {
        return Err("文件大小在扫描后发生变化，请重新扫描".to_string());
    }
    if let Some(expected) = candidate.expected_modified_ms
        && modified_ms(&metadata) != Some(expected)
    {
        return Err("文件修改时间在扫描后发生变化，请重新扫描".to_string());
    }
    Ok(path)
}

pub(crate) fn write_operation_log(
    log_dir: Option<&Path>,
    raw_root: &str,
    destination: CleanupDestination,
    results: &[CleanupResult],
) -> (Option<String>, Option<String>) {
    let Some(log_dir) = log_dir else {
        return (None, Some("无法确定应用日志目录".to_string()));
    };
    if let Err(error) = fs::create_dir_all(log_dir) {
        return (None, Some(format!("无法创建日志目录：{error}")));
    }
    let log_path = log_dir.join("operations.jsonl");
    let mut file = match OpenOptions::new().create(true).append(true).open(&log_path) {
        Ok(file) => file,
        Err(error) => return (None, Some(format!("无法打开操作日志：{error}"))),
    };

    for result in results {
        let record = OperationLogRecord {
            timestamp: Utc::now().to_rfc3339(),
            raw_root,
            relative_path: &result.relative_path,
            destination,
            success: result.success,
            message: &result.message,
        };
        let line = match serde_json::to_string(&record) {
            Ok(line) => line,
            Err(error) => return (None, Some(format!("无法序列化操作日志：{error}"))),
        };
        if let Err(error) = writeln!(file, "{line}") {
            return (None, Some(format!("无法写入操作日志：{error}")));
        }
    }

    (Some(log_path.to_string_lossy().into_owned()), None)
}

pub(crate) fn cleanup_impl(
    request: &CleanupRequest,
    log_dir: Option<&Path>,
    plan: &CleanupPlan,
) -> Result<CleanupSummary, String> {
    let raw_root = canonical_directory_from_input(&request.raw_root, "RAW 源目录")?;
    let mut results = Vec::with_capacity(request.items.len());

    for candidate in &request.items {
        let snapshot = FileSnapshot::new(
            candidate.expected_size_bytes,
            candidate.expected_modified_ms,
        );
        let authorized = plan.authorize(
            &request.plan_id,
            &raw_root,
            &candidate.relative_path,
            &snapshot,
        );
        let outcome = match authorized.and_then(|_| validate_delete_candidate(&raw_root, candidate))
        {
            Ok(path) => match request.destination {
                CleanupDestination::Trash => trash::delete(&path)
                    .map(|_| "已移入系统回收站/废纸篓".to_string())
                    .map_err(|error| format!("移入系统回收站/废纸篓失败：{error}")),
                CleanupDestination::Quarantine => quarantine::move_file(
                    &raw_root,
                    &request.plan_id,
                    Path::new(&candidate.relative_path),
                )
                .map(|_| "已移入 FramePair 隔离区".to_string()),
            },
            Err(error) => Err(error),
        };
        match outcome {
            Ok(message) => results.push(CleanupResult {
                relative_path: candidate.relative_path.clone(),
                success: true,
                message,
            }),
            Err(message) => results.push(CleanupResult {
                relative_path: candidate.relative_path.clone(),
                success: false,
                message,
            }),
        }
    }

    let succeeded = results.iter().filter(|result| result.success).count();
    let failed = results.len().saturating_sub(succeeded);
    let (log_path, log_warning) =
        write_operation_log(log_dir, &request.raw_root, request.destination, &results);
    let quarantined = request.destination == CleanupDestination::Quarantine && succeeded > 0;
    Ok(CleanupSummary {
        succeeded,
        failed,
        destination: request.destination,
        operation_id: quarantined.then(|| request.plan_id.clone()),
        quarantine_path: quarantined
            .then(|| quarantine::operation_root(&raw_root, &request.plan_id))
            .transpose()?
            .map(|path| path.to_string_lossy().into_owned()),
        results,
        log_path,
        log_warning,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats;
    use std::path::Path;

    #[test]
    fn delete_validation_accepts_supported_raws_and_rejects_other_files() {
        let temp = tempfile::tempdir().expect("temp directory");
        let raw = temp.path().join("photo.CR3");
        let text = temp.path().join("notes.txt");
        fs::write(&raw, b"raw").expect("raw file");
        fs::write(&text, b"text").expect("text file");
        let root = fs::canonicalize(temp.path()).expect("canonical root");

        for (name, accepted) in [("photo.CR3", true), ("notes.txt", false)] {
            let metadata = fs::metadata(temp.path().join(name)).expect("metadata");
            let candidate = CleanupCandidate {
                relative_path: name.to_string(),
                expected_size_bytes: metadata.len(),
                expected_modified_ms: modified_ms(&metadata),
            };
            assert_eq!(
                validate_delete_candidate(&root, &candidate).is_ok(),
                accepted,
                "unexpected validation result for {name}"
            );
        }
    }

    #[test]
    fn cleanup_plan_can_move_an_authorized_file_to_quarantine() {
        let temp = tempfile::tempdir().expect("temp directory");
        let raw_root = temp.path().join("RAW");
        fs::create_dir_all(&raw_root).expect("raw directory");
        let raw = raw_root.join("day/photo.CR3");
        fs::create_dir_all(raw.parent().expect("raw parent")).expect("raw parent directory");
        fs::write(&raw, b"raw").expect("raw file");
        let metadata = fs::metadata(&raw).expect("metadata");
        let relative_path = "day/photo.CR3".to_string();
        let snapshot = FileSnapshot::new(metadata.len(), modified_ms(&metadata));
        let plan = CleanupPlan::new(
            "1000-1-1".to_string(),
            fs::canonicalize(&raw_root).expect("canonical raw root"),
            [(relative_path.clone(), snapshot)],
        );
        let request = CleanupRequest {
            plan_id: "1000-1-1".to_string(),
            raw_root: raw_root.to_string_lossy().into_owned(),
            destination: CleanupDestination::Quarantine,
            items: vec![CleanupCandidate {
                relative_path: relative_path.clone(),
                expected_size_bytes: metadata.len(),
                expected_modified_ms: modified_ms(&metadata),
            }],
        };

        let result = cleanup_impl(&request, None, &plan).expect("cleanup");
        assert_eq!(result.succeeded, 1);
        assert_eq!(result.failed, 0);
        assert_eq!(result.operation_id.as_deref(), Some("1000-1-1"));
        assert!(!raw.exists());
        assert!(
            raw_root
                .join(".framepair-quarantine/1000-1-1/day/photo.CR3")
                .exists()
        );
    }
}
