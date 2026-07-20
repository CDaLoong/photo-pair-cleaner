use crate::formats;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::metadata::Orientation;
use image::{DynamicImage, GenericImageView, ImageDecoder, ImageEncoder, ImageReader};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

const THUMBNAIL_CACHE_VERSION: u8 = 3;
const PREVIEW_CACHE_INDEX_VERSION: u8 = 1;
const PREVIEW_CACHE_INDEX_FILE: &str = "preview-cache-index-v1.json";
const PREVIEW_CACHE_BUDGET_BYTES: u64 = 10 * 1024 * 1024 * 1024;
const PREVIEW_CACHE_MAX_ENTRIES: usize = 100_000;
const PREVIEW_CACHE_ACCESS_FLUSH_COUNT: usize = 32;
const PREVIEW_CACHE_ACCESS_FLUSH_MS: u64 = 30_000;
const PREVIEW_CACHE_TARGET_PERCENT: u64 = 90;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviewCacheEntry {
    size_bytes: u64,
    last_access_ms: u64,
    #[serde(default)]
    max_edge: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviewCacheIndex {
    schema_version: u8,
    entries: HashMap<String, PreviewCacheEntry>,
}

impl PreviewCacheIndex {
    fn empty() -> Self {
        Self {
            schema_version: PREVIEW_CACHE_INDEX_VERSION,
            entries: HashMap::new(),
        }
    }
}

struct PreviewCacheState {
    index: PreviewCacheIndex,
    dirty_accesses: usize,
    last_flush_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreviewCacheStats {
    pub(crate) entry_count: usize,
    pub(crate) size_bytes: u64,
    pub(crate) budget_bytes: u64,
    pub(crate) max_entries: usize,
}

static PREVIEW_CACHE_STATES: LazyLock<Mutex<HashMap<PathBuf, PreviewCacheState>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static THUMBNAIL_GENERATION_LOCKS: LazyLock<Vec<Mutex<()>>> =
    LazyLock::new(|| (0..64).map(|_| Mutex::new(())).collect());

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

fn safe_relative_path(value: &str) -> Result<PathBuf, String> {
    let path = Path::new(value);
    if value.trim().is_empty() || path.is_absolute() {
        return Err("预览路径必须是安全相对路径".to_string());
    }
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("预览路径包含不安全片段".to_string());
    }
    Ok(path.to_path_buf())
}

pub(crate) fn resolve_preview_path(root: &Path, relative_path: &str) -> Result<PathBuf, String> {
    let root = fs::canonicalize(root).map_err(|error| format!("照片目录不可访问：{error}"))?;
    if !root.is_dir() {
        return Err("照片目录不是文件夹".to_string());
    }
    let relative = safe_relative_path(relative_path)?;
    if !formats::is_reference(&relative) {
        return Err("当前阶段只为 JPG/JPEG 生成预览".to_string());
    }
    let path = fs::canonicalize(root.join(relative))
        .map_err(|error| format!("预览文件不可访问：{error}"))?;
    if !path.starts_with(&root) || !path.is_file() {
        return Err("预览文件超出了所选目录".to_string());
    }
    Ok(path)
}

fn thumbnail_cache_path(
    source: &Path,
    metadata: &fs::Metadata,
    max_edge: u32,
    cache_root: &Path,
) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    metadata.len().hash(&mut hasher);
    modified_ms(metadata).hash(&mut hasher);
    max_edge.hash(&mut hasher);
    THUMBNAIL_CACHE_VERSION.hash(&mut hasher);
    cache_root.join(format!("{:016x}.jpg", hasher.finish()))
}

pub(crate) fn temporary_cache_path(cache_path: &Path) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    cache_path.with_file_name(format!(
        ".{}-{}-{sequence}.tmp",
        cache_path
            .file_stem()
            .map(|value| value.to_string_lossy())
            .unwrap_or_default(),
        std::process::id(),
    ))
}

fn cache_index_path(cache_root: &Path) -> PathBuf {
    cache_root.join(PREVIEW_CACHE_INDEX_FILE)
}

fn cache_file_name(cache_path: &Path) -> Result<String, String> {
    cache_path
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "缩略图缓存文件名无效".to_string())
}

