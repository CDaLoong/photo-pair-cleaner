use crate::formats;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

const QUARANTINE_DIR: &str = ".framepair-quarantine";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum PhotoMemberKind {
    Jpeg,
    Raw,
    Xmp,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PhotoMemberSnapshot {
    pub(crate) kind: PhotoMemberKind,
    pub(crate) relative_path: String,
    pub(crate) size_bytes: u64,
    pub(crate) modified_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RatingState {
    pub(crate) frame_pair: u8,
    pub(crate) jpeg_metadata: Option<i8>,
    pub(crate) raw_xmp: Option<i8>,
    pub(crate) resolved: u8,
    pub(crate) conflict: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PhotoAsset {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) relative_stem: String,
    pub(crate) preview_path: Option<String>,
    pub(crate) jpeg_paths: Vec<String>,
    pub(crate) raw_paths: Vec<String>,
    pub(crate) xmp_paths: Vec<String>,
    pub(crate) members: Vec<PhotoMemberSnapshot>,
    pub(crate) extensions: Vec<String>,
    pub(crate) size_bytes: u64,
    pub(crate) modified_ms: Option<u64>,
    pub(crate) rating: u8,
    pub(crate) rating_state: RatingState,
    pub(crate) rating_issues: Vec<String>,
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
    xmp_paths: Vec<String>,
    members: Vec<PhotoMemberSnapshot>,
    size_bytes: u64,
    modified_ms: Option<u64>,
    rating_issues: Vec<String>,
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

fn member_snapshot(
    root: &Path,
    path: &Path,
    kind: PhotoMemberKind,
) -> Result<PhotoMemberSnapshot, String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| "照片索引超出了所选目录".to_string())?;
    let metadata = fs::metadata(path).map_err(|error| format!("读取照片信息失败：{error}"))?;
    Ok(PhotoMemberSnapshot {
        kind,
        relative_path: display_path(relative),
        size_bytes: metadata.len(),
        modified_ms: modified_ms(&metadata),
    })
}

fn finalize_asset(key: String, mut builder: PhotoAssetBuilder) -> PhotoAsset {
    builder.jpeg_paths.sort_by_key(|path| path.to_lowercase());
    builder.raw_paths.sort_by_key(|path| path.to_lowercase());
    builder.xmp_paths.sort_by_key(|path| path.to_lowercase());
    builder
        .members
        .sort_by_key(|member| member.relative_path.to_lowercase());

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
        xmp_paths: builder.xmp_paths,
        members: builder.members,
        extensions,
        size_bytes: builder.size_bytes,
        modified_ms: builder.modified_ms,
        rating: 0,
        rating_state: RatingState::default(),
        rating_issues: builder.rating_issues,
    }
}

pub(crate) fn index_directory(root: &Path) -> Result<PhotoIndex, String> {
    let root = fs::canonicalize(root).map_err(|error| format!("照片目录不可访问：{error}"))?;
    if !root.is_dir() {
        return Err("照片目录不是文件夹".to_string());
    }

    let mut groups = BTreeMap::<String, PhotoAssetBuilder>::new();
    let mut sidecars = Vec::new();
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
        if formats::is_sidecar(path) {
            sidecars.push(path.to_path_buf());
            continue;
        }
        let kind = if formats::is_reference(path) {
            PhotoMemberKind::Jpeg
        } else if formats::is_raw(path) {
            PhotoMemberKind::Raw
        } else {
            continue;
        };

        let relative = path
            .strip_prefix(&root)
            .map_err(|_| "照片索引超出了所选目录".to_string())?;
        let relative_path = display_path(relative);
        let relative_stem = display_path(&relative.with_extension(""));
        let key = formats::photo_group_key(relative, false);
        let member = member_snapshot(&root, path, kind)?;
        let builder = groups.entry(key).or_insert_with(|| PhotoAssetBuilder {
            relative_stem,
            ..PhotoAssetBuilder::default()
        });
        match kind {
            PhotoMemberKind::Jpeg => builder.jpeg_paths.push(relative_path),
            PhotoMemberKind::Raw => builder.raw_paths.push(relative_path),
            PhotoMemberKind::Xmp => unreachable!(),
        }
        builder.size_bytes = builder.size_bytes.saturating_add(member.size_bytes);
        builder.modified_ms = match (builder.modified_ms, member.modified_ms) {
            (Some(current), Some(next)) => Some(current.max(next)),
            (None, next) => next,
            (current, None) => current,
        };
        builder.members.push(member);
    }

    sidecars.sort_by_key(|path| display_path(path).to_lowercase());
    for path in sidecars {
        let relative = path
            .strip_prefix(&root)
            .map_err(|_| "XMP 文件超出了所选目录".to_string())?;
        let matching_keys = formats::sidecar_match_keys(relative, false)
            .into_iter()
            .filter(|key| groups.contains_key(key))
            .collect::<Vec<_>>();
        let Some(key) = matching_keys.last() else {
            continue;
        };
        let member = member_snapshot(&root, &path, PhotoMemberKind::Xmp)?;
        let builder = groups.get_mut(key).expect("matched group must exist");
        if matching_keys.len() > 1 {
            builder
                .rating_issues
                .push(format!("XMP {} 可以匹配多个照片组", member.relative_path));
        }
        builder.xmp_paths.push(member.relative_path.clone());
        builder.members.push(member);
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
