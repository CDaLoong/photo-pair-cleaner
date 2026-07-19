use crate::formats;
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, GenericImageView, ImageDecoder, ImageReader};
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::UNIX_EPOCH;

const THUMBNAIL_CACHE_VERSION: u8 = 2;

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

fn decode_thumbnail(source: &Path, max_edge: u32) -> Result<Vec<u8>, String> {
    #[cfg(windows)]
    if let Ok(bytes) = crate::windows_thumbnail::load_system_thumbnail(source, max_edge) {
        return Ok(bytes);
    }

    let reader = ImageReader::open(source)
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

    let bytes = decode_thumbnail(&source, max_edge)?;
    save_thumbnail(&cache_path, bytes)
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
            .expect("cache entries");

        assert_eq!(first, second);
        assert_eq!(cache_files.len(), 1);
        assert!(thumbnail.width() <= 256);
        assert!(thumbnail.height() <= 256);
    }
}
