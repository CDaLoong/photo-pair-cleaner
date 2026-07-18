#[tauri::command]
pub(crate) async fn prepare_watermark_source(
    request: crate::watermark_source::WatermarkSourceRequest,
) -> Result<crate::watermark_model::WatermarkSourceSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || crate::watermark_source::prepare_source(request))
        .await
        .map_err(|error| format!("准备水印照片任务异常结束：{error}"))?
}
