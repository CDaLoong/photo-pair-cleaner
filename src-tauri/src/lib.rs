mod editors;
mod file_organizer;
mod formats;
mod operation_history;
#[allow(dead_code)]
mod operation_plan;
mod photo_groups;
mod preview;
mod quarantine;
mod rating_metadata;
#[allow(dead_code)]
mod rating_rules;
mod rating_sync;
mod ratings;
mod reference;
mod safety;
#[allow(dead_code)]
mod watermark_color;
mod watermark_commands;
#[allow(dead_code)]
mod watermark_geometry;
#[allow(dead_code)]
mod watermark_model;
#[allow(dead_code)]
mod watermark_render;
mod watermark_source;

use chrono::Utc;
use safety::{CleanupPlan, FileSnapshot, unique_keys};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Manager;
use walkdir::WalkDir;
use watermark_commands::prepare_watermark_source;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanRequest {
    pub reference_source: reference::ReferenceSource,
    pub raw_root: String,
    pub case_sensitive: bool,
    pub mode: ScanMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ScanMode {
    CleanupRaw,
    AuditReference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MatchStatus {
    Matched,
    Unmatched,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FileKind {
    Raw,
    Reference,
    Sidecar,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanItem {
    pub id: String,
    pub relative_path: String,
    pub file_name: String,
    pub extension: String,
    pub size_bytes: u64,
    pub modified_ms: Option<u64>,
    pub match_status: MatchStatus,
    pub kind: FileKind,
    pub matched_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub plan_id: String,
    pub mode: ScanMode,
    pub reference_files: usize,
    pub raw_files: usize,
    pub matched: usize,
    pub unmatched: usize,
    pub sidecars: usize,
    pub reclaimable_bytes: u64,
    pub duplicate_reference_keys: usize,
    pub scanned_at_ms: u64,
    pub warnings: Vec<String>,
    pub items: Vec<ScanItem>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupCandidate {
    pub relative_path: String,
    pub expected_size_bytes: u64,
    pub expected_modified_ms: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupRequest {
    pub plan_id: String,
    pub raw_root: String,
    pub destination: CleanupDestination,
    pub items: Vec<CleanupCandidate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CleanupDestination {
    Trash,
    Quarantine,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupResult {
    pub relative_path: String,
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupSummary {
    pub succeeded: usize,
    pub failed: usize,
    pub destination: CleanupDestination,
    pub operation_id: Option<String>,
    pub quarantine_path: Option<String>,
    pub results: Vec<CleanupResult>,
    pub log_path: Option<String>,
    pub log_warning: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreSummary {
    pub succeeded: usize,
    pub failed: usize,
    pub results: Vec<CleanupResult>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OperationLogRecord<'a> {
    timestamp: String,
    raw_root: &'a str,
    relative_path: &'a str,
    destination: CleanupDestination,
    success: bool,
    message: &'a str,
}

#[derive(Default)]
struct ScanPlanStore {
    current: Mutex<Option<CurrentPlan>>,
}

#[derive(Default)]
struct RatingStore {
    access: Arc<Mutex<()>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PhotoRatingUpdate {
    asset_id: String,
    rating: u8,
    auto_sync: rating_sync::AutoSyncOutcome,
}

struct CurrentPlan {
    cleanup: CleanupPlan,
    mode: ScanMode,
    audit_paths: Vec<String>,
}

fn next_plan_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}-{}-{sequence}", now_ms(), std::process::id())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
        .unwrap_or_default()
}

fn modified_ms(metadata: &fs::Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
}

fn canonical_directory(input: &str, label: &str) -> Result<PathBuf, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(format!("请选择{label}"));
    }

    let path = fs::canonicalize(trimmed).map_err(|error| format!("{label}不可访问：{error}"))?;
    if !path.is_dir() {
        return Err(format!("{label}不是文件夹"));
    }
    Ok(path)
}

fn display_relative(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn collect_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| entry.depth() != 1 || entry.file_name() != quarantine::QUARANTINE_DIR)
    {
        let entry = entry.map_err(|error| format!("扫描目录失败：{error}"))?;
        if entry.file_type().is_file() {
            files.push(entry.into_path());
        }
    }
    files.sort_by_key(|path| display_relative(path).to_lowercase());
    Ok(files)
}

fn scan_item(
    root: &Path,
    path: &Path,
    match_status: MatchStatus,
    kind: FileKind,
    matched_path: Option<String>,
) -> Result<ScanItem, String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| "扫描结果超出了源目录".to_string())?;
    let metadata = fs::metadata(path).map_err(|error| format!("读取文件信息失败：{error}"))?;
    let relative_path = display_relative(relative);
    let prefix = match kind {
        FileKind::Raw => "raw",
        FileKind::Reference => "reference",
        FileKind::Sidecar => "sidecar",
    };

    Ok(ScanItem {
        id: format!("{prefix}:{relative_path}"),
        relative_path,
        file_name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        extension: path
            .extension()
            .map(|extension| format!(".{}", extension.to_string_lossy()))
            .unwrap_or_default(),
        size_bytes: metadata.len(),
        modified_ms: modified_ms(&metadata),
        match_status,
        kind,
        matched_path,
    })
}

pub fn scan_pairs_impl(request: &ScanRequest) -> Result<ScanSummary, String> {
    let raw_root = canonical_directory(&request.raw_root, "RAW 源目录")?;
    if request.mode == ScanMode::AuditReference && !request.reference_source.is_directory() {
        return Err("反向审计只支持 JPG 目录参考源".to_string());
    }
    let reference_index =
        reference::build_index(&request.reference_source, request.case_sensitive)?;
    if let reference::ReferenceSource::Directory { .. } = &request.reference_source {
        let reference_root = reference_index
            .root
            .as_ref()
            .ok_or_else(|| "JPG 参考目录缺失".to_string())?;
        if reference_root.starts_with(&raw_root) || raw_root.starts_with(reference_root) {
            return Err("JPG 参考目录与 RAW 源目录不能相同或互相嵌套".to_string());
        }
    }

    let duplicate_reference_keys = reference_index.duplicate_keys;
    let mut raw_paths = Vec::new();
    let mut raws: HashMap<String, Vec<String>> = HashMap::new();
    let mut sidecars: HashMap<String, Vec<PathBuf>> = HashMap::new();
    for path in collect_files(&raw_root)? {
        let relative = path
            .strip_prefix(&raw_root)
            .map_err(|_| "RAW 文件超出了源目录".to_string())?;
        if formats::is_raw(&path) {
            raws.entry(formats::photo_group_key(relative, request.case_sensitive))
                .or_default()
                .push(display_relative(relative));
            raw_paths.push(path);
        } else if formats::is_sidecar(&path) {
            for key in formats::sidecar_match_keys(relative, request.case_sensitive) {
                sidecars.entry(key).or_default().push(path.clone());
            }
        }
    }

    let mut items = Vec::new();
    let mut unmatched_keys = Vec::new();
    let mut matched = 0usize;
    let mut unmatched = 0usize;
    let mut reclaimable_bytes = 0u64;

    match request.mode {
        ScanMode::CleanupRaw => {
            for path in &raw_paths {
                let relative = path
                    .strip_prefix(&raw_root)
                    .map_err(|_| "RAW 文件超出了源目录".to_string())?;
                let key = formats::photo_group_key(relative, request.case_sensitive);
                let matched_path = reference_index
                    .entries
                    .get(&key)
                    .and_then(|paths| paths.first())
                    .map(|reference| reference.display_path.clone());
                let match_status = if matched_path.is_some() {
                    matched += 1;
                    MatchStatus::Matched
                } else {
                    unmatched += 1;
                    unmatched_keys.push(key);
                    MatchStatus::Unmatched
                };
                let item = scan_item(&raw_root, path, match_status, FileKind::Raw, matched_path)?;
                if match_status == MatchStatus::Unmatched {
                    reclaimable_bytes = reclaimable_bytes.saturating_add(item.size_bytes);
                }
                items.push(item);
            }
        }
        ScanMode::AuditReference => {
            let reference_root = reference_index
                .root
                .as_ref()
                .ok_or_else(|| "JPG 参考目录缺失".to_string())?;
            for (key, paths) in &reference_index.entries {
                let matched_path = raws.get(key).and_then(|paths| paths.first()).cloned();
                for reference in paths {
                    let path = reference
                        .physical_path
                        .as_ref()
                        .ok_or_else(|| "反向审计缺少 JPG 文件路径".to_string())?;
                    let match_status = if matched_path.is_some() {
                        matched += 1;
                        MatchStatus::Matched
                    } else {
                        unmatched += 1;
                        MatchStatus::Unmatched
                    };
                    items.push(scan_item(
                        reference_root,
                        path,
                        match_status,
                        FileKind::Reference,
                        matched_path.clone(),
                    )?);
                }
            }
        }
    }

    let mut sidecar_count = 0usize;
    if request.mode == ScanMode::CleanupRaw {
        for key in unique_keys(unmatched_keys) {
            if let Some(paths) = sidecars.get(&key) {
                for path in paths {
                    let item = scan_item(
                        &raw_root,
                        path,
                        MatchStatus::Unmatched,
                        FileKind::Sidecar,
                        None,
                    )?;
                    reclaimable_bytes = reclaimable_bytes.saturating_add(item.size_bytes);
                    sidecar_count += 1;
                    items.push(item);
                }
            }
        }
    }

    items.sort_by(|left, right| {
        let left_rank = if left.match_status == MatchStatus::Unmatched {
            0
        } else {
            1
        };
        let right_rank = if right.match_status == MatchStatus::Unmatched {
            0
        } else {
            1
        };
        left_rank.cmp(&right_rank).then_with(|| {
            left.relative_path
                .to_lowercase()
                .cmp(&right.relative_path.to_lowercase())
        })
    });

    let mut warnings = Vec::new();
    if request.mode == ScanMode::CleanupRaw && duplicate_reference_keys > 0 {
        warnings.push(format!(
            "参考目录中有 {duplicate_reference_keys} 组重复匹配键，请在执行前核对"
        ));
    }

    Ok(ScanSummary {
        plan_id: String::new(),
        mode: request.mode,
        reference_files: reference_index.source_items,
        raw_files: raw_paths.len(),
        matched,
        unmatched,
        sidecars: sidecar_count,
        reclaimable_bytes,
        duplicate_reference_keys,
        scanned_at_ms: now_ms(),
        warnings,
        items,
    })
}

fn safe_relative_path(value: &str) -> Result<PathBuf, String> {
    let path = Path::new(value);
    if value.trim().is_empty() || path.is_absolute() {
        return Err("文件路径必须是非空相对路径".to_string());
    }
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("文件路径包含不允许的跳转部分".to_string());
    }
    Ok(path.to_path_buf())
}

fn resolve_scan_item_path(raw_root: &str, relative_path: &str) -> Result<PathBuf, String> {
    let root = canonical_directory(raw_root, "RAW 源目录")?;
    let relative = safe_relative_path(relative_path)?;
    let path = fs::canonicalize(root.join(relative))
        .map_err(|error| format!("文件已不存在或不可访问：{error}"))?;
    if !path.starts_with(&root) {
        return Err("文件解析后超出了 RAW 源目录".to_string());
    }
    Ok(path)
}

fn resolve_photo_asset_path(root: &str, relative_path: &str) -> Result<PathBuf, String> {
    let root = canonical_directory(root, "照片目录")?;
    let relative = safe_relative_path(relative_path)?;
    if !formats::is_reference(&relative) && !formats::is_raw(&relative) {
        return Err("只能使用受支持的 JPG/RAW 照片".to_string());
    }
    let path = fs::canonicalize(root.join(relative))
        .map_err(|error| format!("照片已不存在或不可访问：{error}"))?;
    if !path.starts_with(&root) || !path.is_file() {
        return Err("照片解析后超出了所选目录".to_string());
    }
    Ok(path)
}

fn validate_operation_log_path(log_root: &Path, value: &str) -> Result<PathBuf, String> {
    let root = fs::canonicalize(log_root).map_err(|error| format!("日志目录不可访问：{error}"))?;
    let path = fs::canonicalize(value).map_err(|error| format!("操作日志不可访问：{error}"))?;
    if !path.starts_with(&root)
        || path.file_name().and_then(|name| name.to_str()) != Some("operations.jsonl")
    {
        return Err("操作日志路径不在应用日志目录中".to_string());
    }
    Ok(path)
}

fn write_audit_manifest(paths: &[String], destination: &Path) -> Result<(), String> {
    if formats::extension_of(destination) != "txt" {
        return Err("审计清单必须保存为 .txt 文件".to_string());
    }
    if paths.iter().any(|path| path.contains(['\r', '\n'])) {
        return Err("审计路径包含换行符，无法导出为逐行清单".to_string());
    }
    let parent = destination
        .parent()
        .ok_or_else(|| "无法确定审计清单保存目录".to_string())?;
    if !parent.is_dir() {
        return Err("审计清单保存目录不存在".to_string());
    }
    if let Ok(metadata) = fs::symlink_metadata(destination)
        && (metadata.file_type().is_symlink() || !metadata.is_file())
    {
        return Err("审计清单目标不是可信普通文件".to_string());
    }
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(destination)
        .map_err(|error| format!("无法创建审计清单：{error}"))?;
    for path in paths {
        writeln!(file, "{path}").map_err(|error| format!("无法写入审计清单：{error}"))?;
    }
    file.sync_data()
        .map_err(|error| format!("无法同步审计清单：{error}"))
}

#[cfg(target_os = "macos")]
fn reveal_path(path: &Path) -> Result<(), String> {
    Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法在 Finder 中显示：{error}"))
}

#[cfg(target_os = "windows")]
fn reveal_path(path: &Path) -> Result<(), String> {
    Command::new("explorer.exe")
        .arg(format!("/select,{}", path.display()))
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法在文件资源管理器中显示：{error}"))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn reveal_path(path: &Path) -> Result<(), String> {
    let directory = path.parent().unwrap_or(path);
    Command::new("xdg-open")
        .arg(directory)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法在文件管理器中显示：{error}"))
}

#[cfg(target_os = "macos")]
fn open_trash_location() -> Result<(), String> {
    let home = std::env::var_os("HOME").ok_or_else(|| "无法确定用户目录".to_string())?;
    Command::new("open")
        .arg(PathBuf::from(home).join(".Trash"))
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法打开废纸篓：{error}"))
}

#[cfg(target_os = "windows")]
fn open_trash_location() -> Result<(), String> {
    Command::new("explorer.exe")
        .arg("shell:RecycleBinFolder")
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法打开回收站：{error}"))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn open_trash_location() -> Result<(), String> {
    Command::new("xdg-open")
        .arg("trash:///")
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法打开回收站：{error}"))
}

fn validate_delete_candidate(
    raw_root: &Path,
    candidate: &CleanupCandidate,
) -> Result<PathBuf, String> {
    let relative = safe_relative_path(&candidate.relative_path)?;
    let extension = formats::extension_of(&relative);
    if !formats::is_raw(&relative) && !formats::is_sidecar(&relative) {
        return Err(format!("不允许处理 .{extension} 文件"));
    }

    let path = fs::canonicalize(raw_root.join(relative))
        .map_err(|error| format!("文件已不存在或不可访问：{error}"))?;
    if !path.starts_with(raw_root) {
        return Err("文件解析后超出了 RAW 源目录".to_string());
    }
    let metadata = fs::metadata(&path).map_err(|error| format!("无法读取文件信息：{error}"))?;
    if !metadata.is_file() {
        return Err("目标不是普通文件".to_string());
    }
    if metadata.len() != candidate.expected_size_bytes {
        return Err("文件大小在扫描后发生变化，请重新扫描".to_string());
    }
    if let Some(expected) = candidate.expected_modified_ms
        && modified_ms(&metadata) != Some(expected)
    {
        return Err("文件修改时间在扫描后发生变化，请重新扫描".to_string());
    }
    Ok(path)
}

fn write_operation_log(
    log_dir: Option<&Path>,
    raw_root: &str,
    destination: CleanupDestination,
    results: &[CleanupResult],
) -> (Option<String>, Option<String>) {
    let Some(log_dir) = log_dir else {
        return (None, Some("无法确定应用日志目录".to_string()));
    };
    if let Err(error) = fs::create_dir_all(log_dir) {
        return (None, Some(format!("无法创建日志目录：{error}")));
    }
    let log_path = log_dir.join("operations.jsonl");
    let mut file = match OpenOptions::new().create(true).append(true).open(&log_path) {
        Ok(file) => file,
        Err(error) => return (None, Some(format!("无法打开操作日志：{error}"))),
    };

    for result in results {
        let record = OperationLogRecord {
            timestamp: Utc::now().to_rfc3339(),
            raw_root,
            relative_path: &result.relative_path,
            destination,
            success: result.success,
            message: &result.message,
        };
        let line = match serde_json::to_string(&record) {
            Ok(line) => line,
            Err(error) => return (None, Some(format!("无法序列化操作日志：{error}"))),
        };
        if let Err(error) = writeln!(file, "{line}") {
            return (None, Some(format!("无法写入操作日志：{error}")));
        }
    }

    (Some(log_path.to_string_lossy().into_owned()), None)
}

fn cleanup_impl(
    request: &CleanupRequest,
    log_dir: Option<&Path>,
    plan: &CleanupPlan,
) -> Result<CleanupSummary, String> {
    let raw_root = canonical_directory(&request.raw_root, "RAW 源目录")?;
    let mut results = Vec::with_capacity(request.items.len());

    for candidate in &request.items {
        let snapshot = FileSnapshot::new(
            candidate.expected_size_bytes,
            candidate.expected_modified_ms,
        );
        let authorized = plan.authorize(
            &request.plan_id,
            &raw_root,
            &candidate.relative_path,
            &snapshot,
        );
        let outcome = match authorized.and_then(|_| validate_delete_candidate(&raw_root, candidate))
        {
            Ok(path) => match request.destination {
                CleanupDestination::Trash => trash::delete(&path)
                    .map(|_| "已移入系统回收站/废纸篓".to_string())
                    .map_err(|error| format!("移入系统回收站/废纸篓失败：{error}")),
                CleanupDestination::Quarantine => quarantine::move_file(
                    &raw_root,
                    &request.plan_id,
                    Path::new(&candidate.relative_path),
                )
                .map(|_| "已移入 FramePair 隔离区".to_string()),
            },
            Err(error) => Err(error),
        };
        match outcome {
            Ok(message) => results.push(CleanupResult {
                relative_path: candidate.relative_path.clone(),
                success: true,
                message,
            }),
            Err(message) => results.push(CleanupResult {
                relative_path: candidate.relative_path.clone(),
                success: false,
                message,
            }),
        }
    }

    let succeeded = results.iter().filter(|result| result.success).count();
    let failed = results.len().saturating_sub(succeeded);
    let (log_path, log_warning) =
        write_operation_log(log_dir, &request.raw_root, request.destination, &results);
    let quarantined = request.destination == CleanupDestination::Quarantine && succeeded > 0;
    Ok(CleanupSummary {
        succeeded,
        failed,
        destination: request.destination,
        operation_id: quarantined.then(|| request.plan_id.clone()),
        quarantine_path: quarantined
            .then(|| quarantine::operation_root(&raw_root, &request.plan_id))
            .transpose()?
            .map(|path| path.to_string_lossy().into_owned()),
        results,
        log_path,
        log_warning,
    })
}

#[tauri::command]
async fn validate_directory_path(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        canonical_directory(&path, "拖入路径")
            .map(|directory| directory.to_string_lossy().into_owned())
    })
    .await
    .map_err(|error| format!("目录校验任务异常结束：{error}"))?
}

#[tauri::command]
async fn scan_pairs(
    state: tauri::State<'_, ScanPlanStore>,
    request: ScanRequest,
) -> Result<ScanSummary, String> {
    *state
        .current
        .lock()
        .map_err(|_| "无法重置清理计划状态".to_string())? = None;
    let raw_root = canonical_directory(&request.raw_root, "RAW 源目录")?;
    let mut summary = tauri::async_runtime::spawn_blocking(move || scan_pairs_impl(&request))
        .await
        .map_err(|error| format!("扫描任务异常结束：{error}"))??;

    let plan_id = next_plan_id();
    let candidates = summary
        .items
        .iter()
        .filter(|item| {
            summary.mode == ScanMode::CleanupRaw && item.match_status == MatchStatus::Unmatched
        })
        .map(|item| {
            (
                item.relative_path.clone(),
                FileSnapshot::new(item.size_bytes, item.modified_ms),
            )
        });
    let audit_paths = summary
        .items
        .iter()
        .filter(|item| {
            summary.mode == ScanMode::AuditReference
                && item.match_status == MatchStatus::Unmatched
                && item.kind == FileKind::Reference
        })
        .map(|item| item.relative_path.clone())
        .collect();
    let plan = CleanupPlan::new(plan_id.clone(), raw_root, candidates);
    summary.plan_id = plan_id;
    *state
        .current
        .lock()
        .map_err(|_| "无法保存清理计划状态".to_string())? = Some(CurrentPlan {
        cleanup: plan,
        mode: summary.mode,
        audit_paths,
    });
    Ok(summary)
}

#[tauri::command]
async fn execute_cleanup(
    app: tauri::AppHandle,
    state: tauri::State<'_, ScanPlanStore>,
    request: CleanupRequest,
) -> Result<CleanupSummary, String> {
    if request.items.is_empty() {
        return Err("清理计划中没有文件".to_string());
    }
    let raw_root = canonical_directory(&request.raw_root, "RAW 源目录")?;
    let plan = {
        let mut current = state
            .current
            .lock()
            .map_err(|_| "无法读取清理计划状态".to_string())?;
        let current_plan = current
            .as_ref()
            .ok_or_else(|| "清理计划不存在，请重新扫描".to_string())?;
        if current_plan.mode != ScanMode::CleanupRaw {
            return Err("当前是只读审计计划，不能执行清理".to_string());
        }
        if !current_plan.cleanup.matches(&request.plan_id, &raw_root) {
            return Err("清理计划已失效或 RAW 源目录不匹配，请重新扫描".to_string());
        }
        current
            .take()
            .ok_or_else(|| "清理计划不存在，请重新扫描".to_string())?
            .cleanup
    };
    let log_dir = app.path().app_log_dir().ok();
    tauri::async_runtime::spawn_blocking(move || cleanup_impl(&request, log_dir.as_deref(), &plan))
        .await
        .map_err(|error| format!("清理任务异常结束：{error}"))?
}

#[tauri::command]
async fn export_audit_manifest(
    state: tauri::State<'_, ScanPlanStore>,
    plan_id: String,
    raw_root: String,
    destination: String,
) -> Result<(), String> {
    let raw_root = canonical_directory(&raw_root, "RAW 源目录")?;
    let audit_paths = {
        let current = state
            .current
            .lock()
            .map_err(|_| "无法读取审计计划状态".to_string())?;
        let plan = current
            .as_ref()
            .ok_or_else(|| "审计计划不存在，请重新扫描".to_string())?;
        if plan.mode != ScanMode::AuditReference {
            return Err("当前计划不是反向审计计划".to_string());
        }
        if !plan.cleanup.matches(&plan_id, &raw_root) {
            return Err("审计计划已失效或 RAW 源目录不匹配，请重新扫描".to_string());
        }
        plan.audit_paths.clone()
    };
    tauri::async_runtime::spawn_blocking(move || {
        write_audit_manifest(&audit_paths, Path::new(&destination))
    })
    .await
    .map_err(|error| format!("导出审计清单任务异常结束：{error}"))?
}

#[tauri::command]
async fn list_quarantine_operations(
    raw_root: String,
) -> Result<Vec<quarantine::QuarantineOperation>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = canonical_directory(&raw_root, "RAW 源目录")?;
        quarantine::list_operations(&root)
    })
    .await
    .map_err(|error| format!("读取隔离历史任务异常结束：{error}"))?
}

