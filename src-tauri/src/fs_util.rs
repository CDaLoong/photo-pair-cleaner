//! 后端各模块共用的文件系统与时间工具。
//!
//! 这些函数原本被复制到十几个模块里，副本之间已经开始漂移——最要命的是路径安全
//! 校验，弱化的副本不是风格问题而是安全漏洞。凡是「守卫路径」或「打时间戳」的
//! 逻辑都应该收敛到这里。
//!
//! 集成测试注意：`tests/` 下的文件通过 `#[path]` 重新编译 `src/` 模块，因此那里的
//! `crate::` 指向测试 crate 而不是 lib。只要测试引入了用到本模块的 src 模块，就必须
//! 同时声明 `#[path = "../src/fs_util.rs"] mod fs_util;`。

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use walkdir::WalkDir;

/// Unix 纪元以来的毫秒数，遇到异常系统时钟时饱和取值而不 panic。
/// 用于生成操作 ID 和缓存时间戳，不要用它测量时间间隔。
pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
        .unwrap_or_default()
}

/// 文件修改时间（纪元毫秒）。平台无法提供时返回 `None`，
/// 调用方一律把 `None` 当作「无法证明文件未被改动」处理。
pub(crate) fn modified_ms(metadata: &fs::Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
}

/// 把路径渲染成界面用的字符串，统一使用正斜杠，保证 Windows 与 macOS 输出一致。
/// 仅供展示，结果不要再交回文件系统使用。
pub(crate) fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// 拒绝一切可能逃逸出根目录的相对路径：绝对路径、`..`、`.` 以及 Windows 盘符前缀。
/// `label` 用于在错误信息里指明这是哪种路径，让各调用方保留自己的措辞。
///
/// 所有「根目录 + 相对路径」的拼接都必须先过这道关卡。
/// 调用方可以在此基础上叠加自己的额外规则（允许的扩展名、保留目录等）。
pub(crate) fn safe_relative_path<'a>(value: &'a Path, label: &str) -> Result<&'a Path, String> {
    if value.to_string_lossy().trim().is_empty() || value.is_absolute() {
        return Err(format!("{label}必须是安全相对路径"));
    }
    if value
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!("{label}包含不安全片段"));
    }
    Ok(value)
}

/// 面向前端传来的字符串路径的 [`safe_relative_path`]。
pub(crate) fn safe_relative_path_str(value: &str, label: &str) -> Result<PathBuf, String> {
    safe_relative_path(Path::new(value), label).map(Path::to_path_buf)
}

/// `path` 是符号链接时报错，否则返回它的元数据。
/// 使用 `symlink_metadata`，检查的是链接本身而不是它指向的目标。
pub(crate) fn reject_symlink(path: &Path, label: &str) -> Result<fs::Metadata, String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| format!("{label}不可访问：{error}"))?;
    if metadata.file_type().is_symlink() {
        return Err(format!("{label}不能是符号链接"));
    }
    Ok(metadata)
}

/// 解析用户自己选中的目录。
///
/// 这里故意跟随符号链接：用户把照片库放在软链接后面是完全合理的用法，
/// 规范化后的结果会被记为授权根目录。
/// 应用要写入的目录请改用 [`canonical_trusted_directory`]。
pub(crate) fn canonical_directory(path: &Path, label: &str) -> Result<PathBuf, String> {
    let canonical = fs::canonicalize(path).map_err(|error| format!("{label}不可访问：{error}"))?;
    if !canonical.is_dir() {
        return Err(format!("{label}不是文件夹"));
    }
    Ok(canonical)
}

/// 面向原始用户输入的 [`canonical_directory`]，
/// 空输入时给出明确的「请选择」提示，而不是让用户看到晦涩的系统报错。
pub(crate) fn canonical_directory_from_input(input: &str, label: &str) -> Result<PathBuf, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(format!("请选择{label}"));
    }
    canonical_directory(Path::new(trimmed), label)
}

/// 解析应用即将写入的目录。
///
/// 与 [`canonical_directory`] 不同，这里拒绝符号链接：
/// 否则授权目录里被植入一个软链接，就能把复制、移动、删除重定向到用户从未批准的位置。
pub(crate) fn canonical_trusted_directory(path: &Path, label: &str) -> Result<PathBuf, String> {
    let metadata = reject_symlink(path, label)?;
    if !metadata.is_dir() {
        return Err(format!("{label}不是可信文件夹"));
    }
    fs::canonicalize(path).map_err(|error| format!("{label}不可访问：{error}"))
}

/// 遍历 `root` 返回其中所有普通文件，按大小写不敏感排序以保证扫描结果可复现。
///
/// 不跟随符号链接；`exclude_top_level` 会跳过深度为 1 的某个目录——
/// 隔离区正是靠这个参数不被重复扫描成新的待处理项。
pub(crate) fn collect_files(
    root: &Path,
    exclude_top_level: &str,
    error_label: &str,
) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| entry.depth() != 1 || entry.file_name() != exclude_top_level)
    {
        let entry = entry.map_err(|error| format!("{error_label}：{error}"))?;
        if entry.file_type().is_file() {
            files.push(entry.into_path());
        }
    }
    files.sort_by_key(|path| display_path(path).to_lowercase());
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_relative_path_rejects_every_escape_shape() {
        for value in ["", "   ", "/absolute", "../up", "a/../b", "./here"] {
            assert!(
                safe_relative_path(Path::new(value), "测试路径").is_err(),
                "expected {value:?} to be rejected"
            );
        }
        assert!(safe_relative_path(Path::new("day/photo.NEF"), "测试路径").is_ok());
    }

    #[test]
    fn display_path_normalizes_separators_for_the_ui() {
        assert_eq!(display_path(Path::new("day/photo.NEF")), "day/photo.NEF");
        assert_eq!(display_path(Path::new(r"day\photo.NEF")), "day/photo.NEF");
    }
}
