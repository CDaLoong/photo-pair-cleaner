use base64::Engine;
use image::ImageFormat;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::NamedTempFile;

use crate::watermark_model::{
    EmbeddedTemplateResource, GradientStop, ImageFit, NormalizedPlacement, TextAlign,
    VariantLayerLayout, WATERMARK_SCHEMA_VERSION, WatermarkAnchorSpace, WatermarkBackground,
    WatermarkFrameEdge, WatermarkLayer, WatermarkLayerBase, WatermarkTemplate, default_template,
    validate_template,
};

const TEMPLATE_DATABASE_VERSION: u16 = 1;
const TEMPLATE_FILE_VERSION: u16 = 1;
const MAX_TEMPLATE_FILE_BYTES: u64 = 180 * 1024 * 1024;
const MAX_RESOURCE_EDGE: u32 = 16_384;
const MAX_RESOURCE_PIXELS: u64 = 100_000_000;

const BUILTIN_TEMPLATE_IDS: [(&str, &str); 6] = [
    ("minimal-signature", "极简署名"),
    ("white-exif-frame", "白色 EXIF 底边框"),
    ("dark-gallery-frame", "深色画廊边框"),
    ("gradient-magazine", "渐变杂志边框"),
    ("blurred-extension", "照片模糊延展"),
    ("transparent-logo", "透明 Logo 角标"),
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WatermarkTemplateEntry {
    pub(crate) template: WatermarkTemplate,
    pub(crate) built_in: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WatermarkTemplateDatabase {
    schema_version: u16,
    templates: Vec<WatermarkTemplate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WatermarkTemplateFile {
    schema_version: u16,
    template: WatermarkTemplate,
}

fn builtin_id(id: &str) -> bool {
    BUILTIN_TEMPLATE_IDS
        .iter()
        .any(|(candidate, _)| *candidate == id)
}

fn base(id: &str, name: &str, z_index: i32) -> WatermarkLayerBase {
    WatermarkLayerBase {
        id: id.to_string(),
        name: name.to_string(),
        z_index,
        visible: true,
        locked: false,
    }
}

fn placement(
    anchor_space: WatermarkAnchorSpace,
    frame_edge: Option<WatermarkFrameEdge>,
    x: f32,
    y: f32,
    width: f32,
    font_size_ratio: Option<f32>,
) -> VariantLayerLayout {
    VariantLayerLayout {
        placement: NormalizedPlacement {
            anchor_space,
            frame_edge,
            x,
            y,
            width,
            rotation_deg: 0.0,
            opacity: 1.0,
        },
        font_size_ratio,
    }
}

fn text_layer(id: &str, name: &str, text: &str, color: &str) -> WatermarkLayer {
    WatermarkLayer::Text {
        base: base(id, name, 0),
        text: text.to_string(),
        font_family: "Noto Sans CJK SC".to_string(),
        font_weight: 500,
        color: color.to_string(),
        align: TextAlign::Center,
        letter_spacing_ratio: 0.02,
        line_height: 1.2,
        stroke_color: "#00000000".to_string(),
        stroke_width_ratio: 0.0,
        shadow_color: "#00000055".to_string(),
        shadow_blur_ratio: 0.0,
        shadow_offset_x_ratio: 0.0,
        shadow_offset_y_ratio: 0.0,
    }
}

fn add_layer(template: &mut WatermarkTemplate, layer: WatermarkLayer, layout: VariantLayerLayout) {
    let id = layer.base().id.clone();
    template.shared.layers.push(layer);
    for variant in template.variants.values_mut() {
        variant.layer_layouts.insert(id.clone(), layout.clone());
    }
}

fn logo_resource() -> Result<EmbeddedTemplateResource, String> {
    let bytes = include_bytes!("../icons/icon.png");
    let image = image::load_from_memory_with_format(bytes, ImageFormat::Png)
        .map_err(|error| format!("无法读取内置 Logo：{error}"))?;
    let sha256 = format!("{:x}", Sha256::digest(bytes));
    Ok(EmbeddedTemplateResource {
        id: "framepair-logo".into(),
        name: "FramePair Logo.png".into(),
        mime_type: "image/png".into(),
        sha256,
        width: image.width(),
        height: image.height(),
        data_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
    })
}

pub(crate) fn built_in_templates() -> Result<Vec<WatermarkTemplate>, String> {
    let mut minimal = default_template(BUILTIN_TEMPLATE_IDS[0].0, BUILTIN_TEMPLATE_IDS[0].1);
    add_layer(
        &mut minimal,
        text_layer("signature", "署名", "YOUR NAME", "#202321"),
        placement(
            WatermarkAnchorSpace::Frame,
            Some(WatermarkFrameEdge::Bottom),
            0.5,
            0.5,
            0.55,
            Some(0.05),
        ),
    );

    let mut exif = default_template(BUILTIN_TEMPLATE_IDS[1].0, BUILTIN_TEMPLATE_IDS[1].1);
    exif.shared.layers.push(WatermarkLayer::ExifText {
        base: base("exif", "拍摄参数", 0),
        fields: vec![
            "cameraModel".into(),
            "lensModel".into(),
            "focalLength".into(),
            "aperture".into(),
            "shutterSpeed".into(),
            "iso".into(),
        ],
        separator: " · ".into(),
        prefix: String::new(),
        suffix: String::new(),
        missing_value: None,
        font_family: "Noto Sans CJK SC".into(),
        font_weight: 400,
        color: "#202321".into(),
        align: TextAlign::Center,
        letter_spacing_ratio: 0.0,
        line_height: 1.2,
        stroke_color: "#00000000".into(),
        stroke_width_ratio: 0.0,
        shadow_color: "#00000000".into(),
        shadow_blur_ratio: 0.0,
        shadow_offset_x_ratio: 0.0,
        shadow_offset_y_ratio: 0.0,
    });
    for variant in exif.variants.values_mut() {
        variant.frame.bottom = 0.18;
        variant.layer_layouts.insert(
            "exif".into(),
            placement(
                WatermarkAnchorSpace::Frame,
                Some(WatermarkFrameEdge::Bottom),
                0.5,
                0.5,
                0.82,
                Some(0.035),
            ),
        );
    }

    let mut dark = default_template(BUILTIN_TEMPLATE_IDS[2].0, BUILTIN_TEMPLATE_IDS[2].1);
    for variant in dark.variants.values_mut() {
        variant.frame.top = 0.08;
        variant.frame.right = 0.08;
        variant.frame.bottom = 0.18;
        variant.frame.left = 0.08;
        variant.background = WatermarkBackground::Solid {
            color: "#171a18".into(),
            opacity: 1.0,
        };
        variant.photo.shadow_blur_ratio = 0.04;
        variant.photo.shadow_opacity = 0.55;
    }
    add_layer(
        &mut dark,
        text_layer("gallery-title", "作品标题", "UNTITLED · 2026", "#f4f5f2"),
        placement(
            WatermarkAnchorSpace::Frame,
            Some(WatermarkFrameEdge::Bottom),
            0.5,
            0.5,
            0.7,
            Some(0.042),
        ),
    );

    let mut gradient = default_template(BUILTIN_TEMPLATE_IDS[3].0, BUILTIN_TEMPLATE_IDS[3].1);
    for variant in gradient.variants.values_mut() {
        variant.frame.top = 0.08;
        variant.frame.right = 0.08;
        variant.frame.bottom = 0.22;
        variant.frame.left = 0.08;
        variant.background = WatermarkBackground::LinearGradient {
            angle_deg: 135.0,
            stops: vec![
                GradientStop {
                    offset: 0.0,
                    color: "#f4d35e".into(),
                    opacity: 1.0,
                },
                GradientStop {
                    offset: 1.0,
                    color: "#5ab1bb".into(),
                    opacity: 1.0,
                },
            ],
        };
    }
    add_layer(
        &mut gradient,
        text_layer("magazine-title", "杂志标题", "FRAME / 01", "#17201d"),
        placement(
            WatermarkAnchorSpace::Frame,
            Some(WatermarkFrameEdge::Bottom),
            0.12,
            0.5,
            0.65,
            Some(0.06),
        ),
    );

    let mut blurred = default_template(BUILTIN_TEMPLATE_IDS[4].0, BUILTIN_TEMPLATE_IDS[4].1);
    for variant in blurred.variants.values_mut() {
        variant.canvas_ratio = Some(1.0);
        variant.frame.top = 0.1;
        variant.frame.right = 0.1;
        variant.frame.bottom = 0.1;
        variant.frame.left = 0.1;
        variant.background = WatermarkBackground::BlurredPhoto {
            blur_ratio: 0.08,
            scale: 1.18,
            overlay_color: "#ffffff".into(),
            overlay_opacity: 0.12,
        };
        variant.photo.corner_radius_ratio = 0.02;
        variant.photo.shadow_blur_ratio = 0.04;
        variant.photo.shadow_opacity = 0.4;
    }

    let mut logo = default_template(BUILTIN_TEMPLATE_IDS[5].0, BUILTIN_TEMPLATE_IDS[5].1);
    let resource = logo_resource()?;
    logo.resources.insert(resource.id.clone(), resource);
    add_layer(
        &mut logo,
        WatermarkLayer::Image {
            base: base("logo", "替换为你的 Logo", 0),
            resource_id: "framepair-logo".into(),
            fit: ImageFit::Contain,
        },
        placement(WatermarkAnchorSpace::Photo, None, 0.1, 0.12, 0.12, None),
    );

    let templates = vec![minimal, exif, dark, gradient, blurred, logo];
    for template in &templates {
        validate_portable_template(template)?;
    }
    Ok(templates)
}

fn validate_identifier(value: &str, label: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 100
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(format!("{label}只能包含字母、数字、短横线和下划线"));
    }
    Ok(())
}

fn validate_resource(resource: &EmbeddedTemplateResource) -> Result<(), String> {
    validate_identifier(&resource.id, "资源 ID")?;
    if resource.name.trim().is_empty()
        || resource.name.contains("..")
        || resource.name.contains('/')
        || resource.name.contains('\\')
        || resource.name.contains("://")
    {
        return Err(format!("资源 {} 的名称不安全", resource.id));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&resource.data_base64)
        .map_err(|_| format!("资源 {} 不是有效 Base64", resource.id))?;
    let digest = format!("{:x}", Sha256::digest(&bytes));
    if !digest.eq_ignore_ascii_case(&resource.sha256) {
        return Err(format!("资源 {} 的 SHA-256 不匹配", resource.id));
    }
    let format =
        image::guess_format(&bytes).map_err(|_| format!("资源 {} 无法识别", resource.id))?;
    let expected_mime = match format {
        ImageFormat::Png => "image/png",
        ImageFormat::Jpeg => "image/jpeg",
        _ => return Err(format!("资源 {} 不是 JPG 或 PNG", resource.id)),
    };
    if resource.mime_type != expected_mime {
        return Err(format!("资源 {} 的 MIME 与内容不一致", resource.id));
    }
    let decoded = image::load_from_memory_with_format(&bytes, format)
        .map_err(|error| format!("资源 {} 无法解码：{error}", resource.id))?;
    let (width, height) = (decoded.width(), decoded.height());
    if width != resource.width || height != resource.height {
        return Err(format!("资源 {} 的尺寸与内容不一致", resource.id));
    }
    if width > MAX_RESOURCE_EDGE || height > MAX_RESOURCE_EDGE {
        return Err(format!("资源 {} 单边超过 16384 像素", resource.id));
    }
    if u64::from(width) * u64::from(height) > MAX_RESOURCE_PIXELS {
        return Err(format!("资源 {} 超过 1 亿像素", resource.id));
    }
    Ok(())
}

fn validate_portable_template(template: &WatermarkTemplate) -> Result<(), String> {
    validate_template(template)?;
    validate_identifier(&template.id, "模板 ID")?;
    if template.name.trim().is_empty() || template.name.chars().count() > 100 {
        return Err("模板名称必须为 1 到 100 个字符".into());
    }
    for resource in template.resources.values() {
        validate_resource(resource)?;
    }
    for layer in &template.shared.layers {
        validate_identifier(&layer.base().id, "图层 ID")?;
        if let WatermarkLayer::Image { resource_id, .. } = layer
            && !template.resources.contains_key(resource_id)
        {
            return Err(format!("图片图层引用了缺失资源 {resource_id}"));
        }
    }
    for variant in template.variants.values() {
        if let WatermarkBackground::Image { resource_id, .. } = &variant.background
            && !template.resources.contains_key(resource_id)
        {
            return Err(format!("图片背景引用了缺失资源 {resource_id}"));
        }
    }
    Ok(())
}

fn empty_database() -> WatermarkTemplateDatabase {
    WatermarkTemplateDatabase {
        schema_version: TEMPLATE_DATABASE_VERSION,
        templates: Vec::new(),
    }
}

fn read_database(path: &Path) -> Result<WatermarkTemplateDatabase, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(empty_database()),
        Err(error) => return Err(format!("无法读取水印模板库：{error}")),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("水印模板库不是可信普通文件".into());
    }
    if metadata.len() > MAX_TEMPLATE_FILE_BYTES {
        return Err("水印模板库超过大小上限".into());
    }
    let bytes = fs::read(path).map_err(|error| format!("无法读取水印模板库：{error}"))?;
    let database: WatermarkTemplateDatabase =
        serde_json::from_slice(&bytes).map_err(|error| format!("水印模板库已损坏：{error}"))?;
    if database.schema_version > TEMPLATE_DATABASE_VERSION {
        return Err(format!("不支持水印模板库版本 {}", database.schema_version));
    }
    let mut ids = HashSet::new();
    for template in &database.templates {
        if !template.id.starts_with("local-")
            || builtin_id(&template.id)
            || !ids.insert(&template.id)
        {
            return Err(format!("本地模板 ID 无效或重复：{}", template.id));
        }
        validate_portable_template(template)?;
    }
    Ok(WatermarkTemplateDatabase {
        schema_version: TEMPLATE_DATABASE_VERSION,
        templates: database.templates,
    })
}

