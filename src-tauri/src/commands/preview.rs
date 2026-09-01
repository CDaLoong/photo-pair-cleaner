//! 预览相关命令：缩略图、原图、缓存统计、macOS 原生 Quick Look 浮层。

#[cfg(target_os = "macos")]
use crate::native_preview;
use crate::preview;
use serde::Deserialize;
use std::path::Path;
use tauri::Manager;

#[tauri::command]
pub(crate) async fn load_photo_thumbnail(
    app: tauri::AppHandle,
    root: String,
    relative_path: String,
    max_edge: u32,
) -> Result<tauri::ipc::Response, String> {
    let cache_root = app
        .path()
        .app_cache_dir()
        .map_err(|error| format!("无法确定缩略图缓存目录：{error}"))?
        .join("photo-thumbnails");
    let bytes = tauri::async_runtime::spawn_blocking(move || {
        preview::load_thumbnail(Path::new(&root), &relative_path, max_edge, &cache_root)
    })
    .await
    .map_err(|error| format!("缩略图任务异常结束：{error}"))??;
    Ok(tauri::ipc::Response::new(bytes))
}

#[tauri::command]
pub(crate) async fn prepare_photo_original(
    app: tauri::AppHandle,
    root: String,
    relative_path: String,
) -> Result<String, String> {
    let source = tauri::async_runtime::spawn_blocking(move || {
        preview::resolve_preview_path(Path::new(&root), &relative_path)
    })
    .await
    .map_err(|error| format!("原图授权任务异常结束：{error}"))??;
    app.asset_protocol_scope()
        .allow_file(&source)
        .map_err(|error| format!("无法授权原图读取：{error}"))?;
    Ok(source.to_string_lossy().into_owned())
}

#[tauri::command]
pub(crate) async fn get_preview_cache_stats(
    app: tauri::AppHandle,
) -> Result<preview::PreviewCacheStats, String> {
    let cache_root = app
        .path()
        .app_cache_dir()
        .map_err(|error| format!("无法确定缩略图缓存目录：{error}"))?
        .join("photo-thumbnails");
    tauri::async_runtime::spawn_blocking(move || preview::cache_stats(&cache_root))
        .await
        .map_err(|error| format!("缩略图缓存统计任务异常结束：{error}"))?
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativePreviewRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    viewport_width: f64,
    viewport_height: f64,
}

#[cfg(target_os = "macos")]
#[tauri::command]
pub(crate) async fn show_native_photo_preview(
    app: tauri::AppHandle,
    root: String,
    relative_path: String,
    preview_id: String,
    rect: NativePreviewRect,
) -> Result<bool, String> {
    if preview_id.is_empty() || preview_id.len() > 4096 {
        return Err("原生预览标识无效".to_string());
    }
    let source = tauri::async_runtime::spawn_blocking(move || {
        preview::resolve_preview_path(Path::new(&root), &relative_path)
    })
    .await
    .map_err(|error| format!("原生预览授权任务异常结束：{error}"))??;
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "无法获取主预览窗口".to_string())?;
    let native_rect = native_preview::PreviewRect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
        viewport_width: rect.viewport_width,
        viewport_height: rect.viewport_height,
    };
    tauri::async_runtime::spawn_blocking(move || {
        let preview_window = window.clone();
        let _ = window.run_on_main_thread(move || {
            let _ = preview_window.with_webview(move |webview| {
                let _ = unsafe {
                    native_preview::show(webview.inner(), preview_id, &source, native_rect)
                };
            });
        });
    });
    Ok(true)
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub(crate) async fn show_native_photo_preview(
    _app: tauri::AppHandle,
    _root: String,
    _relative_path: String,
    _preview_id: String,
    _rect: NativePreviewRect,
) -> Result<bool, String> {
    Ok(false)
}

#[cfg(target_os = "macos")]
#[tauri::command]
pub(crate) async fn hide_native_photo_preview(
    app: tauri::AppHandle,
    preview_id: String,
) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "无法获取主预览窗口".to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let preview_window = window.clone();
        let _ = window.run_on_main_thread(move || {
            let _ =
                preview_window.with_webview(move |_| unsafe { native_preview::hide(&preview_id) });
        });
    });
    Ok(())
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub(crate) async fn hide_native_photo_preview(
    _app: tauri::AppHandle,
    _preview_id: String,
) -> Result<(), String> {
    Ok(())
}
