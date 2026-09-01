//! 组内执行失败时的回滚。
//!
//! 回滚只在能确认「这确实是我们刚写下去的那个文件」时才动手（比对指纹），
//! 否则宁可留下并上报，也不能误删用户的原始数据。

use super::*;

pub(crate) fn rollback_copies(records: &[OperationMemberRecord]) -> Vec<String> {
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

pub(crate) fn rollback_moves_without_history(
    root: &Path,
    rules: &[RatingRule],
    records: &[OperationMemberRecord],
) -> Vec<String> {
    let mut failures = Vec::new();
    for member in records.iter().rev() {
        let source = match trusted_history_source(root, &member.source_path) {
            Ok(source) => source,
            Err(error) => {
                failures.push(error);
                continue;
            }
        };
        let target = match trusted_history_target(rules, &member.target_path) {
            Ok(target) => target,
            Err(error) => {
                failures.push(error);
                continue;
            }
        };
        let Some(snapshot) = &member.target_snapshot else {
            continue;
        };
        if source.exists() {
            if unchanged(&target, snapshot) {
                if let Err(error) = fs::remove_file(&target) {
                    failures.push(format!("{}：{error}", member.target_path));
                }
            } else if target.exists() {
                failures.push(format!("{}：目标已变化", member.target_path));
            }
            continue;
        }
        let result = restore_member(root, rules, "", OrganizerAction::Move, member);
        if !result.success {
            failures.push(format!("{}：{}", result.target_path, result.message));
        }
    }
    failures
}

pub(crate) fn source_matches_record(record: &OperationMemberRecord) -> bool {
    let path = Path::new(&record.source_path);
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    !metadata.file_type().is_symlink()
        && metadata.is_file()
        && metadata.len() == record.expected_size_bytes
        && modified_ms(&metadata) == record.expected_modified_ms
}

pub(crate) fn rollback_renamed_moves(records: &[OperationMemberRecord]) -> Vec<String> {
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
