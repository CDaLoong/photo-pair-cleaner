#![allow(dead_code)]

#[path = "../src/watermark_metadata.rs"]
mod watermark_metadata;
#[path = "../src/watermark_model.rs"]
mod watermark_model;

use image::{Rgb, RgbImage, Rgba, RgbaImage};
use little_exif::exif_tag::ExifTag;
use little_exif::filetype::FileExtension;
use little_exif::ifd::ExifTagGroup;
use little_exif::metadata::Metadata;
use std::fs;
use std::path::Path;
use watermark_metadata::{
    ExifField, ExifValues, MetadataTarget, extract_jpeg_sidecars, format_exif_fields,
    prepare_output_metadata, read_exif_values,
};
use watermark_model::MetadataPolicy;

const XMP_PREFIX: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";

fn save_jpeg(path: &Path) {
    RgbImage::from_pixel(40, 30, Rgb([80, 120, 160]))
        .save(path)
        .unwrap();
}

fn source_metadata() -> Metadata {
    let mut metadata = Metadata::new();
    metadata.set_tag(ExifTag::Make("Nikon".into()));
    metadata.set_tag(ExifTag::Model("Z8".into()));
    metadata.set_tag(ExifTag::LensModel("NIKKOR Z 50mm f/1.8 S".into()));
    metadata.set_tag(ExifTag::FocalLength(vec![50.0_f64.into()]));
    metadata.set_tag(ExifTag::FNumber(vec![2.8_f64.into()]));
    metadata.set_tag(ExifTag::ExposureTime(vec![(1.0_f64 / 250.0).into()]));
    metadata.set_tag(ExifTag::ISO(vec![800]));
    metadata.set_tag(ExifTag::DateTimeOriginal("2026:07:18 12:34:56".into()));
    metadata.set_tag(ExifTag::Artist("FramePair 摄影师".into()));
    metadata.set_tag(ExifTag::Copyright("Copyright FramePair".into()));
    metadata.set_tag(ExifTag::Orientation(vec![6]));
    metadata.set_tag(ExifTag::ExifImageWidth(vec![40]));
    metadata.set_tag(ExifTag::ExifImageHeight(vec![30]));
    metadata.set_tag(ExifTag::OwnerName("Private owner".into()));
    metadata.set_tag(ExifTag::SerialNumber("BODY-SECRET".into()));
    metadata.set_tag(ExifTag::LensSerialNumber("LENS-SECRET".into()));
    metadata.set_tag(ExifTag::GPSLatitudeRef("N".into()));
    metadata.set_tag(ExifTag::GPSLatitude(vec![
        30.0_f64.into(),
        16.0_f64.into(),
        12.0_f64.into(),
    ]));
    metadata
}

fn add_segment(jpeg: &mut Vec<u8>, marker: u8, payload: &[u8]) {
    assert!(jpeg.starts_with(&[0xff, 0xd8]));
    let segment_len = payload.len() + 2;
    assert!(segment_len <= u16::MAX as usize);
    let mut segment = Vec::with_capacity(payload.len() + 4);
    segment.extend_from_slice(&[0xff, marker]);
    segment.extend_from_slice(&(segment_len as u16).to_be_bytes());
    segment.extend_from_slice(payload);
    jpeg.splice(2..2, segment);
}

fn create_source(path: &Path) {
    save_jpeg(path);
    let mut bytes = fs::read(path).unwrap();
    source_metadata()
        .write_to_vec(&mut bytes, FileExtension::JPEG)
        .unwrap();

    let xmp = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/" xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:tiff="http://ns.adobe.com/tiff/1.0/" xmlns:exif="http://ns.adobe.com/exif/1.0/" xmlns:dc="http://purl.org/dc/elements/1.1/"><rdf:RDF><rdf:Description tiff:ImageWidth="40" tiff:ImageLength="30" tiff:Orientation="6" exif:PixelXDimension="40" exif:PixelYDimension="30" dc:title="保留标题"/></rdf:RDF></x:xmpmeta>"#;
    let mut xmp_payload = XMP_PREFIX.to_vec();
    xmp_payload.extend_from_slice(xmp.as_bytes());
    add_segment(&mut bytes, 0xe1, &xmp_payload);
    add_segment(&mut bytes, 0xed, b"Photoshop 3.0\0private-iptc-fixture");
    fs::write(path, bytes).unwrap();
}

