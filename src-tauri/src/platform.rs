//! 操作系统集成：在文件管理器中定位文件、打开回收站。
//!
//! 每个函数都有 macOS / Windows / 其它平台三份 cfg 实现，签名保持一致，
//! 调用方不需要关心平台差异。

use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(target_os = "macos")]
pub(crate) fn reveal_path(path: &Path) -> Result<(), String> {
    Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法在 Finder 中显示：{error}"))
}

#[cfg(target_os = "windows")]
pub(crate) fn reveal_path(path: &Path) -> Result<(), String> {
    Command::new("explorer.exe")
        .arg(format!("/select,{}", path.display()))
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法在文件资源管理器中显示：{error}"))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) fn reveal_path(path: &Path) -> Result<(), String> {
    let directory = path.parent().unwrap_or(path);
    Command::new("xdg-open")
        .arg(directory)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法在文件管理器中显示：{error}"))
}

#[cfg(target_os = "macos")]
pub(crate) fn open_trash_location() -> Result<(), String> {
    let home = std::env::var_os("HOME").ok_or_else(|| "无法确定用户目录".to_string())?;
    Command::new("open")
        .arg(PathBuf::from(home).join(".Trash"))
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法打开废纸篓：{error}"))
}

#[cfg(target_os = "windows")]
pub(crate) fn open_trash_location() -> Result<(), String> {
    Command::new("explorer.exe")
        .arg("shell:RecycleBinFolder")
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法打开回收站：{error}"))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) fn open_trash_location() -> Result<(), String> {
    Command::new("xdg-open")
        .arg("trash:///")
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法打开回收站：{error}"))
}
