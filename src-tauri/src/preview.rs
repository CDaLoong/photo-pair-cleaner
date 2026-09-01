use crate::fs_util::{self, modified_ms, now_ms};
use crate::{formats, preview_cache};
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::metadata::Orientation;
use image::{DynamicImage, GenericImageView, ImageDecoder, ImageEncoder, ImageReader};
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

const THUMBNAIL_CACHE_VERSION: u8 = 4;
const PREVIEW_CACHE_BUDGET_BYTES: u64 = 10 * 1024 * 1024 * 1024;
const PREVIEW_CACHE_MAX_ENTRIES: usize = 100_000;
pub(crate) use crate::preview_cache::PreviewCacheStats;

static THUMBNAIL_GENERATION_LOCKS: LazyLock<Vec<Mutex<()>>> =
    LazyLock::new(|| (0..64).map(|_| Mutex::new(())).collect());

fn safe_relative_path(value: &str) -> Result<PathBuf, String> {
    fs_util::safe_relative_path_str(value, "预览路径")
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

fn thumbnail_cache_key(source: &Path, metadata: &fs::Metadata, max_edge: u32) -> String {
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    metadata.len().hash(&mut hasher);
    modified_ms(metadata).hash(&mut hasher);
    max_edge.hash(&mut hasher);
    THUMBNAIL_CACHE_VERSION.hash(&mut hasher);
    format!("{:016x}.jpg", hasher.finish())
}

pub(crate) fn thumbnail_cache_relative_path(
    source: &Path,
    metadata: &fs::Metadata,
    max_edge: u32,
) -> PathBuf {
    let key = thumbnail_cache_key(source, metadata, max_edge);
    PathBuf::from(&key[0..2]).join(&key[2..4]).join(key)
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

pub(crate) fn cache_stats(cache_root: &Path) -> Result<PreviewCacheStats, String> {
    preview_cache::cache_for(
        cache_root,
        PREVIEW_CACHE_BUDGET_BYTES,
        PREVIEW_CACHE_MAX_ENTRIES,
    )?
    .stats()
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
    let cache_parent = cache_path
        .parent()
        .ok_or_else(|| "缩略图缓存目录无效".to_string())?;
    fs::create_dir_all(cache_parent).map_err(|error| format!("无法创建缩略图缓存分片：{error}"))?;
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
    let cache = preview_cache::cache_for(
        cache_root,
        PREVIEW_CACHE_BUDGET_BYTES,
        PREVIEW_CACHE_MAX_ENTRIES,
    )?;
    let cache_relative = thumbnail_cache_relative_path(&source, &metadata, max_edge);
    let cache_relative_text = cache_relative
        .to_str()
        .ok_or_else(|| "缩略图缓存路径不是有效 UTF-8".to_string())?;
    let cache_path = cache_root.join(&cache_relative);
    let legacy_path = cache_root.join(
        cache_relative
            .file_name()
            .ok_or_else(|| "缩略图缓存文件名无效".to_string())?,
    );
    if !cache_path.is_file() && legacy_path.is_file() {
        let cache_parent = cache_path
            .parent()
            .ok_or_else(|| "缩略图缓存分片目录无效".to_string())?;
        fs::create_dir_all(cache_parent)
            .map_err(|error| format!("无法创建缩略图缓存分片：{error}"))?;
        match fs::rename(&legacy_path, &cache_path) {
            Ok(()) => {}
            Err(_) => {
                fs::copy(&legacy_path, &cache_path)
                    .map_err(|error| format!("无法迁移旧版缩略图缓存：{error}"))?;
                fs::remove_file(&legacy_path)
                    .map_err(|error| format!("无法移除旧版缩略图缓存：{error}"))?;
            }
        }
        if let Some(legacy_name) = legacy_path.file_name().and_then(|value| value.to_str()) {
            cache.remove_missing(legacy_name)?;
        }
    }
    if cache_path.is_file() {
        let bytes =
            fs::read(&cache_path).map_err(|error| format!("无法读取缩略图缓存：{error}"))?;
        cache.record_access(cache_relative_text, bytes.len() as u64, max_edge, now_ms())?;
        return Ok(bytes);
    }

    let generation_lock = &THUMBNAIL_GENERATION_LOCKS[generation_lock_index(&cache_path)];
    let _generation_guard = generation_lock
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if cache_path.is_file() {
        let bytes =
            fs::read(&cache_path).map_err(|error| format!("无法读取缩略图缓存：{error}"))?;
        cache.record_access(cache_relative_text, bytes.len() as u64, max_edge, now_ms())?;
        return Ok(bytes);
    }
    let bytes = decode_thumbnail(&source, max_edge)?;
    let bytes = save_thumbnail(&cache_path, bytes)?;
    cache.record_generated(cache_relative_text, bytes.len() as u64, max_edge, now_ms())?;
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
        let cache_files = walkdir::WalkDir::new(cache.path())
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.path().extension().and_then(|value| value.to_str()) == Some("jpg")
            })
            .collect::<Vec<_>>();
        let stats = cache_stats(cache.path()).expect("cache stats");

        assert_eq!(first, second);
        assert_eq!(cache_files.len(), 1);
        assert!(cache.path().join("preview-cache-v2.sqlite3").is_file());
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
}