fn atomic_write(path: &Path, bytes: &[u8], export: bool) -> Result<(), String> {
    if bytes.len() as u64 > MAX_TEMPLATE_FILE_BYTES {
        return Err("水印模板文件超过大小上限".into());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "无法确定水印模板目录".to_string())?;
    if export {
        let metadata = fs::symlink_metadata(parent)
            .map_err(|error| format!("水印模板导出目录不可访问：{error}"))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("水印模板导出目录不是可信文件夹".into());
        }
    } else {
        fs::create_dir_all(parent).map_err(|error| format!("无法创建水印模板目录：{error}"))?;
    }
    let exists = match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err("水印模板目标不是可信普通文件".into());
            }
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(format!("无法检查水印模板目标：{error}")),
    };
    let mut temporary = NamedTempFile::new_in(parent)
        .map_err(|error| format!("无法创建水印模板临时文件：{error}"))?;
    temporary
        .write_all(bytes)
        .and_then(|_| temporary.as_file_mut().sync_all())
        .map_err(|error| format!("无法写入水印模板临时文件：{error}"))?;
    if exists {
        temporary
            .persist(path)
            .map_err(|error| format!("无法替换水印模板：{}", error.error))?;
    } else {
        temporary
            .persist_noclobber(path)
            .map_err(|error| format!("无法保存水印模板：{}", error.error))?;
    }
    #[cfg(unix)]
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("水印模板已写入，但目录同步失败：{error}"))?;
    Ok(())
}