#[tauri::command]
async fn restore_quarantine_operation(
    raw_root: String,
    operation_id: String,
) -> Result<RestoreSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = canonical_directory(&raw_root, "RAW 源目录")?;
        let results = quarantine::restore_operation(&root, &operation_id)?
            .into_iter()
            .map(|result| CleanupResult {
                relative_path: result.relative_path,
                success: result.success,
                message: result.message,
            })
            .collect::<Vec<_>>();
        let succeeded = results.iter().filter(|result| result.success).count();
        Ok(RestoreSummary {
            succeeded,
            failed: results.len().saturating_sub(succeeded),
            results,
        })
    })
    .await
    .map_err(|error| format!("恢复隔离文件任务异常结束：{error}"))?
}

#[tauri::command]
async fn reveal_quarantine_operation(raw_root: String, operation_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = canonical_directory(&raw_root, "RAW 源目录")?;
        let path = quarantine::operation_root(&root, &operation_id)?;
        let path = fs::canonicalize(path).map_err(|error| format!("隔离目录不可访问：{error}"))?;
        if !path.starts_with(&root) {
            return Err("隔离目录解析后超出了 RAW 源目录".to_string());
        }
        reveal_path(&path)
    })
    .await
    .map_err(|error| format!("定位隔离目录任务异常结束：{error}"))?
}