fn load_cache_state(cache_root: &Path) -> Result<PreviewCacheState, String> {
    let index_path = cache_index_path(cache_root);
    let mut index = fs::read(&index_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<PreviewCacheIndex>(&bytes).ok())
        .filter(|index| index.schema_version == PREVIEW_CACHE_INDEX_VERSION)
        .unwrap_or_else(PreviewCacheIndex::empty);
    let mut discovered = HashSet::new();

    for item in
        fs::read_dir(cache_root).map_err(|error| format!("无法扫描缩略图缓存目录：{error}"))?
    {
        let item = item.map_err(|error| format!("无法读取缩略图缓存项：{error}"))?;
        let file_type = item
            .file_type()
            .map_err(|error| format!("无法读取缩略图缓存类型：{error}"))?;
        if !file_type.is_file()
            || item.path().extension().and_then(|value| value.to_str()) != Some("jpg")
        {
            continue;
        }
        let Some(name) = item.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let metadata = item
            .metadata()
            .map_err(|error| format!("无法读取缩略图缓存信息：{error}"))?;
        discovered.insert(name.clone());
        index
            .entries
            .entry(name)
            .and_modify(|entry| entry.size_bytes = metadata.len())
            .or_insert_with(|| PreviewCacheEntry {
                size_bytes: metadata.len(),
                last_access_ms: modified_ms(&metadata).unwrap_or_default(),
                max_edge: 0,
            });
    }
    index.entries.retain(|name, _| discovered.contains(name));

    Ok(PreviewCacheState {
        index,
        dirty_accesses: 1,
        last_flush_ms: 0,
    })
}

fn save_cache_index(cache_root: &Path, index: &PreviewCacheIndex) -> Result<(), String> {
    let index_path = cache_index_path(cache_root);
    let temporary_path = temporary_cache_path(&index_path);
    let bytes =
        serde_json::to_vec(index).map_err(|error| format!("无法序列化缩略图缓存索引：{error}"))?;
    let mut temporary = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary_path)
        .map_err(|error| format!("无法写入缩略图缓存索引：{error}"))?;
    if let Err(error) = temporary
        .write_all(&bytes)
        .and_then(|_| temporary.sync_all())
    {
        let _ = fs::remove_file(&temporary_path);
        return Err(format!("无法保存缩略图缓存索引：{error}"));
    }
    drop(temporary);
    #[cfg(windows)]
    if index_path.is_file() {
        if let Err(error) = fs::remove_file(&index_path) {
            let _ = fs::remove_file(&temporary_path);
            return Err(format!("无法替换缩略图缓存索引：{error}"));
        }
    }
    if let Err(error) = fs::rename(&temporary_path, &index_path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(format!("无法完成缩略图缓存索引：{error}"));
    }
    Ok(())
}

fn prune_cache_to_limits(
    cache_root: &Path,
    index: &mut PreviewCacheIndex,
    budget_bytes: u64,
    max_entries: usize,
    protected_name: Option<&str>,
) -> bool {
    let budget_bytes = budget_bytes.max(1);
    let max_entries = max_entries.max(1);
    let mut size_bytes = index
        .entries
        .values()
        .map(|entry| entry.size_bytes)
        .sum::<u64>();
    if size_bytes <= budget_bytes && index.entries.len() <= max_entries {
        return false;
    }

    let target_bytes = budget_bytes
        .saturating_mul(PREVIEW_CACHE_TARGET_PERCENT)
        .checked_div(100)
        .unwrap_or(budget_bytes)
        .max(1);
    let target_entries = max_entries
        .saturating_mul(PREVIEW_CACHE_TARGET_PERCENT as usize)
        .checked_div(100)
        .unwrap_or(max_entries)
        .max(1);
    let mut candidates = index
        .entries
        .iter()
        .map(|(name, entry)| (name.clone(), entry.last_access_ms, entry.size_bytes))
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)));
    let mut changed = false;

    for (name, _, entry_size) in candidates {
        if size_bytes <= target_bytes && index.entries.len() <= target_entries {
            break;
        }
        if protected_name == Some(name.as_str()) {
            continue;
        }
        let path = cache_root.join(&name);
        match fs::remove_file(path) {
            Ok(()) => {
                index.entries.remove(&name);
                size_bytes = size_bytes.saturating_sub(entry_size);
                changed = true;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                index.entries.remove(&name);
                size_bytes = size_bytes.saturating_sub(entry_size);
                changed = true;
            }
            Err(_) => {}
        }
    }
    changed
}

