#![allow(dead_code)]
#[path = "../src/formats.rs"]
mod formats;
#[path = "../src/fs_util.rs"]
mod fs_util;
#[path = "../src/photo_groups.rs"]
mod photo_groups;
#[path = "../src/rating_metadata.rs"]
mod rating_metadata;
#[allow(dead_code)]
#[path = "../src/rating_sync.rs"]
mod rating_sync;
#[path = "../src/ratings.rs"]
mod ratings;

use photo_groups::RatingState;
use rating_sync::{
    AutoSyncStatus, PendingRatingSync, RatingConflictPolicy, RatingResolution,
    RatingSyncExecuteRequest, RatingSyncMode, RatingSyncPlanRequest, RatingSyncPlanStore,
    RatingSyncSettings, RatingSyncStatus, RatingSyncTarget, RatingSyncTargets,
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

fn indexed_plan(
    root: &std::path::Path,
    ratings: HashMap<String, u8>,
    mut request: RatingSyncPlanRequest,
    plan_id: &str,
) -> rating_sync::RatingSyncPlan {
    let mut index = photo_groups::index_directory(root).expect("photo index");
    photo_groups::apply_framepair_ratings(&mut index, &ratings);
    request.root = root.to_string_lossy().into_owned();
    rating_sync::build_plan(&index, &request, plan_id.to_string()).expect("rating sync plan")
}

fn execute_request(
    root: &std::path::Path,
    plan_id: &str,
    asset_ids: &[&str],
) -> RatingSyncExecuteRequest {
    RatingSyncExecuteRequest {
        plan_id: plan_id.to_string(),
        root: root.to_string_lossy().into_owned(),
        asset_ids: asset_ids.iter().map(|value| (*value).to_string()).collect(),
    }
}

#[test]
fn executes_new_and_existing_raw_xmp_writes_without_touching_raw_files() {
    let temp = tempfile::tempdir().expect("temp directory");
    let root = temp.path().join("photos");
    fs::create_dir_all(&root).expect("photo directory");
    fs::write(root.join("A.NEF"), b"raw-a-original").expect("first raw");
    fs::write(root.join("B.NEF"), b"raw-b-original").expect("second raw");
    fs::write(
        root.join("B.xmp"),
        br#"<rdf:Description xmlns:rdf='rdf' xmlns:xmp='xmp' xmp:Label='Green'><xmp:CreatorTool>Keep me</xmp:CreatorTool><xmp:Rating>2</xmp:Rating></rdf:Description>"#,
    )
    .expect("existing xmp");
    let mut request = raw_sync_request(&root);
    request.conflict_policy = RatingConflictPolicy::FramePair;
    let plan = indexed_plan(
        &root,
        HashMap::from([("a".to_string(), 4), ("b".to_string(), 5)]),
        request,
        "execute-raw",
    );

    let summary =
        rating_sync::execute_plan(&plan, &execute_request(&root, "execute-raw", &["a", "b"]))
            .expect("execution summary");

    assert_eq!(summary.succeeded, 2);
    assert_eq!(summary.failed, 0);
    assert_eq!(
        rating_metadata::read_sidecar_rating(&root.join("A.xmp")).expect("first rating"),
        Some(4),
    );
    assert_eq!(
        rating_metadata::read_sidecar_rating(&root.join("B.xmp")).expect("second rating"),
        Some(5),
    );
    assert!(
        fs::read_to_string(root.join("B.xmp"))
            .expect("xmp text")
            .contains("Keep me")
    );
    assert_eq!(
        fs::read(root.join("A.NEF")).expect("first raw bytes"),
        b"raw-a-original"
    );
    assert_eq!(
        fs::read(root.join("B.NEF")).expect("second raw bytes"),
        b"raw-b-original"
    );
}

#[test]
fn executes_confirmed_jpeg_metadata_write_without_reencoding_pixels() {
    use image::{GenericImageView, Rgb, RgbImage};

    let temp = tempfile::tempdir().expect("temp directory");
    let root = temp.path().join("photos");
    fs::create_dir_all(&root).expect("photo directory");
    let jpeg_path = root.join("A.JPG");
    RgbImage::from_pixel(8, 8, Rgb([20, 40, 60]))
        .save_with_format(&jpeg_path, image::ImageFormat::Jpeg)
        .expect("jpeg");
    let before = image::open(&jpeg_path).expect("before jpeg").dimensions();
    let request = RatingSyncPlanRequest {
        root: root.to_string_lossy().into_owned(),
        minimum_rating: 1,
        maximum_rating: 5,
        asset_ids: vec!["a".to_string()],
        targets: RatingSyncTargets {
            raw_xmp: false,
            jpeg_metadata: true,
        },
        conflict_policy: RatingConflictPolicy::FramePair,
        jpeg_write_confirmed: true,
    };
    let plan = indexed_plan(
        &root,
        HashMap::from([("a".to_string(), 4)]),
        request,
        "execute-jpeg",
    );

    let summary = rating_sync::execute_plan(&plan, &execute_request(&root, "execute-jpeg", &["a"]))
        .expect("execution summary");

    assert_eq!(summary.succeeded, 1);
    assert_eq!(summary.failed, 0);
    assert_eq!(
        rating_metadata::read_jpeg_rating(&jpeg_path).expect("jpeg rating"),
        Some(4)
    );
    assert_eq!(
        image::open(&jpeg_path).expect("after jpeg").dimensions(),
        before
    );
}

#[test]
fn stale_or_new_targets_fail_without_stopping_independent_groups() {
    let temp = tempfile::tempdir().expect("temp directory");
    let root = temp.path().join("photos");
    fs::create_dir_all(&root).expect("photo directory");
    fs::write(root.join("A.NEF"), b"raw-a").expect("first raw");
    fs::write(root.join("B.NEF"), b"raw-b").expect("second raw");
    let plan = indexed_plan(
        &root,
        HashMap::from([("a".to_string(), 4), ("b".to_string(), 5)]),
        raw_sync_request(&root),
        "stale",
    );
    fs::write(root.join("A.xmp"), br#"<xmp:Rating>1</xmp:Rating>"#).expect("appeared sidecar");

    let summary = rating_sync::execute_plan(&plan, &execute_request(&root, "stale", &["a", "b"]))
        .expect("partial execution summary");

    assert_eq!(summary.succeeded, 1);
    assert_eq!(summary.failed, 1);
    assert_eq!(
        rating_metadata::read_sidecar_rating(&root.join("A.xmp")).expect("unchanged first"),
        Some(1),
    );
    assert_eq!(
        rating_metadata::read_sidecar_rating(&root.join("B.xmp")).expect("second written"),
        Some(5),
    );
    assert!(summary.results.iter().any(|result| {
        result.asset_id == "a" && !result.success && result.message.contains("发生变化")
    }));
}

#[test]
fn execution_rejects_wrong_authorization_and_consumes_stored_plans_once() {
    let temp = tempfile::tempdir().expect("temp directory");
    let root = temp.path().join("photos");
    fs::create_dir_all(&root).expect("photo directory");
    fs::write(root.join("A.NEF"), b"raw").expect("raw");
    let plan = indexed_plan(
        &root,
        HashMap::from([("a".to_string(), 4)]),
        raw_sync_request(&root),
        "authorized",
    );

    assert!(
        rating_sync::execute_plan(&plan, &execute_request(&root, "wrong-plan", &["a"]),).is_err(),
    );
    assert!(
        rating_sync::execute_plan(&plan, &execute_request(temp.path(), "authorized", &["a"]),)
            .is_err(),
    );
    assert!(
        rating_sync::execute_plan(&plan, &execute_request(&root, "authorized", &["missing"]),)
            .is_err(),
    );

    let store = RatingSyncPlanStore::default();
    store.replace(plan).expect("stored plan");
    assert!(store.take("wrong-plan", &root).is_err());
    assert!(store.take("authorized", &root).is_ok());
    assert!(store.take("authorized", &root).is_err());
}

#[test]
fn execution_rejects_a_tampered_relative_path() {
    let temp = tempfile::tempdir().expect("temp directory");
    let root = temp.path().join("photos");
    fs::create_dir_all(&root).expect("photo directory");
    fs::write(root.join("A.NEF"), b"raw").expect("raw");
    let mut plan = indexed_plan(
        &root,
        HashMap::from([("a".to_string(), 4)]),
        raw_sync_request(&root),
        "tampered",
    );
    plan.writes[0].relative_path = "../outside.xmp".to_string();

    let summary = rating_sync::execute_plan(&plan, &execute_request(&root, "tampered", &["a"]))
        .expect("execution summary");

    assert_eq!(summary.succeeded, 0);
    assert_eq!(summary.failed, 1);
    assert!(!temp.path().join("outside.xmp").exists());
}

#[cfg(unix)]
#[test]
fn execution_rejects_a_sidecar_that_became_a_symlink() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("temp directory");
    let root = temp.path().join("photos");
    fs::create_dir_all(&root).expect("photo directory");
    fs::write(root.join("A.NEF"), b"raw").expect("raw");
    let plan = indexed_plan(
        &root,
        HashMap::from([("a".to_string(), 4)]),
        raw_sync_request(&root),
        "symlink",
    );
    let outside = temp.path().join("outside.xmp");
    fs::write(&outside, br#"<xmp:Rating>1</xmp:Rating>"#).expect("outside xmp");
    symlink(&outside, root.join("A.xmp")).expect("sidecar symlink");

    let summary = rating_sync::execute_plan(&plan, &execute_request(&root, "symlink", &["a"]))
        .expect("execution summary");

    assert_eq!(summary.failed, 1);
    assert_eq!(
        rating_metadata::read_sidecar_rating(&outside).expect("outside unchanged"),
        Some(1),
    );
}

#[test]
fn sync_settings_default_to_manual_raw_xmp_and_persist_valid_changes() {
    let temp = tempfile::tempdir().expect("temp directory");
    let database = temp.path().join("app-data/rating-sync.json");

    let initial = rating_sync::load_sync_state(&database, None).expect("default sync state");
    assert_eq!(initial.settings, RatingSyncSettings::default());
    assert_eq!(initial.settings.mode, RatingSyncMode::Manual);
    assert_eq!(
        initial.settings.targets,
        RatingSyncTargets {
            raw_xmp: true,
            jpeg_metadata: false,
        },
    );
    assert_eq!(initial.settings.conflict_policy, RatingConflictPolicy::Skip);
    assert!(!initial.settings.jpeg_write_confirmed);

    let settings = RatingSyncSettings {
        mode: RatingSyncMode::Automatic,
        targets: RatingSyncTargets {
            raw_xmp: true,
            jpeg_metadata: true,
        },
        conflict_policy: RatingConflictPolicy::FramePair,
        jpeg_write_confirmed: true,
    };
    assert_eq!(
        rating_sync::save_sync_settings(&database, &settings).expect("saved settings"),
        settings,
    );
    assert_eq!(
        rating_sync::load_sync_state(&database, None)
            .expect("stored sync state")
            .settings,
        settings,
    );
}

#[test]
fn invalid_settings_or_untrusted_databases_are_never_overwritten() {
    let temp = tempfile::tempdir().expect("temp directory");
    let database = temp.path().join("rating-sync.json");
    let no_targets = RatingSyncSettings {
        mode: RatingSyncMode::Automatic,
        targets: RatingSyncTargets {
            raw_xmp: false,
            jpeg_metadata: false,
        },
        ..RatingSyncSettings::default()
    };
    assert!(rating_sync::save_sync_settings(&database, &no_targets).is_err());

    let unconfirmed_jpeg = RatingSyncSettings {
        targets: RatingSyncTargets {
            raw_xmp: false,
            jpeg_metadata: true,
        },
        jpeg_write_confirmed: false,
        ..RatingSyncSettings::default()
    };
    assert!(rating_sync::save_sync_settings(&database, &unconfirmed_jpeg).is_err());

    fs::write(&database, b"not json").expect("damaged database");
    assert!(rating_sync::save_sync_settings(&database, &RatingSyncSettings::default()).is_err(),);
    assert_eq!(
        fs::read(&database).expect("damaged bytes remain"),
        b"not json"
    );

    fs::write(&database, br#"{"version":99,"settings":{},"pending":[]}"#)
        .expect("unknown database");
    assert!(rating_sync::load_sync_state(&database, None).is_err());

    fs::write(&database, vec![b'x'; 4 * 1024 * 1024 + 1]).expect("oversized database");
    assert!(rating_sync::load_sync_state(&database, None).is_err());
}

#[cfg(unix)]
#[test]
fn symlinked_sync_database_is_rejected() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("temp directory");
    let target = temp.path().join("target.json");
    let database = temp.path().join("rating-sync.json");
    fs::write(&target, br#"{"version":1}"#).expect("target database");
    symlink(&target, &database).expect("database symlink");

    assert!(rating_sync::load_sync_state(&database, None).is_err());
    assert!(rating_sync::save_sync_settings(&database, &RatingSyncSettings::default()).is_err(),);
}

#[test]
fn pending_failures_are_filtered_updated_and_cleared_by_root_and_asset() {
    let temp = tempfile::tempdir().expect("temp directory");
    let database = temp.path().join("app-data/rating-sync.json");
    let first_root = temp.path().join("first");
    let second_root = temp.path().join("second");
    fs::create_dir_all(&first_root).expect("first root");
    fs::create_dir_all(&second_root).expect("second root");
    let pending = |root: &std::path::Path, error: &str, failed_at_ms| PendingRatingSync {
        root: root.to_string_lossy().into_owned(),
        asset_id: "a".to_string(),
        rating: 4,
        targets: RatingSyncTargets {
            raw_xmp: true,
            jpeg_metadata: false,
        },
        error: error.to_string(),
        failed_at_ms,
    };

    rating_sync::record_pending(&database, pending(&first_root, "第一次失败", 100))
        .expect("first pending");
    rating_sync::record_pending(&database, pending(&first_root, "更新后的失败", 200))
        .expect("updated pending");
    rating_sync::record_pending(&database, pending(&second_root, "另一个目录", 300))
        .expect("second root pending");

    let first =
        rating_sync::load_sync_state(&database, Some(&first_root)).expect("first root state");
    assert_eq!(first.pending.len(), 1);
    assert_eq!(first.pending[0].error, "更新后的失败");
    assert_eq!(first.pending[0].failed_at_ms, 200);

    rating_sync::clear_pending(&database, &first_root, "a").expect("clear first pending");
    assert!(
        rating_sync::load_sync_state(&database, Some(&first_root))
            .expect("cleared state")
            .pending
            .is_empty(),
    );
    assert_eq!(
        rating_sync::load_sync_state(&database, Some(&second_root))
            .expect("second root remains")
            .pending
            .len(),
        1,
    );
}

fn indexed_with_saved_ratings(
    root: &std::path::Path,
    database: &std::path::Path,
) -> photo_groups::PhotoIndex {
    let mut index = photo_groups::index_directory(root).expect("photo index");
    let saved = ratings::load_ratings(database, root).expect("saved ratings");
    photo_groups::apply_framepair_ratings(&mut index, &saved);
    index
}

#[test]
fn manual_mode_never_writes_external_metadata() {
    let temp = tempfile::tempdir().expect("temp directory");
    let root = temp.path().join("photos");
    let sync_database = temp.path().join("app-data/rating-sync.json");
    fs::create_dir_all(&root).expect("photo directory");
    fs::write(root.join("A.NEF"), b"raw").expect("raw");
    let mut index = photo_groups::index_directory(&root).expect("photo index");
    photo_groups::apply_framepair_ratings(&mut index, &HashMap::from([("a".to_string(), 4)]));

    let outcome = rating_sync::auto_sync_saved_rating(
        &sync_database,
        &index,
        &RatingSyncSettings::default(),
        &root,
        "a",
        4,
        "manual",
        100,
    );

    assert_eq!(outcome.status, AutoSyncStatus::Disabled);
    assert!(!root.join("A.xmp").exists());
}

#[test]
fn automatic_raw_sync_writes_metadata_and_clears_pending_state() {
    let temp = tempfile::tempdir().expect("temp directory");
    let root = temp.path().join("photos");
    let sync_database = temp.path().join("app-data/rating-sync.json");
    fs::create_dir_all(&root).expect("photo directory");
    fs::write(root.join("A.NEF"), b"raw").expect("raw");
    let settings = RatingSyncSettings {
        mode: RatingSyncMode::Automatic,
        conflict_policy: RatingConflictPolicy::FramePair,
        ..RatingSyncSettings::default()
    };
    rating_sync::save_sync_settings(&sync_database, &settings).expect("saved settings");
    rating_sync::record_pending(
        &sync_database,
        PendingRatingSync {
            root: root.to_string_lossy().into_owned(),
            asset_id: "a".to_string(),
            rating: 3,
            targets: settings.targets,
            error: "旧失败".to_string(),
            failed_at_ms: 50,
        },
    )
    .expect("old pending");
    let mut index = photo_groups::index_directory(&root).expect("photo index");
    photo_groups::apply_framepair_ratings(&mut index, &HashMap::from([("a".to_string(), 4)]));

    let outcome = rating_sync::auto_sync_saved_rating(
        &sync_database,
        &index,
        &settings,
        &root,
        "a",
        4,
        "automatic",
        100,
    );

    assert_eq!(outcome.status, AutoSyncStatus::Synced);
    assert_eq!(
        rating_metadata::read_sidecar_rating(&root.join("A.xmp")).expect("xmp rating"),
        Some(4),
    );
    assert!(
        rating_sync::load_sync_state(&sync_database, Some(&root))
            .expect("sync state")
            .pending
            .is_empty(),
    );
}

#[test]
fn automatic_conflict_keeps_framepair_rating_and_records_retryable_pending_state() {
    let temp = tempfile::tempdir().expect("temp directory");
    let root = temp.path().join("photos");
    let rating_database = temp.path().join("app-data/photo-ratings.json");
    let sync_database = temp.path().join("app-data/rating-sync.json");
    fs::create_dir_all(&root).expect("photo directory");
    fs::write(root.join("A.NEF"), b"raw").expect("raw");
    fs::write(root.join("A.xmp"), br#"<xmp:Rating>5</xmp:Rating>"#).expect("external rating");
    ratings::set_rating(&rating_database, &root, "A.NEF", 4).expect("FramePair saved first");
    let settings = RatingSyncSettings {
        mode: RatingSyncMode::Automatic,
        conflict_policy: RatingConflictPolicy::Skip,
        ..RatingSyncSettings::default()
    };
    rating_sync::save_sync_settings(&sync_database, &settings).expect("saved settings");
    let index = indexed_with_saved_ratings(&root, &rating_database);

    let outcome = rating_sync::auto_sync_saved_rating(
        &sync_database,
        &index,
        &settings,
        &root,
        "a",
        4,
        "conflict",
        200,
    );

    assert_eq!(outcome.status, AutoSyncStatus::Pending);
    assert_eq!(
        ratings::load_ratings(&rating_database, &root).expect("FramePair remains")["a"],
        4
    );
    assert_eq!(
        rating_metadata::read_sidecar_rating(&root.join("A.xmp")).expect("external unchanged"),
        Some(5),
    );
    let pending = rating_sync::load_sync_state(&sync_database, Some(&root))
        .expect("pending state")
        .pending;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].asset_id, "a");
    assert_eq!(pending[0].rating, 4);
    assert_eq!(pending[0].failed_at_ms, 200);
}

#[test]
fn automatic_retry_can_clear_an_external_rating_to_zero() {
    let temp = tempfile::tempdir().expect("temp directory");
    let root = temp.path().join("photos");
    let sync_database = temp.path().join("app-data/rating-sync.json");
    fs::create_dir_all(&root).expect("photo directory");
    fs::write(root.join("A.NEF"), b"raw").expect("raw");
    fs::write(root.join("A.xmp"), br#"<xmp:Rating>5</xmp:Rating>"#).expect("external rating");
    let settings = RatingSyncSettings {
        mode: RatingSyncMode::Automatic,
        conflict_policy: RatingConflictPolicy::FramePair,
        ..RatingSyncSettings::default()
    };
    let index = photo_groups::index_directory(&root).expect("photo index");

    let outcome = rating_sync::auto_sync_saved_rating(
        &sync_database,
        &index,
        &settings,
        &root,
        "a",
        0,
        "clear",
        300,
    );

    assert_eq!(outcome.status, AutoSyncStatus::Synced);
    assert_eq!(
        rating_metadata::read_sidecar_rating(&root.join("A.xmp")).expect("cleared xmp"),
        Some(0),
    );
}
