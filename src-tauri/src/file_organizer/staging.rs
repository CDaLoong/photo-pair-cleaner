//! 落盘前的暂存与校验原语：准备目标父目录、流式拷贝到临时文件、计算指纹。
//!
//! 所有写入都先落到临时文件再原子改名，这样中途崩溃不会在目标位置留下半个文件。

use super::*;

pub(crate) fn ensure_target_parent(destination: &Path, target: &Path) -> Result<(), String> {
    let parent = target
        .parent()
        .ok_or_else(|| "无法确定目标父目录".to_string())?;
    let relative = parent
        .strip_prefix(destination)
        .map_err(|_| "目标父目录超出了规则目标目录".to_string())?;
    let mut current = destination.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(format!(
                    "目标父目录不是可信文件夹：{}",
                    display_path(&current)
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&current)
                .map_err(|error| format!("无法创建目标目录 {}：{error}", display_path(&current)))?,
            Err(error) => return Err(format!("无法检查目标目录：{error}")),
        }
    }
    let canonical_parent =
        fs::canonicalize(parent).map_err(|error| format!("无法校验目标父目录：{error}"))?;
    if !canonical_parent.starts_with(destination) {
        return Err("目标父目录解析后超出了规则目标目录".to_string());
    }
    Ok(())
}

pub(crate) fn stream_copy_to_temporary(
    member: ValidatedMember,
    destination: &Path,
) -> Result<StagedCopy, String> {
    ensure_target_parent(destination, &member.target)?;
    let parent = member
        .target
        .parent()
        .ok_or_else(|| "无法确定目标父目录".to_string())?;
    let mut source = File::open(&member.source)
        .map_err(|error| format!("无法打开源文件 {}：{error}", display_path(&member.source)))?;
    let opened_metadata = source
        .metadata()
        .map_err(|error| format!("无法读取源文件信息：{error}"))?;
    if opened_metadata.len() != member.expected_size_bytes
        || modified_ms(&opened_metadata) != member.expected_modified_ms
    {
        return Err("源文件在复制前发生变化".to_string());
    }
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("无法创建目标临时文件：{error}"))?;
    let mut hasher = Sha256::new();
    let mut copied = 0_u64;
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = source
            .read(&mut buffer)
            .map_err(|error| format!("无法读取源文件：{error}"))?;
        if read == 0 {
            break;
        }
        temporary
            .write_all(&buffer[..read])
            .map_err(|error| format!("无法写入目标临时文件：{error}"))?;
        hasher.update(&buffer[..read]);
        copied = copied.saturating_add(read as u64);
    }
    temporary
        .as_file_mut()
        .sync_all()
        .map_err(|error| format!("无法同步目标临时文件：{error}"))?;
    let final_source_metadata = source
        .metadata()
        .map_err(|error| format!("无法复核源文件信息：{error}"))?;
    if copied != member.expected_size_bytes
        || final_source_metadata.len() != member.expected_size_bytes
        || modified_ms(&final_source_metadata) != member.expected_modified_ms
    {
        return Err("源文件在复制过程中发生变化".to_string());
    }
    let sha256 = format!("{:x}", hasher.finalize());
    if fingerprint(temporary.path())?.sha256 != sha256 {
        return Err("目标临时文件内容校验失败".to_string());
    }
    Ok(StagedCopy {
        member,
        temporary,
        sha256,
    })
}

pub(crate) fn fingerprint(path: &Path) -> Result<FileFingerprint, String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| format!("无法读取文件校验信息：{error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("校验目标不是可信普通文件".to_string());
    }
    let mut file = File::open(path).map_err(|error| format!("无法打开校验文件：{error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("无法读取校验文件：{error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(FileFingerprint {
        size_bytes: metadata.len(),
        modified_ms: modified_ms(&metadata),
        sha256: format!("{:x}", hasher.finalize()),
    })
}

pub(crate) fn unchanged(path: &Path, expected: &FileFingerprint) -> bool {
    fingerprint(path).is_ok_and(|actual| actual == *expected)
}

#[cfg(unix)]
pub(crate) fn paths_share_device(source: &Path, target_parent: &Path) -> Result<bool, String> {
    use std::os::unix::fs::MetadataExt;

    let source_metadata =
        fs::metadata(source).map_err(|error| format!("无法读取移动源设备信息：{error}"))?;
    let target_metadata = fs::metadata(target_parent)
        .map_err(|error| format!("无法读取移动目标设备信息：{error}"))?;
    Ok(source_metadata.dev() == target_metadata.dev())
}

#[cfg(not(unix))]
pub(crate) fn paths_share_device(_source: &Path, _target_parent: &Path) -> Result<bool, String> {
    Ok(false)
}
