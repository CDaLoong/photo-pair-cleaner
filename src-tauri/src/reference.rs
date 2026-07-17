use quick_xml::Reader;
use quick_xml::events::Event;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use walkdir::WalkDir;

const MAX_MANIFEST_BYTES: u64 = 8 * 1024 * 1024;
const MAX_XMP_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ReferenceSource {
    Directory { root: String },
    Manifest { path: String },
    XmpRating { root: String, minimum_rating: u8 },
}

impl ReferenceSource {
    pub(crate) fn is_directory(&self) -> bool {
        matches!(self, Self::Directory { .. })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ReferenceMatch {
    pub display_path: String,
    pub physical_path: Option<PathBuf>,
}

#[derive(Debug)]
pub(crate) struct ReferenceIndex {
    pub root: Option<PathBuf>,
    pub entries: HashMap<String, Vec<ReferenceMatch>>,
    pub source_items: usize,
    pub duplicate_keys: usize,
}

fn canonical_directory(input: &str, label: &str) -> Result<PathBuf, String> {
    let path =
        fs::canonicalize(input.trim()).map_err(|error| format!("{label}不可访问：{error}"))?;
    if !path.is_dir() {
        return Err(format!("{label}不是文件夹"));
    }
    Ok(path)
}

fn collect_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() != 1 || entry.file_name() != crate::quarantine::QUARANTINE_DIR
        })
    {
        let entry = entry.map_err(|error| format!("扫描参考源失败：{error}"))?;
        if entry.file_type().is_file() {
            files.push(entry.into_path());
        }
    }
    files.sort_by_key(|path| path.to_string_lossy().to_lowercase());
    Ok(files)
}

fn match_key(path: &Path, case_sensitive: bool) -> String {
    let value = path.with_extension("").to_string_lossy().replace('\\', "/");
    if case_sensitive {
        value
    } else {
        value.to_lowercase()
    }
}

#[cfg(test)]
fn parse_manifest(input: &str, case_sensitive: bool) -> Result<HashSet<String>, String> {
    Ok(parse_manifest_entries(input, case_sensitive)?
        .into_iter()
        .map(|(key, _)| key)
        .collect())
}

fn parse_manifest_entries(
    input: &str,
    case_sensitive: bool,
) -> Result<Vec<(String, String)>, String> {
    let mut keys = HashSet::new();
    let mut entries = Vec::new();
    for (index, line) in input.lines().enumerate() {
        let value = line.trim();
        if value.is_empty() || value.starts_with('#') {
            continue;
        }
        let path = Path::new(value);
        if path.is_absolute()
            || path
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(format!("文件清单第 {} 行不是安全相对路径", index + 1));
        }
        if !matches!(
            path.extension()
                .map(|extension| extension.to_string_lossy().to_ascii_lowercase()),
            Some(extension) if extension == "jpg" || extension == "jpeg"
        ) {
            return Err(format!("文件清单第 {} 行不是 JPG/JPEG", index + 1));
        }
        let key = match_key(path, case_sensitive);
        if !keys.insert(key.clone()) {
            return Err(format!("文件清单第 {} 行产生了重复匹配键", index + 1));
        }
        entries.push((key, path.to_string_lossy().replace('\\', "/")));
    }
    if keys.is_empty() {
        return Err("文件清单中没有有效 JPG 路径".to_string());
    }
    Ok(entries)
}

fn parse_rating(value: &str) -> Result<i8, String> {
    let rating = value
        .trim()
        .parse::<i8>()
        .map_err(|_| "XMP Rating 不是整数".to_string())?;
    if !(-1..=5).contains(&rating) {
        return Err("XMP Rating 必须在 -1 到 5 之间".to_string());
    }
    Ok(rating)
}

