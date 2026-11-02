//! # `src/app/ui_helpers.rs`
//!
//! **功能域：** 启动器应用外壳、页面状态、交互流程和原生 egui 绘制。
//!
//! 提供设置卡片、滑块、状态标签和通用按钮
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `paint_ambient_background`
//! - `paint_setting_card`
//! - `paint_setting_label`
//! - `paint_toggle_row`
//! - `paint_effect_slider`
//! - `skin_rgb_hex`
//! - `paint_compact_value_slider`
//! - `paint_skin_rgb_slider`
//! - `paint_status_badge`
//! - `paint_page_heading`
//! - `page_card_anchor`
//! - `page_kicker`
//! - `page_title`
//! - `page_status_label`
//! - `compact_icon_button`
//! - `settings_button`
//! - `skin_history_button`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

// 启动器 app 模块的 UI 绘制辅助函数集合（pub(super)，仅 app 模块内部可见）。
//
// 本文件封装大量与 egui 直接交互的"绘制"函数，把静态卡片、开关、滑块、
// 标签、徽章、页面标题、图标按钮等常见控件的几何与配色集中实现，供各页面复用。
// 这些函数通常接收一个 `Painter`（立即模式绘制）或一个 `&mut egui::Ui`（用于交互命中），
// 并依赖 `theme` 模块统一的外观规范（圆角、玻璃、绿色主色、文字色等）。

use super::*;

/// 绘制首页"环境背景"占位（当前为空实现，预留给未来的动态背景）。
///
/// 参数 `_painter` 画布、`_screen` 屏幕矩形、`_time` 时间（秒），均暂未使用。
pub(super) fn paint_ambient_background(_painter: &Painter, _screen: Rect, _time: f64) {}

/// 返回设置行内部实际卡片的矩形。
pub(super) fn setting_card_rect(row: Rect) -> Rect {
    row.shrink2(Vec2::new(0.0, 4.5))
}

/// 计算右对齐设置控件位置，使控件到卡片顶部、底部和右侧的距离相等。
pub(super) fn setting_right_control_rect(row: Rect, size: Vec2) -> Rect {
    let card = setting_card_rect(row);
    let margin = ((card.height() - size.y) * 0.5).max(8.0);
    Rect::from_min_size(
        Pos2::new(card.right() - margin - size.x, card.top() + margin),
        size,
    )
}

/// 绘制设置页的一行"玻璃卡片"背景。
///
/// 参数 `painter` 画布、`row` 该行矩形、`time` 动画时间（用于玻璃光泽流动）。
/// 卡片在行内上下各缩进 4.5px；高度不足 1px 时跳过绘制。
pub(super) fn paint_setting_card(painter: &Painter, row: Rect, time: f64) {
    // 行内上下各缩 4.5px 形成卡片间距
    let card = setting_card_rect(row);
    if card.height() <= 1.0 {
        return;
    }
    // 用深绿底色 + 主题绿边框绘制玻璃质感面板；row.top()*0.01 作为光泽相位偏移
    theme::glass_panel(
        painter,
        card,
        theme::corner_radius(),
        theme::GlassSpec::new(
            Color32::from_rgb(20, 32, 24),
            theme::GREEN,
            time,
            row.top() * 0.01,
        ),
    );
}

/// 在设置卡片内绘制标题（加粗）与描述（次要色）两行文字。
///
/// 参数 `painter` 画布、`row` 卡片矩形、`title` 标题、`description` 描述。
/// 先裁剪到卡片内；标题在左侧内边距 + 上 18px、字号 11；描述在上 40px、字号 8.2。
pub(super) fn paint_setting_label(painter: &Painter, row: Rect, title: &str, description: &str) {
    // 裁剪到卡片，避免文字溢出圆角
    let content = card_clip(painter, row);
    let painter = &content;
    // 标题与左右文字使用同一 14px 卡片内边距，顶部偏移包含行到卡片的 4.5px 间距。
    theme::bold_text(
        painter,
        Pos2::new(
            row.left() + SETTING_CARD_PADDING_X,
            row.top() + SETTING_CARD_CONTENT_TOP_OFFSET,
        ),
        title,
        11.0,
        theme::ink(),
    );
    // 描述：常规、次要灰
    theme::text(
        painter,
        Pos2::new(
            row.left() + SETTING_CARD_PADDING_X,
            row.top() + SETTING_CARD_CONTENT_TOP_OFFSET + 22.0,
        ),
        description,
        8.2,
        theme::muted(),
    );
}

