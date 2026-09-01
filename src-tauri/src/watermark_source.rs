use crate::formats;
use crate::fs_util::{self, reject_symlink};
use crate::watermark_model::{
    WatermarkOrientation, WatermarkSourceOrigin, WatermarkSourcePhoto, WatermarkSourceSnapshot,
};
use image::metadata::Orientation;
use image::{ImageDecoder, ImageReader};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum SourceInput {
    Directory {
        path: String,
    },
    File {
        path: String,
    },
    RelativePaths {
        root: String,
        relative_paths: Vec<String>,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WatermarkSourceRequest {
    pub(crate) origin: WatermarkSourceOrigin,
    pub(crate) inputs: Vec<SourceInput>,
}

#[derive(Debug)]
struct SourceCandidate {
    root: PathBuf,
    path: PathBuf,
    relative_path: PathBuf,
}

fn modified_ms(metadata: &fs::Metadata) -> Result<u64, String> {
    metadata
        .modified()
        .map_err(|error| format!("无法读取照片修改时间：{error}"))?
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
        .map_err(|_| "照片修改时间早于系统纪元".to_string())
}

fn safe_relative_path(value: &str) -> Result<PathBuf, String> {
    fs_util::safe_relative_path_str(value, "水印照片路径")
}

fn reject_relative_symlinks(root: &Path, relative: &Path) -> Result<(), String> {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(value) = component else {
            return Err("水印照片路径包含不安全片段".to_string());
        };
        current.push(value);
        reject_symlink(&current, "水印照片路径")?;
    }
    Ok(())
}

fn canonical_directory(path: &Path) -> Result<PathBuf, String> {
    fs_util::canonical_trusted_directory(path, "水印照片目录")
}

fn canonical_file(path: &Path) -> Result<PathBuf, String> {
    let metadata = reject_symlink(path, "水印照片文件")?;
    if !metadata.is_file() {
        return Err("水印照片来源不是普通文件".to_string());
    }
    fs::canonicalize(path).map_err(|error| format!("无法规范化水印照片文件：{error}"))
}

fn normalized_relative(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn group_key(root: &Path, relative_path: &Path) -> String {
    format!(
        "{}\0{}",
        root.to_string_lossy().to_lowercase(),
        formats::photo_group_key(relative_path, false)
    )
}

fn collect_candidate(
    root: &Path,
    path: &Path,
    relative_path: &Path,
    candidates: &mut Vec<SourceCandidate>,
    jpeg_groups: &mut HashSet<String>,
    raw_groups: &mut HashSet<String>,
    skipped_unsupported: &mut usize,
) -> Result<(), String> {
    let canonical_path = canonical_file(path)?;
    if !canonical_path.starts_with(root) {
        return Err("水印照片超出了所选目录".to_string());
    }
    if formats::is_reference(relative_path) {
        jpeg_groups.insert(group_key(root, relative_path));
        candidates.push(SourceCandidate {
            root: root.to_path_buf(),
            path: canonical_path,
            relative_path: relative_path.to_path_buf(),
        });
    } else if formats::is_raw(relative_path) {
        raw_groups.insert(group_key(root, relative_path));
    } else {
        *skipped_unsupported = skipped_unsupported.saturating_add(1);
    }
    Ok(())
}

fn collect_directory(
    path: &Path,
    candidates: &mut Vec<SourceCandidate>,
    roots: &mut BTreeSet<PathBuf>,
    jpeg_groups: &mut HashSet<String>,
    raw_groups: &mut HashSet<String>,
    skipped_unsupported: &mut usize,
) -> Result<(), String> {
    let root = canonical_directory(path)?;
    roots.insert(root.clone());
    for entry in WalkDir::new(&root).follow_links(false).sort_by_file_name() {
        let entry = entry.map_err(|error| format!("无法遍历水印照片目录：{error}"))?;
        if entry.path() == root {
            continue;
        }
        if entry.file_type().is_symlink() {
            return Err(format!(
                "水印照片目录包含符号链接：{}",
                entry.path().display()
            ));
        }
        if entry.file_type().is_dir() {
            continue;
        }
        if !entry.file_type().is_file() {
            return Err(format!(
                "水印照片目录包含非普通文件：{}",
                entry.path().display()
            ));
        }
        let relative = entry
            .path()
            .strip_prefix(&root)
            .map_err(|_| "无法确定水印照片相对路径".to_string())?;
        collect_candidate(
            &root,
            entry.path(),
            relative,
            candidates,
            jpeg_groups,
            raw_groups,
            skipped_unsupported,
        )?;
    }
    Ok(())
}

fn collect_file(
    path: &Path,
    candidates: &mut Vec<SourceCandidate>,
    roots: &mut BTreeSet<PathBuf>,
    jpeg_groups: &mut HashSet<String>,
    raw_groups: &mut HashSet<String>,
    skipped_unsupported: &mut usize,
) -> Result<(), String> {
    let path = canonical_file(path)?;
    let root = path
        .parent()
        .ok_or_else(|| "无法确定水印照片所在目录".to_string())?
        .to_path_buf();
    let relative = path
        .file_name()
        .map(PathBuf::from)
        .ok_or_else(|| "无法确定水印照片文件名".to_string())?;
    roots.insert(root.clone());
    collect_candidate(
        &root,
        &path,
        &relative,
        candidates,
        jpeg_groups,
        raw_groups,
        skipped_unsupported,
    )
}

fn collect_relative_paths(
    root: &Path,
    relative_paths: &[String],
    candidates: &mut Vec<SourceCandidate>,
    roots: &mut BTreeSet<PathBuf>,
    jpeg_groups: &mut HashSet<String>,
    raw_groups: &mut HashSet<String>,
    skipped_unsupported: &mut usize,
) -> Result<(), String> {
    let root = canonical_directory(root)?;
    roots.insert(root.clone());
    for value in relative_paths {
        let relative = safe_relative_path(value)?;
        reject_relative_symlinks(&root, &relative)?;
        collect_candidate(
            &root,
            &root.join(&relative),
            &relative,
            candidates,
            jpeg_groups,
            raw_groups,
            skipped_unsupported,
        )?;
    }
    Ok(())
}

fn corrected_dimensions(path: &Path) -> Result<(u32, u32), String> {
    let reader = ImageReader::open(path)
        .map_err(|error| format!("无法打开 JPG 水印来源：{error}"))?
        .with_guessed_format()
        .map_err(|error| format!("无法识别 JPG 水印来源：{error}"))?;
    let mut decoder = reader
        .into_decoder()
        .map_err(|error| format!("无法创建 JPG 解码器：{error}"))?;
    let (width, height) = decoder.dimensions();
    let orientation = decoder
        .orientation()
        .map_err(|error| format!("无法读取 JPG 方向：{error}"))?;
    if matches!(
        orientation,
        Orientation::Rotate90
            | Orientation::Rotate270
            | Orientation::Rotate90FlipH
            | Orientation::Rotate270FlipH
    ) {
        Ok((height, width))
    } else {
        Ok((width, height))
    }
}

fn classify_orientation(width: u32, height: u32) -> Result<WatermarkOrientation, String> {
    if width == 0 || height == 0 {
        return Err("JPG 水印来源尺寸无效".to_string());
    }
    let ratio = width as f64 / height as f64;
    if (0.95..=1.05).contains(&ratio) {
        Ok(WatermarkOrientation::Square)
    } else if width > height {
        Ok(WatermarkOrientation::Landscape)
    } else {
        Ok(WatermarkOrientation::Portrait)
    }
}

fn stable_photo_id(path: &Path, size_bytes: u64, modified_ms: u64) -> String {
    let mut digest = Sha256::new();
    digest.update(path.to_string_lossy().as_bytes());
    digest.update(size_bytes.to_be_bytes());
    digest.update(modified_ms.to_be_bytes());
    format!("{:x}", digest.finalize())
}

fn snapshot_id(created_at_ms: u64, photos: &[WatermarkSourcePhoto]) -> String {
    let mut digest = Sha256::new();
    digest.update(created_at_ms.to_be_bytes());
    for photo in photos {
        digest.update(photo.id.as_bytes());
    }
    format!("{:x}", digest.finalize())
}

pub(crate) fn prepare_source(
    request: WatermarkSourceRequest,
) -> Result<WatermarkSourceSnapshot, String> {
    if request.inputs.is_empty() {
        return Err("请至少选择一个 JPG 文件或照片目录".to_string());
    }

    let mut candidates = Vec::new();
    let mut roots = BTreeSet::new();
    let mut jpeg_groups = HashSet::new();
    let mut raw_groups = HashSet::new();
    let mut skipped_unsupported = 0usize;
    for input in request.inputs {
        match input {
            SourceInput::Directory { path } => collect_directory(
                Path::new(&path),
                &mut candidates,
                &mut roots,
                &mut jpeg_groups,
                &mut raw_groups,
                &mut skipped_unsupported,
            )?,
            SourceInput::File { path } => collect_file(
                Path::new(&path),
                &mut candidates,
                &mut roots,
                &mut jpeg_groups,
                &mut raw_groups,
                &mut skipped_unsupported,
            )?,
            SourceInput::RelativePaths {
                root,
                relative_paths,
            } => collect_relative_paths(
                Path::new(&root),
                &relative_paths,
                &mut candidates,
                &mut roots,
                &mut jpeg_groups,
                &mut raw_groups,
                &mut skipped_unsupported,
            )?,
        }
    }

    let mut seen_paths = HashSet::new();
    let mut photos = Vec::new();
    for candidate in candidates {
        if !seen_paths.insert(candidate.path.clone()) {
            continue;
        }
        let metadata = fs::metadata(&candidate.path)
            .map_err(|error| format!("无法读取 JPG 水印来源信息：{error}"))?;
        let size_bytes = metadata.len();
        let modified_ms = modified_ms(&metadata)?;
        let (pixel_width, pixel_height) = corrected_dimensions(&candidate.path)?;
        let file_name = candidate
            .path
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .ok_or_else(|| "无法确定 JPG 水印来源文件名".to_string())?;
        photos.push(WatermarkSourcePhoto {
            id: stable_photo_id(&candidate.path, size_bytes, modified_ms),
            root: candidate.root.to_string_lossy().into_owned(),
            relative_path: normalized_relative(&candidate.relative_path),
            file_name,
            size_bytes,
            modified_ms,
            pixel_width,
            pixel_height,
            orientation: classify_orientation(pixel_width, pixel_height)?,
        });
    }
    photos.sort_by(|left, right| {
        left.root
            .to_lowercase()
            .cmp(&right.root.to_lowercase())
            .then_with(|| {
                left.relative_path
                    .to_lowercase()
                    .cmp(&right.relative_path.to_lowercase())
            })
            .then_with(|| left.relative_path.cmp(&right.relative_path))
    });

    let created_at_ms = fs_util::now_ms();
    Ok(WatermarkSourceSnapshot {
        id: snapshot_id(created_at_ms, &photos),
        created_at_ms,
        origin: request.origin,
        root_paths: roots
            .into_iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect(),
        photos,
        skipped_raw_only: raw_groups.difference(&jpeg_groups).count(),
        skipped_unsupported,
    })
}

#[allow(dead_code)]
pub(crate) fn revalidate_photo(photo: &WatermarkSourcePhoto) -> Result<PathBuf, String> {
    let root_path = Path::new(&photo.root);
    reject_symlink(root_path, "水印照片目录")?;
    let root =
        fs::canonicalize(root_path).map_err(|error| format!("水印照片目录不可访问：{error}"))?;
    if root != root_path || !root.is_dir() {
        return Err("水印照片目录在扫描后发生变化".to_string());
    }
    let relative = safe_relative_path(&photo.relative_path)?;
    reject_relative_symlinks(&root, &relative)?;
    if !formats::is_reference(&relative) {
        return Err("当前水印来源不再是 JPG/JPEG".to_string());
    }
    let path = fs::canonicalize(root.join(&relative))
        .map_err(|error| format!("水印照片不可访问：{error}"))?;
    if !path.starts_with(&root) || !path.is_file() {
        return Err("水印照片超出了授权目录".to_string());
    }
    let metadata = fs::metadata(&path).map_err(|error| format!("无法读取水印照片信息：{error}"))?;
    if metadata.len() != photo.size_bytes || modified_ms(&metadata)? != photo.modified_ms {
        return Err("水印照片在扫描后发生变化，请重新载入".to_string());
    }
    Ok(path)
}
