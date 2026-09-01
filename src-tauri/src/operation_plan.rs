use crate::fs_util::display_path;
use crate::photo_groups::{PhotoIndex, PhotoMemberKind};
use crate::rating_rules::{RatingRule, RuleAction, RuleMemberKind, validate_rule_set};
use crate::rating_sync::{
    RatingConflictPolicy, RatingResolution, RatingSyncTarget, RatingSyncTargets, resolve_rating,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OperationSyncPreference {
    pub(crate) enabled: bool,
    pub(crate) targets: RatingSyncTargets,
    pub(crate) jpeg_write_confirmed: bool,
    pub(crate) sync_cleanup_before: bool,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum CleanupExecutionDestination {
    Quarantine,
    Trash,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExecutionSelection {
    pub(crate) plan_id: String,
    pub(crate) root: String,
    pub(crate) group_ids: Vec<String>,
    pub(crate) cleanup_destination: Option<CleanupExecutionDestination>,
}

#[derive(Debug, Clone)]
pub(crate) struct AuthorizedOperationPlan {
    pub(crate) summary: OperationPlanSummary,
    pub(crate) items: Vec<OperationPlanItem>,
    pub(crate) rules: Vec<RatingRule>,
    pub(crate) sync: OperationSyncPreference,
    pub(crate) cleanup_destination: Option<CleanupExecutionDestination>,
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

    /// 仅供测试使用：生产代码通过 `take_plan`/`peek_plan` 读取计划，
    /// 那两个入口同时负责保证「一次性计划」的约束。
    #[cfg(test)]
    pub(crate) fn current_summary(&self) -> Result<Option<OperationPlanSummary>, String> {
        Ok(self
            .current
            .lock()
            .map_err(|_| "无法锁定评分整理计划".to_string())?
            .as_ref()
            .map(|plan| plan.summary.clone()))
    }

    pub(crate) fn take_for_execution(
        &self,
        selection: &ExecutionSelection,
    ) -> Result<AuthorizedOperationPlan, String> {
        if selection.group_ids.is_empty() {
            return Err("请至少选择一个可执行照片组".to_string());
        }
        let mut selected_ids = HashSet::with_capacity(selection.group_ids.len());
        if selection
            .group_ids
            .iter()
            .any(|group_id| group_id.trim().is_empty() || !selected_ids.insert(group_id.as_str()))
        {
            return Err("执行照片组不能为空或重复".to_string());
        }
        let requested_root = fs::canonicalize(&selection.root)
            .map_err(|error| format!("评分整理目录不可访问：{error}"))?;
        let mut current = self
            .current
            .lock()
            .map_err(|_| "无法锁定评分整理计划".to_string())?;
        let plan = current
            .as_ref()
            .ok_or_else(|| "评分整理计划不存在或已经执行，请重新生成".to_string())?;
        if plan.summary.plan_id != selection.plan_id {
            return Err("评分整理计划已变化，请重新生成".to_string());
        }
        let planned_root = fs::canonicalize(&plan.summary.root)
            .map_err(|error| format!("计划中的评分整理目录不可访问：{error}"))?;
        if requested_root != planned_root {
            return Err("执行目录与评分整理计划不一致".to_string());
        }
        let mut items = Vec::with_capacity(selection.group_ids.len());
        for group_id in &selection.group_ids {
            let item = plan
                .summary
                .items
                .iter()
                .find(|item| item.group_id == *group_id)
                .ok_or_else(|| format!("评分整理计划中不存在照片组：{group_id}"))?;
            if item.status != OperationPlanStatus::Ready
                || !matches!(
                    item.terminal_action,
                    Some(RuleAction::Copy | RuleAction::Move | RuleAction::Cleanup)
                )
            {
                return Err(format!(
                    "照片组“{}”当前不可执行复制、移动或清理",
                    item.relative_stem
                ));
            }
            items.push(item.clone());
        }
        let contains_cleanup = items
            .iter()
            .any(|item| item.terminal_action == Some(RuleAction::Cleanup));
        if contains_cleanup != selection.cleanup_destination.is_some() {
            return Err(if contains_cleanup {
                "待清理照片组必须明确选择隔离区或系统回收站".to_string()
            } else {
                "当前选择不包含待清理照片组，不能设置清理目标".to_string()
            });
        }
        let plan = current
            .take()
            .expect("validated operation plan must remain available");
        Ok(AuthorizedOperationPlan {
            summary: plan.summary,
            items,
            rules: plan.rules,
            sync: plan.sync,
            cleanup_destination: selection.cleanup_destination,
        })
    }
}

fn member_kind(kind: PhotoMemberKind) -> RuleMemberKind {
    match kind {
        PhotoMemberKind::Jpeg => RuleMemberKind::Jpeg,
        PhotoMemberKind::Raw => RuleMemberKind::Raw,
        PhotoMemberKind::Xmp => RuleMemberKind::Xmp,
    }
}

fn validate_request(
    index: &PhotoIndex,
    request: &OperationPlanRequest,
) -> Result<(PathBuf, Vec<RatingRule>), String> {
    let mut rules = validate_rule_set(&request.rules)?;
    if rules.is_empty() {
        return Err("请至少创建一条评分规则".to_string());
    }
    let root =
        fs::canonicalize(&request.root).map_err(|error| format!("照片目录不可访问：{error}"))?;
    let index_root =
        fs::canonicalize(&index.root).map_err(|error| format!("照片索引目录不可访问：{error}"))?;
    if root != index_root {
        return Err("评分整理目录与照片索引不一致".to_string());
    }
    for rule in rules
        .iter_mut()
        .filter(|rule| rule.enabled && matches!(rule.action, RuleAction::Copy | RuleAction::Move))
    {
        let destination = rule.destination.as_deref().unwrap_or_default();
        let metadata = fs::symlink_metadata(destination)
            .map_err(|error| format!("规则“{}”的目标目录不可访问：{error}", rule.name))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(format!("规则“{}”的目标目录不是可信文件夹", rule.name));
        }
        let destination = fs::canonicalize(destination)
            .map_err(|error| format!("规则“{}”的目标目录不可访问：{error}", rule.name))?;
        if destination == root || destination.starts_with(&root) || root.starts_with(&destination) {
            return Err(format!(
                "规则“{}”的目标目录与照片目录不能相同或互相嵌套",
                rule.name
            ));
        }
        rule.destination = Some(display_path(&destination));
    }
    if request.sync.enabled {
        if !request.sync.targets.raw_xmp && !request.sync.targets.jpeg_metadata {
            return Err("启用评分同步预览时必须至少选择一个目标".to_string());
        }
        if request.sync.targets.jpeg_metadata && !request.sync.jpeg_write_confirmed {
            return Err("启用 JPG 元数据同步前必须明确确认".to_string());
        }
    }
    Ok((root, rules))
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

fn destination_target(
    destination: &Path,
    relative_path: &str,
    preserve_relative_path: bool,
) -> Result<PathBuf, String> {
    let relative = Path::new(relative_path);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("照片成员路径不是安全相对路径".to_string());
    }
    let suffix = if preserve_relative_path {
        relative.to_path_buf()
    } else {
        PathBuf::from(
            relative
                .file_name()
                .ok_or_else(|| "无法确定平铺目标文件名".to_string())?,
        )
    };
    Ok(destination.join(suffix))
}

fn target_path_issue(destination: &Path, target: &Path) -> Option<String> {
    if !target.starts_with(destination) {
        return Some("目标路径超出了规则目标目录".to_string());
    }
    match fs::symlink_metadata(target) {
        Ok(_) => return Some(format!("目标路径已存在：{}", display_path(target))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Some(format!(
                "无法检查目标路径 {}：{error}",
                display_path(target)
            ));
        }
    }
    let parent = target.parent()?;
    let relative_parent = parent.strip_prefix(destination).ok()?;
    let mut current = destination.to_path_buf();
    for component in relative_parent.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Some(format!(
                    "目标父目录不是可信文件夹：{}",
                    display_path(&current)
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => {
                return Some(format!(
                    "无法检查目标父目录 {}：{error}",
                    display_path(&current)
                ));
            }
        }
    }
    None
}

fn assign_member_targets(
    rule: &RatingRule,
    members: &mut [PlannedMember],
    issues: &mut Vec<String>,
) {
    if !matches!(rule.action, RuleAction::Copy | RuleAction::Move) {
        return;
    }
    let destination = Path::new(rule.destination.as_deref().unwrap_or_default());
    for member in members {
        match destination_target(
            destination,
            &member.source_relative_path,
            rule.preserve_relative_path,
        ) {
            Ok(target) => {
                if let Some(issue) = target_path_issue(destination, &target) {
                    issues.push(issue);
                }
                member.target_path = Some(display_path(&target));
            }
            Err(error) => issues.push(error),
        }
    }
}

fn sync_location(
    root: &Path,
    rule: &RatingRule,
    members: &[PlannedMember],
    kind: RuleMemberKind,
    source_relative_path: &str,
    destination_extension: Option<&str>,
    sync_cleanup_before: bool,
) -> Option<(PathBuf, SyncTiming)> {
    match rule.action {
        RuleAction::Cleanup if !sync_cleanup_before => None,
        RuleAction::Cleanup => Some((root.join(source_relative_path), SyncTiming::BeforeCleanup)),
        RuleAction::Copy | RuleAction::Move => members
            .iter()
            .find(|member| member.kind == kind)
            .and_then(|member| member.target_path.as_deref())
            .map(|target| {
                let mut target = PathBuf::from(target);
                if let Some(extension) = destination_extension {
                    target.set_extension(extension);
                }
                (target, SyncTiming::Destination)
            })
            .or_else(|| Some((root.join(source_relative_path), SyncTiming::Source))),
        RuleAction::Keep => Some((root.join(source_relative_path), SyncTiming::Source)),
    }
}

fn build_sync_actions(
    root: &Path,
    asset: &crate::photo_groups::PhotoAsset,
    rule: &RatingRule,
    members: &[PlannedMember],
    preference: OperationSyncPreference,
    target_rating: u8,
    issues: &mut Vec<String>,
) -> Vec<PlannedSyncAction> {
    if !preference.enabled
        || (rule.action == RuleAction::Cleanup && !preference.sync_cleanup_before)
    {
        return Vec::new();
    }
    let mut actions = Vec::new();
    if preference.targets.raw_xmp
        && !(asset.rating_state.raw_xmp == Some(target_rating as i8)
            || (target_rating == 0 && asset.rating_state.raw_xmp.is_none()))
    {
        if asset.raw_paths.len() != 1 {
            issues.push("评分同步需要照片组中恰好一个 RAW".to_string());
        } else if asset.xmp_paths.len() > 1 {
            issues.push("评分同步发现多个 RAW XMP，目标不明确".to_string());
        } else {
            let source_relative = asset.xmp_paths.first().cloned().unwrap_or_else(|| {
                display_path(&PathBuf::from(&asset.relative_stem).with_extension("xmp"))
            });
            if let Some((target, timing)) = sync_location(
                root,
                rule,
                members,
                RuleMemberKind::Raw,
                &source_relative,
                Some("xmp"),
                preference.sync_cleanup_before,
            ) {
                if timing == SyncTiming::Source
                    && matches!(rule.action, RuleAction::Copy | RuleAction::Move)
                {
                    issues.push("RAW XMP 评分同步要求把 RAW 或 XMP 加入本次处理范围".to_string());
                } else if timing == SyncTiming::Destination {
                    let destination = Path::new(rule.destination.as_deref().unwrap_or_default());
                    if let Some(issue) = target_path_issue(destination, &target) {
                        issues.push(issue);
                    }
                }
                actions.push(PlannedSyncAction {
                    target: RatingSyncTarget::RawXmp,
                    target_path: display_path(&target),
                    target_rating,
                    timing,
                });
            }
        }
    }
    if preference.targets.jpeg_metadata
        && !(asset.rating_state.jpeg_metadata == Some(target_rating as i8)
            || (target_rating == 0 && asset.rating_state.jpeg_metadata.is_none()))
    {
        if asset.jpeg_paths.len() != 1 {
            issues.push("评分同步需要照片组中恰好一个 JPG".to_string());
        } else if let Some((target, timing)) = sync_location(
            root,
            rule,
            members,
            RuleMemberKind::Jpeg,
            &asset.jpeg_paths[0],
            None,
            preference.sync_cleanup_before,
        ) {
            if timing == SyncTiming::Source
                && matches!(rule.action, RuleAction::Copy | RuleAction::Move)
            {
                issues.push("JPG 评分同步要求把 JPG 加入本次处理范围".to_string());
            } else if timing == SyncTiming::Destination {
                let destination = Path::new(rule.destination.as_deref().unwrap_or_default());
                if target_path_issue(destination, &target).is_some()
                    && !members
                        .iter()
                        .any(|member| member.target_path.as_deref() == Some(&display_path(&target)))
                {
                    issues.push(format!("JPG 评分同步目标已存在：{}", display_path(&target)));
                }
            }
            actions.push(PlannedSyncAction {
                target: RatingSyncTarget::JpegMetadata,
                target_path: display_path(&target),
                target_rating,
                timing,
            });
        }
    }
    actions
}

fn mark_duplicate_targets(items: &mut [OperationPlanItem]) {
    let mut owners = HashMap::<String, Vec<usize>>::new();
    for (index, item) in items.iter().enumerate() {
        if item.status == OperationPlanStatus::Conflict {
            continue;
        }
        for target in item
            .members
            .iter()
            .filter_map(|member| member.target_path.as_ref())
        {
            owners.entry(target.to_lowercase()).or_default().push(index);
        }
    }
    for indexes in owners.values().filter(|indexes| indexes.len() > 1) {
        for index in indexes {
            let item = &mut items[*index];
            item.status = OperationPlanStatus::Conflict;
            if !item
                .issues
                .iter()
                .any(|issue| issue.contains("多个成员映射到同一目标路径"))
            {
                item.issues
                    .push("多个成员映射到同一目标路径，禁止覆盖或自动重命名".to_string());
            }
        }
    }
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
            .fold(0_u64, |total, member| {
                total.saturating_add(member.size_bytes)
            })
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
        let matched_rule_ids = matches
            .iter()
            .map(|rule| rule.id.clone())
            .collect::<Vec<_>>();
        let matched_rule_names = matches
            .iter()
            .map(|rule| rule.name.clone())
            .collect::<Vec<_>>();

        let (terminal_action, status, members, missing_kinds, sync_actions) = if rating.is_none() {
            (
                None,
                OperationPlanStatus::Conflict,
                Vec::new(),
                Vec::new(),
                Vec::new(),
            )
        } else if matches.is_empty() {
            (
                None,
                OperationPlanStatus::Skipped,
                Vec::new(),
                Vec::new(),
                Vec::new(),
            )
        } else if matches.len() > 1 {
            issues.push(format!(
                "照片组命中多条最终操作规则：{}",
                matched_rule_names.join("、")
            ));
            (
                None,
                OperationPlanStatus::Conflict,
                Vec::new(),
                Vec::new(),
                Vec::new(),
            )
        } else {
            let rule = matches[0];
            let (mut members, missing) = selected_members(asset, rule);
            if members.is_empty() {
                issues.push("规则选择的格式在当前照片组中均不存在".to_string());
                (
                    Some(rule.action),
                    OperationPlanStatus::Skipped,
                    members,
                    missing,
                    Vec::new(),
                )
            } else {
                let target_rating = match rating {
                    Some(rating) => rating,
                    None => unreachable!("resolved rating must exist for a matched rule"),
                };
                let issue_count = issues.len();
                assign_member_targets(rule, &mut members, &mut issues);
                let sync_actions = build_sync_actions(
                    &root,
                    asset,
                    rule,
                    &members,
                    request.sync,
                    target_rating,
                    &mut issues,
                );
                let status = if issues.len() > issue_count {
                    OperationPlanStatus::Conflict
                } else if rule.action == RuleAction::Keep {
                    OperationPlanStatus::Keep
                } else {
                    OperationPlanStatus::Ready
                };
                (Some(rule.action), status, members, missing, sync_actions)
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
            sync_actions,
            issues,
        });
    }
    items.sort_by(|left, right| left.group_id.cmp(&right.group_id));
    mark_duplicate_targets(&mut items);
    let summary = summarize(plan_id, display_path(&root), items);
    Ok(OperationPlan {
        summary,
        rules,
        sync: request.sync,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn item(group_id: &str, action: RuleAction, status: OperationPlanStatus) -> OperationPlanItem {
        OperationPlanItem {
            group_id: group_id.to_string(),
            relative_stem: group_id.to_string(),
            rating: Some(3),
            frame_pair: 3,
            jpeg_metadata: None,
            raw_xmp: None,
            matched_rule_ids: vec!["rule-1".to_string()],
            matched_rule_names: vec!["测试规则".to_string()],
            terminal_action: Some(action),
            status,
            members: Vec::new(),
            missing_kinds: Vec::new(),
            sync_actions: Vec::new(),
            issues: Vec::new(),
        }
    }

    fn plan(root: &Path) -> OperationPlan {
        OperationPlan {
            summary: summarize(
                "plan-1".to_string(),
                display_path(root),
                vec![
                    item("copy", RuleAction::Copy, OperationPlanStatus::Ready),
                    item("move", RuleAction::Move, OperationPlanStatus::Ready),
                    item("cleanup", RuleAction::Cleanup, OperationPlanStatus::Ready),
                    item("conflict", RuleAction::Copy, OperationPlanStatus::Conflict),
                ],
            ),
            rules: Vec::new(),
            sync: OperationSyncPreference::default(),
        }
    }

    fn selection(
        root: &Path,
        groups: &[&str],
        cleanup_destination: Option<CleanupExecutionDestination>,
    ) -> ExecutionSelection {
        ExecutionSelection {
            plan_id: "plan-1".to_string(),
            root: display_path(root),
            group_ids: groups.iter().map(|group| (*group).to_string()).collect(),
            cleanup_destination,
        }
    }

    #[test]
    fn execution_selection_is_authorized_once() {
        let directory = tempdir().expect("tempdir");
        let root = fs::canonicalize(directory.path()).expect("canonical root");
        let store = OperationPlanStore::default();
        store.replace(plan(&root)).expect("store plan");

        let authorized = store
            .take_for_execution(&selection(&root, &["copy", "move"], None))
            .expect("authorized selection");

        assert_eq!(authorized.summary.plan_id, "plan-1");
        assert_eq!(authorized.items.len(), 2);
        assert_eq!(authorized.items[0].group_id, "copy");
        assert!(store.current_summary().expect("summary").is_none());
        assert!(
            store
                .take_for_execution(&selection(&root, &["copy"], None))
                .is_err()
        );
    }

    #[test]
    fn execution_rejects_invalid_groups_without_consuming_the_plan() {
        let directory = tempdir().expect("tempdir");
        let root = fs::canonicalize(directory.path()).expect("canonical root");
        for groups in [
            vec!["copy", "copy"],
            vec!["cleanup"],
            vec!["conflict"],
            vec!["missing"],
        ] {
            let store = OperationPlanStore::default();
            store.replace(plan(&root)).expect("store plan");
            assert!(
                store
                    .take_for_execution(&selection(&root, &groups, None))
                    .is_err()
            );
            assert!(store.current_summary().expect("summary").is_some());
        }
    }

    #[test]
    fn execution_rejects_a_changed_root_without_consuming_the_plan() {
        let source = tempdir().expect("source");
        let other = tempdir().expect("other");
        let root = fs::canonicalize(source.path()).expect("canonical source");
        let other_root = fs::canonicalize(other.path()).expect("canonical other");
        let store = OperationPlanStore::default();
        store.replace(plan(&root)).expect("store plan");

        assert!(
            store
                .take_for_execution(&selection(&other_root, &["copy"], None))
                .is_err()
        );
        assert!(store.current_summary().expect("summary").is_some());
    }

    #[test]
    fn cleanup_execution_requires_an_explicit_destination() {
        let directory = tempdir().expect("tempdir");
        let root = fs::canonicalize(directory.path()).expect("canonical root");
        for destination in [
            CleanupExecutionDestination::Quarantine,
            CleanupExecutionDestination::Trash,
        ] {
            let store = OperationPlanStore::default();
            store.replace(plan(&root)).expect("store plan");
            let authorized = store
                .take_for_execution(&selection(&root, &["cleanup"], Some(destination)))
                .expect("authorize cleanup");
            assert_eq!(authorized.cleanup_destination, Some(destination));
            assert_eq!(authorized.items[0].group_id, "cleanup");
        }

        let store = OperationPlanStore::default();
        store.replace(plan(&root)).expect("store plan");
        assert!(
            store
                .take_for_execution(&selection(&root, &["cleanup"], None))
                .is_err()
        );
        assert!(store.current_summary().expect("summary").is_some());
    }

    #[test]
    fn cleanup_destination_is_rejected_without_a_cleanup_group() {
        let directory = tempdir().expect("tempdir");
        let root = fs::canonicalize(directory.path()).expect("canonical root");
        let store = OperationPlanStore::default();
        store.replace(plan(&root)).expect("store plan");

        assert!(
            store
                .take_for_execution(&selection(
                    &root,
                    &["copy"],
                    Some(CleanupExecutionDestination::Quarantine),
                ))
                .is_err()
        );
        assert!(store.current_summary().expect("summary").is_some());
    }
}
