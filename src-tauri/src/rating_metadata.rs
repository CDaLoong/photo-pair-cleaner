use image::{ImageDecoder, ImageReader};
use quick_xml::Reader;
use quick_xml::Writer;
use quick_xml::encoding::Decoder;
use quick_xml::events::{BytesStart, BytesText, Event};
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

const MAX_XMP_BYTES: u64 = 4 * 1024 * 1024;
const JPEG_XMP_PREFIX: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";

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
                if element.local_name().as_ref() == b"Rating"
                    && rating_depth.replace(depth).is_some()
                {
                    return Err("XMP 中包含嵌套的 Rating".to_string());
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

    if !rating_updated || (current.is_none() && !description_found) {
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

type JpegXmpSegment<'a> = Option<(usize, usize, &'a [u8])>;

fn jpeg_xmp_segment(input: &[u8]) -> Result<JpegXmpSegment<'_>, String> {
    if input.len() < 4 || input[..2] != [0xff, 0xd8] {
        return Err("JPG 缺少有效的 SOI 文件头".to_string());
    }
    let mut offset = 2_usize;
    let mut found = None;
    let mut reached_image_data = false;

    while offset < input.len() {
        if input[offset] != 0xff {
            return Err("JPG 元数据段缺少标记前缀".to_string());
        }
        while offset < input.len() && input[offset] == 0xff {
            offset += 1;
        }
        if offset >= input.len() {
            return Err("JPG 标记未完整结束".to_string());
        }
        let marker = input[offset];
        let marker_start = offset - 1;
        offset += 1;
        match marker {
            0xda | 0xd9 => {
                reached_image_data = true;
                break;
            }
            0x01 | 0xd0..=0xd7 => continue,
            0x00 | 0xd8 => return Err("JPG 元数据段包含无效标记".to_string()),
            _ => {}
        }

        if offset + 2 > input.len() {
            return Err("JPG 元数据段缺少长度".to_string());
        }
        let segment_length = u16::from_be_bytes([input[offset], input[offset + 1]]) as usize;
        if segment_length < 2 {
            return Err("JPG 元数据段长度无效".to_string());
        }
        let segment_end = offset
            .checked_add(segment_length)
            .filter(|end| *end <= input.len())
            .ok_or_else(|| "JPG 元数据段超出了文件边界".to_string())?;
        let payload = &input[offset + 2..segment_end];
        if marker == 0xe1 && payload.starts_with(JPEG_XMP_PREFIX) {
            if found.is_some() {
                return Err("JPG 包含多个标准 XMP APP1 段".to_string());
            }
            found = Some((marker_start, segment_end, &payload[JPEG_XMP_PREFIX.len()..]));
        }
        offset = segment_end;
    }

    if !reached_image_data {
        return Err("JPG 缺少 SOS 或 EOI 结束标记".to_string());
    }
    Ok(found)
}

fn jpeg_xmp_app1(xml: &[u8]) -> Result<Vec<u8>, String> {
    let payload_length = JPEG_XMP_PREFIX
        .len()
        .checked_add(xml.len())
        .ok_or_else(|| "JPG XMP 大小溢出".to_string())?;
    if payload_length > u16::MAX as usize - 2 {
        return Err("更新后的 JPG XMP 超过 APP1 段大小限制".to_string());
    }
    let length = (payload_length as u16 + 2).to_be_bytes();
    let mut segment = Vec::with_capacity(payload_length + 4);
    segment.extend_from_slice(&[0xff, 0xe1, length[0], length[1]]);
    segment.extend_from_slice(JPEG_XMP_PREFIX);
    segment.extend_from_slice(xml);
    Ok(segment)
}

pub(crate) fn rewrite_jpeg_rating(input: &[u8], rating: u8) -> Result<Vec<u8>, String> {
    let existing = jpeg_xmp_segment(input)?;
    let xml = rewrite_xmp_rating(existing.map(|(_, _, xml)| xml), rating)?;
    let segment = jpeg_xmp_app1(&xml)?;
    let mut output = Vec::with_capacity(input.len().saturating_add(segment.len()));
    if let Some((start, end, _)) = existing {
        output.extend_from_slice(&input[..start]);
        output.extend_from_slice(&segment);
        output.extend_from_slice(&input[end..]);
    } else {
        output.extend_from_slice(&input[..2]);
        output.extend_from_slice(&segment);
        output.extend_from_slice(&input[2..]);
    }

    let (_, _, output_xml) =
        jpeg_xmp_segment(&output)?.ok_or_else(|| "更新后的 JPG 缺少 XMP APP1 段".to_string())?;
    if xmp_rating(output_xml)? != Some(rating as i8) {
        return Err("更新后的 JPG XMP 评分校验失败".to_string());
    }
    Ok(output)
}
