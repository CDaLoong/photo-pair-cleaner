#![allow(dead_code)]

#[path = "../src/watermark_color.rs"]
mod watermark_color;
#[path = "../src/watermark_geometry.rs"]
mod watermark_geometry;
#[path = "../src/watermark_model.rs"]
mod watermark_model;
#[path = "../src/watermark_render.rs"]
mod watermark_render;

use base64::Engine;
use image::codecs::png::PngEncoder;
use image::{ImageEncoder, Rgb, RgbImage, Rgba, RgbaImage};
use moxcms::ColorProfile;
use std::collections::BTreeMap;
use std::path::Path;
use watermark_color::{OutputColorSpace, linear_srgb_to_output, source_to_linear_srgb};
use watermark_model::{
    EmbeddedTemplateResource, FrameInsets, GradientStop, ImageFit, LayoutVariant,
    WatermarkBackground, default_template,
};
use watermark_render::{RenderTarget, render_base, render_base_with_resources};

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
    .err()
    .expect("PNG source should be rejected");
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