/// 绘制一个开关（Toggle）行：左侧标签 + 右侧开关控件，点击切换 `value`。
///
/// 参数：`ui` 用于交互命中；`painter` 画布；`row` 行矩形；`id` 唯一交互标识；
/// `title`/`description` 标签；`value` 为开关状态（in/out，点击取反）。
/// 返回值为副作用（直接在函数内翻转 `value`），无显式返回值。
pub(super) fn paint_toggle_row(
    ui: &mut egui::Ui,
    painter: &Painter,
    row: Rect,
    id: &'static str,
    title: &str,
    description: &str,
    value: &mut bool,
) {
    let content = card_clip(painter, row);
    let painter = &content;
    // 开关到卡片顶部、底部、右侧的距离保持一致。
    let toggle = setting_right_control_rect(row, Vec2::new(48.0, 27.0));
    // 文字区域严格截止在开关左侧，避免长描述穿过控件或贴近卡片右边界。
    let text_width = (toggle.left() - row.left() - SETTING_CARD_PADDING_X - 12.0).max(72.0);
    let title = fit_text_to_width(painter, title, theme::bold_font(10.4), text_width);
    let description = fit_text_to_width(painter, description, theme::font(8.0), text_width);
    theme::bold_text(
        painter,
        Pos2::new(
            row.left() + SETTING_CARD_PADDING_X,
            row.top() + SETTING_CARD_CONTENT_TOP_OFFSET,
        ),
        title,
        10.4,
        theme::ink(),
    );
    theme::text(
        painter,
        Pos2::new(
            row.left() + SETTING_CARD_PADDING_X,
            row.top() + SETTING_CARD_CONTENT_TOP_OFFSET + 20.0,
        ),
        description,
        8.0,
        theme::muted(),
    );
    // 注册点击交互，悬停手型光标
    let response = ui
        .interact(toggle, Id::unique(id), Sense::click())
        .on_hover_cursor(CursorIcon::PointingHand);
    // 开启时填主题绿，关闭时填深灰
    theme::fill_round(
        painter,
        toggle,
        theme::corner_radius(),
        if *value {
            theme::GREEN
        } else {
            Color32::from_rgb(45, 58, 49)
        },
    );
    // 细描边
    theme::stroke_round(
        painter,
        toggle,
        theme::corner_radius(),
        1.0,
        theme::white_alpha(30),
    );
    // 旋钮 x：开启靠右 13.5、关闭靠左 13.5
    let knob_x = if *value {
        toggle.right() - 13.5
    } else {
        toggle.left() + 13.5
    };
    // 旋钮圆形：开启近白、关闭灰
    painter.circle_filled(
        Pos2::new(knob_x, toggle.center().y),
        9.5,
        if *value {
            Color32::from_rgb(236, 255, 230)
        } else {
            Color32::from_rgb(140, 153, 144)
        },
    );
    // 点击后翻转开关状态
    if response.clicked() {
        *value = !*value;
    }
}