fn write_database(path: &Path, database: &WatermarkTemplateDatabase) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(database)
        .map_err(|error| format!("无法序列化水印模板库：{error}"))?;
    atomic_write(path, &bytes, false)
}

fn new_local_id(template: &WatermarkTemplate) -> Result<String, String> {
    let mut hasher = Sha256::new();
    hasher
        .update(serde_json::to_vec(template).map_err(|error| format!("无法生成模板 ID：{error}"))?);
    hasher.update(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "系统时间早于 Unix 纪元".to_string())?
            .as_nanos()
            .to_le_bytes(),
    );
    Ok(format!("local-{:x}", hasher.finalize())[..22].to_string())
}

pub(crate) fn list_templates(path: &Path) -> Result<Vec<WatermarkTemplateEntry>, String> {
    let mut entries = built_in_templates()?
        .into_iter()
        .map(|template| WatermarkTemplateEntry {
            template,
            built_in: true,
        })
        .collect::<Vec<_>>();
    let mut local = read_database(path)?.templates;
    local.sort_by(|left, right| left.name.cmp(&right.name));
    entries.extend(local.into_iter().map(|template| WatermarkTemplateEntry {
        template,
        built_in: false,
    }));
    Ok(entries)
}

pub(crate) fn save_template(
    path: &Path,
    mut template: WatermarkTemplate,
    save_as: bool,
) -> Result<WatermarkTemplateEntry, String> {
    validate_portable_template(&template)?;
    let mut database = read_database(path)?;
    if builtin_id(&template.id) {
        if !save_as {
            return Err("内置模板不能覆盖，请使用另存为".into());
        }
        template.id = new_local_id(&template)?;
    } else if save_as {
        template.id = new_local_id(&template)?;
    } else if !template.id.starts_with("local-") {
        return Err("本地模板 ID 无效，请使用另存为".into());
    }
    validate_portable_template(&template)?;
    if let Some(existing) = database
        .templates
        .iter_mut()
        .find(|existing| existing.id == template.id)
    {
        *existing = template.clone();
    } else {
        database.templates.push(template.clone());
    }
    write_database(path, &database)?;
    Ok(WatermarkTemplateEntry {
        template,
        built_in: false,
    })
}