#[tauri::command]
async fn reveal_scan_item(root: String, relative_path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = resolve_scan_item_path(&root, &relative_path)?;
        reveal_path(&path)
    })
    .await
    .map_err(|error| format!("定位文件任务异常结束：{error}"))?
}

#[tauri::command]
async fn index_photo_directory(
    app: tauri::AppHandle,
    state: tauri::State<'_, RatingStore>,
    root: String,
) -> Result<photo_groups::PhotoIndex, String> {
    let database_path = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法确定评分数据目录：{error}"))?
        .join("photo-ratings.json");
    let access = Arc::clone(&state.access);
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = access
            .lock()
            .map_err(|_| "无法锁定评分数据库".to_string())?;
        let mut index = photo_groups::index_directory(Path::new(&root))?;
        let ratings = ratings::load_ratings(&database_path, Path::new(&root))?;
        photo_groups::apply_framepair_ratings(&mut index, &ratings);
        Ok(index)
    })
    .await
    .map_err(|error| format!("照片索引任务异常结束：{error}"))?
}

#[tauri::command]
async fn set_photo_rating(
    app: tauri::AppHandle,
    state: tauri::State<'_, RatingStore>,
    root: String,
    relative_path: String,
    rating: u8,
) -> Result<PhotoRatingUpdate, String> {
    let database_path = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法确定评分数据目录：{error}"))?
        .join("photo-ratings.json");
    let sync_database_path = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法确定评分同步设置目录：{error}"))?
        .join("rating-sync.json");
    let access = Arc::clone(&state.access);
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = access
            .lock()
            .map_err(|_| "无法锁定评分数据库".to_string())?;
        let update = ratings::set_rating(&database_path, Path::new(&root), &relative_path, rating)?;
        let sync_state = rating_sync::load_sync_state(&sync_database_path, Some(Path::new(&root)));
        let auto_sync = match sync_state {
            Ok(sync_state) if sync_state.settings.mode == rating_sync::RatingSyncMode::Manual => {
                rating_sync::AutoSyncOutcome {
                    status: rating_sync::AutoSyncStatus::Disabled,
                    message: None,
                }
            }
            Ok(sync_state) => {
                let index =
                    photo_groups::index_directory(Path::new(&root)).and_then(|mut index| {
                        let saved = ratings::load_ratings(&database_path, Path::new(&root))?;
                        photo_groups::apply_framepair_ratings(&mut index, &saved);
                        Ok(index)
                    });
                match index {
                    Ok(index) => rating_sync::auto_sync_saved_rating(
                        &sync_database_path,
                        &index,
                        &sync_state.settings,
                        Path::new(&root),
                        &update.asset_id,
                        update.rating,
                        &next_plan_id(),
                        now_ms(),
                    ),
                    Err(error) => {
                        let pending = rating_sync::PendingRatingSync {
                            root: root.clone(),
                            asset_id: update.asset_id.clone(),
                            rating: update.rating,
                            targets: sync_state.settings.targets,
                            error: error.clone(),
                            failed_at_ms: now_ms(),
                        };
                        let message = rating_sync::record_pending(&sync_database_path, pending)
                            .err()
                            .map(|pending_error| {
                                format!("{error}；待处理状态保存失败：{pending_error}")
                            })
                            .unwrap_or(error);
                        rating_sync::AutoSyncOutcome {
                            status: rating_sync::AutoSyncStatus::Pending,
                            message: Some(message),
                        }
                    }
                }
            }
            Err(error) => rating_sync::AutoSyncOutcome {
                status: rating_sync::AutoSyncStatus::Pending,
                message: Some(format!(
                    "FramePair 评分已保存，但无法读取自动同步设置：{error}"
                )),
            },
        };
        Ok(PhotoRatingUpdate {
            asset_id: update.asset_id,
            rating: update.rating,
            auto_sync,
        })
    })
    .await
    .map_err(|error| format!("保存照片评分任务异常结束：{error}"))?
}

