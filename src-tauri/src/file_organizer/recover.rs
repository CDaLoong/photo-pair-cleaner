//! 撤销已完成的整理操作：移动还原、隔离区还原、复制撤销。
//!
//! 三条路径共用 recover_operation，差别只在于目标位置怎么算。
//! 恢复前会 preflight 一遍，任何一个成员对不上就整组不动。

use super::*;

pub(crate) fn validate_recovery_selection(group_ids: &[String]) -> Result<HashSet<&str>, String> {
    if group_ids.is_empty() {
        return Err("请至少选择一个可恢复照片组".to_string());
    }
    let mut selected = HashSet::with_capacity(group_ids.len());
    if group_ids
        .iter()
        .any(|group_id| group_id.trim().is_empty() || !selected.insert(group_id.as_str()))
    {
        return Err("恢复照片组不能为空或重复".to_string());
    }
    Ok(selected)
}

pub(crate) fn recovery_status(results: &[RecoveryMemberResult]) -> OrganizerGroupStatus {
    let succeeded = results.iter().filter(|result| result.success).count();
    if succeeded == results.len() && !results.is_empty() {
        OrganizerGroupStatus::Success
    } else if succeeded == 0 {
        OrganizerGroupStatus::Failed
    } else {
        OrganizerGroupStatus::Partial
    }
}

pub(crate) fn trusted_history_source(root: &Path, raw: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(raw);
    if !path.is_absolute() || !path.starts_with(root) || path == root {
        return Err("历史记录中的原始路径超出了照片目录".to_string());
    }
    Ok(path)
}

pub(crate) fn trusted_history_target(rules: &[RatingRule], raw: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return Err("历史记录中的目标路径不是绝对路径".to_string());
    }
    let trusted = rules.iter().any(|rule| {
        rule.destination
            .as_deref()
            .map(Path::new)
            .is_some_and(|destination| path.starts_with(destination) && path != destination)
    });
    if !trusted {
        return Err("历史记录中的目标路径超出了规则目录".to_string());
    }
    Ok(path)
}

pub(crate) fn trusted_quarantine_target(
    root: &Path,
    operation_id: &str,
    raw: &str,
) -> Result<PathBuf, String> {
    let path = PathBuf::from(raw);
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err("历史记录中的隔离路径无效".to_string());
    }
    let operation_root = crate::quarantine::operation_root(root, operation_id)?;
    if path == operation_root || !path.starts_with(&operation_root) {
        return Err("历史记录中的隔离路径超出了本次操作目录".to_string());
    }
    Ok(path)
}

pub(crate) fn trusted_recovery_target(
    root: &Path,
    rules: &[RatingRule],
    operation_id: &str,
    action: OrganizerAction,
    raw: &str,
) -> Result<PathBuf, String> {
    match action {
        OrganizerAction::Move => trusted_history_target(rules, raw),
        OrganizerAction::Quarantine => trusted_quarantine_target(root, operation_id, raw),
        OrganizerAction::Copy | OrganizerAction::Trash => {
            Err("当前操作类型没有可恢复移动目标".to_string())
        }
    }
}

pub(crate) fn ensure_recovery_parent(root: &Path, destination: &Path) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or_else(|| "无法确定恢复目标目录".to_string())?;
    let relative = parent
        .strip_prefix(root)
        .map_err(|_| "恢复目标目录超出了照片目录".to_string())?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err("恢复目标父目录不是可信文件夹".to_string());
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&current)
                .map_err(|error| format!("无法创建恢复目标目录：{error}"))?,
            Err(error) => return Err(format!("无法检查恢复目标目录：{error}")),
        }
    }
    Ok(())
}

pub(crate) fn copy_verified_noclobber(
    source: &Path,
    destination: &Path,
    expected: &FileFingerprint,
) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or_else(|| "无法确定恢复目标目录".to_string())?;
    let mut input = File::open(source).map_err(|error| format!("无法打开恢复源文件：{error}"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("无法创建恢复临时文件：{error}"))?;
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = input
            .read(&mut buffer)
            .map_err(|error| format!("无法读取恢复源文件：{error}"))?;
        if read == 0 {
            break;
        }
        temporary
            .write_all(&buffer[..read])
            .map_err(|error| format!("无法写入恢复临时文件：{error}"))?;
        hasher.update(&buffer[..read]);
        size = size.saturating_add(read as u64);
    }
    temporary
        .as_file_mut()
        .sync_all()
        .map_err(|error| format!("无法同步恢复临时文件：{error}"))?;
    let digest = format!("{:x}", hasher.finalize());
    if size != expected.size_bytes || digest != expected.sha256 {
        return Err("恢复源文件内容与历史记录不一致".to_string());
    }
    temporary
        .persist_noclobber(destination)
        .map_err(|error| format!("恢复目标在写入前发生变化：{}", error.error))?;
    if fingerprint(destination)?.sha256 != expected.sha256 {
        let _ = fs::remove_file(destination);
        return Err("恢复目标内容复验失败".to_string());
    }
    Ok(())
}

