use image::{ImageDecoder, ImageReader};
use quick_xml::Reader;
use quick_xml::Writer;
use quick_xml::encoding::Decoder;
use quick_xml::events::{BytesStart, BytesText, Event};
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

#[allow(dead_code)]
fn rewritten_description(
    element: &BytesStart<'_>,
    decoder: Decoder,
    rating: u8,
    insert_if_missing: bool,
) -> Result<(BytesStart<'static>, bool), String> {
    let name = String::from_utf8_lossy(element.name().as_ref()).into_owned();
    let mut updated = BytesStart::new(name);
    let mut replaced = false;
    let mut attributes = Vec::new();
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|error| format!("XMP 属性无效：{error}"))?;
        let key = String::from_utf8_lossy(attribute.key.as_ref()).into_owned();
        let value = attribute
            .decode_and_unescape_value(decoder)
            .map_err(|error| format!("无法读取 XMP 属性：{error}"))?
            .into_owned();
        if attribute.key.local_name().as_ref() == b"Rating" {
            attributes.push((key, rating.to_string()));
            replaced = true;
        } else {
            attributes.push((key, value));
        }
    }
    if insert_if_missing && !replaced {
        attributes.push(("xmp:Rating".to_string(), rating.to_string()));
        replaced = true;
    }
    for (key, value) in &attributes {
        updated.push_attribute((key.as_str(), value.as_str()));
    }
    Ok((updated, replaced))
}

#[allow(dead_code)]
pub(crate) fn rewrite_xmp_rating(input: Option<&[u8]>, rating: u8) -> Result<Vec<u8>, String> {
    if rating > 5 {
        return Err("照片评分必须在 0 到 5 星之间".to_string());
    }
    let Some(input) = input else {
        let output = format!(
            r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?><x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:Rating="{rating}"/></rdf:RDF></x:xmpmeta><?xpacket end="w"?>"#,
        )
        .into_bytes();
        if output.len() as u64 > MAX_XMP_BYTES || xmp_rating(&output)? != Some(rating as i8) {
            return Err("无法生成有效的 XMP 评分".to_string());
        }
        return Ok(output);
    };

    if input.len() as u64 > MAX_XMP_BYTES {
        return Err(format!(
            "XMP 数据超过 {} MiB 限制",
            MAX_XMP_BYTES / 1024 / 1024
        ));
    }
    let current = xmp_rating(input)?;
    if current == Some(-1) {
        return Err("XMP 包含暂不支持的拒绝评分 -1".to_string());
    }

    let mut reader = Reader::from_reader(input);
    let mut writer = Writer::new(Vec::with_capacity(input.len().saturating_add(32)));
    let mut depth = 0_usize;
    let mut rating_depth = None;
    let mut rating_text_written = false;
    let mut description_found = false;
    let mut rating_updated = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) => {
                depth += 1;
                if element.local_name().as_ref() == b"Description" {
                    description_found = true;
                    let (updated, replaced) = rewritten_description(
                        &element,
                        reader.decoder(),
                        rating,
                        current.is_none() && !rating_updated,
                    )?;
                    rating_updated |= replaced;
                    writer
                        .write_event(Event::Start(updated))
                        .map_err(|error| format!("无法写入 XMP：{error}"))?;
                } else {
                    if element.local_name().as_ref() == b"Rating" {
                        rating_depth = Some(depth);
                        rating_text_written = false;
                        rating_updated = true;
                    }
                    writer
                        .write_event(Event::Start(element.into_owned()))
                        .map_err(|error| format!("无法写入 XMP：{error}"))?;
                }
            }
            Ok(Event::Empty(element)) => {
                if element.local_name().as_ref() == b"Description" {
                    description_found = true;
                    let (updated, replaced) = rewritten_description(
                        &element,
                        reader.decoder(),
                        rating,
                        current.is_none() && !rating_updated,
                    )?;
                    rating_updated |= replaced;
                    writer
                        .write_event(Event::Empty(updated))
                        .map_err(|error| format!("无法写入 XMP：{error}"))?;
                } else {
                    writer
                        .write_event(Event::Empty(element.into_owned()))
                        .map_err(|error| format!("无法写入 XMP：{error}"))?;
                }
            }
            Ok(Event::Text(text)) if rating_depth == Some(depth) => {
                if !rating_text_written {
                    writer
                        .write_event(Event::Text(BytesText::new(&rating.to_string())))
                        .map_err(|error| format!("无法写入 XMP Rating：{error}"))?;
                    rating_text_written = true;
                }
                let _ = text;
            }
            Ok(Event::End(element)) => {
                if element.local_name().as_ref() == b"Rating" && rating_depth == Some(depth) {
                    if !rating_text_written {
                        writer
                            .write_event(Event::Text(BytesText::new(&rating.to_string())))
                            .map_err(|error| format!("无法写入 XMP Rating：{error}"))?;
                    }
                    rating_depth = None;
                }
                writer
                    .write_event(Event::End(element.into_owned()))
                    .map_err(|error| format!("无法写入 XMP：{error}"))?;
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| "XMP XML 结束标签无效".to_string())?;
            }
            Ok(Event::Eof) => break,
            Ok(event) => writer
                .write_event(event.into_owned())
                .map_err(|error| format!("无法写入 XMP：{error}"))?,
            Err(error) => return Err(format!("XMP XML 无效：{error}")),
        }
    }

    if !description_found || !rating_updated {
        return Err("XMP 中没有可安全更新的 rdf:Description".to_string());
    }
    let output = writer.into_inner();
    if output.len() as u64 > MAX_XMP_BYTES {
        return Err(format!(
            "更新后的 XMP 超过 {} MiB 限制",
            MAX_XMP_BYTES / 1024 / 1024
        ));
    }
    if xmp_rating(&output)? != Some(rating as i8) {
        return Err("更新后的 XMP 评分校验失败".to_string());
    }
    Ok(output)
}
