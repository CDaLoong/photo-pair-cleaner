//! 成对扫描领域：把参考源（JPG 目录 / 清单 / XMP 评分）与 RAW 目录比对，
//! 产出「哪些 RAW 已失去参考」的扫描结果。这是清理流程的唯一事实来源，
//! 后续的清理计划只能基于这里产出的 ScanSummary 生成。

use crate::fs_util::{
    canonical_directory_from_input, collect_files, display_path, modified_ms, now_ms,
};
use crate::safety::unique_keys;
use crate::{formats, quarantine, reference};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self};
use std::path::{Path, PathBuf};

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

pub(crate) fn scan_item(
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
    let relative_path = display_path(relative);
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
    let raw_root = canonical_directory_from_input(&request.raw_root, "RAW 源目录")?;
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
    for path in collect_files(&raw_root, quarantine::QUARANTINE_DIR, "扫描目录失败")? {
        let relative = path
            .strip_prefix(&raw_root)
            .map_err(|_| "RAW 文件超出了源目录".to_string())?;
        if formats::is_raw(&path) {
            raws.entry(formats::photo_group_key(relative, request.case_sensitive))
                .or_default()
                .push(display_path(relative));
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
#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats;
    use std::path::Path;

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
}
