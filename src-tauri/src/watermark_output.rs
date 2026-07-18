use crate::watermark_metadata::{MetadataTarget, prepare_output_metadata, verify_output_metadata};
use crate::watermark_model::{
    CollisionPolicy, WatermarkOutputFormat, WatermarkOutputSettings, WatermarkRenderRequest,
    WatermarkSizing, WatermarkSourcePhoto, WatermarkSourceSnapshot,
};
use image::{GenericImageView, ImageFormat, ImageReader};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

const OUTPUT_MANIFEST_NAME: &str = ".framepair-watermark-outputs.json";
const OUTPUT_MANIFEST_VERSION: u16 = 1;
const MAX_OUTPUT_MANIFEST_BYTES: u64 = 1024 * 1024;
static OUTPUT_MANIFEST_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum PlannedCollision {
    Create,
    Sequence,
    SkipExisting,
    OverwriteOutput,
}

#[derive(Debug, Clone)]
pub(crate) struct PlannedWatermarkOutput {
    pub(crate) photo: WatermarkSourcePhoto,
    pub(crate) target_path: PathBuf,
    pub(crate) format: WatermarkOutputFormat,
    pub(crate) target_width: u32,
    pub(crate) target_height: u32,
    pub(crate) collision: PlannedCollision,
    output_directory: PathBuf,
    output_long_edge: Option<u32>,
    settings: WatermarkOutputSettings,
}

impl PlannedWatermarkOutput {
    pub(crate) fn output_directory(&self) -> &Path {
        &self.output_directory
    }

    pub(crate) fn estimated_max_bytes(&self) -> u64 {
        u64::from(self.target_width)
            .saturating_mul(u64::from(self.target_height))
            .saturating_mul(16)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum WatermarkOutputStatus {
    Succeeded,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WatermarkOutputResult {
    pub(crate) photo_id: String,
    pub(crate) target_path: String,
    pub(crate) status: WatermarkOutputStatus,
    pub(crate) message: String,
    pub(crate) size_bytes: Option<u64>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OutputManifest {
    schema_version: u16,
    outputs: BTreeSet<String>,
}

fn validate_suffix(value: &str) -> Result<(), String> {
    if value.chars().any(|character| {
        character.is_control()
            || matches!(
                character,
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
            )
    }) {
        return Err("文件名后缀包含系统不允许的字符".into());
    }
    if value.ends_with([' ', '.']) || value == "." || value == ".." {
        return Err("文件名后缀不能以空格或句点结尾".into());
    }
    if value.encode_utf16().count() > 120 {
        return Err("文件名后缀过长".into());
    }
    Ok(())
}

fn safe_source_path(photo: &WatermarkSourcePhoto) -> Result<PathBuf, String> {
    let relative = Path::new(&photo.relative_path);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("水印来源包含不安全路径".into());
    }
    Ok(Path::new(&photo.root).join(relative))
}

fn existing_directory(path: &Path) -> Result<PathBuf, String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| format!("输出目录不可访问：{error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("输出目录必须是本地普通文件夹，不能使用符号链接".into());
    }
    fs::canonicalize(path).map_err(|error| format!("无法规范化输出目录：{error}"))
}

fn output_directory(
    snapshot: &WatermarkSourceSnapshot,
    settings: &WatermarkOutputSettings,
) -> Result<PathBuf, String> {
    if let Some(value) = settings.output_directory.as_deref() {
        if value.trim().is_empty() {
            return Err("请选择输出目录".into());
        }
        return existing_directory(Path::new(value));
    }
    if snapshot.root_paths.len() != 1 {
        return Err("照片来自多个目录，请选择统一的输出目录".into());
    }
    let root = existing_directory(Path::new(&snapshot.root_paths[0]))?;
    let parent = root
        .parent()
        .ok_or_else(|| "无法确定默认输出目录".to_string())?;
    let destination = parent.join("FramePair-Watermarked");
    if destination.exists() {
        existing_directory(&destination)
    } else {
        Ok(destination)
    }
}

fn output_dimensions(
    photo: &WatermarkSourcePhoto,
    sizing: &WatermarkSizing,
) -> Result<(u32, u32, Option<u32>), String> {
    let width = photo.pixel_width;
    let height = photo.pixel_height;
    if width == 0 || height == 0 {
        return Err("来源照片尺寸无效".into());
    }
    match sizing {
        WatermarkSizing::Original { .. } => Ok((width, height, None)),
        WatermarkSizing::LongEdge {
            pixels,
            allow_upscale,
        } => {
            if !(64..=32_768).contains(pixels) {
                return Err("输出长边必须在 64 到 32768 像素之间".into());
            }
            let source_long_edge = width.max(height);
            let target_long_edge = if !allow_upscale && *pixels > source_long_edge {
                source_long_edge
            } else {
                *pixels
            };
            let target_width = ((u64::from(width) * u64::from(target_long_edge)
                + u64::from(source_long_edge) / 2)
                / u64::from(source_long_edge))
            .max(1) as u32;
            let target_height = ((u64::from(height) * u64::from(target_long_edge)
                + u64::from(source_long_edge) / 2)
                / u64::from(source_long_edge))
            .max(1) as u32;
            let render_edge = (target_long_edge != source_long_edge).then_some(target_long_edge);
            Ok((target_width, target_height, render_edge))
        }
    }
}

fn extension(format: WatermarkOutputFormat) -> &'static str {
    match format {
        WatermarkOutputFormat::Jpeg => "jpg",
        WatermarkOutputFormat::Png => "png",
    }
}

fn manifest_path(output_directory: &Path) -> PathBuf {
    output_directory.join(OUTPUT_MANIFEST_NAME)
}

fn read_manifest(output_directory: &Path) -> Result<OutputManifest, String> {
    let path = manifest_path(output_directory);
    if !path.exists() {
        return Ok(OutputManifest {
            schema_version: OUTPUT_MANIFEST_VERSION,
            outputs: BTreeSet::new(),
        });
    }
    let metadata =
        fs::symlink_metadata(&path).map_err(|error| format!("无法读取输出记录：{error}"))?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_OUTPUT_MANIFEST_BYTES
    {
        return Err("输出记录不是可信普通文件".into());
    }
    let manifest: OutputManifest = serde_json::from_slice(
        &fs::read(&path).map_err(|error| format!("无法读取输出记录：{error}"))?,
    )
    .map_err(|error| format!("输出记录已损坏：{error}"))?;
    if manifest.schema_version != OUTPUT_MANIFEST_VERSION {
        return Err(format!("不支持输出记录版本 {}", manifest.schema_version));
    }
    Ok(manifest)
}

fn trusted_output(output_directory: &Path, target: &Path) -> Result<bool, String> {
    let name = target
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "输出文件名不是有效 UTF-8".to_string())?;
    Ok(read_manifest(output_directory)?.outputs.contains(name))
}