#[tauri::command]
async fn get_rating_sync_state(
    app: tauri::AppHandle,
    state: tauri::State<'_, RatingStore>,
    root: Option<String>,
) -> Result<rating_sync::RatingSyncState, String> {
    let database_path = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法确定评分同步设置目录：{error}"))?
        .join("rating-sync.json");
    let access = Arc::clone(&state.access);
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = access
            .lock()
            .map_err(|_| "无法锁定评分同步设置".to_string())?;
        rating_sync::load_sync_state(&database_path, root.as_deref().map(Path::new))
    })
    .await
    .map_err(|error| format!("读取评分同步设置任务异常结束：{error}"))?
}

#[tauri::command]
async fn save_rating_sync_settings(
    app: tauri::AppHandle,
    state: tauri::State<'_, RatingStore>,
    settings: rating_sync::RatingSyncSettings,
) -> Result<rating_sync::RatingSyncSettings, String> {
    let database_path = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法确定评分同步设置目录：{error}"))?
        .join("rating-sync.json");
    let access = Arc::clone(&state.access);
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = access
            .lock()
            .map_err(|_| "无法锁定评分同步设置".to_string())?;
        rating_sync::save_sync_settings(&database_path, &settings)
    })
    .await
    .map_err(|error| format!("保存评分同步设置任务异常结束：{error}"))?
}

