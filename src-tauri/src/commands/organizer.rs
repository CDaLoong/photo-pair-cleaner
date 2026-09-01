//! 评分整理模块的命令：生成/执行整理计划，以及三种撤销路径。

use crate::app_state::{RatingStore, next_plan_id};
use crate::fs_util::now_ms;
use crate::{file_organizer, operation_history, operation_plan, photo_groups, ratings};
use std::path::Path;
use std::sync::Arc;
use tauri::Manager;

#[tauri::command]
pub(crate) async fn generate_operation_plan(
    app: tauri::AppHandle,
    rating_state: tauri::State<'_, RatingStore>,
    plan_state: tauri::State<'_, operation_plan::OperationPlanStore>,
    request: operation_plan::OperationPlanRequest,
) -> Result<operation_plan::OperationPlanSummary, String> {
    let database_path = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法确定评分数据目录：{error}"))?
        .join("photo-ratings.json");
    let access = Arc::clone(&rating_state.access);
    let plan_id = next_plan_id();
    let plan = tauri::async_runtime::spawn_blocking(move || {
        let _guard = access
            .lock()
            .map_err(|_| "无法锁定评分整理计划".to_string())?;
        let mut index = photo_groups::index_directory(Path::new(&request.root))?;
        let ratings = ratings::load_ratings(&database_path, Path::new(&request.root))?;
        photo_groups::apply_framepair_ratings(&mut index, &ratings);
        operation_plan::build_operation_plan(&index, &request, plan_id)
    })
    .await
    .map_err(|error| format!("评分整理计划任务异常结束：{error}"))??;
    let summary = plan.summary().clone();
    plan_state.replace(plan)?;
    Ok(summary)
}

#[tauri::command]
pub(crate) async fn execute_operation_plan(
    app: tauri::AppHandle,
    plan_state: tauri::State<'_, operation_plan::OperationPlanStore>,
    request: operation_plan::ExecutionSelection,
) -> Result<file_organizer::OrganizerExecutionSummary, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法确定评分整理历史目录：{error}"))?;
    let plan = plan_state.take_for_execution(&request)?;
    let operation_id = next_plan_id();
    let created_at_ms = now_ms();
    tauri::async_runtime::spawn_blocking(move || {
        file_organizer::execute_authorized_plan(&app_data_dir, operation_id, created_at_ms, plan)
    })
    .await
    .map_err(|error| format!("评分整理执行任务异常结束：{error}"))?
}

#[tauri::command]
pub(crate) async fn list_rating_operation_history(
    app: tauri::AppHandle,
) -> Result<Vec<operation_history::OperationHistoryEntry>, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法确定评分整理历史目录：{error}"))?;
    tauri::async_runtime::spawn_blocking(move || operation_history::list_operations(&app_data_dir))
        .await
        .map_err(|error| format!("读取评分整理历史任务异常结束：{error}"))?
}

#[tauri::command]
pub(crate) async fn restore_rating_move(
    app: tauri::AppHandle,
    request: file_organizer::OrganizerRecoveryRequest,
) -> Result<file_organizer::OrganizerRecoverySummary, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法确定评分整理历史目录：{error}"))?;
    let created_at_ms = now_ms();
    tauri::async_runtime::spawn_blocking(move || {
        file_organizer::restore_move_operation(
            &app_data_dir,
            &request.operation_id,
            &request.group_ids,
            created_at_ms,
        )
    })
    .await
    .map_err(|error| format!("恢复评分移动任务异常结束：{error}"))?
}

#[tauri::command]
pub(crate) async fn restore_rating_quarantine(
    app: tauri::AppHandle,
    request: file_organizer::OrganizerRecoveryRequest,
) -> Result<file_organizer::OrganizerRecoverySummary, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法确定评分整理历史目录：{error}"))?;
    let created_at_ms = now_ms();
    tauri::async_runtime::spawn_blocking(move || {
        file_organizer::restore_quarantine_operation(
            &app_data_dir,
            &request.operation_id,
            &request.group_ids,
            created_at_ms,
        )
    })
    .await
    .map_err(|error| format!("恢复评分隔离任务异常结束：{error}"))?
}

#[tauri::command]
pub(crate) async fn undo_rating_copy(
    app: tauri::AppHandle,
    request: file_organizer::OrganizerRecoveryRequest,
) -> Result<file_organizer::OrganizerRecoverySummary, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法确定评分整理历史目录：{error}"))?;
    let created_at_ms = now_ms();
    tauri::async_runtime::spawn_blocking(move || {
        file_organizer::undo_copy_operation(
            &app_data_dir,
            &request.operation_id,
            &request.group_ids,
            created_at_ms,
        )
    })
    .await
    .map_err(|error| format!("撤销评分复制任务异常结束：{error}"))?
}
