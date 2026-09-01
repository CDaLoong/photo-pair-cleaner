//! 复制类动作：保留原件，在目标位置生成副本。

use crate::file_organizer::*;

pub(crate) fn execute_copy_group(
    root: &Path,
    plan: &AuthorizedOperationPlan,
    item: &OperationPlanItem,
) -> OperationGroupRecord {
    if let Err(error) = validate_sync_actions(item) {
        return failed_group(item, OrganizerAction::Copy, error);
    }
    let rule = match matching_rule(plan, item) {
        Ok(rule) => rule,
        Err(error) => return failed_group(item, OrganizerAction::Copy, error),
    };
    let destination = match rule
        .destination
        .as_deref()
        .ok_or_else(|| "复制规则缺少目标目录".to_string())
        .and_then(|path| canonical_directory(Path::new(path), "评分整理目标目录"))
    {
        Ok(destination) => destination,
        Err(error) => return failed_group(item, OrganizerAction::Copy, error),
    };
    let validated = match validate_group(root, plan, item) {
        Ok(validated) => validated,
        Err(error) => return failed_group(item, OrganizerAction::Copy, error),
    };
    let mut staged = Vec::with_capacity(validated.len());
    for member in validated {
        match stream_copy_to_temporary(member, &destination) {
            Ok(member) => staged.push(member),
            Err(error) => return failed_group(item, OrganizerAction::Copy, error),
        }
    }
    let mut records = Vec::with_capacity(staged.len());
    for staged_member in staged {
        let StagedCopy {
            member,
            temporary,
            sha256,
        } = staged_member;
        if let Err(error) = validate_existing_target_ancestry(&destination, &member.target) {
            let rollback_failures = rollback_copies(&records);
            let detail = if rollback_failures.is_empty() {
                error
            } else {
                format!("{error}；回滚失败：{}", rollback_failures.join("；"))
            };
            return failed_group(item, OrganizerAction::Copy, detail);
        }
        let persisted = match temporary.persist_noclobber(&member.target) {
            Ok(file) => file,
            Err(error) => {
                let rollback_failures = rollback_copies(&records);
                let mut detail = format!("提交复制目标失败：{}", error.error);
                if !rollback_failures.is_empty() {
                    detail.push_str(&format!("；回滚失败：{}", rollback_failures.join("；")));
                }
                return failed_group(item, OrganizerAction::Copy, detail);
            }
        };
        if let Err(error) = persisted.sync_all() {
            let _ = fs::remove_file(&member.target);
            let rollback_failures = rollback_copies(&records);
            let mut detail = format!("同步复制目标失败：{error}");
            if !rollback_failures.is_empty() {
                detail.push_str(&format!("；回滚失败：{}", rollback_failures.join("；")));
            }
            return failed_group(item, OrganizerAction::Copy, detail);
        }
        let snapshot = match fingerprint(&member.target) {
            Ok(snapshot) if snapshot.sha256 == sha256 => snapshot,
            Ok(_) => {
                let _ = fs::remove_file(&member.target);
                let rollback_failures = rollback_copies(&records);
                return failed_group(
                    item,
                    OrganizerAction::Copy,
                    format!(
                        "复制目标提交后内容校验失败；回滚：{}",
                        rollback_failures.join("；")
                    ),
                );
            }
            Err(error) => {
                let _ = fs::remove_file(&member.target);
                let rollback_failures = rollback_copies(&records);
                return failed_group(
                    item,
                    OrganizerAction::Copy,
                    format!("{error}；回滚：{}", rollback_failures.join("；")),
                );
            }
        };
        records.push(OperationMemberRecord {
            kind: member.kind,
            source_path: display_path(&member.source),
            target_path: display_path(&member.target),
            expected_size_bytes: member.expected_size_bytes,
            expected_modified_ms: member.expected_modified_ms,
            target_snapshot: Some(snapshot),
            message: "已复制并校验".to_string(),
        });
    }
    OperationGroupRecord {
        group_id: item.group_id.clone(),
        relative_stem: item.relative_stem.clone(),
        action: OrganizerAction::Copy,
        status: OrganizerGroupStatus::Success,
        message: format!("已复制并校验 {} 个文件", records.len()),
        members: records,
    }
}
