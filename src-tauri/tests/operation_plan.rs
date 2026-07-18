#[path = "../src/formats.rs"]
mod formats;
#[allow(dead_code)]
#[path = "../src/operation_plan.rs"]
mod operation_plan;
#[path = "../src/photo_groups.rs"]
mod photo_groups;
#[path = "../src/rating_metadata.rs"]
mod rating_metadata;
#[allow(dead_code)]
#[path = "../src/rating_rules.rs"]
mod rating_rules;
#[allow(dead_code)]
#[path = "../src/rating_sync.rs"]
mod rating_sync;

use operation_plan::{
    OperationPlanRequest, OperationPlanStatus, OperationSyncPreference, build_operation_plan,
};
use rating_rules::{RatingCondition, RatingRule, RuleAction, RuleMemberKind};
use rating_sync::RatingConflictPolicy;
use rating_sync::{RatingSyncTarget, RatingSyncTargets};
use std::collections::HashMap;
use std::fs;

fn rule(
    id: &str,
    condition: RatingCondition,
    member_scope: Vec<RuleMemberKind>,
    action: RuleAction,
) -> RatingRule {
    RatingRule {
        id: id.to_string(),
        name: format!("规则 {id}"),
        enabled: true,
        condition,
        member_scope,
        action,
        destination: None,
        preserve_relative_path: true,
    }
}

fn request(root: &std::path::Path, rules: Vec<RatingRule>) -> OperationPlanRequest {
    OperationPlanRequest {
        root: root.to_string_lossy().into_owned(),
        rules,
        conflict_policy: RatingConflictPolicy::Skip,
        sync: OperationSyncPreference::default(),
    }
}

fn destination_rule(
    id: &str,
    action: RuleAction,
    destination: &std::path::Path,
    preserve_relative_path: bool,
) -> RatingRule {
    let mut output = rule(
        id,
        RatingCondition::Between {
            minimum: 0,
            maximum: 5,
        },
        vec![
            RuleMemberKind::Jpeg,
            RuleMemberKind::Raw,
            RuleMemberKind::Xmp,
        ],
        action,
    );
    output.destination = Some(destination.to_string_lossy().into_owned());
    output.preserve_relative_path = preserve_relative_path;
    output
}

fn write_jpeg(path: &std::path::Path) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    image::RgbImage::from_pixel(2, 2, image::Rgb([30, 60, 90]))
        .save_with_format(path, image::ImageFormat::Jpeg)
        .unwrap();
}

fn relative_entries(root: &std::path::Path) -> Vec<String> {
    let mut entries = walkdir::WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .map(|entry| entry.unwrap())
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| {
            entry
                .path()
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect::<Vec<_>>();
    entries.sort();
    entries
}

fn raw_index(
    root: &std::path::Path,
    files: &[&str],
    ratings: &[(&str, u8)],
) -> photo_groups::PhotoIndex {
    fs::create_dir_all(root).unwrap();
    for file in files {
        let path = root.join(file);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, b"raw-bytes").unwrap();
    }
    let mut index = photo_groups::index_directory(root).unwrap();
    photo_groups::apply_framepair_ratings(
        &mut index,
        &ratings
            .iter()
            .map(|(id, rating)| ((*id).to_string(), *rating))
            .collect::<HashMap<_, _>>(),
    );
    index
}

#[test]
fn one_matching_rule_plans_the_existing_requested_members() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("photos");
    let index = raw_index(&root, &["day/A.NEF"], &[("day/a", 4)]);
    let matching = rule(
        "high",
        RatingCondition::AtLeast { rating: 4 },
        vec![RuleMemberKind::Raw],
        RuleAction::Cleanup,
    );

    let plan =
        build_operation_plan(&index, &request(&root, vec![matching]), "plan-1".into()).unwrap();
    let item = &plan.summary().items[0];
    assert_eq!(item.status, OperationPlanStatus::Ready);
    assert_eq!(item.rating, Some(4));
    assert_eq!(item.matched_rule_ids, ["high"]);
    assert_eq!(item.terminal_action, Some(RuleAction::Cleanup));
    assert_eq!(item.members.len(), 1);
    assert_eq!(item.members[0].source_relative_path, "day/A.NEF");
    assert!(item.issues.is_empty());
}

