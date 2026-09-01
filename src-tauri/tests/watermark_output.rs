#![allow(dead_code)]
#[path = "../src/fs_util.rs"]
mod fs_util;

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
#[path = "../src/watermark_output.rs"]
mod watermark_output;
#[path = "../src/watermark_render.rs"]
mod watermark_render;
#[path = "../src/watermark_source.rs"]
mod watermark_source;
#[path = "../src/watermark_text.rs"]
mod watermark_text;

use image::{ImageFormat, ImageReader, Rgb, RgbImage};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use watermark_model::{
    CollisionPolicy, MetadataPolicy, OutputColorSpace, WATERMARK_SCHEMA_VERSION,
    WatermarkOutputFormat, WatermarkOutputSettings, WatermarkRenderRequest, WatermarkSizing,
    WatermarkSourceOrigin, WatermarkSourceSnapshot, default_template,
};
use watermark_output::{PlannedCollision, WatermarkOutputStatus, plan_outputs, write_output};
use watermark_source::{SourceInput, WatermarkSourceRequest, prepare_source};

fn save_jpeg(path: &Path, width: u32, height: u32, color: Rgb<u8>) {
    RgbImage::from_pixel(width, height, color)
        .save(path)
        .unwrap();
}

fn snapshot(root: &Path) -> WatermarkSourceSnapshot {
    prepare_source(WatermarkSourceRequest {
        origin: WatermarkSourceOrigin::Directory,
        inputs: vec![SourceInput::Directory {
            path: root.to_string_lossy().into_owned(),
        }],
    })
    .unwrap()
}

fn settings(format: WatermarkOutputFormat, output: Option<&Path>) -> WatermarkOutputSettings {
    WatermarkOutputSettings {
        format,
        jpeg_quality: 90,
        sizing: WatermarkSizing::Original {
            allow_upscale: false,
        },
        color_space: OutputColorSpace::Srgb,
        transparent_background: false,
        jpeg_flatten_color: "#ffffff".into(),
        metadata_policy: MetadataPolicy::Remove,
        output_directory: output.map(|path| path.to_string_lossy().into_owned()),
        suffix: "_FramePair".into(),
        collision_policy: CollisionPolicy::Sequence,
    }
}

fn source_hash(snapshot: &WatermarkSourceSnapshot) -> Vec<u8> {
    let photo = &snapshot.photos[0];
    let bytes = fs::read(Path::new(&photo.root).join(&photo.relative_path)).unwrap();
    Sha256::digest(bytes).to_vec()
}

#[test]
fn plans_extensions_sizes_and_requires_a_directory_for_multiple_roots() {
    let temp = tempfile::tempdir().unwrap();
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(&second).unwrap();
    save_jpeg(&first.join("wide.jpg"), 120, 80, Rgb([20, 80, 160]));
    save_jpeg(&second.join("tall.jpeg"), 60, 100, Rgb([180, 50, 20]));

    let single = snapshot(&first);
    let jpeg = plan_outputs(&single, &settings(WatermarkOutputFormat::Jpeg, None)).unwrap();
    assert_eq!(jpeg[0].target_path.extension().unwrap(), "jpg");
    assert_eq!((jpeg[0].target_width, jpeg[0].target_height), (120, 80));
    assert_eq!(
        jpeg[0].target_path.parent().unwrap(),
        fs::canonicalize(first.parent().unwrap())
            .unwrap()
            .join("FramePair-Watermarked")
    );

    let mut resized = settings(WatermarkOutputFormat::Png, Some(temp.path()));
    resized.sizing = WatermarkSizing::LongEdge {
        pixels: 64,
        allow_upscale: false,
    };
    let png = plan_outputs(&single, &resized).unwrap();
    assert_eq!(png[0].target_path.extension().unwrap(), "png");
    assert_eq!((png[0].target_width, png[0].target_height), (64, 43));
    resized.sizing = WatermarkSizing::LongEdge {
        pixels: 240,
        allow_upscale: false,
    };
    assert_eq!(
        plan_outputs(&single, &resized).unwrap()[0].target_width,
        120
    );
    resized.sizing = WatermarkSizing::LongEdge {
        pixels: 240,
        allow_upscale: true,
    };
    assert_eq!(
        plan_outputs(&single, &resized).unwrap()[0].target_width,
        240
    );

    let mut multiple = single.clone();
    let other = snapshot(&second);
    multiple.root_paths.extend(other.root_paths);
    multiple.photos.extend(other.photos);
    assert!(plan_outputs(&multiple, &settings(WatermarkOutputFormat::Jpeg, None)).is_err());
}

#[test]
fn rejects_invalid_quality_suffix_source_equality_and_untrusted_overwrite() {
    let temp = tempfile::tempdir().unwrap();
    save_jpeg(&temp.path().join("photo.jpg"), 40, 30, Rgb([40, 80, 120]));
    let source = snapshot(temp.path());
    let output = temp.path().join("output");
    fs::create_dir(&output).unwrap();

    let mut invalid = settings(WatermarkOutputFormat::Jpeg, Some(&output));
    invalid.jpeg_quality = 0;
    assert!(plan_outputs(&source, &invalid).is_err());
    invalid.jpeg_quality = 90;
    for suffix in ["../bad", "bad:name", "bad.", "bad "] {
        invalid.suffix = suffix.into();
        assert!(
            plan_outputs(&source, &invalid).is_err(),
            "accepted {suffix:?}"
        );
    }

    let mut equality = settings(WatermarkOutputFormat::Jpeg, Some(temp.path()));
    equality.suffix.clear();
    assert!(plan_outputs(&source, &equality).is_err());

    let mut overwrite = settings(WatermarkOutputFormat::Jpeg, Some(&output));
    overwrite.collision_policy = CollisionPolicy::OverwriteOutput;
    save_jpeg(&output.join("photo_FramePair.jpg"), 10, 10, Rgb([1, 2, 3]));
    assert!(plan_outputs(&source, &overwrite).is_err());
}

