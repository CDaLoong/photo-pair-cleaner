use serde::{Deserialize, Serialize};
use std::collections::HashSet;

const MAX_RULES: usize = 100;
const MAX_RULE_NAME_CHARS: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum RatingCondition {
    Unrated,
    Equal { rating: u8 },
    AtLeast { rating: u8 },
    AtMost { rating: u8 },
    Between { minimum: u8, maximum: u8 },
}

impl RatingCondition {
    pub(crate) fn matches(&self, rating: u8) -> bool {
        if rating > 5 {
            return false;
        }
        match self {
            Self::Unrated => rating == 0,
            Self::Equal { rating: expected } => rating == *expected,
            Self::AtLeast { rating: minimum } => rating >= *minimum,
            Self::AtMost { rating: maximum } => rating <= *maximum,
            Self::Between { minimum, maximum } => (*minimum..=*maximum).contains(&rating),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum RuleMemberKind {
    Jpeg,
    Raw,
    Xmp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum RuleAction {
    Keep,
    Copy,
    Move,
    Cleanup,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RatingRule {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) enabled: bool,
    pub(crate) condition: RatingCondition,
    pub(crate) member_scope: Vec<RuleMemberKind>,
    pub(crate) action: RuleAction,
    pub(crate) destination: Option<String>,
    pub(crate) preserve_relative_path: bool,
}

fn validate_rating(rating: u8) -> Result<(), String> {
    if rating > 5 {
        return Err("评分必须在 0 到 5 星之间".to_string());
    }
    Ok(())
}

fn validate_condition(condition: &RatingCondition) -> Result<(), String> {
    match condition {
        RatingCondition::Unrated => Ok(()),
        RatingCondition::Equal { rating }
        | RatingCondition::AtLeast { rating }
        | RatingCondition::AtMost { rating } => validate_rating(*rating),
        RatingCondition::Between { minimum, maximum } => {
            validate_rating(*minimum)?;
            validate_rating(*maximum)?;
            if minimum > maximum {
                return Err("最低评分不能高于最高评分".to_string());
            }
            Ok(())
        }
    }
}

fn validate_rule(rule: &RatingRule) -> Result<(), String> {
    if rule.id.trim().is_empty() {
        return Err("规则 ID 不能为空".to_string());
    }
    if rule.name.trim().is_empty() {
        return Err("规则名称不能为空".to_string());
    }
    if rule.name.chars().count() > MAX_RULE_NAME_CHARS {
        return Err(format!(
            "规则名称不能超过 {MAX_RULE_NAME_CHARS} 个字符"
        ));
    }
    validate_condition(&rule.condition)?;
    if rule.member_scope.is_empty() {
        return Err("至少选择一种处理格式".to_string());
    }
    let members = rule.member_scope.iter().copied().collect::<HashSet<_>>();
    if members.len() != rule.member_scope.len() {
        return Err("处理格式不能重复".to_string());
    }

    let destination = rule.destination.as_deref().map(str::trim).unwrap_or("");
    match rule.action {
        RuleAction::Copy if destination.is_empty() => {
            Err("复制规则必须选择目标目录".to_string())
        }
        RuleAction::Move if destination.is_empty() => {
            Err("移动规则必须选择目标目录".to_string())
        }
        RuleAction::Keep | RuleAction::Cleanup if !destination.is_empty() => {
            Err("保留和待清理规则不能设置目标目录".to_string())
        }
        _ => Ok(()),
    }
}

pub(crate) fn validate_rule_set(rules: &[RatingRule]) -> Result<Vec<RatingRule>, String> {
    if rules.len() > MAX_RULES {
        return Err(format!("最多只能创建 {MAX_RULES} 条规则"));
    }
    let mut ids = HashSet::new();
    for rule in rules {
        if !ids.insert(rule.id.trim()) {
            return Err(format!("规则 ID 重复：{}", rule.id));
        }
    }
    for rule in rules {
        validate_rule(rule).map_err(|error| format!("规则“{}”：{error}", rule.name.trim()))?;
    }
    Ok(rules.to_vec())
}
