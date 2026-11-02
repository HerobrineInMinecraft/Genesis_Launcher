//! # `src/app/layout.rs`
//!
//! **功能域：** 启动器应用外壳、页面状态、交互流程和原生 egui 绘制。
//!
//! 计算窗口、卡片和目标区域布局
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `Layout`
//! - `new`
//! - `profile_menu_rect`
//! - `account_name_dialog_rect`
//! - `version_folder_selector_rect`
//! - `version_folder_menu_rect`
//! - `version_content_dialog_rect`
//! - `version_content_close_rect`
//! - `java_picker_rect`
//! - `version_export_dialog_rect`
//! - `version_menu_rect`
//! - `resource_version_menu_rect`
//! - `favorite_editor_rect`
//! - `launch_more_rect`
//! - `launch_settings_card_rect`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use super::*;

/// 定义 布局 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub(super) struct Layout {
    pub(super) screen: Rect,
    pub(super) titlebar: Rect,
    pub(super) rail: Rect,
    pub(super) content_left: f32,
    pub(super) content_right: f32,
    pub(super) header: Rect,
    pub(super) version_trigger: Rect,
    pub(super) rail_profile: Rect,
    pub(super) hero: Rect,
    pub(super) latest: Rect,
    pub(super) news_cards: [Rect; 3],
}

impl Layout {
    /// 实现 new 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `Self`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn new(screen: Rect) -> Self {
        let rail_width = if screen.width() < 1120.0 {
            78.0
        } else {
            RAIL_WIDTH
        };
        let content_left = rail_width + CONTENT_PAD;
        let content_right = screen.right() - CONTENT_PAD;
        let rail_profile_size = 45.0;
        let rail_profile_margin = (rail_width - rail_profile_size) * 0.5;
        let header_top = TITLEBAR_HEIGHT + 19.0;
        let header = Rect::from_min_max(
            Pos2::new(content_left, header_top),
            Pos2::new(content_right, header_top + 54.0),
        );
        let version_trigger = Rect::from_min_size(
            Pos2::new(content_right - 170.0, header.top() + 4.0),
            Vec2::new(170.0, 46.0),
        );
        let bottom = screen.bottom() - 22.0;
        let latest = Rect::from_min_max(
            Pos2::new(content_left, bottom - 162.0),
            Pos2::new(content_right, bottom),
        );
        let hero = Rect::from_min_max(
            Pos2::new(content_left, header.bottom() + 14.0),
            Pos2::new(content_right, latest.top() - 14.0),
        );
        let cards_y = latest.top() + 43.0;
        let cards_height = bottom - cards_y;
        let gap = 10.0;
        let card_width = (content_right - content_left - gap * 2.0) / 3.0;
        let mut news_cards = [Rect::NOTHING; 3];
        for (index, card) in news_cards.iter_mut().enumerate() {
            let left = content_left + index as f32 * (card_width + gap);
            *card = Rect::from_min_size(
                Pos2::new(left, cards_y),
                Vec2::new(card_width, cards_height),
            );
        }

        Self {
            screen,
            titlebar: Rect::from_min_max(
                screen.left_top(),
                Pos2::new(screen.right(), TITLEBAR_HEIGHT),
            ),
            rail: Rect::from_min_max(
                Pos2::new(screen.left(), TITLEBAR_HEIGHT),
                Pos2::new(rail_width, screen.bottom()),
            ),
            rail_profile: Rect::from_center_size(
                Pos2::new(
                    rail_width * 0.5,
                    screen.bottom() - rail_profile_margin - rail_profile_size * 0.5,
                ),
                Vec2::splat(rail_profile_size),
            ),
            content_left,
            content_right,
            header,
            version_trigger,
            hero,
            latest,
            news_cards,
        }
    }
}

/// 实现 配置菜单rect 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Rect`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn profile_menu_rect(layout: &Layout) -> Rect {
    let size = Vec2::new(264.0, 260.0);
    let top = (layout.rail_profile.top() - size.y - 10.0).max(layout.titlebar.bottom() + 10.0);
    Rect::from_min_size(Pos2::new(layout.rail.right() + 12.0, top), size)
}

/// 实现 account名称dialogrect 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Rect`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn account_name_dialog_rect(screen: Rect) -> Rect {
    Rect::from_center_size(
        screen.center(),
        Vec2::new(420.0_f32.min(screen.width() - 30.0), 236.0),
    )
}

/// 实现 版本folderselectorrect 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Rect`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn version_folder_selector_rect(layout: &Layout) -> Rect {
    let page_top = layout.header.bottom() + 14.0;
    let list_width = ((layout.content_right - layout.content_left) * 0.49).clamp(430.0, 590.0);
    Rect::from_min_size(
        Pos2::new(layout.content_left + 22.0, page_top + 68.0),
        Vec2::new(list_width - 44.0, 34.0),
    )
}

/// 实现 版本folder菜单rect 相关的业务流程。
///
/// # 参数
/// - `directory_count`：由调用方传入的目录统计。
/// # 返回
/// 返回 `Rect`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn version_folder_menu_rect(layout: &Layout, directory_count: usize) -> Rect {
    let selector = version_folder_selector_rect(layout);
    let visible = directory_count.min(5);
    Rect::from_min_size(
        Pos2::new(selector.left(), selector.bottom() + 6.0),
        Vec2::new(selector.width(), 126.0 + visible as f32 * 42.0),
    )
}

