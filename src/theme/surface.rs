//! # `src/theme/surface.rs`
//!
//! **功能域：** 颜色、字体、玻璃表面、指针光效和屏幕空间着色器。
//!
//! 绘制圆角表面、文字、渐变和阴影
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `alpha`
//! - `black_alpha`
//! - `white_alpha`
//! - `fill_round`
//! - `stroke_round`
//! - `adaptive_text_color`
//! - `text`
//! - `bold_text`
//! - `center_text`
//! - `center_text_exact`
//! - `mono_text`
//! - `wrapped_text`
//! - `horizontal_fade`
//! - `soft_orb`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use eframe::egui::{self, Align2, Color32, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2};

use super::AppearanceMode;
use super::appearance_mode;
use super::contrast::{adaptive_text_color_at, register_surface};
use super::fonts::{bold_font, font, mono_font, optical_text_offset, snap_text_position};

/// 实现 alpha 相关的业务流程。
///
/// # 参数
/// - `color`：由调用方传入的color。
/// - `alpha`：由调用方传入的alpha。
/// # 返回
/// 返回 `Color32`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn alpha(color: Color32, alpha: u8) -> Color32 {
    let [r, g, b, _] = color.to_array();
    Color32::from_rgba_unmultiplied(r, g, b, alpha)
}

/// 实现 blackalpha 相关的业务流程。
///
/// # 参数
/// - `alpha`：由调用方传入的alpha。
/// # 返回
/// 返回 `Color32`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn black_alpha(alpha: u8) -> Color32 {
    Color32::from_black_alpha(alpha)
}

/// 实现 whitealpha 相关的业务流程。
///
/// # 参数
/// - `alpha`：由调用方传入的alpha。
/// # 返回
/// 返回 `Color32`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn white_alpha(alpha: u8) -> Color32 {
    Color32::from_black_alpha(alpha)
}

/// 实现 fillround 相关的业务流程。
///
/// # 参数
/// - `painter`：由调用方传入的painter。
/// - `rect`：由调用方传入的rect。
/// - `radius`：由调用方传入的radius。
/// - `color`：由调用方传入的color。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn fill_round(painter: &Painter, rect: Rect, radius: u8, color: Color32) {
    let painted_color = surface_color(color);
    painter.rect_filled(rect, radius, painted_color);
    register_surface(painter, rect, painted_color);
}

/// 实现 strokeround 相关的业务流程。
///
/// # 参数
/// - `painter`：由调用方传入的painter。
/// - `rect`：由调用方传入的rect。
/// - `radius`：由调用方传入的radius。
/// - `width`：由调用方传入的width。
/// - `color`：由调用方传入的color。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn stroke_round(painter: &Painter, rect: Rect, radius: u8, width: f32, color: Color32) {
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(width, stroke_color(color)),
        StrokeKind::Inside,
    );
}

/// 实现 表面color 相关的业务流程。
///
/// # 参数
/// - `color`：由调用方传入的color。
/// # 返回
/// 返回 `Color32`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn surface_color(color: Color32) -> Color32 {
    let [r, g, b, a] = color.to_array();
    let luminance = r as f32 * 0.2126 + g as f32 * 0.7152 + b as f32 * 0.0722;
    if a > 0 && luminance < 85.0 {
        Color32::from_rgba_unmultiplied(247, 248, 245, a.max(150))
    } else {
        color
    }
}

/// 实现 strokecolor 相关的业务流程。
///
/// # 参数
/// - `color`：由调用方传入的color。
/// # 返回
/// 返回 `Color32`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn stroke_color(color: Color32) -> Color32 {
    let [r, g, b, a] = color.to_array();
    let luminance = r as f32 * 0.2126 + g as f32 * 0.7152 + b as f32 * 0.0722;
    if luminance > 145.0 {
        Color32::from_black_alpha(a.min(80))
    } else {
        color
    }
}

