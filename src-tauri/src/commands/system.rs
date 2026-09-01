//! 系统集成命令：外部编辑器、操作日志、回收站。

use crate::app_paths::{resolve_photo_asset_path, validate_operation_log_path};
use crate::editors;
use crate::platform::{open_trash_location, reveal_path};
use tauri::Manager;

#[tauri::command]
pub(crate) async fn list_external_editors() -> Result<Vec<editors::ExternalEditor>, String> {
    tauri::async_runtime::spawn_blocking(editors::discover_installed)
        .await
        .map_err(|error| format!("发现外部编辑器任务异常结束：{error}"))
}

#[tauri::command]
pub(crate) async fn open_photo_in_editor(
    root: String,
    relative_path: String,
    editor_id: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let photo = resolve_photo_asset_path(&root, &relative_path)?;
        let editor = editors::discover_installed()
            .into_iter()
            .find(|editor| editor.id == editor_id)
            .ok_or_else(|| "外部编辑器不存在或已经卸载".to_string())?;
        editors::open_with(&editor, &photo)
    })
    .await
    .map_err(|error| format!("启动外部编辑器任务异常结束：{error}"))?
}
#[tauri::command]
pub(crate) async fn reveal_operation_log(
    app: tauri::AppHandle,
    log_path: String,
) -> Result<(), String> {
    let log_root = app
        .path()
        .app_log_dir()
        .map_err(|error| format!("无法确定应用日志目录：{error}"))?;
    tauri::async_runtime::spawn_blocking(move || {
        let path = validate_operation_log_path(&log_root, &log_path)?;
        reveal_path(&path)
    })
    .await
    .map_err(|error| format!("定位操作日志任务异常结束：{error}"))?
}

#[tauri::command]
pub(crate) async fn open_system_trash() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(open_trash_location)
        .await
        .map_err(|error| format!("打开回收站任务异常结束：{error}"))?
}
