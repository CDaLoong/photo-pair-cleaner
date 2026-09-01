//! file_organizer 的行为测试。覆盖执行、同步、回滚与三条恢复路径。

use super::*;
use crate::operation_history::OrganizerGroupStatus;
use crate::operation_plan::{
    AuthorizedOperationPlan, CleanupExecutionDestination, OperationPlanItem, OperationPlanStatus,
    OperationPlanSummary, OperationSyncPreference, PlannedMember, PlannedSyncAction, SyncTiming,
};
use crate::rating_rules::{RatingCondition, RatingRule, RuleAction, RuleMemberKind};
use crate::rating_sync::{RatingSyncTarget, RatingSyncTargets};
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;
use tempfile::tempdir;

fn modified_ms(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
}

fn copy_rule(destination: &Path) -> RatingRule {
    RatingRule {
        id: "copy-rule".to_string(),
        name: "复制三星".to_string(),
        enabled: true,
        condition: RatingCondition::Equal { rating: 3 },
        member_scope: vec![RuleMemberKind::Jpeg],
        action: RuleAction::Copy,
        destination: Some(destination.to_string_lossy().into_owned()),
        preserve_relative_path: true,
    }
}

fn move_rule(destination: &Path) -> RatingRule {
    RatingRule {
        id: "move-rule".to_string(),
        name: "移动三星".to_string(),
        enabled: true,
        condition: RatingCondition::Equal { rating: 3 },
        member_scope: vec![RuleMemberKind::Jpeg],
        action: RuleAction::Move,
        destination: Some(destination.to_string_lossy().into_owned()),
        preserve_relative_path: true,
    }
}

fn copy_item(
    root: &Path,
    destination: &Path,
    group_id: &str,
    relative_path: &str,
) -> OperationPlanItem {
    let source = root.join(relative_path);
    let metadata = fs::metadata(&source).expect("source metadata");
    OperationPlanItem {
        group_id: group_id.to_string(),
        relative_stem: relative_path.trim_end_matches(".jpg").to_string(),
        rating: Some(3),
        frame_pair: 3,
        jpeg_metadata: None,
        raw_xmp: None,
        matched_rule_ids: vec!["copy-rule".to_string()],
        matched_rule_names: vec!["复制三星".to_string()],
        terminal_action: Some(RuleAction::Copy),
        status: OperationPlanStatus::Ready,
        members: vec![PlannedMember {
            kind: RuleMemberKind::Jpeg,
            source_relative_path: relative_path.to_string(),
            target_path: Some(
                destination
                    .join(relative_path)
                    .to_string_lossy()
                    .into_owned(),
            ),
            size_bytes: metadata.len(),
            modified_ms: modified_ms(&source),
        }],
        missing_kinds: Vec::new(),
        sync_actions: Vec::new(),
        issues: Vec::new(),
    }
}

fn plan(root: &Path, destination: &Path, items: Vec<OperationPlanItem>) -> AuthorizedOperationPlan {
    AuthorizedOperationPlan {
        summary: OperationPlanSummary {
            plan_id: "plan-1".to_string(),
            root: root.to_string_lossy().into_owned(),
            total_items: items.len(),
            ready: items.len(),
            kept: 0,
            skipped: 0,
            conflicts: 0,
            move_groups: 0,
            copy_groups: items.len(),
            cleanup_groups: 0,
            sync_groups: 0,
            jpeg_files: items.len(),
            raw_files: 0,
            xmp_files: 0,
            copy_bytes: items
                .iter()
                .flat_map(|item| &item.members)
                .map(|member| member.size_bytes)
                .sum(),
            cleanup_bytes: 0,
            items: items.clone(),
        },
        items,
        rules: vec![copy_rule(destination)],
        sync: OperationSyncPreference::default(),
        cleanup_destination: None,
    }
}

fn move_item(
    root: &Path,
    destination: &Path,
    group_id: &str,
    relative_path: &str,
) -> OperationPlanItem {
    let mut item = copy_item(root, destination, group_id, relative_path);
    item.matched_rule_ids = vec!["move-rule".to_string()];
    item.matched_rule_names = vec!["移动三星".to_string()];
    item.terminal_action = Some(RuleAction::Move);
    item
}

