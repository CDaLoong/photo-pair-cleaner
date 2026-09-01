//! 成对清理模块的命令：目录校验、扫描、执行清理、审计清单、隔离区恢复。

use crate::app_paths::{resolve_scan_item_path, write_audit_manifest};
use crate::app_state::{CurrentPlan, ScanPlanStore, next_plan_id};
use crate::fs_util::canonical_directory_from_input;
use crate::pair_cleanup::*;
use crate::pair_scan::*;
use crate::platform::reveal_path;
use crate::quarantine;
use crate::safety::{CleanupPlan, FileSnapshot};
use std::fs::{self};
use std::path::Path;
use tauri::Manager;

#[tauri::command]
pub(crate) async fn validate_directory_path(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        canonical_directory_from_input(&path, "拖入路径")
            .map(|directory| directory.to_string_lossy().into_owned())
    })
    .await
    .map_err(|error| format!("目录校验任务异常结束：{error}"))?
}

#[tauri::command]
pub(crate) async fn scan_pairs(
    state: tauri::State<'_, ScanPlanStore>,
    request: ScanRequest,
) -> Result<ScanSummary, String> {
    *state
        .current
        .lock()
        .map_err(|_| "无法重置清理计划状态".to_string())? = None;
    let raw_root = canonical_directory_from_input(&request.raw_root, "RAW 源目录")?;
    let mut summary = tauri::async_runtime::spawn_blocking(move || scan_pairs_impl(&request))
        .await
        .map_err(|error| format!("扫描任务异常结束：{error}"))??;

    let plan_id = next_plan_id();
    let candidates = summary
        .items
        .iter()
        .filter(|item| {
            summary.mode == ScanMode::CleanupRaw && item.match_status == MatchStatus::Unmatched
        })
        .map(|item| {
            (
                item.relative_path.clone(),
                FileSnapshot::new(item.size_bytes, item.modified_ms),
            )
        });
    let audit_paths = summary
        .items
        .iter()
        .filter(|item| {
            summary.mode == ScanMode::AuditReference
                && item.match_status == MatchStatus::Unmatched
                && item.kind == FileKind::Reference
        })
        .map(|item| item.relative_path.clone())
        .collect();
    let plan = CleanupPlan::new(plan_id.clone(), raw_root, candidates);
    summary.plan_id = plan_id;
    *state
        .current
        .lock()
        .map_err(|_| "无法保存清理计划状态".to_string())? = Some(CurrentPlan {
        cleanup: plan,
        mode: summary.mode,
        audit_paths,
    });
    Ok(summary)
}

#[tauri::command]
pub(crate) async fn execute_cleanup(
    app: tauri::AppHandle,
    state: tauri::State<'_, ScanPlanStore>,
    request: CleanupRequest,
) -> Result<CleanupSummary, String> {
    if request.items.is_empty() {
        return Err("清理计划中没有文件".to_string());
    }
    let raw_root = canonical_directory_from_input(&request.raw_root, "RAW 源目录")?;
    let plan = {
        let mut current = state
            .current
            .lock()
            .map_err(|_| "无法读取清理计划状态".to_string())?;
        let current_plan = current
            .as_ref()
            .ok_or_else(|| "清理计划不存在，请重新扫描".to_string())?;
        if current_plan.mode != ScanMode::CleanupRaw {
            return Err("当前是只读审计计划，不能执行清理".to_string());
        }
        if !current_plan.cleanup.matches(&request.plan_id, &raw_root) {
            return Err("清理计划已失效或 RAW 源目录不匹配，请重新扫描".to_string());
        }
        current
            .take()
            .ok_or_else(|| "清理计划不存在，请重新扫描".to_string())?
            .cleanup
    };
    let log_dir = app.path().app_log_dir().ok();
    tauri::async_runtime::spawn_blocking(move || cleanup_impl(&request, log_dir.as_deref(), &plan))
        .await
        .map_err(|error| format!("清理任务异常结束：{error}"))?
}

#[tauri::command]
pub(crate) async fn export_audit_manifest(
    state: tauri::State<'_, ScanPlanStore>,
    plan_id: String,
    raw_root: String,
    destination: String,
) -> Result<(), String> {
    let raw_root = canonical_directory_from_input(&raw_root, "RAW 源目录")?;
    let audit_paths = {
        let current = state
            .current
            .lock()
            .map_err(|_| "无法读取审计计划状态".to_string())?;
        let plan = current
            .as_ref()
            .ok_or_else(|| "审计计划不存在，请重新扫描".to_string())?;
        if plan.mode != ScanMode::AuditReference {
            return Err("当前计划不是反向审计计划".to_string());
        }
        if !plan.cleanup.matches(&plan_id, &raw_root) {
            return Err("审计计划已失效或 RAW 源目录不匹配，请重新扫描".to_string());
        }
        plan.audit_paths.clone()
    };
    tauri::async_runtime::spawn_blocking(move || {
        write_audit_manifest(&audit_paths, Path::new(&destination))
    })
    .await
    .map_err(|error| format!("导出审计清单任务异常结束：{error}"))?
}

#[tauri::command]
pub(crate) async fn list_quarantine_operations(
    raw_root: String,
) -> Result<Vec<quarantine::QuarantineOperation>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = canonical_directory_from_input(&raw_root, "RAW 源目录")?;
        quarantine::list_operations(&root)
    })
    .await
    .map_err(|error| format!("读取隔离历史任务异常结束：{error}"))?
}

#[tauri::command]
pub(crate) async fn restore_quarantine_operation(
    raw_root: String,
    operation_id: String,
) -> Result<RestoreSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = canonical_directory_from_input(&raw_root, "RAW 源目录")?;
        let results = quarantine::restore_operation(&root, &operation_id)?
            .into_iter()
            .map(|result| CleanupResult {
                relative_path: result.relative_path,
                success: result.success,
                message: result.message,
            })
            .collect::<Vec<_>>();
        let succeeded = results.iter().filter(|result| result.success).count();
        Ok(RestoreSummary {
            succeeded,
            failed: results.len().saturating_sub(succeeded),
            results,
        })
    })
    .await
    .map_err(|error| format!("恢复隔离文件任务异常结束：{error}"))?
}

#[tauri::command]
pub(crate) async fn reveal_quarantine_operation(
    raw_root: String,
    operation_id: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = canonical_directory_from_input(&raw_root, "RAW 源目录")?;
        let path = quarantine::operation_root(&root, &operation_id)?;
        let path = fs::canonicalize(path).map_err(|error| format!("隔离目录不可访问：{error}"))?;
        if !path.starts_with(&root) {
            return Err("隔离目录解析后超出了 RAW 源目录".to_string());
        }
        reveal_path(&path)
    })
    .await
    .map_err(|error| format!("定位隔离目录任务异常结束：{error}"))?
}

#[tauri::command]
pub(crate) async fn reveal_scan_item(root: String, relative_path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = resolve_scan_item_path(&root, &relative_path)?;
        reveal_path(&path)
    })
    .await
    .map_err(|error| format!("定位文件任务异常结束：{error}"))?
}
