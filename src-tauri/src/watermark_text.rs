use crate::watermark_color::parse_css_color_linear;
use crate::watermark_model::TextAlign;
use cosmic_text::fontdb::{Database, Family as DatabaseFamily, Query, Weight};
use cosmic_text::{
    Align, Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache, Wrap,
};
use image::{Rgba, Rgba32FImage};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const BUNDLED_FONT_FILE: &str = "NotoSansCJKsc-Regular.otf";
const BUNDLED_FONT_FAMILY: &str = "Noto Sans CJK SC";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FontSummary {
    pub(crate) family: String,
    pub(crate) weights: Vec<u16>,
    pub(crate) bundled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedFont {
    pub(crate) requested_family: String,
    pub(crate) resolved_family: String,
    pub(crate) used_fallback: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct TextRenderRequest {
    pub(crate) text: String,
    pub(crate) font_family: String,
    pub(crate) font_weight: u16,
    pub(crate) font_size_px: f32,
    pub(crate) box_width: u32,
    pub(crate) line_height: f32,
    pub(crate) letter_spacing_ratio: f32,
    pub(crate) align: TextAlign,
    pub(crate) color: String,
    pub(crate) stroke_color: String,
    pub(crate) stroke_width_px: f32,
    pub(crate) shadow_color: String,
    pub(crate) shadow_blur_px: f32,
    pub(crate) shadow_offset_x_px: f32,
    pub(crate) shadow_offset_y_px: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TextMetrics {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) line_count: usize,
}

pub(crate) struct RasterizedText {
    pub(crate) image: Rgba32FImage,
    pub(crate) metrics: TextMetrics,
    pub(crate) resolved_font: ResolvedFont,
}

pub(crate) struct FontCatalog {
    font_system: FontSystem,
    swash_cache: SwashCache,
    bundled_families: BTreeSet<String>,
    bundled_family: String,
}

impl FontCatalog {
    pub(crate) fn new(resource_dir: &Path) -> Result<Self, String> {
        let bundled_path = find_bundled_font(resource_dir)?;
        let mut database = Database::new();
        database
            .load_font_file(&bundled_path)
            .map_err(|error| format!("无法加载内置中文字体：{error}"))?;
        let bundled_families = database
            .faces()
            .flat_map(|face| face.families.iter().map(|family| family.0.clone()))
            .collect::<BTreeSet<_>>();
        let bundled_family = if bundled_families.contains(BUNDLED_FONT_FAMILY) {
            BUNDLED_FONT_FAMILY.to_string()
        } else {
            bundled_families
                .iter()
                .next()
                .cloned()
                .ok_or_else(|| "内置中文字体未包含可用字族".to_string())?
        };
        database.load_system_fonts();
        database.set_sans_serif_family(bundled_family.clone());
        let font_system = FontSystem::new_with_locale_and_db("zh-CN".into(), database);
        Ok(Self {
            font_system,
            swash_cache: SwashCache::new(),
            bundled_families,
            bundled_family,
        })
    }

    fn resolve(&self, requested_family: &str, weight: u16) -> ResolvedFont {
        let requested = requested_family.trim();
        let requested = if requested.is_empty() {
            self.bundled_family.as_str()
        } else {
            requested
        };
        let families = [DatabaseFamily::Name(requested)];
        let query = Query {
            families: &families,
            weight: Weight(weight),
            ..Query::default()
        };
        let resolved_family = self
            .font_system
            .db()
            .query(&query)
            .and_then(|id| self.font_system.db().face(id))
            .and_then(|face| face.families.first())
            .map(|family| family.0.clone());
        let used_fallback = resolved_family.is_none();
        ResolvedFont {
            requested_family: requested.to_string(),
            resolved_family: resolved_family.unwrap_or_else(|| self.bundled_family.clone()),
            used_fallback,
        }
    }

    fn summaries(&self) -> Vec<FontSummary> {
        let mut families = BTreeMap::<String, (BTreeSet<u16>, bool)>::new();
        for face in self.font_system.db().faces() {
            for (family, _) in &face.families {
                let entry = families.entry(family.clone()).or_default();
                entry.0.insert(face.weight.0);
                entry.1 |= self.bundled_families.contains(family);
            }
        }
        families
            .into_iter()
            .map(|(family, (weights, bundled))| FontSummary {
                family,
                weights: weights.into_iter().collect(),
                bundled,
            })
            .collect()
    }
}

pub(crate) fn list_fonts(resource_dir: &Path) -> Result<Vec<FontSummary>, String> {
    Ok(FontCatalog::new(resource_dir)?.summaries())
}

pub(crate) fn measure_text(
    request: &TextRenderRequest,
    catalog: &mut FontCatalog,
) -> Result<TextMetrics, String> {
    validate_request(request)?;
    if request.text.is_empty() {
        return Ok(TextMetrics::default());
    }
    let (_, mut buffer) = build_buffer(request, catalog)?;
    buffer.shape_until_scroll(&mut catalog.font_system, false);
    Ok(buffer_metrics(&buffer, request.box_width))
}

pub(crate) fn draw_text(
    canvas: &mut Rgba32FImage,
    request: &TextRenderRequest,
    catalog: &mut FontCatalog,
) -> Result<ResolvedFont, String> {
    let rasterized = rasterize_text(request, catalog)?;
    composite_at_origin(canvas, &rasterized.image);
    Ok(rasterized.resolved_font)
}

pub(crate) fn rasterize_text(
    request: &TextRenderRequest,
    catalog: &mut FontCatalog,
) -> Result<RasterizedText, String> {
    validate_request(request)?;
    let (resolved_font, mut buffer) = build_buffer(request, catalog)?;
    buffer.shape_until_scroll(&mut catalog.font_system, false);
    let metrics = if request.text.is_empty() {
        TextMetrics::default()
    } else {
        buffer_metrics(&buffer, request.box_width)
    };
    let effect_extent = request
        .stroke_width_px
        .max(request.shadow_blur_px * 2.0 + request.shadow_offset_x_px.abs())
        .max(request.shadow_blur_px * 2.0 + request.shadow_offset_y_px.abs());
    let padding = effect_extent.ceil().clamp(0.0, 256.0) as u32 + 2;
    let width = request.box_width.saturating_add(padding.saturating_mul(2));
    let height = metrics
        .height
        .max(1)
        .saturating_add(padding.saturating_mul(2));
    let mut base_mask = vec![0.0_f32; (width as usize).saturating_mul(height as usize)];

    if !request.text.is_empty() {
        buffer.set_size(
            Some(request.box_width as f32),
            Some(metrics.height.max(1) as f32),
        );
        let padding_i32 = padding as i32;
        buffer.draw(
            &mut catalog.font_system,
            &mut catalog.swash_cache,
            Color::rgb(255, 255, 255),
            |x, y, glyph_width, glyph_height, color| {
                let alpha = f32::from(color.a()) / 255.0;
                for local_y in 0..glyph_height {
                    let destination_y = y + local_y as i32 + padding_i32;
                    if destination_y < 0 || destination_y >= height as i32 {
                        continue;
                    }
                    for local_x in 0..glyph_width {
                        let destination_x = x + local_x as i32 + padding_i32;
                        if destination_x < 0 || destination_x >= width as i32 {
                            continue;
                        }
                        let index =
                            destination_y as usize * width as usize + destination_x as usize;
                        base_mask[index] = base_mask[index].max(alpha);
                    }
                }
            },
        );
    }

    let mut image = Rgba32FImage::new(width, height);
    if request.shadow_blur_px > 0.0
        || request.shadow_offset_x_px != 0.0
        || request.shadow_offset_y_px != 0.0
    {
        let radius = request.shadow_blur_px.round().clamp(0.0, 128.0) as u32;
        let shadow_mask = blur_mask(&base_mask, width, height, radius);
        composite_mask(
            &mut image,
            &shadow_mask,
            parse_css_color_linear(&request.shadow_color)?,
            request.shadow_offset_x_px.round() as i32,
            request.shadow_offset_y_px.round() as i32,
        );
    }
    if request.stroke_width_px > 0.0 {
        let radius = request.stroke_width_px.ceil().clamp(0.0, 64.0) as u32;
        let stroke_mask = dilate_mask(&base_mask, width, height, radius);
        composite_mask(
            &mut image,
            &stroke_mask,
            parse_css_color_linear(&request.stroke_color)?,
            0,
            0,
        );
    }
    composite_mask(
        &mut image,
        &base_mask,
        parse_css_color_linear(&request.color)?,
        0,
        0,
    );

    Ok(RasterizedText {
        image,
        metrics,
        resolved_font,
    })
}

fn find_bundled_font(resource_dir: &Path) -> Result<PathBuf, String> {
    let candidates = [
        resource_dir.join("fonts").join(BUNDLED_FONT_FILE),
        resource_dir
            .join("resources")
            .join("fonts")
            .join(BUNDLED_FONT_FILE),
        resource_dir.join(BUNDLED_FONT_FILE),
    ];
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| format!("找不到内置中文字体 {BUNDLED_FONT_FILE}"))
}

fn validate_request(request: &TextRenderRequest) -> Result<(), String> {
    if request.box_width == 0 {
        return Err("文字图层宽度必须大于 0".into());
    }
    if !request.font_size_px.is_finite() || !(1.0..=4096.0).contains(&request.font_size_px) {
        return Err("文字字号必须在 1 到 4096 像素之间".into());
    }
    if !request.line_height.is_finite() || !(0.5..=5.0).contains(&request.line_height) {
        return Err("文字行高必须在 0.5 到 5 之间".into());
    }
    if !request.letter_spacing_ratio.is_finite()
        || !(-1.0..=4.0).contains(&request.letter_spacing_ratio)
    {
        return Err("文字字距必须在 -1 到 4 之间".into());
    }
    for (label, value, maximum) in [
        ("文字描边", request.stroke_width_px, 128.0),
        ("文字阴影模糊", request.shadow_blur_px, 128.0),
        ("文字阴影 X", request.shadow_offset_x_px.abs(), 512.0),
        ("文字阴影 Y", request.shadow_offset_y_px.abs(), 512.0),
    ] {
        if !value.is_finite() || value < 0.0 || value > maximum {
            return Err(format!("{label}超出可处理范围"));
        }
    }
    Ok(())
}

fn build_buffer(
    request: &TextRenderRequest,
    catalog: &mut FontCatalog,
) -> Result<(ResolvedFont, Buffer), String> {
    let resolved = catalog.resolve(&request.font_family, request.font_weight);
    let metrics = Metrics::new(
        request.font_size_px,
        (request.font_size_px * request.line_height).max(1.0),
    );
    let mut buffer = Buffer::new(&mut catalog.font_system, metrics);
    buffer.set_size(Some(request.box_width as f32), None);
    buffer.set_wrap(Wrap::WordOrGlyph);
    let attrs = Attrs::new()
        .family(Family::Name(&resolved.resolved_family))
        .weight(Weight(request.font_weight))
        .letter_spacing(request.letter_spacing_ratio);
    let align = match request.align {
        TextAlign::Left => Align::Left,
        TextAlign::Center => Align::Center,
        TextAlign::Right => Align::Right,
    };
    buffer.set_text(&request.text, &attrs, Shaping::Advanced, Some(align));
    Ok((resolved, buffer))
}

fn buffer_metrics(buffer: &Buffer, box_width: u32) -> TextMetrics {
    let mut width = 0.0_f32;
    let mut height = 0.0_f32;
    let mut line_count = 0usize;
    for run in buffer.layout_runs() {
        width = width.max(run.line_w);
        height = height.max(run.line_top + run.line_height);
        line_count += 1;
    }
    TextMetrics {
        width: width.ceil().clamp(0.0, box_width as f32) as u32,
        height: height.ceil().max(0.0) as u32,
        line_count,
    }
}

fn premultiplied(color: [f32; 4], coverage: f32) -> [f32; 4] {
    let alpha = (color[3] * coverage).clamp(0.0, 1.0);
    [color[0] * alpha, color[1] * alpha, color[2] * alpha, alpha]
}

fn blend(destination: &mut Rgba<f32>, source: [f32; 4]) {
    let inverse = 1.0 - source[3];
    destination[0] = source[0] + destination[0] * inverse;
    destination[1] = source[1] + destination[1] * inverse;
    destination[2] = source[2] + destination[2] * inverse;
    destination[3] = source[3] + destination[3] * inverse;
}

fn composite_mask(
    image: &mut Rgba32FImage,
    mask: &[f32],
    color: [f32; 4],
    offset_x: i32,
    offset_y: i32,
) {
    let width = image.width();
    let height = image.height();
    for y in 0..height {
        for x in 0..width {
            let coverage = mask[(y * width + x) as usize];
            if coverage <= 0.0 {
                continue;
            }
            let destination_x = x as i64 + i64::from(offset_x);
            let destination_y = y as i64 + i64::from(offset_y);
            if destination_x < 0
                || destination_y < 0
                || destination_x >= i64::from(width)
                || destination_y >= i64::from(height)
            {
                continue;
            }
            blend(
                image.get_pixel_mut(destination_x as u32, destination_y as u32),
                premultiplied(color, coverage),
            );
        }
    }
}

fn blur_mask(mask: &[f32], width: u32, height: u32, radius: u32) -> Vec<f32> {
    if radius == 0 {
        return mask.to_vec();
    }
    let mut horizontal = vec![0.0; mask.len()];
    for y in 0..height {
        let mut sum = 0.0;
        for x in 0..=radius.min(width - 1) {
            sum += mask[(y * width + x) as usize];
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
    let mut output = vec![0.0; mask.len()];
    for x in 0..width {
        let mut sum = 0.0;
        for y in 0..=radius.min(height - 1) {
            sum += horizontal[(y * width + x) as usize];
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
            output[(y * width + x) as usize] = sum / (bottom - top + 1) as f32;
        }
    }
    output
}

fn dilate_mask(mask: &[f32], width: u32, height: u32, radius: u32) -> Vec<f32> {
    if radius == 0 {
        return mask.to_vec();
    }
    let mut output = vec![0.0; mask.len()];
    let radius_i64 = i64::from(radius);
    for y in 0..height {
        for x in 0..width {
            let mut maximum = 0.0_f32;
            for offset_y in -radius_i64..=radius_i64 {
                for offset_x in -radius_i64..=radius_i64 {
                    if offset_x * offset_x + offset_y * offset_y > radius_i64 * radius_i64 {
                        continue;
                    }
                    let sample_x = i64::from(x) + offset_x;
                    let sample_y = i64::from(y) + offset_y;
                    if sample_x < 0
                        || sample_y < 0
                        || sample_x >= i64::from(width)
                        || sample_y >= i64::from(height)
                    {
                        continue;
                    }
                    maximum =
                        maximum.max(mask[sample_y as usize * width as usize + sample_x as usize]);
                }
            }
            output[(y * width + x) as usize] = maximum;
        }
    }
    output
}

fn composite_at_origin(destination: &mut Rgba32FImage, source: &Rgba32FImage) {
    let width = destination.width().min(source.width());
    let height = destination.height().min(source.height());
    for y in 0..height {
        for x in 0..width {
            blend(destination.get_pixel_mut(x, y), source.get_pixel(x, y).0);
        }
    }
}
