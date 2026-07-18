use image::{ImageDecoder, ImageReader};
use quick_xml::Reader;
use quick_xml::events::Event;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

const MAX_XMP_BYTES: u64 = 4 * 1024 * 1024;

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
    let mut depth = 0_usize;
    let mut rating_depth = None;
    let mut rating = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) => {
                depth += 1;
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
                if element.local_name().as_ref() == b"Rating" {
                    if rating_depth.replace(depth).is_some() {
                        return Err("XMP 中包含嵌套的 Rating".to_string());
                    }
                }
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
            Ok(Event::Text(text)) if rating_depth == Some(depth) => {
                let value = text
                    .decode()
                    .map_err(|error| format!("无法解码 XMP Rating：{error}"))?;
                if rating.replace(parse_rating(&value)?).is_some() {
                    return Err("XMP 中包含多个 Rating".to_string());
                }
            }
            Ok(Event::End(element)) => {
                if element.local_name().as_ref() == b"Rating" && rating_depth == Some(depth) {
                    rating_depth = None;
                }
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| "XMP XML 结束标签无效".to_string())?;
            }
            Ok(Event::Eof) if depth == 0 => return Ok(rating),
            Ok(Event::Eof) => return Err("XMP XML 未完整结束".to_string()),
            Ok(_) => {}
            Err(error) => return Err(format!("XMP XML 无效：{error}")),
        }
    }
}

fn checked_metadata(path: &Path, label: &str) -> Result<fs::Metadata, String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| format!("无法读取{label}文件信息：{error}"))?;
    if metadata.file_type().is_symlink() {
        return Err(format!("{label}文件不能是符号链接"));
    }
    if !metadata.is_file() {
        return Err(format!("{label}路径不是文件"));
    }
    Ok(metadata)
}

pub(crate) fn read_sidecar_rating(path: &Path) -> Result<Option<i8>, String> {
    let metadata = checked_metadata(path, "XMP")?;
    if metadata.len() > MAX_XMP_BYTES {
        return Err(format!(
            "XMP 文件超过 {} MiB 限制",
            MAX_XMP_BYTES / 1024 / 1024
        ));
    }

    let file = File::open(path).map_err(|error| format!("无法打开 XMP 文件：{error}"))?;
    let mut input = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_XMP_BYTES + 1)
        .read_to_end(&mut input)
        .map_err(|error| format!("无法读取 XMP 文件：{error}"))?;
    if input.len() as u64 > MAX_XMP_BYTES {
        return Err(format!(
            "XMP 文件超过 {} MiB 限制",
            MAX_XMP_BYTES / 1024 / 1024
        ));
    }
    xmp_rating(&input)
}

pub(crate) fn read_jpeg_rating(path: &Path) -> Result<Option<i8>, String> {
    checked_metadata(path, "JPG")?;
    let reader = ImageReader::open(path)
        .map_err(|error| format!("无法打开 JPG 文件：{error}"))?
        .with_guessed_format()
        .map_err(|error| format!("无法识别 JPG 文件：{error}"))?;
    let mut decoder = reader
        .into_decoder()
        .map_err(|error| format!("无法读取 JPG 元数据：{error}"))?;
    let Some(input) = decoder
        .xmp_metadata()
        .map_err(|error| format!("无法读取 JPG 内嵌 XMP：{error}"))?
    else {
        return Ok(None);
    };
    if input.len() as u64 > MAX_XMP_BYTES {
        return Err(format!(
            "JPG 内嵌 XMP 超过 {} MiB 限制",
            MAX_XMP_BYTES / 1024 / 1024
        ));
    }
    xmp_rating(&input)
}
