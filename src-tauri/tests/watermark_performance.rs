#![allow(dead_code)]

#[path = "../src/formats.rs"]
mod formats;
#[path = "../src/preview.rs"]
mod preview;

use image::codecs::jpeg::JpegEncoder;
use image::{ImageEncoder, Rgb, RgbImage};
use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

#[test]
#[ignore = "release performance check with 100 high-resolution JPGs"]
fn preloads_one_hundred_high_resolution_jpegs_with_three_workers() {
    let temp = tempfile::tempdir().unwrap();
    let photos = temp.path().join("photos");
    let cache = temp.path().join("cache");
    fs::create_dir(&photos).unwrap();
    let image = RgbImage::from_fn(2400, 1600, |x, y| {
        Rgb([
            (x * 255 / 2399) as u8,
            (y * 255 / 1599) as u8,
            ((x + y) * 255 / 3998) as u8,
        ])
    });
    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg, 88)
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
    for index in 0..100 {
        fs::write(photos.join(format!("photo-{index:03}.jpg")), &jpeg).unwrap();
    }

    let next = Arc::new(AtomicUsize::new(0));
    let active = Arc::new(AtomicUsize::new(0));
    let maximum_active = Arc::new(AtomicUsize::new(0));
    let started = Instant::now();
    std::thread::scope(|scope| {
        for _ in 0..3 {
            let next = next.clone();
            let active = active.clone();
            let maximum_active = maximum_active.clone();
            let photos = &photos;
            let cache = &cache;
            scope.spawn(move || {
                loop {
                    let index = next.fetch_add(1, Ordering::SeqCst);
                    if index >= 100 {
                        break;
                    }
                    let workers = active.fetch_add(1, Ordering::SeqCst) + 1;
                    maximum_active.fetch_max(workers, Ordering::SeqCst);
                    preview::load_thumbnail(photos, &format!("photo-{index:03}.jpg"), 220, cache)
                        .unwrap();
                    active.fetch_sub(1, Ordering::SeqCst);
                }
            });
        }
    });
    let elapsed = started.elapsed();
    let cache_files = fs::read_dir(&cache)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("jpg"))
        .count();
    eprintln!("generated {cache_files} thumbnails in {elapsed:?}");
    assert_eq!(maximum_active.load(Ordering::SeqCst), 3);
    assert_eq!(cache_files, 100);
    assert!(elapsed < Duration::from_secs(120));
}

#[test]
#[ignore = "manual cold/warm benchmark against FRAMEPAIR_BENCH_PHOTO_ROOT"]
fn benchmarks_real_high_resolution_preview_tiers() {
    let root = std::env::var_os("FRAMEPAIR_BENCH_PHOTO_ROOT")
        .map(std::path::PathBuf::from)
        .expect("set FRAMEPAIR_BENCH_PHOTO_ROOT");
    let cache = tempfile::tempdir().expect("benchmark cache");
    let mut photos = walkdir::WalkDir::new(&root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file() && formats::is_reference(entry.path()))
        .filter_map(|entry| {
            let size = entry.metadata().ok()?.len();
            let relative = entry
                .path()
                .strip_prefix(&root)
                .ok()?
                .to_string_lossy()
                .to_string();
            Some((size, relative))
        })
        .collect::<Vec<_>>();
    photos.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    photos.truncate(3);
    assert_eq!(photos.len(), 3, "benchmark requires at least three JPGs");

    for max_edge in [512, 1600, 2560] {
        let started = Instant::now();
        for (_, relative_path) in &photos {
            preview::load_thumbnail(&root, relative_path, max_edge, cache.path())
                .expect("cold preview tier");
        }
        eprintln!(
            "cold edge={max_edge} photos=3 elapsed={:?}",
            started.elapsed()
        );
    }

    let started = Instant::now();
    for (_, relative_path) in &photos {
        preview::load_thumbnail(&root, relative_path, 2560, cache.path())
            .expect("warm preview tier");
    }
    let warm_elapsed = started.elapsed();
    eprintln!("warm edge=2560 photos=3 elapsed={warm_elapsed:?}");
    assert!(warm_elapsed < Duration::from_secs(1));
}