fn move_plan(
    root: &Path,
    destination: &Path,
    items: Vec<OperationPlanItem>,
) -> AuthorizedOperationPlan {
    let mut plan = plan(root, destination, items);
    plan.summary.move_groups = plan.summary.copy_groups;
    plan.summary.copy_groups = 0;
    plan.summary.copy_bytes = 0;
    plan.rules = vec![move_rule(destination)];
    plan
}

fn cleanup_rule() -> RatingRule {
    RatingRule {
        id: "cleanup-rule".to_string(),
        name: "低分清理".to_string(),
        enabled: true,
        condition: RatingCondition::AtMost { rating: 2 },
        member_scope: vec![
            RuleMemberKind::Jpeg,
            RuleMemberKind::Raw,
            RuleMemberKind::Xmp,
        ],
        action: RuleAction::Cleanup,
        destination: None,
        preserve_relative_path: true,
    }
}

fn cleanup_plan(
    root: &Path,
    group_id: &str,
    members: &[(&str, RuleMemberKind)],
) -> AuthorizedOperationPlan {
    let planned_members = members
        .iter()
        .map(|(relative, kind)| {
            let source = root.join(relative);
            let metadata = fs::metadata(&source).expect("cleanup source metadata");
            PlannedMember {
                kind: *kind,
                source_relative_path: (*relative).to_string(),
                target_path: None,
                size_bytes: metadata.len(),
                modified_ms: modified_ms(&source),
            }
        })
        .collect::<Vec<_>>();
    let bytes = planned_members.iter().map(|member| member.size_bytes).sum();
    let item = OperationPlanItem {
        group_id: group_id.to_string(),
        relative_stem: "album/photo".to_string(),
        rating: Some(1),
        frame_pair: 1,
        jpeg_metadata: None,
        raw_xmp: None,
        matched_rule_ids: vec!["cleanup-rule".to_string()],
        matched_rule_names: vec!["低分清理".to_string()],
        terminal_action: Some(RuleAction::Cleanup),
        status: OperationPlanStatus::Ready,
        members: planned_members,
        missing_kinds: Vec::new(),
        sync_actions: Vec::new(),
        issues: Vec::new(),
    };
    AuthorizedOperationPlan {
        summary: OperationPlanSummary {
            plan_id: "cleanup-plan".to_string(),
            root: root.to_string_lossy().into_owned(),
            total_items: 1,
            ready: 1,
            kept: 0,
            skipped: 0,
            conflicts: 0,
            move_groups: 0,
            copy_groups: 0,
            cleanup_groups: 1,
            sync_groups: 0,
            jpeg_files: members
                .iter()
                .filter(|(_, kind)| *kind == RuleMemberKind::Jpeg)
                .count(),
            raw_files: members
                .iter()
                .filter(|(_, kind)| *kind == RuleMemberKind::Raw)
                .count(),
            xmp_files: members
                .iter()
                .filter(|(_, kind)| *kind == RuleMemberKind::Xmp)
                .count(),
            copy_bytes: 0,
            cleanup_bytes: bytes,
            items: vec![item.clone()],
        },
        items: vec![item],
        rules: vec![cleanup_rule()],
        sync: OperationSyncPreference::default(),
        cleanup_destination: Some(CleanupExecutionDestination::Quarantine),
    }
}

fn trash_plan(
    root: &Path,
    group_id: &str,
    members: &[(&str, RuleMemberKind)],
) -> AuthorizedOperationPlan {
    let mut plan = cleanup_plan(root, group_id, members);
    plan.cleanup_destination = Some(CleanupExecutionDestination::Trash);
    plan
}

fn create_cleanup_group(root: &Path) -> Vec<(&'static str, RuleMemberKind)> {
    let members = vec![
        ("album/photo.jpg", RuleMemberKind::Jpeg),
        ("album/photo.nef", RuleMemberKind::Raw),
        ("album/photo.xmp", RuleMemberKind::Xmp),
    ];
    fs::create_dir_all(root.join("album")).expect("cleanup album");
    for (relative, _) in &members {
        fs::write(root.join(relative), relative.as_bytes()).expect("cleanup member");
    }
    members
}

