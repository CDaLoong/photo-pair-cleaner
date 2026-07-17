use serde::Serialize;
use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorPlatform {
    Macos,
    Windows,
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    Linux,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExternalEditor {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) kind: String,
    #[serde(skip)]
    launch_path: Option<PathBuf>,
}

fn editor_id(kind: &str, path: &Path) -> String {
    let mut hasher = DefaultHasher::new();
    kind.hash(&mut hasher);
    path.hash(&mut hasher);
    format!("{kind}:{:016x}", hasher.finish())
}

fn app_label(path: &Path) -> String {
    path.file_stem()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Adobe 编辑器".to_string())
}

pub(crate) fn discover_in(roots: &[PathBuf], platform: EditorPlatform) -> Vec<ExternalEditor> {
    let mut editors = vec![ExternalEditor {
        id: "system".to_string(),
        label: "系统默认应用".to_string(),
        kind: "system".to_string(),
        launch_path: None,
    }];
    let mut seen = HashSet::new();

    for root in roots {
        if !root.is_dir() {
            continue;
        }
        for entry in WalkDir::new(root)
            .follow_links(false)
            .max_depth(if platform == EditorPlatform::Windows {
                6
            } else {
                2
            })
            .into_iter()
            .filter_map(Result::ok)
        {
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy();
            let lower = file_name.to_ascii_lowercase();
            let kind = match platform {
                EditorPlatform::Macos if entry.file_type().is_dir() && lower.ends_with(".app") => {
                    if lower.contains("photoshop") {
                        Some("photoshop")
                    } else if lower.contains("lightroom classic") {
                        Some("lightroomClassic")
                    } else {
                        None
                    }
                }
                EditorPlatform::Windows if entry.file_type().is_file() => {
                    if lower == "photoshop.exe" {
                        Some("photoshop")
                    } else if lower == "lightroom.exe" {
                        Some("lightroomClassic")
                    } else {
                        None
                    }
                }
                #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                EditorPlatform::Linux => None,
                _ => None,
            };
            let Some(kind) = kind else { continue };
            let path = path.to_path_buf();
            if !seen.insert(path.clone()) {
                continue;
            }
            editors.push(ExternalEditor {
                id: editor_id(kind, &path),
                label: app_label(&path),
                kind: kind.to_string(),
                launch_path: Some(path),
            });
        }
    }
    editors[1..].sort_by(|left, right| left.label.to_lowercase().cmp(&right.label.to_lowercase()));
    editors
}

pub(crate) fn discover_installed() -> Vec<ExternalEditor> {
    #[cfg(target_os = "macos")]
    {
        let mut roots = vec![PathBuf::from("/Applications")];
        if let Some(home) = std::env::var_os("HOME") {
            roots.push(PathBuf::from(home).join("Applications"));
        }
        return discover_in(&roots, EditorPlatform::Macos);
    }
    #[cfg(target_os = "windows")]
    {
        let roots = ["ProgramFiles", "ProgramFiles(x86)"]
            .into_iter()
            .filter_map(std::env::var_os)
            .map(|path| PathBuf::from(path).join("Adobe"))
            .collect::<Vec<_>>();
        return discover_in(&roots, EditorPlatform::Windows);
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    discover_in(&[], EditorPlatform::Linux)
}

pub(crate) fn open_with(editor: &ExternalEditor, photo: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        if let Some(application) = &editor.launch_path {
            command.arg("-a").arg(application);
        }
        command.arg(photo);
        command
    };
    #[cfg(target_os = "windows")]
    let mut command = if let Some(application) = &editor.launch_path {
        let mut command = Command::new(application);
        command.arg(photo);
        command
    } else {
        let mut command = Command::new("explorer.exe");
        command.arg(photo);
        command
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(photo);
        command
    };

    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法启动 {}：{error}", editor.label))
}