fn fresh_jpeg_bytes() -> Vec<u8> {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("output.jpg");
    save_jpeg(&path);
    fs::read(path).unwrap()
}

fn fresh_png_bytes() -> Vec<u8> {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("output.png");
    RgbaImage::from_pixel(120, 80, Rgba([20, 40, 60, 180]))
        .save(&path)
        .unwrap();
    fs::read(path).unwrap()
}

fn parse_metadata(bytes: &Vec<u8>, target: MetadataTarget) -> Metadata {
    let file_type = match target {
        MetadataTarget::Jpeg => FileExtension::JPEG,
        MetadataTarget::Png => FileExtension::PNG {
            as_zTXt_chunk: false,
        },
    };
    Metadata::new_from_vec(bytes, file_type).unwrap_or_else(|_| Metadata::new())
}

fn has_tag(metadata: &Metadata, expected: fn(String) -> ExifTag) -> bool {
    metadata.get_tag(&expected(String::new())).next().is_some()
}

#[test]
fn reads_visible_exif_values_and_formats_camera_settings() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source.jpg");
    create_source(&source);

    let values = read_exif_values(&source).unwrap();
    assert_eq!(values.camera_make.as_deref(), Some("Nikon"));
    assert_eq!(values.camera_model.as_deref(), Some("Z8"));
    assert_eq!(values.lens_model.as_deref(), Some("NIKKOR Z 50mm f/1.8 S"));
    assert_eq!(values.focal_length.as_deref(), Some("50 mm"));
    assert_eq!(values.aperture.as_deref(), Some("f/2.8"));
    assert_eq!(values.shutter_speed.as_deref(), Some("1/250 s"));
    assert_eq!(values.iso.as_deref(), Some("ISO 800"));
    assert_eq!(values.date_time.as_deref(), Some("2026:07:18 12:34:56"));
    assert_eq!(values.author.as_deref(), Some("FramePair 摄影师"));
    assert_eq!(values.copyright.as_deref(), Some("Copyright FramePair"));

    assert_eq!(
        format_exif_fields(
            &[
                ExifField::CameraMake,
                ExifField::CameraModel,
                ExifField::FocalLength,
                ExifField::Aperture,
                ExifField::ShutterSpeed,
                ExifField::Iso,
            ],
            " · ",
            &values,
            None,
        ),
        "Nikon · Z8 · 50 mm · f/2.8 · 1/250 s · ISO 800"
    );
}

#[test]
fn missing_exif_fields_collapse_separators() {
    let values = ExifValues {
        camera_model: Some("Nikon Z8".into()),
        lens_model: None,
        aperture: Some("f/2.8".into()),
        ..ExifValues::default()
    };
    assert_eq!(
        format_exif_fields(
            &[
                ExifField::CameraModel,
                ExifField::LensModel,
                ExifField::Aperture,
            ],
            " · ",
            &values,
            None,
        ),
        "Nikon Z8 · f/2.8"
    );
    assert_eq!(
        format_exif_fields(
            &[ExifField::LensModel, ExifField::Aperture],
            " / ",
            &values,
            Some("未知"),
        ),
        "未知 / f/2.8"
    );
}

#[test]
fn preserve_policy_normalizes_output_geometry_and_keeps_bounded_sidecars() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source.jpg");
    create_source(&source);

    let prepared = prepare_output_metadata(
        &source,
        MetadataPolicy::Preserve,
        120,
        80,
        MetadataTarget::Jpeg,
    )
    .unwrap();
    let mut output = fresh_jpeg_bytes();
    prepared.apply_to_encoded(&mut output).unwrap();

    let metadata = parse_metadata(&output, MetadataTarget::Jpeg);
    assert!(matches!(
        metadata.get_tag(&ExifTag::Orientation(Vec::new())).next(),
        Some(ExifTag::Orientation(value)) if value == &vec![1]
    ));
    assert!(matches!(
        metadata.get_tag(&ExifTag::ExifImageWidth(Vec::new())).next(),
        Some(ExifTag::ExifImageWidth(value)) if value == &vec![120]
    ));
    assert!(matches!(
        metadata.get_tag(&ExifTag::ExifImageHeight(Vec::new())).next(),
        Some(ExifTag::ExifImageHeight(value)) if value == &vec![80]
    ));
    assert!(
        metadata
            .get_ifd(ExifTagGroup::GPS, 0)
            .is_some_and(|ifd| !ifd.get_tags().is_empty())
    );
    assert!(has_tag(&metadata, ExifTag::SerialNumber));

    let sidecars = extract_jpeg_sidecars(&output).unwrap();
    assert_eq!(sidecars.xmp_packets.len(), 1);
    assert_eq!(sidecars.iptc_segments.len(), 1);
    let xmp = String::from_utf8(sidecars.xmp_packets[0].clone()).unwrap();
    assert!(xmp.contains("保留标题"));
    assert!(!xmp.contains("ImageWidth"));
    assert!(!xmp.contains("ImageLength"));
    assert!(!xmp.contains("Orientation"));
    assert!(!xmp.contains("PixelXDimension"));
    assert!(!xmp.contains("PixelYDimension"));
}