/// 绘制一个带数值显示的"效果强度"滑块（用于设置页，如模糊强度）。
///
/// 参数：`ui` 交互；`painter` 画布；`row` 行矩形；`id` 唯一标识；`title`/`description` 标签；
/// `value` 当前值（in/out，按 step 吸附）；`minimum`/`maximum`/`step` 范围与步进；
/// `suffix` 数值后缀（如 "%"）。
/// 点击或拖动按指针 x 位置换算比例并更新值（四舍五入到 step），随后请求重绘。
#[allow(clippy::too_many_arguments)]
pub(super) fn paint_effect_slider(
    ui: &mut egui::Ui,
    painter: &Painter,
    row: Rect,
    id: &'static str,
    title: &str,
    description: &str,
    value: &mut u16,
    minimum: u16,
    maximum: u16,
    step: u16,
    suffix: &str,
) {
    let content = card_clip(painter, row);
    let painter = &content;
    // 内容区左右内边距缩进
    let content = row.shrink2(Vec2::new(SETTING_CARD_PADDING_X, 0.0));
    // 左上标题
    theme::bold_text(
        painter,
        Pos2::new(content.left(), row.top() + SETTING_CARD_CONTENT_TOP_OFFSET),
        title,
        9.0,
        theme::ink_soft(),
    );
    // 右上当前值（加粗、浅绿）
    let value_position = Pos2::new(content.right(), row.top() + SETTING_CARD_CONTENT_TOP_OFFSET);
    let value_color = theme::adaptive_text_color_at(
        painter,
        value_position,
        Color32::from_rgb(180, 221, 173),
        9.0,
    );
    painter.text(
        value_position,
        egui::Align2::RIGHT_TOP,
        format!("{value}{suffix}"),
        theme::bold_font(9.0),
        value_color,
    );
    // 可选描述
    if !description.is_empty() {
        theme::text(
            painter,
            Pos2::new(
                content.left(),
                row.top() + SETTING_CARD_CONTENT_TOP_OFFSET + 18.0,
            ),
            description,
            7.3,
            theme::muted(),
        );
    }
    // 轨道：上 49px、高 5px，跨内容宽度
    let track = Rect::from_min_size(
        Pos2::new(content.left(), row.top() + 49.0),
        Vec2::new(content.width(), 5.0),
    );
    // 命中区在轨道上下各扩大 7px，便于点击/拖动
    let hit = Rect::from_min_max(
        Pos2::new(track.left(), track.top() - 7.0),
        Pos2::new(track.right(), track.bottom() + 7.0),
    );
    // 注册点击+拖动交互
    let response = ui
        .interact(hit, Id::unique(id), Sense::click_and_drag())
        .on_hover_cursor(CursorIcon::PointingHand);
    // 点击或拖动时按指针位置换算并吸附到 step
    if (response.clicked() || response.dragged())
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let ratio = ((pointer.x - track.left()) / track.width()).clamp(0.0, 1.0);
        let raw = minimum as f32 + ratio * (maximum - minimum) as f32;
        // 四舍五入到 step 并夹在 [min,max]
        *value = (((raw / step as f32).round() as u16) * step).clamp(minimum, maximum);
        ui.ctx().request_repaint();
    }
    // 计算当前比例并绘制轨道底与已填充部分
    let ratio = (*value - minimum) as f32 / (maximum - minimum).max(1) as f32;
    theme::fill_round(painter, track, 4, theme::white_alpha(18));
    theme::fill_round(
        painter,
        Rect::from_min_size(
            track.left_top(),
            Vec2::new(track.width() * ratio, track.height()),
        ),
        4,
        if response.hovered() {
            theme::GREEN_BRIGHT
        } else {
            theme::GREEN
        },
    );
    // 滑块圆点
    painter.circle_filled(
        Pos2::new(track.left() + track.width() * ratio, track.center().y),
        6.0,
        Color32::from_rgb(238, 255, 232),
    );
}

/// 把皮肤编辑中的 RGB 颜色（含 alpha）格式化为 `#RRGGBB` 字符串（忽略 alpha）。
///
/// 参数 `color` 为 `[r,g,b,a]`。返回大写 6 位十六进制（带 # 前缀）。
pub(super) fn skin_rgb_hex(color: [u8; 4]) -> String {
    format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2])
}