pub(crate) fn restore_member(
    root: &Path,
    rules: &[RatingRule],
    operation_id: &str,
    action: OrganizerAction,
    member: &OperationMemberRecord,
) -> RecoveryMemberResult {
    let source_result = trusted_history_source(root, &member.source_path);
    let target_result =
        trusted_recovery_target(root, rules, operation_id, action, &member.target_path);
    let result = source_result.and_then(|source| {
        let target = target_result?;
        let snapshot = member
            .target_snapshot
            .as_ref()
            .ok_or_else(|| "历史记录缺少目标摘要".to_string())?;
        if source.exists() {
            return Err("原位置已有文件，不会覆盖".to_string());
        }
        if !unchanged(&target, snapshot) {
            return Err("移动目标已变化或不存在，不会自动恢复".to_string());
        }
        ensure_recovery_parent(root, &source)?;
        match fs::rename(&target, &source) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::CrossesDevices => {
                copy_verified_noclobber(&target, &source, snapshot)?;
                fs::remove_file(&target)
                    .map_err(|error| format!("已恢复原文件，但无法移除移动目标：{error}"))
            }
            Err(error) => Err(format!("恢复移动文件失败：{error}")),
        }
    });
    RecoveryMemberResult {
        source_path: member.source_path.clone(),
        target_path: member.target_path.clone(),
        success: result.is_ok(),
        message: result.err().unwrap_or_else(|| "已恢复到原位置".to_string()),
    }
}

pub(crate) fn preflight_restore_member(
    root: &Path,
    rules: &[RatingRule],
    operation_id: &str,
    action: OrganizerAction,
    member: &OperationMemberRecord,
) -> Result<(), String> {
    let source = trusted_history_source(root, &member.source_path)?;
    let target = trusted_recovery_target(root, rules, operation_id, action, &member.target_path)?;
    let snapshot = member
        .target_snapshot
        .as_ref()
        .ok_or_else(|| "历史记录缺少目标摘要".to_string())?;
    if source.exists() {
        return Err(format!("原位置已有文件：{}", member.source_path));
    }
    if !unchanged(&target, snapshot) {
        return Err(format!("移动目标已变化或不存在：{}", member.target_path));
    }
    let parent = source
        .parent()
        .ok_or_else(|| "无法确定恢复目标目录".to_string())?;
    if !parent.starts_with(root) {
        return Err("恢复目标目录超出了照片目录".to_string());
    }
    Ok(())
}

pub(crate) fn undo_copy_member(
    rules: &[RatingRule],
    member: &OperationMemberRecord,
) -> RecoveryMemberResult {
    let result = trusted_history_target(rules, &member.target_path).and_then(|target| {
        let snapshot = member
            .target_snapshot
            .as_ref()
            .ok_or_else(|| "历史记录缺少副本摘要".to_string())?;
        if !unchanged(&target, snapshot) {
            return Err("复制目标已变化或不存在，不会自动撤销".to_string());
        }
        fs::remove_file(&target).map_err(|error| format!("撤销复制失败：{error}"))
    });
    RecoveryMemberResult {
        source_path: member.source_path.clone(),
        target_path: member.target_path.clone(),
        success: result.is_ok(),
        message: result.err().unwrap_or_else(|| "已撤销复制".to_string()),
    }
}

