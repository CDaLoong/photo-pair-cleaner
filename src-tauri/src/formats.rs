use std::path::Path;

pub(crate) const REFERENCE_EXTENSIONS: &[&str] = &["jpg", "jpeg"];
pub(crate) const RAW_EXTENSIONS: &[&str] = &[
    "nef", "nrw", "cr2", "cr3", "arw", "sr2", "srf", "raf", "dng", "rw2", "orf", "pef",
];
pub(crate) const SIDECAR_EXTENSIONS: &[&str] = &["xmp"];

pub(crate) fn extension_of(path: &Path) -> String {
    path.extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

pub(crate) fn is_reference(path: &Path) -> bool {
    REFERENCE_EXTENSIONS.contains(&extension_of(path).as_str())
}

pub(crate) fn is_raw(path: &Path) -> bool {
    RAW_EXTENSIONS.contains(&extension_of(path).as_str())
}

pub(crate) fn is_sidecar(path: &Path) -> bool {
    SIDECAR_EXTENSIONS.contains(&extension_of(path).as_str())
}

pub(crate) fn normalized_path_key(path: &Path, case_sensitive: bool) -> String {
    let value = path.to_string_lossy().replace('\\', "/");
    if case_sensitive {
        value
    } else {
        value.to_lowercase()
    }
}

pub(crate) fn photo_group_key(path: &Path, case_sensitive: bool) -> String {
    normalized_path_key(&path.with_extension(""), case_sensitive)
}

pub(crate) fn sidecar_match_keys(path: &Path, case_sensitive: bool) -> Vec<String> {
    let without_xmp = path.with_extension("");
    let mut keys = vec![normalized_path_key(&without_xmp, case_sensitive)];
    if RAW_EXTENSIONS.contains(&extension_of(&without_xmp).as_str()) {
        keys.push(normalized_path_key(
            &without_xmp.with_extension(""),
            case_sensitive,
        ));
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn recognizes_supported_raw_families() {
        for name in [
            "a.NEF", "a.NRW", "a.CR2", "a.CR3", "a.ARW", "a.SR2", "a.SRF", "a.RAF", "a.DNG",
            "a.RW2", "a.ORF", "a.PEF",
        ] {
            assert!(is_raw(Path::new(name)), "{name} should be RAW");
        }
        assert!(!is_raw(Path::new("a.tiff")));
        assert!(!is_raw(Path::new("a.exe")));
    }

    #[test]
    fn photo_group_keys_are_stable_and_optionally_case_sensitive() {
        assert_eq!(photo_group_key(Path::new("Day/A.NEF"), false), "day/a");
        assert_eq!(photo_group_key(Path::new("Day/A.JPG"), false), "day/a");
        assert_eq!(photo_group_key(Path::new("Day/A.NEF"), true), "Day/A");
    }

    #[test]
    fn xmp_keys_support_both_common_naming_forms() {
        assert_eq!(
            sidecar_match_keys(Path::new("day/a.xmp"), false),
            vec!["day/a"]
        );
        assert_eq!(
            sidecar_match_keys(Path::new("day/a.NEF.xmp"), false),
            vec!["day/a.nef", "day/a"]
        );
    }
}
