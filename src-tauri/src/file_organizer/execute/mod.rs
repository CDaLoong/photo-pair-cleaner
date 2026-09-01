//! 执行已授权的整理计划。
//!
//! 每个照片组独立成败：一组失败不影响其它组，但组内是全有或全无。
//! 具体动作按类型分派到 moves / removal / copy 子模块。

use crate::file_organizer::*;

mod copy;
mod moves;
mod removal;

pub(crate) use copy::*;
pub(crate) use moves::*;
pub(crate) use removal::*;

pub(crate) fn assign_quarantine_targets(
    root: &Path,
    operation_root: &Path,
    members: &mut [ValidatedMember],
) -> Result<(), String> {
    for member in members {
        let relative = member
            .source
            .strip_prefix(root)
            .map_err(|_| "待隔离文件超出了照片根目录".to_string())?;
        member.target = operation_root.join(relative);
        validate_existing_target_ancestry(operation_root, &member.target)?;
    }
    Ok(())
}

pub(crate) fn record_for_committed_member(
    member: &ValidatedMember,
    snapshot: FileFingerprint,
    message: &str,
) -> OperationMemberRecord {
    OperationMemberRecord {
        kind: member.kind,
        source_path: display_path(&member.source),
        target_path: display_path(&member.target),
        expected_size_bytes: member.expected_size_bytes,
        expected_modified_ms: member.expected_modified_ms,
        target_snapshot: Some(snapshot),
        message: message.to_string(),
    }
}

pub(crate) fn move_result_group(
    item: &OperationPlanItem,
    status: OrganizerGroupStatus,
    message: String,
    members: Vec<OperationMemberRecord>,
) -> OperationGroupRecord {
    relocation_result_group(item, OrganizerAction::Move, status, message, members)
}

pub(crate) fn relocation_result_group(
    item: &OperationPlanItem,
    action: OrganizerAction,
    status: OrganizerGroupStatus,
    message: String,
    members: Vec<OperationMemberRecord>,
) -> OperationGroupRecord {
    OperationGroupRecord {
        group_id: item.group_id.clone(),
        relative_stem: item.relative_stem.clone(),
        action,
        status,
        message,
        members,
    }
}

pub(crate) fn failed_group(
    item: &OperationPlanItem,
    action: OrganizerAction,
    error: String,
) -> OperationGroupRecord {
    OperationGroupRecord {
        group_id: item.group_id.clone(),
        relative_stem: item.relative_stem.clone(),
        action,
        status: OrganizerGroupStatus::Failed,
        message: error.clone(),
        members: item
            .members
            .iter()
            .map(|member| OperationMemberRecord {
                kind: member.kind,
                source_path: member.source_relative_path.clone(),
                target_path: member.target_path.clone().unwrap_or_default(),
                expected_size_bytes: member.size_bytes,
                expected_modified_ms: member.modified_ms,
                target_snapshot: None,
                message: error.clone(),
            })
            .collect(),
    }
}

pub(crate) fn execute_authorized_plan(
    app_data_dir: &Path,
    operation_id: String,
    created_at_ms: u64,
    plan: AuthorizedOperationPlan,
) -> Result<OrganizerExecutionSummary, String> {
    execute_authorized_plan_with_options(
        app_data_dir,
        operation_id,
        created_at_ms,
        plan,
        ExecutionOptions::default(),
    )
}

pub(crate) fn execute_authorized_plan_with_options(
    app_data_dir: &Path,
    operation_id: String,
    created_at_ms: u64,
    plan: AuthorizedOperationPlan,
    options: ExecutionOptions,
) -> Result<OrganizerExecutionSummary, String> {
    let root = canonical_directory(Path::new(&plan.summary.root), "评分整理照片目录")?;
    let mut groups = Vec::with_capacity(plan.items.len());
    for item in &plan.items {
        let mut group = match item.terminal_action {
            Some(RuleAction::Copy) => execute_copy_group(&root, &plan, item),
            Some(RuleAction::Move) => execute_move_group(&root, &plan, item, options),
            Some(RuleAction::Cleanup)
                if plan.cleanup_destination == Some(CleanupExecutionDestination::Quarantine) =>
            {
                execute_quarantine_group(&root, &operation_id, &plan, item, options)
            }
            Some(RuleAction::Cleanup)
                if plan.cleanup_destination == Some(CleanupExecutionDestination::Trash) =>
            {
                execute_trash_group(&root, &plan, item, options)
            }
            Some(RuleAction::Cleanup) => {
                failed_group(item, OrganizerAction::Trash, "待清理目标缺失".to_string())
            }
            _ => failed_group(
                item,
                OrganizerAction::Copy,
                "当前仅允许执行复制、移动或清理".to_string(),
            ),
        };
        if matches!(group.action, OrganizerAction::Copy | OrganizerAction::Move)
            && matches!(
                group.status,
                OrganizerGroupStatus::Success | OrganizerGroupStatus::Partial
            )
            && let Err(error) = apply_destination_sync(&plan, item, &mut group)
        {
            group.status = OrganizerGroupStatus::Partial;
            group.message = format!("{}；评分同步未完成：{error}", group.message);
        }
        groups.push(group);
    }
    let manifest = OperationManifest::new(
        operation_id.clone(),
        plan.summary.plan_id.clone(),
        display_path(&root),
        created_at_ms,
        plan.rules,
        plan.sync,
        groups.clone(),
    );
    if let Err(error) = persist_manifest(app_data_dir, &manifest) {
        let rollback_failures = groups
            .iter()
            .filter(|group| {
                matches!(
                    group.status,
                    OrganizerGroupStatus::Success | OrganizerGroupStatus::Partial
                )
            })
            .flat_map(|group| match group.action {
                OrganizerAction::Copy => rollback_copies(&group.members),
                OrganizerAction::Move => {
                    rollback_moves_without_history(&root, &manifest.rules, &group.members)
                }
                OrganizerAction::Quarantine => rollback_renamed_moves(&group.members),
                OrganizerAction::Trash => Vec::new(),
            })
            .collect::<Vec<_>>();
        return Err(if rollback_failures.is_empty() {
            error
        } else {
            format!(
                "{error}；文件操作回滚失败：{}",
                rollback_failures.join("；")
            )
        });
    }
    Ok(OrganizerExecutionSummary {
        operation_id,
        plan_id: plan.summary.plan_id,
        succeeded: groups
            .iter()
            .filter(|group| group.status == OrganizerGroupStatus::Success)
            .count(),
        failed: groups
            .iter()
            .filter(|group| group.status == OrganizerGroupStatus::Failed)
            .count(),
        partial: groups
            .iter()
            .filter(|group| group.status == OrganizerGroupStatus::Partial)
            .count(),
        skipped: groups
            .iter()
            .filter(|group| group.status == OrganizerGroupStatus::Skipped)
            .count(),
        groups,
    })
}
