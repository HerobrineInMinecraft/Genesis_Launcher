//! # `src/app/icons.rs`
//!
//! **功能域：** 启动器应用外壳、页面状态、交互流程和原生 egui 绘制。
//!
//! 定义应用导航、操作与窗口控制图标，并使用矢量线段和几何形状绘制。
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `WindowAction`
//! - `ALL`
//! - `NavKind`
//! - `label`
//! - `NavIcon`
//! - `draw_icon`
//! - `paint_window_icon`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use super::*;

#[derive(Clone, Copy)]
/// 定义 WindowAction 可选状态或事件集合。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub(super) enum WindowAction {
    Minimize,
    Close,
}

impl WindowAction {
    /// 提供 全部 使用的只读常量。
    ///
    /// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
    pub(super) const ALL: [Self; 2] = [Self::Minimize, Self::Close];
}

#[derive(Clone, Copy)]
/// 定义 NavKind 可选状态或事件集合。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub(super) enum NavKind {
    Home,
    Versions,
    Download,
    Community,
    Create,
}

impl NavKind {
    /// 实现 标签 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `&'static str`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Home => "游戏",
            Self::Versions => "版本",
            Self::Download => "下载",
            Self::Community => "收藏",
            Self::Create => "创造",
        }
    }
}

#[derive(Clone, Copy)]
/// 定义 NavIcon 可选状态或事件集合。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub(super) enum NavIcon {
    Home,
    Layers,
    Download,
    Search,
    Box,
    Settings,
    Monitor,
    Chevron,
    Sparkles,
    User,
    Folder,
    Logout,
    Close,
}

impl From<NavKind> for NavIcon {
    /// 实现 从 相关的业务流程。
    ///
    /// # 参数
    /// - `value`：由调用方传入的value。
    /// # 返回
    /// 返回 `Self`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn from(value: NavKind) -> Self {
        match value {
            NavKind::Home => Self::Home,
            NavKind::Versions => Self::Layers,
            NavKind::Download => Self::Download,
            NavKind::Community => Self::Sparkles,
            NavKind::Create => Self::Box,
        }
    }
}

