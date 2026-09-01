//! 评分同步：整理文件的同时把 XMP / JPEG 里的评分带到新位置。

use super::*;

pub(crate) fn validate_sync_actions(item: &OperationPlanItem) -> Result<(), String> {
    if item
        .sync_actions
        .iter()
        .any(|action| action.timing != SyncTiming::Destination)
    {
        return Err(
            "评分整理执行仅支持写入复制或移动后的目标文件；请把另一格式加入处理范围".to_string(),
        );
    }
    Ok(())
}

pub(crate) fn validate_cleanup_sync_target(
    root: &Path,
    sync: &PlannedSyncAction,
) -> Result<(PathBuf, bool), String> {
    if sync.timing != SyncTiming::BeforeCleanup {
        return Err("待清理评分同步必须在清理前执行".to_string());
    }
    let target = PathBuf::from(&sync.target_path);
    if !target.is_absolute()
        || target == root
        || !target.starts_with(root)
        || target
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err("待清理评分同步目标超出了照片根目录".to_string());
    }
    let parent = target
        .parent()
        .ok_or_else(|| "无法确定待清理评分同步目录".to_string())?;
    let canonical_parent = canonical_directory(parent, "待清理评分同步目录")?;
    if !canonical_parent.starts_with(root) {
        return Err("待清理评分同步目录解析后超出了照片根目录".to_string());
    }
    match fs::symlink_metadata(&target) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err("待清理评分同步目标不是可信普通文件".to_string())
        }
        Ok(_) => {
            let canonical = fs::canonicalize(&target)
                .map_err(|error| format!("无法校验待清理评分同步目标：{error}"))?;
            if canonical != target {
                return Err("待清理评分同步目标解析后发生变化".to_string());
            }
            Ok((target, true))
        }
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound
                && sync.target == RatingSyncTarget::RawXmp =>
        {
            Ok((target, false))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Err("待清理 JPG 评分同步目标不存在".to_string())
        }
        Err(error) => Err(format!("无法检查待清理评分同步目标：{error}")),
    }
}

pub(crate) fn apply_cleanup_sync(
    root: &Path,
    item: &OperationPlanItem,
    members: &mut Vec<ValidatedMember>,
) -> Result<(), String> {
    let targets = item
        .sync_actions
        .iter()
        .map(|sync| validate_cleanup_sync_target(root, sync))
        .collect::<Result<Vec<_>, _>>()?;
    for (sync, (target, existed)) in item.sync_actions.iter().zip(targets) {
        rating_sync::write_rating_to_validated_path(&target, sync.target, sync.target_rating)?;
        let metadata = fs::symlink_metadata(&target)
            .map_err(|error| format!("无法复核清理前评分同步目标：{error}"))?;
        if let Some(member) = members.iter_mut().find(|member| member.source == target) {
            member.expected_size_bytes = metadata.len();
            member.expected_modified_ms = modified_ms(&metadata);
        } else if !existed && sync.target == RatingSyncTarget::RawXmp {
            members.push(ValidatedMember {
                kind: RuleMemberKind::Xmp,
                source: target,
                target: PathBuf::new(),
                expected_size_bytes: metadata.len(),
                expected_modified_ms: modified_ms(&metadata),
            });
        }
    }
    Ok(())
}

pub(crate) fn raw_sidecar_paths(
    group: &OperationGroupRecord,
    sync: &PlannedSyncAction,
) -> Option<(PathBuf, PathBuf)> {
    let target = PathBuf::from(&sync.target_path);
    group
        .members
        .iter()
        .filter(|member| member.kind == RuleMemberKind::Raw)
        .find_map(|member| {
            let mut expected_target = PathBuf::from(&member.target_path);
            expected_target.set_extension("xmp");
            (expected_target == target).then(|| {
                let mut source = PathBuf::from(&member.source_path);
                source.set_extension("xmp");
                (source, target.clone())
            })
        })
}

pub(crate) fn apply_destination_sync(
    plan: &AuthorizedOperationPlan,
    item: &OperationPlanItem,
    group: &mut OperationGroupRecord,
) -> Result<(), String> {
    if item.sync_actions.is_empty() {
        return Ok(());
    }
    let rule = matching_rule(plan, item)?;
    let destination = canonical_directory(
        Path::new(
            rule.destination
                .as_deref()
                .ok_or_else(|| "评分同步缺少目标目录".to_string())?,
        ),
        "评分整理目标目录",
    )?;
    for sync in &item.sync_actions {
        if sync.timing != SyncTiming::Destination {
            return Err("评分同步目标不是复制或移动后的文件".to_string());
        }
        let target = PathBuf::from(&sync.target_path);
        if !target.is_absolute() || !target.starts_with(&destination) {
            return Err("评分同步目标超出了规则目标目录".to_string());
        }
        if let Some(member) = group
            .members
            .iter_mut()
            .find(|member| Path::new(&member.target_path) == target)
        {
            rating_sync::write_rating_to_validated_path(&target, sync.target, sync.target_rating)?;
            member.target_snapshot = Some(fingerprint(&target)?);
            member.message = format!("{}；评分已同步为 {} 星", member.message, sync.target_rating);
            continue;
        }
        if sync.target != RatingSyncTarget::RawXmp {
            return Err("JPG 评分同步目标不在已提交文件中".to_string());
        }
        let (source, target) = raw_sidecar_paths(group, sync)
            .ok_or_else(|| "RAW XMP 同步目标无法对应已提交 RAW".to_string())?;
        match fs::symlink_metadata(&target) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("无法检查 RAW XMP 同步目标：{error}")),
            Ok(_) => return Err("RAW XMP 同步目标已存在但不在执行清单中".to_string()),
        }
        rating_sync::write_rating_to_validated_path(&target, sync.target, sync.target_rating)?;
        let snapshot = fingerprint(&target)?;
        group.members.push(OperationMemberRecord {
            kind: RuleMemberKind::Xmp,
            source_path: display_path(&source),
            target_path: display_path(&target),
            expected_size_bytes: 0,
            expected_modified_ms: None,
            target_snapshot: Some(snapshot),
            message: format!("已在目标位置创建 {} 星 RAW XMP", sync.target_rating),
        });
    }
    Ok(())
}
