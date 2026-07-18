#![allow(dead_code)]

#[path = "../src/watermark_color.rs"]
mod watermark_color;
#[path = "../src/watermark_geometry.rs"]
mod watermark_geometry;
#[path = "../src/watermark_metadata.rs"]
mod watermark_metadata;
#[path = "../src/watermark_model.rs"]
mod watermark_model;
#[path = "../src/watermark_render.rs"]
mod watermark_render;
#[path = "../src/watermark_text.rs"]
mod watermark_text;

use base64::Engine;
use image::codecs::png::PngEncoder;
use image::{ImageEncoder, Rgb, RgbImage, Rgba, Rgba32FImage, RgbaImage};
use moxcms::ColorProfile;
use std::collections::BTreeMap;
use std::path::Path;
use watermark_color::{OutputColorSpace, linear_srgb_to_output, source_to_linear_srgb};
use watermark_model::{
    EmbeddedTemplateResource, FrameInsets, GradientStop, ImageFit, LayoutVariant,
    NormalizedPlacement, TextAlign, VariantLayerLayout, WATERMARK_SCHEMA_VERSION,
    WatermarkAnchorSpace, WatermarkBackground, WatermarkFrameEdge, WatermarkLayer,
    WatermarkLayerBase, WatermarkOrientation, WatermarkRenderRequest, WatermarkSourcePhoto,
    WatermarkTemplate, default_template,
};
use watermark_render::{
    RenderTarget, encode_preview_png, render_base, render_base_with_resources, render_request,
    render_request_with_target,
};
use watermark_text::{FontCatalog, TextRenderRequest, draw_text, measure_text};

fn save_jpeg(path: &Path, width: u32, height: u32, color: Rgb<u8>) {
    RgbImage::from_pixel(width, height, color)
        .save(path)
        .unwrap();
}

fn variant() -> LayoutVariant {
    default_template("test", "测试")
        .variants
        .remove("landscape")
        .unwrap()
}

fn wide_frame(variant: &mut LayoutVariant) {
    variant.frame = FrameInsets {
        top: 0.3,
        right: 0.3,
        bottom: 0.3,
        left: 0.3,
    };
}

fn pixel(image: &image::Rgba32FImage, x: u32, y: u32) -> [f32; 4] {
    image.get_pixel(x, y).0
}

#[test]
fn display_p3_is_converted_instead_of_relabelled_as_srgb() {
    let p3 = ColorProfile::new_display_p3().encode().unwrap();
    let source = [200, 60, 40];
    let linear = source_to_linear_srgb(&source, Some(&p3)).unwrap();
    let (encoded, icc) = linear_srgb_to_output(&linear, &OutputColorSpace::Srgb).unwrap();
    assert_ne!(encoded, source);
    assert!(!icc.is_empty());

    let (roundtrip, preserved) =
        linear_srgb_to_output(&linear, &OutputColorSpace::SourceIcc(p3.clone())).unwrap();
    for (actual, expected) in roundtrip.iter().zip(source) {
        assert!((i16::from(*actual) - i16::from(expected)).abs() <= 2);
    }
    assert_eq!(preserved, p3);
}

#[test]
fn solid_and_transparent_frames_keep_photo_pixels_separate() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("red.jpg");
    save_jpeg(&source, 80, 40, Rgb([220, 20, 20]));

    let mut solid = variant();
    wide_frame(&mut solid);
    solid.background = WatermarkBackground::Solid {
        color: "#ffffff".into(),
        opacity: 1.0,
    };
    let rendered = render_base(
        &source,
        &solid,
        None,
        RenderTarget::Export {
            output_long_edge: None,
        },
    )
    .unwrap();
    let corner = pixel(&rendered.image, 0, 0);
    assert!(corner[0] > 0.99 && corner[1] > 0.99 && corner[2] > 0.99 && corner[3] > 0.99);
    let center = pixel(
        &rendered.image,
        rendered.layout.photo_rect.x as u32 + rendered.layout.photo_rect.width / 2,
        rendered.layout.photo_rect.y as u32 + rendered.layout.photo_rect.height / 2,
    );
    assert!(center[0] > center[1] * 5.0);

    solid.background = WatermarkBackground::Transparent;
    let transparent = render_base(
        &source,
        &solid,
        None,
        RenderTarget::Export {
            output_long_edge: None,
        },
    )
    .unwrap();
    assert_eq!(pixel(&transparent.image, 0, 0)[3], 0.0);
}

