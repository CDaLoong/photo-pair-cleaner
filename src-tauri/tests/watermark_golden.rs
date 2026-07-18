#![allow(dead_code)]

#[path = "../src/formats.rs"]
mod formats;
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
#[path = "../src/watermark_source.rs"]
mod watermark_source;
#[path = "../src/watermark_text.rs"]
mod watermark_text;

use base64::Engine;
use image::codecs::jpeg::JpegEncoder;
use image::{ImageEncoder, Rgb, RgbImage, RgbaImage};
use little_exif::exif_tag::ExifTag;
use little_exif::filetype::FileExtension;
use little_exif::metadata::Metadata;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use watermark_model::{
    EmbeddedTemplateResource, ImageFit, NormalizedPlacement, OutputColorSpace, TextAlign,
    VariantLayerLayout, WATERMARK_SCHEMA_VERSION, WatermarkAnchorSpace, WatermarkBackground,
    WatermarkLayer, WatermarkLayerBase, WatermarkOrientation, WatermarkRenderRequest,
    WatermarkSourceOrigin, WatermarkSourcePhoto, WatermarkTemplate, default_template,
};
use watermark_source::{SourceInput, WatermarkSourceRequest, prepare_source};

const FIXTURE_ROOT: &str = "tests/fixtures/watermark";

fn fixture(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(FIXTURE_ROOT)
        .join(path)
}

fn resource_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources")
}

fn save_fixture_jpeg(path: &Path, image: &RgbImage, orientation: Option<u16>) {
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, 96)
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
    if let Some(orientation) = orientation {
        let mut metadata = Metadata::new();
        metadata.set_tag(ExifTag::Make("FramePair".into()));
        metadata.set_tag(ExifTag::Model("Golden Camera".into()));
        metadata.set_tag(ExifTag::LensModel("Golden 35mm".into()));
        metadata.set_tag(ExifTag::FocalLength(vec![35.0_f64.into()]));
        metadata.set_tag(ExifTag::FNumber(vec![2.8_f64.into()]));
        metadata.set_tag(ExifTag::ExposureTime(vec![(1.0_f64 / 125.0).into()]));
        metadata.set_tag(ExifTag::ISO(vec![200]));
        metadata.set_tag(ExifTag::Orientation(vec![orientation]));
        metadata
            .write_to_vec(&mut bytes, FileExtension::JPEG)
            .unwrap();
    }
    fs::write(path, bytes).unwrap();
}

fn generate_inputs() {
    fs::create_dir_all(fixture("inputs")).unwrap();
    fs::create_dir_all(fixture("expected")).unwrap();
    let landscape = RgbImage::from_fn(160, 100, |x, y| {
        Rgb([
            35 + (x * 150 / 159) as u8,
            55 + (y * 120 / 99) as u8,
            190 - (x * 90 / 159) as u8,
        ])
    });
    save_fixture_jpeg(&fixture("inputs/landscape.jpg"), &landscape, None);

    let oriented = RgbImage::from_fn(140, 90, |x, y| {
        let checker = if (x / 14 + y / 15) % 2 == 0 { 28 } else { 0 };
        Rgb([
            185 - (y * 90 / 89) as u8,
            70 + (x * 120 / 139) as u8,
            65 + checker,
        ])
    });
    save_fixture_jpeg(&fixture("inputs/portrait-oriented.jpg"), &oriented, Some(6));
}

fn photos() -> Vec<WatermarkSourcePhoto> {
    prepare_source(WatermarkSourceRequest {
        origin: WatermarkSourceOrigin::Directory,
        inputs: vec![SourceInput::Directory {
            path: fixture("inputs").to_string_lossy().into_owned(),
        }],
    })
    .unwrap()
    .photos
}

fn base(id: &str, name: &str) -> WatermarkLayerBase {
    WatermarkLayerBase {
        id: id.into(),
        name: name.into(),
        z_index: 0,
        visible: true,
        locked: false,
    }
}

