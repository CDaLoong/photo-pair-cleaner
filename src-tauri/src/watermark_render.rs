use crate::watermark_color::{parse_css_color_linear, source_to_linear_srgb};
use crate::watermark_geometry::{
    PixelRect, ResolvedLayout, ResolvedLayoutInput, resolve_layout, resolve_preview_layout,
};
use crate::watermark_model::{
    EmbeddedTemplateResource, GradientStop, ImageFit, LayoutVariant, MAX_RESOURCE_BYTES,
    PhotoPlacementOverride, WatermarkBackground,
};
use base64::Engine;
use image::imageops::{FilterType, resize};
use image::{DynamicImage, ImageDecoder, ImageReader, Rgba, Rgba32FImage};
use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RenderTarget {
    Preview { max_edge: u32 },
    Export { output_long_edge: Option<u32> },
}

pub(crate) struct RenderedCanvas {
    pub(crate) image: Rgba32FImage,
    pub(crate) layout: ResolvedLayout,
    pub(crate) source_icc: Option<Vec<u8>>,
}

struct DecodedImage {
    image: Rgba32FImage,
    source_icc: Option<Vec<u8>>,
}

fn rgba8_to_linear(
    image: image::RgbaImage,
    source_icc: Option<&[u8]>,
) -> Result<Rgba32FImage, String> {
    let mut rgb = Vec::with_capacity(image.len() / 4 * 3);
    for pixel in image.pixels() {
        rgb.extend_from_slice(&pixel.0[..3]);
    }
    let linear = source_to_linear_srgb(&rgb, source_icc)?;
    let mut result = Rgba32FImage::new(image.width(), image.height());
    for ((destination, source), color) in result
        .pixels_mut()
        .zip(image.pixels())
        .zip(linear.chunks_exact(3))
    {
        let alpha = f32::from(source[3]) / 255.0;
        *destination = Rgba([color[0] * alpha, color[1] * alpha, color[2] * alpha, alpha]);
    }
    Ok(result)
}

fn decode_path(path: &Path) -> Result<DecodedImage, String> {
    let reader = ImageReader::open(path)
        .map_err(|error| format!("无法打开水印来源：{error}"))?
        .with_guessed_format()
        .map_err(|error| format!("无法识别水印来源：{error}"))?;
    let mut decoder = reader
        .into_decoder()
        .map_err(|error| format!("无法创建水印来源解码器：{error}"))?;
    let source_icc = decoder
        .icc_profile()
        .map_err(|error| format!("无法读取来源 ICC：{error}"))?;
    let orientation = decoder
        .orientation()
        .map_err(|error| format!("无法读取来源方向：{error}"))?;
    let mut image = DynamicImage::from_decoder(decoder)
        .map_err(|error| format!("无法解码水印来源：{error}"))?;
    image.apply_orientation(orientation);
    Ok(DecodedImage {
        image: rgba8_to_linear(image.to_rgba8(), source_icc.as_deref())?,
        source_icc,
    })
}

fn decode_bytes(bytes: &[u8]) -> Result<DecodedImage, String> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| format!("无法识别模板图片：{error}"))?;
    let mut decoder = reader
        .into_decoder()
        .map_err(|error| format!("无法创建模板图片解码器：{error}"))?;
    let source_icc = decoder
        .icc_profile()
        .map_err(|error| format!("无法读取模板图片 ICC：{error}"))?;
    let orientation = decoder
        .orientation()
        .map_err(|error| format!("无法读取模板图片方向：{error}"))?;
    let mut image = DynamicImage::from_decoder(decoder)
        .map_err(|error| format!("无法解码模板图片：{error}"))?;
    image.apply_orientation(orientation);
    Ok(DecodedImage {
        image: rgba8_to_linear(image.to_rgba8(), source_icc.as_deref())?,
        source_icc,
    })
}

fn premultiplied(color: [f32; 4], opacity: f32) -> [f32; 4] {
    let alpha = (color[3] * opacity).clamp(0.0, 1.0);
    [color[0] * alpha, color[1] * alpha, color[2] * alpha, alpha]
}