/// 实现 文本 相关的业务流程。
///
/// # 参数
/// - `painter`：由调用方传入的painter。
/// - `position`：由调用方传入的position。
/// - `value`：由调用方传入的value。
/// - `size`：由调用方传入的size。
/// - `color`：由调用方传入的color。
/// # 返回
/// 返回 `Rect`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn text(
    painter: &Painter,
    position: Pos2,
    value: impl ToString,
    size: f32,
    color: Color32,
) -> Rect {
    let color = adaptive_text_color_at(painter, position, color, size);
    painter.text(
        snap_text_position(
            painter,
            position + Vec2::new(0.0, optical_text_offset(size)),
        ),
        Align2::LEFT_TOP,
        value,
        font(size),
        color,
    )
}

/// 实现 bold文本 相关的业务流程。
///
/// # 参数
/// - `painter`：由调用方传入的painter。
/// - `position`：由调用方传入的position。
/// - `value`：由调用方传入的value。
/// - `size`：由调用方传入的size。
/// - `color`：由调用方传入的color。
/// # 返回
/// 返回 `Rect`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn bold_text(
    painter: &Painter,
    position: Pos2,
    value: impl ToString,
    size: f32,
    color: Color32,
) -> Rect {
    let color = adaptive_text_color_at(painter, position, color, size);
    painter.text(
        snap_text_position(
            painter,
            position + Vec2::new(0.0, optical_text_offset(size)),
        ),
        Align2::LEFT_TOP,
        value,
        bold_font(size),
        color,
    )
}

/// 实现 center文本 相关的业务流程。
///
/// # 参数
/// - `painter`：由调用方传入的painter。
/// - `center`：由调用方传入的center。
/// - `value`：由调用方传入的value。
/// - `size`：由调用方传入的size。
/// - `color`：由调用方传入的color。
/// - `bold`：由调用方传入的bold。
/// # 返回
/// 返回 `Rect`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn center_text(
    painter: &Painter,
    center: Pos2,
    value: impl ToString,
    size: f32,
    color: Color32,
    bold: bool,
) -> Rect {
    let color = adaptive_text_color_at(painter, center, color, size);
    painter.text(
        snap_text_position(
            painter,
            center + Vec2::new(0.0, optical_text_offset(size) * 0.45),
        ),
        Align2::CENTER_CENTER,
        value,
        if bold { bold_font(size) } else { font(size) },
        color,
    )
}

/// 实现 center文本exact 相关的业务流程。
///
/// # 参数
/// - `painter`：由调用方传入的painter。
/// - `center`：由调用方传入的center。
/// - `value`：由调用方传入的value。
/// - `size`：由调用方传入的size。
/// - `color`：由调用方传入的color。
/// - `bold`：由调用方传入的bold。
/// # 返回
/// 返回 `Rect`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn center_text_exact(
    painter: &Painter,
    center: Pos2,
    value: impl ToString,
    size: f32,
    color: Color32,
    bold: bool,
) -> Rect {
    let color = adaptive_text_color_at(painter, center, color, size);
    painter.text(
        snap_text_position(
            painter,
            center + Vec2::new(0.0, optical_text_offset(size) * 0.45),
        ),
        Align2::CENTER_CENTER,
        value,
        if bold { bold_font(size) } else { font(size) },
        color,
    )
}

/// 实现 mono文本 相关的业务流程。
///
/// # 参数
/// - `painter`：由调用方传入的painter。
/// - `position`：由调用方传入的position。
/// - `value`：由调用方传入的value。
/// - `size`：由调用方传入的size。
/// - `color`：由调用方传入的color。
/// # 返回
/// 返回 `Rect`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn mono_text(
    painter: &Painter,
    position: Pos2,
    value: impl ToString,
    size: f32,
    color: Color32,
) -> Rect {
    let color = adaptive_text_color_at(painter, position, color, size);
    painter.text(
        snap_text_position(
            painter,
            position + Vec2::new(0.0, optical_text_offset(size)),
        ),
        Align2::LEFT_TOP,
        value,
        mono_font(size),
        color,
    )
}

