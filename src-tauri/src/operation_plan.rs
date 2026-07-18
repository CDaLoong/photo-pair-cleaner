use crate::photo_groups::{PhotoIndex, PhotoMemberKind};
use crate::rating_rules::{
    RatingRule, RuleAction, RuleMemberKind, validate_rule_set,
};
use crate::rating_sync::{
    RatingConflictPolicy, RatingResolution, RatingSyncTarget, RatingSyncTargets, resolve_rating,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SyncTiming {
    Source,
    Destination,
    BeforeCleanup,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlannedSyncAction {
    pub(crate) target: RatingSyncTarget,
    pub(crate) target_path: String,
    pub(crate) target_rating: u8,
    pub(crate) timing: SyncTiming,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OperationSyncPreference {
    pub(crate) enabled: bool,
    pub(crate) targets: RatingSyncTargets,
    pub(crate) jpeg_write_confirmed: bool,
    pub(crate) sync_cleanup_before: bool,
}

impl Default for OperationSyncPreference {
    fn default() -> Self {
        Self {
            enabled: false,
            targets: RatingSyncTargets::default(),
            jpeg_write_confirmed: false,
            sync_cleanup_before: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OperationPlanRequest {
    pub(crate) root: String,
    pub(crate) rules: Vec<RatingRule>,
    pub(crate) conflict_policy: RatingConflictPolicy,
    #[serde(default)]
    pub(crate) sync: OperationSyncPreference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum OperationPlanStatus {
    Ready,
    Keep,
    Skipped,
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlannedMember {
    pub(crate) kind: RuleMemberKind,
    pub(crate) source_relative_path: String,
    pub(crate) target_path: Option<String>,
    pub(crate) size_bytes: u64,
    pub(crate) modified_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OperationPlanItem {
    pub(crate) group_id: String,
    pub(crate) relative_stem: String,
    pub(crate) rating: Option<u8>,
    pub(crate) frame_pair: u8,
    pub(crate) jpeg_metadata: Option<i8>,
    pub(crate) raw_xmp: Option<i8>,
    pub(crate) matched_rule_ids: Vec<String>,
    pub(crate) matched_rule_names: Vec<String>,
    pub(crate) terminal_action: Option<RuleAction>,
    pub(crate) status: OperationPlanStatus,
    pub(crate) members: Vec<PlannedMember>,
    pub(crate) missing_kinds: Vec<RuleMemberKind>,
    pub(crate) sync_actions: Vec<PlannedSyncAction>,
    pub(crate) issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OperationPlanSummary {
    pub(crate) plan_id: String,
    pub(crate) root: String,
    pub(crate) total_items: usize,
    pub(crate) ready: usize,
    pub(crate) kept: usize,
    pub(crate) skipped: usize,
    pub(crate) conflicts: usize,
    pub(crate) move_groups: usize,
    pub(crate) copy_groups: usize,
    pub(crate) cleanup_groups: usize,
    pub(crate) sync_groups: usize,
    pub(crate) jpeg_files: usize,
    pub(crate) raw_files: usize,
    pub(crate) xmp_files: usize,
    pub(crate) copy_bytes: u64,
    pub(crate) cleanup_bytes: u64,
    pub(crate) items: Vec<OperationPlanItem>,
}

#[derive(Debug, Clone)]
pub(crate) struct OperationPlan {
    summary: OperationPlanSummary,
    pub(crate) rules: Vec<RatingRule>,
    pub(crate) sync: OperationSyncPreference,
}

impl OperationPlan {
    pub(crate) fn summary(&self) -> &OperationPlanSummary {
        &self.summary
    }
}

#[derive(Default)]
pub(crate) struct OperationPlanStore {
    current: Mutex<Option<OperationPlan>>,
}

impl OperationPlanStore {
    pub(crate) fn replace(&self, plan: OperationPlan) -> Result<(), String> {
        *self
            .current
            .lock()
            .map_err(|_| "无法锁定评分整理计划".to_string())? = Some(plan);
        Ok(())
    }

    pub(crate) fn current_summary(&self) -> Result<Option<OperationPlanSummary>, String> {
        Ok(self
            .current
            .lock()
            .map_err(|_| "无法锁定评分整理计划".to_string())?
            .as_ref()
            .map(|plan| plan.summary.clone()))
    }
}

fn member_kind(kind: PhotoMemberKind) -> RuleMemberKind {
    match kind {
        PhotoMemberKind::Jpeg => RuleMemberKind::Jpeg,
        PhotoMemberKind::Raw => RuleMemberKind::Raw,
        PhotoMemberKind::Xmp => RuleMemberKind::Xmp,
    }
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn validate_request(
    index: &PhotoIndex,
    request: &OperationPlanRequest,
) -> Result<(String, Vec<RatingRule>), String> {
    let rules = validate_rule_set(&request.rules)?;
    if rules.is_empty() {
        return Err("请至少创建一条评分规则".to_string());
    }
    let root = fs::canonicalize(&request.root)
        .map_err(|error| format!("照片目录不可访问：{error}"))?;
    let index_root = fs::canonicalize(&index.root)
        .map_err(|error| format!("照片索引目录不可访问：{error}"))?;
    if root != index_root {
        return Err("评分整理目录与照片索引不一致".to_string());
    }
    if request.sync.enabled {
        if !request.sync.targets.raw_xmp && !request.sync.targets.jpeg_metadata {
            return Err("启用评分同步预览时必须至少选择一个目标".to_string());
        }
        if request.sync.targets.jpeg_metadata && !request.sync.jpeg_write_confirmed {
            return Err("启用 JPG 元数据同步前必须明确确认".to_string());
        }
    }
    Ok((display_path(&root), rules))
}

fn selected_members(
    asset: &crate::photo_groups::PhotoAsset,
    rule: &RatingRule,
) -> (Vec<PlannedMember>, Vec<RuleMemberKind>) {
    let members = asset
        .members
        .iter()
        .filter_map(|member| {
            let kind = member_kind(member.kind);
            rule.member_scope.contains(&kind).then(|| PlannedMember {
                kind,
                source_relative_path: member.relative_path.clone(),
                target_path: None,
                size_bytes: member.size_bytes,
                modified_ms: member.modified_ms,
            })
        })
        .collect::<Vec<_>>();
    let missing = rule
        .member_scope
        .iter()
        .copied()
        .filter(|kind| !members.iter().any(|member| member.kind == *kind))
        .collect();
    (members, missing)
}

fn summarize(plan_id: String, root: String, items: Vec<OperationPlanItem>) -> OperationPlanSummary {
    let ready = items
        .iter()
        .filter(|item| item.status == OperationPlanStatus::Ready)
        .count();
    let kept = items
        .iter()
        .filter(|item| item.status == OperationPlanStatus::Keep)
        .count();
    let skipped = items
        .iter()
        .filter(|item| item.status == OperationPlanStatus::Skipped)
        .count();
    let conflicts = items
        .iter()
        .filter(|item| item.status == OperationPlanStatus::Conflict)
        .count();
    let action_count = |action| {
        items
            .iter()
            .filter(|item| {
                item.status == OperationPlanStatus::Ready && item.terminal_action == Some(action)
            })
            .count()
    };
    let member_count = |kind| {
        items
            .iter()
            .flat_map(|item| &item.members)
            .filter(|member| member.kind == kind)
            .count()
    };
    let bytes_for = |action| {
        items
            .iter()
            .filter(|item| {
                item.status == OperationPlanStatus::Ready && item.terminal_action == Some(action)
            })
            .flat_map(|item| &item.members)
            .fold(0_u64, |total, member| total.saturating_add(member.size_bytes))
    };
    OperationPlanSummary {
        plan_id,
        root,
        total_items: items.len(),
        ready,
        kept,
        skipped,
        conflicts,
        move_groups: action_count(RuleAction::Move),
        copy_groups: action_count(RuleAction::Copy),
        cleanup_groups: action_count(RuleAction::Cleanup),
        sync_groups: items
            .iter()
            .filter(|item| !item.sync_actions.is_empty())
            .count(),
        jpeg_files: member_count(RuleMemberKind::Jpeg),
        raw_files: member_count(RuleMemberKind::Raw),
        xmp_files: member_count(RuleMemberKind::Xmp),
        copy_bytes: bytes_for(RuleAction::Copy),
        cleanup_bytes: bytes_for(RuleAction::Cleanup),
        items,
    }
}

pub(crate) fn build_operation_plan(
    index: &PhotoIndex,
    request: &OperationPlanRequest,
    plan_id: String,
) -> Result<OperationPlan, String> {
    if plan_id.trim().is_empty() {
        return Err("评分整理计划 ID 不能为空".to_string());
    }
    let (root, rules) = validate_request(index, request)?;
    let mut items = Vec::with_capacity(index.assets.len());

    for asset in &index.assets {
        let mut issues = asset.rating_issues.clone();
        let resolution = resolve_rating(&asset.rating_state, &issues, request.conflict_policy);
        let rating = match resolution {
            RatingResolution::Ready(rating) => Some(rating),
            RatingResolution::Conflict => {
                if issues.is_empty() {
                    issues.push("评分来源不一致，当前冲突策略不会覆盖".to_string());
                }
                None
            }
        };
        let matches = rating
            .map(|rating| {
                rules
                    .iter()
                    .filter(|rule| rule.enabled && rule.condition.matches(rating))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let matched_rule_ids = matches.iter().map(|rule| rule.id.clone()).collect::<Vec<_>>();
        let matched_rule_names = matches
            .iter()
            .map(|rule| rule.name.clone())
            .collect::<Vec<_>>();

        let (terminal_action, status, members, missing_kinds) = if rating.is_none() {
            (None, OperationPlanStatus::Conflict, Vec::new(), Vec::new())
        } else if matches.is_empty() {
            (None, OperationPlanStatus::Skipped, Vec::new(), Vec::new())
        } else if matches.len() > 1 {
            issues.push(format!(
                "照片组命中多条最终操作规则：{}",
                matched_rule_names.join("、")
            ));
            (None, OperationPlanStatus::Conflict, Vec::new(), Vec::new())
        } else {
            let rule = matches[0];
            let (members, missing) = selected_members(asset, rule);
            if members.is_empty() {
                issues.push("规则选择的格式在当前照片组中均不存在".to_string());
                (
                    Some(rule.action),
                    OperationPlanStatus::Skipped,
                    members,
                    missing,
                )
            } else {
                let status = if rule.action == RuleAction::Keep {
                    OperationPlanStatus::Keep
                } else {
                    OperationPlanStatus::Ready
                };
                (Some(rule.action), status, members, missing)
            }
        };

        items.push(OperationPlanItem {
            group_id: asset.id.clone(),
            relative_stem: asset.relative_stem.clone(),
            rating,
            frame_pair: asset.rating_state.frame_pair,
            jpeg_metadata: asset.rating_state.jpeg_metadata,
            raw_xmp: asset.rating_state.raw_xmp,
            matched_rule_ids,
            matched_rule_names,
            terminal_action,
            status,
            members,
            missing_kinds,
            sync_actions: Vec::new(),
            issues,
        });
    }
    items.sort_by(|left, right| left.group_id.cmp(&right.group_id));
    let summary = summarize(plan_id, root, items);
    Ok(OperationPlan {
        summary,
        rules,
        sync: request.sync,
    })
}
