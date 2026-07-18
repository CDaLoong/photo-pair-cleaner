use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use tempfile::NamedTempFile;

const MAX_RULES: usize = 100;
const MAX_RULE_NAME_CHARS: usize = 80;
const RULE_DATABASE_VERSION: u8 = 1;
const MAX_RULE_DATABASE_BYTES: u64 = 4 * 1024 * 1024;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RatingRuleState {
    pub(crate) rules: Vec<RatingRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RatingRuleDatabase {
    version: u8,
    rules: Vec<RatingRule>,
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
        return Err(format!("规则名称不能超过 {MAX_RULE_NAME_CHARS} 个字符"));
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
        RuleAction::Copy if destination.is_empty() => Err("复制规则必须选择目标目录".to_string()),
        RuleAction::Move if destination.is_empty() => Err("移动规则必须选择目标目录".to_string()),
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

fn read_database(path: &Path) -> Result<RatingRuleDatabase, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(RatingRuleDatabase {
                version: RULE_DATABASE_VERSION,
                rules: Vec::new(),
            });
        }
        Err(error) => return Err(format!("无法读取评分规则文件信息：{error}")),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("评分规则不是可信普通文件".to_string());
    }
    if metadata.len() > MAX_RULE_DATABASE_BYTES {
        return Err("评分规则超过 4 MiB 上限".to_string());
    }
    let bytes = fs::read(path).map_err(|error| format!("无法读取评分规则：{error}"))?;
    let database: RatingRuleDatabase =
        serde_json::from_slice(&bytes).map_err(|error| format!("评分规则已损坏：{error}"))?;
    if database.version != RULE_DATABASE_VERSION {
        return Err(format!("不支持评分规则版本 {}", database.version));
    }
    validate_rule_set(&database.rules)?;
    Ok(database)
}

fn write_database(path: &Path, database: &RatingRuleDatabase, export: bool) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(database)
        .map_err(|error| format!("无法序列化评分规则：{error}"))?;
    if bytes.len() as u64 > MAX_RULE_DATABASE_BYTES {
        return Err("评分规则超过 4 MiB 上限".to_string());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "无法确定评分规则目录".to_string())?;
    if export {
        let metadata = fs::symlink_metadata(parent)
            .map_err(|error| format!("评分规则导出目录不可访问：{error}"))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("评分规则导出目录不是可信文件夹".to_string());
        }
    } else {
        fs::create_dir_all(parent).map_err(|error| format!("无法创建评分规则目录：{error}"))?;
    }
    let target_exists = match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(if export {
                    "导出目标不是可信普通文件".to_string()
                } else {
                    "评分规则不是可信普通文件".to_string()
                });
            }
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(format!("无法读取评分规则目标信息：{error}")),
    };
    let mut temporary = NamedTempFile::new_in(parent)
        .map_err(|error| format!("无法创建评分规则临时文件：{error}"))?;
    temporary
        .write_all(&bytes)
        .and_then(|_| temporary.as_file_mut().sync_all())
        .map_err(|error| format!("无法写入评分规则临时文件：{error}"))?;
    if target_exists {
        temporary
            .persist(path)
            .map_err(|error| format!("无法替换评分规则：{}", error.error))?;
    } else {
        temporary
            .persist_noclobber(path)
            .map_err(|error| format!("无法保存评分规则：{}", error.error))?;
    }
    #[cfg(unix)]
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("评分规则已写入，但目录同步失败：{error}"))?;
    Ok(())
}

pub(crate) fn load_rules(path: &Path) -> Result<RatingRuleState, String> {
    Ok(RatingRuleState {
        rules: read_database(path)?.rules,
    })
}

pub(crate) fn save_rules(path: &Path, rules: &[RatingRule]) -> Result<RatingRuleState, String> {
    let rules = validate_rule_set(rules)?;
    read_database(path)?;
    let database = RatingRuleDatabase {
        version: RULE_DATABASE_VERSION,
        rules: rules.clone(),
    };
    write_database(path, &database, false)?;
    Ok(RatingRuleState { rules })
}

pub(crate) fn import_rules(path: &Path) -> Result<RatingRuleState, String> {
    let extension = path
        .extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if extension != "json" {
        return Err("导入文件必须使用 .json 扩展名".to_string());
    }
    load_rules(path)
}

pub(crate) fn export_rules(path: &Path, rules: &[RatingRule]) -> Result<String, String> {
    let extension = path
        .extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if extension != "json" {
        return Err("导出文件必须使用 .json 扩展名".to_string());
    }
    let rules = validate_rule_set(rules)?;
    let database = RatingRuleDatabase {
        version: RULE_DATABASE_VERSION,
        rules,
    };
    write_database(path, &database, true)?;
    Ok(path.to_string_lossy().into_owned())
}