pub(crate) fn blend_pixel(destination: &mut Rgba<f32>, source: [f32; 4]) {
    let inverse = 1.0 - source[3].clamp(0.0, 1.0);
    destination[0] = source[0] + destination[0] * inverse;
    destination[1] = source[1] + destination[1] * inverse;
    destination[2] = source[2] + destination[2] * inverse;
    destination[3] = source[3] + destination[3] * inverse;
}

fn fill(canvas: &mut Rgba32FImage, color: [f32; 4]) {
    for pixel in canvas.pixels_mut() {
        *pixel = Rgba(color);
    }
}

fn gradient_stops(stops: &[GradientStop]) -> Result<Vec<(f32, [f32; 4])>, String> {
    if stops.len() < 2 {
        return Err("渐变至少需要两个色标".to_string());
    }
    let mut result = Vec::with_capacity(stops.len());
    let mut previous = -1.0;
    for stop in stops {
        if !stop.offset.is_finite() || !(0.0..=1.0).contains(&stop.offset) || stop.offset < previous
        {
            return Err("渐变色标位置必须按 0 到 1 升序排列".to_string());
        }
        result.push((
            stop.offset,
            premultiplied(parse_css_color_linear(&stop.color)?, stop.opacity),
        ));
        previous = stop.offset;
    }
    Ok(result)
}

fn sample_gradient(stops: &[(f32, [f32; 4])], value: f32) -> [f32; 4] {
    let value = value.clamp(0.0, 1.0);
    if value <= stops[0].0 {
        return stops[0].1;
    }
    for pair in stops.windows(2) {
        if value <= pair[1].0 {
            let distance = (pair[1].0 - pair[0].0).max(f32::EPSILON);
            let amount = (value - pair[0].0) / distance;
            return [
                pair[0].1[0] + (pair[1].1[0] - pair[0].1[0]) * amount,
                pair[0].1[1] + (pair[1].1[1] - pair[0].1[1]) * amount,
                pair[0].1[2] + (pair[1].1[2] - pair[0].1[2]) * amount,
                pair[0].1[3] + (pair[1].1[3] - pair[0].1[3]) * amount,
            ];
        }
    }
    stops[stops.len() - 1].1
}

fn fit_image(
    source: &Rgba32FImage,
    width: u32,
    height: u32,
    fit: ImageFit,
    scale: f32,
) -> Rgba32FImage {
    let width_ratio = width as f64 / source.width() as f64;
    let height_ratio = height as f64 / source.height() as f64;
    let fit_ratio = match fit {
        ImageFit::Contain => width_ratio.min(height_ratio),
        ImageFit::Cover => width_ratio.max(height_ratio),
    } * f64::from(scale.max(0.01));
    let target_width = (source.width() as f64 * fit_ratio).round().max(1.0) as u32;
    let target_height = (source.height() as f64 * fit_ratio).round().max(1.0) as u32;
    let resized = resize(source, target_width, target_height, FilterType::Lanczos3);
    let mut result = Rgba32FImage::new(width, height);
    let offset_x = (i64::from(width) - i64::from(target_width)) / 2;
    let offset_y = (i64::from(height) - i64::from(target_height)) / 2;
    for y in 0..target_height {
        let destination_y = i64::from(y) + offset_y;
        if destination_y < 0 || destination_y >= i64::from(height) {
            continue;
        }
        for x in 0..target_width {
            let destination_x = i64::from(x) + offset_x;
            if destination_x < 0 || destination_x >= i64::from(width) {
                continue;
            }
            *result.get_pixel_mut(destination_x as u32, destination_y as u32) =
                *resized.get_pixel(x, y);
        }
    }
    result
}

