use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::Manager;

static WATERMARK_EXPORT_SEQUENCE: AtomicU64 = AtomicU64::new(1);

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
    pub(crate) photo_rect: WatermarkPreviewRect,
    pub(crate) layers: Vec<WatermarkPreviewLayerGeometry>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WatermarkPreviewRect {
    pub(crate) x: i64,
    pub(crate) y: i64,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WatermarkPreviewLayerGeometry {
    pub(crate) id: String,
    pub(crate) anchor_rect: WatermarkPreviewRect,
    pub(crate) center_x: i64,
    pub(crate) center_y: i64,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rotation_deg: f32,
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
pub(crate) async fn import_watermark_resource(
    path: String,
) -> Result<crate::watermark_model::EmbeddedTemplateResource, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::watermark_resource::import_image_resource(std::path::Path::new(&path))
    })
    .await
    .map_err(|error| format!("导入图片水印任务异常结束：{error}"))?
}

fn watermark_template_database(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法定位应用数据目录：{error}"))?
        .join("watermark-templates.json"))
}

#[tauri::command]
pub(crate) async fn list_watermark_templates(
    app: tauri::AppHandle,
) -> Result<Vec<crate::watermark_templates::WatermarkTemplateEntry>, String> {
    let path = watermark_template_database(&app)?;
    tauri::async_runtime::spawn_blocking(move || crate::watermark_templates::list_templates(&path))
        .await
        .map_err(|error| format!("读取水印模板任务异常结束：{error}"))?
}

#[tauri::command]
pub(crate) async fn save_watermark_template(
    app: tauri::AppHandle,
    template: crate::watermark_model::WatermarkTemplate,
    save_as: bool,
) -> Result<crate::watermark_templates::WatermarkTemplateEntry, String> {
    let path = watermark_template_database(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::watermark_templates::save_template(&path, template, save_as)
    })
    .await
    .map_err(|error| format!("保存水印模板任务异常结束：{error}"))?
}

#[tauri::command]
pub(crate) async fn delete_watermark_template(
    app: tauri::AppHandle,
    id: String,
) -> Result<(), String> {
    let path = watermark_template_database(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::watermark_templates::delete_template(&path, &id)
    })
    .await
    .map_err(|error| format!("删除水印模板任务异常结束：{error}"))?
}

#[tauri::command]
pub(crate) async fn import_watermark_template(
    app: tauri::AppHandle,
    path: String,
) -> Result<crate::watermark_templates::WatermarkTemplateEntry, String> {
    let database = watermark_template_database(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::watermark_templates::import_template(&database, std::path::Path::new(&path))
    })
    .await
    .map_err(|error| format!("导入水印模板任务异常结束：{error}"))?
}

#[tauri::command]
pub(crate) async fn export_watermark_template(
    path: String,
    template: crate::watermark_model::WatermarkTemplate,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::watermark_templates::export_template(std::path::Path::new(&path), &template)
    })
    .await
    .map_err(|error| format!("导出水印模板任务异常结束：{error}"))?
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
            photo_rect: WatermarkPreviewRect {
                x: rendered.layout.photo_rect.x,
                y: rendered.layout.photo_rect.y,
                width: rendered.layout.photo_rect.width,
                height: rendered.layout.photo_rect.height,
            },
            layers: rendered
                .layer_geometries
                .iter()
                .map(|geometry| WatermarkPreviewLayerGeometry {
                    id: geometry.id.clone(),
                    anchor_rect: WatermarkPreviewRect {
                        x: geometry.anchor_rect.x,
                        y: geometry.anchor_rect.y,
                        width: geometry.anchor_rect.width,
                        height: geometry.anchor_rect.height,
                    },
                    center_x: geometry.center_x,
                    center_y: geometry.center_y,
                    width: geometry.width,
                    height: geometry.height,
                    rotation_deg: geometry.rotation_deg,
                })
                .collect(),
        };
        let png = crate::watermark_render::encode_preview_png(&rendered)?;
        preview_envelope(&header, &png)
    })
    .await
    .map_err(|error| format!("渲染水印预览任务异常结束：{error}"))??;
    Ok(tauri::ipc::Response::new(envelope))
}

