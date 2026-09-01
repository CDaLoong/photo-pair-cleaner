use crate::watermark_model::{FrameInsets, NormalizedPlacement, normalized};
pub(crate) use crate::watermark_model::{
    WatermarkAnchorSpace as AnchorSpace, WatermarkFrameEdge as FrameEdge,
};

const MAX_CANVAS_EDGE: u32 = 32_768;
const MAX_EXPORT_PIXELS: u64 = 200_000_000;
const MAX_PREVIEW_PIXELS: u64 = 16_000_000;

#[derive(Debug, Clone)]
pub(crate) struct ResolvedLayoutInput {
    pub(crate) photo_width: u32,
    pub(crate) photo_height: u32,
    pub(crate) output_long_edge: Option<u32>,
    pub(crate) canvas_ratio: Option<f32>,
    pub(crate) frame: FrameInsets,
    pub(crate) align_x: f32,
    pub(crate) align_y: f32,
    pub(crate) photo_scale: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PixelSize {
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PixelRect {
    pub(crate) x: i64,
    pub(crate) y: i64,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl PixelRect {
    pub(crate) fn right(self) -> i64 {
        self.x.saturating_add(i64::from(self.width))
    }

    pub(crate) fn bottom(self) -> i64 {
        self.y.saturating_add(i64::from(self.height))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResolvedFrameInsets {
    pub(crate) top: u32,
    pub(crate) right: u32,
    pub(crate) bottom: u32,
    pub(crate) left: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedLayout {
    pub(crate) canvas: PixelSize,
    pub(crate) photo_rect: PixelRect,
    pub(crate) frame: ResolvedFrameInsets,
    pub(crate) scaled_photo: PixelSize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ResolvedLayerPlacement {
    pub(crate) center_x: i64,
    pub(crate) center_y: i64,
    pub(crate) width: u32,
    pub(crate) rotation_deg: f32,
    pub(crate) opacity: f32,
}

fn finite_between(value: f32, minimum: f32, maximum: f32, label: &str) -> Result<(), String> {
    if !value.is_finite() || value < minimum || value > maximum {
        return Err(format!("{label}必须在 {minimum} 到 {maximum} 之间"));
    }
    Ok(())
}

fn checked_dimension(value: f64, label: &str) -> Result<u32, String> {
    if !value.is_finite() || value < 1.0 || value > u32::MAX as f64 {
        return Err(format!("{label}超出可处理范围"));
    }
    Ok(value.round() as u32)
}

fn scaled_photo_size(input: &ResolvedLayoutInput) -> Result<PixelSize, String> {
    if input.photo_width == 0 || input.photo_height == 0 {
        return Err("照片尺寸必须大于 0".to_string());
    }
    let Some(output_long_edge) = input.output_long_edge else {
        return Ok(PixelSize {
            width: input.photo_width,
            height: input.photo_height,
        });
    };
    if output_long_edge == 0 {
        return Err("输出长边必须大于 0".to_string());
    }
    let source_long_edge = input.photo_width.max(input.photo_height);
    let ratio = f64::from(output_long_edge) / f64::from(source_long_edge);
    Ok(PixelSize {
        width: checked_dimension(f64::from(input.photo_width) * ratio, "缩放照片宽度")?,
        height: checked_dimension(f64::from(input.photo_height) * ratio, "缩放照片高度")?,
    })
}

fn inset_pixels(short_edge: u32, ratio: f32, label: &str) -> Result<u32, String> {
    normalized(ratio, label)?;
    let pixels = f64::from(short_edge) * f64::from(ratio);
    if !pixels.is_finite() || pixels > u32::MAX as f64 {
        return Err(format!("{label}尺寸超出可处理范围"));
    }
    Ok(pixels.round() as u32)
}

fn checked_sum(left: u32, middle: u32, right: u32, label: &str) -> Result<u32, String> {
    let value = u64::from(left)
        .checked_add(u64::from(middle))
        .and_then(|sum| sum.checked_add(u64::from(right)))
        .ok_or_else(|| format!("{label}尺寸溢出"))?;
    u32::try_from(value).map_err(|_| format!("{label}尺寸超出可处理范围"))
}

fn fixed_ratio_canvas(
    natural_width: u32,
    natural_height: u32,
    ratio: Option<f32>,
) -> Result<PixelSize, String> {
    let Some(ratio) = ratio else {
        return Ok(PixelSize {
            width: natural_width,
            height: natural_height,
        });
    };
    finite_between(ratio, 0.05, 20.0, "画布比例")?;
    let ratio = f64::from(ratio);
    let natural_ratio = f64::from(natural_width) / f64::from(natural_height);
    if natural_ratio < ratio {
        Ok(PixelSize {
            width: checked_dimension((f64::from(natural_height) * ratio).ceil(), "画布宽度")?,
            height: natural_height,
        })
    } else {
        Ok(PixelSize {
            width: natural_width,
            height: checked_dimension((f64::from(natural_width) / ratio).ceil(), "画布高度")?,
        })
    }
}

fn validate_canvas(canvas: PixelSize, max_pixels: u64, limit_label: &str) -> Result<(), String> {
    if canvas.width == 0 || canvas.height == 0 {
        return Err("画布尺寸必须大于 0".to_string());
    }
    if canvas.width > MAX_CANVAS_EDGE || canvas.height > MAX_CANVAS_EDGE {
        return Err(format!("画布单边不能超过 {MAX_CANVAS_EDGE} 像素"));
    }
    let pixels = u64::from(canvas.width)
        .checked_mul(u64::from(canvas.height))
        .ok_or_else(|| "画布像素数量溢出".to_string())?;
    if pixels > max_pixels {
        return Err(format!(
            "{limit_label}画布不能超过 {} 像素",
            if max_pixels == MAX_PREVIEW_PIXELS {
                "1600 万"
            } else {
                "2 亿"
            }
        ));
    }
    Ok(())
}

fn aligned_origin(start: u32, available: u32, content: u32, align: f32) -> Result<i64, String> {
    normalized(align, "照片对齐")?;
    let remaining = i64::from(available) - i64::from(content);
    let offset = (remaining as f64 * f64::from(align)).round();
    if !offset.is_finite() || offset < i64::MIN as f64 || offset > i64::MAX as f64 {
        return Err("照片对齐位置超出可处理范围".to_string());
    }
    Ok(i64::from(start).saturating_add(offset as i64))
}

fn resolve_layout_with_limit(
    input: ResolvedLayoutInput,
    max_pixels: u64,
    limit_label: &str,
) -> Result<ResolvedLayout, String> {
    let scaled_photo = scaled_photo_size(&input)?;
    let short_edge = scaled_photo.width.min(scaled_photo.height);
    let frame = ResolvedFrameInsets {
        top: inset_pixels(short_edge, input.frame.top, "上边框")?,
        right: inset_pixels(short_edge, input.frame.right, "右边框")?,
        bottom: inset_pixels(short_edge, input.frame.bottom, "下边框")?,
        left: inset_pixels(short_edge, input.frame.left, "左边框")?,
    };
    let natural_width = checked_sum(frame.left, scaled_photo.width, frame.right, "自然画布宽度")?;
    let natural_height = checked_sum(frame.top, scaled_photo.height, frame.bottom, "自然画布高度")?;
    let canvas = fixed_ratio_canvas(natural_width, natural_height, input.canvas_ratio)?;
    validate_canvas(canvas, max_pixels, limit_label)?;

    finite_between(input.photo_scale, 0.01, 8.0, "照片缩放")?;
    let photo_width = checked_dimension(
        f64::from(scaled_photo.width) * f64::from(input.photo_scale),
        "照片显示宽度",
    )?;
    let photo_height = checked_dimension(
        f64::from(scaled_photo.height) * f64::from(input.photo_scale),
        "照片显示高度",
    )?;
    if photo_width > MAX_CANVAS_EDGE || photo_height > MAX_CANVAS_EDGE {
        return Err(format!("照片显示单边不能超过 {MAX_CANVAS_EDGE} 像素"));
    }
    let available_width = canvas
        .width
        .checked_sub(frame.left)
        .and_then(|value| value.checked_sub(frame.right))
        .ok_or_else(|| "左右边框超过画布宽度".to_string())?;
    let available_height = canvas
        .height
        .checked_sub(frame.top)
        .and_then(|value| value.checked_sub(frame.bottom))
        .ok_or_else(|| "上下边框超过画布高度".to_string())?;

    Ok(ResolvedLayout {
        canvas,
        photo_rect: PixelRect {
            x: aligned_origin(frame.left, available_width, photo_width, input.align_x)?,
            y: aligned_origin(frame.top, available_height, photo_height, input.align_y)?,
            width: photo_width,
            height: photo_height,
        },
        frame,
        scaled_photo,
    })
}

pub(crate) fn resolve_layout(input: ResolvedLayoutInput) -> Result<ResolvedLayout, String> {
    resolve_layout_with_limit(input, MAX_EXPORT_PIXELS, "导出")
}

pub(crate) fn resolve_preview_layout(input: ResolvedLayoutInput) -> Result<ResolvedLayout, String> {
    resolve_layout_with_limit(input, MAX_PREVIEW_PIXELS, "预览")
}

pub(crate) fn anchor_region(
    layout: &ResolvedLayout,
    space: AnchorSpace,
    edge: Option<FrameEdge>,
) -> Result<PixelRect, String> {
    let canvas_width = i64::from(layout.canvas.width);
    let canvas_height = i64::from(layout.canvas.height);
    let region = match space {
        AnchorSpace::Photo => layout.photo_rect,
        AnchorSpace::Canvas => PixelRect {
            x: 0,
            y: 0,
            width: layout.canvas.width,
            height: layout.canvas.height,
        },
        AnchorSpace::Frame => match edge.ok_or_else(|| "边框锚点必须指定边缘".to_string())?
        {
            FrameEdge::Top => {
                let bottom = layout.photo_rect.y.clamp(0, canvas_height);
                PixelRect {
                    x: 0,
                    y: 0,
                    width: layout.canvas.width,
                    height: bottom as u32,
                }
            }
            FrameEdge::Right => {
                let left = layout.photo_rect.right().clamp(0, canvas_width);
                PixelRect {
                    x: left,
                    y: 0,
                    width: (canvas_width - left) as u32,
                    height: layout.canvas.height,
                }
            }
            FrameEdge::Bottom => {
                let top = layout.photo_rect.bottom().clamp(0, canvas_height);
                PixelRect {
                    x: 0,
                    y: top,
                    width: layout.canvas.width,
                    height: (canvas_height - top) as u32,
                }
            }
            FrameEdge::Left => {
                let right = layout.photo_rect.x.clamp(0, canvas_width);
                PixelRect {
                    x: 0,
                    y: 0,
                    width: right as u32,
                    height: layout.canvas.height,
                }
            }
        },
    };
    if region.width == 0 || region.height == 0 {
        return Err("当前锚定区域没有可用空间".to_string());
    }
    Ok(region)
}

pub(crate) fn normalized_placement(
    region: PixelRect,
    placement: &NormalizedPlacement,
) -> Result<ResolvedLayerPlacement, String> {
    if region.width == 0 || region.height == 0 {
        return Err("图层锚定区域不能为空".to_string());
    }
    finite_between(placement.x, -1.0, 2.0, "图层 X")?;
    finite_between(placement.y, -1.0, 2.0, "图层 Y")?;
    normalized(placement.width, "图层宽度")?;
    normalized(placement.opacity, "图层透明度")?;
    finite_between(placement.rotation_deg, -360.0, 360.0, "图层角度")?;
    let width = (f64::from(region.width) * f64::from(placement.width)).round();
    if width < 1.0 {
        return Err("图层宽度至少为 1 像素".to_string());
    }
    Ok(ResolvedLayerPlacement {
        center_x: region
            .x
            .saturating_add((f64::from(region.width) * f64::from(placement.x)).round() as i64),
        center_y: region
            .y
            .saturating_add((f64::from(region.height) * f64::from(placement.y)).round() as i64),
        width: width as u32,
        rotation_deg: placement.rotation_deg,
        opacity: placement.opacity,
    })
}

/// 仅被 `tests/watermark_geometry.rs` 使用（该测试通过 `#[path]` 引入本模块）。
/// 渲染器在合成过程中会隐式裁剪，不需要显式调用。
#[allow(dead_code)]
pub(crate) fn clip_rect(rect: PixelRect, canvas: PixelSize) -> Option<PixelRect> {
    let left = rect.x.max(0).min(i64::from(canvas.width));
    let top = rect.y.max(0).min(i64::from(canvas.height));
    let right = rect.right().max(0).min(i64::from(canvas.width));
    let bottom = rect.bottom().max(0).min(i64::from(canvas.height));
    if right <= left || bottom <= top {
        return None;
    }
    Some(PixelRect {
        x: left,
        y: top,
        width: u32::try_from(right - left).ok()?,
        height: u32::try_from(bottom - top).ok()?,
    })
}