fn box_blur(image: &Rgba32FImage, radius: u32) -> Rgba32FImage {
    if radius == 0 {
        return image.clone();
    }
    let radius = radius.min(128);
    let mut horizontal = Rgba32FImage::new(image.width(), image.height());
    for y in 0..image.height() {
        let mut sum = [0.0; 4];
        for sample_x in 0..=radius.min(image.width() - 1) {
            let sample = image.get_pixel(sample_x, y).0;
            for channel in 0..4 {
                sum[channel] += sample[channel];
            }
        }
        for x in 0..image.width() {
            if x > 0 {
                if let Some(remove_x) = x.checked_sub(radius + 1) {
                    let sample = image.get_pixel(remove_x, y).0;
                    for channel in 0..4 {
                        sum[channel] -= sample[channel];
                    }
                }
                let add_x = x.saturating_add(radius);
                if add_x < image.width() {
                    let sample = image.get_pixel(add_x, y).0;
                    for channel in 0..4 {
                        sum[channel] += sample[channel];
                    }
                }
            }
            let left = x.saturating_sub(radius);
            let right = x.saturating_add(radius).min(image.width() - 1);
            let count = (right - left + 1) as f32;
            *horizontal.get_pixel_mut(x, y) = Rgba(sum.map(|value| value / count));
        }
    }
    let mut result = Rgba32FImage::new(image.width(), image.height());
    for x in 0..image.width() {
        let mut sum = [0.0; 4];
        for sample_y in 0..=radius.min(image.height() - 1) {
            let sample = horizontal.get_pixel(x, sample_y).0;
            for channel in 0..4 {
                sum[channel] += sample[channel];
            }
        }
        for y in 0..image.height() {
            if y > 0 {
                if let Some(remove_y) = y.checked_sub(radius + 1) {
                    let sample = horizontal.get_pixel(x, remove_y).0;
                    for channel in 0..4 {
                        sum[channel] -= sample[channel];
                    }
                }
                let add_y = y.saturating_add(radius);
                if add_y < image.height() {
                    let sample = horizontal.get_pixel(x, add_y).0;
                    for channel in 0..4 {
                        sum[channel] += sample[channel];
                    }
                }
            }
            let top = y.saturating_sub(radius);
            let bottom = y.saturating_add(radius).min(image.height() - 1);
            let count = (bottom - top + 1) as f32;
            *result.get_pixel_mut(x, y) = Rgba(sum.map(|value| value / count));
        }
    }
    result
}

fn apply_opacity(image: &mut Rgba32FImage, opacity: f32) {
    let opacity = opacity.clamp(0.0, 1.0);
    for pixel in image.pixels_mut() {
        for channel in 0..4 {
            pixel[channel] *= opacity;
        }
    }
}

fn composite_image(destination: &mut Rgba32FImage, source: &Rgba32FImage) {
    for (destination, source) in destination.pixels_mut().zip(source.pixels()) {
        blend_pixel(destination, source.0);
    }
}

