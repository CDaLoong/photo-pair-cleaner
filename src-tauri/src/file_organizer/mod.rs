//! 评分整理的文件操作引擎：把已授权的整理计划落到磁盘，并支持撤销。
//!
//! 对外只暴露四个入口——执行计划，以及移动/隔离/复制三条撤销路径。
//! 内部按阶段分层：validate（能不能做）→ staging（怎么安全落盘）→
//! execute（分动作执行）→ rollback（组内失败怎么退回）→ recover（事后撤销）。
//!
//! 贯穿全局的不变量：
//! 1. 每个照片组要么整组成功，要么整组回到原状，不留中间态；
//! 2. 所有写入先落临时文件再原子改名；
//! 3. 删除一律可恢复（隔离区或回收站），永不直接 unlink 用户原件；
//! 4. 路径只能来自已授权计划，不接受调用方传入的任意路径。

use crate::fs_util::{self, display_path, modified_ms};
use crate::operation_history::{
    FileFingerprint, OperationGroupRecord, OperationManifest, OperationMemberRecord,
    OrganizerAction, OrganizerGroupStatus, RecoveryKind, RecoveryMemberResult, RecoveryRecord,
    append_recovery, load_operation, persist_manifest,
};
use crate::operation_plan::{
    AuthorizedOperationPlan, CleanupExecutionDestination, OperationPlanItem, PlannedMember,
    PlannedSyncAction, SyncTiming,
};
use crate::rating_rules::{RatingRule, RuleAction, RuleMemberKind};
use crate::rating_sync::{self, RatingSyncTarget};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OrganizerRecoverySummary {
    pub(crate) operation_id: String,
    pub(crate) succeeded: usize,
    pub(crate) failed: usize,
    pub(crate) partial: usize,
    pub(crate) results: Vec<RecoveryRecord>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OrganizerRecoveryRequest {
    pub(crate) operation_id: String,
    pub(crate) group_ids: Vec<String>,
}

pub(crate) struct ValidatedMember {
    kind: RuleMemberKind,
    source: PathBuf,
    target: PathBuf,
    expected_size_bytes: u64,
    expected_modified_ms: Option<u64>,
}

pub(crate) struct StagedCopy {
    member: ValidatedMember,
    temporary: tempfile::NamedTempFile,
    sha256: String,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ExecutionOptions {
    force_copy_delete: bool,
    fail_rename_at: Option<usize>,
    fail_delete_at: Option<usize>,
    simulate_trash: bool,
    fail_trash_at: Option<usize>,
}

pub(crate) fn safe_relative_path(value: &str) -> Result<&Path, String> {
    fs_util::safe_relative_path(Path::new(value), "照片成员路径")
}

pub(crate) fn canonical_directory(path: &Path, label: &str) -> Result<PathBuf, String> {
    fs_util::canonical_trusted_directory(path, label)
}

pub(crate) fn matching_rule<'a>(
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

mod execute;
mod recover;
mod rollback;
mod staging;
mod sync;
#[cfg(test)]
mod tests;
mod validate;

pub(crate) use execute::*;
pub(crate) use recover::*;
pub(crate) use rollback::*;
pub(crate) use staging::*;
pub(crate) use sync::*;
pub(crate) use validate::*;
