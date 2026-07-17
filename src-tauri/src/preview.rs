use crate::formats;
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, GenericImageView, ImageDecoder, ImageReader};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

const QUARANTINE_DIR: &str = ".framepair-quarantine";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PhotoAsset {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) relative_stem: String,
    pub(crate) preview_path: Option<String>,
    pub(crate) jpeg_paths: Vec<String>,
    pub(crate) raw_paths: Vec<String>,
    pub(crate) extensions: Vec<String>,
    pub(crate) size_bytes: u64,
    pub(crate) modified_ms: Option<u64>,
    pub(crate) rating: u8,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PhotoIndex {
    pub(crate) root: String,
    pub(crate) indexed_at_ms: u64,
    pub(crate) total_assets: usize,
    pub(crate) paired_assets: usize,
    pub(crate) previewable_assets: usize,
    pub(crate) raw_only_assets: usize,
    pub(crate) assets: Vec<PhotoAsset>,
}

#[derive(Default)]
struct PhotoAssetBuilder {
    relative_stem: String,
    jpeg_paths: Vec<String>,
    raw_paths: Vec<String>,
    size_bytes: u64,
    modified_ms: Option<u64>,
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

fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn extension_label(path: &str) -> String {
    Path::new(path)
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_uppercase())
        .unwrap_or_default()
}

fn finalize_asset(key: String, mut builder: PhotoAssetBuilder) -> PhotoAsset {
    builder.jpeg_paths.sort_by_key(|path| path.to_lowercase());
    builder.raw_paths.sort_by_key(|path| path.to_lowercase());

    let mut extensions = Vec::new();
    for path in builder.jpeg_paths.iter().chain(&builder.raw_paths) {
        let extension = extension_label(path);
        if !extension.is_empty() && !extensions.contains(&extension) {
            extensions.push(extension);
        }
    }

    let name = Path::new(&builder.relative_stem)
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| builder.relative_stem.clone());

    PhotoAsset {
        id: key,
        name,
        relative_stem: builder.relative_stem,
        preview_path: builder.jpeg_paths.first().cloned(),
        jpeg_paths: builder.jpeg_paths,
        raw_paths: builder.raw_paths,
        extensions,
        size_bytes: builder.size_bytes,
        modified_ms: builder.modified_ms,
        rating: 0,
    }
}

pub(crate) fn index_directory(root: &Path) -> Result<PhotoIndex, String> {
    let root = fs::canonicalize(root).map_err(|error| format!("照片目录不可访问：{error}"))?;
    if !root.is_dir() {
        return Err("照片目录不是文件夹".to_string());
    }

    let mut groups = BTreeMap::<String, PhotoAssetBuilder>::new();
    for entry in WalkDir::new(&root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| entry.depth() != 1 || entry.file_name() != QUARANTINE_DIR)
    {
        let entry = entry.map_err(|error| format!("读取照片目录失败：{error}"))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let is_jpeg = formats::is_reference(path);
        let is_raw = formats::is_raw(path);
        if !is_jpeg && !is_raw {
            continue;
        }

        let relative = path
            .strip_prefix(&root)
            .map_err(|_| "照片索引超出了所选目录".to_string())?;
        let relative_path = display_path(relative);
        let relative_stem = display_path(&relative.with_extension(""));
        let key = relative_stem.to_lowercase();
        let metadata = fs::metadata(path).map_err(|error| format!("读取照片信息失败：{error}"))?;
        let builder = groups.entry(key).or_insert_with(|| PhotoAssetBuilder {
            relative_stem,
            ..PhotoAssetBuilder::default()
        });
        if is_jpeg {
            builder.jpeg_paths.push(relative_path);
        } else {
            builder.raw_paths.push(relative_path);
        }
        builder.size_bytes = builder.size_bytes.saturating_add(metadata.len());
        builder.modified_ms = match (builder.modified_ms, modified_ms(&metadata)) {
            (Some(current), Some(next)) => Some(current.max(next)),
            (None, next) => next,
            (current, None) => current,
        };
    }

    let assets = groups
        .into_iter()
        .map(|(key, builder)| finalize_asset(key, builder))
        .collect::<Vec<_>>();
    let paired_assets = assets
        .iter()
        .filter(|asset| !asset.jpeg_paths.is_empty() && !asset.raw_paths.is_empty())
        .count();
    let previewable_assets = assets
        .iter()
        .filter(|asset| asset.preview_path.is_some())
        .count();
    let raw_only_assets = assets
        .iter()
        .filter(|asset| asset.jpeg_paths.is_empty() && !asset.raw_paths.is_empty())
        .count();

    Ok(PhotoIndex {
        root: display_path(&root),
        indexed_at_ms: now_ms(),
        total_assets: assets.len(),
        paired_assets,
        previewable_assets,
        raw_only_assets,
        assets,
    })
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

pub(crate) fn load_thumbnail(
    root: &Path,
    relative_path: &str,
    max_edge: u32,
    cache_root: &Path,
) -> Result<Vec<u8>, String> {
    if !(96..=2048).contains(&max_edge) {
        return Err("缩略图尺寸必须在 96 到 2048 像素之间".to_string());
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
        return fs::read(&cache_path).map_err(|error| format!("无法读取缩略图缓存：{error}"));
    }

    let reader = ImageReader::open(&source)
        .map_err(|error| format!("无法打开 JPG 预览：{error}"))?
        .with_guessed_format()
        .map_err(|error| format!("无法识别 JPG 预览：{error}"))?;
    let mut decoder = reader
        .into_decoder()
        .map_err(|error| format!("无法创建 JPG 解码器：{error}"))?;
    let orientation = decoder
        .orientation()
        .map_err(|error| format!("无法读取 JPG 方向：{error}"))?;
    let mut image = DynamicImage::from_decoder(decoder)
        .map_err(|error| format!("无法解码 JPG 预览：{error}"))?;
    image.apply_orientation(orientation);
    let (width, height) = image.dimensions();
    let thumbnail = if width > max_edge || height > max_edge {
        image.thumbnail(max_edge, max_edge)
    } else {
        image
    };
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, 84)
        .encode_image(&thumbnail)
        .map_err(|error| format!("无法生成 JPG 缩略图：{error}"))?;

    let temporary_path = temporary_cache_path(&cache_path);
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
    match fs::rename(&temporary_path, &cache_path) {
        Ok(()) => Ok(bytes),
        Err(_) if cache_path.is_file() => {
            let _ = fs::remove_file(&temporary_path);
            fs::read(&cache_path).map_err(|error| format!("无法读取缩略图缓存：{error}"))
        }
        Err(error) => {
            let _ = fs::remove_file(&temporary_path);
            Err(format!("无法完成缩略图缓存：{error}"))
        }
    }
}