fn render_background(
    canvas: &mut Rgba32FImage,
    background: &WatermarkBackground,
    source: &Rgba32FImage,
    resources: &BTreeMap<String, EmbeddedTemplateResource>,
) -> Result<(), String> {
    match background {
        WatermarkBackground::Transparent => {}
        WatermarkBackground::Solid { color, opacity } => {
            fill(
                canvas,
                premultiplied(parse_css_color_linear(color)?, *opacity),
            );
        }
        WatermarkBackground::Sampled {
            x,
            y,
            color,
            sample_each_photo,
        } => {
            let sampled = if *sample_each_photo {
                let sample_x = (x.clamp(0.0, 1.0) * (source.width() - 1) as f32).round() as u32;
                let sample_y = (y.clamp(0.0, 1.0) * (source.height() - 1) as f32).round() as u32;
                source.get_pixel(sample_x, sample_y).0
            } else {
                premultiplied(parse_css_color_linear(color)?, 1.0)
            };
            fill(canvas, sampled);
        }
        WatermarkBackground::LinearGradient { angle_deg, stops } => {
            let stops = gradient_stops(stops)?;
            let angle = angle_deg.to_radians();
            let direction_x = angle.cos();
            let direction_y = angle.sin();
            let range = (direction_x.abs() + direction_y.abs()).max(f32::EPSILON);
            let canvas_width = canvas.width() as f32;
            let canvas_height = canvas.height() as f32;
            for (x, y, pixel) in canvas.enumerate_pixels_mut() {
                let normalized_x = (x as f32 + 0.5) / canvas_width - 0.5;
                let normalized_y = (y as f32 + 0.5) / canvas_height - 0.5;
                let value = 0.5 + (normalized_x * direction_x + normalized_y * direction_y) / range;
                *pixel = Rgba(sample_gradient(&stops, value));
            }
        }
        WatermarkBackground::RadialGradient {
            center_x,
            center_y,
            radius,
            stops,
        } => {
            if !radius.is_finite() || *radius <= 0.0 {
                return Err("径向渐变半径必须大于 0".to_string());
            }
            let stops = gradient_stops(stops)?;
            let canvas_width = canvas.width() as f32;
            let canvas_height = canvas.height() as f32;
            for (x, y, pixel) in canvas.enumerate_pixels_mut() {
                let normalized_x = (x as f32 + 0.5) / canvas_width;
                let normalized_y = (y as f32 + 0.5) / canvas_height;
                let distance =
                    ((normalized_x - center_x).powi(2) + (normalized_y - center_y).powi(2)).sqrt()
                        / radius;
                *pixel = Rgba(sample_gradient(&stops, distance));
            }
        }
        WatermarkBackground::BlurredPhoto {
            blur_ratio,
            scale,
            overlay_color,
            overlay_opacity,
        } => {
            let mut background = fit_image(
                source,
                canvas.width(),
                canvas.height(),
                ImageFit::Cover,
                *scale,
            );
            let blur = (blur_ratio.clamp(0.0, 1.0) * canvas.width().min(canvas.height()) as f32)
                .round() as u32;
            background = box_blur(&background, blur);
            composite_image(canvas, &background);
            let overlay = premultiplied(parse_css_color_linear(overlay_color)?, *overlay_opacity);
            for pixel in canvas.pixels_mut() {
                blend_pixel(pixel, overlay);
            }
        }
        WatermarkBackground::Image {
            resource_id,
            fit,
            opacity,
        } => {
            let resource = resources
                .get(resource_id)
                .ok_or_else(|| format!("模板背景资源 {resource_id} 不存在"))?;
            if !matches!(resource.mime_type.as_str(), "image/png" | "image/jpeg") {
                return Err(format!("模板背景资源 {resource_id} 不是 JPG 或 PNG"));
            }
            let maximum_encoded_resource = MAX_RESOURCE_BYTES.div_ceil(3) * 4 + 4;
            if resource.data_base64.len() > maximum_encoded_resource {
                return Err(format!("模板背景资源 {resource_id} 超过 32 MiB"));
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(&resource.data_base64)
                .map_err(|_| format!("模板背景资源 {resource_id} 不是有效 Base64"))?;
            if bytes.len() > MAX_RESOURCE_BYTES {
                return Err(format!("模板背景资源 {resource_id} 超过 32 MiB"));
            }
            let decoded = decode_bytes(&bytes)?;
            let mut background =
                fit_image(&decoded.image, canvas.width(), canvas.height(), *fit, 1.0);
            apply_opacity(&mut background, *opacity);
            composite_image(canvas, &background);
        }
    }
    Ok(())
}

fn rounded_contains(x: f32, y: f32, width: f32, height: f32, radius: f32) -> bool {
    if x < 0.0 || y < 0.0 || x >= width || y >= height {
        return false;
    }
    let radius = radius.clamp(0.0, width.min(height) / 2.0);
    if radius <= 0.0 {
        return true;
    }
    let nearest_x = x.clamp(radius, width - radius);
    let nearest_y = y.clamp(radius, height - radius);
    (x - nearest_x).powi(2) + (y - nearest_y).powi(2) <= radius.powi(2)
}

fn scalar_box_blur(mask: &[f32], width: u32, height: u32, radius: u32) -> Vec<f32> {
    if radius == 0 {
        return mask.to_vec();
    }
    let radius = radius.min(128);
    let mut horizontal = vec![0.0; mask.len()];
    for y in 0..height {
        let mut sum = 0.0;
        for sample_x in 0..=radius.min(width - 1) {
            sum += mask[(y * width + sample_x) as usize];
        }
        for x in 0..width {
            if x > 0 {
                if let Some(remove_x) = x.checked_sub(radius + 1) {
                    sum -= mask[(y * width + remove_x) as usize];
                }
                let add_x = x.saturating_add(radius);
                if add_x < width {
                    sum += mask[(y * width + add_x) as usize];
                }
            }
            let left = x.saturating_sub(radius);
            let right = x.saturating_add(radius).min(width - 1);
            horizontal[(y * width + x) as usize] = sum / (right - left + 1) as f32;
        }
    }
    let mut result = vec![0.0; mask.len()];
    for x in 0..width {
        let mut sum = 0.0;
        for sample_y in 0..=radius.min(height - 1) {
            sum += horizontal[(sample_y * width + x) as usize];
        }
        for y in 0..height {
            if y > 0 {
                if let Some(remove_y) = y.checked_sub(radius + 1) {
                    sum -= horizontal[(remove_y * width + x) as usize];
                }
                let add_y = y.saturating_add(radius);
                if add_y < height {
                    sum += horizontal[(add_y * width + x) as usize];
                }
            }
            let top = y.saturating_sub(radius);
            let bottom = y.saturating_add(radius).min(height - 1);
            result[(y * width + x) as usize] = sum / (bottom - top + 1) as f32;
        }
    }
    result
}

fn render_shadow(canvas: &mut Rgba32FImage, rect: PixelRect, variant: &LayoutVariant) {
    if variant.photo.shadow_opacity <= 0.0 {
        return;
    }
    let short_edge = rect.width.min(rect.height) as f32;
    let radius = variant.photo.corner_radius_ratio * short_edge;
    let offset_x = (variant.photo.shadow_offset_x_ratio * short_edge).round() as i64;
    let offset_y = (variant.photo.shadow_offset_y_ratio * short_edge).round() as i64;
    let mut mask = vec![0.0; (canvas.width() * canvas.height()) as usize];
    for y in 0..canvas.height() {
        for x in 0..canvas.width() {
            let local_x = i64::from(x) - rect.x - offset_x;
            let local_y = i64::from(y) - rect.y - offset_y;
            if rounded_contains(
                local_x as f32 + 0.5,
                local_y as f32 + 0.5,
                rect.width as f32,
                rect.height as f32,
                radius,
            ) {
                mask[(y * canvas.width() + x) as usize] = variant.photo.shadow_opacity;
            }
        }
    }
    let blur = (variant.photo.shadow_blur_ratio * short_edge).round() as u32;
    let mask = scalar_box_blur(&mask, canvas.width(), canvas.height(), blur);
    for (pixel, alpha) in canvas.pixels_mut().zip(mask) {
        blend_pixel(pixel, [0.0, 0.0, 0.0, alpha.clamp(0.0, 1.0)]);
    }
}

fn render_photo(
    canvas: &mut Rgba32FImage,
    source: &Rgba32FImage,
    rect: PixelRect,
    variant: &LayoutVariant,
) -> Result<(), String> {
    let resized = resize(source, rect.width, rect.height, FilterType::Lanczos3);
    let radius = variant.photo.corner_radius_ratio * rect.width.min(rect.height) as f32;
    for local_y in 0..rect.height {
        let destination_y = rect.y + i64::from(local_y);
        if destination_y < 0 || destination_y >= i64::from(canvas.height()) {
            continue;
        }
        for local_x in 0..rect.width {
            let destination_x = rect.x + i64::from(local_x);
            if destination_x < 0 || destination_x >= i64::from(canvas.width()) {
                continue;
            }
            if !rounded_contains(
                local_x as f32 + 0.5,
                local_y as f32 + 0.5,
                rect.width as f32,
                rect.height as f32,
                radius,
            ) {
                continue;
            }
            blend_pixel(
                canvas.get_pixel_mut(destination_x as u32, destination_y as u32),
                resized.get_pixel(local_x, local_y).0,
            );
        }
    }

    let stroke_width =
        (variant.photo.stroke_width_ratio * rect.width.min(rect.height) as f32).round() as u32;
    if stroke_width == 0 {
        return Ok(());
    }
    let stroke = premultiplied(parse_css_color_linear(&variant.photo.stroke_color)?, 1.0);
    let inset = stroke_width as f32;
    let inner_width = (rect.width as f32 - inset * 2.0).max(0.0);
    let inner_height = (rect.height as f32 - inset * 2.0).max(0.0);
    let inner_radius = (radius - inset).max(0.0);
    for local_y in 0..rect.height {
        let destination_y = rect.y + i64::from(local_y);
        if destination_y < 0 || destination_y >= i64::from(canvas.height()) {
            continue;
        }
        for local_x in 0..rect.width {
            let destination_x = rect.x + i64::from(local_x);
            if destination_x < 0 || destination_x >= i64::from(canvas.width()) {
                continue;
            }
            let x = local_x as f32 + 0.5;
            let y = local_y as f32 + 0.5;
            let outer = rounded_contains(x, y, rect.width as f32, rect.height as f32, radius);
            let inner = rounded_contains(
                x - inset,
                y - inset,
                inner_width,
                inner_height,
                inner_radius,
            );
            if outer && !inner {
                blend_pixel(
                    canvas.get_pixel_mut(destination_x as u32, destination_y as u32),
                    stroke,
                );
            }
        }
    }
    Ok(())
}

pub(crate) fn render_base(
    source: &Path,
    variant: &LayoutVariant,
    photo_override: Option<&PhotoPlacementOverride>,
    target: RenderTarget,
) -> Result<RenderedCanvas, String> {
    render_base_with_resources(source, variant, photo_override, target, &BTreeMap::new())
}

pub(crate) fn render_base_with_resources(
    source: &Path,
    variant: &LayoutVariant,
    photo_override: Option<&PhotoPlacementOverride>,
    target: RenderTarget,
    resources: &BTreeMap<String, EmbeddedTemplateResource>,
) -> Result<RenderedCanvas, String> {
    let extension = source
        .extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if !matches!(extension.as_str(), "jpg" | "jpeg") {
        return Err("水印照片来源只支持 JPG/JPEG".to_string());
    }
    let decoded = decode_path(source)?;
    let (source_width, source_height) = decoded.image.dimensions();
    let (align_x, align_y, photo_scale) = photo_override
        .map(|value| (value.align_x, value.align_y, value.scale))
        .unwrap_or((
            variant.photo.align_x,
            variant.photo.align_y,
            variant.photo.scale,
        ));
    let (output_long_edge, preview) = match target {
        RenderTarget::Preview { max_edge } => {
            if max_edge == 0 {
                return Err("预览长边必须大于 0".to_string());
            }
            (Some(max_edge.min(source_width.max(source_height))), true)
        }
        RenderTarget::Export { output_long_edge } => (output_long_edge, false),
    };
    let input = ResolvedLayoutInput {
        photo_width: source_width,
        photo_height: source_height,
        output_long_edge,
        canvas_ratio: variant.canvas_ratio,
        frame: variant.frame,
        align_x,
        align_y,
        photo_scale,
    };
    let layout = if preview {
        resolve_preview_layout(input)?
    } else {
        resolve_layout(input)?
    };
    let mut canvas = Rgba32FImage::new(layout.canvas.width, layout.canvas.height);
    render_background(&mut canvas, &variant.background, &decoded.image, resources)?;
    render_shadow(&mut canvas, layout.photo_rect, variant);
    render_photo(&mut canvas, &decoded.image, layout.photo_rect, variant)?;
    Ok(RenderedCanvas {
        image: canvas,
        layout,
        source_icc: decoded.source_icc,
    })
}
