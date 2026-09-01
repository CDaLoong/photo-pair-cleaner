#![allow(dead_code)]
#[path = "../src/fs_util.rs"]
mod fs_util;

#[path = "../src/formats.rs"]
mod formats;
#[path = "../src/watermark_model.rs"]
mod watermark_model;
#[path = "../src/watermark_source.rs"]
mod watermark_source;

use image::{Rgb, RgbImage};
use std::fs;
use std::path::Path;
use watermark_model::{WatermarkOrientation, WatermarkSourceOrigin};
use watermark_source::{SourceInput, WatermarkSourceRequest, prepare_source, revalidate_photo};

fn save_jpeg(path: &Path, width: u32, height: u32) {
    RgbImage::from_pixel(width, height, Rgb([20, 40, 60]))
        .save(path)
        .unwrap();
}

fn add_exif_orientation(path: &Path, orientation: u16) {
    let mut jpeg = fs::read(path).unwrap();
    let mut app1 = vec![0xff, 0xe1, 0x00, 0x22];
    app1.extend_from_slice(b"Exif\0\0");
    app1.extend_from_slice(&[
        0x4d,
        0x4d,
        0x00,
        0x2a,
        0x00,
        0x00,
        0x00,
        0x08,
        0x00,
        0x01,
        0x01,
        0x12,
        0x00,
        0x03,
        0x00,
        0x00,
        0x00,
        0x01,
        (orientation >> 8) as u8,
        orientation as u8,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
    ]);
    jpeg.splice(2..2, app1);
    fs::write(path, jpeg).unwrap();
}

#[test]
fn directory_source_contains_each_jpeg_and_counts_raw_only() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("photos");
    fs::create_dir_all(root.join("day")).unwrap();
    save_jpeg(&root.join("day/A.JPG"), 120, 80);
    fs::write(root.join("day/A.NEF"), b"raw").unwrap();
    fs::write(root.join("day/B.CR3"), b"raw").unwrap();

    let snapshot = prepare_source(WatermarkSourceRequest {
        origin: WatermarkSourceOrigin::Directory,
        inputs: vec![SourceInput::Directory {
            path: root.to_string_lossy().into_owned(),
        }],
    })
    .unwrap();

    assert_eq!(snapshot.photos.len(), 1);
    assert_eq!(snapshot.skipped_raw_only, 1);
    assert_eq!(
        snapshot.photos[0].orientation,
        WatermarkOrientation::Landscape
    );
    assert!(revalidate_photo(&snapshot.photos[0]).is_ok());
}

#[test]
fn multiple_jpegs_with_one_stem_remain_independent_sources() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("photos");
    fs::create_dir_all(&root).unwrap();
    save_jpeg(&root.join("same.jpg"), 120, 80);
    save_jpeg(&root.join("same.jpeg"), 80, 120);
    fs::write(root.join("same.NEF"), b"raw").unwrap();

    let snapshot = prepare_source(WatermarkSourceRequest {
        origin: WatermarkSourceOrigin::Drop,
        inputs: vec![SourceInput::Directory {
            path: root.to_string_lossy().into_owned(),
        }],
    })
    .unwrap();

    assert_eq!(snapshot.photos.len(), 2);
    assert_eq!(snapshot.skipped_raw_only, 0);
    assert_eq!(snapshot.photos[0].file_name, "same.jpeg");
    assert_eq!(snapshot.photos[1].file_name, "same.jpg");
}

#[test]
fn explicit_file_is_deduplicated_and_exif_orientation_is_applied() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("portrait-after-exif.jpg");
    save_jpeg(&path, 120, 80);
    add_exif_orientation(&path, 6);

    let input = SourceInput::File {
        path: path.to_string_lossy().into_owned(),
    };
    let snapshot = prepare_source(WatermarkSourceRequest {
        origin: WatermarkSourceOrigin::PreviewPhoto,
        inputs: vec![input.clone(), input],
    })
    .unwrap();

    assert_eq!(snapshot.photos.len(), 1);
    assert_eq!(snapshot.photos[0].pixel_width, 80);
    assert_eq!(snapshot.photos[0].pixel_height, 120);
    assert_eq!(
        snapshot.photos[0].orientation,
        WatermarkOrientation::Portrait
    );
}

#[test]
fn relative_paths_count_unsupported_members_without_authorizing_them() {
    let temp = tempfile::tempdir().unwrap();
    save_jpeg(&temp.path().join("one.jpg"), 100, 100);
    fs::write(temp.path().join("notes.txt"), b"no").unwrap();

    let snapshot = prepare_source(WatermarkSourceRequest {
        origin: WatermarkSourceOrigin::PreviewFilter,
        inputs: vec![SourceInput::RelativePaths {
            root: temp.path().to_string_lossy().into_owned(),
            relative_paths: vec!["one.jpg".to_string(), "notes.txt".to_string()],
        }],
    })
    .unwrap();

    assert_eq!(snapshot.photos.len(), 1);
    assert_eq!(snapshot.skipped_unsupported, 1);
}

#[test]
fn revalidation_detects_a_changed_source() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("one.jpg");
    save_jpeg(&path, 100, 100);
    let snapshot = prepare_source(WatermarkSourceRequest {
        origin: WatermarkSourceOrigin::PreviewPhoto,
        inputs: vec![SourceInput::File {
            path: path.to_string_lossy().into_owned(),
        }],
    })
    .unwrap();

    fs::write(&path, b"changed").unwrap();
    let error = revalidate_photo(&snapshot.photos[0]).unwrap_err();
    assert!(error.contains("发生变化"));
}

#[cfg(unix)]
#[test]
fn source_preparation_rejects_symlinks() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("photos");
    fs::create_dir_all(&root).unwrap();
    save_jpeg(&root.join("real.jpg"), 100, 100);
    symlink(root.join("real.jpg"), root.join("linked.jpg")).unwrap();

    let error = prepare_source(WatermarkSourceRequest {
        origin: WatermarkSourceOrigin::Directory,
        inputs: vec![SourceInput::Directory {
            path: root.to_string_lossy().into_owned(),
        }],
    })
    .unwrap_err();
    assert!(error.contains("符号链接"));
}