#[test]
fn quarantine_cleanup_moves_and_restores_a_complete_photo_group() {
    let source = tempdir().expect("source");
    let app_data = tempdir().expect("app data");
    let members = create_cleanup_group(source.path());

    let summary = execute_authorized_plan(
        app_data.path(),
        "operation-1".to_string(),
        100,
        cleanup_plan(source.path(), "cleanup", &members),
    )
    .expect("execute quarantine cleanup");

    assert_eq!(summary.succeeded, 1, "{:?}", summary.groups);
    assert_eq!(summary.groups[0].action, OrganizerAction::Quarantine);
    for (relative, _) in &members {
        assert!(!source.path().join(relative).exists());
        assert!(
            source
                .path()
                .join(crate::quarantine::QUARANTINE_DIR)
                .join("operation-1")
                .join(relative)
                .exists()
        );
    }

    let restored = restore_quarantine_operation(
        app_data.path(),
        "operation-1",
        &["cleanup".to_string()],
        200,
    )
    .expect("restore quarantine cleanup");
    assert_eq!(restored.succeeded, 1);
    for (relative, _) in &members {
        assert!(source.path().join(relative).exists());
    }
}

#[test]
fn quarantine_cleanup_preflight_blocks_drift_and_occupied_targets() {
    let source = tempdir().expect("source");
    let app_data = tempdir().expect("app data");
    let members = create_cleanup_group(source.path());
    let plan = cleanup_plan(source.path(), "cleanup", &members);
    fs::write(source.path().join("album/photo.nef"), b"changed after plan").expect("change source");

    let summary = execute_authorized_plan(app_data.path(), "operation-1".to_string(), 100, plan)
        .expect("execute drifted cleanup");
    assert_eq!(summary.failed, 1);
    assert!(
        members
            .iter()
            .all(|(relative, _)| source.path().join(relative).exists())
    );

    let second = tempdir().expect("second source");
    let second_data = tempdir().expect("second app data");
    let members = create_cleanup_group(second.path());
    let occupied = second
        .path()
        .join(crate::quarantine::QUARANTINE_DIR)
        .join("operation-2")
        .join("album/photo.jpg");
    fs::create_dir_all(occupied.parent().expect("occupied parent")).expect("target parent");
    fs::write(&occupied, b"occupied").expect("occupied target");
    let summary = execute_authorized_plan(
        second_data.path(),
        "operation-2".to_string(),
        100,
        cleanup_plan(second.path(), "cleanup", &members),
    )
    .expect("execute occupied cleanup");
    assert_eq!(summary.failed, 1);
    assert!(
        members
            .iter()
            .all(|(relative, _)| second.path().join(relative).exists())
    );
}

#[test]
fn quarantine_cleanup_rolls_back_group_and_history_failures() {
    let source = tempdir().expect("source");
    let app_data = tempdir().expect("app data");
    let members = create_cleanup_group(source.path());
    let summary = execute_authorized_plan_with_options(
        app_data.path(),
        "operation-1".to_string(),
        100,
        cleanup_plan(source.path(), "cleanup", &members),
        ExecutionOptions {
            fail_rename_at: Some(1),
            ..ExecutionOptions::default()
        },
    )
    .expect("execute injected failure");
    assert_eq!(summary.failed, 1);
    assert!(
        members
            .iter()
            .all(|(relative, _)| source.path().join(relative).exists())
    );

    let second = tempdir().expect("second source");
    let second_data = tempdir().expect("second app data");
    let second_members = create_cleanup_group(second.path());
    fs::create_dir_all(
        second_data
            .path()
            .join(crate::operation_history::HISTORY_DIR)
            .join("operation-2"),
    )
    .expect("occupy history operation");
    assert!(
        execute_authorized_plan(
            second_data.path(),
            "operation-2".to_string(),
            100,
            cleanup_plan(second.path(), "cleanup", &second_members),
        )
        .is_err()
    );
    assert!(
        second_members
            .iter()
            .all(|(relative, _)| second.path().join(relative).exists())
    );
}