fn layout(
    anchor_space: WatermarkAnchorSpace,
    x: f32,
    y: f32,
    width: f32,
    font_size_ratio: Option<f32>,
) -> VariantLayerLayout {
    VariantLayerLayout {
        placement: NormalizedPlacement {
            anchor_space,
            frame_edge: None,
            x,
            y,
            width,
            rotation_deg: 0.0,
            opacity: 1.0,
        },
        font_size_ratio,
    }
}

fn install_layer(
    template: &mut WatermarkTemplate,
    layer: WatermarkLayer,
    layer_layout: VariantLayerLayout,
) {
    let id = layer.base().id.clone();
    template.shared.layers.push(layer);
    for variant in template.variants.values_mut() {
        variant
            .layer_layouts
            .insert(id.clone(), layer_layout.clone());
    }
}

fn text_style(layer_base: WatermarkLayerBase, text: &str) -> WatermarkLayer {
    WatermarkLayer::Text {
        base: layer_base,
        text: text.into(),
        font_family: "Noto Sans CJK SC".into(),
        font_weight: 500,
        color: "#202321".into(),
        align: TextAlign::Center,
        letter_spacing_ratio: 0.02,
        line_height: 1.2,
        stroke_color: "#ffffff".into(),
        stroke_width_ratio: 0.0,
        shadow_color: "#00000044".into(),
        shadow_blur_ratio: 0.006,
        shadow_offset_x_ratio: 0.002,
        shadow_offset_y_ratio: 0.004,
    }
}

fn solid_text_template() -> WatermarkTemplate {
    let mut template = default_template("golden-solid", "金图纯色文字");
    for variant in template.variants.values_mut() {
        variant.frame.top = 0.08;
        variant.frame.right = 0.08;
        variant.frame.bottom = 0.28;
        variant.frame.left = 0.08;
        variant.background = WatermarkBackground::Solid {
            color: "#f4f1e8".into(),
            opacity: 1.0,
        };
    }
    install_layer(
        &mut template,
        text_style(base("signature", "署名"), "成都 · FramePair"),
        layout(WatermarkAnchorSpace::Canvas, 0.5, 0.88, 0.7, Some(0.075)),
    );
    template
}

fn gradient_exif_template() -> WatermarkTemplate {
    let mut template = default_template("golden-gradient", "金图渐变 EXIF");
    for variant in template.variants.values_mut() {
        variant.frame.top = 0.1;
        variant.frame.right = 0.1;
        variant.frame.bottom = 0.34;
        variant.frame.left = 0.1;
        variant.background = WatermarkBackground::LinearGradient {
            angle_deg: 135.0,
            stops: vec![
                watermark_model::GradientStop {
                    offset: 0.0,
                    color: "#e8f0ec".into(),
                    opacity: 1.0,
                },
                watermark_model::GradientStop {
                    offset: 1.0,
                    color: "#d8b44a".into(),
                    opacity: 1.0,
                },
            ],
        };
    }
    install_layer(
        &mut template,
        WatermarkLayer::ExifText {
            base: base("exif", "拍摄参数"),
            fields: vec![
                "cameraModel".into(),
                "focalLength".into(),
                "aperture".into(),
                "shutterSpeed".into(),
                "iso".into(),
            ],
            separator: " · ".into(),
            prefix: "".into(),
            suffix: "".into(),
            missing_value: None,
            font_family: "Noto Sans CJK SC".into(),
            font_weight: 400,
            color: "#17352c".into(),
            align: TextAlign::Center,
            letter_spacing_ratio: 0.0,
            line_height: 1.2,
            stroke_color: "#ffffff".into(),
            stroke_width_ratio: 0.0,
            shadow_color: "#00000000".into(),
            shadow_blur_ratio: 0.0,
            shadow_offset_x_ratio: 0.0,
            shadow_offset_y_ratio: 0.0,
        },
        layout(WatermarkAnchorSpace::Canvas, 0.5, 0.88, 0.84, Some(0.044)),
    );
    template
}

