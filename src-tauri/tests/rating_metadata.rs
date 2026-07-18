#[path = "../src/rating_metadata.rs"]
mod rating_metadata;

use image::{GenericImageView, Rgb, RgbImage};
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

fn insert_app_segment(jpeg: &mut Vec<u8>, marker: u8, payload: &[u8]) {
    let length = u16::try_from(payload.len() + 2)
        .expect("APP payload length")
        .to_be_bytes();
    let mut segment = vec![0xff, marker, length[0], length[1]];
    segment.extend_from_slice(payload);
    jpeg.splice(2..2, segment);
}

fn insert_xmp_segment(jpeg: &mut Vec<u8>, xml: &[u8]) {
    const PREFIX: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
    let mut payload = PREFIX.to_vec();
    payload.extend_from_slice(xml);
    insert_app_segment(jpeg, 0xe1, &payload);
}

fn sos_tail(jpeg: &[u8]) -> &[u8] {
    let offset = jpeg
        .windows(2)
        .position(|window| window == [0xff, 0xda])
        .expect("SOS marker");
    &jpeg[offset..]
}

fn occurrences(input: &[u8], needle: &[u8]) -> usize {
    input
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count()
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

#[test]
fn rewrites_jpeg_xmp_without_changing_exif_app2_or_image_data() {
    let temp = tempfile::tempdir().expect("temp directory");
    let path = temp.path().join("source.jpg");
    RgbImage::from_pixel(8, 8, Rgb([20, 40, 60]))
        .save_with_format(&path, image::ImageFormat::Jpeg)
        .expect("jpeg");
    let mut jpeg = fs::read(&path).expect("jpeg bytes");
    let app2 = b"ICC_PROFILE\0FramePair-preserve";
    let exif = b"Exif\0\0FramePair-orientation";
    insert_app_segment(&mut jpeg, 0xe2, app2);
    insert_app_segment(&mut jpeg, 0xe1, exif);
    insert_xmp_segment(
        &mut jpeg,
        br#"<rdf:Description xmlns:rdf='rdf' xmlns:xmp='xmp' xmp:Rating='2' xmp:Label='Green'/>"#,
    );
    let original_tail = sos_tail(&jpeg).to_vec();

    let output = rating_metadata::rewrite_jpeg_rating(&jpeg, 4).expect("rewritten jpeg");
    fs::write(&path, &output).expect("output jpeg");

    assert_eq!(
        rating_metadata::read_jpeg_rating(&path).expect("jpeg rating"),
        Some(4)
    );
    assert!(output.windows(app2.len()).any(|window| window == app2));
    assert!(output.windows(exif.len()).any(|window| window == exif));
    assert_eq!(sos_tail(&output), original_tail);
    assert_eq!(
        image::load_from_memory(&output)
            .expect("decoded output")
            .dimensions(),
        (8, 8),
    );
    assert_eq!(occurrences(&output, b"http://ns.adobe.com/xap/1.0/\0"), 1,);
}

#[test]
fn inserts_jpeg_xmp_when_the_photo_has_none() {
    let temp = tempfile::tempdir().expect("temp directory");
    let path = temp.path().join("source.jpg");
    RgbImage::from_pixel(8, 8, Rgb([60, 40, 20]))
        .save_with_format(&path, image::ImageFormat::Jpeg)
        .expect("jpeg");
    let jpeg = fs::read(&path).expect("jpeg bytes");

    let output = rating_metadata::rewrite_jpeg_rating(&jpeg, 5).expect("jpeg with xmp");
    fs::write(&path, &output).expect("output jpeg");

    assert_eq!(
        rating_metadata::read_jpeg_rating(&path).expect("jpeg rating"),
        Some(5)
    );
    assert_eq!(&output[..2], &[0xff, 0xd8]);
    assert_eq!(&output[2..4], &[0xff, 0xe1]);
    assert_eq!(sos_tail(&output), sos_tail(&jpeg));
}

#[test]
fn rejects_ambiguous_or_malformed_jpeg_metadata() {
    let temp = tempfile::tempdir().expect("temp directory");
    let path = temp.path().join("source.jpg");
    RgbImage::from_pixel(8, 8, Rgb([10, 20, 30]))
        .save_with_format(&path, image::ImageFormat::Jpeg)
        .expect("jpeg");
    let base = fs::read(&path).expect("jpeg bytes");

    let mut duplicate = base.clone();
    insert_xmp_segment(&mut duplicate, br#"<rdf:Description xmp:Rating='2'/>"#);
    insert_xmp_segment(&mut duplicate, br#"<rdf:Description xmp:Rating='3'/>"#);
    assert!(rating_metadata::rewrite_jpeg_rating(&duplicate, 4).is_err());

    let mut malformed_xmp = base.clone();
    insert_xmp_segment(&mut malformed_xmp, b"<xmp:Rating>2");
    assert!(rating_metadata::rewrite_jpeg_rating(&malformed_xmp, 4).is_err());

    let malformed_length = vec![0xff, 0xd8, 0xff, 0xe1, 0x00, 0x01, 0xff, 0xd9];
    assert!(rating_metadata::rewrite_jpeg_rating(&malformed_length, 4).is_err());
}

#[test]
fn rejects_jpeg_xmp_that_would_overflow_the_app1_segment() {
    const PREFIX_LENGTH: usize = b"http://ns.adobe.com/xap/1.0/\0".len();
    let temp = tempfile::tempdir().expect("temp directory");
    let path = temp.path().join("source.jpg");
    RgbImage::from_pixel(8, 8, Rgb([10, 20, 30]))
        .save_with_format(&path, image::ImageFormat::Jpeg)
        .expect("jpeg");
    let mut jpeg = fs::read(&path).expect("jpeg bytes");
    let prefix = b"<rdf:Description xmlns:rdf='rdf' xmlns:xmp='xmp' xmp:Label='";
    let suffix = b"'/>";
    let xml_length = (u16::MAX as usize - 2) - PREFIX_LENGTH;
    let padding_length = xml_length - prefix.len() - suffix.len();
    let mut xml = prefix.to_vec();
    xml.extend(std::iter::repeat_n(b'x', padding_length));
    xml.extend_from_slice(suffix);
    insert_xmp_segment(&mut jpeg, &xml);

    assert!(rating_metadata::rewrite_jpeg_rating(&jpeg, 4).is_err());
}
