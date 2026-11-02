//! # `src/theme/fonts.rs`
//!
//! **功能域：** 颜色、字体、玻璃表面、指针光效和屏幕空间着色器。
//!
//! 注册字体并创建统一文本样式
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `setup`
//! - `font`
//! - `bold_font`
//! - `mono_font`
//! - `optical_text_offset`
//! - `snap_text_position`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::sync::Arc;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily, FontId, Painter, Pos2, Stroke};

use super::{background, ink, line};

/// 提供 BOLDFAMILY 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const BOLD_FAMILY: &str = "launcher-bold";

/// 实现 setup 相关的业务流程。
///
/// # 参数
/// - `ctx`：由调用方传入的ctx。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn setup(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    load_font(
        &mut fonts,
        "cjk-regular",
        r"C:\Windows\Fonts\msyh.ttc",
        0,
        FontFamily::Proportional,
    );
    load_font(
        &mut fonts,
        "cjk-monospace",
        r"C:\Windows\Fonts\msyh.ttc",
        0,
        FontFamily::Monospace,
    );

    let family = FontFamily::Name(Arc::from(BOLD_FAMILY));
    if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\msyhbd.ttc") {
        fonts
            .font_data
            .insert("cjk-bold".to_owned(), Arc::new(FontData::from_owned(bytes)));
        fonts.families.insert(
            family,
            vec![
                "cjk-bold".to_owned(),
                "Ubuntu-Light".to_owned(),
                "Hack".to_owned(),
            ],
        );
    }
    ctx.set_fonts(fonts);

    let mut visuals = egui::Visuals::light();
    visuals.panel_fill = background();
    visuals.window_fill = egui::Color32::WHITE;
    visuals.extreme_bg_color = egui::Color32::from_rgb(238, 240, 237);
    visuals.override_text_color = Some(ink());
    visuals.selection.bg_fill = egui::Color32::from_rgba_unmultiplied(61, 150, 70, 74);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, line());
    ctx.set_visuals(visuals);

    // 保持 egui 的覆盖率羽化，并略微收紧贝塞尔曲线的平坦化误差。
    // 不扩大 feathering 尺寸，避免为了消除锯齿而让整个界面重新变糊。
    ctx.tessellation_options_mut(|options| {
        options.feathering = true;
        options.feathering_size_in_pixels = 1.0;
        options.prerasterized_discs = true;
        options.round_text_to_pixels = true;
        options.round_line_segments_to_pixels = true;
        options.round_rects_to_pixels = true;
        options.bezier_tolerance = 0.05;
    });

    ctx.style_mut_of(egui::Theme::Dark, |style| {
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.animation_time = 0.13;
        style.visuals.text_options.font_hinting = true;
        // 子像素装箱可减少小字号在非整数坐标上的跳字和毛边。
        style.visuals.text_options.subpixel_binning = true;
    });
}

/// 实现 加载字体 相关的业务流程。
///
/// # 参数
/// - `fonts`：由调用方传入的fonts。
/// - `name`：由调用方传入的名称。
/// - `path`：由调用方传入的路径。
/// - `index`：由调用方传入的index。
/// - `family`：由调用方传入的family。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn load_font(fonts: &mut FontDefinitions, name: &str, path: &str, index: u32, family: FontFamily) {
    let Ok(bytes) = std::fs::read(path) else {
        return;
    };
    let data = Arc::new(FontData::from_blob(Arc::new(bytes), index));
    fonts.font_data.insert(name.to_owned(), data);
    let priority = fonts.families.entry(family).or_default();
    priority.insert(0, name.to_owned());
}

/// 实现 字体 相关的业务流程。
///
/// # 参数
/// - `size`：由调用方传入的size。
/// # 返回
/// 返回 `FontId`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn font(size: f32) -> FontId {
    let size = tuned_font_size(size);
    let family = if size <= 11.0 {
        FontFamily::Name(Arc::from(BOLD_FAMILY))
    } else {
        FontFamily::Proportional
    };
    FontId::new(size, family)
}

/// 实现 bold字体 相关的业务流程。
///
/// # 参数
/// - `size`：由调用方传入的size。
/// # 返回
/// 返回 `FontId`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn bold_font(size: f32) -> FontId {
    FontId::new(
        tuned_font_size(size),
        FontFamily::Name(Arc::from(BOLD_FAMILY)),
    )
}

/// 实现 mono字体 相关的业务流程。
///
/// # 参数
/// - `size`：由调用方传入的size。
/// # 返回
/// 返回 `FontId`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn mono_font(size: f32) -> FontId {
    FontId::new(tuned_font_size(size), FontFamily::Monospace)
}

/// 实现 tuned字体size 相关的业务流程。
///
/// # 参数
/// - `size`：由调用方传入的size。
/// # 返回
/// 返回 `f32`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn tuned_font_size(size: f32) -> f32 {
    (size.max(9.2) * 1.04 * 2.0).round() / 2.0
}

/// 实现 optical文本offset 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `f32`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn optical_text_offset(size: f32) -> f32 {
    -(tuned_font_size(size) * 0.055).clamp(0.55, 1.35)
}

/// 实现 snap文本position 相关的业务流程。
///
/// # 参数
/// - `position`：由调用方传入的position。
/// # 返回
/// 返回 `Pos2`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn snap_text_position(painter: &Painter, position: Pos2) -> Pos2 {
    let pixels_per_point = painter.ctx().pixels_per_point();
    Pos2::new(
        (position.x * pixels_per_point).round() / pixels_per_point,
        (position.y * pixels_per_point).round() / pixels_per_point,
    )
}
