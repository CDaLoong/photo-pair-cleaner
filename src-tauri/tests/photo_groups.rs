#[path = "../src/formats.rs"]
mod formats;
#[allow(dead_code)]
#[path = "../src/photo_groups.rs"]
mod photo_groups;
#[allow(dead_code)]
#[path = "../src/rating_metadata.rs"]
mod rating_metadata;

use image::{Rgb, RgbImage};
use std::fs;
use std::path::Path;

fn add_xmp(path: &Path, xml: &[u8]) {
    const PREFIX: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
    let mut jpeg = fs::read(path).expect("jpeg bytes");
    let mut payload = PREFIX.to_vec();
    payload.extend_from_slice(xml);
    let length = u16::try_from(payload.len() + 2)
        .expect("APP1 payload length")
        .to_be_bytes();
    let mut app1 = vec![0xff, 0xe1, length[0], length[1]];
    app1.extend_from_slice(&payload);
    jpeg.splice(2..2, app1);
    fs::write(path, jpeg).expect("jpeg with XMP");
}

#[test]
fn groups_jpeg_raw_and_sidecar_members_by_relative_stem() {
    let temp = tempfile::tempdir().expect("temp root");
    let root = temp.path().join("photos");
    fs::create_dir_all(root.join("day")).expect("photo directory");
    fs::write(root.join("day/A.JPG"), b"jpeg").expect("jpeg");
    fs::write(root.join("day/A.NEF"), b"raw").expect("raw");
    fs::write(root.join("day/A.xmp"), br#"<xmp:Rating>4</xmp:Rating>"#).expect("xmp");

    let index = photo_groups::index_directory(&root).expect("photo index");

    assert_eq!(index.total_assets, 1);
    assert_eq!(index.paired_assets, 1);
    let group = &index.assets[0];
    assert_eq!(group.id, "day/a");
    assert_eq!(group.jpeg_paths, ["day/A.JPG"]);
    assert_eq!(group.raw_paths, ["day/A.NEF"]);
    assert_eq!(group.xmp_paths, ["day/A.xmp"]);
    assert_eq!(group.members.len(), 3);
    assert_eq!(group.size_bytes, 7);
}

#[test]
fn double_extension_sidecars_keep_their_path_and_join_the_photo() {
    let temp = tempfile::tempdir().expect("temp root");
    let root = temp.path().join("photos");
    fs::create_dir_all(&root).expect("photo directory");
    fs::write(root.join("A.CR3"), b"raw").expect("raw");
    fs::write(root.join("A.CR3.xmp"), br#"<xmp:Rating>5</xmp:Rating>"#).expect("xmp");

    let index = photo_groups::index_directory(&root).expect("photo index");

    assert_eq!(index.total_assets, 1);
    assert_eq!(index.assets[0].id, "a");
    assert_eq!(index.assets[0].xmp_paths, ["A.CR3.xmp"]);
    assert_eq!(index.assets[0].members.len(), 2);
}

#[test]
fn reports_external_ratings_without_changing_the_framepair_score() {
    let temp = tempfile::tempdir().expect("temp root");
    let root = temp.path().join("photos");
    fs::create_dir_all(&root).expect("photo directory");
    let jpeg = root.join("A.JPG");
    RgbImage::from_pixel(8, 8, Rgb([30, 60, 90]))
        .save_with_format(&jpeg, image::ImageFormat::Jpeg)
        .expect("jpeg");
    add_xmp(&jpeg, br#"<rdf:Description xmp:Rating="4"/>"#);
    fs::write(root.join("A.NEF"), b"raw").expect("raw");
    fs::write(root.join("A.xmp"), br#"<xmp:Rating>5</xmp:Rating>"#).expect("xmp");

    let index = photo_groups::index_directory(&root).expect("photo index");
    let group = &index.assets[0];

    assert_eq!(group.rating_state.jpeg_metadata, Some(4));
    assert_eq!(group.rating_state.raw_xmp, Some(5));
    assert_eq!(group.rating_state.frame_pair, 0);
    assert_eq!(group.rating_state.resolved, 0);
    assert!(group.rating_state.conflict);
    assert_eq!(group.rating, 0);
}

#[test]
fn equal_external_ratings_are_not_a_source_conflict() {
    let temp = tempfile::tempdir().expect("temp root");
    let root = temp.path().join("photos");
    fs::create_dir_all(&root).expect("photo directory");
    let jpeg = root.join("A.JPG");
    RgbImage::from_pixel(8, 8, Rgb([30, 60, 90]))
        .save_with_format(&jpeg, image::ImageFormat::Jpeg)
        .expect("jpeg");
    add_xmp(&jpeg, br#"<rdf:Description xmp:Rating="4"/>"#);
    fs::write(root.join("A.NEF"), b"raw").expect("raw");
    fs::write(root.join("A.xmp"), br#"<xmp:Rating>4</xmp:Rating>"#).expect("xmp");

    let index = photo_groups::index_directory(&root).expect("photo index");
    let group = &index.assets[0];

    assert_eq!(group.rating_state.jpeg_metadata, Some(4));
    assert_eq!(group.rating_state.raw_xmp, Some(4));
    assert!(!group.rating_state.conflict);
    assert!(group.rating_issues.is_empty());
}

#[test]
fn duplicate_or_invalid_xmp_isolated_to_the_photo_group() {
    let temp = tempfile::tempdir().expect("temp root");
    let root = temp.path().join("photos");
    fs::create_dir_all(&root).expect("photo directory");
    fs::write(root.join("A.NEF"), b"raw").expect("raw");
    fs::write(root.join("A.xmp"), br#"<xmp:Rating>4</xmp:Rating>"#).expect("xmp");
    fs::write(root.join("A.NEF.xmp"), b"<xmp:Rating>broken</xmp:Rating>").expect("broken xmp");

    let index = photo_groups::index_directory(&root).expect("photo index");
    let group = &index.assets[0];

    assert!(group.rating_state.conflict);
    assert!(!group.rating_issues.is_empty());
    assert_eq!(group.xmp_paths.len(), 2);
}

#[test]
fn rejected_external_rating_is_reported_as_unsupported_conflict() {
    let temp = tempfile::tempdir().expect("temp root");
    let root = temp.path().join("photos");
    fs::create_dir_all(&root).expect("photo directory");
    fs::write(root.join("A.RAF"), b"raw").expect("raw");
    fs::write(root.join("A.xmp"), br#"<xmp:Rating>-1</xmp:Rating>"#).expect("xmp");

    let index = photo_groups::index_directory(&root).expect("photo index");
    let group = &index.assets[0];

    assert_eq!(group.rating_state.raw_xmp, Some(-1));
    assert!(group.rating_state.conflict);
    assert!(group.rating_issues.iter().any(|issue| issue.contains("-1")));
}