fn record_cache_access(
    cache_root: &Path,
    cache_path: &Path,
    max_edge: u32,
    persist_now: bool,
) -> Result<(), String> {
    let name = cache_file_name(cache_path)?;
    let size_bytes = fs::metadata(cache_path)
        .map_err(|error| format!("无法读取缩略图缓存信息：{error}"))?
        .len();
    let timestamp = now_ms();
    let mut states = PREVIEW_CACHE_STATES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !states.contains_key(cache_root) {
        states.insert(cache_root.to_path_buf(), load_cache_state(cache_root)?);
    }
    let state = states.get_mut(cache_root).expect("cache state inserted");
    state.index.entries.insert(
        name.clone(),
        PreviewCacheEntry {
            size_bytes,
            last_access_ms: timestamp,
            max_edge,
        },
    );
    state.dirty_accesses = state.dirty_accesses.saturating_add(1);
    let pruned = prune_cache_to_limits(
        cache_root,
        &mut state.index,
        PREVIEW_CACHE_BUDGET_BYTES,
        PREVIEW_CACHE_MAX_ENTRIES,
        Some(&name),
    );
    let should_flush = persist_now
        || pruned
        || state.dirty_accesses >= PREVIEW_CACHE_ACCESS_FLUSH_COUNT
        || timestamp.saturating_sub(state.last_flush_ms) >= PREVIEW_CACHE_ACCESS_FLUSH_MS;
    if should_flush {
        save_cache_index(cache_root, &state.index)?;
        state.dirty_accesses = 0;
        state.last_flush_ms = timestamp;
    }
    Ok(())
}

pub(crate) fn cache_stats(cache_root: &Path) -> Result<PreviewCacheStats, String> {
    fs::create_dir_all(cache_root).map_err(|error| format!("无法创建缩略图缓存目录：{error}"))?;
    let mut states = PREVIEW_CACHE_STATES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !states.contains_key(cache_root) {
        states.insert(cache_root.to_path_buf(), load_cache_state(cache_root)?);
    }
    let state = states.get_mut(cache_root).expect("cache state inserted");
    let pruned = prune_cache_to_limits(
        cache_root,
        &mut state.index,
        PREVIEW_CACHE_BUDGET_BYTES,
        PREVIEW_CACHE_MAX_ENTRIES,
        None,
    );
    if pruned || state.dirty_accesses > 0 {
        save_cache_index(cache_root, &state.index)?;
        state.dirty_accesses = 0;
        state.last_flush_ms = now_ms();
    }
    Ok(PreviewCacheStats {
        entry_count: state.index.entries.len(),
        size_bytes: state
            .index
            .entries
            .values()
            .map(|entry| entry.size_bytes)
            .sum(),
        budget_bytes: PREVIEW_CACHE_BUDGET_BYTES,
        max_entries: PREVIEW_CACHE_MAX_ENTRIES,
    })
}

fn generation_lock_index(cache_path: &Path) -> usize {
    let mut hasher = DefaultHasher::new();
    cache_path.hash(&mut hasher);
    hasher.finish() as usize % THUMBNAIL_GENERATION_LOCKS.len()
}

#[cfg(target_os = "macos")]
fn load_macos_thumbnail(source: &Path, max_edge: u32) -> Result<Vec<u8>, String> {
    let reader = ImageReader::open(source)
        .map_err(|error| format!("无法打开 JPG 预览：{error}"))?
        .with_guessed_format()
        .map_err(|error| format!("无法识别 JPG 格式：{error}"))?;
    let mut decoder = reader
        .into_decoder()
        .map_err(|error| format!("无法创建 JPG 解码器：{error}"))?;
    let orientation = decoder
        .orientation()
        .map_err(|error| format!("无法读取 JPG 方向：{error}"))?;
    if orientation != Orientation::NoTransforms {
        return Err("macOS 快速预览不处理带旋转标记的 JPG".to_string());
    }
    let (width, height) = decoder.dimensions();
    let target_edge = max_edge.min(width.max(height));
    let temporary = tempfile::Builder::new()
        .prefix("framepair-preview-")
        .suffix(".jpg")
        .tempfile()
        .map_err(|error| format!("无法创建系统预览临时文件：{error}"))?;
    let quality = if max_edge >= 1024 { "95" } else { "90" };
    let output = Command::new("/usr/bin/sips")
        .arg("-Z")
        .arg(target_edge.to_string())
        .args(["-s", "format", "jpeg", "-s", "formatOptions", quality])
        .arg(source)
        .arg("--out")
        .arg(temporary.path())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| format!("无法启动 macOS 图像预览服务：{error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if detail.is_empty() {
            "macOS 图像预览服务生成缩略图失败".to_string()
        } else {
            format!("macOS 图像预览服务生成缩略图失败：{detail}")
        });
    }
    let bytes =
        fs::read(temporary.path()).map_err(|error| format!("无法读取 macOS 系统预览：{error}"))?;
    if !bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Err("macOS 系统预览没有返回有效 JPEG".to_string());
    }
    Ok(bytes)
}

