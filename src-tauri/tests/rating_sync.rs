#[path = "../src/formats.rs"]
mod formats;
#[path = "../src/photo_groups.rs"]
mod photo_groups;
#[path = "../src/rating_metadata.rs"]
mod rating_metadata;
#[allow(dead_code)]
#[path = "../src/rating_sync.rs"]
mod rating_sync;

use photo_groups::RatingState;
use rating_sync::{
    RatingConflictPolicy, RatingResolution, RatingSyncPlanRequest, RatingSyncStatus,
    RatingSyncTarget, RatingSyncTargets,
};
use std::collections::HashMap;
use std::fs;

fn state(frame_pair: u8, jpeg_metadata: Option<i8>, raw_xmp: Option<i8>) -> RatingState {
    RatingState {
        frame_pair,
        jpeg_metadata,
        raw_xmp,
        resolved: 0,
        conflict: false,
    }
}

#[test]
fn conflict_policies_resolve_only_the_sources_the_user_selected() {
    let conflicting = state(3, Some(4), Some(5));

    assert_eq!(
        rating_sync::resolve_rating(&conflicting, &[], RatingConflictPolicy::Skip),
        RatingResolution::Conflict,
    );
    assert_eq!(
        rating_sync::resolve_rating(&conflicting, &[], RatingConflictPolicy::FramePair),
        RatingResolution::Ready(3),
    );
    assert_eq!(
        rating_sync::resolve_rating(&conflicting, &[], RatingConflictPolicy::Highest),
        RatingResolution::Ready(5),
    );
    assert_eq!(
        rating_sync::resolve_rating(&conflicting, &[], RatingConflictPolicy::External),
        RatingResolution::Conflict,
    );
}

#[test]
fn external_policy_uses_a_single_or_equal_external_source() {
    assert_eq!(
        rating_sync::resolve_rating(
            &state(3, Some(4), None),
            &[],
            RatingConflictPolicy::External,
        ),
        RatingResolution::Ready(4),
    );
    assert_eq!(
        rating_sync::resolve_rating(
            &state(3, Some(4), Some(4)),
            &[],
            RatingConflictPolicy::External,
        ),
        RatingResolution::Ready(4),
    );
    assert_eq!(
        rating_sync::resolve_rating(&state(3, None, None), &[], RatingConflictPolicy::External,),
        RatingResolution::Ready(3),
    );
}

#[test]
fn skip_accepts_equal_sources_and_zero_is_a_real_resolution() {
    assert_eq!(
        rating_sync::resolve_rating(&state(4, Some(4), Some(4)), &[], RatingConflictPolicy::Skip,),
        RatingResolution::Ready(4),
    );
    assert_eq!(
        rating_sync::resolve_rating(&state(0, None, None), &[], RatingConflictPolicy::Skip,),
        RatingResolution::Ready(0),
    );
}

#[test]
fn unsupported_or_unsafe_metadata_is_a_hard_conflict() {
    for policy in [
        RatingConflictPolicy::Skip,
        RatingConflictPolicy::FramePair,
        RatingConflictPolicy::External,
        RatingConflictPolicy::Highest,
    ] {
        assert_eq!(
            rating_sync::resolve_rating(&state(5, None, Some(-1)), &[], policy),
            RatingResolution::Conflict,
        );
        assert_eq!(
            rating_sync::resolve_rating(
                &state(5, None, None),
                &["XMP 文件已损坏".to_string()],
                policy,
            ),
            RatingResolution::Conflict,
        );
    }
}

fn raw_sync_request(root: &std::path::Path) -> RatingSyncPlanRequest {
    RatingSyncPlanRequest {
        root: root.to_string_lossy().into_owned(),
        minimum_rating: 1,
        maximum_rating: 5,
        asset_ids: Vec::new(),
        targets: RatingSyncTargets {
            raw_xmp: true,
            jpeg_metadata: false,
        },
        conflict_policy: RatingConflictPolicy::Skip,
        jpeg_write_confirmed: false,
    }
}

