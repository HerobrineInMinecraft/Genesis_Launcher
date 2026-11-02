//! # `src/scene/news_version.rs`
//!
//! **功能域：** 首页世界插画和新闻缩略图的矢量场景绘制。
//!
//! 绘制版本主题新闻缩略图
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `paint_version_art`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use eframe::egui::{Color32, Painter, Rect, Stroke, Vec2};

use crate::theme;

/// 实现 绘制版本art 相关的业务流程。
///
/// # 参数
/// - `rect`：由调用方传入的rect。
/// - `time`：由调用方传入的time。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn paint_version_art(painter: &Painter, rect: Rect, time: f64) {
    let center = rect.center() + Vec2::new(0.0, (time * 0.7).sin() as f32 * 1.5);
    let half = rect.width().min(rect.height()) * 0.18;
    painter.add(eframe::egui::Shape::convex_polygon(
        vec![
            center + Vec2::new(0.0, -half),
            center + Vec2::new(half, 0.0),
            center + Vec2::new(0.0, half),
            center + Vec2::new(-half, 0.0),
        ],
        Color32::from_rgb(78, 198, 194),
        Stroke::new(1.0, theme::alpha(Color32::WHITE, 105)),
    ));
    painter.add(eframe::egui::Shape::convex_polygon(
        vec![
            center + Vec2::new(0.0, -half * 0.52),
            center + Vec2::new(half * 0.52, 0.0),
            center + Vec2::new(0.0, half * 0.52),
            center + Vec2::new(-half * 0.52, 0.0),
        ],
        theme::alpha(Color32::from_rgb(190, 246, 238), 90),
        Stroke::NONE,
    ));
}