fn decode_thumbnail(source: &Path, max_edge: u32) -> Result<Vec<u8>, String> {
    #[cfg(windows)]
    if let Ok(bytes) = crate::windows_thumbnail::load_system_thumbnail(source, max_edge) {
        return Ok(bytes);
    }

    #[cfg(target_os = "macos")]
    if let Ok(bytes) = load_macos_thumbnail(source, max_edge) {
        return Ok(bytes);
    }

    let reader = ImageReader::open(source)
        .map_err(|error| format!("无法打开 JPG 预览：{error}"))?
        .with_guessed_format()
        .map_err(|error| format!("无法识别 JPG 预览：{error}"))?;
    let mut decoder = reader
        .into_decoder()
        .map_err(|error| format!("无法创建 JPG 解码器：{error}"))?;
    let icc_profile = decoder
        .icc_profile()
        .map_err(|error| format!("无法读取 JPG 色彩配置：{error}"))?;
    let orientation = decoder
        .orientation()
        .map_err(|error| format!("无法读取 JPG 方向：{error}"))?;
    let mut image = DynamicImage::from_decoder(decoder)
        .map_err(|error| format!("无法解码 JPG 预览：{error}"))?;
    image.apply_orientation(orientation);
    let (width, height) = image.dimensions();
    let thumbnail = if width > max_edge || height > max_edge {
        image.resize(max_edge, max_edge, FilterType::Lanczos3)
    } else {
        image
    };
    let mut bytes = Vec::new();
    let quality = if max_edge >= 1024 { 95 } else { 90 };
    let mut encoder = JpegEncoder::new_with_quality(&mut bytes, quality);
    if let Some(profile) = icc_profile {
        encoder
            .set_icc_profile(profile)
            .map_err(|error| format!("无法保留 JPG 色彩配置：{error}"))?;
    }
    encoder
        .encode_image(&thumbnail)
        .map_err(|error| format!("无法生成 JPG 缩略图：{error}"))?;
    Ok(bytes)
}

fn save_thumbnail(cache_path: &Path, bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    let temporary_path = temporary_cache_path(cache_path);
    let mut temporary = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary_path)
        .map_err(|error| format!("无法写入缩略图缓存：{error}"))?;
    if let Err(error) = temporary
        .write_all(&bytes)
        .and_then(|_| temporary.sync_all())
    {
        let _ = fs::remove_file(&temporary_path);
        return Err(format!("无法保存缩略图缓存：{error}"));
    }
    drop(temporary);
    match fs::rename(&temporary_path, cache_path) {
        Ok(()) => Ok(bytes),
        Err(_) if cache_path.is_file() => {
            let _ = fs::remove_file(&temporary_path);
            fs::read(cache_path).map_err(|error| format!("无法读取缩略图缓存：{error}"))
        }
        Err(error) => {
            let _ = fs::remove_file(&temporary_path);
            Err(format!("无法完成缩略图缓存：{error}"))
        }
    }
}

