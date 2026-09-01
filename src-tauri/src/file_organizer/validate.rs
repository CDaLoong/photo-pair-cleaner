//! 执行前的校验：确认计划里的每个成员在磁盘上仍是当初被授权的那个文件，
//! 且目标路径不会踩到已存在的内容或逃出授权目录。
//!
//! 这里失败意味着整组放弃，不做部分执行——宁可什么都不动，也不留半吊子状态。

use super::*;

pub(crate) fn validate_existing_target_ancestry(
    destination: &Path,
    target: &Path,
) -> Result<(), String> {
    if !target.is_absolute() || !target.starts_with(destination) || target == destination {
        return Err("目标路径超出了规则目标目录".to_string());
    }
    match fs::symlink_metadata(target) {
        Ok(_) => return Err(format!("目标路径已存在：{}", display_path(target))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("无法检查目标路径：{error}")),
    }
    let parent = target
        .parent()
        .ok_or_else(|| "无法确定目标父目录".to_string())?;
    let relative = parent
        .strip_prefix(destination)
        .map_err(|_| "目标父目录超出了规则目标目录".to_string())?;
    let mut current = destination.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(format!(
                    "目标父目录不是可信文件夹：{}",
                    display_path(&current)
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(format!("无法检查目标父目录：{error}")),
        }
    }
    Ok(())
}

pub(crate) fn validate_member(
    root: &Path,
    destination: &Path,
    member: &PlannedMember,
) -> Result<ValidatedMember, String> {
    let relative = safe_relative_path(&member.source_relative_path)?;
    let source = root.join(relative);
    let metadata = fs::symlink_metadata(&source)
        .map_err(|error| format!("源文件不可访问 {}：{error}", display_path(&source)))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!("源路径不是可信普通文件：{}", display_path(&source)));
    }
    let canonical_source = fs::canonicalize(&source)
        .map_err(|error| format!("无法校验源文件 {}：{error}", display_path(&source)))?;
    if canonical_source != source || !canonical_source.starts_with(root) {
        return Err("源文件解析后超出了照片根目录".to_string());
    }
    if metadata.len() != member.size_bytes || modified_ms(&metadata) != member.modified_ms {
        return Err(format!(
            "源文件在生成计划后发生变化：{}",
            member.source_relative_path
        ));
    }
    let target = PathBuf::from(
        member
            .target_path
            .as_deref()
            .ok_or_else(|| "复制或移动成员缺少目标路径".to_string())?,
    );
    validate_existing_target_ancestry(destination, &target)?;
    Ok(ValidatedMember {
        kind: member.kind,
        source,
        target,
        expected_size_bytes: member.size_bytes,
        expected_modified_ms: member.modified_ms,
    })
}

pub(crate) fn validate_group(
    root: &Path,
    plan: &AuthorizedOperationPlan,
    item: &OperationPlanItem,
) -> Result<Vec<ValidatedMember>, String> {
    let rule = matching_rule(plan, item)?;
    let destination = rule
        .destination
        .as_deref()
        .ok_or_else(|| "执行规则缺少目标目录".to_string())?;
    let destination = canonical_directory(Path::new(destination), "评分整理目标目录")?;
    let mut members = Vec::with_capacity(item.members.len());
    for member in &item.members {
        members.push(validate_member(root, &destination, member)?);
    }
    if members.is_empty() {
        return Err("照片组没有可执行文件".to_string());
    }
    Ok(members)
}

pub(crate) fn validate_cleanup_sources(
    root: &Path,
    plan: &AuthorizedOperationPlan,
    item: &OperationPlanItem,
) -> Result<Vec<ValidatedMember>, String> {
    let rule = matching_rule(plan, item)?;
    if rule.action != RuleAction::Cleanup || rule.destination.is_some() {
        return Err("待清理照片组的规则或目标目录无效".to_string());
    }
    let mut members = Vec::with_capacity(item.members.len());
    for member in &item.members {
        let relative = safe_relative_path(&member.source_relative_path)?;
        let source = root.join(relative);
        let metadata = fs::symlink_metadata(&source)
            .map_err(|error| format!("待清理文件不可访问 {}：{error}", display_path(&source)))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(format!(
                "待清理路径不是可信普通文件：{}",
                display_path(&source)
            ));
        }
        let canonical_source = fs::canonicalize(&source)
            .map_err(|error| format!("无法校验待清理文件 {}：{error}", display_path(&source)))?;
        if canonical_source != source || !canonical_source.starts_with(root) {
            return Err("待清理文件解析后超出了照片根目录".to_string());
        }
        if metadata.len() != member.size_bytes || modified_ms(&metadata) != member.modified_ms {
            return Err(format!(
                "待清理文件在生成计划后发生变化：{}",
                member.source_relative_path
            ));
        }
        members.push(ValidatedMember {
            kind: member.kind,
            source,
            target: PathBuf::new(),
            expected_size_bytes: member.size_bytes,
            expected_modified_ms: member.modified_ms,
        });
    }
    if members.is_empty() {
        return Err("待清理照片组没有可执行文件".to_string());
    }
    Ok(members)
}