#[test]
fn photo_source_rejects_png_even_when_the_decoder_supports_it() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("not-a-jpg.png");
    RgbaImage::from_pixel(20, 20, Rgba([20, 40, 60, 255]))
        .save(&source)
        .unwrap();
    let error = render_base(
        &source,
        &variant(),
        None,
        RenderTarget::Export {
            output_long_edge: None,
        },
    )
    .expect_err("PNG source should be rejected");
    assert!(error.contains("JPG/JPEG"));
}

#[test]
fn sampled_linear_and_radial_backgrounds_produce_expected_variation() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("blue.jpg");
    save_jpeg(&source, 60, 40, Rgb([20, 70, 220]));
    let mut layout = variant();
    wide_frame(&mut layout);

    layout.background = WatermarkBackground::Sampled {
        x: 0.5,
        y: 0.5,
        color: "#ffffff".into(),
        sample_each_photo: true,
    };
    let sampled = render_base(
        &source,
        &layout,
        None,
        RenderTarget::Export {
            output_long_edge: None,
        },
    )
    .unwrap();
    let sampled_corner = pixel(&sampled.image, 0, 0);
    assert!(sampled_corner[2] > sampled_corner[0] * 3.0);

    layout.background = WatermarkBackground::LinearGradient {
        angle_deg: 0.0,
        stops: vec![
            GradientStop {
                offset: 0.0,
                color: "#ff0000".into(),
                opacity: 1.0,
            },
            GradientStop {
                offset: 1.0,
                color: "#0000ff".into(),
                opacity: 1.0,
            },
        ],
    };
    let linear = render_base(
        &source,
        &layout,
        None,
        RenderTarget::Export {
            output_long_edge: None,
        },
    )
    .unwrap();
    let left = pixel(&linear.image, 0, 0);
    let right = pixel(&linear.image, linear.image.width() - 1, 0);
    assert!(left[0] > left[2]);
    assert!(right[2] > right[0]);

    layout.background = WatermarkBackground::RadialGradient {
        center_x: 0.0,
        center_y: 0.0,
        radius: 1.0,
        stops: vec![
            GradientStop {
                offset: 0.0,
                color: "#ffffff".into(),
                opacity: 1.0,
            },
            GradientStop {
                offset: 1.0,
                color: "#000000".into(),
                opacity: 1.0,
            },
        ],
    };
    let radial = render_base(
        &source,
        &layout,
        None,
        RenderTarget::Export {
            output_long_edge: None,
        },
    )
    .unwrap();
    assert!(
        pixel(&radial.image, 0, 0)[0]
            > pixel(
                &radial.image,
                radial.image.width() - 1,
                radial.image.height() - 1
            )[0]
    );
}

#[test]
fn blurred_and_embedded_image_backgrounds_cover_the_canvas() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("photo.jpg");
    save_jpeg(&source, 80, 40, Rgb([220, 180, 30]));
    let mut layout = variant();
    wide_frame(&mut layout);
    layout.background = WatermarkBackground::BlurredPhoto {
        blur_ratio: 0.08,
        scale: 1.2,
        overlay_color: "#000000".into(),
        overlay_opacity: 0.1,
    };
    let blurred = render_base(
        &source,
        &layout,
        None,
        RenderTarget::Export {
            output_long_edge: None,
        },
    )
    .unwrap();
    assert!(pixel(&blurred.image, 0, 0)[3] > 0.99);

    let background = RgbaImage::from_pixel(4, 4, Rgba([20, 220, 40, 255]));
    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(background.as_raw(), 4, 4, image::ExtendedColorType::Rgba8)
        .unwrap();
    let resource = EmbeddedTemplateResource {
        id: "green".into(),
        name: "绿色背景".into(),
        mime_type: "image/png".into(),
        sha256: "test".into(),
        width: 4,
        height: 4,
        data_base64: base64::engine::general_purpose::STANDARD.encode(png),
    };
    let resources = BTreeMap::from([("green".to_string(), resource)]);
    layout.background = WatermarkBackground::Image {
        resource_id: "green".into(),
        fit: ImageFit::Cover,
        opacity: 1.0,
    };
    let image_background = render_base_with_resources(
        &source,
        &layout,
        None,
        RenderTarget::Export {
            output_long_edge: None,
        },
        &resources,
    )
    .unwrap();
    let green = pixel(&image_background.image, 0, 0);
    assert!(green[1] > green[0] * 4.0 && green[1] > green[2] * 4.0);
}

