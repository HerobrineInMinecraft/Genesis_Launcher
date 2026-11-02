//! # `src/scene/news_mod.rs`
//!
//! **功能域：** 首页世界插画和新闻缩略图的矢量场景绘制。
//!
//! 绘制模组主题新闻缩略图
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `paint_mod_art`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use eframe::egui::{Color32, Painter, Rect, Vec2};

use super::cube::paint_cube;

/// 实现 绘制模组art 相关的业务流程。
///
/// # 参数
/// - `rect`：由调用方传入的rect。
/// - `time`：由调用方传入的time。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn paint_mod_art(painter: &Painter, rect: Rect, time: f64) {
    for (index, offset) in [
        Vec2::new(-11.0, -13.0),
        Vec2::new(5.0, 10.0),
        Vec2::new(19.0, -16.0),
    ]
    .into_iter()
    .enumerate()
    {
        let bob = (time * 1.2 + index as f64).sin() as f32 * 2.0;
        let center = rect.center() + offset + Vec2::Y * bob;
        let size = rect.width() * if index == 0 { 0.20 } else { 0.13 };
        paint_cube(
            painter,
            center,
            size,
            Color32::from_rgb(138, 213, 105),
            Color32::from_rgb(69, 128, 68),
            Color32::from_rgb(83, 151, 77),
        );
    }
}