pub(crate) fn delete_template(path: &Path, id: &str) -> Result<(), String> {
    if builtin_id(id) {
        return Err("内置模板不能删除".into());
    }
    let mut database = read_database(path)?;
    let before = database.templates.len();
    database.templates.retain(|template| template.id != id);
    if database.templates.len() == before {
        return Err("未找到要删除的本地模板".into());
    }
    write_database(path, &database)
}

fn require_json_file(path: &Path, must_exist: bool) -> Result<(), String> {
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_none_or(|extension| !extension.eq_ignore_ascii_case("json"))
    {
        return Err("水印模板文件必须使用 .json 扩展名".into());
    }
    if must_exist {
        let metadata =
            fs::symlink_metadata(path).map_err(|error| format!("无法读取水印模板文件：{error}"))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err("水印模板文件不是可信普通文件".into());
        }
        if metadata.len() > MAX_TEMPLATE_FILE_BYTES {
            return Err("水印模板文件超过大小上限".into());
        }
    }
    Ok(())
}

pub(crate) fn export_template(path: &Path, template: &WatermarkTemplate) -> Result<(), String> {
    require_json_file(path, false)?;
    validate_portable_template(template)?;
    let bytes = serde_json::to_vec_pretty(&WatermarkTemplateFile {
        schema_version: TEMPLATE_FILE_VERSION,
        template: template.clone(),
    })
    .map_err(|error| format!("无法序列化水印模板：{error}"))?;
    atomic_write(path, &bytes, true)
}