fn write_manifest(output_directory: &Path, target: &Path) -> Result<(), String> {
    let _guard = OUTPUT_MANIFEST_LOCK
        .lock()
        .map_err(|_| "输出记录状态异常".to_string())?;
    let mut manifest = read_manifest(output_directory)?;
    let name = target
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "输出文件名不是有效 UTF-8".to_string())?;
    manifest.outputs.insert(name.to_string());
    let mut temporary = tempfile::Builder::new()
        .prefix(".framepair-output-record-")
        .suffix(".tmp")
        .tempfile_in(output_directory)
        .map_err(|error| format!("无法创建输出记录临时文件：{error}"))?;
    serde_json::to_writer(&mut temporary, &manifest)
        .map_err(|error| format!("无法写入输出记录：{error}"))?;
    temporary
        .flush()
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|error| format!("无法同步输出记录：{error}"))?;
    temporary
        .persist(manifest_path(output_directory))
        .map_err(|error| format!("无法提交输出记录：{}", error.error))?;
    Ok(())
}

pub(crate) fn plan_outputs(
    snapshot: &WatermarkSourceSnapshot,
    settings: &WatermarkOutputSettings,
) -> Result<Vec<PlannedWatermarkOutput>, String> {
    if snapshot.photos.is_empty() {
        return Err("没有可导出的 JPG/JPEG 照片".into());
    }
    if !(1..=100).contains(&settings.jpeg_quality) {
        return Err("JPEG 质量必须在 1 到 100 之间".into());
    }
    validate_suffix(&settings.suffix)?;
    crate::watermark_color::parse_css_color_linear(&settings.jpeg_flatten_color)?;
    let output_directory = output_directory(snapshot, settings)?;
    let sources = snapshot
        .photos
        .iter()
        .map(safe_source_path)
        .collect::<Result<HashSet<_>, _>>()?;
    let mut reserved = HashSet::new();
    let mut outputs = Vec::with_capacity(snapshot.photos.len());

    for photo in &snapshot.photos {
        let stem = Path::new(&photo.file_name)
            .file_stem()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("无法生成 {} 的输出文件名", photo.file_name))?;
        let base_name = format!("{stem}{}.{}", settings.suffix, extension(settings.format));
        let base_target = output_directory.join(&base_name);
        if sources.contains(&base_target) {
            return Err(format!("输出路径与来源照片相同：{}", photo.file_name));
        }
        let key = base_name.to_lowercase();
        let occupied = base_target.exists() || reserved.contains(&key);
        let (target_path, collision) = match settings.collision_policy {
            CollisionPolicy::Sequence if occupied => {
                let mut sequence = 2usize;
                loop {
                    let name = format!(
                        "{stem}{}_{}.{}",
                        settings.suffix,
                        sequence,
                        extension(settings.format)
                    );
                    let candidate = output_directory.join(&name);
                    if !candidate.exists() && !reserved.contains(&name.to_lowercase()) {
                        break (candidate, PlannedCollision::Sequence);
                    }
                    sequence = sequence
                        .checked_add(1)
                        .ok_or_else(|| "输出文件序号溢出".to_string())?;
                }
            }
            CollisionPolicy::Skip if occupied => (base_target, PlannedCollision::SkipExisting),
            CollisionPolicy::OverwriteOutput if reserved.contains(&key) => {
                return Err(format!("多个来源会写入同一个输出文件：{base_name}"));
            }
            CollisionPolicy::OverwriteOutput if base_target.exists() => {
                let metadata = fs::symlink_metadata(&base_target)
                    .map_err(|error| format!("无法检查同名输出：{error}"))?;
                if metadata.file_type().is_symlink()
                    || !metadata.is_file()
                    || sources.contains(&base_target)
                    || !trusted_output(&output_directory, &base_target)?
                {
                    return Err(format!("同名文件不是 FramePair 生成的副本：{base_name}"));
                }
                (base_target, PlannedCollision::OverwriteOutput)
            }
            _ => (base_target, PlannedCollision::Create),
        };
        let reserved_name = target_path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| "输出文件名不是有效 UTF-8".to_string())?
            .to_lowercase();
        reserved.insert(reserved_name);
        let (target_width, target_height, output_long_edge) =
            output_dimensions(photo, &settings.sizing)?;
        outputs.push(PlannedWatermarkOutput {
            photo: photo.clone(),
            target_path,
            format: settings.format,
            target_width,
            target_height,
            collision,
            output_directory: output_directory.clone(),
            output_long_edge,
            settings: settings.clone(),
        });
    }
    Ok(outputs)
}

