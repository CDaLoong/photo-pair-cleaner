//! 删除类动作：移入隔离区或送系统回收站。
//!
//! 两者都可恢复——本应用不做不可逆删除。

use crate::file_organizer::*;

pub(crate) fn execute_quarantine_group(
    root: &Path,
    operation_id: &str,
    plan: &AuthorizedOperationPlan,
    item: &OperationPlanItem,
    options: ExecutionOptions,
) -> OperationGroupRecord {
    let operation_root = match crate::quarantine::operation_root(root, operation_id) {
        Ok(path) => path,
        Err(error) => return failed_group(item, OrganizerAction::Quarantine, error),
    };
    let mut validated = match validate_cleanup_sources(root, plan, item) {
        Ok(validated) => validated,
        Err(error) => return failed_group(item, OrganizerAction::Quarantine, error),
    };
    if let Err(error) = apply_cleanup_sync(root, item, &mut validated) {
        return failed_group(
            item,
            OrganizerAction::Quarantine,
            format!("清理前评分同步失败：{error}"),
        );
    }
    if let Err(error) = assign_quarantine_targets(root, &operation_root, &mut validated) {
        return failed_group(item, OrganizerAction::Quarantine, error);
    }
    if let Err(error) = fs::create_dir_all(&operation_root) {
        return failed_group(
            item,
            OrganizerAction::Quarantine,
            format!("无法创建评分隔离操作目录：{error}"),
        );
    }
    let canonical_operation_root = match canonical_directory(&operation_root, "评分隔离操作目录")
    {
        Ok(path) if path == operation_root => path,
        Ok(_) => {
            return failed_group(
                item,
                OrganizerAction::Quarantine,
                "评分隔离操作目录解析后发生变化".to_string(),
            );
        }
        Err(error) => return failed_group(item, OrganizerAction::Quarantine, error),
    };
    for member in &validated {
        if let Err(error) = ensure_target_parent(&canonical_operation_root, &member.target) {
            return failed_group(item, OrganizerAction::Quarantine, error);
        }
    }
    let same_volume = validated.iter().all(|member| {
        member
            .target
            .parent()
            .is_some_and(|parent| paths_share_device(&member.source, parent).unwrap_or(false))
    });
    if !same_volume {
        return failed_group(
            item,
            OrganizerAction::Quarantine,
            "FramePair 隔离区必须与照片根目录位于同一文件系统".to_string(),
        );
    }
    execute_rename_move(item, validated, options, OrganizerAction::Quarantine)
}

pub(crate) fn trash_record(member: &ValidatedMember, message: String) -> OperationMemberRecord {
    OperationMemberRecord {
        kind: member.kind,
        source_path: display_path(&member.source),
        target_path: String::new(),
        expected_size_bytes: member.expected_size_bytes,
        expected_modified_ms: member.expected_modified_ms,
        target_snapshot: None,
        message,
    }
}

pub(crate) fn execute_trash_group(
    root: &Path,
    plan: &AuthorizedOperationPlan,
    item: &OperationPlanItem,
    options: ExecutionOptions,
) -> OperationGroupRecord {
    let mut validated = match validate_cleanup_sources(root, plan, item) {
        Ok(validated) => validated,
        Err(error) => return failed_group(item, OrganizerAction::Trash, error),
    };
    if let Err(error) = apply_cleanup_sync(root, item, &mut validated) {
        return failed_group(
            item,
            OrganizerAction::Trash,
            format!("清理前评分同步失败：{error}"),
        );
    }
    let mut records = Vec::with_capacity(validated.len());
    for (index, member) in validated.iter().enumerate() {
        let result = if options.fail_trash_at == Some(index) {
            Err("测试注入的系统回收站失败".to_string())
        } else if options.simulate_trash {
            fs::remove_file(&member.source).map_err(|error| format!("模拟系统回收站失败：{error}"))
        } else {
            trash::delete(&member.source)
                .map_err(|error| format!("移入系统回收站/废纸篓失败：{error}"))
        };
        match result {
            Ok(()) => records.push(trash_record(
                member,
                "已移入系统回收站/废纸篓，FramePair 不提供应用内恢复".to_string(),
            )),
            Err(error) => {
                records.push(trash_record(member, error.clone()));
                records.extend(validated.iter().skip(index + 1).map(|remaining| {
                    trash_record(remaining, "因同组前序文件失败而停止清理".to_string())
                }));
                let succeeded = records
                    .iter()
                    .take(index)
                    .filter(|record| !Path::new(&record.source_path).exists())
                    .count();
                return relocation_result_group(
                    item,
                    OrganizerAction::Trash,
                    if succeeded == 0 {
                        OrganizerGroupStatus::Failed
                    } else {
                        OrganizerGroupStatus::Partial
                    },
                    format!(
                        "{} / {} 个文件已进入系统回收站；{error}",
                        succeeded,
                        validated.len()
                    ),
                    records,
                );
            }
        }
    }
    relocation_result_group(
        item,
        OrganizerAction::Trash,
        OrganizerGroupStatus::Success,
        format!("已将 {} 个文件移入系统回收站/废纸篓", records.len()),
        records,
    )
}
