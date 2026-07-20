#[path = "../src/formats.rs"]
#[allow(dead_code)]
mod formats;
#[allow(dead_code)]
#[path = "../src/photo_groups.rs"]
mod photo_groups;
#[path = "../src/preview.rs"]
mod preview;
#[allow(dead_code)]
#[path = "../src/preview_cache.rs"]
mod preview_cache;
#[allow(dead_code)]
#[path = "../src/rating_metadata.rs"]
mod rating_metadata;

use std::fs;

use image::{GenericImageView, Rgb, RgbImage};

fn add_exif_orientation(path: &std::path::Path, orientation: u16) {
    let mut jpeg = fs::read(path).expect("jpeg bytes");
    assert_eq!(&jpeg[..2], &[0xff, 0xd8]);
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
    fs::write(path, jpeg).expect("oriented jpeg");
}

#[test]
fn index_groups_jpeg_and_raw_files_into_logical_photos() {
    let temp = tempfile::tempdir().expect("temp root");
    let root = temp.path().join("photos");
    fs::create_dir_all(root.join("day-one")).expect("photo directory");
    fs::write(root.join("day-one/DSC_0001.JPG"), b"jpeg").expect("jpeg");
    fs::write(root.join("day-one/DSC_0001.NEF"), b"raw").expect("raw");
    fs::write(root.join("day-one/DSC_0002.CR3"), b"raw").expect("raw");

    let index = photo_groups::index_directory(&root).expect("photo index");

    assert_eq!(index.total_assets, 2);
    assert_eq!(index.paired_assets, 1);
    assert_eq!(index.previewable_assets, 1);
    assert_eq!(index.raw_only_assets, 1);

    let paired = &index.assets[0];
    assert_eq!(paired.relative_stem, "day-one/DSC_0001");
    assert_eq!(paired.preview_path.as_deref(), Some("day-one/DSC_0001.JPG"));
    assert_eq!(paired.jpeg_paths, ["day-one/DSC_0001.JPG"]);
    assert_eq!(paired.raw_paths, ["day-one/DSC_0001.NEF"]);
    assert_eq!(paired.extensions, ["JPG", "NEF"]);

    let raw_only = &index.assets[1];
    assert_eq!(raw_only.relative_stem, "day-one/DSC_0002");
    assert!(raw_only.preview_path.is_none());
    assert_eq!(raw_only.raw_paths, ["day-one/DSC_0002.CR3"]);
}

#[test]
fn index_ignores_unrelated_files_and_framepair_quarantine() {
    let temp = tempfile::tempdir().expect("temp root");
    let root = temp.path().join("photos");
    fs::create_dir_all(root.join(".framepair-quarantine/operation-1"))
        .expect("quarantine directory");
    fs::write(root.join("visible.ARW"), b"raw").expect("visible raw");
    fs::write(root.join("notes.txt"), b"notes").expect("unrelated file");
    fs::write(
        root.join(".framepair-quarantine/operation-1/hidden.ARW"),
        b"raw",
    )
    .expect("quarantined raw");

    let index = photo_groups::index_directory(&root).expect("photo index");

    assert_eq!(index.total_assets, 1);
    assert_eq!(index.assets[0].relative_stem, "visible");
}

#[test]
fn preview_path_resolution_rejects_traversal_and_non_jpeg_files() {
    let temp = tempfile::tempdir().expect("temp root");
    let root = temp.path().join("photos");
    fs::create_dir_all(&root).expect("photo directory");
    fs::write(root.join("photo.JPG"), b"jpeg").expect("jpeg");
    fs::write(root.join("photo.NEF"), b"raw").expect("raw");

    assert!(preview::resolve_preview_path(&root, "photo.JPG").is_ok());
    assert!(preview::resolve_preview_path(&root, "../photo.JPG").is_err());
    assert!(preview::resolve_preview_path(&root, "photo.NEF").is_err());
}

#[test]
fn thumbnail_generation_resizes_jpeg_and_reuses_the_disk_cache() {
    let temp = tempfile::tempdir().expect("temp root");
    let root = temp.path().join("photos");
    let cache = temp.path().join("cache");
    fs::create_dir_all(&root).expect("photo directory");
    RgbImage::from_pixel(1200, 800, Rgb([210, 80, 40]))
        .save_with_format(root.join("photo.JPG"), image::ImageFormat::Jpeg)
        .expect("test jpeg");

    let first =
        preview::load_thumbnail(&root, "photo.JPG", 320, &cache).expect("generated thumbnail");
    let decoded = image::load_from_memory(&first).expect("thumbnail jpeg");
    assert_eq!(decoded.dimensions(), (320, 213));

    let cached =
        preview::load_thumbnail(&root, "photo.JPG", 320, &cache).expect("cached thumbnail");
    assert_eq!(cached, first);
    assert_eq!(
        walkdir::WalkDir::new(&cache)
            .into_iter()
            .filter_map(Result::ok)
            .filter(
                |entry| entry.path().extension().and_then(|value| value.to_str()) == Some("jpg")
            )
            .count(),
        1,
    );
    assert_eq!(
        preview::cache_stats(&cache)
            .expect("cache stats")
            .entry_count,
        1
    );
}