pub(crate) fn recover_operation(
    app_data_dir: &Path,
    operation_id: &str,
    group_ids: &[String],
    created_at_ms: u64,
    kind: RecoveryKind,
) -> Result<OrganizerRecoverySummary, String> {
    let selected = validate_recovery_selection(group_ids)?;
    let history = load_operation(app_data_dir, operation_id)?;
    let root = canonical_directory(Path::new(&history.manifest.root), "评分整理照片目录")?;
    let completed = history
        .recoveries
        .iter()
        .filter(|record| record.status == OrganizerGroupStatus::Success)
        .map(|record| record.group_id.as_str())
        .collect::<HashSet<_>>();
    let mut results = Vec::with_capacity(selected.len());
    for group_id in group_ids {
        let group = history
            .manifest
            .groups
            .iter()
            .find(|group| group.group_id == *group_id)
            .ok_or_else(|| format!("操作历史中不存在照片组：{group_id}"))?;
        let expected_action = match kind {
            RecoveryKind::RestoreMove => OrganizerAction::Move,
            RecoveryKind::UndoCopy => OrganizerAction::Copy,
            RecoveryKind::RestoreQuarantine => OrganizerAction::Quarantine,
        };
        if group.action != expected_action {
            return Err(format!("照片组“{}”的原操作类型不匹配", group.relative_stem));
        }
        if !matches!(
            group.status,
            OrganizerGroupStatus::Success | OrganizerGroupStatus::Partial
        ) {
            return Err(format!("照片组“{}”没有可恢复文件", group.relative_stem));
        }
        if completed.contains(group_id.as_str()) {
            return Err(format!("照片组“{}”已经恢复或撤销", group.relative_stem));
        }
        let recoverable_members = group
            .members
            .iter()
            .filter(|member| member.target_snapshot.is_some())
            .collect::<Vec<_>>();
        let restore_preflight = if matches!(
            kind,
            RecoveryKind::RestoreMove | RecoveryKind::RestoreQuarantine
        ) {
            recoverable_members.iter().find_map(|member| {
                preflight_restore_member(
                    &root,
                    &history.manifest.rules,
                    operation_id,
                    expected_action,
                    member,
                )
                .err()
            })
        } else {
            None
        };
        let members = if let Some(error) = restore_preflight {
            recoverable_members
                .iter()
                .map(|member| RecoveryMemberResult {
                    source_path: member.source_path.clone(),
                    target_path: member.target_path.clone(),
                    success: false,
                    message: format!("照片组恢复预检未通过：{error}"),
                })
                .collect::<Vec<_>>()
        } else {
            recoverable_members
                .iter()
                .map(|member| match kind {
                    RecoveryKind::RestoreMove => restore_member(
                        &root,
                        &history.manifest.rules,
                        operation_id,
                        expected_action,
                        member,
                    ),
                    RecoveryKind::UndoCopy => undo_copy_member(&history.manifest.rules, member),
                    RecoveryKind::RestoreQuarantine => restore_member(
                        &root,
                        &history.manifest.rules,
                        operation_id,
                        expected_action,
                        member,
                    ),
                })
                .collect::<Vec<_>>()
        };
        let status = recovery_status(&members);
        let succeeded = members.iter().filter(|member| member.success).count();
        let record = RecoveryRecord {
            operation_id: operation_id.to_string(),
            group_id: group_id.clone(),
            kind,
            created_at_ms,
            status,
            message: format!("{} / {} 个文件处理完成", succeeded, members.len()),
            members,
        };
        append_recovery(app_data_dir, &record)?;
        results.push(record);
    }
    Ok(OrganizerRecoverySummary {
        operation_id: operation_id.to_string(),
        succeeded: results
            .iter()
            .filter(|record| record.status == OrganizerGroupStatus::Success)
            .count(),
        failed: results
            .iter()
            .filter(|record| record.status == OrganizerGroupStatus::Failed)
            .count(),
        partial: results
            .iter()
            .filter(|record| record.status == OrganizerGroupStatus::Partial)
            .count(),
        results,
    })
}

pub(crate) fn restore_move_operation(
    app_data_dir: &Path,
    operation_id: &str,
    group_ids: &[String],
    created_at_ms: u64,
) -> Result<OrganizerRecoverySummary, String> {
    recover_operation(
        app_data_dir,
        operation_id,
        group_ids,
        created_at_ms,
        RecoveryKind::RestoreMove,
    )
}

pub(crate) fn restore_quarantine_operation(
    app_data_dir: &Path,
    operation_id: &str,
    group_ids: &[String],
    created_at_ms: u64,
) -> Result<OrganizerRecoverySummary, String> {
    recover_operation(
        app_data_dir,
        operation_id,
        group_ids,
        created_at_ms,
        RecoveryKind::RestoreQuarantine,
    )
}

pub(crate) fn undo_copy_operation(
    app_data_dir: &Path,
    operation_id: &str,
    group_ids: &[String],
    created_at_ms: u64,
) -> Result<OrganizerRecoverySummary, String> {
    recover_operation(
        app_data_dir,
        operation_id,
        group_ids,
        created_at_ms,
        RecoveryKind::UndoCopy,
    )
}