/// 绘制紧凑数值滑块（小尺寸，常用于皮肤/外观等密集设置）。
///
/// 参数：`ui` 交互；`painter` 画布；`row` 行矩形；`id` 唯一标识；`label` 标签；
/// `value` 当前值（in/out）；`minimum`/`maximum` 范围；`suffix` 后缀；`accent` 强调色。
/// 点击/拖动按指针 x 更新值（四舍五入），绘制轨道、填充与滑块圆点。
#[allow(clippy::too_many_arguments)]
pub(super) fn paint_compact_value_slider(
    ui: &mut egui::Ui,
    painter: &Painter,
    row: Rect,
    id: &'static str,
    label: &str,
    value: &mut u16,
    minimum: u16,
    maximum: u16,
    suffix: &str,
    accent: Color32,
) {
    let content = card_clip(painter, row);
    let painter = &content;
    // 左侧标签（强调色、小字号）
    theme::bold_text(
        painter,
        Pos2::new(row.left(), row.top() + 3.0),
        label,
        7.0,
        accent,
    );
    // 右侧当前值
    let value_position = Pos2::new(row.right(), row.top() + 3.0);
    let value_color =
        theme::adaptive_text_color_at(painter, value_position, theme::ink_soft(), 7.0);
    painter.text(
        value_position,
        egui::Align2::RIGHT_TOP,
        format!("{value}{suffix}"),
        theme::bold_font(7.0),
        value_color,
    );
    // 轨道：左侧留 22px 给标签，宽度取 行宽-82 与 48 的较大值，高 4px
    let track = Rect::from_min_size(
        Pos2::new(row.left() + 22.0, row.center().y + 2.0),
        Vec2::new((row.width() - 82.0).max(48.0), 4.0),
    );
    let hit = Rect::from_min_max(
        Pos2::new(track.left(), track.top() - 6.0),
        Pos2::new(track.right(), track.bottom() + 6.0),
    );
    let response = ui
        .interact(hit, Id::unique(id), Sense::click_and_drag())
        .on_hover_cursor(CursorIcon::PointingHand);
    if (response.clicked() || response.dragged())
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let ratio = ((pointer.x - track.left()) / track.width()).clamp(0.0, 1.0);
        *value = (minimum as f32 + ratio * (maximum - minimum) as f32).round() as u16;
        ui.ctx().request_repaint();
    }
    // 当前比例（夹在范围内）
    let ratio = ((*value).clamp(minimum, maximum).saturating_sub(minimum)) as f32
        / maximum.saturating_sub(minimum).max(1) as f32;
    theme::fill_round(painter, track, 3, theme::white_alpha(18));
    theme::fill_round(
        painter,
        Rect::from_min_size(
            track.left_top(),
            Vec2::new(track.width() * ratio, track.height()),
        ),
        3,
        if response.hovered() {
            theme::GREEN_BRIGHT
        } else {
            accent
        },
    );
    painter.circle_filled(
        Pos2::new(track.left() + track.width() * ratio, track.center().y),
        3.6,
        Color32::from_rgb(236, 248, 235),
    );
}

/// 绘制皮肤 RGB 单通道滑块（0–255），仅在 `active` 时允许交互。
///
/// 参数：`ui` 交互；`painter` 画布；`row` 行矩形；`id` 唯一标识；`label` 标签；
/// `value` 当前通道值（in/out，0–255）；`accent` 强调色（同时作为填充色）；
/// `active` 是否为可编辑状态（false 时仅展示、不响应指针）。
/// 交互时按指针 x 换算到 0–255 并四舍五入更新值。
#[allow(clippy::too_many_arguments)]
pub(super) fn paint_skin_rgb_slider(
    ui: &mut egui::Ui,
    painter: &Painter,
    row: Rect,
    id: &'static str,
    label: &str,
    value: &mut u8,
    accent: Color32,
    active: bool,
) {
    let content = card_clip(painter, row);
    let painter = &content;
    // 左侧标签
    theme::bold_text(
        painter,
        Pos2::new(row.left(), row.top() + 7.0),
        label,
        8.8,
        accent,
    );
    // 轨道：左侧留 27px，宽度取 行宽-75 与 80 的较大值，高 6px
    let track = Rect::from_min_size(
        Pos2::new(row.left() + 27.0, row.center().y - 3.0),
        Vec2::new((row.width() - 75.0).max(80.0), 6.0),
    );
    let hit = Rect::from_min_max(
        Pos2::new(track.left(), track.top() - 10.0),
        Pos2::new(track.right(), track.bottom() + 10.0),
    );
    let mut hovered = false;
    // 仅在激活时可交互
    if active {
        let response = ui
            .interact(hit, Id::unique(id), Sense::click_and_drag())
            .on_hover_cursor(CursorIcon::PointingHand);
        hovered = response.hovered();
        if (response.clicked() || response.dragged())
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let ratio = ((pointer.x - track.left()) / track.width()).clamp(0.0, 1.0);
            *value = (ratio * 255.0).round() as u8;
            ui.ctx().request_repaint();
        }
    }
    // 当前比例
    let ratio = *value as f32 / 255.0;
    theme::fill_round(
        painter,
        track,
        theme::corner_radius(),
        theme::white_alpha(18),
    );
    // 填充部分：悬停时强调色提亮 1.18 倍
    if ratio > 0.0 {
        theme::fill_round(
            painter,
            Rect::from_min_size(
                track.left_top(),
                Vec2::new(track.width() * ratio, track.height()),
            ),
            theme::corner_radius(),
            if hovered {
                accent.gamma_multiply(1.18)
            } else {
                accent
            },
        );
    }
    // 滑块圆点
    painter.circle_filled(
        Pos2::new(track.left() + track.width() * ratio, track.center().y),
        6.0,
        Color32::from_rgb(239, 246, 240),
    );
    // 右侧显示 0–255 数值
    let value_position = Pos2::new(row.right(), row.top() + 7.0);
    let value_color =
        theme::adaptive_text_color_at(painter, value_position, theme::ink_soft(), 8.8);
    painter.text(
        value_position,
        egui::Align2::RIGHT_TOP,
        value.to_string(),
        theme::bold_font(8.8),
        value_color,
    );
}

