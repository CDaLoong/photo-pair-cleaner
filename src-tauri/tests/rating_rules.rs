#[path = "../src/rating_rules.rs"]
mod rating_rules;

use rating_rules::{
    RatingCondition, RatingRule, RuleAction, RuleMemberKind, export_rules, import_rules,
    load_rules, save_rules, validate_rule_set,
};
use std::fs;

fn rule(id: &str, action: RuleAction) -> RatingRule {
    RatingRule {
        id: id.to_string(),
        name: format!("规则 {id}"),
        enabled: true,
        condition: RatingCondition::Between {
            minimum: 0,
            maximum: 5,
        },
        member_scope: vec![
            RuleMemberKind::Jpeg,
            RuleMemberKind::Raw,
            RuleMemberKind::Xmp,
        ],
        action,
        destination: None,
        preserve_relative_path: true,
    }
}

fn move_rule(id: &str, destination: &str) -> RatingRule {
    RatingRule {
        destination: Some(destination.to_string()),
        ..rule(id, RuleAction::Move)
    }
}

#[test]
fn conditions_match_only_the_configured_zero_to_five_range() {
    assert!(RatingCondition::Unrated.matches(0));
    assert!(!RatingCondition::Unrated.matches(1));
    assert!(RatingCondition::Equal { rating: 3 }.matches(3));
    assert!(!RatingCondition::Equal { rating: 3 }.matches(2));
    assert!(RatingCondition::AtLeast { rating: 4 }.matches(5));
    assert!(!RatingCondition::AtLeast { rating: 4 }.matches(3));
    assert!(RatingCondition::AtMost { rating: 2 }.matches(0));
    assert!(!RatingCondition::AtMost { rating: 2 }.matches(3));
    assert!(RatingCondition::Between {
        minimum: 2,
        maximum: 4,
    }
    .matches(2));
    assert!(RatingCondition::Between {
        minimum: 2,
        maximum: 4,
    }
    .matches(4));
    assert!(!RatingCondition::Between {
        minimum: 2,
        maximum: 4,
    }
    .matches(5));
}

#[test]
fn a_complete_rule_set_is_accepted_in_user_order() {
    let rules = vec![move_rule("high", "/archive"), rule("low", RuleAction::Cleanup)];
    let validated = validate_rule_set(&rules).unwrap();
    assert_eq!(validated[0].id, "high");
    assert_eq!(validated[1].id, "low");
}

#[test]
fn duplicate_ids_are_rejected_before_rule_specific_errors() {
    let rules = vec![move_rule("same", ""), move_rule("same", "")];
    let error = validate_rule_set(&rules).unwrap_err();
    assert!(error.contains("规则 ID 重复"));
}

#[test]
fn names_and_member_scopes_must_be_actionable() {
    let mut blank_name = rule("blank", RuleAction::Keep);
    blank_name.name = "   ".to_string();
    assert!(validate_rule_set(&[blank_name])
        .unwrap_err()
        .contains("规则名称不能为空"));

    let mut long_name = rule("long", RuleAction::Keep);
    long_name.name = "长".repeat(81);
    assert!(validate_rule_set(&[long_name])
        .unwrap_err()
        .contains("规则名称不能超过 80 个字符"));

    let mut empty_scope = rule("empty", RuleAction::Keep);
    empty_scope.member_scope.clear();
    assert!(validate_rule_set(&[empty_scope])
        .unwrap_err()
        .contains("至少选择一种处理格式"));

    let mut duplicate_scope = rule("duplicate", RuleAction::Keep);
    duplicate_scope.member_scope = vec![RuleMemberKind::Raw, RuleMemberKind::Raw];
    assert!(validate_rule_set(&[duplicate_scope])
        .unwrap_err()
        .contains("处理格式不能重复"));
}

#[test]
fn rating_conditions_stay_between_zero_and_five() {
    let mut invalid_equal = rule("equal", RuleAction::Keep);
    invalid_equal.condition = RatingCondition::Equal { rating: 6 };
    assert!(validate_rule_set(&[invalid_equal])
        .unwrap_err()
        .contains("评分必须在 0 到 5 星之间"));

    let mut reversed = rule("between", RuleAction::Keep);
    reversed.condition = RatingCondition::Between {
        minimum: 4,
        maximum: 2,
    };
    assert!(validate_rule_set(&[reversed])
        .unwrap_err()
        .contains("最低评分不能高于最高评分"));
}

#[test]
fn destinations_are_required_only_for_copy_and_move() {
    let missing_move = move_rule("move", "");
    assert!(validate_rule_set(&[missing_move])
        .unwrap_err()
        .contains("移动规则必须选择目标目录"));

    let mut missing_copy = rule("copy", RuleAction::Copy);
    missing_copy.destination = Some("  ".to_string());
    assert!(validate_rule_set(&[missing_copy])
        .unwrap_err()
        .contains("复制规则必须选择目标目录"));

    let mut unexpected = rule("cleanup", RuleAction::Cleanup);
    unexpected.destination = Some("/unused".to_string());
    assert!(validate_rule_set(&[unexpected])
        .unwrap_err()
        .contains("保留和待清理规则不能设置目标目录"));
}

