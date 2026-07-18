use crate::watermark_model::MetadataPolicy;
use little_exif::exif_tag::ExifTag;
use little_exif::filetype::FileExtension;
use little_exif::ifd::ExifTagGroup;
use little_exif::metadata::Metadata;
use little_exif::rational::uR64;
use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use quick_xml::{Reader, Writer};
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

const EXIF_PREFIX: &[u8] = b"Exif\0\0";
const XMP_PREFIX: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
const MAX_JPEG_SIDECAR_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetadataTarget {
    Jpeg,
    Png,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExifField {
    CameraMake,
    CameraModel,
    LensModel,
    FocalLength,
    Aperture,
    ShutterSpeed,
    Iso,
    DateTime,
    Author,
    Copyright,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ExifValues {
    pub(crate) camera_make: Option<String>,
    pub(crate) camera_model: Option<String>,
    pub(crate) lens_model: Option<String>,
    pub(crate) focal_length: Option<String>,
    pub(crate) aperture: Option<String>,
    pub(crate) shutter_speed: Option<String>,
    pub(crate) iso: Option<String>,
    pub(crate) date_time: Option<String>,
    pub(crate) author: Option<String>,
    pub(crate) copyright: Option<String>,
}

impl ExifValues {
    fn from_metadata(metadata: &Metadata) -> Self {
        let mut values = Self::default();
        for tag in metadata {
            match tag {
                ExifTag::Make(value) if values.camera_make.is_none() => {
                    values.camera_make = clean_exif_string(value);
                }
                ExifTag::Model(value) if values.camera_model.is_none() => {
                    values.camera_model = clean_exif_string(value);
                }
                ExifTag::LensModel(value) if values.lens_model.is_none() => {
                    values.lens_model = clean_exif_string(value);
                }
                ExifTag::FocalLength(value) if values.focal_length.is_none() => {
                    values.focal_length =
                        first_rational(value).map(|number| format!("{} mm", format_number(number)));
                }
                ExifTag::FNumber(value) if values.aperture.is_none() => {
                    values.aperture =
                        first_rational(value).map(|number| format!("f/{}", format_number(number)));
                }
                ExifTag::ExposureTime(value) if values.shutter_speed.is_none() => {
                    values.shutter_speed = first_rational(value).map(format_shutter_speed);
                }
                ExifTag::ISO(value) if values.iso.is_none() => {
                    values.iso = value.first().map(|number| format!("ISO {number}"));
                }
                ExifTag::DateTimeOriginal(value) if values.date_time.is_none() => {
                    values.date_time = clean_exif_string(value);
                }
                ExifTag::Artist(value) if values.author.is_none() => {
                    values.author = clean_exif_string(value);
                }
                ExifTag::Copyright(value) if values.copyright.is_none() => {
                    values.copyright = clean_exif_string(value);
                }
                _ => {}
            }
        }
        values
    }

    fn get(&self, field: ExifField) -> Option<&str> {
        match field {
            ExifField::CameraMake => self.camera_make.as_deref(),
            ExifField::CameraModel => self.camera_model.as_deref(),
            ExifField::LensModel => self.lens_model.as_deref(),
            ExifField::FocalLength => self.focal_length.as_deref(),
            ExifField::Aperture => self.aperture.as_deref(),
            ExifField::ShutterSpeed => self.shutter_speed.as_deref(),
            ExifField::Iso => self.iso.as_deref(),
            ExifField::DateTime => self.date_time.as_deref(),
            ExifField::Author => self.author.as_deref(),
            ExifField::Copyright => self.copyright.as_deref(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct JpegSidecars {
    pub(crate) xmp_packets: Vec<Vec<u8>>,
    pub(crate) iptc_segments: Vec<Vec<u8>>,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedMetadata {
    target: MetadataTarget,
    exif: Option<Metadata>,
    sidecars: JpegSidecars,
}

impl PreparedMetadata {
    pub(crate) fn apply_to_encoded(&self, output: &mut Vec<u8>) -> Result<(), String> {
        let file_type = self.target.file_extension();
        panic_safe("清理输出元数据", || {
            Metadata::clear_metadata(output, file_type)
        })?;

        if self.target == MetadataTarget::Jpeg {
            panic_safe("清理输出 IPTC 元数据", || {
                Metadata::clear_app13_segment(output, FileExtension::JPEG)
            })?;
            *output = strip_jpeg_xmp(output)?;
        }

        if let Some(metadata) = &self.exif {
            panic_safe("写入输出 EXIF", || {
                metadata.write_to_vec(output, file_type)
            })?;
        }

        if self.target == MetadataTarget::Jpeg {
            insert_jpeg_sidecars(output, &self.sidecars)?;
        }
        Ok(())
    }
}

impl MetadataTarget {
    fn file_extension(self) -> FileExtension {
        match self {
            Self::Jpeg => FileExtension::JPEG,
            Self::Png => FileExtension::PNG {
                as_zTXt_chunk: false,
            },
        }
    }
}

pub(crate) fn read_exif_values(path: &Path) -> Result<ExifValues, String> {
    let bytes = fs::read(path).map_err(|error| format!("无法读取 JPG 元数据：{error}"))?;
    let metadata = parse_jpeg_exif(&bytes)?;
    Ok(ExifValues::from_metadata(&metadata))
}

pub(crate) fn format_exif_fields(
    fields: &[ExifField],
    separator: &str,
    values: &ExifValues,
    missing_value: Option<&str>,
) -> String {
    fields
        .iter()
        .filter_map(|field| {
            values
                .get(*field)
                .filter(|value| !value.is_empty())
                .or(missing_value.filter(|value| !value.is_empty()))
        })
        .collect::<Vec<_>>()
        .join(separator)
}

pub(crate) fn prepare_output_metadata(
    source: &Path,
    policy: MetadataPolicy,
    output_width: u32,
    output_height: u32,
    target: MetadataTarget,
) -> Result<PreparedMetadata, String> {
    if output_width == 0 || output_height == 0 {
        return Err("输出元数据尺寸必须大于 0".into());
    }

    if policy == MetadataPolicy::Remove {
        return Ok(PreparedMetadata {
            target,
            exif: None,
            sidecars: JpegSidecars::default(),
        });
    }

    let bytes = fs::read(source).map_err(|error| format!("无法读取 JPG 元数据：{error}"))?;
    let source_metadata = parse_jpeg_exif(&bytes)?;
    let exif_values = ExifValues::from_metadata(&source_metadata);
    let mut output_metadata = match policy {
        MetadataPolicy::Preserve => source_metadata,
        MetadataPolicy::Privacy => privacy_metadata(&source_metadata),
        MetadataPolicy::Remove => unreachable!(),
    };
    normalize_output_metadata(&mut output_metadata, output_width, output_height);

    let sidecars = if target == MetadataTarget::Jpeg {
        match policy {
            MetadataPolicy::Preserve => {
                let source_sidecars = extract_jpeg_sidecars(&bytes)?;
                let xmp_packets = source_sidecars
                    .xmp_packets
                    .iter()
                    .map(|packet| sanitize_preserved_xmp(packet))
                    .collect::<Result<Vec<_>, _>>()?;
                JpegSidecars {
                    xmp_packets,
                    iptc_segments: source_sidecars.iptc_segments,
                }
            }
            MetadataPolicy::Privacy => JpegSidecars {
                xmp_packets: privacy_xmp(&exif_values).into_iter().collect(),
                iptc_segments: Vec::new(),
            },
            MetadataPolicy::Remove => JpegSidecars::default(),
        }
    } else {
        JpegSidecars::default()
    };

    Ok(PreparedMetadata {
        target,
        exif: Some(output_metadata),
        sidecars,
    })
}

pub(crate) fn extract_jpeg_sidecars(jpeg: &[u8]) -> Result<JpegSidecars, String> {
    let mut sidecars = JpegSidecars::default();
    let mut total_bytes = 0usize;
    visit_jpeg_header_segments(jpeg, |marker, payload| {
        let item = if marker == 0xe1 && payload.starts_with(XMP_PREFIX) {
            Some((&mut sidecars.xmp_packets, &payload[XMP_PREFIX.len()..]))
        } else if marker == 0xed {
            Some((&mut sidecars.iptc_segments, payload))
        } else {
            None
        };

        if let Some((collection, data)) = item {
            total_bytes = total_bytes
                .checked_add(data.len())
                .ok_or_else(|| "JPG 附加元数据大小溢出".to_string())?;
            if total_bytes > MAX_JPEG_SIDECAR_BYTES {
                return Err("JPG 的 XMP/IPTC 元数据超过 4 MB 安全限制".into());
            }
            collection.push(data.to_vec());
        }
        Ok(())
    })?;
    Ok(sidecars)
}

fn parse_jpeg_exif(jpeg: &[u8]) -> Result<Metadata, String> {
    let mut exif_payload = None;
    visit_jpeg_header_segments(jpeg, |marker, payload| {
        if exif_payload.is_none() && marker == 0xe1 && payload.starts_with(EXIF_PREFIX) {
            exif_payload = Some(payload.to_vec());
        }
        Ok(())
    })?;

    let Some(payload) = exif_payload else {
        return Ok(Metadata::new());
    };
    let segment_length = payload
        .len()
        .checked_add(2)
        .filter(|length| *length <= u16::MAX as usize)
        .ok_or_else(|| "JPG EXIF 段大小无效".to_string())?;
    let mut isolated = Vec::with_capacity(payload.len() + 8);
    isolated.extend_from_slice(&[0xff, 0xd8, 0xff, 0xe1]);
    isolated.extend_from_slice(&(segment_length as u16).to_be_bytes());
    isolated.extend_from_slice(&payload);
    isolated.extend_from_slice(&[0xff, 0xd9]);

    panic_safe("解析 JPG EXIF", || {
        Metadata::new_from_vec(&isolated, FileExtension::JPEG)
    })
}

fn panic_safe<T, F>(label: &str, operation: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, std::io::Error>,
{
    catch_unwind(AssertUnwindSafe(operation))
        .map_err(|_| format!("{label}异常"))?
        .map_err(|error| format!("{label}失败：{error}"))
}

fn clean_exif_string(value: &str) -> Option<String> {
    let cleaned =
        value.trim_matches(|character: char| character == '\0' || character.is_whitespace());
    (!cleaned.is_empty()).then(|| cleaned.to_string())
}

fn first_rational(values: &[uR64]) -> Option<f64> {
    let value = values.first()?;
    if value.denominator == 0 {
        return None;
    }
    Some(value.nominator as f64 / value.denominator as f64)
}

fn format_number(value: f64) -> String {
    if (value - value.round()).abs() < 0.01 {
        format!("{value:.0}")
    } else {
        let formatted = format!("{value:.2}");
        formatted
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

fn format_shutter_speed(seconds: f64) -> String {
    if seconds <= 0.0 || !seconds.is_finite() {
        return "未知快门".into();
    }
    if seconds >= 1.0 {
        return format!("{} s", format_number(seconds));
    }
    let reciprocal = 1.0 / seconds;
    format!("1/{} s", format_number(reciprocal))
}

fn privacy_metadata(source: &Metadata) -> Metadata {
    let mut output = Metadata::new();
    for tag in source {
        if tag.get_group() == ExifTagGroup::GPS
            || matches!(
                tag,
                ExifTag::GPSInfo(_)
                    | ExifTag::OwnerName(_)
                    | ExifTag::SerialNumber(_)
                    | ExifTag::LensSerialNumber(_)
                    | ExifTag::ImageUniqueID(_)
                    | ExifTag::MakerNote(_)
                    | ExifTag::ThumbnailOffset(_, _)
                    | ExifTag::ThumbnailLength(_)
                    | ExifTag::StripOffsets(_, _)
                    | ExifTag::StripByteCounts(_)
                    | ExifTag::ExifOffset(_)
                    | ExifTag::InteropOffset(_)
            )
        {
            continue;
        }
        output.set_tag(tag.clone());
    }
    output
}

fn normalize_output_metadata(metadata: &mut Metadata, width: u32, height: u32) {
    metadata.remove_tag(ExifTag::ThumbnailOffset(Vec::new(), Vec::new()));
    metadata.remove_tag(ExifTag::ThumbnailLength(Vec::new()));
    metadata.remove_tag(ExifTag::ImageWidth(Vec::new()));
    metadata.remove_tag(ExifTag::ImageHeight(Vec::new()));
    metadata.remove_tag(ExifTag::ExifImageWidth(Vec::new()));
    metadata.remove_tag(ExifTag::ExifImageHeight(Vec::new()));
    metadata.remove_tag(ExifTag::Orientation(Vec::new()));
    metadata.set_tag(ExifTag::ImageWidth(vec![width]));
    metadata.set_tag(ExifTag::ImageHeight(vec![height]));
    metadata.set_tag(ExifTag::ExifImageWidth(vec![width]));
    metadata.set_tag(ExifTag::ExifImageHeight(vec![height]));
    metadata.set_tag(ExifTag::Orientation(vec![1]));
}

fn visit_jpeg_header_segments<F>(jpeg: &[u8], mut visitor: F) -> Result<(), String>
where
    F: FnMut(u8, &[u8]) -> Result<(), String>,
{
    if !jpeg.starts_with(&[0xff, 0xd8]) {
        return Err("文件不是有效的 JPG".into());
    }
    let mut position = 2usize;
    while position < jpeg.len() {
        if jpeg[position] != 0xff {
            return Err("JPG 段标记无效".into());
        }
        while position < jpeg.len() && jpeg[position] == 0xff {
            position += 1;
        }
        let marker = *jpeg
            .get(position)
            .ok_or_else(|| "JPG 段标记不完整".to_string())?;
        position += 1;

        if marker == 0xd9 || marker == 0xda {
            return Ok(());
        }
        if marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        let length_bytes = jpeg
            .get(position..position + 2)
            .ok_or_else(|| "JPG 段长度不完整".to_string())?;
        let length = u16::from_be_bytes([length_bytes[0], length_bytes[1]]) as usize;
        if length < 2 {
            return Err("JPG 段长度无效".into());
        }
        let payload_start = position + 2;
        let payload_end = payload_start
            .checked_add(length - 2)
            .filter(|end| *end <= jpeg.len())
            .ok_or_else(|| "JPG 段超出文件范围".to_string())?;
        visitor(marker, &jpeg[payload_start..payload_end])?;
        position = payload_end;
    }
    Err("JPG 文件缺少扫描或结束标记".into())
}

fn strip_jpeg_xmp(jpeg: &[u8]) -> Result<Vec<u8>, String> {
    rewrite_jpeg_header(jpeg, |marker, payload| {
        !(marker == 0xe1 && payload.starts_with(XMP_PREFIX))
    })
}

fn rewrite_jpeg_header<F>(jpeg: &[u8], keep: F) -> Result<Vec<u8>, String>
where
    F: Fn(u8, &[u8]) -> bool,
{
    if !jpeg.starts_with(&[0xff, 0xd8]) {
        return Err("输出文件不是有效的 JPG".into());
    }
    let mut output = jpeg[..2].to_vec();
    let mut position = 2usize;
    while position < jpeg.len() {
        let marker_start = position;
        if jpeg[position] != 0xff {
            return Err("输出 JPG 段标记无效".into());
        }
        while position < jpeg.len() && jpeg[position] == 0xff {
            position += 1;
        }
        let marker = *jpeg
            .get(position)
            .ok_or_else(|| "输出 JPG 段标记不完整".to_string())?;
        position += 1;
        if marker == 0xd9 || marker == 0xda {
            output.extend_from_slice(&jpeg[marker_start..]);
            return Ok(output);
        }
        if marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            output.extend_from_slice(&jpeg[marker_start..position]);
            continue;
        }
        let length_bytes = jpeg
            .get(position..position + 2)
            .ok_or_else(|| "输出 JPG 段长度不完整".to_string())?;
        let length = u16::from_be_bytes([length_bytes[0], length_bytes[1]]) as usize;
        if length < 2 {
            return Err("输出 JPG 段长度无效".into());
        }
        let segment_end = position
            .checked_add(length)
            .filter(|end| *end <= jpeg.len())
            .ok_or_else(|| "输出 JPG 段超出文件范围".to_string())?;
        let payload = &jpeg[position + 2..segment_end];
        if keep(marker, payload) {
            output.extend_from_slice(&jpeg[marker_start..segment_end]);
        }
        position = segment_end;
    }
    Err("输出 JPG 文件缺少扫描或结束标记".into())
}

fn insert_jpeg_sidecars(jpeg: &mut Vec<u8>, sidecars: &JpegSidecars) -> Result<(), String> {
    if !jpeg.starts_with(&[0xff, 0xd8]) {
        return Err("输出文件不是有效的 JPG".into());
    }
    let insertion_offset = jpeg_sidecar_insertion_offset(jpeg)?;
    let mut encoded = Vec::new();
    for packet in &sidecars.xmp_packets {
        let mut payload = Vec::with_capacity(XMP_PREFIX.len() + packet.len());
        payload.extend_from_slice(XMP_PREFIX);
        payload.extend_from_slice(packet);
        append_jpeg_segment(&mut encoded, 0xe1, &payload)?;
    }
    for payload in &sidecars.iptc_segments {
        append_jpeg_segment(&mut encoded, 0xed, payload)?;
    }
    jpeg.splice(insertion_offset..insertion_offset, encoded);
    Ok(())
}

fn jpeg_sidecar_insertion_offset(jpeg: &[u8]) -> Result<usize, String> {
    let mut position = 2usize;
    let mut fallback = 2usize;
    while position < jpeg.len() {
        let marker_start = position;
        if jpeg[position] != 0xff {
            return Err("输出 JPG 段标记无效".into());
        }
        while position < jpeg.len() && jpeg[position] == 0xff {
            position += 1;
        }
        let marker = *jpeg
            .get(position)
            .ok_or_else(|| "输出 JPG 段标记不完整".to_string())?;
        position += 1;
        if marker == 0xd9 || marker == 0xda {
            return Ok(fallback.max(marker_start));
        }
        if marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        let length_bytes = jpeg
            .get(position..position + 2)
            .ok_or_else(|| "输出 JPG 段长度不完整".to_string())?;
        let length = u16::from_be_bytes([length_bytes[0], length_bytes[1]]) as usize;
        if length < 2 {
            return Err("输出 JPG 段长度无效".into());
        }
        let segment_end = position
            .checked_add(length)
            .filter(|end| *end <= jpeg.len())
            .ok_or_else(|| "输出 JPG 段超出文件范围".to_string())?;
        let payload = &jpeg[position + 2..segment_end];
        if marker == 0xe1 && payload.starts_with(EXIF_PREFIX) {
            return Ok(segment_end);
        }
        if marker == 0xe0 {
            fallback = segment_end;
        }
        position = segment_end;
    }
    Err("输出 JPG 文件缺少扫描或结束标记".into())
}

fn append_jpeg_segment(output: &mut Vec<u8>, marker: u8, payload: &[u8]) -> Result<(), String> {
    let length = payload
        .len()
        .checked_add(2)
        .filter(|length| *length <= u16::MAX as usize)
        .ok_or_else(|| "JPG 附加元数据段过大".to_string())?;
    output.extend_from_slice(&[0xff, marker]);
    output.extend_from_slice(&(length as u16).to_be_bytes());
    output.extend_from_slice(payload);
    Ok(())
}

fn sanitize_preserved_xmp(packet: &[u8]) -> Result<Vec<u8>, String> {
    if packet.len() > MAX_JPEG_SIDECAR_BYTES {
        return Err("XMP 元数据超过 4 MB 安全限制".into());
    }
    let source = std::str::from_utf8(packet).map_err(|_| "XMP 不是有效的 UTF-8".to_string())?;
    let mut reader = Reader::from_str(source);
    reader.config_mut().check_end_names = true;
    let mut writer = Writer::new(Vec::new());
    let mut skipped_depth = 0usize;

    loop {
        let event = reader
            .read_event()
            .map_err(|error| format!("XMP 解析失败：{error}"))?;
        match event {
            Event::Eof => break,
            Event::DocType(_) => return Err("XMP 不允许包含文档类型声明".into()),
            Event::Start(start) => {
                if skipped_depth > 0 {
                    skipped_depth += 1;
                } else if is_stale_xmp_name(start.name().as_ref()) {
                    skipped_depth = 1;
                } else {
                    writer
                        .write_event(Event::Start(clean_xmp_attributes(&start)?))
                        .map_err(|error| format!("XMP 写入失败：{error}"))?;
                }
            }
            Event::Empty(start) => {
                if skipped_depth == 0 && !is_stale_xmp_name(start.name().as_ref()) {
                    writer
                        .write_event(Event::Empty(clean_xmp_attributes(&start)?))
                        .map_err(|error| format!("XMP 写入失败：{error}"))?;
                }
            }
            Event::End(end) => {
                if skipped_depth > 0 {
                    skipped_depth -= 1;
                } else {
                    writer
                        .write_event(Event::End(end.into_owned()))
                        .map_err(|error| format!("XMP 写入失败：{error}"))?;
                }
            }
            other if skipped_depth == 0 => writer
                .write_event(other.into_owned())
                .map_err(|error| format!("XMP 写入失败：{error}"))?,
            _ => {}
        }
    }
    let output = writer.into_inner();
    if output.len() > MAX_JPEG_SIDECAR_BYTES {
        return Err("处理后的 XMP 超过 4 MB 安全限制".into());
    }
    Ok(output)
}

fn clean_xmp_attributes(start: &BytesStart<'_>) -> Result<BytesStart<'static>, String> {
    let name = std::str::from_utf8(start.name().as_ref())
        .map_err(|_| "XMP 元素名不是有效的 UTF-8".to_string())?
        .to_string();
    let mut cleaned = BytesStart::new(name);
    for attribute in start.attributes().with_checks(true) {
        let attribute = attribute.map_err(|error| format!("XMP 属性无效：{error}"))?;
        if is_stale_xmp_name(attribute.key.as_ref()) {
            continue;
        }
        let key = std::str::from_utf8(attribute.key.as_ref())
            .map_err(|_| "XMP 属性名不是有效的 UTF-8".to_string())?;
        let value = std::str::from_utf8(attribute.value.as_ref())
            .map_err(|_| "XMP 属性值不是有效的 UTF-8".to_string())?;
        cleaned.push_attribute((key, value));
    }
    Ok(cleaned.into_owned())
}

fn is_stale_xmp_name(name: &[u8]) -> bool {
    let local = name.rsplit(|byte| *byte == b':').next().unwrap_or(name);
    matches!(
        local,
        b"ImageWidth"
            | b"ImageLength"
            | b"Orientation"
            | b"PixelXDimension"
            | b"PixelYDimension"
            | b"ExifImageWidth"
            | b"ExifImageHeight"
    )
}

fn privacy_xmp(values: &ExifValues) -> Option<Vec<u8>> {
    if values.author.is_none() && values.copyright.is_none() && values.date_time.is_none() {
        return None;
    }
    let mut writer = Writer::new(Vec::new());
    let mut root = BytesStart::new("x:xmpmeta");
    root.push_attribute(("xmlns:x", "adobe:ns:meta/"));
    writer.write_event(Event::Start(root)).ok()?;
    let mut rdf = BytesStart::new("rdf:RDF");
    rdf.push_attribute(("xmlns:rdf", "http://www.w3.org/1999/02/22-rdf-syntax-ns#"));
    rdf.push_attribute(("xmlns:dc", "http://purl.org/dc/elements/1.1/"));
    rdf.push_attribute(("xmlns:xmp", "http://ns.adobe.com/xap/1.0/"));
    writer.write_event(Event::Start(rdf)).ok()?;
    writer
        .write_event(Event::Start(BytesStart::new("rdf:Description")))
        .ok()?;
    if let Some(author) = &values.author {
        write_rdf_collection(&mut writer, "dc:creator", "rdf:Seq", author).ok()?;
    }
    if let Some(copyright) = &values.copyright {
        write_rdf_collection(&mut writer, "dc:rights", "rdf:Alt", copyright).ok()?;
    }
    if let Some(date_time) = &values.date_time {
        write_text_element(&mut writer, "xmp:CreateDate", date_time).ok()?;
    }
    writer
        .write_event(Event::End(BytesEnd::new("rdf:Description")))
        .ok()?;
    writer
        .write_event(Event::End(BytesEnd::new("rdf:RDF")))
        .ok()?;
    writer
        .write_event(Event::End(BytesEnd::new("x:xmpmeta")))
        .ok()?;
    Some(writer.into_inner())
}

fn write_rdf_collection(
    writer: &mut Writer<Vec<u8>>,
    element: &str,
    collection: &str,
    value: &str,
) -> Result<(), std::io::Error> {
    writer.write_event(Event::Start(BytesStart::new(element)))?;
    writer.write_event(Event::Start(BytesStart::new(collection)))?;
    write_text_element(writer, "rdf:li", value)?;
    writer.write_event(Event::End(BytesEnd::new(collection)))?;
    writer.write_event(Event::End(BytesEnd::new(element)))?;
    Ok(())
}

fn write_text_element(
    writer: &mut Writer<Vec<u8>>,
    element: &str,
    value: &str,
) -> Result<(), std::io::Error> {
    writer.write_event(Event::Start(BytesStart::new(element)))?;
    writer.write_event(Event::Text(BytesText::new(value)))?;
    writer.write_event(Event::End(BytesEnd::new(element)))?;
    Ok(())
}
