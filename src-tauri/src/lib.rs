mod safety;

use chrono::Utc;
use safety::{DeletionPlan, FileSnapshot, unique_keys};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Manager;
use walkdir::WalkDir;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanRequest {
    pub reference_root: String,
    pub raw_root: String,
    pub reference_extensions: Vec<String>,
    pub raw_extensions: Vec<String>,
    pub sidecar_extensions: Vec<String>,
    pub case_sensitive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ScanStatus {
    Keep,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FileKind {
    Raw,
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
    pub status: ScanStatus,
    pub kind: FileKind,
    pub matched_reference: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub plan_id: String,
    pub reference_files: usize,
    pub raw_files: usize,
    pub matched_raws: usize,
    pub missing_raws: usize,
    pub sidecars: usize,
    pub reclaimable_bytes: u64,
    pub duplicate_reference_keys: usize,
    pub scanned_at_ms: u64,
    pub warnings: Vec<String>,
    pub items: Vec<ScanItem>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteCandidate {
    pub relative_path: String,
    pub expected_size_bytes: u64,
    pub expected_modified_ms: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteRequest {
    pub plan_id: String,
    pub raw_root: String,
    pub items: Vec<DeleteCandidate>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteResult {
    pub relative_path: String,
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSummary {
    pub succeeded: usize,
    pub failed: usize,
    pub results: Vec<DeleteResult>,
    pub log_path: Option<String>,
    pub log_warning: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OperationLogRecord<'a> {
    timestamp: String,
    raw_root: &'a str,
    relative_path: &'a str,
    success: bool,
    message: &'a str,
}

#[derive(Default)]
struct DeletionPlanStore {
    current: Mutex<Option<DeletionPlan>>,
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

fn normalize_extensions(values: &[String], label: &str) -> Result<HashSet<String>, String> {
    let extensions: HashSet<String> = values
        .iter()
        .map(|value| value.trim().trim_start_matches('.').to_lowercase())
        .filter(|value| !value.is_empty())
        .collect();

    if extensions.is_empty() {
        return Err(format!("{label}不能为空"));
    }
    if extensions.iter().any(|value| {
        !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    }) {
        return Err(format!("{label}只能包含字母和数字"));
    }
    Ok(extensions)
}

fn extension_of(path: &Path) -> String {
    path.extension()
        .map(|extension| extension.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

fn display_relative(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn match_key(relative: &Path, case_sensitive: bool) -> String {
    let key = display_relative(&relative.with_extension(""));
    if case_sensitive {
        key
    } else {
        key.to_lowercase()
    }
}

fn collect_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for entry in WalkDir::new(root).follow_links(false) {
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
    status: ScanStatus,
    kind: FileKind,
    matched_reference: Option<String>,
) -> Result<ScanItem, String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| "扫描结果超出了源目录".to_string())?;
    let metadata = fs::metadata(path).map_err(|error| format!("读取文件信息失败：{error}"))?;
    let relative_path = display_relative(relative);
    let prefix = match kind {
        FileKind::Raw => "raw",
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
        status,
        kind,
        matched_reference,
    })
}

pub fn scan_pairs_impl(request: &ScanRequest) -> Result<ScanSummary, String> {
    let reference_root = canonical_directory(&request.reference_root, "JPG 参考目录")?;
    let raw_root = canonical_directory(&request.raw_root, "RAW 源目录")?;
    if reference_root.starts_with(&raw_root) || raw_root.starts_with(&reference_root) {
        return Err("JPG 参考目录与 RAW 源目录不能相同或互相嵌套".to_string());
    }

    let reference_extensions =
        normalize_extensions(&request.reference_extensions, "参考文件扩展名")?;
    let raw_extensions = normalize_extensions(&request.raw_extensions, "RAW 扩展名")?;
    let sidecar_extensions = normalize_extensions(&request.sidecar_extensions, "伴随文件扩展名")?;

    let mut references: HashMap<String, Vec<String>> = HashMap::new();
    let mut reference_files = 0usize;
    for path in collect_files(&reference_root)? {
        if !reference_extensions.contains(&extension_of(&path)) {
            continue;
        }
        let relative = path
            .strip_prefix(&reference_root)
            .map_err(|_| "参考文件超出了参考目录".to_string())?;
        references
            .entry(match_key(relative, request.case_sensitive))
            .or_default()
            .push(display_relative(relative));
        reference_files += 1;
    }

    let duplicate_reference_keys = references.values().filter(|paths| paths.len() > 1).count();
    let mut raw_paths = Vec::new();
    let mut sidecars: HashMap<String, Vec<PathBuf>> = HashMap::new();
    for path in collect_files(&raw_root)? {
        let extension = extension_of(&path);
        let relative = path
            .strip_prefix(&raw_root)
            .map_err(|_| "RAW 文件超出了源目录".to_string())?;
        if raw_extensions.contains(&extension) {
            raw_paths.push(path);
        } else if sidecar_extensions.contains(&extension) {
            sidecars
                .entry(match_key(relative, request.case_sensitive))
                .or_default()
                .push(path);
        }
    }

    let mut items = Vec::new();
    let mut missing_keys = Vec::new();
    let mut matched_raws = 0usize;
    let mut missing_raws = 0usize;
    let mut reclaimable_bytes = 0u64;

    for path in &raw_paths {
        let relative = path
            .strip_prefix(&raw_root)
            .map_err(|_| "RAW 文件超出了源目录".to_string())?;
        let key = match_key(relative, request.case_sensitive);
        let matched_reference = references
            .get(&key)
            .and_then(|paths| paths.first())
            .cloned();
        let status = if matched_reference.is_some() {
            matched_raws += 1;
            ScanStatus::Keep
        } else {
            missing_raws += 1;
            missing_keys.push(key);
            ScanStatus::Delete
        };
        let item = scan_item(&raw_root, path, status, FileKind::Raw, matched_reference)?;
        if status == ScanStatus::Delete {
            reclaimable_bytes = reclaimable_bytes.saturating_add(item.size_bytes);
        }
        items.push(item);
    }

    let mut sidecar_count = 0usize;
    for key in unique_keys(missing_keys) {
        if let Some(paths) = sidecars.get(&key) {
            for path in paths {
                let item = scan_item(&raw_root, path, ScanStatus::Delete, FileKind::Sidecar, None)?;
                reclaimable_bytes = reclaimable_bytes.saturating_add(item.size_bytes);
                sidecar_count += 1;
                items.push(item);
            }
        }
    }

    items.sort_by(|left, right| {
        let left_rank = if left.status == ScanStatus::Delete {
            0
        } else {
            1
        };
        let right_rank = if right.status == ScanStatus::Delete {
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
    if duplicate_reference_keys > 0 {
        warnings.push(format!(
            "参考目录中有 {duplicate_reference_keys} 组重复匹配键，请在执行前核对"
        ));
    }

    Ok(ScanSummary {
        plan_id: String::new(),
        reference_files,
        raw_files: raw_paths.len(),
        matched_raws,
        missing_raws,
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
    candidate: &DeleteCandidate,
) -> Result<PathBuf, String> {
    let relative = safe_relative_path(&candidate.relative_path)?;
    let extension = extension_of(&relative);
    if !matches!(extension.as_str(), "nef" | "xmp") {
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
    if let Some(expected) = candidate.expected_modified_ms {
        if modified_ms(&metadata) != Some(expected) {
            return Err("文件修改时间在扫描后发生变化，请重新扫描".to_string());
        }
    }
    Ok(path)
}

fn write_operation_log(
    log_dir: Option<&Path>,
    raw_root: &str,
    results: &[DeleteResult],
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

fn delete_impl(
    request: &DeleteRequest,
    log_dir: Option<&Path>,
    plan: &DeletionPlan,
) -> Result<DeleteSummary, String> {
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
            Ok(path) => trash::delete(&path)
                .map(|_| "已移入系统回收站/废纸篓".to_string())
                .map_err(|error| format!("移入系统回收站/废纸篓失败：{error}")),
            Err(error) => Err(error),
        };
        match outcome {
            Ok(message) => results.push(DeleteResult {
                relative_path: candidate.relative_path.clone(),
                success: true,
                message,
            }),
            Err(message) => results.push(DeleteResult {
                relative_path: candidate.relative_path.clone(),
                success: false,
                message,
            }),
        }
    }

    let succeeded = results.iter().filter(|result| result.success).count();
    let failed = results.len().saturating_sub(succeeded);
    let (log_path, log_warning) = write_operation_log(log_dir, &request.raw_root, &results);
    Ok(DeleteSummary {
        succeeded,
        failed,
        results,
        log_path,
        log_warning,
    })
}

#[tauri::command]
async fn scan_pairs(
    state: tauri::State<'_, DeletionPlanStore>,
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
        .filter(|item| item.status == ScanStatus::Delete)
        .map(|item| {
            (
                item.relative_path.clone(),
                FileSnapshot::new(item.size_bytes, item.modified_ms),
            )
        });
    let plan = DeletionPlan::new(plan_id.clone(), raw_root, candidates);
    summary.plan_id = plan_id;
    *state
        .current
        .lock()
        .map_err(|_| "无法保存清理计划状态".to_string())? = Some(plan);
    Ok(summary)
}

#[tauri::command]
async fn move_to_trash(
    app: tauri::AppHandle,
    state: tauri::State<'_, DeletionPlanStore>,
    request: DeleteRequest,
) -> Result<DeleteSummary, String> {
    if request.items.is_empty() {
        return Err("清理计划中没有文件".to_string());
    }
    let raw_root = canonical_directory(&request.raw_root, "RAW 源目录")?;
    let plan = {
        let mut current = state
            .current
            .lock()
            .map_err(|_| "无法读取清理计划状态".to_string())?;
        let plan = current
            .as_ref()
            .ok_or_else(|| "清理计划不存在，请重新扫描".to_string())?;
        if !plan.matches(&request.plan_id, &raw_root) {
            return Err("清理计划已失效或 RAW 源目录不匹配，请重新扫描".to_string());
        }
        current
            .take()
            .ok_or_else(|| "清理计划不存在，请重新扫描".to_string())?
    };
    let log_dir = app.path().app_log_dir().ok();
    tauri::async_runtime::spawn_blocking(move || delete_impl(&request, log_dir.as_deref(), &plan))
        .await
        .map_err(|error| format!("清理任务异常结束：{error}"))?
}

#[tauri::command]
async fn reveal_scan_item(raw_root: String, relative_path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = resolve_scan_item_path(&raw_root, &relative_path)?;
        reveal_path(&path)
    })
    .await
    .map_err(|error| format!("定位文件任务异常结束：{error}"))?
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
        .manage(DeletionPlanStore::default())
        .invoke_handler(tauri::generate_handler![
            scan_pairs,
            move_to_trash,
            reveal_scan_item,
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
            reference_root: reference_root.to_string_lossy().into_owned(),
            raw_root: raw_root.to_string_lossy().into_owned(),
            reference_extensions: vec!["jpg".to_string(), "jpeg".to_string()],
            raw_extensions: vec!["nef".to_string()],
            sidecar_extensions: vec!["xmp".to_string()],
            case_sensitive: false,
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
        assert_eq!(summary.matched_raws, 1);
        assert_eq!(summary.missing_raws, 1);
        assert_eq!(summary.sidecars, 1);
        assert_eq!(summary.items.len(), 3);
        assert!(summary.items.iter().any(|item| {
            item.relative_path == "20260712/DSC_0002.NEF"
                && item.status == ScanStatus::Delete
                && item.kind == FileKind::Raw
        }));
        assert!(summary.items.iter().any(|item| {
            item.relative_path == "20260712/DSC_0002.xmp"
                && item.status == ScanStatus::Delete
                && item.kind == FileKind::Sidecar
        }));
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
    fn exposes_each_sidecar_once_for_repeated_missing_match_keys() {
        let temp = tempfile::tempdir().expect("temp directory");
        let reference_root = temp.path().join("JPG");
        let raw_root = temp.path().join("RAW");
        fs::create_dir_all(&reference_root).expect("reference directory");
        fs::create_dir_all(&raw_root).expect("raw directory");

        fs::write(raw_root.join("DSC_0001.NEF"), b"nef").expect("nef");
        fs::write(raw_root.join("DSC_0001.RAW"), b"raw").expect("raw");
        fs::write(raw_root.join("DSC_0001.xmp"), b"xmp").expect("xmp");

        let mut scan_request = request(&reference_root, &raw_root);
        scan_request.raw_extensions.push("raw".to_string());
        let summary = scan_pairs_impl(&scan_request).expect("scan");

        assert_eq!(summary.missing_raws, 2);
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
        let candidate = DeleteCandidate {
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
}
