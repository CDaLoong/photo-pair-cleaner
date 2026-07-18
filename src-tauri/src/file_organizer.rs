use crate::operation_history::{
    FileFingerprint, OperationGroupRecord, OperationManifest, OperationMemberRecord,
    OrganizerAction, OrganizerGroupStatus, persist_manifest,
};
use crate::operation_plan::{AuthorizedOperationPlan, OperationPlanItem, PlannedMember};
use crate::rating_rules::{RatingRule, RuleAction, RuleMemberKind};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OrganizerExecutionSummary {
    pub(crate) operation_id: String,
    pub(crate) plan_id: String,
    pub(crate) succeeded: usize,
    pub(crate) failed: usize,
    pub(crate) partial: usize,
    pub(crate) skipped: usize,
    pub(crate) groups: Vec<OperationGroupRecord>,
}

struct ValidatedMember {
    kind: RuleMemberKind,
    source: PathBuf,
    target: PathBuf,
    expected_size_bytes: u64,
    expected_modified_ms: Option<u64>,
}

struct StagedCopy {
    member: ValidatedMember,
    temporary: tempfile::NamedTempFile,
    sha256: String,
}

#[derive(Debug, Clone, Copy, Default)]
struct ExecutionOptions {
    force_copy_delete: bool,
    fail_rename_at: Option<usize>,
    fail_delete_at: Option<usize>,
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn modified_ms(metadata: &fs::Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
}

fn safe_relative_path(value: &str) -> Result<&Path, String> {
    let path = Path::new(value);
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("照片成员路径不是安全相对路径".to_string());
    }
    Ok(path)
}

fn canonical_directory(path: &Path, label: &str) -> Result<PathBuf, String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| format!("{label}不可访问：{error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!("{label}不是可信文件夹"));
    }
    fs::canonicalize(path).map_err(|error| format!("{label}不可访问：{error}"))
}

fn matching_rule<'a>(
    plan: &'a AuthorizedOperationPlan,
    item: &OperationPlanItem,
) -> Result<&'a RatingRule, String> {
    let action = item
        .terminal_action
        .ok_or_else(|| "照片组没有最终操作".to_string())?;
    let mut rules = plan.rules.iter().filter(|rule| {
        item.matched_rule_ids.iter().any(|id| id == &rule.id) && rule.action == action
    });
    let rule = rules
        .next()
        .ok_or_else(|| "照片组的命中规则已丢失".to_string())?;
    if rules.next().is_some() {
        return Err("照片组命中了多条执行规则".to_string());
    }
    Ok(rule)
}

fn validate_existing_target_ancestry(destination: &Path, target: &Path) -> Result<(), String> {
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

fn validate_member(
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

fn validate_group(
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

fn ensure_target_parent(destination: &Path, target: &Path) -> Result<(), String> {
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
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&current)
                .map_err(|error| format!("无法创建目标目录 {}：{error}", display_path(&current)))?,
            Err(error) => return Err(format!("无法检查目标目录：{error}")),
        }
    }
    let canonical_parent =
        fs::canonicalize(parent).map_err(|error| format!("无法校验目标父目录：{error}"))?;
    if !canonical_parent.starts_with(destination) {
        return Err("目标父目录解析后超出了规则目标目录".to_string());
    }
    Ok(())
}

