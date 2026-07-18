use crate::photo_groups::{PhotoAsset, PhotoIndex, RatingState};
use crate::rating_metadata;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::UNIX_EPOCH;
use tempfile::NamedTempFile;

const MAX_XMP_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum RatingConflictPolicy {
    Skip,
    FramePair,
    External,
    Highest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RatingResolution {
    Ready(u8),
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RatingSyncTargets {
    pub(crate) raw_xmp: bool,
    pub(crate) jpeg_metadata: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum RatingSyncTarget {
    RawXmp,
    JpegMetadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum RatingSyncStatus {
    Ready,
    Unchanged,
    Conflict,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RatingSyncPlanRequest {
    pub(crate) root: String,
    pub(crate) minimum_rating: u8,
    pub(crate) maximum_rating: u8,
    #[serde(default)]
    pub(crate) asset_ids: Vec<String>,
    pub(crate) targets: RatingSyncTargets,
    pub(crate) conflict_policy: RatingConflictPolicy,
    #[serde(default)]
    pub(crate) jpeg_write_confirmed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RatingSyncWrite {
    pub(crate) target: RatingSyncTarget,
    pub(crate) relative_path: String,
    pub(crate) current_rating: Option<i8>,
    pub(crate) target_rating: u8,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RatingSyncPlanItem {
    pub(crate) asset_id: String,
    pub(crate) relative_stem: String,
    pub(crate) frame_pair: u8,
    pub(crate) jpeg_metadata: Option<i8>,
    pub(crate) raw_xmp: Option<i8>,
    pub(crate) resolved: Option<u8>,
    pub(crate) status: RatingSyncStatus,
    pub(crate) writes: Vec<RatingSyncWrite>,
    pub(crate) issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RatingSyncPlanSummary {
    pub(crate) plan_id: String,
    pub(crate) root: String,
    pub(crate) total_items: usize,
    pub(crate) ready: usize,
    pub(crate) unchanged: usize,
    pub(crate) conflicts: usize,
    pub(crate) items: Vec<RatingSyncPlanItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TargetSnapshot {
    Absent,
    Existing {
        size_bytes: u64,
        modified_ms: Option<u64>,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct PlannedRatingWrite {
    pub(crate) asset_id: String,
    pub(crate) target: RatingSyncTarget,
    pub(crate) relative_path: String,
    pub(crate) target_rating: u8,
    pub(crate) snapshot: TargetSnapshot,
}

#[derive(Debug, Clone)]
pub(crate) struct RatingSyncPlan {
    summary: RatingSyncPlanSummary,
    pub(crate) writes: Vec<PlannedRatingWrite>,
}

impl RatingSyncPlan {
    pub(crate) fn summary(&self) -> &RatingSyncPlanSummary {
        &self.summary
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RatingSyncExecuteRequest {
    pub(crate) plan_id: String,
    pub(crate) root: String,
    pub(crate) asset_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RatingSyncExecutionResult {
    pub(crate) asset_id: String,
    pub(crate) target: RatingSyncTarget,
    pub(crate) relative_path: String,
    pub(crate) success: bool,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RatingSyncExecutionSummary {
    pub(crate) succeeded: usize,
    pub(crate) failed: usize,
    pub(crate) results: Vec<RatingSyncExecutionResult>,
}

#[derive(Default)]
pub(crate) struct RatingSyncPlanStore {
    current: Mutex<Option<RatingSyncPlan>>,
}

impl RatingSyncPlanStore {
    pub(crate) fn replace(&self, plan: RatingSyncPlan) -> Result<(), String> {
        *self
            .current
            .lock()
            .map_err(|_| "无法锁定评分同步计划".to_string())? = Some(plan);
        Ok(())
    }

    pub(crate) fn take(&self, plan_id: &str, root: &Path) -> Result<RatingSyncPlan, String> {
        let root =
            fs::canonicalize(root).map_err(|error| format!("评分同步目录不可访问：{error}"))?;
        let mut current = self
            .current
            .lock()
            .map_err(|_| "无法锁定评分同步计划".to_string())?;
        let plan = current
            .as_ref()
            .ok_or_else(|| "评分同步计划不存在，请重新生成".to_string())?;
        let plan_root = fs::canonicalize(&plan.summary.root)
            .map_err(|error| format!("评分同步计划目录不可访问：{error}"))?;
        if plan.summary.plan_id != plan_id || plan_root != root {
            return Err("评分同步计划已失效或目录不匹配".to_string());
        }
        current
            .take()
            .ok_or_else(|| "评分同步计划不存在，请重新生成".to_string())
    }
}

pub(crate) fn resolve_rating(
    state: &RatingState,
    issues: &[String],
    policy: RatingConflictPolicy,
) -> RatingResolution {
    let external = [state.jpeg_metadata, state.raw_xmp]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    if !issues.is_empty()
        || external
            .iter()
            .any(|rating| !(-1..=5).contains(rating) || *rating == -1)
    {
        return RatingResolution::Conflict;
    }

    let distinct_positive = |ratings: &[i8]| {
        let mut values = ratings
            .iter()
            .copied()
            .filter(|rating| (1..=5).contains(rating))
            .collect::<Vec<_>>();
        values.sort_unstable();
        values.dedup();
        values
    };

    match policy {
        RatingConflictPolicy::FramePair => RatingResolution::Ready(state.frame_pair),
        RatingConflictPolicy::External => {
            let values = distinct_positive(&external);
            match values.as_slice() {
                [] if external.contains(&0) => RatingResolution::Ready(0),
                [] => RatingResolution::Ready(state.frame_pair),
                [rating] => RatingResolution::Ready(*rating as u8),
                _ => RatingResolution::Conflict,
            }
        }
        RatingConflictPolicy::Highest => {
            let highest = external
                .iter()
                .copied()
                .filter(|rating| *rating > 0)
                .map(|rating| rating as u8)
                .chain((state.frame_pair > 0).then_some(state.frame_pair))
                .max()
                .unwrap_or_default();
            RatingResolution::Ready(highest)
        }
        RatingConflictPolicy::Skip => {
            let mut ratings = external;
            if state.frame_pair > 0 {
                ratings.push(state.frame_pair as i8);
            }
            let values = distinct_positive(&ratings);
            match values.as_slice() {
                [] => RatingResolution::Ready(0),
                [rating] => RatingResolution::Ready(*rating as u8),
                _ => RatingResolution::Conflict,
            }
        }
    }
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

fn existing_snapshot(root: &Path, relative_path: &str) -> Result<TargetSnapshot, String> {
    let path = root.join(relative_path);
    let metadata = fs::symlink_metadata(&path)
        .map_err(|error| format!("无法读取同步目标 {relative_path}：{error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!("同步目标 {relative_path} 不是可信普通文件"));
    }
    Ok(TargetSnapshot::Existing {
        size_bytes: metadata.len(),
        modified_ms: modified_ms(&metadata),
    })
}

fn absent_snapshot(root: &Path, relative_path: &str) -> Result<TargetSnapshot, String> {
    match fs::symlink_metadata(root.join(relative_path)) {
        Ok(_) => Err(format!("预期新建的同步目标 {relative_path} 已存在")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(TargetSnapshot::Absent),
        Err(error) => Err(format!("无法检查同步目标 {relative_path}：{error}")),
    }
}

fn raw_target(
    root: &Path,
    asset: &PhotoAsset,
    target_rating: u8,
    issues: &mut Vec<String>,
) -> Option<(RatingSyncWrite, PlannedRatingWrite)> {
    if asset.raw_paths.is_empty() {
        issues.push("照片组没有 RAW，无法同步 RAW XMP".to_string());
        return None;
    }
    if asset.raw_paths.len() > 1 {
        issues.push("照片组包含多个 RAW，无法确定 XMP 写入对象".to_string());
        return None;
    }
    if asset.xmp_paths.len() > 1 {
        issues.push("照片组包含多个 XMP，无法确定写入目标".to_string());
        return None;
    }

    let (relative_path, snapshot) = if let Some(path) = asset.xmp_paths.first() {
        match existing_snapshot(root, path) {
            Ok(snapshot) => (path.clone(), snapshot),
            Err(error) => {
                issues.push(error);
                return None;
            }
        }
    } else {
        let path = display_path(&PathBuf::from(&asset.relative_stem).with_extension("xmp"));
        match absent_snapshot(root, &path) {
            Ok(snapshot) => (path, snapshot),
            Err(error) => {
                issues.push(error);
                return None;
            }
        }
    };
    let write = RatingSyncWrite {
        target: RatingSyncTarget::RawXmp,
        relative_path: relative_path.clone(),
        current_rating: asset.rating_state.raw_xmp,
        target_rating,
    };
    let planned = PlannedRatingWrite {
        asset_id: asset.id.clone(),
        target: RatingSyncTarget::RawXmp,
        relative_path,
        target_rating,
        snapshot,
    };
    Some((write, planned))
}

fn jpeg_target(
    root: &Path,
    asset: &PhotoAsset,
    target_rating: u8,
    issues: &mut Vec<String>,
) -> Option<(RatingSyncWrite, PlannedRatingWrite)> {
    if asset.jpeg_paths.is_empty() {
        issues.push("照片组没有 JPG，无法同步 JPG 元数据".to_string());
        return None;
    }
    if asset.jpeg_paths.len() > 1 {
        issues.push("照片组包含多个 JPG，无法确定元数据写入目标".to_string());
        return None;
    }
    let relative_path = asset.jpeg_paths[0].clone();
    let snapshot = match existing_snapshot(root, &relative_path) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            issues.push(error);
            return None;
        }
    };
    let write = RatingSyncWrite {
        target: RatingSyncTarget::JpegMetadata,
        relative_path: relative_path.clone(),
        current_rating: asset.rating_state.jpeg_metadata,
        target_rating,
    };
    let planned = PlannedRatingWrite {
        asset_id: asset.id.clone(),
        target: RatingSyncTarget::JpegMetadata,
        relative_path,
        target_rating,
        snapshot,
    };
    Some((write, planned))
}

fn validate_request(
    index: &PhotoIndex,
    request: &RatingSyncPlanRequest,
) -> Result<PathBuf, String> {
    if !request.targets.raw_xmp && !request.targets.jpeg_metadata {
        return Err("请至少选择一个评分同步目标".to_string());
    }
    if request.minimum_rating > 5
        || request.maximum_rating > 5
        || request.minimum_rating > request.maximum_rating
    {
        return Err("评分范围必须在 0 到 5 星之间，且最低评分不能高于最高评分".to_string());
    }
    if request.targets.jpeg_metadata && !request.jpeg_write_confirmed {
        return Err("启用 JPG 元数据写入前必须明确确认".to_string());
    }
    let root = fs::canonicalize(&request.root)
        .map_err(|error| format!("评分同步目录不可访问：{error}"))?;
    let index_root =
        fs::canonicalize(&index.root).map_err(|error| format!("照片索引目录不可访问：{error}"))?;
    if root != index_root {
        return Err("评分同步目录与照片索引不一致".to_string());
    }
    let requested = request.asset_ids.iter().collect::<HashSet<_>>();
    if requested.len() != request.asset_ids.len() {
        return Err("评分同步范围包含重复照片组".to_string());
    }
    if !requested.is_empty()
        && requested
            .iter()
            .any(|id| !index.assets.iter().any(|asset| &asset.id == *id))
    {
        return Err("评分同步范围包含当前索引中不存在的照片组".to_string());
    }
    Ok(root)
}

pub(crate) fn build_plan(
    index: &PhotoIndex,
    request: &RatingSyncPlanRequest,
    plan_id: String,
) -> Result<RatingSyncPlan, String> {
    let root = validate_request(index, request)?;
    let requested = request.asset_ids.iter().collect::<HashSet<_>>();
    let mut items = Vec::new();
    let mut planned_writes = Vec::new();

    for asset in &index.assets {
        let selected = if requested.is_empty() {
            (request.minimum_rating..=request.maximum_rating)
                .contains(&asset.rating_state.frame_pair)
        } else {
            requested.contains(&asset.id)
        };
        if !selected {
            continue;
        }

        let mut issues = asset.rating_issues.clone();
        let initial_resolution =
            resolve_rating(&asset.rating_state, &issues, request.conflict_policy);
        let target_rating = match initial_resolution {
            RatingResolution::Ready(rating) => rating,
            RatingResolution::Conflict => 0,
        };
        let mut writes = Vec::new();
        let mut candidates = Vec::new();
        if initial_resolution != RatingResolution::Conflict {
            if request.targets.raw_xmp {
                if let Some((write, planned)) = raw_target(&root, asset, target_rating, &mut issues)
                {
                    writes.push(write);
                    candidates.push(planned);
                }
            }
            if request.targets.jpeg_metadata {
                if let Some((write, planned)) =
                    jpeg_target(&root, asset, target_rating, &mut issues)
                {
                    writes.push(write);
                    candidates.push(planned);
                }
            }
        }

        let resolution = resolve_rating(&asset.rating_state, &issues, request.conflict_policy);
        let (resolved, status) = match resolution {
            RatingResolution::Conflict => {
                if issues.is_empty() {
                    issues.push("评分来源不一致，当前冲突策略不会覆盖".to_string());
                }
                (None, RatingSyncStatus::Conflict)
            }
            RatingResolution::Ready(rating) => {
                let unchanged = writes.iter().all(|write| {
                    write.current_rating == Some(rating as i8)
                        || (rating == 0 && write.current_rating.is_none())
                });
                if unchanged {
                    (Some(rating), RatingSyncStatus::Unchanged)
                } else {
                    planned_writes.extend(candidates);
                    (Some(rating), RatingSyncStatus::Ready)
                }
            }
        };
        items.push(RatingSyncPlanItem {
            asset_id: asset.id.clone(),
            relative_stem: asset.relative_stem.clone(),
            frame_pair: asset.rating_state.frame_pair,
            jpeg_metadata: asset.rating_state.jpeg_metadata,
            raw_xmp: asset.rating_state.raw_xmp,
            resolved,
            status,
            writes,
            issues,
        });
    }
    items.sort_by(|left, right| left.asset_id.cmp(&right.asset_id));

    let ready = items
        .iter()
        .filter(|item| item.status == RatingSyncStatus::Ready)
        .count();
    let unchanged = items
        .iter()
        .filter(|item| item.status == RatingSyncStatus::Unchanged)
        .count();
    let conflicts = items
        .iter()
        .filter(|item| item.status == RatingSyncStatus::Conflict)
        .count();
    let summary = RatingSyncPlanSummary {
        plan_id,
        root: display_path(&root),
        total_items: items.len(),
        ready,
        unchanged,
        conflicts,
        items,
    };
    Ok(RatingSyncPlan {
        summary,
        writes: planned_writes,
    })
}

fn safe_target_path(
    root: &Path,
    relative_path: &str,
    target: RatingSyncTarget,
) -> Result<PathBuf, String> {
    let relative = Path::new(relative_path);
    if relative_path.trim().is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("评分同步目标必须是安全相对路径".to_string());
    }
    let extension = relative
        .extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let valid_extension = match target {
        RatingSyncTarget::RawXmp => extension == "xmp",
        RatingSyncTarget::JpegMetadata => matches!(extension.as_str(), "jpg" | "jpeg"),
    };
    if !valid_extension {
        return Err("评分同步目标扩展名与计划类型不一致".to_string());
    }
    let path = root.join(relative);
    let parent = path
        .parent()
        .ok_or_else(|| "无法确定评分同步目标目录".to_string())?;
    let canonical_parent =
        fs::canonicalize(parent).map_err(|error| format!("评分同步目标目录不可访问：{error}"))?;
    if !canonical_parent.starts_with(root) {
        return Err("评分同步目标超出了照片目录".to_string());
    }
    Ok(path)
}

fn revalidate_snapshot(path: &Path, expected: &TargetSnapshot) -> Result<(), String> {
    match (expected, fs::symlink_metadata(path)) {
        (TargetSnapshot::Absent, Err(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(())
        }
        (TargetSnapshot::Absent, _) => Err("同步目标在计划生成后发生变化".to_string()),
        (
            TargetSnapshot::Existing {
                size_bytes,
                modified_ms: expected_modified,
            },
            Ok(metadata),
        ) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err("同步目标在计划生成后发生变化或不再是普通文件".to_string());
            }
            if metadata.len() != *size_bytes || modified_ms(&metadata) != *expected_modified {
                return Err("同步目标在计划生成后发生变化".to_string());
            }
            Ok(())
        }
        (TargetSnapshot::Existing { .. }, Err(_)) => {
            Err("同步目标在计划生成后发生变化或已不存在".to_string())
        }
    }
}

fn transformed_bytes(write: &PlannedRatingWrite, path: &Path) -> Result<Vec<u8>, String> {
    match write.target {
        RatingSyncTarget::RawXmp => {
            let input = match write.snapshot {
                TargetSnapshot::Absent => None,
                TargetSnapshot::Existing { size_bytes, .. } => {
                    if size_bytes > MAX_XMP_BYTES {
                        return Err("XMP 文件超过 4 MiB 限制".to_string());
                    }
                    Some(fs::read(path).map_err(|error| format!("无法读取 XMP 文件：{error}"))?)
                }
            };
            rating_metadata::rewrite_xmp_rating(input.as_deref(), write.target_rating)
        }
        RatingSyncTarget::JpegMetadata => {
            let input = fs::read(path).map_err(|error| format!("无法读取 JPG 文件：{error}"))?;
            rating_metadata::rewrite_jpeg_rating(&input, write.target_rating)
        }
    }
}

fn verify_temporary_rating(
    path: &Path,
    target: RatingSyncTarget,
    expected: u8,
) -> Result<(), String> {
    let rating = match target {
        RatingSyncTarget::RawXmp => rating_metadata::read_sidecar_rating(path),
        RatingSyncTarget::JpegMetadata => rating_metadata::read_jpeg_rating(path),
    }?;
    if rating != Some(expected as i8) {
        return Err("临时文件评分复验失败".to_string());
    }
    Ok(())
}

fn persist_transformed(
    path: &Path,
    bytes: &[u8],
    target: RatingSyncTarget,
    expected_rating: u8,
    snapshot: &TargetSnapshot,
) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "无法确定评分同步目标目录".to_string())?;
    let mut temporary = NamedTempFile::new_in(parent)
        .map_err(|error| format!("无法创建评分同步临时文件：{error}"))?;
    temporary
        .write_all(bytes)
        .and_then(|_| temporary.as_file_mut().sync_all())
        .map_err(|error| format!("无法写入评分同步临时文件：{error}"))?;
    verify_temporary_rating(temporary.path(), target, expected_rating)?;
    match snapshot {
        TargetSnapshot::Absent => temporary
            .persist_noclobber(path)
            .map_err(|error| format!("同步目标在写入前发生变化：{}", error.error))?,
        TargetSnapshot::Existing { .. } => temporary
            .persist(path)
            .map_err(|error| format!("无法替换评分同步目标：{}", error.error))?,
    };

    verify_temporary_rating(path, target, expected_rating)?;
    #[cfg(unix)]
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("评分已写入，但无法同步目标目录：{error}"))?;
    Ok(())
}

fn execute_write(root: &Path, write: &PlannedRatingWrite) -> Result<(), String> {
    let path = safe_target_path(root, &write.relative_path, write.target)?;
    revalidate_snapshot(&path, &write.snapshot)?;
    if matches!(write.snapshot, TargetSnapshot::Existing { .. }) {
        let canonical =
            fs::canonicalize(&path).map_err(|error| format!("评分同步目标不可访问：{error}"))?;
        if !canonical.starts_with(root) {
            return Err("评分同步目标解析后超出了照片目录".to_string());
        }
    }
    let bytes = transformed_bytes(write, &path)?;
    persist_transformed(
        &path,
        &bytes,
        write.target,
        write.target_rating,
        &write.snapshot,
    )
}

pub(crate) fn execute_plan(
    plan: &RatingSyncPlan,
    request: &RatingSyncExecuteRequest,
) -> Result<RatingSyncExecutionSummary, String> {
    if request.plan_id != plan.summary.plan_id {
        return Err("评分同步计划已失效，请重新生成".to_string());
    }
    let root = fs::canonicalize(&request.root)
        .map_err(|error| format!("评分同步目录不可访问：{error}"))?;
    let plan_root = fs::canonicalize(&plan.summary.root)
        .map_err(|error| format!("评分同步计划目录不可访问：{error}"))?;
    if root != plan_root {
        return Err("评分同步目录与当前计划不一致".to_string());
    }
    if request.asset_ids.is_empty() {
        return Err("请至少选择一个可执行照片组".to_string());
    }
    let selected = request.asset_ids.iter().collect::<HashSet<_>>();
    if selected.len() != request.asset_ids.len() {
        return Err("评分同步执行范围包含重复照片组".to_string());
    }
    let planned_assets = plan
        .writes
        .iter()
        .map(|write| &write.asset_id)
        .collect::<HashSet<_>>();
    if selected.iter().any(|id| !planned_assets.contains(id)) {
        return Err("执行范围包含未被当前计划授权的照片组".to_string());
    }

    let mut results = Vec::new();
    for write in plan
        .writes
        .iter()
        .filter(|write| selected.contains(&write.asset_id))
    {
        let outcome = execute_write(&root, write);
        results.push(RatingSyncExecutionResult {
            asset_id: write.asset_id.clone(),
            target: write.target,
            relative_path: write.relative_path.clone(),
            success: outcome.is_ok(),
            message: outcome.err().unwrap_or_else(|| "评分同步完成".to_string()),
        });
    }
    let succeeded = results.iter().filter(|result| result.success).count();
    Ok(RatingSyncExecutionSummary {
        succeeded,
        failed: results.len().saturating_sub(succeeded),
        results,
    })
}