#[test]
fn photo_scale_alignment_rounding_stroke_and_shadow_are_applied() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("red.jpg");
    save_jpeg(&source, 100, 60, Rgb([230, 20, 20]));
    let mut layout = variant();
    wide_frame(&mut layout);
    layout.background = WatermarkBackground::Transparent;
    layout.photo.align_x = 1.0;
    layout.photo.align_y = 1.0;
    layout.photo.scale = 0.6;
    layout.photo.corner_radius_ratio = 0.2;
    layout.photo.stroke_width_ratio = 0.04;
    layout.photo.stroke_color = "#ffffff".into();
    layout.photo.shadow_blur_ratio = 0.05;
    layout.photo.shadow_opacity = 0.8;
    layout.photo.shadow_offset_x_ratio = 0.04;
    layout.photo.shadow_offset_y_ratio = 0.04;

    let rendered = render_base(
        &source,
        &layout,
        None,
        RenderTarget::Export {
            output_long_edge: None,
        },
    )
    .unwrap();
    assert!(rendered.layout.photo_rect.x > i64::from(rendered.layout.frame.left));
    assert!(rendered.layout.photo_rect.y > i64::from(rendered.layout.frame.top));
    let center = pixel(
        &rendered.image,
        rendered.layout.photo_rect.x as u32 + rendered.layout.photo_rect.width / 2,
        rendered.layout.photo_rect.y as u32 + rendered.layout.photo_rect.height / 2,
    );
    assert!(center[0] > center[1] * 5.0);
    let corner = pixel(
        &rendered.image,
        rendered.layout.photo_rect.x as u32,
        rendered.layout.photo_rect.y as u32,
    );
    assert!(corner[0] < center[0] || corner[3] < center[3]);
    let shadow_x = (rendered.layout.photo_rect.right() + 1)
        .min(i64::from(rendered.layout.canvas.width - 1)) as u32;
    let shadow_y =
        (rendered.layout.photo_rect.y + i64::from(rendered.layout.photo_rect.height / 2)) as u32;
    assert!(pixel(&rendered.image, shadow_x, shadow_y)[3] > 0.0);
}

fn resource_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("resources")
}

fn placement(
    anchor_space: WatermarkAnchorSpace,
    frame_edge: Option<WatermarkFrameEdge>,
    x: f32,
    y: f32,
    width: f32,
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
        font_size_ratio: None,
    }
}

fn add_layer(template: &mut WatermarkTemplate, layer: WatermarkLayer, layout: VariantLayerLayout) {
    let id = layer.base().id.clone();
    template.shared.layers.push(layer);
    for variant in template.variants.values_mut() {
        variant.layer_layouts.insert(id.clone(), layout.clone());
    }
}

fn base(id: &str, z_index: i32) -> WatermarkLayerBase {
    WatermarkLayerBase {
        id: id.into(),
        name: id.into(),
        z_index,
        visible: true,
        locked: false,
    }
}

fn image_resource(id: &str, color: Rgba<u8>) -> EmbeddedTemplateResource {
    let image = RgbaImage::from_pixel(8, 8, color);
    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(image.as_raw(), 8, 8, image::ExtendedColorType::Rgba8)
        .unwrap();
    EmbeddedTemplateResource {
        id: id.into(),
        name: id.into(),
        mime_type: "image/png".into(),
        sha256: "test".into(),
        width: 8,
        height: 8,
        data_base64: base64::engine::general_purpose::STANDARD.encode(png),
    }
}

fn render_request_for(source: &Path, template: WatermarkTemplate) -> WatermarkRenderRequest {
    WatermarkRenderRequest {
        schema_version: WATERMARK_SCHEMA_VERSION,
        source: WatermarkSourcePhoto {
            id: "photo".into(),
            root: source.parent().unwrap().to_string_lossy().into_owned(),
            relative_path: source.file_name().unwrap().to_string_lossy().into_owned(),
            file_name: source.file_name().unwrap().to_string_lossy().into_owned(),
            size_bytes: std::fs::metadata(source).unwrap().len(),
            modified_ms: 1,
            pixel_width: 240,
            pixel_height: 120,
            orientation: WatermarkOrientation::Landscape,
        },
        template,
        photo_override: None,
        color_space: watermark_model::OutputColorSpace::Srgb,
        transparent_background: false,
        jpeg_flatten_color: "#ffffff".into(),
    }
}