#[tauri::command]
async fn generate_rating_sync_plan(
    app: tauri::AppHandle,
    rating_state: tauri::State<'_, RatingStore>,
    plan_state: tauri::State<'_, rating_sync::RatingSyncPlanStore>,
    request: rating_sync::RatingSyncPlanRequest,
) -> Result<rating_sync::RatingSyncPlanSummary, String> {
    let database_path = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法确定评分数据目录：{error}"))?
        .join("photo-ratings.json");
    let access = Arc::clone(&rating_state.access);
    let plan_id = next_plan_id();
    let plan = tauri::async_runtime::spawn_blocking(move || {
        let _guard = access
            .lock()
            .map_err(|_| "无法锁定评分数据库".to_string())?;
        let mut index = photo_groups::index_directory(Path::new(&request.root))?;
        let ratings = ratings::load_ratings(&database_path, Path::new(&request.root))?;
        photo_groups::apply_framepair_ratings(&mut index, &ratings);
        rating_sync::build_plan(&index, &request, plan_id)
    })
    .await
    .map_err(|error| format!("评分同步计划任务异常结束：{error}"))??;
    let summary = plan.summary().clone();
    plan_state.replace(plan)?;
    Ok(summary)
}

#[tauri::command]
async fn execute_rating_sync_plan(
    rating_state: tauri::State<'_, RatingStore>,
    plan_state: tauri::State<'_, rating_sync::RatingSyncPlanStore>,
    request: rating_sync::RatingSyncExecuteRequest,
) -> Result<rating_sync::RatingSyncExecutionSummary, String> {
    let plan = plan_state.take(&request.plan_id, Path::new(&request.root))?;
    let access = Arc::clone(&rating_state.access);
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = access
            .lock()
            .map_err(|_| "无法锁定评分同步任务".to_string())?;
        rating_sync::execute_plan(&plan, &request)
    })
    .await
    .map_err(|error| format!("评分同步执行任务异常结束：{error}"))?
}

#[tauri::command]
async fn get_rating_rules(
    app: tauri::AppHandle,
    state: tauri::State<'_, RatingStore>,
) -> Result<rating_rules::RatingRuleState, String> {
    let database_path = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法确定评分规则目录：{error}"))?
        .join("rating-rules.json");
    let access = Arc::clone(&state.access);
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = access.lock().map_err(|_| "无法锁定评分规则".to_string())?;
        rating_rules::load_rules(&database_path)
    })
    .await
    .map_err(|error| format!("读取评分规则任务异常结束：{error}"))?
}

#[tauri::command]
async fn save_rating_rules(
    app: tauri::AppHandle,
    state: tauri::State<'_, RatingStore>,
    rules: Vec<rating_rules::RatingRule>,
) -> Result<rating_rules::RatingRuleState, String> {
    let database_path = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法确定评分规则目录：{error}"))?
        .join("rating-rules.json");
    let access = Arc::clone(&state.access);
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = access.lock().map_err(|_| "无法锁定评分规则".to_string())?;
        rating_rules::save_rules(&database_path, &rules)
    })
    .await
    .map_err(|error| format!("保存评分规则任务异常结束：{error}"))?
}

#[tauri::command]
async fn import_rating_rules(path: String) -> Result<rating_rules::RatingRuleState, String> {
    tauri::async_runtime::spawn_blocking(move || rating_rules::import_rules(Path::new(&path)))
        .await
        .map_err(|error| format!("导入评分规则任务异常结束：{error}"))?
}

#[tauri::command]
async fn export_rating_rules(
    path: String,
    rules: Vec<rating_rules::RatingRule>,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        rating_rules::export_rules(Path::new(&path), &rules)
    })
    .await
    .map_err(|error| format!("导出评分规则任务异常结束：{error}"))?
}

#[tauri::command]
async fn generate_operation_plan(
    app: tauri::AppHandle,
    rating_state: tauri::State<'_, RatingStore>,
    plan_state: tauri::State<'_, operation_plan::OperationPlanStore>,
    request: operation_plan::OperationPlanRequest,
) -> Result<operation_plan::OperationPlanSummary, String> {
    let database_path = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法确定评分数据目录：{error}"))?
        .join("photo-ratings.json");
    let access = Arc::clone(&rating_state.access);
    let plan_id = next_plan_id();
    let plan = tauri::async_runtime::spawn_blocking(move || {
        let _guard = access
            .lock()
            .map_err(|_| "无法锁定评分整理计划".to_string())?;
        let mut index = photo_groups::index_directory(Path::new(&request.root))?;
        let ratings = ratings::load_ratings(&database_path, Path::new(&request.root))?;
        photo_groups::apply_framepair_ratings(&mut index, &ratings);
        operation_plan::build_operation_plan(&index, &request, plan_id)
    })
    .await
    .map_err(|error| format!("评分整理计划任务异常结束：{error}"))??;
    let summary = plan.summary().clone();
    plan_state.replace(plan)?;
    Ok(summary)
}

#[tauri::command]
async fn execute_operation_plan(
    app: tauri::AppHandle,
    plan_state: tauri::State<'_, operation_plan::OperationPlanStore>,
    request: operation_plan::ExecutionSelection,
) -> Result<file_organizer::OrganizerExecutionSummary, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法确定评分整理历史目录：{error}"))?;
    let plan = plan_state.take_for_execution(&request)?;
    let operation_id = next_plan_id();
    let created_at_ms = now_ms();
    tauri::async_runtime::spawn_blocking(move || {
        file_organizer::execute_authorized_plan(&app_data_dir, operation_id, created_at_ms, plan)
    })
    .await
    .map_err(|error| format!("评分整理执行任务异常结束：{error}"))?
}