pub(crate) fn xmp_rating(input: &[u8]) -> Result<Option<i8>, String> {
    let mut reader = Reader::from_reader(input);
    reader.config_mut().trim_text(true);
    let mut inside_rating = false;
    let mut rating = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) => {
                for attribute in element.attributes() {
                    let attribute = attribute.map_err(|error| format!("XMP 属性无效：{error}"))?;
                    if attribute.key.local_name().as_ref() == b"Rating" {
                        let value = attribute
                            .decode_and_unescape_value(reader.decoder())
                            .map_err(|error| format!("无法读取 XMP Rating：{error}"))?;
                        if rating.replace(parse_rating(&value)?).is_some() {
                            return Err("XMP 中包含多个 Rating".to_string());
                        }
                    }
                }
                inside_rating = element.local_name().as_ref() == b"Rating";
            }
            Ok(Event::Empty(element)) => {
                for attribute in element.attributes() {
                    let attribute = attribute.map_err(|error| format!("XMP 属性无效：{error}"))?;
                    if attribute.key.local_name().as_ref() == b"Rating" {
                        let value = attribute
                            .decode_and_unescape_value(reader.decoder())
                            .map_err(|error| format!("无法读取 XMP Rating：{error}"))?;
                        if rating.replace(parse_rating(&value)?).is_some() {
                            return Err("XMP 中包含多个 Rating".to_string());
                        }
                    }
                }
            }
            Ok(Event::Text(text)) if inside_rating => {
                let value = text
                    .decode()
                    .map_err(|error| format!("无法解码 XMP Rating：{error}"))?;
                if rating.replace(parse_rating(&value)?).is_some() {
                    return Err("XMP 中包含多个 Rating".to_string());
                }
            }
            Ok(Event::End(element)) if element.local_name().as_ref() == b"Rating" => {
                inside_rating = false;
            }
            Ok(Event::Eof) => return Ok(rating),
            Ok(_) => {}
            Err(error) => return Err(format!("XMP XML 无效：{error}")),
        }
    }
}