#[test]
fn cleanup_trash_preflights_the_group_and_records_nonrecoverable_results() {
    let source = tempdir().expect("source");
    let app_data = tempdir().expect("app data");
    let members = create_cleanup_group(source.path());
    let summary = execute_authorized_plan_with_options(
        app_data.path(),
        "operation-1".to_string(),
        100,
        trash_plan(source.path(), "cleanup", &members),
        ExecutionOptions {
            simulate_trash: true,
            ..ExecutionOptions::default()
        },
    )
    .expect("execute simulated trash cleanup");
    assert_eq!(summary.succeeded, 1);
    assert_eq!(summary.groups[0].action, OrganizerAction::Trash);
    assert!(
        summary.groups[0]
            .members
            .iter()
            .all(|member| member.target_snapshot.is_none())
    );
    assert!(
        members
            .iter()
            .all(|(relative, _)| !source.path().join(relative).exists())
    );
    assert_eq!(
        crate::operation_history::list_operations(app_data.path()).expect("trash history")[0]
            .recoverable_groups,
        0
    );

    let drifted = tempdir().expect("drifted source");
    let drifted_data = tempdir().expect("drifted app data");
    let drifted_members = create_cleanup_group(drifted.path());
    let drifted_plan = trash_plan(drifted.path(), "cleanup", &drifted_members);
    fs::write(drifted.path().join("album/photo.xmp"), b"changed").expect("change later member");
    let summary = execute_authorized_plan_with_options(
        drifted_data.path(),
        "operation-2".to_string(),
        100,
        drifted_plan,
        ExecutionOptions {
            simulate_trash: true,
            ..ExecutionOptions::default()
        },
    )
    .expect("execute drifted trash cleanup");
    assert_eq!(summary.failed, 1);
    assert!(
        drifted_members
            .iter()
            .all(|(relative, _)| drifted.path().join(relative).exists())
    );
}

#[test]
fn cleanup_trash_stops_after_a_partial_system_failure() {
    let source = tempdir().expect("source");
    let app_data = tempdir().expect("app data");
    let members = create_cleanup_group(source.path());
    let summary = execute_authorized_plan_with_options(
        app_data.path(),
        "operation-1".to_string(),
        100,
        trash_plan(source.path(), "cleanup", &members),
        ExecutionOptions {
            simulate_trash: true,
            fail_trash_at: Some(1),
            ..ExecutionOptions::default()
        },
    )
    .expect("execute partial trash cleanup");
    assert_eq!(summary.partial, 1);
    assert!(!source.path().join(members[0].0).exists());
    assert!(source.path().join(members[1].0).exists());
    assert!(source.path().join(members[2].0).exists());
}

#[test]
fn cleanup_before_sync_moves_a_new_xmp_into_quarantine() {
    let source = tempdir().expect("source");
    let app_data = tempdir().expect("app data");
    fs::create_dir_all(source.path().join("album")).expect("album");
    fs::write(source.path().join("album/photo.nef"), b"raw").expect("raw");
    let members = vec![("album/photo.nef", RuleMemberKind::Raw)];
    let mut plan = cleanup_plan(source.path(), "cleanup", &members);
    plan.sync = OperationSyncPreference {
        enabled: true,
        targets: RatingSyncTargets {
            raw_xmp: true,
            jpeg_metadata: false,
        },
        jpeg_write_confirmed: false,
        sync_cleanup_before: true,
    };
    plan.items[0].sync_actions = vec![PlannedSyncAction {
        target: RatingSyncTarget::RawXmp,
        target_path: fs::canonicalize(source.path())
            .expect("canonical source")
            .join("album/photo.xmp")
            .to_string_lossy()
            .into_owned(),
        target_rating: 4,
        timing: SyncTiming::BeforeCleanup,
    }];

    let summary = execute_authorized_plan(app_data.path(), "operation-1".to_string(), 100, plan)
        .expect("execute cleanup sync");
    assert_eq!(summary.succeeded, 1, "{:?}", summary.groups);
    let quarantined_xmp = source
        .path()
        .join(crate::quarantine::QUARANTINE_DIR)
        .join("operation-1/album/photo.xmp");
    assert_eq!(
        crate::rating_metadata::xmp_rating(&fs::read(&quarantined_xmp).expect("quarantined xmp"))
            .expect("read xmp rating"),
        Some(4)
    );
    assert_eq!(summary.groups[0].members.len(), 2);
}