#[tauri::command]
async fn list_rating_operation_history(
    app: tauri::AppHandle,
) -> Result<Vec<operation_history::OperationHistoryEntry>, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法确定评分整理历史目录：{error}"))?;
    tauri::async_runtime::spawn_blocking(move || operation_history::list_operations(&app_data_dir))
        .await
        .map_err(|error| format!("读取评分整理历史任务异常结束：{error}"))?
}

#[tauri::command]
async fn restore_rating_move(
    app: tauri::AppHandle,
    request: file_organizer::OrganizerRecoveryRequest,
) -> Result<file_organizer::OrganizerRecoverySummary, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法确定评分整理历史目录：{error}"))?;
    let created_at_ms = now_ms();
    tauri::async_runtime::spawn_blocking(move || {
        file_organizer::restore_move_operation(
            &app_data_dir,
            &request.operation_id,
            &request.group_ids,
            created_at_ms,
        )
    })
    .await
    .map_err(|error| format!("恢复评分移动任务异常结束：{error}"))?
}

#[tauri::command]
async fn restore_rating_quarantine(
    app: tauri::AppHandle,
    request: file_organizer::OrganizerRecoveryRequest,
) -> Result<file_organizer::OrganizerRecoverySummary, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法确定评分整理历史目录：{error}"))?;
    let created_at_ms = now_ms();
    tauri::async_runtime::spawn_blocking(move || {
        file_organizer::restore_quarantine_operation(
            &app_data_dir,
            &request.operation_id,
            &request.group_ids,
            created_at_ms,
        )
    })
    .await
    .map_err(|error| format!("恢复评分隔离任务异常结束：{error}"))?
}

#[tauri::command]
async fn undo_rating_copy(
    app: tauri::AppHandle,
    request: file_organizer::OrganizerRecoveryRequest,
) -> Result<file_organizer::OrganizerRecoverySummary, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法确定评分整理历史目录：{error}"))?;
    let created_at_ms = now_ms();
    tauri::async_runtime::spawn_blocking(move || {
        file_organizer::undo_copy_operation(
            &app_data_dir,
            &request.operation_id,
            &request.group_ids,
            created_at_ms,
        )
    })
    .await
    .map_err(|error| format!("撤销评分复制任务异常结束：{error}"))?
}

#[tauri::command]
async fn list_external_editors() -> Result<Vec<editors::ExternalEditor>, String> {
    tauri::async_runtime::spawn_blocking(editors::discover_installed)
        .await
        .map_err(|error| format!("发现外部编辑器任务异常结束：{error}"))
}

#[tauri::command]
async fn open_photo_in_editor(
    root: String,
    relative_path: String,
    editor_id: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let photo = resolve_photo_asset_path(&root, &relative_path)?;
        let editor = editors::discover_installed()
            .into_iter()
            .find(|editor| editor.id == editor_id)
            .ok_or_else(|| "外部编辑器不存在或已经卸载".to_string())?;
        editors::open_with(&editor, &photo)
    })
    .await
    .map_err(|error| format!("启动外部编辑器任务异常结束：{error}"))?
}

#[tauri::command]
async fn load_photo_thumbnail(
    app: tauri::AppHandle,
    root: String,
    relative_path: String,
    max_edge: u32,
) -> Result<tauri::ipc::Response, String> {
    let cache_root = app
        .path()
        .app_cache_dir()
        .map_err(|error| format!("无法确定缩略图缓存目录：{error}"))?
        .join("photo-thumbnails");
    let bytes = tauri::async_runtime::spawn_blocking(move || {
        preview::load_thumbnail(Path::new(&root), &relative_path, max_edge, &cache_root)
    })
    .await
    .map_err(|error| format!("缩略图任务异常结束：{error}"))??;
    Ok(tauri::ipc::Response::new(bytes))
}

#[tauri::command]
async fn reveal_operation_log(app: tauri::AppHandle, log_path: String) -> Result<(), String> {
    let log_root = app
        .path()
        .app_log_dir()
        .map_err(|error| format!("无法确定应用日志目录：{error}"))?;
    tauri::async_runtime::spawn_blocking(move || {
        let path = validate_operation_log_path(&log_root, &log_path)?;
        reveal_path(&path)
    })
    .await
    .map_err(|error| format!("定位操作日志任务异常结束：{error}"))?
}

