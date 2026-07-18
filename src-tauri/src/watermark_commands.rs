use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::Manager;

#[derive(Default)]
pub(crate) struct WatermarkRenderState {
    catalog: Arc<Mutex<Option<CachedFontCatalog>>>,
}

struct CachedFontCatalog {
    resource_dir: PathBuf,
    catalog: crate::watermark_text::FontCatalog,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WatermarkPreviewHeader {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) warnings: Vec<String>,
}

pub(crate) fn preview_envelope(
    header: &WatermarkPreviewHeader,
    png: &[u8],
) -> Result<Vec<u8>, String> {
    let json =
        serde_json::to_vec(header).map_err(|error| format!("无法序列化水印预览信息：{error}"))?;
    let length = u32::try_from(json.len()).map_err(|_| "水印预览信息过大".to_string())?;
    let capacity = 4usize
        .checked_add(json.len())
        .and_then(|value| value.checked_add(png.len()))
        .ok_or_else(|| "水印预览响应大小溢出".to_string())?;
    let mut output = Vec::with_capacity(capacity);
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(&json);
    output.extend_from_slice(png);
    Ok(output)
}

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

#[tauri::command]
pub(crate) async fn render_watermark_preview(
    app: tauri::AppHandle,
    state: tauri::State<'_, WatermarkRenderState>,
    photo: crate::watermark_model::WatermarkSourcePhoto,
    request: crate::watermark_model::WatermarkRenderRequest,
    max_edge: u32,
) -> Result<tauri::ipc::Response, String> {
    if !(256..=2400).contains(&max_edge) {
        return Err("水印预览长边必须在 256 到 2400 像素之间".into());
    }
    if request.source != photo {
        return Err("水印预览请求与授权照片不一致".into());
    }
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|error| format!("无法定位应用资源目录：{error}"))?;
    let shared_catalog = state.catalog.clone();
    let envelope = tauri::async_runtime::spawn_blocking(move || {
        let source = crate::watermark_source::revalidate_photo(&photo)?;
        let mut cached = shared_catalog
            .lock()
            .map_err(|_| "水印字体缓存状态异常".to_string())?;
        if cached
            .as_ref()
            .is_none_or(|current| current.resource_dir != resource_dir)
        {
            *cached = Some(CachedFontCatalog {
                resource_dir: resource_dir.clone(),
                catalog: crate::watermark_text::FontCatalog::new(&resource_dir)?,
            });
        }
        let catalog = &mut cached
            .as_mut()
            .ok_or_else(|| "水印字体缓存初始化失败".to_string())?
            .catalog;
        let rendered = crate::watermark_render::render_request_with_catalog(
            &source,
            &request,
            catalog,
            crate::watermark_render::RenderTarget::Preview { max_edge },
        )?;
        let header = WatermarkPreviewHeader {
            width: rendered.image.width(),
            height: rendered.image.height(),
            warnings: rendered.warnings.clone(),
        };
        let png = crate::watermark_render::encode_preview_png(&rendered)?;
        preview_envelope(&header, &png)
    })
    .await
    .map_err(|error| format!("渲染水印预览任务异常结束：{error}"))??;
    Ok(tauri::ipc::Response::new(envelope))
}

#[cfg(test)]
mod tests {
    use super::{WatermarkPreviewHeader, preview_envelope};

    #[test]
    fn watermark_preview_envelope_prefixes_a_big_endian_header_length() {
        let header = WatermarkPreviewHeader {
            width: 320,
            height: 200,
            warnings: vec!["字体已回退".into()],
        };
        let png = [137, 80, 78, 71];
        let envelope = preview_envelope(&header, &png).unwrap();
        let header_length = u32::from_be_bytes(envelope[..4].try_into().unwrap()) as usize;
        let decoded: serde_json::Value =
            serde_json::from_slice(&envelope[4..4 + header_length]).unwrap();
        assert_eq!(decoded["width"], 320);
        assert_eq!(decoded["height"], 200);
        assert_eq!(&envelope[4 + header_length..], png);
    }
}
