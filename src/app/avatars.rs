//! # `src/app/avatars.rs`
//!
//! **功能域：** 启动器应用外壳、页面状态、交互流程和原生 egui 绘制。
//!
//! 绘制玩家头像和方块占位图
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `paint_avatar`
//! - `paint_block`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use super::*;

/// 实现 绘制头像 相关的业务流程。
///
/// # 参数
/// - `rect`：由调用方传入的rect。
/// - `radius`：由调用方传入的radius。
/// - `skin_texture`：由调用方传入的皮肤texture。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn paint_avatar(
    painter: &Painter,
    rect: Rect,
    radius: u8,
    skin_texture: Option<&egui::TextureHandle>,
) {
    if let Some(texture) = skin_texture {
        theme::fill_round(painter, rect, radius, Color32::from_rgb(21, 34, 25));
        theme::soft_orb(
            painter,
            Pos2::new(rect.center().x + 3.0, rect.center().y + 4.0),
            rect.width() * 0.52,
            theme::GREEN,
            9,
        );
        let stage = rect.shrink(1.5);
        let scale = stage.height() / 14.0;
        let faces = skin_head_preview_faces(stage, scale, -0.48, -0.14);
        paint_skin_model(painter, texture, &faces);
        theme::stroke_round(painter, rect, radius, 1.0, theme::alpha(Color32::BLACK, 52));
        return;
    }
    theme::fill_round(painter, rect, radius, Color32::from_rgb(211, 160, 110));
    let hair_height = rect.height() * 0.30;
    painter.rect_filled(
        Rect::from_min_size(rect.left_top(), Vec2::new(rect.width(), hair_height)),
        0,
        Color32::from_rgb(66, 47, 34),
    );
    painter.rect_filled(
        Rect::from_min_size(
            rect.left_top() + Vec2::new(0.0, hair_height),
            Vec2::new(rect.width() * 0.16, rect.height() * 0.42),
        ),
        0,
        Color32::from_rgb(72, 50, 38),
    );
    let eye_size = Vec2::new(rect.width() * 0.14, rect.height() * 0.13);
    for eye_x in [0.27, 0.61] {
        painter.rect_filled(
            Rect::from_min_size(
                rect.left_top() + Vec2::new(rect.width() * eye_x, rect.height() * 0.46),
                eye_size,
            ),
            0,
            Color32::from_rgb(70, 55, 77),
        );
    }
    painter.rect_filled(
        Rect::from_min_size(
            rect.left_top() + Vec2::new(rect.width() * 0.39, rect.height() * 0.72),
            Vec2::new(rect.width() * 0.23, rect.height() * 0.08),
        ),
        0,
        Color32::from_rgb(112, 70, 51),
    );
    theme::stroke_round(painter, rect, radius, 1.0, theme::alpha(Color32::BLACK, 52));
}

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
pub(super) fn paint_block(
    painter: &Painter,
    center: Pos2,
    size: f32,
    top: Color32,
    left: Color32,
    right: Color32,
) {
    let half_width = size * 0.45;
    let half_height = size * 0.24;
    let depth = size * 0.37;
    painter.add(Shape::convex_polygon(
        vec![
            center + Vec2::new(0.0, -half_height),
            center + Vec2::new(half_width, 0.0),
            center + Vec2::new(0.0, half_height),
            center + Vec2::new(-half_width, 0.0),
        ],
        top,
        Stroke::new(0.7, theme::white_alpha(36)),
    ));
    painter.add(Shape::convex_polygon(
        vec![
            center + Vec2::new(-half_width, 0.0),
            center + Vec2::new(0.0, half_height),
            center + Vec2::new(0.0, half_height + depth),
            center + Vec2::new(-half_width, depth),
        ],
        left,
        Stroke::NONE,
    ));
    painter.add(Shape::convex_polygon(
        vec![
            center + Vec2::new(half_width, 0.0),
            center + Vec2::new(0.0, half_height),
            center + Vec2::new(0.0, half_height + depth),
            center + Vec2::new(half_width, depth),
        ],
        right,
        Stroke::NONE,
    ));
}
