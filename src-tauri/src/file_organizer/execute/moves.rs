//! 移动类动作。
//!
//! 优先走同设备 rename（原子且瞬时）；跨设备时退化为「拷贝到临时文件 →
//! 校验 → 改名 → 删源」，每一步都可回滚。

use crate::file_organizer::*;

pub(crate) fn execute_rename_move(
    item: &OperationPlanItem,
    validated: Vec<ValidatedMember>,
    options: ExecutionOptions,
    action: OrganizerAction,
) -> OperationGroupRecord {
    let mut records = Vec::with_capacity(validated.len());
    for (index, member) in validated.into_iter().enumerate() {
        if options.fail_rename_at == Some(index) {
            let rollback_failures = rollback_renamed_moves(&records);
            return if rollback_failures.is_empty() {
                failed_group(item, action, "移动重命名失败，已完整回滚".to_string())
            } else {
                relocation_result_group(
                    item,
                    action,
                    OrganizerGroupStatus::Partial,
                    format!(
                        "移动重命名失败且回滚不完整：{}",
                        rollback_failures.join("；")
                    ),
                    records,
                )
            };
        }
        if let Err(error) = match fs::symlink_metadata(&member.target) {
            Ok(_) => Err(format!("目标路径已存在：{}", display_path(&member.target))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("无法检查目标路径：{error}")),
        } {
            let rollback_failures = rollback_renamed_moves(&records);
            return if rollback_failures.is_empty() {
                failed_group(item, action, error)
            } else {
                relocation_result_group(
                    item,
                    action,
                    OrganizerGroupStatus::Partial,
                    format!("{error}；回滚不完整：{}", rollback_failures.join("；")),
                    records,
                )
            };
        }
        let source_metadata = match fs::symlink_metadata(&member.source) {
            Ok(metadata) => metadata,
            Err(error) => {
                let rollback_failures = rollback_renamed_moves(&records);
                return if rollback_failures.is_empty() {
                    failed_group(item, action, format!("移动源文件不可访问：{error}"))
                } else {
                    relocation_result_group(
                        item,
                        action,
                        OrganizerGroupStatus::Partial,
                        format!(
                            "移动源文件不可访问且回滚不完整：{}",
                            rollback_failures.join("；")
                        ),
                        records,
                    )
                };
            }
        };
        if source_metadata.file_type().is_symlink()
            || !source_metadata.is_file()
            || source_metadata.len() != member.expected_size_bytes
            || modified_ms(&source_metadata) != member.expected_modified_ms
        {
            let rollback_failures = rollback_renamed_moves(&records);
            return if rollback_failures.is_empty() {
                failed_group(item, action, "移动源文件发生变化，已回滚".to_string())
            } else {
                relocation_result_group(
                    item,
                    action,
                    OrganizerGroupStatus::Partial,
                    format!(
                        "移动源文件发生变化且回滚不完整：{}",
                        rollback_failures.join("；")
                    ),
                    records,
                )
            };
        }
        if let Err(error) = fs::rename(&member.source, &member.target) {
            let rollback_failures = rollback_renamed_moves(&records);
            return if rollback_failures.is_empty() {
                failed_group(item, action, format!("移动文件失败：{error}；已回滚"))
            } else {
                relocation_result_group(
                    item,
                    action,
                    OrganizerGroupStatus::Partial,
                    format!(
                        "移动文件失败：{error}；回滚不完整：{}",
                        rollback_failures.join("；")
                    ),
                    records,
                )
            };
        }
        match fingerprint(&member.target) {
            Ok(snapshot) => {
                records.push(record_for_committed_member(&member, snapshot, "已原子移动"))
            }
            Err(error) => {
                let _ = fs::rename(&member.target, &member.source);
                let rollback_failures = rollback_renamed_moves(&records);
                return if rollback_failures.is_empty() {
                    failed_group(item, action, error)
                } else {
                    relocation_result_group(
                        item,
                        action,
                        OrganizerGroupStatus::Partial,
                        format!("{error}；回滚不完整：{}", rollback_failures.join("；")),
                        records,
                    )
                };
            }
        }
    }
    relocation_result_group(
        item,
        action,
        OrganizerGroupStatus::Success,
        format!("已原子移动 {} 个文件", records.len()),
        records,
    )
}