fn stream_copy_to_temporary(
    member: ValidatedMember,
    destination: &Path,
) -> Result<StagedCopy, String> {
    ensure_target_parent(destination, &member.target)?;
    let parent = member
        .target
        .parent()
        .ok_or_else(|| "无法确定目标父目录".to_string())?;
    let mut source = File::open(&member.source)
        .map_err(|error| format!("无法打开源文件 {}：{error}", display_path(&member.source)))?;
    let opened_metadata = source
        .metadata()
        .map_err(|error| format!("无法读取源文件信息：{error}"))?;
    if opened_metadata.len() != member.expected_size_bytes
        || modified_ms(&opened_metadata) != member.expected_modified_ms
    {
        return Err("源文件在复制前发生变化".to_string());
    }
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("无法创建目标临时文件：{error}"))?;
    let mut hasher = Sha256::new();
    let mut copied = 0_u64;
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = source
            .read(&mut buffer)
            .map_err(|error| format!("无法读取源文件：{error}"))?;
        if read == 0 {
            break;
        }
        temporary
            .write_all(&buffer[..read])
            .map_err(|error| format!("无法写入目标临时文件：{error}"))?;
        hasher.update(&buffer[..read]);
        copied = copied.saturating_add(read as u64);
    }
    temporary
        .as_file_mut()
        .sync_all()
        .map_err(|error| format!("无法同步目标临时文件：{error}"))?;
    let final_source_metadata = source
        .metadata()
        .map_err(|error| format!("无法复核源文件信息：{error}"))?;
    if copied != member.expected_size_bytes
        || final_source_metadata.len() != member.expected_size_bytes
        || modified_ms(&final_source_metadata) != member.expected_modified_ms
    {
        return Err("源文件在复制过程中发生变化".to_string());
    }
    let sha256 = format!("{:x}", hasher.finalize());
    if fingerprint(temporary.path())?.sha256 != sha256 {
        return Err("目标临时文件内容校验失败".to_string());
    }
    Ok(StagedCopy {
        member,
        temporary,
        sha256,
    })
}