/// 实现 版本内容dialogrect 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Rect`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn version_content_dialog_rect(screen: Rect) -> Rect {
    Rect::from_center_size(
        screen.center(),
        Vec2::new(
            760.0_f32.min(screen.width() - 34.0),
            560.0_f32.min(screen.height() - 42.0),
        ),
    )
}

/// 导出版本内容对话框中的格式选择器位置。
pub(super) fn export_format_selector_rect(dialog: Rect) -> Rect {
    let left = dialog.left() + 24.0;
    let top = dialog.top() + 76.0 + 5.0 * 64.0 + 4.0;
    Rect::from_min_max(
        Pos2::new(left, top),
        Pos2::new(dialog.right() - 24.0, top + 58.0),
    )
}

/// 导出格式下拉列表位置，向上展开以避开底部许可提示和按钮。
pub(super) fn export_format_menu_rect(format_row: Rect) -> Rect {
    let selector = Rect::from_min_size(
        Pos2::new(format_row.right() - 14.0 - 190.0, format_row.top() + 11.0),
        Vec2::new(190.0, 34.0),
    );
    Rect::from_min_size(
        Pos2::new(selector.right() - 282.0, format_row.top() - 6.0 - 98.0),
        Vec2::new(282.0, 98.0),
    )
}

/// 实现 版本内容关闭rect 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Rect`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn version_content_close_rect(dialog: Rect) -> Rect {
    Rect::from_center_size(
        Pos2::new(dialog.right() - 30.0, dialog.top() + 30.0),
        Vec2::splat(40.0),
    )
}

/// 实现 Javapickerrect 相关的业务流程。
///
/// # 参数
/// - `screen`：由调用方传入的screen。
/// - `option_count`：由调用方传入的option统计。
/// # 返回
/// 返回 `Rect`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn java_picker_rect(anchor: Rect, screen: Rect, option_count: usize) -> Rect {
    let visible = option_count.min(6);
    let width = 360.0_f32.min(screen.width() - 24.0);
    let height = 78.0 + visible as f32 * 42.0;
    let x = anchor.right() - width;
    let y = (anchor.bottom() + 6.0).min(screen.bottom() - height - 12.0);
    Rect::from_min_size(
        Pos2::new(
            x.clamp(screen.left() + 12.0, screen.right() - width - 12.0),
            y,
        ),
        Vec2::new(width, height),
    )
}

/// 实现 版本导出dialogrect 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Rect`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn version_export_dialog_rect(screen: Rect) -> Rect {
    Rect::from_center_size(
        screen.center(),
        Vec2::new(
            560.0_f32.min(screen.width() - 30.0),
            470.0_f32.min(screen.height() - 34.0),
        ),
    )
}

/// 实现 版本菜单rect 相关的业务流程。
///
/// # 参数
/// - `version_count`：由调用方传入的版本统计。
/// # 返回
/// 返回 `Rect`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn version_menu_rect(layout: &Layout, version_count: usize) -> Rect {
    let width = layout.version_trigger.width().max(286.0);
    let height = (35.0 + version_count as f32 * VERSION_ROW_HEIGHT).min(VERSION_MENU_MAX_HEIGHT);
    Rect::from_min_size(
        Pos2::new(
            layout.version_trigger.right() - width,
            layout.version_trigger.bottom() + 9.0,
        ),
        Vec2::new(width, height),
    )
}

/// 实现 资源版本菜单rect 相关的业务流程。
///
/// # 参数
/// - `screen`：由调用方传入的screen。
/// # 返回
/// 返回 `Rect`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn resource_version_menu_rect(anchor: Rect, screen: Rect) -> Rect {
    let width = 310.0_f32.min(screen.width() - 24.0);
    let height = 356.0_f32.min(screen.height() - 24.0);
    let x = anchor
        .right()
        .min(screen.right() - width - 12.0)
        .max(screen.left() + 12.0);
    let below = anchor.bottom() + 8.0;
    let y = if below + height <= screen.bottom() - 12.0 {
        below
    } else {
        (anchor.top() - height - 8.0).max(screen.top() + 12.0)
    };
    Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, height))
}

/// 实现 favoriteeditorrect 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Rect`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn favorite_editor_rect(screen: Rect) -> Rect {
    Rect::from_center_size(
        screen.center(),
        Vec2::new(440.0_f32.min(screen.width() - 30.0), 338.0),
    )
}

/// 实现 启动morerect 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Rect`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn launch_more_rect(layout: &Layout) -> Rect {
    let dock = Rect::from_min_max(
        Pos2::new(layout.hero.left() + 19.0, layout.hero.bottom() - 96.0),
        Pos2::new(layout.hero.right() - 19.0, layout.hero.bottom() - 18.0),
    );
    Rect::from_center_size(
        Pos2::new(dock.right() - 295.0, dock.center().y),
        Vec2::new(34.0, 34.0),
    )
}

/// 实现 启动settings卡片rect 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Rect`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn launch_settings_card_rect(layout: &Layout) -> Rect {
    let size = Vec2::new(380.0, 310.0);
    let more = launch_more_rect(layout);
    let top = (more.top() - size.y - 20.0).max(layout.header.bottom() + 10.0);
    let right = layout.hero.right() - 34.0;
    Rect::from_min_size(Pos2::new(right - size.x, top), size)
}