pub(crate) fn commit_staged_move(
    item: &OperationPlanItem,
    destination: &Path,
    staged: Vec<StagedCopy>,
) -> Result<Vec<OperationMemberRecord>, OperationGroupRecord> {
    let mut records = Vec::with_capacity(staged.len());
    for staged_member in staged {
        let StagedCopy {
            member,
            temporary,
            sha256,
        } = staged_member;
        if let Err(error) = validate_existing_target_ancestry(destination, &member.target) {
            let rollback_failures = rollback_copies(&records);
            return Err(if rollback_failures.is_empty() {
                failed_group(item, OrganizerAction::Move, error)
            } else {
                move_result_group(
                    item,
                    OrganizerGroupStatus::Partial,
                    format!(
                        "提交移动目标失败且回滚不完整：{}",
                        rollback_failures.join("；")
                    ),
                    records,
                )
            });
        }
        let file = match temporary.persist_noclobber(&member.target) {
            Ok(file) => file,
            Err(error) => {
                let rollback_failures = rollback_copies(&records);
                return Err(if rollback_failures.is_empty() {
                    failed_group(
                        item,
                        OrganizerAction::Move,
                        format!("提交移动目标失败：{}", error.error),
                    )
                } else {
                    move_result_group(
                        item,
                        OrganizerGroupStatus::Partial,
                        format!(
                            "提交移动目标失败且回滚不完整：{}",
                            rollback_failures.join("；")
                        ),
                        records,
                    )
                });
            }
        };
        if let Err(error) = file.sync_all() {
            let _ = fs::remove_file(&member.target);
            let rollback_failures = rollback_copies(&records);
            return Err(if rollback_failures.is_empty() {
                failed_group(
                    item,
                    OrganizerAction::Move,
                    format!("同步移动目标失败：{error}"),
                )
            } else {
                move_result_group(
                    item,
                    OrganizerGroupStatus::Partial,
                    format!(
                        "同步移动目标失败且回滚不完整：{}",
                        rollback_failures.join("；")
                    ),
                    records,
                )
            });
        }
        let snapshot = match fingerprint(&member.target) {
            Ok(snapshot) if snapshot.sha256 == sha256 => snapshot,
            Ok(_) => {
                let _ = fs::remove_file(&member.target);
                let rollback_failures = rollback_copies(&records);
                return Err(if rollback_failures.is_empty() {
                    failed_group(
                        item,
                        OrganizerAction::Move,
                        "移动目标内容校验失败".to_string(),
                    )
                } else {
                    move_result_group(
                        item,
                        OrganizerGroupStatus::Partial,
                        format!(
                            "移动目标校验失败且回滚不完整：{}",
                            rollback_failures.join("；")
                        ),
                        records,
                    )
                });
            }
            Err(error) => {
                let _ = fs::remove_file(&member.target);
                let rollback_failures = rollback_copies(&records);
                return Err(if rollback_failures.is_empty() {
                    failed_group(item, OrganizerAction::Move, error)
                } else {
                    move_result_group(
                        item,
                        OrganizerGroupStatus::Partial,
                        format!("目标复核失败且回滚不完整：{}", rollback_failures.join("；")),
                        records,
                    )
                });
            }
        };
        records.push(record_for_committed_member(
            &member,
            snapshot,
            "目标已复制并校验，等待删除源文件",
        ));
    }
    Ok(records)
}

pub(crate) fn execute_copy_delete_move(
    item: &OperationPlanItem,
    destination: &Path,
    validated: Vec<ValidatedMember>,
    options: ExecutionOptions,
) -> OperationGroupRecord {
    let mut staged = Vec::with_capacity(validated.len());
    for member in validated {
        match stream_copy_to_temporary(member, destination) {
            Ok(member) => staged.push(member),
            Err(error) => return failed_group(item, OrganizerAction::Move, error),
        }
    }
    let mut records = match commit_staged_move(item, destination, staged) {
        Ok(records) => records,
        Err(group) => return group,
    };
    for index in 0..records.len() {
        let record = &records[index];
        let source = Path::new(&record.source_path);
        let source_valid = source_matches_record(record)
            && record.target_snapshot.as_ref().is_some_and(|target| {
                fingerprint(source).is_ok_and(|source| {
                    source.size_bytes == target.size_bytes && source.sha256 == target.sha256
                })
            });
        let delete_result = if options.fail_delete_at == Some(index) {
            Err(std::io::Error::other("测试注入的源文件删除失败"))
        } else if !source_valid {
            Err(std::io::Error::other("源文件在删除前发生变化"))
        } else {
            fs::remove_file(source)
        };
        if let Err(error) = delete_result {
            for member in &mut records {
                member.message = if Path::new(&member.source_path).exists() {
                    "目标已校验，源文件仍保留".to_string()
                } else {
                    "目标已校验，源文件已删除".to_string()
                };
            }
            return move_result_group(
                item,
                OrganizerGroupStatus::Partial,
                format!("所有目标均已校验，但删除源文件未全部完成：{error}"),
                records,
            );
        }
        records[index].message = "已跨盘移动并校验".to_string();
    }
    move_result_group(
        item,
        OrganizerGroupStatus::Success,
        format!("已跨盘移动并校验 {} 个文件", records.len()),
        records,
    )
}

pub(crate) fn execute_move_group(
    root: &Path,
    plan: &AuthorizedOperationPlan,
    item: &OperationPlanItem,
    options: ExecutionOptions,
) -> OperationGroupRecord {
    if let Err(error) = validate_sync_actions(item) {
        return failed_group(item, OrganizerAction::Move, error);
    }
    let rule = match matching_rule(plan, item) {
        Ok(rule) => rule,
        Err(error) => return failed_group(item, OrganizerAction::Move, error),
    };
    let destination = match rule
        .destination
        .as_deref()
        .ok_or_else(|| "移动规则缺少目标目录".to_string())
        .and_then(|path| canonical_directory(Path::new(path), "评分整理目标目录"))
    {
        Ok(destination) => destination,
        Err(error) => return failed_group(item, OrganizerAction::Move, error),
    };
    let validated = match validate_group(root, plan, item) {
        Ok(validated) => validated,
        Err(error) => return failed_group(item, OrganizerAction::Move, error),
    };
    for member in &validated {
        if let Err(error) = ensure_target_parent(&destination, &member.target) {
            return failed_group(item, OrganizerAction::Move, error);
        }
    }
    let same_volume = !options.force_copy_delete
        && validated.iter().all(|member| {
            member
                .target
                .parent()
                .is_some_and(|parent| paths_share_device(&member.source, parent).unwrap_or(false))
        });
    if same_volume {
        execute_rename_move(item, validated, options, OrganizerAction::Move)
    } else {
        execute_copy_delete_move(item, &destination, validated, options)
    }
}