fn failed(plan: &PlannedWatermarkOutput, message: String) -> WatermarkOutputResult {
    WatermarkOutputResult {
        photo_id: plan.photo.id.clone(),
        target_path: plan.target_path.to_string_lossy().into_owned(),
        status: WatermarkOutputStatus::Failed,
        message,
        size_bytes: None,
    }
}

fn execute_output(
    plan: &PlannedWatermarkOutput,
    request: &WatermarkRenderRequest,
    resource_dir: &Path,
) -> Result<u64, String> {
    if request.source != plan.photo
        || request.color_space != plan.settings.color_space
        || request.transparent_background != plan.settings.transparent_background
        || request.jpeg_flatten_color != plan.settings.jpeg_flatten_color
    {
        return Err("水印输出请求与已确认计划不一致".into());
    }
    let source = crate::watermark_source::revalidate_photo(&plan.photo)?;
    if !plan.output_directory.exists() {
        let parent = plan
            .output_directory
            .parent()
            .ok_or_else(|| "无法确定输出目录上级路径".to_string())?;
        existing_directory(parent)?;
        fs::create_dir(&plan.output_directory)
            .map_err(|error| format!("无法创建默认输出目录：{error}"))?;
    }
    let output_directory = existing_directory(&plan.output_directory)?;
    if output_directory != plan.output_directory {
        return Err("输出目录在确认后发生变化".into());
    }
    match plan.collision {
        PlannedCollision::Create | PlannedCollision::Sequence if plan.target_path.exists() => {
            return Err("输出文件在确认后已被占用，请重新确认".into());
        }
        PlannedCollision::OverwriteOutput
            if !plan.target_path.exists()
                || !trusted_output(&output_directory, &plan.target_path)? =>
        {
            return Err("待覆盖的 FramePair 副本在确认后发生变化".into());
        }
        _ => {}
    }
    let rendered = crate::watermark_render::render_request_with_target(
        &source,
        request,
        resource_dir,
        crate::watermark_render::RenderTarget::Export {
            output_long_edge: plan.output_long_edge,
        },
    )?;
    let mut encoded = crate::watermark_render::encode_output(
        &rendered,
        plan.format,
        plan.settings.jpeg_quality,
        plan.settings.color_space,
        plan.settings.transparent_background,
        &plan.settings.jpeg_flatten_color,
    )?;
    let metadata_target = match plan.format {
        WatermarkOutputFormat::Jpeg => MetadataTarget::Jpeg,
        WatermarkOutputFormat::Png => MetadataTarget::Png,
    };
    prepare_output_metadata(
        &source,
        plan.settings.metadata_policy,
        rendered.image.width(),
        rendered.image.height(),
        metadata_target,
    )?
    .apply_to_encoded(&mut encoded)?;
    verify_output_metadata(&encoded, plan.settings.metadata_policy, metadata_target)?;

    let mut temporary = tempfile::Builder::new()
        .prefix(".framepair-watermark-")
        .suffix(".tmp")
        .tempfile_in(&output_directory)
        .map_err(|error| format!("无法创建输出临时文件：{error}"))?;
    temporary
        .write_all(&encoded)
        .and_then(|_| temporary.flush())
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|error| format!("无法写入输出临时文件：{error}"))?;
    let reader = ImageReader::open(temporary.path())
        .map_err(|error| format!("无法重新打开输出临时文件：{error}"))?
        .with_guessed_format()
        .map_err(|error| format!("无法识别输出临时文件：{error}"))?;
    let expected_format = match plan.format {
        WatermarkOutputFormat::Jpeg => ImageFormat::Jpeg,
        WatermarkOutputFormat::Png => ImageFormat::Png,
    };
    if reader.format() != Some(expected_format) {
        return Err("输出临时文件格式校验失败".into());
    }
    let decoded = reader
        .decode()
        .map_err(|error| format!("无法解码输出临时文件：{error}"))?;
    if decoded.dimensions() != rendered.image.dimensions() {
        return Err("输出临时文件尺寸校验失败".into());
    }
    temporary
        .persist(&plan.target_path)
        .map_err(|error| format!("无法原子提交输出文件：{}", error.error))?;
    if plan.collision != PlannedCollision::OverwriteOutput
        && let Err(error) = write_manifest(&output_directory, &plan.target_path)
    {
        let _ = fs::remove_file(&plan.target_path);
        return Err(error);
    }
    #[cfg(unix)]
    std::fs::File::open(&output_directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("无法同步输出目录：{error}"))?;
    fs::metadata(&plan.target_path)
        .map(|metadata| metadata.len())
        .map_err(|error| format!("无法读取输出文件大小：{error}"))
}

pub(crate) fn write_output(
    plan: &PlannedWatermarkOutput,
    request: &WatermarkRenderRequest,
    resource_dir: &Path,
) -> WatermarkOutputResult {
    if plan.collision == PlannedCollision::SkipExisting {
        return WatermarkOutputResult {
            photo_id: plan.photo.id.clone(),
            target_path: plan.target_path.to_string_lossy().into_owned(),
            status: WatermarkOutputStatus::Skipped,
            message: "已跳过同名文件".into(),
            size_bytes: None,
        };
    }
    match execute_output(plan, request, resource_dir) {
        Ok(size_bytes) => WatermarkOutputResult {
            photo_id: plan.photo.id.clone(),
            target_path: plan.target_path.to_string_lossy().into_owned(),
            status: WatermarkOutputStatus::Succeeded,
            message: "水印副本已生成".into(),
            size_bytes: Some(size_bytes),
        },
        Err(error) => failed(plan, error),
    }
}