#[test]
fn privacy_policy_removes_location_and_serials_but_keeps_attribution() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source.jpg");
    create_source(&source);

    let prepared = prepare_output_metadata(
        &source,
        MetadataPolicy::Privacy,
        120,
        80,
        MetadataTarget::Jpeg,
    )
    .unwrap();
    let mut output = fresh_jpeg_bytes();
    prepared.apply_to_encoded(&mut output).unwrap();

    let metadata = parse_metadata(&output, MetadataTarget::Jpeg);
    assert!(
        metadata
            .get_ifd(ExifTagGroup::GPS, 0)
            .is_none_or(|ifd| ifd.get_tags().is_empty())
    );
    assert!(!has_tag(&metadata, ExifTag::OwnerName));
    assert!(!has_tag(&metadata, ExifTag::SerialNumber));
    assert!(!has_tag(&metadata, ExifTag::LensSerialNumber));
    assert!(has_tag(&metadata, ExifTag::Artist));
    assert!(has_tag(&metadata, ExifTag::Copyright));
    assert!(has_tag(&metadata, ExifTag::DateTimeOriginal));

    let sidecars = extract_jpeg_sidecars(&output).unwrap();
    assert!(sidecars.iptc_segments.is_empty());
    assert_eq!(sidecars.xmp_packets.len(), 1);
    let xmp = String::from_utf8(sidecars.xmp_packets[0].clone()).unwrap();
    assert!(xmp.contains("FramePair 摄影师"));
    assert!(xmp.contains("Copyright FramePair"));
    assert!(xmp.contains("2026:07:18 12:34:56"));
    assert!(!xmp.contains("SECRET"));
}

#[test]
fn remove_policy_and_png_target_do_not_copy_jpeg_sidecars() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source.jpg");
    create_source(&source);

    let removed = prepare_output_metadata(
        &source,
        MetadataPolicy::Remove,
        120,
        80,
        MetadataTarget::Jpeg,
    )
    .unwrap();
    let mut jpeg = fresh_jpeg_bytes();
    removed.apply_to_encoded(&mut jpeg).unwrap();
    let metadata = parse_metadata(&jpeg, MetadataTarget::Jpeg);
    assert_eq!((&metadata).into_iter().count(), 0);
    let sidecars = extract_jpeg_sidecars(&jpeg).unwrap();
    assert!(sidecars.xmp_packets.is_empty());
    assert!(sidecars.iptc_segments.is_empty());

    let png_metadata = prepare_output_metadata(
        &source,
        MetadataPolicy::Preserve,
        120,
        80,
        MetadataTarget::Png,
    )
    .unwrap();
    let mut png = fresh_png_bytes();
    png_metadata.apply_to_encoded(&mut png).unwrap();
    let parsed = parse_metadata(&png, MetadataTarget::Png);
    assert!(matches!(
        parsed.get_tag(&ExifTag::Orientation(Vec::new())).next(),
        Some(ExifTag::Orientation(value)) if value == &vec![1]
    ));
    assert!(
        !png.windows(XMP_PREFIX.len())
            .any(|window| window == XMP_PREFIX)
    );
    assert!(!png.windows(13).any(|window| window == b"Photoshop 3.0"));
}

#[test]
fn malformed_metadata_returns_an_error_without_panicking() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("corrupt.jpg");
    fs::write(&source, [0xff, 0xd8, 0xff, 0xe1, 0xff, 0xff, 0x45]).unwrap();

    let result = std::panic::catch_unwind(|| read_exif_values(&source));
    assert!(result.is_ok());
    assert!(result.unwrap().is_err());
}