fn next_watermark_export_id() -> String {
    let sequence = WATERMARK_EXPORT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!("watermark-{now:x}-{sequence:x}")
}

#[tauri::command]
pub(crate) fn start_watermark_export(
    app: tauri::AppHandle,
    store: tauri::State<'_, crate::watermark_export::WatermarkExportStore>,
    request: crate::watermark_export::WatermarkExportRequest,
    on_event: tauri::ipc::Channel<crate::watermark_export::WatermarkExportEvent>,
) -> Result<String, String> {
    let items = crate::watermark_export::build_export_items(&request)?;
    crate::watermark_export::preflight_disk_space(&items)?;
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|error| format!("无法定位应用资源目录：{error}"))?;
    let task_id = next_watermark_export_id();
    let task = store.insert(&task_id, items)?;
    let indices = (0..task.item_count()).collect();
    let executor = Arc::new(crate::watermark_export::RealOutputExecutor::new(
        resource_dir,
    ));
    let event_channel = on_event.clone();
    std::thread::spawn(move || {
        let _ = crate::watermark_export::run_export_task(
            task,
            indices,
            executor,
            Arc::new(move |event| {
                let _ = event_channel.send(event);
            }),
            crate::watermark_export::default_export_concurrency(),
        );
    });
    Ok(task_id)
}

#[tauri::command]
pub(crate) fn cancel_watermark_export(
    store: tauri::State<'_, crate::watermark_export::WatermarkExportStore>,
    task_id: String,
) -> Result<(), String> {
    store.get(&task_id)?.cancel();
    Ok(())
}

#[tauri::command]
pub(crate) fn retry_watermark_export_failures(
    app: tauri::AppHandle,
    store: tauri::State<'_, crate::watermark_export::WatermarkExportStore>,
    task_id: String,
    on_event: tauri::ipc::Channel<crate::watermark_export::WatermarkExportEvent>,
) -> Result<(), String> {
    let task = store.get(&task_id)?;
    if task.is_running() || !task.is_completed() {
        return Err("水印导出任务尚未结束".into());
    }
    let indices = task.failed_indices();
    if indices.is_empty() {
        return Err("当前任务没有可重试的失败项".into());
    }
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|error| format!("无法定位应用资源目录：{error}"))?;
    let executor = Arc::new(crate::watermark_export::RealOutputExecutor::new(
        resource_dir,
    ));
    let event_channel = on_event.clone();
    std::thread::spawn(move || {
        let _ = crate::watermark_export::run_export_task(
            task,
            indices,
            executor,
            Arc::new(move |event| {
                let _ = event_channel.send(event);
            }),
            crate::watermark_export::default_export_concurrency(),
        );
    });
    Ok(())
}

#[tauri::command]
pub(crate) async fn reveal_watermark_export(
    store: tauri::State<'_, crate::watermark_export::WatermarkExportStore>,
    task_id: String,
) -> Result<(), String> {
    let output_directory = store.completed_output_directory(&task_id)?;
    tauri::async_runtime::spawn_blocking(move || crate::platform::reveal_path(&output_directory))
        .await
        .map_err(|error| format!("打开水印输出目录任务异常结束：{error}"))?
}

#[tauri::command]
pub(crate) fn acknowledge_watermark_export(
    store: tauri::State<'_, crate::watermark_export::WatermarkExportStore>,
    task_id: String,
) -> Result<(), String> {
    store.acknowledge(&task_id)
}

#[cfg(test)]
mod tests {
    use super::{WatermarkPreviewHeader, WatermarkPreviewRect, preview_envelope};

    #[test]
    fn watermark_preview_envelope_prefixes_a_big_endian_header_length() {
        let header = WatermarkPreviewHeader {
            width: 320,
            height: 200,
            warnings: vec!["字体已回退".into()],
            photo_rect: WatermarkPreviewRect {
                x: 12,
                y: 8,
                width: 296,
                height: 170,
            },
            layers: Vec::new(),
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