#[test]
fn rule_sets_have_a_bounded_size() {
    let rules = (0..101)
        .map(|index| rule(&format!("rule-{index}"), RuleAction::Keep))
        .collect::<Vec<_>>();
    assert!(validate_rule_set(&rules)
        .unwrap_err()
        .contains("最多只能创建 100 条规则"));
}

#[test]
fn absent_rule_database_loads_an_empty_state() {
    let temp = tempfile::tempdir().unwrap();
    let state = load_rules(&temp.path().join("rating-rules.json")).unwrap();
    assert!(state.rules.is_empty());
}

#[test]
fn versioned_rules_round_trip_in_user_order() {
    let temp = tempfile::tempdir().unwrap();
    let database = temp.path().join("rating-rules.json");
    let input = vec![
        rule("low", RuleAction::Cleanup),
        move_rule("high", "/archive"),
    ];
    let saved = save_rules(&database, &input).unwrap();
    assert_eq!(saved.rules, input);
    assert_eq!(load_rules(&database).unwrap().rules, input);

    let json: serde_json::Value = serde_json::from_slice(&fs::read(database).unwrap()).unwrap();
    assert_eq!(json["version"], 1);
    assert_eq!(json["rules"][0]["id"], "low");
}

#[test]
fn damaged_existing_database_is_never_silently_overwritten() {
    let temp = tempfile::tempdir().unwrap();
    let database = temp.path().join("rating-rules.json");
    fs::write(&database, b"not-json").unwrap();
    let before = fs::read(&database).unwrap();

    assert!(save_rules(&database, &[rule("safe", RuleAction::Keep)])
        .unwrap_err()
        .contains("评分规则已损坏"));
    assert_eq!(fs::read(database).unwrap(), before);
}

#[test]
fn unsupported_unknown_and_oversized_imports_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let unsupported = temp.path().join("unsupported.json");
    fs::write(&unsupported, br#"{"version":2,"rules":[]}"#).unwrap();
    assert!(import_rules(&unsupported)
        .unwrap_err()
        .contains("不支持评分规则版本 2"));

    let unknown = temp.path().join("unknown.json");
    fs::write(&unknown, br#"{"version":1,"rules":[],"extra":true}"#).unwrap();
    assert!(import_rules(&unknown).unwrap_err().contains("评分规则已损坏"));

    let oversized = temp.path().join("oversized.json");
    let file = fs::File::create(&oversized).unwrap();
    file.set_len(4 * 1024 * 1024 + 1).unwrap();
    assert!(import_rules(&oversized)
        .unwrap_err()
        .contains("评分规则超过 4 MiB 上限"));
}

#[test]
fn imported_rules_are_validated_without_changing_the_app_database() {
    let temp = tempfile::tempdir().unwrap();
    let database = temp.path().join("rating-rules.json");
    save_rules(&database, &[rule("saved", RuleAction::Keep)]).unwrap();
    let import_path = temp.path().join("import.json");
    fs::write(
        &import_path,
        br#"{"version":1,"rules":[{"id":"bad","name":"Bad","enabled":true,"condition":{"type":"equal","rating":8},"memberScope":["jpeg"],"action":"keep","destination":null,"preserveRelativePath":true}]}"#,
    )
    .unwrap();

    assert!(import_rules(&import_path)
        .unwrap_err()
        .contains("评分必须在 0 到 5 星之间"));
    assert_eq!(load_rules(&database).unwrap().rules[0].id, "saved");
}

#[test]
fn export_requires_json_and_round_trips_the_same_envelope() {
    let temp = tempfile::tempdir().unwrap();
    let invalid = temp.path().join("rules.txt");
    assert!(export_rules(&invalid, &[rule("one", RuleAction::Keep)])
        .unwrap_err()
        .contains("导出文件必须使用 .json 扩展名"));

    let destination = temp.path().join("rules.json");
    let exported = export_rules(&destination, &[rule("one", RuleAction::Keep)]).unwrap();
    assert_eq!(exported, destination.to_string_lossy());
    assert_eq!(import_rules(&destination).unwrap().rules[0].id, "one");
}

#[cfg(unix)]
#[test]
fn symlinked_databases_and_exports_are_rejected() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target.json");
    fs::write(&target, br#"{"version":1,"rules":[]}"#).unwrap();
    let link = temp.path().join("link.json");
    symlink(&target, &link).unwrap();

    assert!(load_rules(&link)
        .unwrap_err()
        .contains("评分规则不是可信普通文件"));
    assert!(export_rules(&link, &[rule("one", RuleAction::Keep)])
        .unwrap_err()
        .contains("导出目标不是可信普通文件"));

    let broken = temp.path().join("broken.json");
    symlink(temp.path().join("missing.json"), &broken).unwrap();
    assert!(load_rules(&broken)
        .unwrap_err()
        .contains("评分规则不是可信普通文件"));
}