/// 实现 绘制图标 相关的业务流程。
///
/// # 参数
/// - `icon`：由调用方传入的图标。
/// - `rect`：由调用方传入的rect。
/// - `color`：由调用方传入的color。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn draw_icon(painter: &Painter, icon: NavIcon, rect: Rect, color: Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height());
    let color = theme::adaptive_text_color(color);
    let stroke = Stroke::new(1.65, color);
    match icon {
        NavIcon::Home => {
            painter.line(
                vec![
                    c + Vec2::new(-s * 0.38, -s * 0.04),
                    c + Vec2::new(0.0, -s * 0.38),
                    c + Vec2::new(s * 0.38, -s * 0.04),
                ],
                stroke,
            );
            painter.rect_stroke(
                Rect::from_center_size(c + Vec2::new(0.0, s * 0.10), Vec2::new(s * 0.62, s * 0.52)),
                2,
                stroke,
                StrokeKind::Inside,
            );
        }
        NavIcon::Layers => {
            painter.add(Shape::convex_polygon(
                vec![
                    c + Vec2::new(0.0, -s * 0.35),
                    c + Vec2::new(s * 0.38, -s * 0.13),
                    c + Vec2::new(0.0, s * 0.09),
                    c + Vec2::new(-s * 0.38, -s * 0.13),
                ],
                Color32::TRANSPARENT,
                stroke,
            ));
            painter.line(
                vec![
                    c + Vec2::new(-s * 0.35, s * 0.03),
                    c + Vec2::new(0.0, s * 0.23),
                    c + Vec2::new(s * 0.35, s * 0.03),
                ],
                stroke,
            );
            painter.line(
                vec![
                    c + Vec2::new(-s * 0.35, s * 0.21),
                    c + Vec2::new(0.0, s * 0.41),
                    c + Vec2::new(s * 0.35, s * 0.21),
                ],
                stroke,
            );
        }
        NavIcon::Download => {
            painter.line(
                vec![c + Vec2::new(0.0, -s * 0.40), c + Vec2::new(0.0, s * 0.20)],
                stroke,
            );
            painter.line(
                vec![
                    c + Vec2::new(-s * 0.22, s * 0.00),
                    c + Vec2::new(0.0, s * 0.23),
                    c + Vec2::new(s * 0.22, s * 0.00),
                ],
                stroke,
            );
            painter.line(
                vec![
                    c + Vec2::new(-s * 0.38, s * 0.30),
                    c + Vec2::new(-s * 0.38, s * 0.42),
                    c + Vec2::new(s * 0.38, s * 0.42),
                    c + Vec2::new(s * 0.38, s * 0.30),
                ],
                stroke,
            );
        }
        NavIcon::Search => {
            painter.circle_stroke(c + Vec2::new(-s * 0.10, -s * 0.10), s * 0.27, stroke);
            painter.line_segment(
                [
                    c + Vec2::new(s * 0.12, s * 0.12),
                    c + Vec2::new(s * 0.37, s * 0.37),
                ],
                stroke,
            );
        }
        NavIcon::Box => {
            painter.add(Shape::convex_polygon(
                vec![
                    c + Vec2::new(0.0, -s * 0.38),
                    c + Vec2::new(s * 0.38, -s * 0.18),
                    c + Vec2::new(0.0, s * 0.02),
                    c + Vec2::new(-s * 0.38, -s * 0.18),
                ],
                Color32::TRANSPARENT,
                stroke,
            ));
            painter.line(
                vec![
                    c + Vec2::new(-s * 0.38, -s * 0.18),
                    c + Vec2::new(-s * 0.38, s * 0.23),
                    c + Vec2::new(0.0, s * 0.44),
                    c + Vec2::new(s * 0.38, s * 0.23),
                    c + Vec2::new(s * 0.38, -s * 0.18),
                    c + Vec2::new(0.0, s * 0.02),
                    c + Vec2::new(0.0, s * 0.44),
                ],
                stroke,
            );
        }
        NavIcon::Settings => {
            painter.circle_stroke(c, s * 0.20, stroke);
            for index in 0..8 {
                let angle = std::f32::consts::TAU * index as f32 / 8.0;
                let direction = Vec2::angled(angle);
                painter.line_segment([c + direction * s * 0.29, c + direction * s * 0.41], stroke);
            }
            painter.circle_stroke(c, s * 0.31, stroke);
        }
        NavIcon::Monitor => {
            painter.rect_stroke(
                Rect::from_center_size(
                    c + Vec2::new(0.0, -s * 0.08),
                    Vec2::new(s * 0.76, s * 0.55),
                ),
                2,
                stroke,
                StrokeKind::Inside,
            );
            painter.line_segment(
                [c + Vec2::new(0.0, s * 0.20), c + Vec2::new(0.0, s * 0.36)],
                stroke,
            );
            painter.line_segment(
                [
                    c + Vec2::new(-s * 0.20, s * 0.37),
                    c + Vec2::new(s * 0.20, s * 0.37),
                ],
                stroke,
            );
        }
        NavIcon::Chevron => {
            painter.line(
                vec![
                    c + Vec2::new(-s * 0.22, -s * 0.10),
                    c + Vec2::new(0.0, s * 0.12),
                    c + Vec2::new(s * 0.22, -s * 0.10),
                ],
                stroke,
            );
        }
        NavIcon::Sparkles => {
            painter.line(
                vec![
                    c + Vec2::new(0.0, -s * 0.43),
                    c + Vec2::new(s * 0.11, -s * 0.11),
                    c + Vec2::new(s * 0.42, 0.0),
                    c + Vec2::new(s * 0.11, s * 0.11),
                    c + Vec2::new(0.0, s * 0.42),
                    c + Vec2::new(-s * 0.11, s * 0.11),
                    c + Vec2::new(-s * 0.42, 0.0),
                    c + Vec2::new(-s * 0.11, -s * 0.11),
                    c + Vec2::new(0.0, -s * 0.43),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    c + Vec2::new(s * 0.27, -s * 0.38),
                    c + Vec2::new(s * 0.27, -s * 0.18),
                ],
                Stroke::new(1.2, color),
            );
        }
        NavIcon::User => {
            painter.circle_stroke(c + Vec2::new(0.0, -s * 0.17), s * 0.18, stroke);
            painter.line(
                vec![
                    c + Vec2::new(-s * 0.34, s * 0.37),
                    c + Vec2::new(-s * 0.28, s * 0.13),
                    c + Vec2::new(0.0, s * 0.03),
                    c + Vec2::new(s * 0.28, s * 0.13),
                    c + Vec2::new(s * 0.34, s * 0.37),
                ],
                stroke,
            );
        }
        NavIcon::Folder => {
            painter.line(
                vec![
                    c + Vec2::new(-s * 0.40, -s * 0.24),
                    c + Vec2::new(-s * 0.12, -s * 0.24),
                    c + Vec2::new(-s * 0.02, -s * 0.12),
                    c + Vec2::new(s * 0.38, -s * 0.12),
                    c + Vec2::new(s * 0.38, s * 0.30),
                    c + Vec2::new(-s * 0.40, s * 0.30),
                    c + Vec2::new(-s * 0.40, -s * 0.24),
                ],
                stroke,
            );
        }
        NavIcon::Logout => {
            painter.line(
                vec![
                    c + Vec2::new(-s * 0.36, -s * 0.34),
                    c + Vec2::new(s * 0.08, -s * 0.34),
                    c + Vec2::new(s * 0.08, s * 0.34),
                    c + Vec2::new(-s * 0.36, s * 0.34),
                    c + Vec2::new(-s * 0.36, s * 0.16),
                ],
                stroke,
            );
            painter.line(
                vec![c + Vec2::new(-s * 0.10, 0.0), c + Vec2::new(s * 0.40, 0.0)],
                stroke,
            );
            painter.line(
                vec![
                    c + Vec2::new(s * 0.23, -s * 0.16),
                    c + Vec2::new(s * 0.40, 0.0),
                    c + Vec2::new(s * 0.23, s * 0.16),
                ],
                stroke,
            );
        }
        NavIcon::Close => {
            painter.line_segment(
                [
                    c + Vec2::new(-s * 0.28, -s * 0.28),
                    c + Vec2::new(s * 0.28, s * 0.28),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    c + Vec2::new(s * 0.28, -s * 0.28),
                    c + Vec2::new(-s * 0.28, s * 0.28),
                ],
                stroke,
            );
        }
    }
}

/// 实现 绘制窗口图标 相关的业务流程。
///
/// # 参数
/// - `center`：由调用方传入的center。
/// - `action`：由调用方传入的action。
/// - `color`：由调用方传入的color。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn paint_window_icon(
    painter: &Painter,
    center: Pos2,
    action: WindowAction,
    color: Color32,
) {
    let stroke = Stroke::new(1.0, color);
    match action {
        WindowAction::Minimize => {
            painter.line_segment(
                [center + Vec2::new(-5.0, 3.0), center + Vec2::new(5.0, 3.0)],
                stroke,
            );
        }
        WindowAction::Close => {
            painter.line_segment(
                [center + Vec2::new(-4.0, -4.0), center + Vec2::new(4.0, 4.0)],
                stroke,
            );
            painter.line_segment(
                [center + Vec2::new(4.0, -4.0), center + Vec2::new(-4.0, 4.0)],
                stroke,
            );
        }
    }
}
