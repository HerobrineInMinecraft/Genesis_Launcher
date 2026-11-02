//! # `src/skin/default_pixels.rs`
//!
//! **功能域：** Minecraft 皮肤解码、编辑、保存和资源包安装。
//!
//! 生成默认玩家皮肤像素
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `default_skin_pixels`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

/// 实现 默认皮肤pixels 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Vec<u8>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn default_skin_pixels() -> Vec<u8> {
    let mut pixels = vec![0; 64 * 64 * 4];
    let skin = [204, 153, 119, 255];
    let skin_light = [231, 184, 147, 255];
    let hair = [61, 45, 36, 255];
    let hair_light = [82, 60, 44, 255];
    let shirt = [54, 137, 185, 255];
    let shirt_dark = [39, 106, 150, 255];
    let pants = [52, 77, 139, 255];
    let pants_light = [66, 94, 158, 255];
    let shoe = [66, 61, 56, 255];
    let eye_white = [239, 239, 231, 255];
    let eye = [39, 72, 94, 255];

    fill_skin_rect(&mut pixels, 8, 0, 8, 8, hair);
    fill_skin_rect(&mut pixels, 16, 0, 8, 8, skin);
    fill_skin_rect(&mut pixels, 0, 8, 8, 8, hair_light);
    fill_skin_rect(&mut pixels, 8, 8, 8, 8, skin);
    fill_skin_rect(&mut pixels, 16, 8, 8, 8, hair_light);
    fill_skin_rect(&mut pixels, 24, 8, 8, 8, hair);
    fill_skin_rect(&mut pixels, 8, 8, 8, 2, hair_light);
    fill_skin_rect(&mut pixels, 12, 11, 1, 1, skin_light);
    fill_skin_rect(&mut pixels, 10, 12, 1, 1, eye_white);
    fill_skin_rect(&mut pixels, 13, 12, 1, 1, eye_white);
    fill_skin_rect(&mut pixels, 10, 13, 1, 1, eye);
    fill_skin_rect(&mut pixels, 13, 13, 1, 1, eye);
    fill_skin_rect(&mut pixels, 11, 14, 2, 1, [125, 76, 61, 255]);

    for (x, y, width, height) in [
        (20, 16, 8, 4),
        (28, 16, 8, 4),
        (16, 20, 4, 12),
        (20, 20, 8, 12),
        (28, 20, 4, 12),
        (32, 20, 8, 12),
    ] {
        fill_skin_rect(&mut pixels, x, y, width, height, shirt);
    }
    fill_skin_rect(&mut pixels, 20, 20, 8, 2, shirt_dark);
    fill_skin_rect(&mut pixels, 16, 20, 4, 2, shirt_dark);
    fill_skin_rect(&mut pixels, 28, 20, 4, 2, shirt_dark);

    for (x, y, width, height) in [
        (44, 16, 4, 4),
        (48, 16, 4, 4),
        (40, 20, 4, 12),
        (44, 20, 4, 12),
        (48, 20, 4, 12),
        (52, 20, 4, 12),
    ] {
        fill_skin_rect(&mut pixels, x, y, width, height, skin);
    }
    fill_skin_rect(&mut pixels, 40, 20, 4, 3, shirt);
    fill_skin_rect(&mut pixels, 44, 20, 4, 3, shirt);
    fill_skin_rect(&mut pixels, 48, 20, 4, 3, shirt_dark);
    fill_skin_rect(&mut pixels, 52, 20, 4, 3, shirt);

    for (x, y, width, height) in [
        (36, 48, 4, 4),
        (40, 48, 4, 4),
        (32, 52, 4, 12),
        (36, 52, 4, 12),
        (40, 52, 4, 12),
        (44, 52, 4, 12),
    ] {
        fill_skin_rect(&mut pixels, x, y, width, height, skin);
    }
    fill_skin_rect(&mut pixels, 32, 52, 4, 3, shirt);
    fill_skin_rect(&mut pixels, 36, 52, 4, 3, shirt_dark);
    fill_skin_rect(&mut pixels, 40, 52, 4, 3, shirt);
    fill_skin_rect(&mut pixels, 44, 52, 4, 3, shirt_dark);

    for (x, y, width, height) in [
        (4, 16, 4, 4),
        (8, 16, 4, 4),
        (0, 20, 4, 12),
        (4, 20, 4, 12),
        (8, 20, 4, 12),
        (12, 20, 4, 12),
        (20, 48, 4, 4),
        (24, 48, 4, 4),
        (16, 52, 4, 12),
        (20, 52, 4, 12),
        (24, 52, 4, 12),
        (28, 52, 4, 12),
    ] {
        fill_skin_rect(&mut pixels, x, y, width, height, pants);
    }
    for (x, y) in [(0, 20), (8, 20), (16, 52), (24, 52)] {
        fill_skin_rect(&mut pixels, x, y + 1, 4, 11, pants_light);
    }
    for (x, y) in [
        (0, 29),
        (4, 29),
        (8, 29),
        (12, 29),
        (16, 61),
        (20, 61),
        (24, 61),
        (28, 61),
    ] {
        fill_skin_rect(&mut pixels, x, y, 4, 3, shoe);
    }
    pixels
}

/// 实现 fill皮肤rect 相关的业务流程。
///
/// # 参数
/// - `pixels`：由调用方传入的pixels。
/// - `x`：由调用方传入的x。
/// - `y`：由调用方传入的y。
/// - `width`：由调用方传入的width。
/// - `height`：由调用方传入的height。
/// - `color`：由调用方传入的color。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn fill_skin_rect(
    pixels: &mut [u8],
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    color: [u8; 4],
) {
    for py in y..(y + height).min(64) {
        for px in x..(x + width).min(64) {
            let offset = (py * 64 + px) * 4;
            pixels[offset..offset + 4].copy_from_slice(&color);
        }
    }
}