#[test]
fn no_match_and_disabled_rules_are_skipped() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("photos");
    let index = raw_index(&root, &["A.NEF"], &[("a", 3)]);
    let no_match = rule(
        "five",
        RatingCondition::Equal { rating: 5 },
        vec![RuleMemberKind::Raw],
        RuleAction::Cleanup,
    );
    let mut disabled = rule(
        "disabled",
        RatingCondition::Equal { rating: 3 },
        vec![RuleMemberKind::Raw],
        RuleAction::Cleanup,
    );
    disabled.enabled = false;

    let plan = build_operation_plan(
        &index,
        &request(&root, vec![no_match, disabled]),
        "skip".into(),
    )
    .unwrap();
    assert_eq!(plan.summary().items[0].status, OperationPlanStatus::Skipped);
    assert!(plan.summary().items[0].matched_rule_ids.is_empty());
}

#[test]
fn missing_requested_kinds_are_reported_while_existing_members_remain_planned() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("photos");
    let index = raw_index(&root, &["A.NEF"], &[("a", 2)]);
    let cleanup = rule(
        "mixed",
        RatingCondition::AtMost { rating: 2 },
        vec![
            RuleMemberKind::Jpeg,
            RuleMemberKind::Raw,
            RuleMemberKind::Xmp,
        ],
        RuleAction::Cleanup,
    );

    let plan =
        build_operation_plan(&index, &request(&root, vec![cleanup]), "missing".into()).unwrap();
    let item = &plan.summary().items[0];
    assert_eq!(item.status, OperationPlanStatus::Ready);
    assert_eq!(item.members.len(), 1);
    assert_eq!(
        item.missing_kinds,
        [RuleMemberKind::Jpeg, RuleMemberKind::Xmp]
    );
}

#[test]
fn repeated_terminal_matches_are_conflicts_not_first_rule_wins() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("photos");
    let index = raw_index(&root, &["A.NEF"], &[("a", 4)]);
    let first = rule(
        "one",
        RatingCondition::AtLeast { rating: 4 },
        vec![RuleMemberKind::Raw],
        RuleAction::Cleanup,
    );
    let second = rule(
        "two",
        RatingCondition::Equal { rating: 4 },
        vec![RuleMemberKind::Raw],
        RuleAction::Keep,
    );

    let plan = build_operation_plan(
        &index,
        &request(&root, vec![first, second]),
        "conflict".into(),
    )
    .unwrap();
    let item = &plan.summary().items[0];
    assert_eq!(item.status, OperationPlanStatus::Conflict);
    assert_eq!(item.matched_rule_ids, ["one", "two"]);
    assert!(item.terminal_action.is_none());
    assert!(item.issues.iter().any(|issue| issue.contains("命中多条")));
}

#[test]
fn rating_source_conflicts_block_rule_evaluation_under_the_default_policy() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("photos");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("A.NEF"), b"raw").unwrap();
    fs::write(root.join("A.xmp"), br#"<xmp:Rating>5</xmp:Rating>"#).unwrap();
    let mut index = photo_groups::index_directory(&root).unwrap();
    photo_groups::apply_framepair_ratings(&mut index, &HashMap::from([("a".to_string(), 4)]));
    let cleanup = rule(
        "all",
        RatingCondition::Between {
            minimum: 0,
            maximum: 5,
        },
        vec![RuleMemberKind::Raw],
        RuleAction::Cleanup,
    );

    let plan =
        build_operation_plan(&index, &request(&root, vec![cleanup]), "rating".into()).unwrap();
    let item = &plan.summary().items[0];
    assert_eq!(item.status, OperationPlanStatus::Conflict);
    assert_eq!(item.rating, None);
    assert!(
        item.issues
            .iter()
            .any(|issue| issue.contains("评分来源不一致"))
    );
}