#[test]
fn sequence_skip_and_trusted_overwrite_are_deterministic() {
    let temp = tempfile::tempdir().unwrap();
    let source_dir = temp.path().join("source");
    let output = temp.path().join("output");
    fs::create_dir(&source_dir).unwrap();
    fs::create_dir(&output).unwrap();
    save_jpeg(&source_dir.join("photo.jpg"), 64, 48, Rgb([20, 80, 160]));
    let source = snapshot(&source_dir);
    fs::write(output.join("photo_FramePair.jpg"), b"occupied").unwrap();

    let sequence = plan_outputs(
        &source,
        &settings(WatermarkOutputFormat::Jpeg, Some(&output)),
    )
    .unwrap();
    assert_eq!(
        sequence[0].target_path.file_name().unwrap(),
        "photo_FramePair_2.jpg"
    );

    let mut skip = settings(WatermarkOutputFormat::Jpeg, Some(&output));
    skip.collision_policy = CollisionPolicy::Skip;
    let skipped = plan_outputs(&source, &skip).unwrap();
    assert_eq!(skipped[0].collision, PlannedCollision::SkipExisting);

    fs::remove_file(output.join("photo_FramePair.jpg")).unwrap();
    let request = WatermarkRenderRequest {
        schema_version: WATERMARK_SCHEMA_VERSION,
        source: source.photos[0].clone(),
        template: default_template("test", "测试"),
        photo_override: None,
        color_space: OutputColorSpace::Srgb,
        transparent_background: false,
        jpeg_flatten_color: "#ffffff".into(),
    };
    let plan = plan_outputs(
        &source,
        &settings(WatermarkOutputFormat::Jpeg, Some(&output)),
    )
    .unwrap();
    let resource_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");
    assert_eq!(
        write_output(&plan[0], &request, &resource_dir).status,
        WatermarkOutputStatus::Succeeded
    );
    let mut overwrite = settings(WatermarkOutputFormat::Jpeg, Some(&output));
    overwrite.collision_policy = CollisionPolicy::OverwriteOutput;
    let replacement = plan_outputs(&source, &overwrite).unwrap();
    assert_eq!(replacement[0].collision, PlannedCollision::OverwriteOutput);
    assert_eq!(
        write_output(&replacement[0], &request, &resource_dir).status,
        WatermarkOutputStatus::Succeeded
    );
}

#[test]
fn writes_verified_jpeg_and_transparent_png_without_changing_the_source() {
    let temp = tempfile::tempdir().unwrap();
    let source_dir = temp.path().join("source");
    let output = temp.path().join("output");
    fs::create_dir(&source_dir).unwrap();
    fs::create_dir(&output).unwrap();
    save_jpeg(&source_dir.join("photo.jpg"), 80, 50, Rgb([30, 120, 210]));
    let source = snapshot(&source_dir);
    let before = source_hash(&source);
    let resource_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");

    for format in [WatermarkOutputFormat::Jpeg, WatermarkOutputFormat::Png] {
        let mut output_settings = settings(format, Some(&output));
        output_settings.transparent_background = format == WatermarkOutputFormat::Png;
        output_settings.suffix = match format {
            WatermarkOutputFormat::Jpeg => "_jpg".into(),
            WatermarkOutputFormat::Png => "_png".into(),
        };
        let mut template = default_template("test", "测试");
        for variant in template.variants.values_mut() {
            variant.background = watermark_model::WatermarkBackground::Transparent;
            variant.frame.top = 0.2;
            variant.frame.right = 0.2;
            variant.frame.bottom = 0.2;
            variant.frame.left = 0.2;
        }
        let request = WatermarkRenderRequest {
            schema_version: WATERMARK_SCHEMA_VERSION,
            source: source.photos[0].clone(),
            template,
            photo_override: None,
            color_space: output_settings.color_space,
            transparent_background: output_settings.transparent_background,
            jpeg_flatten_color: "#ff0000".into(),
        };
        output_settings.jpeg_flatten_color = "#ff0000".into();
        let plan = plan_outputs(&source, &output_settings).unwrap();
        let result = write_output(&plan[0], &request, &resource_dir);
        assert_eq!(
            result.status,
            WatermarkOutputStatus::Succeeded,
            "{}",
            result.message
        );
        let reader = ImageReader::open(&plan[0].target_path)
            .unwrap()
            .with_guessed_format()
            .unwrap();
        assert_eq!(
            reader.format(),
            Some(match format {
                WatermarkOutputFormat::Jpeg => ImageFormat::Jpeg,
                WatermarkOutputFormat::Png => ImageFormat::Png,
            })
        );
        let decoded = reader.decode().unwrap();
        assert!(decoded.width() >= 80 && decoded.height() >= 50);
        if format == WatermarkOutputFormat::Png {
            assert_eq!(decoded.to_rgba8().get_pixel(0, 0)[3], 0);
        } else {
            let pixel = decoded.to_rgb8().get_pixel(0, 0).0;
            assert!(pixel[0] > 200 && pixel[1] < 60 && pixel[2] < 60);
        }
    }
    assert_eq!(source_hash(&source), before);
    assert!(
        fs::read_dir(&output)
            .unwrap()
            .filter_map(Result::ok)
            .all(|entry| !entry.file_name().to_string_lossy().contains(".tmp"))
    );
}