#[test]
fn cleanup_sync_failure_prevents_the_terminal_action() {
    let source = tempdir().expect("source");
    let app_data = tempdir().expect("app data");
    fs::create_dir_all(source.path().join("album")).expect("album");
    fs::write(source.path().join("album/photo.jpg"), b"not a jpeg").expect("jpeg");
    let members = vec![("album/photo.jpg", RuleMemberKind::Jpeg)];
    let mut plan = cleanup_plan(source.path(), "cleanup", &members);
    plan.sync = OperationSyncPreference {
        enabled: true,
        targets: RatingSyncTargets {
            raw_xmp: false,
            jpeg_metadata: true,
        },
        jpeg_write_confirmed: true,
        sync_cleanup_before: true,
    };
    plan.items[0].sync_actions = vec![PlannedSyncAction {
        target: RatingSyncTarget::JpegMetadata,
        target_path: fs::canonicalize(source.path())
            .expect("canonical source")
            .join("album/photo.jpg")
            .to_string_lossy()
            .into_owned(),
        target_rating: 1,
        timing: SyncTiming::BeforeCleanup,
    }];

    let summary = execute_authorized_plan(app_data.path(), "operation-1".to_string(), 100, plan)
        .expect("execute failed sync cleanup");
    assert_eq!(summary.failed, 1);
    assert!(source.path().join("album/photo.jpg").exists());
}

#[test]
fn copy_execution_verifies_content_and_persists_history() {
    let source = tempdir().expect("source");
    let target = tempdir().expect("target");
    let app_data = tempdir().expect("app data");
    fs::create_dir_all(source.path().join("album")).expect("source album");
    fs::write(source.path().join("album/photo.jpg"), b"framepair").expect("source file");
    let root = fs::canonicalize(source.path()).expect("source root");
    let destination = fs::canonicalize(target.path()).expect("target root");
    let item = copy_item(&root, &destination, "group-1", "album/photo.jpg");

    let summary = execute_authorized_plan(
        app_data.path(),
        "operation-1".to_string(),
        100,
        plan(&root, &destination, vec![item]),
    )
    .expect("execute copy");

    assert_eq!(summary.succeeded, 1);
    assert_eq!(summary.failed, 0);
    assert_eq!(
        fs::read(destination.join("album/photo.jpg")).unwrap(),
        b"framepair"
    );
    let history = crate::operation_history::list_operations(app_data.path()).unwrap();
    assert_eq!(
        history[0].manifest.groups[0].status,
        OrganizerGroupStatus::Success
    );
    assert_eq!(
        history[0].manifest.groups[0].members[0]
            .target_snapshot
            .as_ref()
            .unwrap()
            .sha256
            .len(),
        64
    );
}

#[test]
fn copy_execution_isolates_a_drifted_group() {
    let source = tempdir().expect("source");
    let target = tempdir().expect("target");
    let app_data = tempdir().expect("app data");
    fs::write(source.path().join("changed.jpg"), b"before").expect("changed source");
    fs::write(source.path().join("stable.jpg"), b"stable").expect("stable source");
    let root = fs::canonicalize(source.path()).expect("source root");
    let destination = fs::canonicalize(target.path()).expect("target root");
    let changed = copy_item(&root, &destination, "changed", "changed.jpg");
    let stable = copy_item(&root, &destination, "stable", "stable.jpg");
    fs::write(root.join("changed.jpg"), b"changed after plan").expect("drift source");

    let summary = execute_authorized_plan(
        app_data.path(),
        "operation-1".to_string(),
        100,
        plan(&root, &destination, vec![changed, stable]),
    )
    .expect("execute copy groups");

    assert_eq!(summary.succeeded, 1);
    assert_eq!(summary.failed, 1);
    assert!(!destination.join("changed.jpg").exists());
    assert_eq!(fs::read(destination.join("stable.jpg")).unwrap(), b"stable");
}

