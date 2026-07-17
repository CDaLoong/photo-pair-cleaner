#[path = "../src/safety.rs"]
mod safety;

use safety::{CleanupPlan, FileSnapshot, unique_keys};
use std::path::PathBuf;

#[test]
fn repeated_missing_keys_are_deduplicated() {
    let keys = unique_keys(vec![
        "day/photo".to_string(),
        "day/photo".to_string(),
        "day/other".to_string(),
    ]);

    assert_eq!(keys.len(), 2);
    assert!(keys.contains("day/photo"));
    assert!(keys.contains("day/other"));
}

#[test]
fn cleanup_plan_rejects_candidates_that_were_not_marked_for_cleanup() {
    let root = PathBuf::from("/photos/raw");
    let plan = CleanupPlan::new(
        "plan-1".to_string(),
        root.clone(),
        [("missing.NEF".to_string(), FileSnapshot::new(12, Some(34)))],
    );

    let error = plan
        .authorize(
            "plan-1",
            &root,
            "paired.NEF",
            &FileSnapshot::new(12, Some(34)),
        )
        .expect_err("paired file must not be authorized");

    assert!(error.contains("不在当前扫描的清理计划中"));
}

#[test]
fn cleanup_plan_binds_plan_root_and_scanned_metadata() {
    let root = PathBuf::from("/photos/raw");
    let plan = CleanupPlan::new(
        "plan-1".to_string(),
        root.clone(),
        [("missing.NEF".to_string(), FileSnapshot::new(12, Some(34)))],
    );

    assert!(plan.matches("plan-1", &root));
    assert!(!plan.matches("other-plan", &root));
    assert!(
        plan.authorize(
            "other-plan",
            &root,
            "missing.NEF",
            &FileSnapshot::new(12, Some(34)),
        )
        .is_err()
    );
    assert!(
        plan.authorize(
            "plan-1",
            &PathBuf::from("/photos/elsewhere"),
            "missing.NEF",
            &FileSnapshot::new(12, Some(34)),
        )
        .is_err()
    );
    assert!(
        plan.authorize(
            "plan-1",
            &root,
            "missing.NEF",
            &FileSnapshot::new(13, Some(34)),
        )
        .is_err()
    );
    assert!(
        plan.authorize(
            "plan-1",
            &root,
            "missing.NEF",
            &FileSnapshot::new(12, Some(34)),
        )
        .is_ok()
    );
}
