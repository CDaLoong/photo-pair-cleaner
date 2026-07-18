#[path = "../src/formats.rs"]
mod formats;
#[path = "../src/photo_groups.rs"]
mod photo_groups;

use std::fs;

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