fn blur_logo_template() -> WatermarkTemplate {
    let mut template = default_template("golden-blur", "金图模糊 Logo");
    let logo_bytes = include_bytes!("../icons/icon.png");
    let logo = image::load_from_memory(logo_bytes).unwrap();
    let resource = EmbeddedTemplateResource {
        id: "framepair-logo".into(),
        name: "FramePair Logo".into(),
        mime_type: "image/png".into(),
        sha256: format!("{:x}", Sha256::digest(logo_bytes)),
        width: logo.width(),
        height: logo.height(),
        data_base64: base64::engine::general_purpose::STANDARD.encode(logo_bytes),
    };
    template.resources.insert(resource.id.clone(), resource);
    for variant in template.variants.values_mut() {
        variant.frame.top = 0.16;
        variant.frame.right = 0.16;
        variant.frame.bottom = 0.16;
        variant.frame.left = 0.16;
        variant.background = WatermarkBackground::BlurredPhoto {
            blur_ratio: 0.08,
            scale: 1.16,
            overlay_color: "#152019".into(),
            overlay_opacity: 0.28,
        };
        variant.photo.corner_radius_ratio = 0.035;
        variant.photo.shadow_blur_ratio = 0.045;
        variant.photo.shadow_opacity = 0.4;
        variant.photo.shadow_offset_y_ratio = 0.02;
    }
    install_layer(
        &mut template,
        WatermarkLayer::Image {
            base: base("logo", "Logo"),
            resource_id: "framepair-logo".into(),
            fit: ImageFit::Contain,
        },
        layout(WatermarkAnchorSpace::Canvas, 0.88, 0.16, 0.16, None),
    );
    template
}

fn render(photo: &WatermarkSourcePhoto, template: WatermarkTemplate) -> RgbaImage {
    let request = WatermarkRenderRequest {
        schema_version: WATERMARK_SCHEMA_VERSION,
        source: photo.clone(),
        template,
        photo_override: None,
        color_space: OutputColorSpace::Srgb,
        transparent_background: false,
        jpeg_flatten_color: "#ffffff".into(),
    };
    let source = Path::new(&photo.root).join(&photo.relative_path);
    let rendered = watermark_render::render_request(&source, &request, &resource_dir()).unwrap();
    image::load_from_memory(&watermark_render::encode_preview_png(&rendered).unwrap())
        .unwrap()
        .to_rgba8()
}

fn assert_image_close(actual: &RgbaImage, expected: &RgbaImage) {
    assert_eq!(actual.dimensions(), expected.dimensions());
    let mut mismatches = 0usize;
    for (left, right) in actual.pixels().zip(expected.pixels()) {
        if left.0.iter().zip(right.0).any(|(a, b)| a.abs_diff(b) > 2) {
            mismatches += 1;
        }
    }
    let ratio = mismatches as f64 / (actual.width() as f64 * actual.height() as f64);
    assert!(ratio < 0.001, "golden mismatch ratio {ratio}");
}

#[test]
fn reviewed_watermark_goldens_remain_stable() {
    let update = std::env::var_os("UPDATE_WATERMARK_GOLDENS").is_some();
    if update {
        generate_inputs();
    }
    let photos = photos();
    let landscape = photos
        .iter()
        .find(|photo| photo.file_name == "landscape.jpg")
        .unwrap();
    let portrait = photos
        .iter()
        .find(|photo| photo.file_name == "portrait-oriented.jpg")
        .unwrap();
    assert_eq!(portrait.orientation, WatermarkOrientation::Portrait);

    let cases = [
        ("solid-text.png", render(landscape, solid_text_template())),
        (
            "gradient-exif.png",
            render(portrait, gradient_exif_template()),
        ),
        ("blur-logo.png", render(landscape, blur_logo_template())),
    ];
    for (name, actual) in cases {
        let expected_path = fixture(&format!("expected/{name}"));
        if update {
            actual.save(&expected_path).unwrap();
        }
        let expected = image::open(&expected_path)
            .unwrap_or_else(|_| panic!("missing reviewed golden {}", expected_path.display()))
            .to_rgba8();
        assert_image_close(&actual, &expected);
    }
}
