#[path = "../src/rating_metadata.rs"]
mod rating_metadata;

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