/// 绘制状态徽章（圆角半透明小标签，如"已安装""可启动"）。
///
/// 参数 `painter` 画布、`row` 行矩形、`label` 徽章文字。
/// 宽度按字符数估算（每字 7.2px + 18 内边距），使用主题绿半透明填充与描边，居中绿字。
pub(super) fn paint_status_badge(painter: &Painter, row: Rect, label: &str) {
    let content = card_clip(painter, row);
    let painter = &content;
    // 按字符数估算徽章宽度
    let width = label.chars().count() as f32 * 7.2 + 18.0;
    let badge = setting_right_control_rect(row, Vec2::new(width, 27.0));
    // 半透明绿底 + 略深描边
    theme::fill_round(
        painter,
        badge,
        theme::corner_radius(),
        theme::alpha(theme::GREEN, 20),
    );
    theme::stroke_round(
        painter,
        badge,
        theme::corner_radius(),
        1.0,
        theme::alpha(theme::GREEN, 44),
    );
    // 居中绿字
    theme::center_text(
        painter,
        badge.center(),
        label,
        7.6,
        theme::GREEN_BRIGHT,
        false,
    );
}

/// 绘制页面标题卡片（含副标题 kicker、主标题、状态药丸）。
///
/// 参数：`painter` 画布；`layout` 布局信息；`page` 当前页面；
/// `game_directory_ready` 游戏目录是否就绪（影响状态药丸颜色/文字）；
/// `motion` 卡片位移动画（切页时产生偏移）。绘制玻璃标题卡、等宽副标题、
/// 大号主标题，以及右上角状态药丸（就绪绿/未就绪橙）。
pub(super) fn paint_page_heading(
    painter: &Painter,
    layout: &Layout,
    page: Page,
    game_directory_ready: bool,
    motion: CardMotion,
) {
    let header = layout.header;
    let anchor = page_card_anchor(page, layout);
    // 由动画进度得出卡片相对锚点的竖直偏移
    let shift = motion.offset(anchor);
    // 标题卡固定 258 宽、占满 header 高度
    let title_card = Rect::from_min_size(
        Pos2::new(header.left(), header.top() + shift),
        Vec2::new(258.0, header.height()),
    );
    // 玻璃面板（深绿底 + 绿边，无流动光泽）
    theme::glass_panel(
        painter,
        title_card,
        theme::corner_radius(),
        theme::GlassSpec::new(Color32::from_rgb(20, 31, 23), theme::GREEN, 0.0, 8.4),
    );
    let content = card_clip(painter, title_card);
    let painter = &content;
    // 副标题（等宽字体、次要色）
    theme::mono_text(
        painter,
        Pos2::new(title_card.left() + 30.0, title_card.top() + 3.0),
        page_kicker(page),
        7.5,
        theme::muted(),
    );
    // 主标题（大号、近白）
    theme::bold_text(
        painter,
        Pos2::new(title_card.left() + 30.0, title_card.top() + 17.0),
        page_title(page),
        23.0,
        Color32::from_rgb(238, 245, 233),
    );
    // 状态药丸：就绪为绿，未就绪为橙
    let pill = Rect::from_min_size(
        Pos2::new(title_card.left() + 98.0, title_card.top() + 25.0),
        Vec2::new(69.0, 21.0),
    );
    let status_color = if game_directory_ready {
        theme::GREEN
    } else {
        Color32::from_rgb(226, 181, 86)
    };
    theme::fill_round(
        painter,
        pill,
        theme::corner_radius(),
        theme::alpha(status_color, 15),
    );
    // 药丸内小圆点 + 状态文字
    painter.circle_filled(
        Pos2::new(pill.left() + 11.0, pill.center().y),
        2.2,
        status_color,
    );
    theme::text(
        painter,
        Pos2::new(pill.left() + 18.0, pill.top() + 4.5),
        page_status_label(page, game_directory_ready),
        8.0,
        Color32::from_rgb(145, 183, 141),
    );
}

