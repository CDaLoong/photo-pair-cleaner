use base64::Engine;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};
use sha2::{Digest, Sha256};
use std::io::Cursor;
use std::path::Path;

const MAX_RESOURCE_BYTES: u64 = 32 * 1024 * 1024;

pub(crate) fn import_image_resource(
    path: &Path,
) -> Result<crate::watermark_model::EmbeddedTemplateResource, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("无法读取图片水印：{error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("图片水印必须是本地普通文件，不能使用符号链接".into());
    }
    if metadata.len() == 0 || metadata.len() > MAX_RESOURCE_BYTES {
        return Err("图片水印必须大于 0 字节且不能超过 32 MiB".into());
    }
    let bytes = std::fs::read(path).map_err(|error| format!("无法读取图片水印：{error}"))?;
    let format = image::guess_format(&bytes).map_err(|_| "无法识别图片水印格式".to_string())?;
    let mime_type = match format {
        ImageFormat::Png => "image/png",
        ImageFormat::Jpeg => "image/jpeg",
        _ => return Err("图片水印仅支持 PNG 或 JPEG".into()),
    };
    let reader = ImageReader::new(Cursor::new(bytes.as_slice()))
        .with_guessed_format()
        .map_err(|error| format!("无法识别图片水印：{error}"))?;
    let mut decoder = reader
        .into_decoder()
        .map_err(|error| format!("无法解码图片水印：{error}"))?;
    let orientation = decoder
        .orientation()
        .map_err(|error| format!("无法读取图片水印方向：{error}"))?;
    let mut decoded = DynamicImage::from_decoder(decoder)
        .map_err(|error| format!("无法解码图片水印：{error}"))?;
    decoded.apply_orientation(orientation);
    if decoded.width() == 0 || decoded.height() == 0 {
        return Err("图片水印尺寸无效".into());
    }
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "图片水印文件名无效".to_string())?
        .to_string();
    Ok(crate::watermark_model::EmbeddedTemplateResource {
        id: format!("image-{}", &sha256[..16]),
        name,
        mime_type: mime_type.to_string(),
        sha256,
        width: decoded.width(),
        height: decoded.height(),
        data_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
    })
}