#[test]
fn thumbnail_generation_rejects_unbounded_sizes() {
    let temp = tempfile::tempdir().expect("temp root");
    let root = temp.path().join("photos");
    fs::create_dir_all(&root).expect("photo directory");
    RgbImage::from_pixel(10, 10, Rgb([0, 0, 0]))
        .save_with_format(root.join("photo.jpg"), image::ImageFormat::Jpeg)
        .expect("test jpeg");

    assert!(preview::load_thumbnail(&root, "photo.jpg", 0, temp.path()).is_err());
    assert!(preview::load_thumbnail(&root, "photo.jpg", 4097, temp.path()).is_err());
}

#[test]
fn concurrent_cache_writes_use_unique_temporary_paths() {
    let cache_path = std::path::Path::new("cache/thumbnail.jpg");

    assert_ne!(
        preview::temporary_cache_path(cache_path),
        preview::temporary_cache_path(cache_path),
    );
}

#[test]
fn thumbnail_generation_applies_exif_orientation() {
    let temp = tempfile::tempdir().expect("temp root");
    let root = temp.path().join("photos");
    let source = root.join("portrait.JPG");
    fs::create_dir_all(&root).expect("photo directory");
    RgbImage::from_pixel(120, 80, Rgb([40, 90, 180]))
        .save_with_format(&source, image::ImageFormat::Jpeg)
        .expect("test jpeg");
    add_exif_orientation(&source, 6);

    let bytes = preview::load_thumbnail(&root, "portrait.JPG", 320, temp.path())
        .expect("oriented thumbnail");
    let decoded = image::load_from_memory(&bytes).expect("thumbnail jpeg");

    assert_eq!(decoded.dimensions(), (80, 120));
}

#[test]
fn thumbnail_cache_uses_two_shard_directories() {
    let temp = tempfile::tempdir().expect("temp root");
    let root = temp.path().join("photos");
    let source = root.join("photo.JPG");
    fs::create_dir_all(&root).expect("photo directory");
    RgbImage::from_pixel(1200, 800, Rgb([210, 80, 40]))
        .save_with_format(&source, image::ImageFormat::Jpeg)
        .expect("test jpeg");
    let metadata = fs::metadata(&source).expect("photo metadata");

    let relative = preview::thumbnail_cache_relative_path(&source, &metadata, 512);
    let components = relative.components().collect::<Vec<_>>();

    assert_eq!(components.len(), 3);
    assert_eq!(
        relative.extension().and_then(|value| value.to_str()),
        Some("jpg")
    );
    assert_eq!(components[0].as_os_str().len(), 2);
    assert_eq!(components[1].as_os_str().len(), 2);
}

#[test]
fn legacy_flat_thumbnail_is_promoted_without_regeneration() {
    let temp = tempfile::tempdir().expect("temp root");
    let root = temp.path().join("photos");
    let cache = temp.path().join("cache");
    let source = root.join("photo.JPG");
    fs::create_dir_all(&root).expect("photo directory");
    fs::create_dir_all(&cache).expect("cache directory");
    RgbImage::from_pixel(1200, 800, Rgb([210, 80, 40]))
        .save_with_format(&source, image::ImageFormat::Jpeg)
        .expect("test jpeg");
    let source_bytes = fs::read(&source).expect("source bytes");
    let canonical_source = fs::canonicalize(&source).expect("canonical source");
    let metadata = fs::metadata(&canonical_source).expect("photo metadata");
    let relative = preview::thumbnail_cache_relative_path(&canonical_source, &metadata, 512);
    let legacy_path = cache.join(relative.file_name().expect("cache filename"));
    fs::write(&legacy_path, &source_bytes).expect("legacy cache file");

    let loaded =
        preview::load_thumbnail(&root, "photo.JPG", 512, &cache).expect("promote legacy cache");

    assert_eq!(loaded, source_bytes);
    assert!(!legacy_path.exists());
    assert!(cache.join(relative).is_file());
}