pub(crate) fn load_thumbnail(
    root: &Path,
    relative_path: &str,
    max_edge: u32,
    cache_root: &Path,
) -> Result<Vec<u8>, String> {
    if !(96..=4096).contains(&max_edge) {
        return Err("缩略图尺寸必须在 96 到 4096 像素之间".to_string());
    }

    let source = resolve_preview_path(root, relative_path)?;
    let metadata =
        fs::metadata(&source).map_err(|error| format!("无法读取预览文件信息：{error}"))?;
    if let Ok(cache_metadata) = fs::symlink_metadata(cache_root)
        && cache_metadata.file_type().is_symlink()
    {
        return Err("缩略图缓存目录不能是符号链接".to_string());
    }
    fs::create_dir_all(cache_root).map_err(|error| format!("无法创建缩略图缓存目录：{error}"))?;
    let cache_path = thumbnail_cache_path(&source, &metadata, max_edge, cache_root);
    if cache_path.is_file() {
        let bytes =
            fs::read(&cache_path).map_err(|error| format!("无法读取缩略图缓存：{error}"))?;
        let _ = record_cache_access(cache_root, &cache_path, max_edge, false);
        return Ok(bytes);
    }

    let generation_lock = &THUMBNAIL_GENERATION_LOCKS[generation_lock_index(&cache_path)];
    let _generation_guard = generation_lock
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if cache_path.is_file() {
        let bytes =
            fs::read(&cache_path).map_err(|error| format!("无法读取缩略图缓存：{error}"))?;
        let _ = record_cache_access(cache_root, &cache_path, max_edge, false);
        return Ok(bytes);
    }
    let bytes = decode_thumbnail(&source, max_edge)?;
    let bytes = save_thumbnail(&cache_path, bytes)?;
    let _ = record_cache_access(cache_root, &cache_path, max_edge, true);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, Rgb, RgbImage};

    #[test]
    fn thumbnail_pipeline_writes_and_reuses_a_bounded_cache_entry() {
        let root = tempfile::tempdir().expect("photo root");
        let cache = tempfile::tempdir().expect("cache root");
        let source = root.path().join("sample.jpg");
        let image = RgbImage::from_fn(1200, 800, |x, y| {
            Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8])
        });
        image
            .save_with_format(&source, ImageFormat::Jpeg)
            .expect("write jpeg fixture");

        let first = load_thumbnail(root.path(), "sample.jpg", 256, cache.path())
            .expect("generate thumbnail");
        let second =
            load_thumbnail(root.path(), "sample.jpg", 256, cache.path()).expect("reuse thumbnail");
        let thumbnail = image::load_from_memory(&first).expect("decode thumbnail");
        let cache_files = fs::read_dir(cache.path())
            .expect("read cache")
            .collect::<Result<Vec<_>, _>>()
            .expect("cache entries")
            .into_iter()
            .filter(|entry| {
                entry.path().extension().and_then(|value| value.to_str()) == Some("jpg")
            })
            .collect::<Vec<_>>();
        let stats = cache_stats(cache.path()).expect("cache stats");

        assert_eq!(first, second);
        assert_eq!(cache_files.len(), 1);
        assert!(cache_index_path(cache.path()).is_file());
        assert_eq!(stats.entry_count, 1);
        assert_eq!(
            stats.size_bytes,
            fs::metadata(cache_files[0].path()).unwrap().len()
        );
        assert!(thumbnail.width() <= 256);
        assert!(thumbnail.height() <= 256);
    }

    #[test]
    fn thumbnail_pipeline_supports_high_resolution_previews_within_the_limit() {
        let root = tempfile::tempdir().expect("photo root");
        let cache = tempfile::tempdir().expect("cache root");
        let source = root.path().join("sample.jpg");
        RgbImage::from_pixel(1200, 800, Rgb([40, 80, 120]))
            .save_with_format(&source, ImageFormat::Jpeg)
            .expect("write jpeg fixture");

        let bytes = load_thumbnail(root.path(), "sample.jpg", 2560, cache.path())
            .expect("generate high-resolution preview");
        let preview = image::load_from_memory(&bytes).expect("decode high-resolution preview");

        assert_eq!(preview.dimensions(), (1200, 800));
        assert!(load_thumbnail(root.path(), "sample.jpg", 4097, cache.path()).is_err());
    }

    #[test]
    fn cache_pruning_uses_lru_order_and_protects_the_current_preview() {
        let cache = tempfile::tempdir().expect("cache root");
        let mut index = PreviewCacheIndex::empty();
        for (name, last_access_ms) in [("a.jpg", 1), ("b.jpg", 2), ("c.jpg", 3), ("d.jpg", 4)] {
            fs::write(cache.path().join(name), [0_u8; 4]).expect("cache fixture");
            index.entries.insert(
                name.to_string(),
                PreviewCacheEntry {
                    size_bytes: 4,
                    last_access_ms,
                    max_edge: 512,
                },
            );
        }

        assert!(prune_cache_to_limits(
            cache.path(),
            &mut index,
            12,
            4,
            Some("b.jpg"),
        ));
        assert_eq!(
            index.entries.keys().cloned().collect::<HashSet<_>>(),
            HashSet::from(["b.jpg".to_string(), "d.jpg".to_string(),])
        );
        assert!(cache.path().join("b.jpg").is_file());
        assert!(cache.path().join("d.jpg").is_file());
        assert!(!cache.path().join("a.jpg").exists());
        assert!(!cache.path().join("c.jpg").exists());
    }
}
