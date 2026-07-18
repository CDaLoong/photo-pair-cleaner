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
        .filter(|entry| entry.path().is_file())
        .count();
    eprintln!("generated {cache_files} thumbnails in {elapsed:?}");
    assert_eq!(maximum_active.load(Ordering::SeqCst), 3);
    assert_eq!(cache_files, 100);
    assert!(elapsed < Duration::from_secs(120));
}