#[test]
fn read_only_plan_distinguishes_ready_unchanged_and_conflict_groups() {
    let temp = tempfile::tempdir().expect("temp directory");
    let root = temp.path().join("photos");
    fs::create_dir_all(&root).expect("photo directory");
    fs::write(root.join("A.NEF"), b"raw-a").expect("first raw");
    fs::write(root.join("B.NEF"), b"raw-b").expect("second raw");
    fs::write(root.join("B.xmp"), br#"<xmp:Rating>4</xmp:Rating>"#).expect("equal xmp");
    fs::write(root.join("C.NEF"), b"raw-c").expect("third raw");
    fs::write(root.join("C.xmp"), br#"<xmp:Rating>5</xmp:Rating>"#).expect("conflicting xmp");

    let mut index = photo_groups::index_directory(&root).expect("photo index");
    photo_groups::apply_framepair_ratings(
        &mut index,
        &HashMap::from([
            ("a".to_string(), 4),
            ("b".to_string(), 4),
            ("c".to_string(), 4),
        ]),
    );

    let plan = rating_sync::build_plan(&index, &raw_sync_request(&root), "plan-1".to_string())
        .expect("rating sync plan");
    let summary = plan.summary();

    assert_eq!(summary.plan_id, "plan-1");
    assert_eq!(summary.total_items, 3);
    assert_eq!(summary.ready, 1);
    assert_eq!(summary.unchanged, 1);
    assert_eq!(summary.conflicts, 1);
    assert_eq!(
        summary
            .items
            .iter()
            .map(|item| (item.asset_id.as_str(), item.status))
            .collect::<Vec<_>>(),
        [
            ("a", RatingSyncStatus::Ready),
            ("b", RatingSyncStatus::Unchanged),
            ("c", RatingSyncStatus::Conflict),
        ],
    );
    assert_eq!(summary.items[0].resolved, Some(4));
    assert_eq!(summary.items[0].writes.len(), 1);
    assert_eq!(summary.items[0].writes[0].target, RatingSyncTarget::RawXmp);
    assert_eq!(summary.items[0].writes[0].relative_path, "A.xmp");
    assert_eq!(summary.items[0].writes[0].current_rating, None);
    assert_eq!(summary.items[0].writes[0].target_rating, 4);
    assert!(!root.join("A.xmp").exists(), "planning must stay read-only");
}

#[test]
fn plan_request_validation_rejects_unsafe_or_ambiguous_configuration() {
    let temp = tempfile::tempdir().expect("temp directory");
    let root = temp.path().join("photos");
    fs::create_dir_all(&root).expect("photo directory");
    fs::write(root.join("A.NEF"), b"raw").expect("raw");
    let index = photo_groups::index_directory(&root).expect("photo index");

    let mut request = raw_sync_request(&root);
    request.targets.raw_xmp = false;
    assert!(rating_sync::build_plan(&index, &request, "empty".to_string()).is_err());

    let mut request = raw_sync_request(&root);
    request.minimum_rating = 5;
    request.maximum_rating = 2;
    assert!(rating_sync::build_plan(&index, &request, "range".to_string()).is_err());

    let mut request = raw_sync_request(&root);
    request.targets.raw_xmp = false;
    request.targets.jpeg_metadata = true;
    assert!(rating_sync::build_plan(&index, &request, "jpeg".to_string()).is_err());

    let mut request = raw_sync_request(&root);
    request.root = temp.path().to_string_lossy().into_owned();
    assert!(rating_sync::build_plan(&index, &request, "root".to_string()).is_err());

    let mut request = raw_sync_request(&root);
    request.asset_ids = vec!["missing".to_string()];
    assert!(rating_sync::build_plan(&index, &request, "missing".to_string()).is_err());
}

#[test]
fn duplicate_xmp_targets_are_reported_inside_the_group() {
    let temp = tempfile::tempdir().expect("temp directory");
    let root = temp.path().join("photos");
    fs::create_dir_all(&root).expect("photo directory");
    fs::write(root.join("A.NEF"), b"raw").expect("raw");
    fs::write(root.join("A.xmp"), br#"<xmp:Rating>4</xmp:Rating>"#).expect("stem xmp");
    fs::write(root.join("A.NEF.xmp"), br#"<xmp:Rating>4</xmp:Rating>"#).expect("extension xmp");
    let mut index = photo_groups::index_directory(&root).expect("photo index");
    photo_groups::apply_framepair_ratings(&mut index, &HashMap::from([("a".to_string(), 4)]));

    let plan = rating_sync::build_plan(&index, &raw_sync_request(&root), "ambiguous".to_string())
        .expect("conflict remains reviewable");

    assert_eq!(plan.summary().conflicts, 1);
    assert_eq!(plan.summary().items[0].status, RatingSyncStatus::Conflict);
    assert!(plan.summary().items[0].writes.is_empty());
    assert!(
        plan.summary().items[0]
            .issues
            .iter()
            .any(|issue| issue.contains("多个 XMP")),
    );
}