#[test]
fn bundled_font_shapes_chinese_latin_multiline_and_draws_effects() {
    let mut catalog = FontCatalog::new(&resource_dir()).unwrap();
    let request = TextRenderRequest {
        text: "FramePair 摄影\n第二行 2026".into(),
        font_family: "Noto Sans CJK SC".into(),
        font_weight: 400,
        font_size_px: 32.0,
        box_width: 280,
        line_height: 1.35,
        letter_spacing_ratio: 0.04,
        align: TextAlign::Center,
        color: "#f5f5f5".into(),
        stroke_color: "#101010".into(),
        stroke_width_px: 2.0,
        shadow_color: "#00000099".into(),
        shadow_blur_px: 4.0,
        shadow_offset_x_px: 3.0,
        shadow_offset_y_px: 3.0,
    };
    let metrics = measure_text(&request, &mut catalog).unwrap();
    assert_eq!(metrics.line_count, 2);
    assert!(metrics.width > 100 && metrics.width <= request.box_width);
    assert!(metrics.height > 60);

    let mut canvas = Rgba32FImage::new(320, 160);
    let resolved = draw_text(&mut canvas, &request, &mut catalog).unwrap();
    assert!(!resolved.used_fallback);
    assert!(canvas.pixels().any(|pixel| pixel[3] > 0.0));

    let mut fallback = request.clone();
    fallback.font_family = "Definitely Missing FramePair Font".into();
    let resolved = draw_text(&mut canvas, &fallback, &mut catalog).unwrap();
    assert!(resolved.used_fallback);
    assert_eq!(resolved.resolved_family, "Noto Sans CJK SC");
}

#[test]
fn request_renders_ordered_image_text_and_exif_layers_in_each_anchor_space() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source.jpg");
    save_jpeg(&source, 240, 120, Rgb([35, 80, 180]));
    let mut template = default_template("layers", "图层测试");
    for variant in template.variants.values_mut() {
        variant.frame.bottom = 0.5;
    }
    let red = image_resource("red", Rgba([255, 0, 0, 255]));
    let green = image_resource("green", Rgba([0, 255, 0, 160]));
    template.resources.insert(red.id.clone(), red);
    template.resources.insert(green.id.clone(), green);

    add_layer(
        &mut template,
        WatermarkLayer::Image {
            base: base("photo-logo", 0),
            resource_id: "red".into(),
            fit: ImageFit::Contain,
        },
        placement(WatermarkAnchorSpace::Photo, None, 0.18, 0.2, 0.18),
    );
    let mut frame_layout = placement(
        WatermarkAnchorSpace::Frame,
        Some(WatermarkFrameEdge::Bottom),
        0.5,
        0.5,
        0.22,
    );
    frame_layout.placement.rotation_deg = 18.0;
    frame_layout.placement.opacity = 0.7;
    add_layer(
        &mut template,
        WatermarkLayer::Image {
            base: base("frame-logo", 1),
            resource_id: "red".into(),
            fit: ImageFit::Contain,
        },
        frame_layout,
    );
    add_layer(
        &mut template,
        WatermarkLayer::Image {
            base: base("top-logo", 2),
            resource_id: "green".into(),
            fit: ImageFit::Cover,
        },
        placement(
            WatermarkAnchorSpace::Frame,
            Some(WatermarkFrameEdge::Bottom),
            0.5,
            0.5,
            0.22,
        ),
    );

    let mut text_layout = placement(WatermarkAnchorSpace::Canvas, None, 0.5, 0.08, 0.7);
    text_layout.font_size_ratio = Some(0.08);
    add_layer(
        &mut template,
        WatermarkLayer::Text {
            base: base("title", 3),
            text: "FramePair 摄影".into(),
            font_family: "Missing UI Font".into(),
            font_weight: 500,
            color: "#ffffff".into(),
            align: TextAlign::Center,
            letter_spacing_ratio: 0.02,
            line_height: 1.2,
            stroke_color: "#000000".into(),
            stroke_width_ratio: 0.01,
            shadow_color: "#00000088".into(),
            shadow_blur_ratio: 0.02,
            shadow_offset_x_ratio: 0.01,
            shadow_offset_y_ratio: 0.01,
        },
        text_layout,
    );
    let mut exif_layout = placement(WatermarkAnchorSpace::Photo, None, 0.5, 0.9, 0.8);
    exif_layout.font_size_ratio = Some(0.05);
    add_layer(
        &mut template,
        WatermarkLayer::ExifText {
            base: base("exif", 4),
            fields: vec!["cameraModel".into(), "lensModel".into(), "aperture".into()],
            separator: " · ".into(),
            prefix: "[".into(),
            suffix: "]".into(),
            missing_value: Some("未知".into()),
            font_family: "Noto Sans CJK SC".into(),
            font_weight: 400,
            color: "#ffffff".into(),
            align: TextAlign::Center,
            letter_spacing_ratio: 0.0,
            line_height: 1.2,
            stroke_color: "#000000".into(),
            stroke_width_ratio: 0.008,
            shadow_color: "#00000000".into(),
            shadow_blur_ratio: 0.0,
            shadow_offset_x_ratio: 0.0,
            shadow_offset_y_ratio: 0.0,
        },
        exif_layout,
    );

    let request = render_request_for(&source, template);
    let rendered = render_request(&source, &request, &resource_dir()).unwrap();
    assert!(
        rendered
            .warnings
            .iter()
            .any(|warning| warning.contains("Missing UI Font"))
    );

    let photo_logo_x = (rendered.layout.photo_rect.x as f32
        + rendered.layout.photo_rect.width as f32 * 0.18)
        .round() as u32;
    let photo_logo_y = (rendered.layout.photo_rect.y as f32
        + rendered.layout.photo_rect.height as f32 * 0.2)
        .round() as u32;
    let photo_logo = pixel(&rendered.image, photo_logo_x, photo_logo_y);
    assert!(photo_logo[0] > photo_logo[2]);

    let frame_y = rendered.layout.photo_rect.bottom() as u32
        + (rendered.layout.canvas.height - rendered.layout.photo_rect.bottom() as u32) / 2;
    let frame_logo = pixel(&rendered.image, rendered.layout.canvas.width / 2, frame_y);
    assert!(frame_logo[1] > 0.1);
    assert!(
        frame_logo[0] > 0.02,
        "transparent top logo must retain the lower layer"
    );
}