#[test]
fn keep_rules_are_visible_but_not_executable_file_actions() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("photos");
    let index = raw_index(&root, &["A.NEF"], &[("a", 5)]);
    let keep = rule(
        "keep",
        RatingCondition::Equal { rating: 5 },
        vec![RuleMemberKind::Raw],
        RuleAction::Keep,
    );

    let plan = build_operation_plan(&index, &request(&root, vec![keep]), "keep".into()).unwrap();
    assert_eq!(plan.summary().items[0].status, OperationPlanStatus::Keep);
    assert_eq!(plan.summary().kept, 1);
    assert_eq!(plan.summary().ready, 0);
}

#[test]
fn copy_targets_preserve_relative_paths_or_flatten_by_explicit_rule() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("photos");
    let target = temp.path().join("archive");
    fs::create_dir_all(&target).unwrap();
    let index = raw_index(
        &root,
        &["day/A.NEF", "other/B.NEF"],
        &[("day/a", 4), ("other/b", 4)],
    );

    let preserved = build_operation_plan(
        &index,
        &request(
            &root,
            vec![destination_rule("copy", RuleAction::Copy, &target, true)],
        ),
        "preserve".into(),
    )
    .unwrap();
    assert!(
        preserved.summary().items[0].members[0]
            .target_path
            .as_deref()
            .unwrap()
            .ends_with("archive/day/A.NEF")
    );

    let flattened = build_operation_plan(
        &index,
        &request(
            &root,
            vec![destination_rule("copy", RuleAction::Copy, &target, false)],
        ),
        "flat".into(),
    )
    .unwrap();
    assert!(
        flattened.summary().items[0].members[0]
            .target_path
            .as_deref()
            .unwrap()
            .ends_with("archive/A.NEF")
    );
    assert!(
        relative_entries(&target).is_empty(),
        "planning must not write targets"
    );
}

#[test]
fn destinations_equal_to_or_nested_with_the_source_are_configuration_errors() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("photos");
    let index = raw_index(&root, &["A.NEF"], &[("a", 4)]);
    let child = root.join("child");
    fs::create_dir_all(&child).unwrap();

    for destination in [&root, &child, temp.path()] {
        let error = build_operation_plan(
            &index,
            &request(
                &root,
                vec![destination_rule(
                    "move",
                    RuleAction::Move,
                    destination,
                    true,
                )],
            ),
            "overlap".into(),
        )
        .unwrap_err();
        assert!(error.contains("不能相同或互相嵌套"), "{error}");
    }
}

#[test]
fn occupied_and_flattened_duplicate_targets_block_every_affected_group() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("photos");
    let target = temp.path().join("archive");
    fs::create_dir_all(&target).unwrap();
    let index = raw_index(
        &root,
        &["day/A.NEF", "other/A.NEF", "unique/B.NEF"],
        &[("day/a", 4), ("other/a", 4), ("unique/b", 4)],
    );
    fs::write(target.join("B.NEF"), b"occupied").unwrap();
    let before = relative_entries(&target);

    let plan = build_operation_plan(
        &index,
        &request(
            &root,
            vec![destination_rule("copy", RuleAction::Copy, &target, false)],
        ),
        "collisions".into(),
    )
    .unwrap();
    assert_eq!(plan.summary().conflicts, 3);
    assert!(
        plan.summary()
            .items
            .iter()
            .all(|item| item.issues.iter().any(|issue| issue.contains("目标路径")))
    );
    assert_eq!(relative_entries(&target), before);
}

#[test]
fn planned_members_bind_source_size_and_modified_snapshots_without_writing() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("photos");
    let target = temp.path().join("archive");
    fs::create_dir_all(&target).unwrap();
    let index = raw_index(&root, &["A.NEF"], &[("a", 4)]);
    let source_metadata = fs::metadata(root.join("A.NEF")).unwrap();

    let plan = build_operation_plan(
        &index,
        &request(
            &root,
            vec![destination_rule("move", RuleAction::Move, &target, true)],
        ),
        "snapshot".into(),
    )
    .unwrap();
    let member = &plan.summary().items[0].members[0];
    assert_eq!(member.size_bytes, source_metadata.len());
    assert!(member.modified_ms.is_some());
    assert!(!std::path::Path::new(member.target_path.as_deref().unwrap()).exists());
}

