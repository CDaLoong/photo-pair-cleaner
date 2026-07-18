use moxcms::{ColorProfile, DataColorSpace, Layout, TransformOptions, curve_from_gamma};

const MAX_ICC_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OutputColorSpace {
    Srgb,
    SourceIcc(Vec<u8>),
}

pub(crate) fn linear_srgb_profile() -> ColorProfile {
    let mut profile = ColorProfile::new_srgb();
    let linear = Some(curve_from_gamma(1.0));
    profile.red_trc = linear.clone();
    profile.green_trc = linear.clone();
    profile.blue_trc = linear;
    profile
}

fn parse_rgb_profile(bytes: &[u8], label: &str) -> Result<ColorProfile, String> {
    if bytes.is_empty() || bytes.len() > MAX_ICC_BYTES {
        return Err(format!("{label} ICC 配置文件大小无效"));
    }
    let profile = ColorProfile::new_from_slice(bytes)
        .map_err(|error| format!("无法解析{label} ICC 配置文件：{error}"))?;
    if profile.color_space != DataColorSpace::Rgb {
        return Err(format!("{label} ICC 不是兼容的 RGB 配置文件"));
    }
    Ok(profile)
}

pub(crate) fn source_to_linear_srgb(
    pixels: &[u8],
    source_icc: Option<&[u8]>,
) -> Result<Vec<f32>, String> {
    if pixels.len() % 3 != 0 {
        return Err("RGB 像素数量必须是 3 的倍数".to_string());
    }
    let source_profile = match source_icc {
        Some(bytes) => parse_rgb_profile(bytes, "来源")?,
        None => ColorProfile::new_srgb(),
    };
    let linear_profile = linear_srgb_profile();
    let transform = source_profile
        .create_transform_f32(
            Layout::Rgb,
            &linear_profile,
            Layout::Rgb,
            TransformOptions::default(),
        )
        .map_err(|error| format!("无法创建来源颜色转换：{error}"))?;
    let source = pixels
        .iter()
        .map(|value| f32::from(*value) / 255.0)
        .collect::<Vec<_>>();
    let mut destination = vec![0.0; source.len()];
    transform
        .transform(&source, &mut destination)
        .map_err(|error| format!("无法转换来源颜色：{error}"))?;
    Ok(destination)
}

pub(crate) fn linear_srgb_to_output(
    pixels: &[f32],
    output: &OutputColorSpace,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    if pixels.len() % 3 != 0 {
        return Err("线性 RGB 像素数量必须是 3 的倍数".to_string());
    }
    let (destination_profile, destination_icc) = match output {
        OutputColorSpace::Srgb => {
            let profile = ColorProfile::new_srgb();
            let encoded = profile
                .encode()
                .map_err(|error| format!("无法生成 sRGB ICC 配置文件：{error}"))?;
            (profile, encoded)
        }
        OutputColorSpace::SourceIcc(bytes) => (parse_rgb_profile(bytes, "输出")?, bytes.clone()),
    };
    let transform = linear_srgb_profile()
        .create_transform_f32(
            Layout::Rgb,
            &destination_profile,
            Layout::Rgb,
            TransformOptions::default(),
        )
        .map_err(|error| format!("无法创建输出颜色转换：{error}"))?;
    let mut converted = vec![0.0; pixels.len()];
    transform
        .transform(pixels, &mut converted)
        .map_err(|error| format!("无法转换输出颜色：{error}"))?;
    let encoded = converted
        .into_iter()
        .map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8)
        .collect();
    Ok((encoded, destination_icc))
}

fn parse_hex_byte(value: &str, label: &str) -> Result<u8, String> {
    u8::from_str_radix(value, 16).map_err(|_| format!("{label}不是有效颜色"))
}

fn srgb_channel_to_linear(value: u8) -> f32 {
    let value = f32::from(value) / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

pub(crate) fn parse_css_color_linear(value: &str) -> Result<[f32; 4], String> {
    let value = value.trim();
    let hex = value
        .strip_prefix('#')
        .ok_or_else(|| format!("颜色 {value} 必须使用 #RRGGBB 或 #RRGGBBAA"))?;
    if hex.len() != 6 && hex.len() != 8 {
        return Err(format!("颜色 {value} 必须使用 #RRGGBB 或 #RRGGBBAA"));
    }
    let red = parse_hex_byte(&hex[0..2], value)?;
    let green = parse_hex_byte(&hex[2..4], value)?;
    let blue = parse_hex_byte(&hex[4..6], value)?;
    let alpha = if hex.len() == 8 {
        f32::from(parse_hex_byte(&hex[6..8], value)?) / 255.0
    } else {
        1.0
    };
    Ok([
        srgb_channel_to_linear(red),
        srgb_channel_to_linear(green),
        srgb_channel_to_linear(blue),
        alpha,
    ])
}
