#![allow(dead_code)]

#[path = "../src/watermark_model.rs"]
mod watermark_model;
#[path = "../src/watermark_resource.rs"]
mod watermark_resource;

use image::{Rgba, RgbaImage};

#[test]
fn imports_a_bounded_png_as_an_embedded_template_resource() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("brand.png");
    RgbaImage::from_pixel(48, 24, Rgba([20, 80, 160, 180]))
        .save(&path)
        .unwrap();
    let resource = watermark_resource::import_image_resource(&path).unwrap();
    assert_eq!(resource.name, "brand.png");
    assert_eq!(resource.mime_type, "image/png");
    assert_eq!((resource.width, resource.height), (48, 24));
    assert_eq!(resource.sha256.len(), 64);
    assert!(!resource.data_base64.is_empty());
}

#[test]
fn refuses_non_image_resources() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("brand.txt");
    std::fs::write(&path, b"not an image").unwrap();
    assert!(watermark_resource::import_image_resource(&path).is_err());
}