/// 返回某页面"标题卡片"应停靠的矩形锚点（随页面布局不同而不同）。
///
/// 参数 `page` 页面枚举、`layout` 布局。Home 用 hero 区；
/// 版本/下载为左侧 45% 宽、高 104；社区为 72% 宽、高 220；创造为整宽、高 218；
/// 设置为右下偏移的小卡片 194x66。返回对应 `Rect`。
pub(super) fn page_card_anchor(page: Page, layout: &Layout) -> Rect {
    let top = layout.header.bottom() + 14.0;
    let left = layout.content_left;
    let width = (layout.content_right - layout.content_left).max(1.0);
    match page {
        Page::Home => layout.hero,
        Page::Versions | Page::Download => {
            Rect::from_min_size(Pos2::new(left, top), Vec2::new(width * 0.45, 104.0))
        }
        Page::Community => {
            Rect::from_min_size(Pos2::new(left, top), Vec2::new(width * 0.72, 220.0))
        }
        Page::Create => Rect::from_min_size(Pos2::new(left, top), Vec2::new(width, 218.0)),
        Page::Settings => {
            Rect::from_min_size(Pos2::new(left + 8.0, top + 8.0), Vec2::new(194.0, 66.0))
        }
    }
}

/// 返回页面英文副标题（kicker），如 "GENESIS LAUNCHER" / "VERSION LIBRARY"。
///
/// 参数 `page` 页面枚举；返回 `'static str`（等宽小字，置于主标题上方）。
pub(super) fn page_kicker(page: Page) -> &'static str {
    match page {
        Page::Home => "GENESIS LAUNCHER",
        Page::Versions => "VERSION LIBRARY",
        Page::Download => "MINECRAFT DOWNLOADS",
        Page::Community => "FAVORITES & MODS",
        Page::Create => "CREATIVE WORKSPACE",
        Page::Settings => "LAUNCHER PREFERENCES",
    }
}

/// 返回页面主标题（中文），如 "游戏"/"版本"/"下载" 等。
///
/// 参数 `page` 页面枚举；返回 `'static str`。
pub(super) fn page_title(page: Page) -> &'static str {
    match page {
        Page::Home => "游戏",
        Page::Versions => "版本",
        Page::Download => "下载",
        Page::Community => "收藏",
        Page::Create => "创造",
        Page::Settings => "设置",
    }
}

/// 返回页面右上角状态药丸文字。
///
/// 参数 `page` 页面枚举、`game_directory_ready` 目录是否就绪（仅 Home 用到）。
/// 各页面有固定文案；Home 根据目录就绪状态返回"目录就绪"或"未发现"。
pub(super) fn page_status_label(page: Page, game_directory_ready: bool) -> &'static str {
    match page {
        Page::Settings => "本机配置",
        Page::Versions => "已安装",
        Page::Download => "官方源",
        Page::Community => "收藏内容",
        Page::Create => "创作空间",
        Page::Home if game_directory_ready => "目录就绪",
        Page::Home => "未发现",
    }
}

/// 绘制一个紧凑方形图标按钮（用于"加号/减号"等小操作），返回是否被点击。
///
/// 参数：`ui` 交互；`painter` 画布；`rect` 按钮矩形；`id` 唯一标识；
/// `plus` 为 true 画加号（两条线），false 只画横线（如减号/关闭）。
/// 悬停时底色变亮；点击返回 `true`。
pub(super) fn compact_icon_button(
    ui: &mut egui::Ui,
    painter: &Painter,
    rect: Rect,
    id: &'static str,
    plus: bool,
) -> bool {
    let content = card_clip(painter, rect);
    let painter = &content;
    // 注册点击交互
    let response = ui
        .interact(rect, Id::unique(id), Sense::click())
        .on_hover_cursor(CursorIcon::PointingHand);
    // 悬停浅绿、否则深绿底
    theme::fill_round(
        painter,
        rect,
        theme::corner_radius(),
        if response.hovered() {
            Color32::from_rgb(42, 64, 47)
        } else {
            Color32::from_rgb(32, 47, 37)
        },
    );
    theme::stroke_round(
        painter,
        rect,
        theme::corner_radius(),
        1.0,
        theme::white_alpha(34),
    );
    // 横线
    painter.line_segment(
        [
            Pos2::new(rect.center().x - 6.0, rect.center().y),
            Pos2::new(rect.center().x + 6.0, rect.center().y),
        ],
        Stroke::new(1.5, theme::ink_soft()),
    );
    // 加号时再画竖线
    if plus {
        painter.line_segment(
            [
                Pos2::new(rect.center().x, rect.center().y - 6.0),
                Pos2::new(rect.center().x, rect.center().y + 6.0),
            ],
            Stroke::new(1.5, theme::ink_soft()),
        );
    }
    response.clicked()
}