#[cfg(unix)]
#[test]
fn copy_execution_rejects_source_symlinks_and_existing_targets() {
    use std::os::unix::fs::symlink;

    let source = tempdir().expect("source");
    let target = tempdir().expect("target");
    let app_data = tempdir().expect("app data");
    fs::write(source.path().join("real.jpg"), b"real").expect("real source");
    let root = fs::canonicalize(source.path()).expect("source root");
    let destination = fs::canonicalize(target.path()).expect("target root");
    let mut symlinked = copy_item(&root, &destination, "symlink", "real.jpg");
    symlink(root.join("real.jpg"), root.join("linked.jpg")).expect("source symlink");
    symlinked.members[0].source_relative_path = "linked.jpg".to_string();
    symlinked.members[0].target_path = Some(
        destination
            .join("linked.jpg")
            .to_string_lossy()
            .into_owned(),
    );
    let existing = copy_item(&root, &destination, "existing", "real.jpg");
    fs::write(destination.join("real.jpg"), b"do not replace").expect("existing target");

    let summary = execute_authorized_plan(
        app_data.path(),
        "operation-1".to_string(),
        100,
        plan(&root, &destination, vec![symlinked, existing]),
    )
    .expect("execute rejected groups");

    assert_eq!(summary.succeeded, 0);
    assert_eq!(summary.failed, 2);
    assert_eq!(
        fs::read(destination.join("real.jpg")).unwrap(),
        b"do not replace"
    );
    assert!(!destination.join("linked.jpg").exists());
}

#[test]
fn move_execution_renames_same_volume_group() {
    let source = tempdir().expect("source");
    let target = tempdir().expect("target");
    let app_data = tempdir().expect("app data");
    fs::write(source.path().join("photo.jpg"), b"move me").expect("source file");
    let root = fs::canonicalize(source.path()).expect("source root");
    let destination = fs::canonicalize(target.path()).expect("target root");
    let item = move_item(&root, &destination, "move", "photo.jpg");

    let summary = execute_authorized_plan(
        app_data.path(),
        "operation-1".to_string(),
        100,
        move_plan(&root, &destination, vec![item]),
    )
    .expect("execute move");

    assert_eq!(summary.succeeded, 1);
    assert!(!root.join("photo.jpg").exists());
    assert_eq!(fs::read(destination.join("photo.jpg")).unwrap(), b"move me");
}

#[test]
fn move_execution_rolls_back_prior_renames_when_group_fails() {
    let source = tempdir().expect("source");
    let target = tempdir().expect("target");
    let app_data = tempdir().expect("app data");
    fs::write(source.path().join("one.jpg"), b"one").expect("first source");
    fs::write(source.path().join("two.jpg"), b"two").expect("second source");
    let root = fs::canonicalize(source.path()).expect("source root");
    let destination = fs::canonicalize(target.path()).expect("target root");
    let mut item = move_item(&root, &destination, "pair", "one.jpg");
    item.members
        .extend(move_item(&root, &destination, "pair", "two.jpg").members);

    let summary = execute_authorized_plan_with_options(
        app_data.path(),
        "operation-1".to_string(),
        100,
        move_plan(&root, &destination, vec![item]),
        ExecutionOptions {
            fail_rename_at: Some(1),
            ..ExecutionOptions::default()
        },
    )
    .expect("execute failed move");

    assert_eq!(summary.failed, 1);
    assert_eq!(fs::read(root.join("one.jpg")).unwrap(), b"one");
    assert_eq!(fs::read(root.join("two.jpg")).unwrap(), b"two");
    assert!(!destination.join("one.jpg").exists());
    assert!(!destination.join("two.jpg").exists());
}