/// 实现 wrapped文本 相关的业务流程。
///
/// # 参数
/// - `painter`：由调用方传入的painter。
/// - `position`：由调用方传入的position。
/// - `value`：由调用方传入的value。
/// - `size`：由调用方传入的size。
/// - `color`：由调用方传入的color。
/// - `wrap_width`：由调用方传入的wrapwidth。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn wrapped_text(
    painter: &Painter,
    position: Pos2,
    value: impl Into<String>,
    size: f32,
    color: Color32,
    wrap_width: f32,
) {
    let color = adaptive_text_color_at(painter, position, color, size);
    let galley = painter.layout(value.into(), font(size), color, wrap_width);
    painter.galley(
        snap_text_position(
            painter,
            position + Vec2::new(0.0, optical_text_offset(size)),
        ),
        galley,
        color,
    );
}

/// 实现 horizontalfade 相关的业务流程。
///
/// # 参数
/// - `painter`：由调用方传入的painter。
/// - `rect`：由调用方传入的rect。
/// - `left`：由调用方传入的left。
/// - `middle`：由调用方传入的middle。
/// - `right`：由调用方传入的right。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn horizontal_fade(
    painter: &Painter,
    rect: Rect,
    left: Color32,
    middle: Color32,
    right: Color32,
) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), left);
    mesh.colored_vertex(rect.center_top(), middle);
    mesh.colored_vertex(rect.right_top(), right);
    mesh.colored_vertex(rect.left_bottom(), left);
    mesh.colored_vertex(rect.center_bottom(), middle);
    mesh.colored_vertex(rect.right_bottom(), right);
    mesh.add_triangle(0, 1, 4);
    mesh.add_triangle(0, 4, 3);
    mesh.add_triangle(1, 2, 5);
    mesh.add_triangle(1, 5, 4);
    painter.add(Shape::mesh(mesh));
}

/// 实现 softorb 相关的业务流程。
///
/// # 参数
/// - `painter`：由调用方传入的painter。
/// - `center`：由调用方传入的center。
/// - `radius`：由调用方传入的radius。
/// - `color`：由调用方传入的color。
/// - `opacity`：由调用方传入的opacity。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn soft_orb(painter: &Painter, center: Pos2, radius: f32, color: Color32, opacity: u8) {
    if appearance_mode() == AppearanceMode::Simple {
        return;
    }
    /// 提供 RINGS 使用的只读常量。
    ///
    /// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
    const RINGS: [f32; 5] = [0.0, 0.32, 0.62, 0.85, 1.0];
    /// 提供 ALPHAS 使用的只读常量。
    ///
    /// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
    const ALPHAS: [f32; 5] = [1.0, 0.68, 0.34, 0.10, 0.0];
    let segments =
        ((radius * painter.ctx().pixels_per_point() * 0.5).ceil() as usize).clamp(48, 128);
    let [r, g, b, _] = color.to_array();
    let mut mesh = egui::Mesh::default();
    for (ring, (radius_scale, alpha_scale)) in RINGS.iter().zip(ALPHAS).enumerate() {
        let ring_radius = radius * radius_scale;
        let alpha = (opacity as f32 * alpha_scale).round() as u8;
        for segment in 0..segments {
            let angle = std::f32::consts::TAU * segment as f32 / segments as f32;
            mesh.colored_vertex(
                center + Vec2::angled(angle) * ring_radius,
                Color32::from_rgba_unmultiplied(r, g, b, alpha),
            );
            if ring > 0 {
                let previous = (ring - 1) * segments + segment;
                let previous_next = (ring - 1) * segments + (segment + 1) % segments;
                let current = ring * segments + segment;
                let current_next = ring * segments + (segment + 1) % segments;
                mesh.add_triangle(previous as u32, current as u32, previous_next as u32);
                mesh.add_triangle(previous_next as u32, current as u32, current_next as u32);
            }
        }
    }
    painter.add(Shape::mesh(mesh));
}
