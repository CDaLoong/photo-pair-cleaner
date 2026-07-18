#[path = "../src/rating_rules.rs"]
mod rating_rules;

use rating_rules::{
    RatingCondition, RatingRule, RuleAction, RuleMemberKind, validate_rule_set,
};

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