fn read_template_file(path: &Path) -> Result<WatermarkTemplateFile, String> {
    require_json_file(path, true)?;
    let bytes = fs::read(path).map_err(|error| format!("无法读取水印模板文件：{error}"))?;
    let mut value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|error| format!("水印模板文件已损坏：{error}"))?;
    let schema_version = value
        .get("schemaVersion")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "水印模板文件缺少版本".to_string())?;
    if schema_version > u64::from(TEMPLATE_FILE_VERSION) {
        return Err(format!("不支持水印模板文件版本 {schema_version}"));
    }
    if schema_version == 0 {
        value["schemaVersion"] = serde_json::json!(TEMPLATE_FILE_VERSION);
        value["template"]["schemaVersion"] = serde_json::json!(WATERMARK_SCHEMA_VERSION);
    }
    let file: WatermarkTemplateFile =
        serde_json::from_value(value).map_err(|error| format!("水印模板文件格式无效：{error}"))?;
    validate_portable_template(&file.template)?;
    Ok(file)
}

pub(crate) fn import_template(
    database_path: &Path,
    import_path: &Path,
) -> Result<WatermarkTemplateEntry, String> {
    let mut file = read_template_file(import_path)?;
    file.template.id = new_local_id(&file.template)?;
    save_template(database_path, file.template, false)
}
