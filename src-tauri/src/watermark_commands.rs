use tauri::Manager;

#[tauri::command]
pub(crate) async fn prepare_watermark_source(
    request: crate::watermark_source::WatermarkSourceRequest,
) -> Result<crate::watermark_model::WatermarkSourceSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || crate::watermark_source::prepare_source(request))
        .await
        .map_err(|error| format!("准备水印照片任务异常结束：{error}"))?
}

#[tauri::command]
pub(crate) async fn list_watermark_fonts(
    app: tauri::AppHandle,
) -> Result<Vec<crate::watermark_text::FontSummary>, String> {
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|error| format!("无法定位应用资源目录：{error}"))?;
    tauri::async_runtime::spawn_blocking(move || crate::watermark_text::list_fonts(&resource_dir))
        .await
        .map_err(|error| format!("读取字体列表任务异常结束：{error}"))?
}
