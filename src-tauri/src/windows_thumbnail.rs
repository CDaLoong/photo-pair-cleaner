use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, GenericImageView, RgbImage};
use std::ffi::c_void;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::{RPC_E_CHANGED_MODE, SIZE};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDIBits, GetObjectW, HBITMAP, HDC, HGDIOBJ,
};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::UI::Shell::{
    IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_THUMBNAILONLY,
};
use windows::core::PCWSTR;

struct ComApartment(bool);

impl ComApartment {
    fn initialize() -> Result<Self, String> {
        let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if result == RPC_E_CHANGED_MODE {
            return Ok(Self(false));
        }
        result
            .ok()
            .map_err(|error| format!("无法初始化 Windows 图像组件：{error}"))?;
        Ok(Self(true))
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() };
        }
    }
}

struct OwnedBitmap(HBITMAP);

impl Drop for OwnedBitmap {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(self.0.0));
        }
    }
}

struct OwnedDc(HDC);

impl Drop for OwnedDc {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteDC(self.0);
        }
    }
}

fn shell_path(path: &Path) -> PathBuf {
    let value = path.to_string_lossy();
    if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = value.strip_prefix(r"\\?\") {
        return PathBuf::from(rest);
    }
    path.to_path_buf()
}

fn bitmap_rgb(bitmap: HBITMAP) -> Result<RgbImage, String> {
    let mut details = BITMAP::default();
    let object_size = i32::try_from(size_of::<BITMAP>())
        .map_err(|_| "Windows 位图信息尺寸超出范围".to_string())?;
    let copied = unsafe {
        GetObjectW(
            HGDIOBJ(bitmap.0),
            object_size,
            Some((&mut details as *mut BITMAP).cast::<c_void>()),
        )
    };
    if copied == 0 || details.bmWidth <= 0 || details.bmHeight == 0 {
        return Err("Windows 没有返回有效缩略图".to_string());
    }

    let width =
        u32::try_from(details.bmWidth).map_err(|_| "Windows 缩略图宽度超出范围".to_string())?;
    let height = details.bmHeight.unsigned_abs();
    let pixel_count = usize::try_from(u64::from(width) * u64::from(height))
        .map_err(|_| "Windows 缩略图像素数量超出范围".to_string())?;
    let byte_count = pixel_count
        .checked_mul(4)
        .ok_or_else(|| "Windows 缩略图缓冲区过大".to_string())?;
    let mut bgra = vec![0u8; byte_count];
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: u32::try_from(size_of::<BITMAPINFOHEADER>())
                .map_err(|_| "Windows 位图头尺寸超出范围".to_string())?,
            biWidth: details.bmWidth,
            biHeight: -i32::try_from(height)
                .map_err(|_| "Windows 缩略图高度超出范围".to_string())?,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            biSizeImage: u32::try_from(byte_count)
                .map_err(|_| "Windows 缩略图缓冲区超出范围".to_string())?,
            ..BITMAPINFOHEADER::default()
        },
        ..BITMAPINFO::default()
    };
    let dc = OwnedDc(unsafe { CreateCompatibleDC(None) });
    if dc.0.0.is_null() {
        return Err("无法创建 Windows 缩略图绘图上下文".to_string());
    }
    let lines = unsafe {
        GetDIBits(
            dc.0,
            bitmap,
            0,
            height,
            Some(bgra.as_mut_ptr().cast::<c_void>()),
            &mut info,
            DIB_RGB_COLORS,
        )
    };
    if lines != i32::try_from(height).unwrap_or(i32::MAX) {
        return Err("无法读取 Windows 缩略图像素".to_string());
    }

    let mut rgb = Vec::with_capacity(pixel_count * 3);
    for pixel in bgra.chunks_exact(4) {
        rgb.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
    }
    RgbImage::from_raw(width, height, rgb).ok_or_else(|| "Windows 缩略图像素布局无效".to_string())
}

pub(crate) fn load_system_thumbnail(source: &Path, max_edge: u32) -> Result<Vec<u8>, String> {
    let _apartment = ComApartment::initialize()?;
    let source = shell_path(source);
    let wide_path = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let image_factory: IShellItemImageFactory = unsafe {
        SHCreateItemFromParsingName(PCWSTR(wide_path.as_ptr()), None)
            .map_err(|error| format!("Windows 无法打开照片缩略图：{error}"))?
    };
    let edge = i32::try_from(max_edge).map_err(|_| "缩略图尺寸超出范围".to_string())?;
    let bitmap = OwnedBitmap(unsafe {
        image_factory
            .GetImage(
                SIZE { cx: edge, cy: edge },
                SIIGBF_THUMBNAILONLY | SIIGBF_BIGGERSIZEOK,
            )
            .map_err(|error| format!("Windows 无法生成照片缩略图：{error}"))?
    });
    let image = DynamicImage::ImageRgb8(bitmap_rgb(bitmap.0)?);
    let (width, height) = image.dimensions();
    if max_edge >= 1024
        && let Ok((source_width, source_height)) = image::image_dimensions(&source)
    {
        let expected_edge = max_edge.min(source_width.max(source_height));
        if u64::from(width.max(height)) * 10 < u64::from(expected_edge) * 9 {
            return Err("Windows 返回的系统缩略图尺寸不足".to_string());
        }
    }
    let image = if width > max_edge || height > max_edge {
        image.thumbnail(max_edge, max_edge)
    } else {
        image
    };
    let mut bytes = Vec::new();
    let quality = if max_edge >= 1024 { 95 } else { 88 };
    JpegEncoder::new_with_quality(&mut bytes, quality)
        .encode_image(&image)
        .map_err(|error| format!("无法编码 Windows 缩略图：{error}"))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, Rgb};

    #[test]
    fn system_thumbnail_is_bounded_and_decodable() {
        let temp = tempfile::tempdir().expect("temp directory");
        let source = temp.path().join("sample.jpg");
        let image = RgbImage::from_fn(2048, 1365, |x, y| {
            Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8])
        });
        image
            .save_with_format(&source, ImageFormat::Jpeg)
            .expect("write jpeg fixture");

        let bytes = load_system_thumbnail(&source, 256).expect("load system thumbnail");
        let thumbnail = image::load_from_memory(&bytes).expect("decode generated thumbnail");

        assert!(thumbnail.width() <= 256);
        assert!(thumbnail.height() <= 256);
        let source_ratio = 2048.0 / 1365.0;
        let thumbnail_ratio = f64::from(thumbnail.width()) / f64::from(thumbnail.height());
        assert!((source_ratio - thumbnail_ratio).abs() < 0.01);
    }
}