#[test]
fn sync_preview_uses_source_destination_and_before_cleanup_timings() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("photos");
    let target = temp.path().join("archive");
    fs::create_dir_all(&target).unwrap();
    let index = raw_index(&root, &["A.NEF"], &[("a", 4)]);

    let mut keep_request = request(
        &root,
        vec![rule(
            "keep",
            RatingCondition::Equal { rating: 4 },
            vec![RuleMemberKind::Raw],
            RuleAction::Keep,
        )],
    );
    keep_request.sync.enabled = true;
    let keep_plan = build_operation_plan(&index, &keep_request, "sync-source".into()).unwrap();
    let source_sync = &keep_plan.summary().items[0].sync_actions[0];
    assert_eq!(source_sync.target, RatingSyncTarget::RawXmp);
    assert_eq!(source_sync.timing, operation_plan::SyncTiming::Source);
    assert!(source_sync.target_path.ends_with("photos/A.xmp"));

    let mut move_request = request(
        &root,
        vec![destination_rule("move", RuleAction::Move, &target, true)],
    );
    move_request.sync.enabled = true;
    let move_plan = build_operation_plan(&index, &move_request, "sync-target".into()).unwrap();
    let destination_sync = &move_plan.summary().items[0].sync_actions[0];
    assert_eq!(
        destination_sync.timing,
        operation_plan::SyncTiming::Destination
    );
    assert!(destination_sync.target_path.ends_with("archive/A.xmp"));

    let cleanup = rule(
        "cleanup",
        RatingCondition::Equal { rating: 4 },
        vec![RuleMemberKind::Raw],
        RuleAction::Cleanup,
    );
    let mut cleanup_request = request(&root, vec![cleanup]);
    cleanup_request.sync.enabled = true;
    let no_sync = build_operation_plan(&index, &cleanup_request, "cleanup-off".into()).unwrap();
    assert!(no_sync.summary().items[0].sync_actions.is_empty());
    cleanup_request.sync.sync_cleanup_before = true;
    let before = build_operation_plan(&index, &cleanup_request, "cleanup-on".into()).unwrap();
    assert_eq!(
        before.summary().items[0].sync_actions[0].timing,
        operation_plan::SyncTiming::BeforeCleanup
    );
    assert!(!root.join("A.xmp").exists());
}

#[test]
fn jpeg_sync_preview_requires_confirmation_and_uses_the_jpeg_target() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("photos");
    fs::create_dir_all(&root).unwrap();
    write_jpeg(&root.join("A.JPG"));
    let mut index = photo_groups::index_directory(&root).unwrap();
    photo_groups::apply_framepair_ratings(&mut index, &HashMap::from([("a".to_string(), 3)]));
    let keep = rule(
        "jpeg",
        RatingCondition::Equal { rating: 3 },
        vec![RuleMemberKind::Jpeg],
        RuleAction::Keep,
    );
    let mut plan_request = request(&root, vec![keep]);
    plan_request.sync = OperationSyncPreference {
        enabled: true,
        targets: RatingSyncTargets {
            raw_xmp: false,
            jpeg_metadata: true,
        },
        jpeg_write_confirmed: false,
        sync_cleanup_before: false,
    };

    assert!(
        build_operation_plan(&index, &plan_request, "unconfirmed".into())
            .unwrap_err()
            .contains("启用 JPG 元数据同步前必须明确确认")
    );
    plan_request.sync.jpeg_write_confirmed = true;
    let plan = build_operation_plan(&index, &plan_request, "confirmed".into()).unwrap();
    assert_eq!(
        plan.summary().items[0].sync_actions[0].target,
        RatingSyncTarget::JpegMetadata
    );
    assert!(
        plan.summary().items[0].sync_actions[0]
            .target_path
            .ends_with("photos/A.JPG")
    );
}
