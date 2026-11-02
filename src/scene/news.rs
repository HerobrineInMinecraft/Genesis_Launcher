//! # `src/scene/news.rs`
//!
//! **功能域：** 首页世界插画和新闻缩略图的矢量场景绘制。
//!
//! 组织新闻缩略图背景、主题和描边
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `paint_news_art`
//! - `NewsArt`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use eframe::egui::{Color32, Painter, Rect, Stroke, StrokeKind};

use crate::theme;

use super::news_community::paint_community_art;
use super::news_mod::paint_mod_art;
use super::news_version::paint_version_art;

/// 实现 绘制新闻art 相关的业务流程。
///
/// # 参数
/// - `painter`：由调用方传入的painter。
/// - `rect`：由调用方传入的rect。
/// - `kind`：由调用方传入的kind。
/// - `time`：由调用方传入的time。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn paint_news_art(painter: &Painter, rect: Rect, kind: NewsArt, time: f64) {
    let clip = painter.clone();
    let start = match kind {
        NewsArt::Version => Color32::from_rgb(25, 62, 55),
        NewsArt::Mod => Color32::from_rgb(39, 72, 44),
        NewsArt::Community => Color32::from_rgb(39, 51, 76),
    };
    theme::fill_round(painter, rect, theme::corner_radius(), start);
    clip.circle_filled(rect.center(), rect.width() * 0.42, theme::white_alpha(8));

    match kind {
        NewsArt::Version => paint_version_art(&clip, rect, time),
        NewsArt::Mod => paint_mod_art(&clip, rect, time),
        NewsArt::Community => paint_community_art(&clip, rect),
    }

    clip.rect_stroke(
        rect,
        theme::corner_radius(),
        Stroke::new(1.0, theme::line()),
        StrokeKind::Inside,
    );
}

#[derive(Clone, Copy)]
/// 定义 NewsArt 可选状态或事件集合。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub enum NewsArt {
    Version,
    Mod,
    Community,
}