#[test]
fn cross_volume_move_commits_all_targets_before_deleting_sources() {
    let source = tempdir().expect("source");
    let target = tempdir().expect("target");
    let app_data = tempdir().expect("app data");
    fs::write(source.path().join("one.jpg"), b"one").expect("first source");
    fs::write(source.path().join("two.jpg"), b"two").expect("second source");
    let root = fs::canonicalize(source.path()).expect("source root");
    let destination = fs::canonicalize(target.path()).expect("target root");
    let mut item = move_item(&root, &destination, "pair", "one.jpg");
    item.members
        .extend(move_item(&root, &destination, "pair", "two.jpg").members);

    let summary = execute_authorized_plan_with_options(
        app_data.path(),
        "operation-1".to_string(),
        100,
        move_plan(&root, &destination, vec![item]),
        ExecutionOptions {
            force_copy_delete: true,
            ..ExecutionOptions::default()
        },
    )
    .expect("execute copy delete move");

    assert_eq!(summary.succeeded, 1);
    assert!(!root.join("one.jpg").exists());
    assert!(!root.join("two.jpg").exists());
    assert_eq!(fs::read(destination.join("one.jpg")).unwrap(), b"one");
    assert_eq!(fs::read(destination.join("two.jpg")).unwrap(), b"two");
}

#[test]
fn cross_volume_source_delete_failure_is_recorded_as_partial() {
    let source = tempdir().expect("source");
    let target = tempdir().expect("target");
    let app_data = tempdir().expect("app data");
    fs::write(source.path().join("one.jpg"), b"one").expect("first source");
    fs::write(source.path().join("two.jpg"), b"two").expect("second source");
    let root = fs::canonicalize(source.path()).expect("source root");
    let destination = fs::canonicalize(target.path()).expect("target root");
    let mut item = move_item(&root, &destination, "pair", "one.jpg");
    item.members
        .extend(move_item(&root, &destination, "pair", "two.jpg").members);

    let summary = execute_authorized_plan_with_options(
        app_data.path(),
        "operation-1".to_string(),
        100,
        move_plan(&root, &destination, vec![item]),
        ExecutionOptions {
            force_copy_delete: true,
            fail_delete_at: Some(1),
            ..ExecutionOptions::default()
        },
    )
    .expect("execute partial move");

    assert_eq!(summary.partial, 1);
    assert!(!root.join("one.jpg").exists());
    assert!(root.join("two.jpg").exists());
    assert!(destination.join("one.jpg").exists());
    assert!(destination.join("two.jpg").exists());
}

#[test]
fn destination_rating_sync_runs_after_copy_commit() {
    let source = tempdir().expect("source");
    let target = tempdir().expect("target");
    let app_data = tempdir().expect("app data");
    fs::write(source.path().join("photo.nef"), b"raw bytes").expect("raw source");
    let root = fs::canonicalize(source.path()).expect("source root");
    let destination = fs::canonicalize(target.path()).expect("target root");
    let mut item = copy_item(&root, &destination, "raw", "photo.nef");
    item.members[0].kind = RuleMemberKind::Raw;
    item.rating = Some(4);
    item.sync_actions = vec![PlannedSyncAction {
        target: RatingSyncTarget::RawXmp,
        target_path: destination.join("photo.xmp").to_string_lossy().into_owned(),
        target_rating: 4,
        timing: SyncTiming::Destination,
    }];
    let mut plan = plan(&root, &destination, vec![item]);
    plan.sync = OperationSyncPreference {
        enabled: true,
        targets: RatingSyncTargets {
            raw_xmp: true,
            jpeg_metadata: false,
        },
        jpeg_write_confirmed: false,
        sync_cleanup_before: false,
    };

    let summary = execute_authorized_plan(app_data.path(), "operation-1".to_string(), 100, plan)
        .expect("copy and sync");

    assert_eq!(summary.succeeded, 1);
    assert_eq!(
        crate::rating_metadata::read_sidecar_rating(&destination.join("photo.xmp")).unwrap(),
        Some(4)
    );
    let history = crate::operation_history::load_operation(app_data.path(), "operation-1").unwrap();
    assert_eq!(history.manifest.groups[0].members.len(), 2);
}

