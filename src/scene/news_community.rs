//! # `src/scene/news_community.rs`
//!
//! **功能域：** 首页世界插画和新闻缩略图的矢量场景绘制。
//!
//! 绘制社区主题新闻缩略图
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `paint_community_art`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use eframe::egui::{Color32, Painter, Rect, Vec2};

/// 实现 绘制社区art 相关的业务流程。
///
/// # 参数
/// - `rect`：由调用方传入的rect。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn paint_community_art(painter: &Painter, rect: Rect) {
    let base = rect.center() + Vec2::new(0.0, 17.0);
    for (x, w, h, color) in [
        (-25.0, 20.0, 40.0, Color32::from_rgb(83, 98, 119)),
        (-4.0, 28.0, 54.0, Color32::from_rgb(97, 113, 136)),
        (25.0, 18.0, 32.0, Color32::from_rgb(73, 87, 107)),
    ] {
        painter.rect_filled(
            Rect::from_min_size(base + Vec2::new(x, -h), Vec2::new(w, h)),
            0,
            color,
        );
    }
    painter.rect_filled(
        Rect::from_min_size(base + Vec2::new(-31.0, 0.0), Vec2::new(77.0, 7.0)),
        0,
        Color32::from_rgb(49, 60, 79),
    );
    painter.circle_filled(
        rect.right_top() + Vec2::new(-22.0, 22.0),
        7.0,
        Color32::from_rgb(239, 219, 151),
    );
}