pub(crate) fn build_index(
    source: &ReferenceSource,
    case_sensitive: bool,
) -> Result<ReferenceIndex, String> {
    let (root, entries, source_items) = match source {
        ReferenceSource::Directory { root } => {
            let root = canonical_directory(root, "JPG 参考目录")?;
            let mut entries: HashMap<String, Vec<ReferenceMatch>> = HashMap::new();
            let mut source_items = 0usize;
            for path in collect_files(&root)? {
                if !crate::formats::is_reference(&path) {
                    continue;
                }
                let relative = path
                    .strip_prefix(&root)
                    .map_err(|_| "JPG 文件超出了参考目录".to_string())?;
                entries
                    .entry(match_key(relative, case_sensitive))
                    .or_default()
                    .push(ReferenceMatch {
                        display_path: relative.to_string_lossy().replace('\\', "/"),
                        physical_path: Some(path),
                    });
                source_items += 1;
            }
            (Some(root), entries, source_items)
        }
        ReferenceSource::Manifest { path } => {
            let path = fs::canonicalize(path.trim())
                .map_err(|error| format!("文件清单不可访问：{error}"))?;
            if !path.is_file() || crate::formats::extension_of(&path) != "txt" {
                return Err("文件清单必须是 .txt 普通文件".to_string());
            }
            let metadata =
                fs::metadata(&path).map_err(|error| format!("无法读取文件清单：{error}"))?;
            if metadata.len() > MAX_MANIFEST_BYTES {
                return Err("文件清单超过 8 MiB 上限".to_string());
            }
            let input = fs::read_to_string(&path)
                .map_err(|error| format!("文件清单必须是有效 UTF-8：{error}"))?;
            let manifest_entries = parse_manifest_entries(&input, case_sensitive)?;
            let source_items = manifest_entries.len();
            let entries = manifest_entries
                .into_iter()
                .map(|(key, display_path)| {
                    (
                        key,
                        vec![ReferenceMatch {
                            display_path,
                            physical_path: None,
                        }],
                    )
                })
                .collect();
            (None, entries, source_items)
        }
        ReferenceSource::XmpRating {
            root,
            minimum_rating,
        } => {
            if !(1..=5).contains(minimum_rating) {
                return Err("最低 XMP 星级必须在 1 到 5 之间".to_string());
            }
            let root = canonical_directory(root, "XMP 评分目录")?;
            let mut entries: HashMap<String, Vec<ReferenceMatch>> = HashMap::new();
            let mut source_items = 0usize;
            for path in collect_files(&root)? {
                if !crate::formats::is_sidecar(&path) {
                    continue;
                }
                let metadata = fs::metadata(&path)
                    .map_err(|error| format!("无法读取 XMP 文件信息：{error}"))?;
                if metadata.len() > MAX_XMP_BYTES {
                    return Err(format!("XMP 文件超过 4 MiB 上限：{}", path.display()));
                }
                let input = fs::read(&path).map_err(|error| format!("无法读取 XMP：{error}"))?;
                let Some(rating) = xmp_rating(&input)? else {
                    continue;
                };
                if rating < *minimum_rating as i8 {
                    continue;
                }
                let relative = path
                    .strip_prefix(&root)
                    .map_err(|_| "XMP 文件超出了评分目录".to_string())?;
                let display_path = relative.to_string_lossy().replace('\\', "/");
                for key in crate::formats::sidecar_match_keys(relative, case_sensitive) {
                    entries.entry(key).or_default().push(ReferenceMatch {
                        display_path: display_path.clone(),
                        physical_path: Some(path.clone()),
                    });
                }
                source_items += 1;
            }
            (Some(root), entries, source_items)
        }
    };

    let duplicate_keys = entries.values().filter(|matches| matches.len() > 1).count();
    Ok(ReferenceIndex {
        root,
        entries,
        source_items,
        duplicate_keys,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::fs;

    #[test]
    fn parses_relative_utf8_manifest_and_rejects_unsafe_entries() {
        let input = "day/a.JPG\nother/b.jpeg\n# comment\n\n";
        let keys = parse_manifest(input, false).expect("valid manifest");
        assert_eq!(
            keys,
            HashSet::from(["day/a".to_string(), "other/b".to_string()])
        );

        assert!(parse_manifest("../a.jpg\n", false).is_err());
        assert!(parse_manifest("/absolute/a.jpg\n", false).is_err());
        assert!(parse_manifest("a.jpg\na.jpeg\n", false).is_err());
        assert!(parse_manifest("a.png\n", false).is_err());
    }

    #[test]
    fn reads_xmp_rating_from_attribute_or_element() {
        let attribute = br#"<rdf:Description xmp:Rating="5" />"#;
        let element = br#"<xmp:Rating>4</xmp:Rating>"#;
        assert_eq!(xmp_rating(attribute).expect("attribute rating"), Some(5));
        assert_eq!(xmp_rating(element).expect("element rating"), Some(4));
        assert_eq!(xmp_rating(b"<x:xmpmeta />").expect("missing rating"), None);
        assert!(xmp_rating(br#"<xmp:Rating>9</xmp:Rating>"#).is_err());
        assert!(xmp_rating(br#"<xmp:Rating>5</xmp:Rating><broken"#).is_err());
    }

    #[test]
    fn builds_a_manifest_reference_index() {
        let temp = tempfile::tempdir().expect("temp directory");
        let manifest = temp.path().join("keepers.txt");
        fs::write(&manifest, "day/a.JPG\nother/b.jpeg\n").expect("manifest");

        let index = build_index(
            &ReferenceSource::Manifest {
                path: manifest.to_string_lossy().into_owned(),
            },
            false,
        )
        .expect("manifest index");
        assert_eq!(index.source_items, 2);
        assert!(index.entries.contains_key("day/a"));
        assert!(index.entries.contains_key("other/b"));
        assert_eq!(index.entries["day/a"][0].display_path, "day/a.JPG");
        assert_eq!(index.duplicate_keys, 0);
    }

    #[test]
    fn xmp_index_keeps_only_ratings_at_or_above_the_threshold() {
        let temp = tempfile::tempdir().expect("temp directory");
        fs::write(
            temp.path().join("a.NEF.xmp"),
            br#"<rdf:Description xmp:Rating="5" />"#,
        )
        .expect("five star xmp");
        fs::write(temp.path().join("b.xmp"), br#"<xmp:Rating>3</xmp:Rating>"#)
            .expect("three star xmp");

        let index = build_index(
            &ReferenceSource::XmpRating {
                root: temp.path().to_string_lossy().into_owned(),
                minimum_rating: 4,
            },
            false,
        )
        .expect("xmp index");
        assert_eq!(index.source_items, 1);
        assert!(index.entries.contains_key("a"));
        assert!(!index.entries.contains_key("b"));
    }

    #[test]
    fn reference_source_uses_camel_case_ipc_fields() {
        let source: ReferenceSource =
            serde_json::from_str(r#"{"type":"xmpRating","root":"/photos","minimumRating":4}"#)
                .expect("camel case source");
        assert!(matches!(
            source,
            ReferenceSource::XmpRating {
                minimum_rating: 4,
                ..
            }
        ));
    }
}
