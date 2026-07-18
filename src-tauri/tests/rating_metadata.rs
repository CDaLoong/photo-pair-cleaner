#[path = "../src/rating_metadata.rs"]
mod rating_metadata;

use image::{Rgb, RgbImage};
use std::fs;
use std::path::Path;

fn add_xmp(path: &Path, xml: &[u8]) {
    const PREFIX: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
    let mut jpeg = fs::read(path).expect("jpeg bytes");
    assert_eq!(&jpeg[..2], &[0xff, 0xd8]);
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
fn reads_attribute_and_element_ratings() {
    assert_eq!(
        rating_metadata::xmp_rating(br#"<rdf:Description xmp:Rating="5"/>"#)
            .expect("attribute rating"),
        Some(5),
    );
    assert_eq!(
        rating_metadata::xmp_rating(br#"<xmp:Rating>4</xmp:Rating>"#).expect("element rating"),
        Some(4),
    );
}

#[test]
fn accepts_rejected_and_absent_external_states() {
    assert_eq!(
        rating_metadata::xmp_rating(br#"<xmp:Rating>-1</xmp:Rating>"#).expect("rejected rating"),
        Some(-1),
    );
    assert_eq!(
        rating_metadata::xmp_rating(b"<x:xmpmeta/>").expect("absent rating"),
        None,
    );
}

#[test]
fn rejects_invalid_or_duplicate_ratings() {
    assert!(rating_metadata::xmp_rating(br#"<xmp:Rating>9</xmp:Rating>"#).is_err());
    assert!(
        rating_metadata::xmp_rating(
            br#"<x:xmpmeta><xmp:Rating>4</xmp:Rating><xmp:Rating>5</xmp:Rating></x:xmpmeta>"#,
        )
        .is_err(),
    );
}

#[test]
fn reads_a_bounded_sidecar_rating() {
    let temp = tempfile::tempdir().expect("temp directory");
    let path = temp.path().join("A.xmp");
    fs::write(&path, br#"<rdf:Description xmp:Rating="3"/>"#).expect("xmp");

    assert_eq!(
        rating_metadata::read_sidecar_rating(&path).expect("sidecar rating"),
        Some(3),
    );
}

#[test]
fn reads_an_embedded_jpeg_xmp_rating_without_decoding_pixels() {
    let temp = tempfile::tempdir().expect("temp directory");
    let path = temp.path().join("A.jpg");
    RgbImage::from_pixel(8, 8, Rgb([20, 40, 60]))
        .save_with_format(&path, image::ImageFormat::Jpeg)
        .expect("jpeg");
    add_xmp(&path, br#"<rdf:Description xmp:Rating="4"/>"#);

    assert_eq!(
        rating_metadata::read_jpeg_rating(&path).expect("jpeg rating"),
        Some(4),
    );
}

#[test]
fn rejects_oversized_or_malformed_sidecars() {
    let temp = tempfile::tempdir().expect("temp directory");
    let oversized = temp.path().join("large.xmp");
    let malformed = temp.path().join("broken.xmp");
    fs::write(&oversized, vec![b'x'; 4 * 1024 * 1024 + 1]).expect("large xmp");
    fs::write(&malformed, b"<xmp:Rating>4").expect("broken xmp");

    assert!(rating_metadata::read_sidecar_rating(&oversized).is_err());
    assert!(rating_metadata::read_sidecar_rating(&malformed).is_err());
}

#[cfg(unix)]
#[test]
fn rejects_symlinked_sidecars() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("temp directory");
    let target = temp.path().join("target.xmp");
    let link = temp.path().join("linked.xmp");
    fs::write(&target, br#"<xmp:Rating>5</xmp:Rating>"#).expect("target xmp");
    symlink(&target, &link).expect("xmp symlink");

    assert!(rating_metadata::read_sidecar_rating(&link).is_err());
}

#[test]
fn rewrites_element_rating_without_losing_other_xmp_metadata() {
    let input = br#"<?xpacket begin='x'?><x:xmpmeta xmlns:x='adobe:ns:meta/'><rdf:RDF xmlns:rdf='http://www.w3.org/1999/02/22-rdf-syntax-ns#'><rdf:Description xmlns:xmp='http://ns.adobe.com/xap/1.0/' xmp:Label='Green'><xmp:CreatorTool>FramePair test</xmp:CreatorTool><xmp:Rating>2</xmp:Rating></rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end='w'?>"#;

    let output = rating_metadata::rewrite_xmp_rating(Some(input), 5).expect("rewritten xmp");
    let text = String::from_utf8(output.clone()).expect("utf8 xmp");

    assert_eq!(
        rating_metadata::xmp_rating(&output).expect("rating"),
        Some(5)
    );
    assert!(text.contains("FramePair test"));
    assert!(text.contains("Green"));
    assert!(!text.contains(">2<"));
}

#[test]
fn rewrites_attribute_rating_and_inserts_a_missing_rating() {
    let attribute =
        br#"<rdf:Description xmlns:rdf='rdf' xmlns:xmp='xmp' xmp:Label='Blue' xmp:Rating='2'/>"#;
    let missing = br#"<rdf:Description xmlns:rdf='rdf' xmlns:xmp='xmp' xmp:Label='Blue'/>"#;

    let updated =
        rating_metadata::rewrite_xmp_rating(Some(attribute), 3).expect("updated attribute rating");
    let inserted =
        rating_metadata::rewrite_xmp_rating(Some(missing), 4).expect("inserted attribute rating");

    assert_eq!(
        rating_metadata::xmp_rating(&updated).expect("updated rating"),
        Some(3)
    );
    assert_eq!(
        rating_metadata::xmp_rating(&inserted).expect("inserted rating"),
        Some(4)
    );
    assert!(String::from_utf8_lossy(&updated).contains("Blue"));
    assert!(String::from_utf8_lossy(&inserted).contains("Blue"));
}

#[test]
fn creates_a_standard_xmp_packet_and_supports_zero_rating() {
    let created = rating_metadata::rewrite_xmp_rating(None, 5).expect("new xmp");
    let cleared = rating_metadata::rewrite_xmp_rating(Some(&created), 0).expect("zero rating");
    let text = String::from_utf8(created).expect("utf8 xmp");

    assert!(text.contains("adobe:ns:meta/"));
    assert!(text.contains("http://www.w3.org/1999/02/22-rdf-syntax-ns#"));
    assert!(text.contains("http://ns.adobe.com/xap/1.0/"));
    assert_eq!(
        rating_metadata::xmp_rating(&cleared).expect("cleared rating"),
        Some(0)
    );
}

#[test]
fn rejects_unsafe_xmp_rewrite_inputs() {
    let duplicate =
        br#"<rdf:Description xmp:Rating='2'><xmp:Rating>3</xmp:Rating></rdf:Description>"#;
    let rejected = br#"<rdf:Description><xmp:Rating>-1</xmp:Rating></rdf:Description>"#;
    let no_description = br#"<x:xmpmeta/>"#;
    let oversized = vec![b'x'; 4 * 1024 * 1024 + 1];

    assert!(rating_metadata::rewrite_xmp_rating(Some(duplicate), 4).is_err());
    assert!(rating_metadata::rewrite_xmp_rating(Some(rejected), 4).is_err());
    assert!(rating_metadata::rewrite_xmp_rating(Some(no_description), 4).is_err());
    assert!(rating_metadata::rewrite_xmp_rating(Some(&oversized), 4).is_err());
    assert!(rating_metadata::rewrite_xmp_rating(None, 6).is_err());
}