#[tauri::command]
async fn open_system_trash() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(open_trash_location)
        .await
        .map_err(|error| format!("打开回收站任务异常结束：{error}"))?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(ScanPlanStore::default())
        .manage(RatingStore::default())
        .manage(rating_sync::RatingSyncPlanStore::default())
        .manage(operation_plan::OperationPlanStore::default())
        .invoke_handler(tauri::generate_handler![
            validate_directory_path,
            scan_pairs,
            execute_cleanup,
            export_audit_manifest,
            list_quarantine_operations,
            restore_quarantine_operation,
            reveal_quarantine_operation,
            reveal_scan_item,
            index_photo_directory,
            set_photo_rating,
            get_rating_sync_state,
            save_rating_sync_settings,
            generate_rating_sync_plan,
            execute_rating_sync_plan,
            get_rating_rules,
            save_rating_rules,
            import_rating_rules,
            export_rating_rules,
            generate_operation_plan,
            execute_operation_plan,
            list_rating_operation_history,
            restore_rating_move,
            restore_rating_quarantine,
            undo_rating_copy,
            load_photo_thumbnail,
            list_external_editors,
            open_photo_in_editor,
            prepare_watermark_source,
            reveal_operation_log,
            open_system_trash
        ])
        .run(tauri::generate_context!())
        .expect("failed to run FramePair");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(reference_root: &Path, raw_root: &Path) -> ScanRequest {
        ScanRequest {
            reference_source: reference::ReferenceSource::Directory {
                root: reference_root.to_string_lossy().into_owned(),
            },
            raw_root: raw_root.to_string_lossy().into_owned(),
            case_sensitive: false,
            mode: ScanMode::CleanupRaw,
        }
    }

    #[test]
    fn scans_nested_pairs_and_exposes_missing_sidecars() {
        let temp = tempfile::tempdir().expect("temp directory");
        let reference_root = temp.path().join("JPG");
        let raw_root = temp.path().join("RAW");
        let reference_day = reference_root.join("20260712");
        let raw_day = raw_root.join("20260712");
        fs::create_dir_all(&reference_day).expect("reference directory");
        fs::create_dir_all(&raw_day).expect("raw directory");

        fs::write(reference_day.join("DSC_0001.JPG"), b"jpg").expect("jpg");
        fs::write(raw_day.join("DSC_0001.NEF"), b"kept raw").expect("kept raw");
        fs::write(raw_day.join("DSC_0002.NEF"), b"missing raw").expect("missing raw");
        fs::write(raw_day.join("DSC_0002.xmp"), b"sidecar").expect("sidecar");

        let summary = scan_pairs_impl(&request(&reference_root, &raw_root)).expect("scan");
        assert_eq!(summary.reference_files, 1);
        assert_eq!(summary.raw_files, 2);
        assert_eq!(summary.matched, 1);
        assert_eq!(summary.unmatched, 1);
        assert_eq!(summary.sidecars, 1);
        assert_eq!(summary.items.len(), 3);
        assert!(summary.items.iter().any(|item| {
            item.relative_path == "20260712/DSC_0002.NEF"
                && item.match_status == MatchStatus::Unmatched
                && item.kind == FileKind::Raw
        }));
        assert!(summary.items.iter().any(|item| {
            item.relative_path == "20260712/DSC_0002.xmp"
                && item.match_status == MatchStatus::Unmatched
                && item.kind == FileKind::Sidecar
        }));
    }

    #[test]
    fn scans_every_supported_raw_extension_from_the_backend_policy() {
        let temp = tempfile::tempdir().expect("temp directory");
        let reference_root = temp.path().join("JPG");
        let raw_root = temp.path().join("RAW");
        fs::create_dir_all(&reference_root).expect("reference directory");
        fs::create_dir_all(&raw_root).expect("raw directory");

        for extension in formats::RAW_EXTENSIONS {
            fs::write(
                raw_root.join(format!("photo-{extension}.{extension}")),
                b"raw",
            )
            .expect("raw file");
        }

        let summary = scan_pairs_impl(&request(&reference_root, &raw_root)).expect("scan");
        assert_eq!(summary.raw_files, formats::RAW_EXTENSIONS.len());
        assert_eq!(summary.unmatched, formats::RAW_EXTENSIONS.len());
    }

    #[test]
    fn scan_excludes_framepair_quarantine_contents() {
        let temp = tempfile::tempdir().expect("temp directory");
        let reference_root = temp.path().join("JPG");
        let raw_root = temp.path().join("RAW");
        fs::create_dir_all(&reference_root).expect("reference directory");
        fs::create_dir_all(raw_root.join(".framepair-quarantine/operation-1"))
            .expect("quarantine directory");
        fs::write(raw_root.join("active.NEF"), b"active").expect("active raw");
        fs::write(
            raw_root.join(".framepair-quarantine/operation-1/hidden.NEF"),
            b"hidden",
        )
        .expect("quarantined raw");

        let summary = scan_pairs_impl(&request(&reference_root, &raw_root)).expect("scan");
        assert_eq!(summary.raw_files, 1);
        assert!(
            summary
                .items
                .iter()
                .all(|item| !item.relative_path.contains(".framepair-quarantine"))
        );
    }

    #[test]
    fn audits_references_without_matching_raws() {
        let temp = tempfile::tempdir().expect("temp directory");
        let reference_root = temp.path().join("jpg");
        let raw_root = temp.path().join("raw");
        fs::create_dir_all(reference_root.join("day")).expect("jpg day");
        fs::create_dir_all(raw_root.join("day")).expect("raw day");
        fs::write(reference_root.join("day/kept.JPG"), b"jpg").expect("kept jpg");
        fs::write(reference_root.join("day/orphan.JPG"), b"jpg").expect("orphan jpg");
        fs::write(raw_root.join("day/kept.CR3"), b"raw").expect("kept raw");

        let mut scan_request = request(&reference_root, &raw_root);
        scan_request.mode = ScanMode::AuditReference;
        let summary = scan_pairs_impl(&scan_request).expect("reverse audit");
        assert_eq!(summary.mode, ScanMode::AuditReference);
        assert_eq!(summary.matched, 1);
        assert_eq!(summary.unmatched, 1);
        assert!(summary.items.iter().any(|item| {
            item.relative_path == "day/orphan.JPG"
                && item.kind == FileKind::Reference
                && item.match_status == MatchStatus::Unmatched
        }));
    }

    #[test]
    fn writes_a_utf8_audit_manifest_and_rejects_other_extensions() {
        let temp = tempfile::tempdir().expect("temp directory");
        let destination = temp.path().join("unmatched.txt");
        write_audit_manifest(
            &["day/orphan.JPG".to_string(), "第二天/照片.jpeg".to_string()],
            &destination,
        )
        .expect("manifest export");
        assert_eq!(
            fs::read_to_string(destination).expect("manifest contents"),
            "day/orphan.JPG\n第二天/照片.jpeg\n"
        );
        assert!(
            write_audit_manifest(
                &["day/orphan.JPG".to_string()],
                &temp.path().join("bad.csv")
            )
            .is_err()
        );
    }

    #[test]
    fn cleanup_scan_accepts_a_manifest_reference_source() {
        let temp = tempfile::tempdir().expect("temp directory");
        let raw_root = temp.path().join("raw");
        let manifest = temp.path().join("keepers.txt");
        fs::create_dir_all(raw_root.join("day")).expect("raw day");
        fs::write(raw_root.join("day/a.NEF"), b"kept").expect("kept raw");
        fs::write(raw_root.join("day/b.NEF"), b"missing").expect("missing raw");
        fs::write(&manifest, "day/a.JPG\n").expect("manifest");

        let summary = scan_pairs_impl(&ScanRequest {
            reference_source: reference::ReferenceSource::Manifest {
                path: manifest.to_string_lossy().into_owned(),
            },
            raw_root: raw_root.to_string_lossy().into_owned(),
            case_sensitive: false,
            mode: ScanMode::CleanupRaw,
        })
        .expect("manifest scan");
        assert_eq!(summary.matched, 1);
        assert_eq!(summary.unmatched, 1);
    }

    #[test]
    fn cleanup_scan_accepts_xmp_ratings_inside_the_raw_root() {
        let temp = tempfile::tempdir().expect("temp directory");
        let raw_root = temp.path().join("raw");
        fs::create_dir_all(&raw_root).expect("raw root");
        fs::write(raw_root.join("a.NEF"), b"kept").expect("kept raw");
        fs::write(raw_root.join("b.NEF"), b"missing").expect("missing raw");
        fs::write(
            raw_root.join("a.NEF.xmp"),
            br#"<rdf:Description xmp:Rating="5" />"#,
        )
        .expect("rated xmp");

        let summary = scan_pairs_impl(&ScanRequest {
            reference_source: reference::ReferenceSource::XmpRating {
                root: raw_root.to_string_lossy().into_owned(),
                minimum_rating: 4,
            },
            raw_root: raw_root.to_string_lossy().into_owned(),
            case_sensitive: false,
            mode: ScanMode::CleanupRaw,
        })
        .expect("xmp scan");
        assert_eq!(summary.matched, 1);
        assert_eq!(summary.unmatched, 1);
    }

    #[test]
    fn matches_double_extension_xmp_sidecars_to_missing_raws() {
        let temp = tempfile::tempdir().expect("temp directory");
        let reference_root = temp.path().join("JPG");
        let raw_root = temp.path().join("RAW");
        fs::create_dir_all(&reference_root).expect("reference directory");
        fs::create_dir_all(&raw_root).expect("raw directory");
        fs::write(raw_root.join("photo.NEF"), b"raw").expect("raw file");
        fs::write(raw_root.join("photo.NEF.xmp"), b"xmp").expect("xmp file");

        let summary = scan_pairs_impl(&request(&reference_root, &raw_root)).expect("scan");
        assert_eq!(summary.sidecars, 1);
        assert!(
            summary
                .items
                .iter()
                .any(|item| item.relative_path == "photo.NEF.xmp")
        );
    }

    #[test]
    fn delete_validation_accepts_supported_raws_and_rejects_other_files() {
        let temp = tempfile::tempdir().expect("temp directory");
        let raw = temp.path().join("photo.CR3");
        let text = temp.path().join("notes.txt");
        fs::write(&raw, b"raw").expect("raw file");
        fs::write(&text, b"text").expect("text file");
        let root = fs::canonicalize(temp.path()).expect("canonical root");

        for (name, accepted) in [("photo.CR3", true), ("notes.txt", false)] {
            let metadata = fs::metadata(temp.path().join(name)).expect("metadata");
            let candidate = CleanupCandidate {
                relative_path: name.to_string(),
                expected_size_bytes: metadata.len(),
                expected_modified_ms: modified_ms(&metadata),
            };
            assert_eq!(
                validate_delete_candidate(&root, &candidate).is_ok(),
                accepted,
                "unexpected validation result for {name}"
            );
        }
    }

    #[test]
    fn cleanup_plan_can_move_an_authorized_file_to_quarantine() {
        let temp = tempfile::tempdir().expect("temp directory");
        let raw_root = temp.path().join("RAW");
        fs::create_dir_all(&raw_root).expect("raw directory");
        let raw = raw_root.join("day/photo.CR3");
        fs::create_dir_all(raw.parent().expect("raw parent")).expect("raw parent directory");
        fs::write(&raw, b"raw").expect("raw file");
        let metadata = fs::metadata(&raw).expect("metadata");
        let relative_path = "day/photo.CR3".to_string();
        let snapshot = FileSnapshot::new(metadata.len(), modified_ms(&metadata));
        let plan = CleanupPlan::new(
            "1000-1-1".to_string(),
            fs::canonicalize(&raw_root).expect("canonical raw root"),
            [(relative_path.clone(), snapshot)],
        );
        let request = CleanupRequest {
            plan_id: "1000-1-1".to_string(),
            raw_root: raw_root.to_string_lossy().into_owned(),
            destination: CleanupDestination::Quarantine,
            items: vec![CleanupCandidate {
                relative_path: relative_path.clone(),
                expected_size_bytes: metadata.len(),
                expected_modified_ms: modified_ms(&metadata),
            }],
        };

        let result = cleanup_impl(&request, None, &plan).expect("cleanup");
        assert_eq!(result.succeeded, 1);
        assert_eq!(result.failed, 0);
        assert_eq!(result.operation_id.as_deref(), Some("1000-1-1"));
        assert!(!raw.exists());
        assert!(
            raw_root
                .join(".framepair-quarantine/1000-1-1/day/photo.CR3")
                .exists()
        );
    }

    #[test]
    fn rejects_overlapping_roots() {
        let temp = tempfile::tempdir().expect("temp directory");
        let root = temp.path().join("photos");
        let nested = root.join("raw");
        fs::create_dir_all(&nested).expect("directories");
        let error = scan_pairs_impl(&request(&root, &nested)).expect_err("overlap should fail");
        assert!(error.contains("不能相同或互相嵌套"));
    }

    #[test]
    fn validates_dropped_directories_and_rejects_files() {
        let temp = tempfile::tempdir().expect("temp directory");
        let directory = temp.path().join("photos");
        let file = temp.path().join("photo.NEF");
        fs::create_dir_all(&directory).expect("photo directory");
        fs::write(&file, b"raw").expect("raw file");

        let canonical = canonical_directory(&directory.to_string_lossy(), "拖入路径")
            .expect("directory should be accepted");
        assert_eq!(
            canonical,
            fs::canonicalize(directory).expect("canonical path")
        );
        assert!(canonical_directory(&file.to_string_lossy(), "拖入路径").is_err());
    }

    #[test]
    fn exposes_each_sidecar_once_for_repeated_missing_match_keys() {
        let temp = tempfile::tempdir().expect("temp directory");
        let reference_root = temp.path().join("JPG");
        let raw_root = temp.path().join("RAW");
        fs::create_dir_all(&reference_root).expect("reference directory");
        fs::create_dir_all(&raw_root).expect("raw directory");

        fs::write(raw_root.join("DSC_0001.NEF"), b"nef").expect("nef");
        fs::write(raw_root.join("DSC_0001.CR3"), b"raw").expect("raw");
        fs::write(raw_root.join("DSC_0001.xmp"), b"xmp").expect("xmp");

        let summary = scan_pairs_impl(&request(&reference_root, &raw_root)).expect("scan");

        assert_eq!(summary.unmatched, 2);
        assert_eq!(summary.sidecars, 1);
        assert_eq!(
            summary
                .items
                .iter()
                .filter(|item| item.kind == FileKind::Sidecar)
                .count(),
            1
        );
    }

    #[test]
    fn rejects_path_traversal_and_unexpected_extensions() {
        assert!(safe_relative_path("../outside.NEF").is_err());
        assert!(safe_relative_path("/absolute.NEF").is_err());

        let temp = tempfile::tempdir().expect("temp directory");
        let file = temp.path().join("notes.txt");
        fs::write(&file, b"do not delete").expect("file");
        let metadata = fs::metadata(&file).expect("metadata");
        let candidate = CleanupCandidate {
            relative_path: "notes.txt".to_string(),
            expected_size_bytes: metadata.len(),
            expected_modified_ms: modified_ms(&metadata),
        };
        let root = fs::canonicalize(temp.path()).expect("canonical root");
        assert!(validate_delete_candidate(&root, &candidate).is_err());
    }

    #[test]
    fn reveal_path_resolution_stays_inside_raw_root() {
        let temp = tempfile::tempdir().expect("temp directory");
        let raw_root = temp.path().join("RAW");
        fs::create_dir_all(&raw_root).expect("raw directory");
        fs::write(raw_root.join("photo.NEF"), b"raw").expect("raw file");

        let raw_root_value = raw_root.to_string_lossy().into_owned();
        let resolved = resolve_scan_item_path(&raw_root_value, "photo.NEF").expect("safe path");
        assert_eq!(
            resolved,
            fs::canonicalize(raw_root.join("photo.NEF")).expect("canonical")
        );
        assert!(resolve_scan_item_path(&raw_root_value, "../outside.NEF").is_err());
    }

    #[test]
    fn operation_log_must_use_expected_file_inside_log_root() {
        let temp = tempfile::tempdir().expect("temp directory");
        let log_root = temp.path().join("logs");
        fs::create_dir_all(&log_root).expect("log directory");
        let operation_log = log_root.join("operations.jsonl");
        let other_log = log_root.join("other.jsonl");
        fs::write(&operation_log, b"{}").expect("operation log");
        fs::write(&other_log, b"{}").expect("other log");

        assert!(validate_operation_log_path(&log_root, &operation_log.to_string_lossy()).is_ok());
        assert!(validate_operation_log_path(&log_root, &other_log.to_string_lossy()).is_err());
    }

    #[test]
    fn frontend_exposes_rating_organizer_execution() {
        let _ = execute_operation_plan;
        let _ = list_rating_operation_history;
        let _ = restore_rating_move;
        let _ = undo_rating_copy;

        let source = include_str!("lib.rs");
        let handler = source
            .split("tauri::generate_handler![")
            .nth(1)
            .and_then(|value| value.split("])").next())
            .expect("Tauri command registration");
        assert!(!handler.contains("execute_rating_cleanup"));
    }
}
