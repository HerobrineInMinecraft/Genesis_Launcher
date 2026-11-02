//! # `src/skin/image.rs`
//!
//! **功能域：** Minecraft 皮肤解码、编辑、保存和资源包安装。
//!
//! 解码、校验和保存皮肤图片
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `load_player_skin`
//! - `decode_skin_image`
//! - `detect_slim_skin`
//! - `save_player_skin`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::fs;
use std::path::{Path, PathBuf};

use crate::brand;

/// 实现 加载玩家皮肤 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn load_player_skin(path: &Path) -> Result<Vec<u8>, String> {
    let image = image::open(path).map_err(|error| format!("无法读取皮肤文件：{error}"))?;
    decode_skin_image(image)
}

/// 实现 解码皮肤图片 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn decode_skin_image(image: image::DynamicImage) -> Result<Vec<u8>, String> {
    let (width, height) = (image.width(), image.height());
    let valid_dimensions = (64..=4096).contains(&width)
        && width % 64 == 0
        && (height == width || height.saturating_mul(2) == width);
    if !valid_dimensions {
        return Err(format!(
            "皮肤尺寸 {width}×{height} 无效；请使用 64×64、64×32，或保持相同比例的 64 倍数高清皮肤"
        ));
    }

    let target_height = if height == width { 64 } else { 32 };
    let resized = if width == 64 && height == target_height {
        image.to_rgba8()
    } else {
        image::imageops::resize(
            &image.to_rgba8(),
            64,
            target_height,
            image::imageops::FilterType::Nearest,
        )
    };
    if target_height == 64 {
        Ok(resized.into_raw())
    } else {
        Ok(expand_legacy_skin(resized.as_raw()))
    }
}

/// 实现 expandlegacy皮肤 相关的业务流程。
///
/// # 参数
/// - `legacy`：由调用方传入的legacy。
/// # 返回
/// 返回 `Vec<u8>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn expand_legacy_skin(legacy: &[u8]) -> Vec<u8> {
    let mut modern = vec![0; 64 * 64 * 4];
    modern[..64 * 32 * 4].copy_from_slice(legacy);
    copy_mirrored_skin_region(legacy, &mut modern, 40, 16, 32, 48);
    copy_mirrored_skin_region(legacy, &mut modern, 0, 16, 16, 48);
    modern
}

/// 实现 copymirrored皮肤region 相关的业务流程。
///
/// # 参数
/// - `source`：由调用方传入的source。
/// - `target`：由调用方传入的target。
/// - `source_x`：由调用方传入的source。
/// - `source_y`：由调用方传入的source。
/// - `target_x`：由调用方传入的target。
/// - `target_y`：由调用方传入的target。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn copy_mirrored_skin_region(
    source: &[u8],
    target: &mut [u8],
    source_x: usize,
    source_y: usize,
    target_x: usize,
    target_y: usize,
) {
    for y in 0..16 {
        for x in 0..16 {
            let source_offset = ((source_y + y) * 64 + source_x + (15 - x)) * 4;
            let target_offset = ((target_y + y) * 64 + target_x + x) * 4;
            target[target_offset..target_offset + 4]
                .copy_from_slice(&source[source_offset..source_offset + 4]);
        }
    }
}

/// 实现 检测slim皮肤 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `bool`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn detect_slim_skin(pixels: &[u8]) -> bool {
    let alpha_at = |x: usize, y: usize| pixels[(y * 64 + x) * 4 + 3];
    alpha_at(54, 20) == 0 && alpha_at(55, 20) == 0
}

/// 实现 保存玩家皮肤 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn save_player_skin(pixels: &[u8]) -> Result<PathBuf, String> {
    let directory = brand::data_directory();
    fs::create_dir_all(&directory).map_err(|error| format!("无法创建皮肤目录：{error}"))?;
    let path = directory.join("player-skin.png");
    let image = image::RgbaImage::from_raw(64, 64, pixels.to_vec())
        .ok_or_else(|| "皮肤像素数据无效".to_owned())?;
    image
        .save(&path)
        .map_err(|error| format!("无法保存玩家皮肤：{error}"))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skin::default_pixels::default_skin_pixels;

    #[test]
    /// 实现 decodeslegacy皮肤andfillsmirroredlimbs 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn decodes_legacy_skin_and_fills_mirrored_limbs() {
        let mut legacy = image::RgbaImage::from_pixel(64, 32, image::Rgba([40, 80, 120, 255]));
        legacy.put_pixel(40, 16, image::Rgba([210, 30, 70, 255]));
        let pixels =
            decode_skin_image(image::DynamicImage::ImageRgba8(legacy)).expect("valid legacy skin");
        assert_eq!(pixels.len(), 64 * 64 * 4);
        let mirrored = (48 * 64 + 47) * 4;
        assert_eq!(&pixels[mirrored..mirrored + 4], &[210, 30, 70, 255]);
    }

    #[test]
    /// 实现 decodes皮肤andrejectsinvalidaspectratio 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn decodes_hd_skin_and_rejects_invalid_aspect_ratio() {
        let hd = image::RgbaImage::from_pixel(128, 128, image::Rgba([20, 30, 40, 255]));
        let pixels = decode_skin_image(image::DynamicImage::ImageRgba8(hd)).unwrap();
        assert_eq!(pixels.len(), 64 * 64 * 4);
        let invalid = image::RgbaImage::from_pixel(128, 32, image::Rgba([20, 30, 40, 255]));
        assert!(decode_skin_image(image::DynamicImage::ImageRgba8(invalid)).is_err());
    }

    #[test]
    /// 实现 detectsslim皮肤从unusedarmcolumns 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn detects_slim_skin_from_unused_arm_columns() {
        let mut pixels = default_skin_pixels();
        assert!(!detect_slim_skin(&pixels));
        for x in [54, 55] {
            let offset = (20 * 64 + x) * 4;
            pixels[offset + 3] = 0;
        }
        assert!(detect_slim_skin(&pixels));
    }
}
