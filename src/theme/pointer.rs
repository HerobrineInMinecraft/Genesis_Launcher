//! # `src/theme/pointer.rs`
//!
//! **功能域：** 颜色、字体、玻璃表面、指针光效和屏幕空间着色器。
//!
//! 绘制跟随鼠标的径向光效
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `pointer_glow`
//! - `pointer_glow_overlay`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use eframe::egui::{Color32, Painter, Pos2, Rect, Vec2};

use super::{pointer_brightness, pointer_radius, soft_orb};

/// 实现 指针glow 相关的业务流程。
///
/// # 参数
/// - `painter`：由调用方传入的painter。
/// - `position`：由调用方传入的position。
/// - `strength`：由调用方传入的strength。
/// - `_screen`：由调用方传入的screen。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn pointer_glow(painter: &Painter, position: Pos2, strength: f32, _screen: Rect) {
    let clip = painter.clone();
    let strength = strength.clamp(0.0, 1.0) * pointer_brightness();
    if strength <= 0.001 {
        return;
    }
    let radius_scale = pointer_radius() / 155.0;
    soft_orb(
        &clip,
        position + Vec2::new(-14.0, -10.0),
        178.0 * radius_scale,
        Color32::from_rgb(119, 230, 155),
        (24.0 * strength.min(1.5)).round().min(255.0) as u8,
    );
    soft_orb(
        &clip,
        position + Vec2::new(12.0, 8.0),
        78.0 * radius_scale,
        Color32::from_rgb(188, 255, 207),
        (15.0 * strength.min(1.5)).round().min(255.0) as u8,
    );
}

/// 实现 指针glowoverlay 相关的业务流程。
///
/// # 参数
/// - `painter`：由调用方传入的painter。
/// - `position`：由调用方传入的position。
/// - `strength`：由调用方传入的strength。
/// - `_screen`：由调用方传入的screen。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn pointer_glow_overlay(painter: &Painter, position: Pos2, strength: f32, _screen: Rect) {
    let clip = painter.clone();
    let strength = strength.clamp(0.0, 1.0) * pointer_brightness();
    if strength <= 0.001 {
        return;
    }
    let radius_scale = pointer_radius() / 155.0;
    soft_orb(
        &clip,
        position,
        118.0 * radius_scale,
        Color32::from_rgb(145, 246, 170),
        (20.0 * strength.min(1.5)).round().min(255.0) as u8,
    );
    soft_orb(
        &clip,
        position,
        48.0 * radius_scale,
        Color32::from_rgb(220, 255, 225),
        (15.0 * strength.min(1.5)).round().min(255.0) as u8,
    );
}
