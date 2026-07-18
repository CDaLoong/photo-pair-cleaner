use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const WATERMARK_SCHEMA_VERSION: u16 = 1;
pub(crate) const MAX_LAYERS: usize = 64;
pub(crate) const MAX_RESOURCE_BYTES: usize = 32 * 1024 * 1024;
pub(crate) const MAX_TEMPLATE_RESOURCE_BYTES: usize = 128 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum WatermarkOrientation {
    Landscape,
    Portrait,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum WatermarkAnchorSpace {
    Photo,
    Frame,
    Canvas,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum WatermarkFrameEdge {
    Top,
    Right,
    Bottom,
    Left,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum WatermarkOutputFormat {
    Jpeg,
    Png,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum MetadataPolicy {
    Preserve,
    Privacy,
    Remove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum CollisionPolicy {
    Sequence,
    Skip,
    OverwriteOutput,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NormalizedPlacement {
    pub(crate) anchor_space: WatermarkAnchorSpace,
    pub(crate) frame_edge: Option<WatermarkFrameEdge>,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) rotation_deg: f32,
    pub(crate) opacity: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WatermarkLayerBase {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) z_index: i32,
    pub(crate) visible: bool,
    pub(crate) locked: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub(crate) enum WatermarkLayer {
    Text {
        #[serde(flatten)]
        base: WatermarkLayerBase,
        text: String,
        font_family: String,
        font_weight: u16,
        color: String,
        align: TextAlign,
        letter_spacing_ratio: f32,
        line_height: f32,
        stroke_color: String,
        stroke_width_ratio: f32,
        shadow_color: String,
        shadow_blur_ratio: f32,
        shadow_offset_x_ratio: f32,
        shadow_offset_y_ratio: f32,
    },
    ExifText {
        #[serde(flatten)]
        base: WatermarkLayerBase,
        fields: Vec<String>,
        separator: String,
        prefix: String,
        suffix: String,
        missing_value: Option<String>,
        font_family: String,
        font_weight: u16,
        color: String,
        align: TextAlign,
        letter_spacing_ratio: f32,
        line_height: f32,
        stroke_color: String,
        stroke_width_ratio: f32,
        shadow_color: String,
        shadow_blur_ratio: f32,
        shadow_offset_x_ratio: f32,
        shadow_offset_y_ratio: f32,
    },
    Image {
        #[serde(flatten)]
        base: WatermarkLayerBase,
        resource_id: String,
        fit: ImageFit,
    },
}

impl WatermarkLayer {
    pub(crate) fn base(&self) -> &WatermarkLayerBase {
        match self {
            Self::Text { base, .. } | Self::ExifText { base, .. } | Self::Image { base, .. } => {
                base
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ImageFit {
    Contain,
    Cover,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct FrameInsets {
    pub(crate) top: f32,
    pub(crate) right: f32,
    pub(crate) bottom: f32,
    pub(crate) left: f32,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GradientStop {
    pub(crate) offset: f32,
    pub(crate) color: String,
    pub(crate) opacity: f32,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub(crate) enum WatermarkBackground {
    Transparent,
    Solid {
        color: String,
        opacity: f32,
    },
    Sampled {
        x: f32,
        y: f32,
        color: String,
        sample_each_photo: bool,
    },
    LinearGradient {
        angle_deg: f32,
        stops: Vec<GradientStop>,
    },
    RadialGradient {
        center_x: f32,
        center_y: f32,
        radius: f32,
        stops: Vec<GradientStop>,
    },
    BlurredPhoto {
        blur_ratio: f32,
        scale: f32,
        overlay_color: String,
        overlay_opacity: f32,
    },
    Image {
        resource_id: String,
        fit: ImageFit,
        opacity: f32,
    },
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PhotoStyle {
    pub(crate) align_x: f32,
    pub(crate) align_y: f32,
    pub(crate) scale: f32,
    pub(crate) corner_radius_ratio: f32,
    pub(crate) stroke_width_ratio: f32,
    pub(crate) stroke_color: String,
    pub(crate) shadow_blur_ratio: f32,
    pub(crate) shadow_opacity: f32,
    pub(crate) shadow_offset_x_ratio: f32,
    pub(crate) shadow_offset_y_ratio: f32,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct VariantLayerLayout {
    pub(crate) placement: NormalizedPlacement,
    pub(crate) font_size_ratio: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct LayoutVariant {
    pub(crate) canvas_ratio: Option<f32>,
    pub(crate) frame: FrameInsets,
    pub(crate) background: WatermarkBackground,
    pub(crate) photo: PhotoStyle,
    pub(crate) layer_layouts: BTreeMap<String, VariantLayerLayout>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EmbeddedTemplateResource {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) mime_type: String,
    pub(crate) sha256: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) data_base64: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WatermarkTemplateShared {
    pub(crate) layers: Vec<WatermarkLayer>,
    pub(crate) palette: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WatermarkTemplate {
    pub(crate) schema_version: u16,
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) shared: WatermarkTemplateShared,
    pub(crate) variants: BTreeMap<String, LayoutVariant>,
    pub(crate) resources: BTreeMap<String, EmbeddedTemplateResource>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum WatermarkSourceOrigin {
    Directory,
    Drop,
    PreviewPhoto,
    PreviewDirectory,
    PreviewFilter,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WatermarkSourcePhoto {
    pub(crate) id: String,
    pub(crate) root: String,
    pub(crate) relative_path: String,
    pub(crate) file_name: String,
    pub(crate) size_bytes: u64,
    pub(crate) modified_ms: u64,
    pub(crate) pixel_width: u32,
    pub(crate) pixel_height: u32,
    pub(crate) orientation: WatermarkOrientation,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WatermarkSourceSnapshot {
    pub(crate) id: String,
    pub(crate) created_at_ms: u64,
    pub(crate) origin: WatermarkSourceOrigin,
    pub(crate) root_paths: Vec<String>,
    pub(crate) photos: Vec<WatermarkSourcePhoto>,
    pub(crate) skipped_raw_only: usize,
    pub(crate) skipped_unsupported: usize,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PhotoPlacementOverride {
    pub(crate) align_x: f32,
    pub(crate) align_y: f32,
    pub(crate) scale: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub(crate) enum WatermarkSizing {
    Original { allow_upscale: bool },
    LongEdge { pixels: u32, allow_upscale: bool },
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WatermarkOutputSettings {
    pub(crate) format: WatermarkOutputFormat,
    pub(crate) jpeg_quality: u8,
    pub(crate) sizing: WatermarkSizing,
    pub(crate) color_space: OutputColorSpace,
    pub(crate) transparent_background: bool,
    pub(crate) jpeg_flatten_color: String,
    pub(crate) metadata_policy: MetadataPolicy,
    pub(crate) output_directory: Option<String>,
    pub(crate) suffix: String,
    pub(crate) collision_policy: CollisionPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum OutputColorSpace {
    Srgb,
    Preserve,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WatermarkRenderRequest {
    pub(crate) schema_version: u16,
    pub(crate) source: WatermarkSourcePhoto,
    pub(crate) template: WatermarkTemplate,
    pub(crate) photo_override: Option<PhotoPlacementOverride>,
    pub(crate) color_space: OutputColorSpace,
    pub(crate) transparent_background: bool,
    pub(crate) jpeg_flatten_color: String,
}

pub(crate) fn normalized(value: f32, label: &str) -> Result<(), String> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(format!("{label}必须在 0 到 1 之间"));
    }
    Ok(())
}

fn finite_between(value: f32, minimum: f32, maximum: f32, label: &str) -> Result<(), String> {
    if !value.is_finite() || value < minimum || value > maximum {
        return Err(format!("{label}必须在 {minimum} 到 {maximum} 之间"));
    }
    Ok(())
}

fn validate_layer_layout(
    layer: &WatermarkLayer,
    layout: &VariantLayerLayout,
) -> Result<(), String> {
    finite_between(layout.placement.x, -1.0, 2.0, "图层 X")?;
    finite_between(layout.placement.y, -1.0, 2.0, "图层 Y")?;
    finite_between(layout.placement.rotation_deg, -360.0, 360.0, "图层角度")?;
    for (label, value) in [
        ("图层宽度", layout.placement.width),
        ("图层透明度", layout.placement.opacity),
    ] {
        normalized(value, label)?;
    }
    match (layer, layout.font_size_ratio) {
        (WatermarkLayer::Text { .. } | WatermarkLayer::ExifText { .. }, Some(value)) => {
            normalized(value, "文字字号")
        }
        (WatermarkLayer::Image { .. }, None) => Ok(()),
        (WatermarkLayer::Text { .. } | WatermarkLayer::ExifText { .. }, None) => {
            Err("文字图层必须设置当前方向字号".to_string())
        }
        (WatermarkLayer::Image { .. }, Some(_)) => Err("图片图层不能设置文字字号".to_string()),
    }
}

fn validate_background(background: &WatermarkBackground) -> Result<(), String> {
    match background {
        WatermarkBackground::Transparent => Ok(()),
        WatermarkBackground::Solid { opacity, .. } | WatermarkBackground::Image { opacity, .. } => {
            normalized(*opacity, "背景透明度")
        }
        WatermarkBackground::Sampled { x, y, .. } => {
            normalized(*x, "背景采样 X")?;
            normalized(*y, "背景采样 Y")
        }
        WatermarkBackground::LinearGradient { angle_deg, stops } => {
            finite_between(*angle_deg, -360.0, 360.0, "渐变角度")?;
            validate_gradient_stops(stops)
        }
        WatermarkBackground::RadialGradient {
            center_x,
            center_y,
            radius,
            stops,
        } => {
            normalized(*center_x, "径向渐变中心 X")?;
            normalized(*center_y, "径向渐变中心 Y")?;
            normalized(*radius, "径向渐变半径")?;
            validate_gradient_stops(stops)
        }
        WatermarkBackground::BlurredPhoto {
            blur_ratio,
            scale,
            overlay_opacity,
            ..
        } => {
            normalized(*blur_ratio, "背景模糊")?;
            finite_between(*scale, 0.01, 8.0, "背景缩放")?;
            normalized(*overlay_opacity, "背景叠色透明度")
        }
    }
}

fn validate_gradient_stops(stops: &[GradientStop]) -> Result<(), String> {
    if stops.len() < 2 || stops.len() > 16 {
        return Err("渐变色标数量必须在 2 到 16 之间".to_string());
    }
    let mut previous = -1.0_f32;
    for stop in stops {
        normalized(stop.offset, "渐变色标位置")?;
        normalized(stop.opacity, "渐变色标透明度")?;
        if stop.offset < previous {
            return Err("渐变色标必须按位置升序排列".to_string());
        }
        previous = stop.offset;
    }
    Ok(())
}

pub(crate) fn validate_template(template: &WatermarkTemplate) -> Result<(), String> {
    if template.schema_version != WATERMARK_SCHEMA_VERSION {
        return Err(format!("不支持水印模板版本 {}", template.schema_version));
    }
    if template.id.trim().is_empty() || template.name.trim().is_empty() {
        return Err("模板 ID 和名称不能为空".to_string());
    }
    if template.shared.layers.len() > MAX_LAYERS {
        return Err(format!("模板图层不能超过 {MAX_LAYERS} 个"));
    }
    if template.variants.len() != 3 {
        return Err("模板只能包含横版、竖版和方形三种布局".to_string());
    }

    let mut layer_ids = BTreeSet::new();
    for layer in &template.shared.layers {
        let base = layer.base();
        if base.id.trim().is_empty() || base.name.trim().is_empty() {
            return Err("图层 ID 和名称不能为空".to_string());
        }
        if !layer_ids.insert(base.id.as_str()) {
            return Err(format!("图层 ID {} 重复", base.id));
        }
    }

    for orientation in ["landscape", "portrait", "square"] {
        let variant = template
            .variants
            .get(orientation)
            .ok_or_else(|| format!("模板缺少 {orientation} 布局"))?;
        if let Some(ratio) = variant.canvas_ratio {
            finite_between(ratio, 0.05, 20.0, "画布比例")?;
        }
        for (label, value) in [
            ("上边框", variant.frame.top),
            ("右边框", variant.frame.right),
            ("下边框", variant.frame.bottom),
            ("左边框", variant.frame.left),
        ] {
            normalized(value, label)?;
        }
        for (label, value) in [
            ("照片水平位置", variant.photo.align_x),
            ("照片垂直位置", variant.photo.align_y),
            ("照片圆角", variant.photo.corner_radius_ratio),
            ("照片描边", variant.photo.stroke_width_ratio),
            ("照片阴影模糊", variant.photo.shadow_blur_ratio),
            ("照片阴影透明度", variant.photo.shadow_opacity),
        ] {
            normalized(value, label)?;
        }
        finite_between(variant.photo.scale, 0.01, 8.0, "照片缩放")?;
        finite_between(variant.photo.shadow_offset_x_ratio, -1.0, 1.0, "照片阴影 X")?;
        finite_between(variant.photo.shadow_offset_y_ratio, -1.0, 1.0, "照片阴影 Y")?;
        validate_background(&variant.background)?;
        if variant.layer_layouts.len() != template.shared.layers.len() {
            return Err(format!("{orientation} 布局的图层位置数量不一致"));
        }
        for layer in &template.shared.layers {
            let base = layer.base();
            let layout = variant
                .layer_layouts
                .get(&base.id)
                .ok_or_else(|| format!("{orientation} 布局缺少图层 {}", base.name))?;
            validate_layer_layout(layer, layout)?;
        }
    }

    let maximum_encoded_resource = MAX_RESOURCE_BYTES.div_ceil(3) * 4 + 4;
    let total = template
        .resources
        .iter()
        .try_fold(0usize, |sum, (key, resource)| {
            if key != &resource.id || resource.id.trim().is_empty() {
                return Err("模板资源 ID 与索引不一致".to_string());
            }
            if !matches!(resource.mime_type.as_str(), "image/png" | "image/jpeg") {
                return Err(format!("资源 {} 不是 JPG 或 PNG", resource.id));
            }
            if resource.width == 0 || resource.height == 0 {
                return Err(format!("资源 {} 的尺寸无效", resource.id));
            }
            if resource.data_base64.len() > maximum_encoded_resource {
                return Err(format!("资源 {} 超过 32 MiB", resource.id));
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(&resource.data_base64)
                .map_err(|_| format!("资源 {} 不是有效 Base64", resource.id))?;
            if bytes.len() > MAX_RESOURCE_BYTES {
                return Err(format!("资源 {} 超过 32 MiB", resource.id));
            }
            sum.checked_add(bytes.len())
                .ok_or_else(|| "模板资源大小溢出".to_string())
        })?;
    if total > MAX_TEMPLATE_RESOURCE_BYTES {
        return Err("模板资源总量超过 128 MiB".to_string());
    }
    Ok(())
}

fn default_variant() -> LayoutVariant {
    LayoutVariant {
        canvas_ratio: None,
        frame: FrameInsets {
            top: 0.04,
            right: 0.04,
            bottom: 0.14,
            left: 0.04,
        },
        background: WatermarkBackground::Solid {
            color: "#ffffff".to_string(),
            opacity: 1.0,
        },
        photo: PhotoStyle {
            align_x: 0.5,
            align_y: 0.5,
            scale: 1.0,
            corner_radius_ratio: 0.0,
            stroke_width_ratio: 0.0,
            stroke_color: "#ffffff".to_string(),
            shadow_blur_ratio: 0.0,
            shadow_opacity: 0.0,
            shadow_offset_x_ratio: 0.0,
            shadow_offset_y_ratio: 0.0,
        },
        layer_layouts: BTreeMap::new(),
    }
}

pub(crate) fn default_template(id: &str, name: &str) -> WatermarkTemplate {
    WatermarkTemplate {
        schema_version: WATERMARK_SCHEMA_VERSION,
        id: id.to_string(),
        name: name.to_string(),
        shared: WatermarkTemplateShared {
            layers: Vec::new(),
            palette: vec!["#ffffff".to_string(), "#111111".to_string()],
        },
        variants: ["landscape", "portrait", "square"]
            .into_iter()
            .map(|orientation| (orientation.to_string(), default_variant()))
            .collect(),
        resources: BTreeMap::new(),
    }
}