/// 绘制设置页普通按钮（主按钮为液态绿，次按钮为深绿描边），返回是否被点击。
///
/// 参数：`ui` 交互；`painter` 画布；`rect` 按钮矩形；`id` 唯一标识；
/// `label` 文字；`primary` 是否主按钮（液态填充 + 白字）。点击返回 `true`。
pub(super) fn settings_button(
    ui: &mut egui::Ui,
    painter: &Painter,
    rect: Rect,
    id: &'static str,
    label: &str,
    primary: bool,
) -> bool {
    let content = card_clip(painter, rect);
    let painter = &content;
    let response = ui
        .interact(rect, Id::unique(id), Sense::click())
        .on_hover_cursor(CursorIcon::PointingHand);
    if primary {
        // 主按钮：液态绿色按钮 + 白色居中文字
        theme::liquid_button(
            painter,
            rect,
            theme::corner_radius(),
            2.2,
            response.hovered(),
        );
        theme::center_text(painter, rect.center(), label, 9.0, Color32::WHITE, true);
    } else {
        // 次按钮：深绿底 + 描边 + 灰绿文字
        theme::fill_round(
            painter,
            rect,
            theme::corner_radius(),
            if response.hovered() {
                Color32::from_rgb(37, 53, 42)
            } else {
                Color32::from_rgb(28, 42, 33)
            },
        );
        theme::stroke_round(
            painter,
            rect,
            theme::corner_radius(),
            1.0,
            theme::white_alpha(34),
        );
        theme::center_text(painter, rect.center(), label, 8.8, theme::ink_soft(), false);
    }
    response.clicked()
}

/// 绘制皮肤编辑历史按钮（撤销/重做），`enabled=false` 时渲染为禁用态且不响应点击。
///
/// 参数：`ui` 交互；`painter` 画布；`rect` 矩形；`id` 唯一标识；`label` 文字；
/// `enabled` 是否可用。禁用态为半透明灰底、暗文字并返回 `false`；
/// 可用时委托 `settings_button` 绘制次按钮并返回其点击状态。
pub(super) fn skin_history_button(
    ui: &mut egui::Ui,
    painter: &Painter,
    rect: Rect,
    id: &'static str,
    label: &str,
    enabled: bool,
) -> bool {
    // 禁用态：半透明、暗色、直接返回 false
    if !enabled {
        theme::fill_round(
            painter,
            rect,
            theme::corner_radius(),
            Color32::from_rgba_unmultiplied(28, 40, 32, 120),
        );
        theme::stroke_round(
            painter,
            rect,
            theme::corner_radius(),
            1.0,
            theme::white_alpha(16),
        );
        theme::center_text(
            painter,
            rect.center(),
            label,
            8.2,
            theme::white_alpha(58),
            false,
        );
        return false;
    }
    // 可用时复用次按钮样式
    settings_button(ui, painter, rect, id, label, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// 验证右侧设置控件到卡片顶部、底部和右侧的距离完全一致。
    fn right_setting_controls_keep_three_equal_margins() {
        let row = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(640.0, 74.0));
        let card = setting_card_rect(row);
        let control = setting_right_control_rect(row, Vec2::new(48.0, 27.0));
        let top = control.top() - card.top();
        let bottom = card.bottom() - control.bottom();
        let right = card.right() - control.right();
        assert!((top - bottom).abs() < 0.001);
        assert!((top - right).abs() < 0.001);
    }
}