#[test]
fn corrupted_logo_and_missing_visible_layout_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source.jpg");
    save_jpeg(&source, 240, 120, Rgb([35, 80, 180]));
    let mut template = default_template("broken", "损坏资源");
    template.resources.insert(
        "broken".into(),
        EmbeddedTemplateResource {
            id: "broken".into(),
            name: "损坏 Logo".into(),
            mime_type: "image/png".into(),
            sha256: "test".into(),
            width: 10,
            height: 10,
            data_base64: base64::engine::general_purpose::STANDARD.encode(b"not a png"),
        },
    );
    add_layer(
        &mut template,
        WatermarkLayer::Image {
            base: base("logo", 0),
            resource_id: "broken".into(),
            fit: ImageFit::Contain,
        },
        placement(WatermarkAnchorSpace::Canvas, None, 0.5, 0.5, 0.2),
    );
    let request = render_request_for(&source, template.clone());
    let error = render_request(&source, &request, &resource_dir()).unwrap_err();
    assert!(error.contains("解码") || error.contains("识别"));

    template
        .variants
        .get_mut("landscape")
        .unwrap()
        .layer_layouts
        .remove("logo");
    let request = render_request_for(&source, template);
    let error = render_request(&source, &request, &resource_dir()).unwrap_err();
    assert!(error.contains("缺少图层") || error.contains("数量不一致"));
}

#[test]
fn preview_target_bounds_the_final_canvas_and_encodes_lossless_png() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source.jpg");
    save_jpeg(&source, 2400, 1200, Rgb([35, 80, 180]));
    let mut template = default_template("preview", "预览");
    for variant in template.variants.values_mut() {
        variant.background = WatermarkBackground::Transparent;
        variant.frame = FrameInsets {
            top: 0.2,
            right: 0.2,
            bottom: 0.4,
            left: 0.2,
        };
    }
    let request = render_request_for(&source, template);
    let rendered = render_request_with_target(
        &source,
        &request,
        &resource_dir(),
        RenderTarget::Preview { max_edge: 600 },
    )
    .unwrap();
    assert!(rendered.image.width().max(rendered.image.height()) <= 600);

    let png = encode_preview_png(&rendered).unwrap();
    assert!(png.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]));
    let decoded = image::load_from_memory(&png).unwrap().to_rgba8();
    assert_eq!(decoded.dimensions(), rendered.image.dimensions());
    assert_eq!(decoded.get_pixel(0, 0)[3], 0);
}
