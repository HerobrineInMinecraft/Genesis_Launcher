//! # `src/scene/cube.rs`
//!
//! **功能域：** 首页世界插画和新闻缩略图的矢量场景绘制。
//!
//! 绘制等距方块几何体
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `paint_cube`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use eframe::egui::{Color32, Painter, Pos2, Stroke, Vec2};

use crate::theme;

/// 实现 绘制方块 相关的业务流程。
///
/// # 参数
/// - `center`：由调用方传入的center。
/// - `size`：由调用方传入的size。
/// - `top`：由调用方传入的top。
/// - `left`：由调用方传入的left。
/// - `right`：由调用方传入的right。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn paint_cube(
    painter: &Painter,
    center: Pos2,
    size: f32,
    top: Color32,
    left: Color32,
    right: Color32,
) {
    let w = size * 0.50;
    let h = size * 0.24;
    let d = size * 0.52;
    painter.add(eframe::egui::Shape::convex_polygon(
        vec![
            center + Vec2::new(0.0, -h),
            center + Vec2::new(w, 0.0),
            center + Vec2::new(0.0, h),
            center + Vec2::new(-w, 0.0),
        ],
        top,
        Stroke::new(0.8, theme::alpha(Color32::WHITE, 25)),
    ));
    painter.add(eframe::egui::Shape::convex_polygon(
        vec![
            center + Vec2::new(-w, 0.0),
            center + Vec2::new(0.0, h),
            center + Vec2::new(0.0, h + d),
            center + Vec2::new(-w, d),
        ],
        left,
        Stroke::NONE,
    ));
    painter.add(eframe::egui::Shape::convex_polygon(
        vec![
            center + Vec2::new(w, 0.0),
            center + Vec2::new(0.0, h),
            center + Vec2::new(0.0, h + d),
            center + Vec2::new(w, d),
        ],
        right,
        Stroke::NONE,
    ));
}