fn fingerprint(path: &Path) -> Result<FileFingerprint, String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| format!("无法读取文件校验信息：{error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("校验目标不是可信普通文件".to_string());
    }
    let mut file = File::open(path).map_err(|error| format!("无法打开校验文件：{error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("无法读取校验文件：{error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(FileFingerprint {
        size_bytes: metadata.len(),
        modified_ms: modified_ms(&metadata),
        sha256: format!("{:x}", hasher.finalize()),
    })
}

fn unchanged(path: &Path, expected: &FileFingerprint) -> bool {
    fingerprint(path).is_ok_and(|actual| actual == *expected)
}

fn rollback_copies(records: &[OperationMemberRecord]) -> Vec<String> {
    let mut failures = Vec::new();
    for record in records.iter().rev() {
        let Some(snapshot) = &record.target_snapshot else {
            continue;
        };
        let target = Path::new(&record.target_path);
        if unchanged(target, snapshot) {
            if let Err(error) = fs::remove_file(target) {
                failures.push(format!("{}：{error}", record.target_path));
            }
        } else if target.exists() {
            failures.push(format!("{}：副本已变化，未自动移除", record.target_path));
        }
    }
    failures
}

fn source_matches_record(record: &OperationMemberRecord) -> bool {
    let path = Path::new(&record.source_path);
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    !metadata.file_type().is_symlink()
        && metadata.is_file()
        && metadata.len() == record.expected_size_bytes
        && modified_ms(&metadata) == record.expected_modified_ms
}

fn rollback_renamed_moves(records: &[OperationMemberRecord]) -> Vec<String> {
    let mut failures = Vec::new();
    for record in records.iter().rev() {
        let target = Path::new(&record.target_path);
        let source = Path::new(&record.source_path);
        let Some(snapshot) = &record.target_snapshot else {
            continue;
        };
        if source.exists() {
            failures.push(format!("{}：原位置已被占用", record.source_path));
        } else if !unchanged(target, snapshot) {
            failures.push(format!("{}：目标已变化", record.target_path));
        } else if let Err(error) = fs::rename(target, source) {
            failures.push(format!("{}：{error}", record.target_path));
        }
    }
    failures
}

#[cfg(unix)]
fn paths_share_device(source: &Path, target_parent: &Path) -> Result<bool, String> {
    use std::os::unix::fs::MetadataExt;

    let source_metadata =
        fs::metadata(source).map_err(|error| format!("无法读取移动源设备信息：{error}"))?;
    let target_metadata = fs::metadata(target_parent)
        .map_err(|error| format!("无法读取移动目标设备信息：{error}"))?;
    Ok(source_metadata.dev() == target_metadata.dev())
}

#[cfg(not(unix))]
fn paths_share_device(_source: &Path, _target_parent: &Path) -> Result<bool, String> {
    Ok(false)
}

fn record_for_committed_member(
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

fn move_result_group(
    item: &OperationPlanItem,
    status: OrganizerGroupStatus,
    message: String,
    members: Vec<OperationMemberRecord>,
) -> OperationGroupRecord {
    OperationGroupRecord {
        group_id: item.group_id.clone(),
        relative_stem: item.relative_stem.clone(),
        action: OrganizerAction::Move,
        status,
        message,
        members,
    }
}

fn execute_rename_move(
    item: &OperationPlanItem,
    validated: Vec<ValidatedMember>,
    options: ExecutionOptions,
) -> OperationGroupRecord {
    let mut records = Vec::with_capacity(validated.len());
    for (index, member) in validated.into_iter().enumerate() {
        if options.fail_rename_at == Some(index) {
            let rollback_failures = rollback_renamed_moves(&records);
            return if rollback_failures.is_empty() {
                failed_group(
                    item,
                    OrganizerAction::Move,
                    "移动重命名失败，已完整回滚".to_string(),
                )
            } else {
                move_result_group(
                    item,
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
                failed_group(item, OrganizerAction::Move, error)
            } else {
                move_result_group(
                    item,
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
                    failed_group(
                        item,
                        OrganizerAction::Move,
                        format!("移动源文件不可访问：{error}"),
                    )
                } else {
                    move_result_group(
                        item,
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
                failed_group(
                    item,
                    OrganizerAction::Move,
                    "移动源文件发生变化，已回滚".to_string(),
                )
            } else {
                move_result_group(
                    item,
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
                failed_group(
                    item,
                    OrganizerAction::Move,
                    format!("移动文件失败：{error}；已回滚"),
                )
            } else {
                move_result_group(
                    item,
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
                    failed_group(item, OrganizerAction::Move, error)
                } else {
                    move_result_group(
                        item,
                        OrganizerGroupStatus::Partial,
                        format!("{error}；回滚不完整：{}", rollback_failures.join("；")),
                        records,
                    )
                };
            }
        }
    }
    move_result_group(
        item,
        OrganizerGroupStatus::Success,
        format!("已原子移动 {} 个文件", records.len()),
        records,
    )
}

fn commit_staged_move(
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

fn execute_copy_delete_move(
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

fn execute_move_group(
    root: &Path,
    plan: &AuthorizedOperationPlan,
    item: &OperationPlanItem,
    options: ExecutionOptions,
) -> OperationGroupRecord {
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
        execute_rename_move(item, validated, options)
    } else {
        execute_copy_delete_move(item, &destination, validated, options)
    }
}

fn failed_group(
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

fn execute_copy_group(
    root: &Path,
    plan: &AuthorizedOperationPlan,
    item: &OperationPlanItem,
) -> OperationGroupRecord {
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

fn execute_authorized_plan_with_options(
    app_data_dir: &Path,
    operation_id: String,
    created_at_ms: u64,
    plan: AuthorizedOperationPlan,
    options: ExecutionOptions,
) -> Result<OrganizerExecutionSummary, String> {
    let root = canonical_directory(Path::new(&plan.summary.root), "评分整理照片目录")?;
    let mut groups = Vec::with_capacity(plan.items.len());
    for item in &plan.items {
        groups.push(match item.terminal_action {
            Some(RuleAction::Copy) => execute_copy_group(&root, &plan, item),
            Some(RuleAction::Move) => execute_move_group(&root, &plan, item, options),
            _ => failed_group(
                item,
                OrganizerAction::Copy,
                "当前仅允许执行复制或移动".to_string(),
            ),
        });
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
                group.action == OrganizerAction::Copy
                    && group.status == OrganizerGroupStatus::Success
            })
            .flat_map(|group| rollback_copies(&group.members))
            .collect::<Vec<_>>();
        return Err(if rollback_failures.is_empty() {
            error
        } else {
            format!(
                "{error}；已复制文件回滚失败：{}",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operation_history::OrganizerGroupStatus;
    use crate::operation_plan::{
        AuthorizedOperationPlan, OperationPlanItem, OperationPlanStatus, OperationPlanSummary,
        OperationSyncPreference, PlannedMember,
    };
    use crate::rating_rules::{RatingCondition, RatingRule, RuleAction, RuleMemberKind};
    use std::fs;
    use std::path::Path;
    use std::time::UNIX_EPOCH;
    use tempfile::tempdir;

    fn modified_ms(path: &Path) -> Option<u64> {
        fs::metadata(path)
            .ok()?
            .modified()
            .ok()?
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
    }

    fn copy_rule(destination: &Path) -> RatingRule {
        RatingRule {
            id: "copy-rule".to_string(),
            name: "复制三星".to_string(),
            enabled: true,
            condition: RatingCondition::Equal { rating: 3 },
            member_scope: vec![RuleMemberKind::Jpeg],
            action: RuleAction::Copy,
            destination: Some(destination.to_string_lossy().into_owned()),
            preserve_relative_path: true,
        }
    }

    fn move_rule(destination: &Path) -> RatingRule {
        RatingRule {
            id: "move-rule".to_string(),
            name: "移动三星".to_string(),
            enabled: true,
            condition: RatingCondition::Equal { rating: 3 },
            member_scope: vec![RuleMemberKind::Jpeg],
            action: RuleAction::Move,
            destination: Some(destination.to_string_lossy().into_owned()),
            preserve_relative_path: true,
        }
    }

    fn copy_item(
        root: &Path,
        destination: &Path,
        group_id: &str,
        relative_path: &str,
    ) -> OperationPlanItem {
        let source = root.join(relative_path);
        let metadata = fs::metadata(&source).expect("source metadata");
        OperationPlanItem {
            group_id: group_id.to_string(),
            relative_stem: relative_path.trim_end_matches(".jpg").to_string(),
            rating: Some(3),
            frame_pair: 3,
            jpeg_metadata: None,
            raw_xmp: None,
            matched_rule_ids: vec!["copy-rule".to_string()],
            matched_rule_names: vec!["复制三星".to_string()],
            terminal_action: Some(RuleAction::Copy),
            status: OperationPlanStatus::Ready,
            members: vec![PlannedMember {
                kind: RuleMemberKind::Jpeg,
                source_relative_path: relative_path.to_string(),
                target_path: Some(
                    destination
                        .join(relative_path)
                        .to_string_lossy()
                        .into_owned(),
                ),
                size_bytes: metadata.len(),
                modified_ms: modified_ms(&source),
            }],
            missing_kinds: Vec::new(),
            sync_actions: Vec::new(),
            issues: Vec::new(),
        }
    }

    fn plan(
        root: &Path,
        destination: &Path,
        items: Vec<OperationPlanItem>,
    ) -> AuthorizedOperationPlan {
        AuthorizedOperationPlan {
            summary: OperationPlanSummary {
                plan_id: "plan-1".to_string(),
                root: root.to_string_lossy().into_owned(),
                total_items: items.len(),
                ready: items.len(),
                kept: 0,
                skipped: 0,
                conflicts: 0,
                move_groups: 0,
                copy_groups: items.len(),
                cleanup_groups: 0,
                sync_groups: 0,
                jpeg_files: items.len(),
                raw_files: 0,
                xmp_files: 0,
                copy_bytes: items
                    .iter()
                    .flat_map(|item| &item.members)
                    .map(|member| member.size_bytes)
                    .sum(),
                cleanup_bytes: 0,
                items: items.clone(),
            },
            items,
            rules: vec![copy_rule(destination)],
            sync: OperationSyncPreference::default(),
        }
    }

    fn move_item(
        root: &Path,
        destination: &Path,
        group_id: &str,
        relative_path: &str,
    ) -> OperationPlanItem {
        let mut item = copy_item(root, destination, group_id, relative_path);
        item.matched_rule_ids = vec!["move-rule".to_string()];
        item.matched_rule_names = vec!["移动三星".to_string()];
        item.terminal_action = Some(RuleAction::Move);
        item
    }

    fn move_plan(
        root: &Path,
        destination: &Path,
        items: Vec<OperationPlanItem>,
    ) -> AuthorizedOperationPlan {
        let mut plan = plan(root, destination, items);
        plan.summary.move_groups = plan.summary.copy_groups;
        plan.summary.copy_groups = 0;
        plan.summary.copy_bytes = 0;
        plan.rules = vec![move_rule(destination)];
        plan
    }

    #[test]
    fn copy_execution_verifies_content_and_persists_history() {
        let source = tempdir().expect("source");
        let target = tempdir().expect("target");
        let app_data = tempdir().expect("app data");
        fs::create_dir_all(source.path().join("album")).expect("source album");
        fs::write(source.path().join("album/photo.jpg"), b"framepair").expect("source file");
        let root = fs::canonicalize(source.path()).expect("source root");
        let destination = fs::canonicalize(target.path()).expect("target root");
        let item = copy_item(&root, &destination, "group-1", "album/photo.jpg");

        let summary = execute_authorized_plan(
            app_data.path(),
            "operation-1".to_string(),
            100,
            plan(&root, &destination, vec![item]),
        )
        .expect("execute copy");

        assert_eq!(summary.succeeded, 1);
        assert_eq!(summary.failed, 0);
        assert_eq!(
            fs::read(destination.join("album/photo.jpg")).unwrap(),
            b"framepair"
        );
        let history = crate::operation_history::list_operations(app_data.path()).unwrap();
        assert_eq!(
            history[0].manifest.groups[0].status,
            OrganizerGroupStatus::Success
        );
        assert_eq!(
            history[0].manifest.groups[0].members[0]
                .target_snapshot
                .as_ref()
                .unwrap()
                .sha256
                .len(),
            64
        );
    }

    #[test]
    fn copy_execution_isolates_a_drifted_group() {
        let source = tempdir().expect("source");
        let target = tempdir().expect("target");
        let app_data = tempdir().expect("app data");
        fs::write(source.path().join("changed.jpg"), b"before").expect("changed source");
        fs::write(source.path().join("stable.jpg"), b"stable").expect("stable source");
        let root = fs::canonicalize(source.path()).expect("source root");
        let destination = fs::canonicalize(target.path()).expect("target root");
        let changed = copy_item(&root, &destination, "changed", "changed.jpg");
        let stable = copy_item(&root, &destination, "stable", "stable.jpg");
        fs::write(root.join("changed.jpg"), b"changed after plan").expect("drift source");

        let summary = execute_authorized_plan(
            app_data.path(),
            "operation-1".to_string(),
            100,
            plan(&root, &destination, vec![changed, stable]),
        )
        .expect("execute copy groups");

        assert_eq!(summary.succeeded, 1);
        assert_eq!(summary.failed, 1);
        assert!(!destination.join("changed.jpg").exists());
        assert_eq!(fs::read(destination.join("stable.jpg")).unwrap(), b"stable");
    }

    #[cfg(unix)]
    #[test]
    fn copy_execution_rejects_source_symlinks_and_existing_targets() {
        use std::os::unix::fs::symlink;

        let source = tempdir().expect("source");
        let target = tempdir().expect("target");
        let app_data = tempdir().expect("app data");
        fs::write(source.path().join("real.jpg"), b"real").expect("real source");
        let root = fs::canonicalize(source.path()).expect("source root");
        let destination = fs::canonicalize(target.path()).expect("target root");
        let mut symlinked = copy_item(&root, &destination, "symlink", "real.jpg");
        symlink(root.join("real.jpg"), root.join("linked.jpg")).expect("source symlink");
        symlinked.members[0].source_relative_path = "linked.jpg".to_string();
        symlinked.members[0].target_path = Some(
            destination
                .join("linked.jpg")
                .to_string_lossy()
                .into_owned(),
        );
        let existing = copy_item(&root, &destination, "existing", "real.jpg");
        fs::write(destination.join("real.jpg"), b"do not replace").expect("existing target");

        let summary = execute_authorized_plan(
            app_data.path(),
            "operation-1".to_string(),
            100,
            plan(&root, &destination, vec![symlinked, existing]),
        )
        .expect("execute rejected groups");

        assert_eq!(summary.succeeded, 0);
        assert_eq!(summary.failed, 2);
        assert_eq!(
            fs::read(destination.join("real.jpg")).unwrap(),
            b"do not replace"
        );
        assert!(!destination.join("linked.jpg").exists());
    }

    #[test]
    fn move_execution_renames_same_volume_group() {
        let source = tempdir().expect("source");
        let target = tempdir().expect("target");
        let app_data = tempdir().expect("app data");
        fs::write(source.path().join("photo.jpg"), b"move me").expect("source file");
        let root = fs::canonicalize(source.path()).expect("source root");
        let destination = fs::canonicalize(target.path()).expect("target root");
        let item = move_item(&root, &destination, "move", "photo.jpg");

        let summary = execute_authorized_plan(
            app_data.path(),
            "operation-1".to_string(),
            100,
            move_plan(&root, &destination, vec![item]),
        )
        .expect("execute move");

        assert_eq!(summary.succeeded, 1);
        assert!(!root.join("photo.jpg").exists());
        assert_eq!(fs::read(destination.join("photo.jpg")).unwrap(), b"move me");
    }

    #[test]
    fn move_execution_rolls_back_prior_renames_when_group_fails() {
        let source = tempdir().expect("source");
        let target = tempdir().expect("target");
        let app_data = tempdir().expect("app data");
        fs::write(source.path().join("one.jpg"), b"one").expect("first source");
        fs::write(source.path().join("two.jpg"), b"two").expect("second source");
        let root = fs::canonicalize(source.path()).expect("source root");
        let destination = fs::canonicalize(target.path()).expect("target root");
        let mut item = move_item(&root, &destination, "pair", "one.jpg");
        item.members
            .extend(move_item(&root, &destination, "pair", "two.jpg").members);

        let summary = execute_authorized_plan_with_options(
            app_data.path(),
            "operation-1".to_string(),
            100,
            move_plan(&root, &destination, vec![item]),
            ExecutionOptions {
                fail_rename_at: Some(1),
                ..ExecutionOptions::default()
            },
        )
        .expect("execute failed move");

        assert_eq!(summary.failed, 1);
        assert_eq!(fs::read(root.join("one.jpg")).unwrap(), b"one");
        assert_eq!(fs::read(root.join("two.jpg")).unwrap(), b"two");
        assert!(!destination.join("one.jpg").exists());
        assert!(!destination.join("two.jpg").exists());
    }

    #[test]
    fn cross_volume_move_commits_all_targets_before_deleting_sources() {
        let source = tempdir().expect("source");
        let target = tempdir().expect("target");
        let app_data = tempdir().expect("app data");
        fs::write(source.path().join("one.jpg"), b"one").expect("first source");
        fs::write(source.path().join("two.jpg"), b"two").expect("second source");
        let root = fs::canonicalize(source.path()).expect("source root");
        let destination = fs::canonicalize(target.path()).expect("target root");
        let mut item = move_item(&root, &destination, "pair", "one.jpg");
        item.members
            .extend(move_item(&root, &destination, "pair", "two.jpg").members);

        let summary = execute_authorized_plan_with_options(
            app_data.path(),
            "operation-1".to_string(),
            100,
            move_plan(&root, &destination, vec![item]),
            ExecutionOptions {
                force_copy_delete: true,
                ..ExecutionOptions::default()
            },
        )
        .expect("execute copy delete move");

        assert_eq!(summary.succeeded, 1);
        assert!(!root.join("one.jpg").exists());
        assert!(!root.join("two.jpg").exists());
        assert_eq!(fs::read(destination.join("one.jpg")).unwrap(), b"one");
        assert_eq!(fs::read(destination.join("two.jpg")).unwrap(), b"two");
    }

    #[test]
    fn cross_volume_source_delete_failure_is_recorded_as_partial() {
        let source = tempdir().expect("source");
        let target = tempdir().expect("target");
        let app_data = tempdir().expect("app data");
        fs::write(source.path().join("one.jpg"), b"one").expect("first source");
        fs::write(source.path().join("two.jpg"), b"two").expect("second source");
        let root = fs::canonicalize(source.path()).expect("source root");
        let destination = fs::canonicalize(target.path()).expect("target root");
        let mut item = move_item(&root, &destination, "pair", "one.jpg");
        item.members
            .extend(move_item(&root, &destination, "pair", "two.jpg").members);

        let summary = execute_authorized_plan_with_options(
            app_data.path(),
            "operation-1".to_string(),
            100,
            move_plan(&root, &destination, vec![item]),
            ExecutionOptions {
                force_copy_delete: true,
                fail_delete_at: Some(1),
                ..ExecutionOptions::default()
            },
        )
        .expect("execute partial move");

        assert_eq!(summary.partial, 1);
        assert!(!root.join("one.jpg").exists());
        assert!(root.join("two.jpg").exists());
        assert!(destination.join("one.jpg").exists());
        assert!(destination.join("two.jpg").exists());
    }
}