#[test]
fn copy_recovery_undoes_only_unchanged_created_files() {
    let source = tempdir().expect("source");
    let target = tempdir().expect("target");
    let app_data = tempdir().expect("app data");
    fs::write(source.path().join("photo.jpg"), b"copy").expect("source file");
    let root = fs::canonicalize(source.path()).expect("source root");
    let destination = fs::canonicalize(target.path()).expect("target root");
    let item = copy_item(&root, &destination, "copy", "photo.jpg");
    execute_authorized_plan(
        app_data.path(),
        "operation-1".to_string(),
        100,
        plan(&root, &destination, vec![item]),
    )
    .expect("copy");

    let undone = undo_copy_operation(app_data.path(), "operation-1", &["copy".to_string()], 200)
        .expect("undo copy");

    assert_eq!(undone.succeeded, 1);
    assert!(!destination.join("photo.jpg").exists());
    assert!(root.join("photo.jpg").exists());

    let second_source = tempdir().expect("second source");
    let second_target = tempdir().expect("second target");
    fs::write(second_source.path().join("photo.jpg"), b"copy").expect("source file");
    let second_root = fs::canonicalize(second_source.path()).unwrap();
    let second_destination = fs::canonicalize(second_target.path()).unwrap();
    let second_item = copy_item(&second_root, &second_destination, "copy", "photo.jpg");
    execute_authorized_plan(
        app_data.path(),
        "operation-2".to_string(),
        300,
        plan(&second_root, &second_destination, vec![second_item]),
    )
    .expect("second copy");
    fs::write(second_destination.join("photo.jpg"), b"user changed copy").expect("change copy");
    let rejected = undo_copy_operation(app_data.path(), "operation-2", &["copy".to_string()], 400)
        .expect("rejected undo result");
    assert_eq!(rejected.failed, 1);
    assert!(second_destination.join("photo.jpg").exists());
}

#[test]
fn move_recovery_restores_missing_originals_without_overwrite() {
    let source = tempdir().expect("source");
    let target = tempdir().expect("target");
    let app_data = tempdir().expect("app data");
    fs::write(source.path().join("photo.jpg"), b"move").expect("source file");
    let root = fs::canonicalize(source.path()).expect("source root");
    let destination = fs::canonicalize(target.path()).expect("target root");
    let item = move_item(&root, &destination, "move", "photo.jpg");
    execute_authorized_plan(
        app_data.path(),
        "operation-1".to_string(),
        100,
        move_plan(&root, &destination, vec![item]),
    )
    .expect("move");

    let restored =
        restore_move_operation(app_data.path(), "operation-1", &["move".to_string()], 200)
            .expect("restore move");

    assert_eq!(restored.succeeded, 1);
    assert_eq!(fs::read(root.join("photo.jpg")).unwrap(), b"move");
    assert!(!destination.join("photo.jpg").exists());
}

#[test]
fn partial_move_recovery_pauses_the_group_when_an_original_is_occupied() {
    let source = tempdir().expect("source");
    let target = tempdir().expect("target");
    let app_data = tempdir().expect("app data");
    fs::write(source.path().join("one.jpg"), b"one").expect("first source");
    fs::write(source.path().join("two.jpg"), b"two").expect("second source");
    let root = fs::canonicalize(source.path()).expect("source root");
    let destination = fs::canonicalize(target.path()).expect("target root");
    let mut item = move_item(&root, &destination, "pair", "one.jpg");
    item.members
        .extend(move_item(&root, &destination, "pair", "two.jpg").members);
    execute_authorized_plan_with_options(
        app_data.path(),
        "operation-1".to_string(),
        100,
        move_plan(&root, &destination, vec![item]),
        ExecutionOptions {
            force_copy_delete: true,
            fail_delete_at: Some(1),
            ..ExecutionOptions::default()
        },
    )
    .expect("partial move");

    let restored =
        restore_move_operation(app_data.path(), "operation-1", &["pair".to_string()], 200)
            .expect("partial restore");

    assert_eq!(restored.failed, 1);
    assert!(!root.join("one.jpg").exists());
    assert!(root.join("two.jpg").exists());
    assert!(destination.join("one.jpg").exists());
    assert!(destination.join("two.jpg").exists());
}
