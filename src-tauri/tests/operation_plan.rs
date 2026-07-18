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

    let plan = build_operation_plan(&index, &request(&root, vec![matching]), "plan-1".into())
        .unwrap();
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
        vec![RuleMemberKind::Jpeg, RuleMemberKind::Raw, RuleMemberKind::Xmp],
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
    assert!(item
        .issues
        .iter()
        .any(|issue| issue.contains("评分来源不一致")));
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
