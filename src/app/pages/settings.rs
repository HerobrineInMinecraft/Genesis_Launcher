//! 设置中心页面的绘制逻辑。
//!
//! 本文件是 `LauncherApp` 的 `impl` 块的一部分，专注于“设置中心”页面的渲染与交互。
//! 它定义了两个核心方法：
//! - `LauncherApp::paint_settings`：负责在设置区块（section）切换时绘制“离场”与“入场”两层视图，
//!   以实现平滑的卡片转场动画。
//! - `LauncherApp::paint_settings_view`：根据当前选中的设置区块（`SettingsSection`），
//!   绘制左侧分类导航、顶部标题卡片以及右侧对应区块的所有设置项（开关、按钮、滑块、文本输入等）。
//!
//! 本文件大量依赖 `theme` 模块提供的玻璃拟态（glassmorphism）绘制函数和 `Painter` 直接绘制，
//! 并通过 `ui.interact` / `ui.put` 与 egui 交互层结合，处理点击、悬停与文本编辑等输入。
//! 所有设置读写都发生在 `self.settings`（`LauncherSettings`）上，修改会自动持久化到本机的 `settings.ini`。

use super::super::*;

impl LauncherApp {
    /// 绘制设置中心页面，支持在切换区块时同时绘制“离场”与“入场”两层视图以实现转场动画。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文，用于请求重绘与获取输入时间。
    /// - `ui`：egui 的 UI 交互句柄，用于创建可点击/可输入的控件。
    /// - `painter`：底层绘制器，用于绘制玻璃卡片、文字、图标等。
    /// - `layout`：当前界面的布局信息（屏幕、页眉、内容区域边界等）。
    /// - `time`：全局动画时钟（秒），驱动玻璃折射等随时间变化的效果。
    /// - `motion`：当前视图的卡片运动参数（转场方向、进度等）。
    ///
    /// # 行为
    /// 当 `self.previous_settings_section` 存在时，说明刚刚发生了区块切换：
    /// 1. 基于上一次的转场方向与进度，构造一个 `outgoing`（离场）视图和一个 `incoming`（入场）视图；
    /// 2. 先以 `draw_chrome: false` 绘制上一区块的离场视图（只画内容、不画导航 chrome）；
    /// 3. 再以 `draw_chrome: true` 绘制当前区块的入场视图（完整导航 + 内容 + 页脚）。
    ///
    /// 若没有 `previous_settings_section`，则两层都为 `None`，仅绘制当前区块。
    ///
    /// # 副作用
    /// 通过 `ctx.request_repaint` 在动画未结束时持续请求重绘，直到转场完成。
    /// 不会修改任何设置数据，也不写入磁盘。
    pub(in crate::app) fn paint_settings(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
        motion: CardMotion,
    ) {
        // 判断是否需要绘制动画转场：只有当曾经记录过一个“上一次的区块”时才说明发生了切换。
        let (outgoing, incoming) = if let Some(previous_section) = self.previous_settings_section {
            // 离场视图的运动参数：沿用上一次转场的方向与进度，并标记 outgoing=true（表示正在退出）。
            let outgoing = CardMotion {
                screen: layout.screen,
                direction: self.settings_transition_direction,
                progress: self.settings_transition_progress,
                outgoing: true,
            };
            // 入场视图的运动参数：方向/进度相同，但标记 outgoing=false（表示正在进入）。
            let incoming = CardMotion {
                screen: layout.screen,
                direction: self.settings_transition_direction,
                progress: self.settings_transition_progress,
                outgoing: false,
            };
            // 返回 (离场视图的 (区块, 运动), 入场视图的运动)，两层都用 Some 包裹表示需要绘制。
            (Some((previous_section, outgoing)), Some(incoming))
        } else {
            // 没有发生切换，两层均为 None，仅绘制当前区块即可。
            (None, None)
        };
        // 若没有入场运动（未发生切换），则回退使用调用方传入的 motion 作为内容运动参数。
        let content_motion = incoming.unwrap_or(motion);
        // 如果存在离场视图，先把它画在底层：只绘制内容（draw_chrome=false），不画导航侧栏。
        if let Some((previous_section, outgoing_motion)) = outgoing {
            self.paint_settings_view(
                ctx,
                ui,
                painter,
                layout,
                time,
                SettingsView {
                    section: previous_section,
                    chrome_motion: motion,
                    content_motion: outgoing_motion,
                    draw_chrome: false,
                    draw_footer: false,
                },
            );
        }
        // 绘制当前区块的完整视图：导航侧栏(content_motion 用于内容)、标题卡片、所有设置项与底部页脚。
        // 这一层画在离场视图之上，实现切换时的叠层淡入/淡出转场。
        self.paint_settings_view(
            ctx,
            ui,
            painter,
            layout,
            time,
            SettingsView {
                section: self.settings_section,
                chrome_motion: motion,
                content_motion,
                draw_chrome: true,
                draw_footer: true,
            },
        );
    }

    /// 绘制设置中心的一个具体视图（由 `view.section` 决定显示哪个区块）。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文。
    /// - `ui`：egui UI 交互句柄，用于开关、按钮、文本编辑等交互控件。
    /// - `painter`：底层绘制器，绘制玻璃卡片、文字、图标等内容。
    /// - `layout`：界面布局信息（屏幕、页眉、内容区域）。
    /// - `time`：动画时钟（秒），驱动玻璃折射等时间相关效果。
    /// - `view`：本次要绘制的视图描述，包含：
    ///   - `section`：当前要显示的设置区块（通用/游戏/版本/启动/首页/外观/关于）。
    ///   - `chrome_motion`：导航侧栏（左侧分类）的运动参数。
    ///   - `content_motion`：右侧内容面板的运动参数。
    ///   - `draw_chrome`：是否绘制左侧分类导航与标题（离场视图设为 false，仅画内容）。
    ///   - `draw_footer`：是否绘制底部“自动保存/恢复默认”页脚。
    ///
    /// # 行为
    /// 先计算内容区域、左侧分类栏与右侧面板的矩形范围，并按运动参数做位移/缩放。
    /// 当 `draw_chrome` 为真时绘制左侧分类导航（含选中高亮条、各区块图标与名称、底部说明）。
    /// 随后绘制右侧标题卡片与对应区块的所有设置项，最后当 `draw_footer` 为真时绘制底部页脚。
    ///
    /// # 副作用
    /// 直接读写 `self.settings` 中的各项设置（开关、内存、目录、Java、主题等），修改会触发自动保存；
    /// 可能请求重绘（`ctx.request_repaint`）、弹出 toast、打开外部页面或文件夹；
    /// 可能修改 `self` 上的多个状态字段（如 `settings_highlight_y`、`java_picker_target` 等）。
    /// 本函数不返回任何值。
    pub(in crate::app) fn paint_settings_view(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
        view: SettingsView,
    ) {
        // 计算整个内容区域：上方从页眉底部再下移 14px 起，下方到屏幕底部上方 22px 止。
        let page = Rect::from_min_max(
            Pos2::new(layout.content_left, layout.header.bottom() + 14.0),
            Pos2::new(layout.content_right, layout.screen.bottom() - 22.0),
        );
        // 左侧分类导航栏的固定宽度（像素）。
        let category_width = 210.0;
        // 左侧分类栏：从内容区左上角起，宽为 category_width，高度与内容区一致。
        let source_categories = Rect::from_min_max(
            page.left_top(),
            Pos2::new(page.left() + category_width, page.bottom()),
        );
        // 右侧内容面板：紧接分类栏右侧留出 14px 间隙，直到内容区右边界。
        let source_panel = Rect::from_min_max(
            Pos2::new(source_categories.right() + 14.0, page.top()),
            page.right_bottom(),
        );
        // 按导航侧栏的运动参数对分类栏矩形做位移/缩放，得到实际绘制位置（转场动画用）。
        let categories = view.chrome_motion.rect(source_categories);
        // 按内容运动参数对右侧面板矩形做位移/缩放。
        let panel = view.chrome_motion.rect(source_panel);

        // 仅当需要在本层绘制导航侧栏时才执行（离场视图会把该标志设为 false）。
        if view.draw_chrome {
            let chrome_base = painter;
            // 顶部标题卡片：分类栏内上移 8px、左右各缩进 8px，高度 66px。
            let title_card = Rect::from_min_size(
                Pos2::new(categories.left() + 8.0, categories.top() + 8.0),
                Vec2::new(categories.width() - 16.0, 66.0),
            );
            // 绘制标题卡片的玻璃背景（深绿底 + 绿色折射高光，强度随时间变化）。
            theme::glass_panel(
                painter,
                title_card,
                theme::corner_radius(),
                theme::GlassSpec::new(Color32::from_rgb(20, 32, 24), theme::GREEN, time, 12.2),
            );
            // 在标题卡片范围内裁剪出绘制区域，后续文字不会越界。
            let title_content = card_clip(painter, title_card);
            let painter = &title_content;
            // 小号等宽字显示英文标签 "SETTINGS"。
            theme::mono_text(
                painter,
                Pos2::new(title_card.left() + 14.0, title_card.top() + 14.0),
                "SETTINGS",
                7.4,
                theme::muted(),
            );
            // 大号粗体显示中文标题 "设置中心"。
            theme::bold_text(
                painter,
                Pos2::new(title_card.left() + 14.0, title_card.top() + 31.0),
                "设置中心",
                18.0,
                theme::ink(),
            );

            // 恢复为整块侧栏的绘制器，继续绘制分类列表。
            let painter = chrome_base;
            // 第一个分类项顶部相对侧栏顶部的内边距。
            let item_top = categories.top() + 86.0;
            // 高亮条目标 Y 坐标：第一个项下方 21px 起，每往后一个区块按固定间距下移。
            let target_y = item_top + 21.0 + view.section.index() as f32 * SETTINGS_ITEM_SPACING;
            // 若高亮条尚未初始化，或用户关闭了动画，则直接跳到目标位置、速度清零，避免突兀动画。
            if !self.settings_highlight_initialized || !self.settings.animations_enabled() {
                self.settings_highlight_y = target_y;
                self.settings_highlight_velocity = 0.0;
                self.settings_highlight_initialized = true;
            } else {
                // 用稳定的帧间隔（clamp 到合理范围避免卡顿时跳变）驱动弹簧平滑。
                let delta = ctx.input(|input| input.stable_dt).clamp(0.001, 0.05);
                // 弹簧动画将高亮条缓动到 target_y；若尚未到达则请求稍后重绘以继续动画。
                if spring_smooth(
                    &mut self.settings_highlight_y,
                    &mut self.settings_highlight_velocity,
                    target_y,
                    delta,
                ) {
                    ctx.request_repaint_after(Duration::from_millis(16));
                }
            }
            // 根据当前高亮 Y 构造高亮条矩形（贴近分类栏中心，宽比栏窄 20px、高 42px）。
            let highlight = Rect::from_center_size(
                Pos2::new(categories.center().x, self.settings_highlight_y),
                Vec2::new(categories.width() - 20.0, 42.0),
            );
            // 绘制高亮条的玻璃背景（较亮的深绿，强调当前选中的区块）。
            theme::glass_panel(
                painter,
                highlight,
                theme::corner_radius(),
                theme::GlassSpec::new(Color32::from_rgb(29, 49, 33), theme::GREEN, time, 13.8),
            );
            // 在高亮条左侧画一条细竖条作为选中指示标记。
            painter.rect_filled(
                Rect::from_min_size(
                    Pos2::new(categories.left() + 8.0, highlight.center().y - 9.5),
                    Vec2::new(3.0, 19.0),
                ),
                theme::corner_radius(),
                theme::GREEN_BRIGHT,
            );

            // 遍历所有设置区块，逐个绘制左侧分类列表项。
            for (index, section) in SettingsSection::ALL.into_iter().enumerate() {
                // 每个列表项矩形：左侧内缩 10px，按索引依次下移 SETTINGS_ITEM_SPACING，高 42px。
                let item = Rect::from_min_size(
                    Pos2::new(
                        categories.left() + 10.0,
                        item_top + index as f32 * SETTINGS_ITEM_SPACING,
                    ),
                    Vec2::new(categories.width() - 20.0, 42.0),
                );
                // 注册一个可点击的交互区域，悬停时显示“手型”光标，并用唯一 id 区分各项。
                let response = ui
                    .interact(
                        item,
                        Id::unique(("settings-category", index)),
                        Sense::click(),
                    )
                    .on_hover_cursor(CursorIcon::PointingHand);
                // 在列表项范围内裁剪绘制区域。
                let item_content = card_clip(painter, item);
                // 判断该项是否为当前选中的区块。
                let selected = self.settings_section == section;
                // 根据区块类型绘制对应图标；颜色随选中/悬停状态变化（选中亮绿、悬停浅色、否则灰色）。
                draw_icon(
                    &item_content,
                    match section {
                        SettingsSection::General => NavIcon::Settings,
                        SettingsSection::Game => NavIcon::Box,
                        SettingsSection::Versions => NavIcon::Layers,
                        SettingsSection::Launch => NavIcon::Monitor,
                        SettingsSection::Home => NavIcon::Sparkles,
                        SettingsSection::Appearance => NavIcon::Sparkles,
                        SettingsSection::About => NavIcon::Monitor,
                    },
                    Rect::from_center_size(
                        Pos2::new(item.left() + 23.0, item.center().y),
                        Vec2::splat(17.0),
                    ),
                    if selected {
                        theme::GREEN_BRIGHT
                    } else if response.hovered() {
                        theme::ink_soft()
                    } else {
                        theme::muted()
                    },
                );
                // 在图标右侧绘制区块名称（粗体，颜色同样随状态变化）。
                theme::bold_text(
                    &item_content,
                    Pos2::new(item.left() + 43.0, item.top() + 16.0),
                    section.label(),
                    9.4,
                    if selected {
                        theme::ink()
                    } else if response.hovered() {
                        theme::ink_soft()
                    } else {
                        Color32::from_rgb(163, 176, 166)
                    },
                );
                // 若用户点击了该项，则切换到对应区块并请求重绘以更新高亮与内容。
                if response.clicked() {
                    self.switch_settings_section(section);
                    ctx.request_repaint();
                }
            }

            // 在分类栏底部上方画一条分隔线，区分导航与底部说明文字。
            painter.line_segment(
                [
                    Pos2::new(categories.left() + 16.0, categories.bottom() - 58.0),
                    Pos2::new(categories.right() - 16.0, categories.bottom() - 58.0),
                ],
                Stroke::new(1.0, theme::line()),
            );
            // 提示用户配置保存在本机。
            theme::text(
                painter,
                Pos2::new(categories.left() + 18.0, categories.bottom() - 45.0),
                "配置保存在本机",
                7.4,
                theme::muted(),
            );
            // 用等宽字显示配置文件名 settings.ini 及其绿色着色。
            theme::mono_text(
                painter,
                Pos2::new(categories.left() + 18.0, categories.bottom() - 29.0),
                "settings.ini",
                8.0,
                Color32::from_rgb(139, 177, 135),
            );
        }

        // 克隆 painter 以便在会重绑定的局部变量上绘制，避免后续借用冲突。
        let painter = painter.clone();
        let painter = &painter;
        // 右侧面板顶部的标题卡片：上移 8px、高度 72px，承载区块名称与描述。
        let heading_card = view.content_motion.rect(Rect::from_min_size(
            Pos2::new(source_panel.left(), source_panel.top() + 8.0),
            Vec2::new(source_panel.width(), 72.0),
        ));
        // 记录“整块内容面板”的绘制器，便于标题卡片绘制完后恢复。
        let section_base = painter;
        // 绘制标题卡片玻璃背景。
        theme::glass_panel(
            painter,
            heading_card,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(20, 32, 24), theme::GREEN, time, 13.5),
        );
        // 裁剪出标题卡片绘制区域。
        let heading_content = card_clip(painter, heading_card);
        let painter = &heading_content;
        // 标题文字顶部内边距。
        let heading_y = heading_card.top() + 14.0;
        // 粗体显示当前区块标题（如“通用”“游戏”）。
        theme::bold_text(
            painter,
            Pos2::new(heading_card.left() + 14.0, heading_y),
            view.section.label(),
            20.0,
            theme::ink(),
        );
        // 小一号文字显示当前区块的功能描述。
        theme::text(
            painter,
            Pos2::new(heading_card.left() + 14.0, heading_y + 30.0),
            view.section.description(),
            8.6,
            theme::muted(),
        );
        // 恢复为整块内容面板的绘制器。
        let painter = section_base;

        // 内容区内所有设置行的统一左右内边距（各缩进 26px）。
        let source_row_left = source_panel.left() + 26.0;
        let source_row_right = source_panel.right() - 26.0;
        let source_row_width = source_row_right - source_row_left;
        // 构造一个闭包：给定 y 与高度，返回一个按内容运动参数变换后的设置行矩形。
        let setting_row = |y: f32, height: f32| {
            view.content_motion.rect(Rect::from_min_size(
                Pos2::new(source_row_left, y),
                Vec2::new(source_row_width, height),
            ))
        };
        // 第一行起始 Y（标题卡片下方 96px 处）。
        let mut row_y = source_panel.top() + 96.0;
        // 相邻设置行之间的垂直间距。
        let row_gap = 74.0;
        // 根据当前选中的区块，绘制对应的一组设置项。每个分支内部的 row_y 都会随行数递增。
        match view.section {
            SettingsSection::General => {
                // 第 1 行：强制自动更新（只读状态徽标，不可更改）。
                let row = setting_row(row_y, row_gap);
                paint_setting_card(painter, row, time);
                paint_setting_label(
                    painter,
                    row,
                    "强制自动更新",
                    "检测到新版本后自动下载、校验、替换并重新启动",
                );
                paint_status_badge(painter, row, "强制开启");
                row_y += row_gap;
                // 第 2 行：启动完成后通知（可开关）。
                let row = setting_row(row_y, row_gap);
                paint_setting_card(painter, row, time);
                paint_toggle_row(
                    ui,
                    painter,
                    row,
                    "setting-notify-launch",
                    "启动完成后通知",
                    "游戏进程接管后显示桌面提示",
                    &mut self.settings.notify_after_launch,
                );
                row_y += row_gap;
                // 第 3 行：启动器更新状态与操作按钮。
                let row = setting_row(row_y, row_gap);
                paint_setting_card(painter, row, time);
                // 读取编译期注入的当前版本号（Cargo.toml 中的 version）。
                let current_version = env!("CARGO_PKG_VERSION");
                // 判断是否存在比当前版本更新的发布版本。
                let available = self.launcher_update_result.as_ref().is_some_and(|release| {
                    update::is_newer_version(current_version, &release.version)
                });
                // 根据更新检查的不同状态生成描述文字。
                let description = if self.launcher_update_receiver.is_some() {
                    // 正在后台检查更新。
                    "正在从 GitHub Releases 检查最新版本".to_owned()
                } else if let Some(release) = self.launcher_update_result.as_ref() {
                    // 已拿到结果：有新版本则提示版本号，否则提示已最新。
                    if available {
                        format!("发现新版本 v{} · 当前 v{current_version}", release.version)
                    } else {
                        format!("已是最新版本 · 当前 v{current_version}")
                    }
                } else if let Some(error) = self.launcher_update_error.as_ref() {
                    // 检查出错：截断错误信息避免溢出。
                    ellipsize_end(&format!("检查失败 · {error}"), 56)
                } else {
                    // 尚未检查过。
                    format!("检查 Genesis Launcher 的 GitHub Releases · 当前 v{current_version}")
                };
                paint_setting_label(painter, row, "启动器更新", &description);
                // “打开更新/发布页”按钮矩形（位于行右侧）。
                let release_button = setting_right_control_rect(row, Vec2::new(88.0, 34.0));
                // “检查更新”按钮矩形（“打开更新”按钮左侧）。
                let check_button = Rect::from_min_size(
                    Pos2::new(release_button.left() - 90.0, release_button.top()),
                    Vec2::new(82.0, 34.0),
                );
                // 点击“检查更新”：主动向 GitHub Releases 发起更新检查。
                if settings_button(
                    ui,
                    painter,
                    check_button,
                    "launcher-update-check",
                    "检查更新",
                    false,
                ) {
                    self.start_launcher_update_check(ui.ctx(), true);
                }
                // 点击“打开更新/发布页”：打开对应网页；有更新时按钮高亮。
                if settings_button(
                    ui,
                    painter,
                    release_button,
                    "launcher-update-page",
                    if available {
                        "打开更新"
                    } else {
                        "发布页"
                    },
                    available,
                ) {
                    // 优先使用发布结果中的页面链接，否则回退到 Releases 总页面。
                    let page_url = self
                        .launcher_update_result
                        .as_ref()
                        .map(|release| release.page_url.as_str())
                        .unwrap_or(update::RELEASES_URL);
                    // 打开外部网页失败则弹出提示。
                    if let Err(error) = crate::favorites::open_site(page_url) {
                        self.show_toast(format!("无法打开更新页面：{error}"));
                    }
                }
            }
            SettingsSection::Game => {
                // 第 1 行：游戏内存（Java 最大堆内存），通过 −/+ 按钮以 512MB 步进调整。
                let row = setting_row(row_y, row_gap);
                paint_setting_card(painter, row, time);
                let memory_limit = memory_limit_mb();
                let memory_description =
                    format!("本机物理内存上限 {:.1} GB", memory_limit as f32 / 1024.0);
                paint_setting_label(painter, row, "游戏内存", &memory_description);
                // 减号按钮矩形（紧贴右侧、距右边 147px 处）。
                let minus = Rect::from_center_size(
                    Pos2::new(row.right() - 147.0, row.center().y),
                    Vec2::splat(34.0),
                );
                // 加号按钮矩形（最右侧）。
                let plus = Rect::from_center_size(
                    Pos2::new(row.right() - 33.0, row.center().y),
                    Vec2::splat(34.0),
                );
                // 注册并绘制 −/+ 圆形图标按钮，返回是否被点击。
                let minus_hit = compact_icon_button(ui, painter, minus, "memory-minus", false);
                let plus_hit = compact_icon_button(ui, painter, plus, "memory-plus", true);
                // 点击减号：内存减少 512MB，但不低于 2048MB 下限（saturating 防止下溢）。
                if minus_hit {
                    self.settings.memory_mb = self.settings.memory_mb.saturating_sub(512).max(2048);
                }
                // 点击加号：内存增加 512MB，但不高于本机物理内存上限。
                if plus_hit {
                    self.settings.memory_mb = (self.settings.memory_mb + 512).min(memory_limit);
                }
                // 数值以减号和加号按钮的几何中点为中心，而不是按左边缘定位。
                let memory_value_center =
                    Pos2::new((minus.center().x + plus.center().x) * 0.5, row.center().y);
                theme::center_text(
                    painter,
                    memory_value_center,
                    format!("{:.1} GB", self.settings.memory_mb as f32 / 1024.0),
                    14.0,
                    theme::ink(),
                    true,
                );
                row_y += row_gap;

                // 第 2 行：游戏目录，显示已发现版本数量与路径，并提供“官方目录”“打开”按钮。
                let row = setting_row(row_y, row_gap);
                paint_setting_card(painter, row, time);
                // 统计当前游戏目录下已安装的版本数量（无目录则为空）。
                let installed = self
                    .settings
                    .game_directory
                    .as_deref()
                    .map(crate::game::installed_versions)
                    .unwrap_or_default();
                // 构造描述：显示版本数与路径，路径过长则只保留末尾约 44 个字符并加省略号。
                let directory_description = self
                    .settings
                    .game_directory
                    .as_ref()
                    .map(|path| {
                        let path = path.display().to_string();
                        let suffix = path
                            .char_indices()
                            .rev()
                            .nth(44)
                            .map(|(index, _)| format!("…{}", &path[index..]))
                            .unwrap_or(path);
                        format!("已发现 {} 个版本 · {suffix}", installed.len())
                    })
                    .unwrap_or_else(|| "尚未发现游戏目录".to_owned());
                paint_setting_label(painter, row, "游戏目录", &directory_description);
                // “官方目录”按钮矩形。
                let discover = setting_right_control_rect(row, Vec2::new(86.0, 32.0));
                // 点击“官方目录”：自动发现并添加系统常见 Minecraft 目录。
                if settings_button(
                    ui,
                    painter,
                    discover,
                    "discover-game-dir",
                    "官方目录",
                    false,
                ) {
                    // 先把当前已选目录作为额外目录保存下来。
                    if let Some(current) = self.settings.game_directory.clone() {
                        self.settings.add_game_directory(current);
                    }
                    // 扫描常见官方目录位置。
                    let discovered = crate::game::discover_game_directories();
                    if let Some(directory) = discovered.first().cloned() {
                        let count = crate::game::installed_versions(&directory).len();
                        // 把所有发现到的目录都加入候选列表。
                        for candidate in discovered {
                            self.settings.add_game_directory(candidate);
                        }
                        // 选中第一个目录并提示版本数。
                        self.select_game_directory(directory);
                        self.show_toast(format!("已选择官方游戏目录 · {} 个版本", count));
                    } else {
                        self.show_toast("未找到官方 Minecraft 游戏目录");
                    }
                }
                // 仅当选定了游戏目录时才显示“打开”按钮。
                if self.settings.game_directory.is_some() {
                    let open = Rect::from_min_size(
                        Pos2::new(discover.left() - 80.0, discover.top()),
                        Vec2::new(72.0, 32.0),
                    );
                    // 点击“打开”：在文件管理器中打开游戏目录。
                    if settings_button(ui, painter, open, "open-game-dir", "打开", false) {
                        self.open_game_folder(ctx);
                    }
                }
                row_y += row_gap;

                // 第 3 行：Java 运行时，显示当前选择或自动模式，并提供“选择”按钮打开选择器。
                let row = setting_row(row_y, row_gap);
                paint_setting_card(painter, row, time);
                // 若已指定 Java 路径则给出可读标签，否则提示“自动选择兼容版本”。
                let java_description = self
                    .settings
                    .java_path
                    .as_ref()
                    .map(|path| self.java_runtime_label(path))
                    .unwrap_or_else(|| "自动选择兼容版本".to_owned());
                paint_setting_label(
                    painter,
                    row,
                    "Java 运行时",
                    &ellipsize_end(&java_description, 48),
                );
                // “选择”按钮矩形。
                let select_java = setting_right_control_rect(row, Vec2::new(82.0, 32.0));
                // 点击“选择”：打开全局 Java 运行时选择面板（设置锚点与目标后由其它代码绘制）。
                if settings_button(
                    ui,
                    painter,
                    select_java,
                    "settings-java-select",
                    "选择",
                    false,
                ) {
                    self.java_picker_anchor = select_java;
                    self.java_picker_target = Some(JavaPickerTarget::Global);
                    self.java_picker_scroll = 0.0;
                }
                row_y += row_gap;

                // 第 4 行：JVM 启动参数（只读说明，由启动器自动管理，不可编辑）。
                let row = setting_row(row_y, row_gap);
                paint_setting_card(painter, row, time);
                paint_setting_label(painter, row, "JVM 启动参数", "-Xms512m · 由启动器自动管理");
                paint_status_badge(painter, row, "自动");
            }
            SettingsSection::Versions => {
                // 第 1 行：Minecraft 文件夹，显示当前目录与已保存数量，提供“浏览”和“官方”按钮。
                let row = setting_row(row_y, row_gap);
                paint_setting_card(painter, row, time);
                // 当前目录路径若过长则从开头截断（保留末尾），未选择时给出提示。
                let current_directory = self
                    .settings
                    .game_directory
                    .as_ref()
                    .map(|path| ellipsize_start(&path.display().to_string(), 34))
                    .unwrap_or_else(|| "尚未选择 Minecraft 文件夹".to_owned());
                let directory_description = format!(
                    "{} · 已保存 {} 个文件夹",
                    current_directory,
                    self.settings.game_directories.len()
                );
                paint_setting_label(painter, row, "Minecraft 文件夹", &directory_description);
                // “浏览”按钮矩形（最右侧）。
                let browse = setting_right_control_rect(row, Vec2::new(72.0, 32.0));
                // 点击“浏览”：弹出系统目录选择框，若用户选定目录则切换游戏目录。
                if settings_button(
                    ui,
                    painter,
                    browse,
                    "settings-version-folder-browse",
                    "浏览",
                    false,
                ) && let Some(directory) = crate::game::pick_game_directory()
                {
                    self.select_game_directory(directory);
                }
                // “官方”按钮矩形（“浏览”按钮左侧）。
                let scan = Rect::from_min_size(
                    Pos2::new(browse.left() - 78.0, browse.top()),
                    Vec2::new(72.0, 32.0),
                );
                // 点击“官方”：扫描并加入所有常见官方目录，并提示保存数量。
                if settings_button(
                    ui,
                    painter,
                    scan,
                    "settings-version-folder-scan",
                    "官方",
                    false,
                ) {
                    for directory in crate::game::discover_game_directories() {
                        self.settings.add_game_directory(directory);
                    }
                    self.show_toast(format!(
                        "官方 Minecraft 文件夹已载入 · 共保存 {} 个",
                        self.settings.game_directories.len()
                    ));
                }
                row_y += row_gap;

                // 第 2 行：显示加载器版本号开关。
                let row = setting_row(row_y, row_gap);
                paint_setting_card(painter, row, time);
                paint_toggle_row(
                    ui,
                    painter,
                    row,
                    "setting-show-loader-version",
                    "显示加载器版本号",
                    "在版本选择名称后附加 Fabric、Forge 或 NeoForge 的实际版本号",
                    &mut self.settings.show_loader_versions,
                );
                row_y += row_gap;

                // 第 3 行：版本隔离（全局）开关，变更时需同步应用到各版本。
                let row = setting_row(row_y, row_gap);
                paint_setting_card(painter, row, time);
                // 记录修改前的隔离状态，用于判断是否发生变更。
                let previous_isolation = self.settings.version_isolation;
                paint_toggle_row(
                    ui,
                    painter,
                    row,
                    "setting-version-isolation",
                    "版本隔离（全局）",
                    "所有版本分别使用独立的 mods、saves 和配置目录",
                    &mut self.settings.version_isolation,
                );
                // 若隔离状态被改变，则调用专门方法刷新所有版本的隔离目录。
                if self.settings.version_isolation != previous_isolation {
                    self.set_version_isolation(self.settings.version_isolation);
                }
                row_y += row_gap;

                // 第 4 行：版本概览，显示已安装与可启动数量，并提供“打开版本目录”按钮。
                let row = setting_row(row_y, row_gap);
                paint_setting_card(painter, row, time);
                // 当前目录下已安装版本数量。
                let installed_count = self
                    .settings
                    .game_directory
                    .as_deref()
                    .map(crate::game::installed_versions)
                    .map(|versions| versions.len())
                    .unwrap_or(0);
                // 列表中标记为可启动（文件完整）的版本数量。
                let launchable_count = self
                    .versions
                    .iter()
                    .filter(|version| version.launchable)
                    .count();
                paint_setting_label(
                    painter,
                    row,
                    "版本概览",
                    &format!("{installed_count} 个已安装 · {launchable_count} 个完整可启动"),
                );
                // “打开版本目录”按钮矩形。
                let open = setting_right_control_rect(row, Vec2::new(106.0, 32.0));
                // 点击：在 <游戏目录>/versions 下创建目录并打开文件管理器。
                if settings_button(
                    ui,
                    painter,
                    open,
                    "settings-open-versions",
                    "打开版本目录",
                    false,
                ) && let Some(directory) = self.settings.game_directory.as_ref()
                {
                    let path = directory.join("versions");
                    let _ = std::fs::create_dir_all(&path);
                    open_path(&path);
                }
            }
            SettingsSection::Launch => {
                // 第 1 行：窗口分辨率，点击按钮在四个预设分辨率间循环切换。
                let row = setting_row(row_y, row_gap);
                paint_setting_card(painter, row, time);
                paint_setting_label(
                    painter,
                    row,
                    "窗口分辨率",
                    "Minecraft 启动时使用的初始窗口大小",
                );
                // 分辨率按钮矩形（显示形如 “1280 × 720”）。
                let resolution = setting_right_control_rect(row, Vec2::new(132.0, 32.0));
                // 构造当前分辨率的显示文本。
                let label = format!(
                    "{} × {}",
                    self.settings.window_width, self.settings.window_height
                );
                // 点击按钮：在预设分辨率列表中找到当前项，切换到下一项（循环到第一项）。
                if settings_button(ui, painter, resolution, "launch-resolution", &label, false) {
                    // 四个常见分辨率预设。
                    /// 提供 PRESETS 使用的只读常量。
                    ///
                    /// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
                    const PRESETS: [(u32, u32); 4] =
                        [(1280, 720), (1600, 900), (1920, 1080), (2560, 1440)];
                    let next = PRESETS
                        .iter()
                        .position(|size| {
                            *size == (self.settings.window_width, self.settings.window_height)
                        })
                        .map(|index| (index + 1) % PRESETS.len())
                        .unwrap_or(0);
                    (self.settings.window_width, self.settings.window_height) = PRESETS[next];
                }
                row_y += row_gap;

                // 第 2 行：全屏启动开关。
                let row = setting_row(row_y, row_gap);
                paint_setting_card(painter, row, time);
                paint_toggle_row(
                    ui,
                    painter,
                    row,
                    "setting-fullscreen",
                    "全屏启动",
                    "游戏启动后直接进入全屏模式",
                    &mut self.settings.fullscreen,
                );
                row_y += row_gap;

                // 第 3 行：额外 JVM 参数，单行文本输入框（用户自由输入）。
                let row = setting_row(row_y, row_gap);
                paint_setting_card(painter, row, time);
                paint_setting_label(
                    painter,
                    row,
                    "额外 JVM 参数",
                    "空格分隔，例如 -Xss2M -Dfile.encoding=UTF-8",
                );
                // 输入框背景与描边矩形（宽 390px）。
                let input = setting_right_control_rect(row, Vec2::new(390.0, 36.0));
                // 绘制输入框深色圆角底与半透明白色描边。
                theme::fill_round(
                    painter,
                    input,
                    theme::corner_radius(),
                    Color32::from_rgb(25, 38, 29),
                );
                theme::stroke_round(
                    painter,
                    input,
                    theme::corner_radius(),
                    1.0,
                    theme::white_alpha(34),
                );
                // 在输入框内放置一个无边框的单行文本框，直接绑定到 settings.extra_jvm_args。
                let _ = ui.put(
                    input.shrink2(Vec2::new(11.0, 5.0)),
                    egui::TextEdit::singleline(&mut self.settings.extra_jvm_args)
                        .frame(egui::Frame::NONE)
                        .font(theme::font(9.0))
                        .text_color(theme::adaptive_text_color_at(
                            painter,
                            input.center(),
                            theme::ink(),
                            9.0,
                        ))
                        .hint_text("可选启动参数"),
                );
                row_y += row_gap;

                // 第 4 行：启动后隐藏启动器开关。
                let row = setting_row(row_y, row_gap);
                paint_setting_card(painter, row, time);
                paint_toggle_row(
                    ui,
                    painter,
                    row,
                    "setting-hide-launch-settings",
                    "启动后隐藏启动器",
                    "游戏进程启动完成后自动最小化启动器",
                    &mut self.settings.hide_after_launch,
                );
            }
            SettingsSection::Home => {
                // 首页相关设置：主标题文本与预览、新增文字的颜色/字号、以及逐字样式编辑。
                let title_row = setting_row(row_y, 128.0);
                paint_setting_card(painter, title_row, time);
                let field_width = (title_row.width() * 0.52).max(280.0);
                let label_painter = painter.with_clip_rect(Rect::from_min_max(
                    title_row.left_top(),
                    Pos2::new(title_row.left() + field_width, title_row.bottom()),
                ));
                paint_setting_label(
                    &label_painter,
                    title_row,
                    "主标题内容",
                    "首页“世界，等你回来”区域显示的文字，最多三行",
                );
                // 标题文本编辑框与预览卡使用相同的上下边界，避免贴近或越过卡片边框。
                let field = Rect::from_min_size(
                    Pos2::new(
                        title_row.left() + SETTING_CARD_PADDING_X,
                        title_row.top() + 68.0,
                    ),
                    Vec2::new(field_width, 44.0),
                );
                theme::fill_round(
                    painter,
                    field,
                    theme::corner_radius(),
                    Color32::from_rgb(25, 38, 29),
                );
                theme::stroke_round(
                    painter,
                    field,
                    theme::corner_radius(),
                    1.0,
                    theme::white_alpha(30),
                );
                // 放置多行文本框，最多 48 字符、约 2 行，直接绑定到 settings.hero_title。
                let previous_title = self.settings.hero_title.clone();
                ui.put(
                    field.shrink2(Vec2::new(11.0, 7.0)),
                    egui::TextEdit::multiline(&mut self.settings.hero_title)
                        .frame(egui::Frame::NONE)
                        .font(theme::font(9.5))
                        .text_color(theme::adaptive_text_color_at(
                            painter,
                            field.center(),
                            theme::ink(),
                            9.5,
                        ))
                        .char_limit(48)
                        .desired_rows(2)
                        .hint_text("输入首页标题，可换行"),
                );
                // TextEdit 在本帧就可能插入或删除字符；按位置对齐逐字符样式，
                // 避免数组越界，同时确保中间插入/删除后样式仍跟随原字符。
                if self.settings.hero_title != previous_title {
                    self.settings
                        .reconcile_hero_title_char_styles(&previous_title);
                    self.hero_title_selected_char = self
                        .hero_title_selected_char
                        .min(self.settings.hero_title.chars().count().saturating_sub(1));
                }
                // 预览卡片位于编辑框右侧，显示带样式的标题效果。
                let preview = Rect::from_min_max(
                    Pos2::new(
                        field.right() + 16.0,
                        title_row.top() + SETTING_CARD_CONTENT_TOP_OFFSET,
                    ),
                    Pos2::new(
                        title_row.right() - SETTING_CARD_PADDING_X,
                        title_row.bottom() - SETTING_CARD_CONTENT_TOP_OFFSET,
                    ),
                );
                theme::fill_round(
                    painter,
                    preview,
                    theme::corner_radius(),
                    theme::alpha(theme::GREEN, 12),
                );
                // 根据标题文字、逐字大小与颜色，解析出用于预览的分行结构。
                let preview_lines = styled_hero_title_lines(
                    &self.settings.hero_title,
                    &self.settings.hero_title_char_sizes,
                    &self.settings.hero_title_char_colors,
                );
                // 若标题为空（被隐藏），显示占位提示。
                if preview_lines.is_empty() {
                    theme::center_text(
                        painter,
                        preview.center(),
                        "标题已隐藏",
                        8.5,
                        theme::muted(),
                        false,
                    );
                } else {
                    // 预览整体缩放系数。
                    let preview_scale = 0.46;
                    // 计算所有行中最高一行的高度（用于垂直居中）。
                    let total_height = preview_lines
                        .iter()
                        .map(|line| {
                            // 取本行各字符大小的最大值乘以缩放作为行高，再乘 1.02 留白。
                            line.iter()
                                .map(|(_, size, _)| *size * preview_scale)
                                .fold(0.0_f32, f32::max)
                                * 1.02
                        })
                        .sum::<f32>();
                    // 从垂直中点向上偏移半高，逐行向下绘制。
                    let mut y = preview.center().y - total_height * 0.5;
                    for line in &preview_lines {
                        y += paint_styled_hero_title_line(
                            painter,
                            Pos2::new(preview.left() + 14.0, y),
                            preview.width() - 28.0,
                            preview_scale,
                            line,
                        );
                    }
                }

                // 第二行：左列“新增文字颜色”面板、右列“新增文字字号”面板（两列并排）。
                let second_row_y = row_y + 140.0;
                let column_gap = 12.0;
                let column_width = (source_row_width - column_gap) * 0.5;
                let color_row = view.content_motion.rect(Rect::from_min_size(
                    Pos2::new(source_row_left, second_row_y),
                    Vec2::new(column_width, 140.0),
                ));
                paint_setting_card(painter, color_row, time);
                theme::bold_text(
                    painter,
                    Pos2::new(
                        color_row.left() + SETTING_CARD_PADDING_X,
                        color_row.top() + SETTING_CARD_CONTENT_TOP_OFFSET,
                    ),
                    "新增文字颜色",
                    9.2,
                    theme::ink(),
                );
                // 当前默认新增文字颜色（RGB 三元组），将随 R/G/B 滑块改变。
                let color = &mut self.settings.hero_title_color;
                let swatch = Rect::from_min_size(
                    Pos2::new(
                        color_row.right() - 92.0,
                        color_row.top() + SETTING_CARD_CONTENT_TOP_OFFSET,
                    ),
                    Vec2::new(28.0, 28.0),
                );
                theme::fill_round(
                    painter,
                    swatch,
                    theme::corner_radius(),
                    Color32::from_rgb(color[0], color[1], color[2]),
                );
                theme::stroke_round(
                    painter,
                    swatch,
                    theme::corner_radius(),
                    1.0,
                    theme::white_alpha(42),
                );
                theme::bold_text(
                    painter,
                    Pos2::new(
                        color_row.right() - 56.0,
                        color_row.top() + SETTING_CARD_CONTENT_TOP_OFFSET,
                    ),
                    format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2]),
                    7.8,
                    theme::ink_soft(),
                );
                // 依次为 R / G / B 三个通道绘制滑块，颜色与通道名一一对应，绑定到 color[index]。
                for (index, (label, accent)) in [
                    ("R", Color32::from_rgb(235, 91, 91)),
                    ("G", Color32::from_rgb(83, 199, 121)),
                    ("B", Color32::from_rgb(88, 142, 244)),
                ]
                .into_iter()
                .enumerate()
                {
                    paint_skin_rgb_slider(
                        ui,
                        painter,
                        Rect::from_min_size(
                            Pos2::new(
                                color_row.left() + SETTING_CARD_PADDING_X,
                                color_row.top() + 48.0 + index as f32 * 28.0,
                            ),
                            Vec2::new(color_row.width() - SETTING_CARD_PADDING_X * 2.0, 24.0),
                        ),
                        match index {
                            0 => "hero-title-red",
                            1 => "hero-title-green",
                            _ => "hero-title-blue",
                        },
                        label,
                        &mut color[index],
                        accent,
                        true,
                    );
                }

                // 右列：新增文字字号面板。
                let size_row = view.content_motion.rect(Rect::from_min_size(
                    Pos2::new(source_row_left + column_width + column_gap, second_row_y),
                    Vec2::new(column_width, 140.0),
                ));
                paint_setting_card(painter, size_row, time);
                paint_effect_slider(
                    ui,
                    painter,
                    size_row,
                    "hero-title-size",
                    "新增文字字号",
                    "新输入的字符默认使用此字号；可逐字覆盖",
                    &mut self.settings.hero_title_size,
                    24,
                    80,
                    1,
                    " px",
                );
                theme::text(
                    painter,
                    Pos2::new(
                        size_row.left() + SETTING_CARD_PADDING_X,
                        size_row.top() + 76.0,
                    ),
                    "实际标题会保持清晰并避免溢出卡片",
                    7.5,
                    theme::muted(),
                );
                // 字号预览卡片，用当前字号与颜色显示示例文字。
                let size_preview = Rect::from_min_size(
                    Pos2::new(
                        size_row.left() + SETTING_CARD_PADDING_X,
                        size_row.top() + 98.0,
                    ),
                    Vec2::new(size_row.width() - SETTING_CARD_PADDING_X * 2.0, 32.0),
                );
                theme::fill_round(
                    painter,
                    size_preview,
                    theme::corner_radius(),
                    theme::alpha(theme::GREEN, 12),
                );
                theme::center_text_exact(
                    painter,
                    size_preview.center(),
                    format!("{} px · 世界，等你回来。", self.settings.hero_title_size),
                    8.2,
                    Color32::from_rgb(
                        self.settings.hero_title_color[0],
                        self.settings.hero_title_color[1],
                        self.settings.hero_title_color[2],
                    ),
                    true,
                );

                // 第三行：逐字样式面板（选择某个字符单独调色/调字号）。
                let third_row_y = second_row_y + 152.0;
                let row = view.content_motion.rect(Rect::from_min_size(
                    Pos2::new(source_row_left, third_row_y),
                    Vec2::new(source_row_width, 132.0),
                ));
                paint_setting_card(painter, row, time);
                theme::bold_text(
                    painter,
                    Pos2::new(row.left() + SETTING_CARD_PADDING_X, row.top() + 8.0),
                    "逐字样式",
                    8.8,
                    theme::ink(),
                );
                theme::text(
                    painter,
                    Pos2::new(row.left() + 82.0, row.top() + 10.0),
                    "选择标题中的一个字，单独调整颜色和大小",
                    7.2,
                    theme::muted(),
                );
                // 收集所有可点击的字符（排除换行与控制字符），保留其原始索引以便后续映射。
                let selectable = self
                    .settings
                    .hero_title
                    .chars()
                    .enumerate()
                    .filter(|(_, character)| *character != '\n' && !character.is_control())
                    .collect::<Vec<_>>();
                // 若当前选中的索引已不再合法（如删除了该字符），则自动选中第一个可点击字符。
                if let Some((first, _)) = selectable.first()
                    && !selectable
                        .iter()
                        .any(|(index, _)| *index == self.hero_title_selected_char)
                {
                    self.hero_title_selected_char = *first;
                }
                // 字符选择区占面板约 56% 宽，据此计算每行最多能放几列字符按钮（每格 31px）。
                let char_area_width = row.width() * 0.56;
                let char_columns = ((char_area_width - SETTING_CARD_PADDING_X) / 31.0)
                    .floor()
                    .max(1.0) as usize;
                // 逐个绘制可选字符按钮（最多显示 3 行，超出则不再绘制）。
                for (order, (character_index, character)) in selectable.iter().copied().enumerate()
                {
                    // 超过 3 行时停止绘制剩余字符按钮。
                    if order / char_columns >= 3 {
                        break;
                    }
                    let button = Rect::from_min_size(
                        Pos2::new(
                            row.left()
                                + SETTING_CARD_PADDING_X
                                + (order % char_columns) as f32 * 31.0,
                            row.top() + 32.0 + (order / char_columns) as f32 * 30.0,
                        ),
                        Vec2::new(27.0, 27.0),
                    );
                    let response = ui
                        .interact(
                            button,
                            Id::unique(("hero-title-character", character_index)),
                            Sense::click(),
                        )
                        .on_hover_cursor(CursorIcon::PointingHand);
                    // 判断是否为当前选中的字符。
                    let selected = self.hero_title_selected_char == character_index;
                    theme::fill_round(
                        painter,
                        button,
                        theme::corner_radius(),
                        if selected {
                            theme::alpha(theme::GREEN, 28)
                        } else if response.hovered() {
                            theme::white_alpha(14)
                        } else {
                            theme::white_alpha(7)
                        },
                    );
                    theme::stroke_round(
                        painter,
                        button,
                        theme::corner_radius(),
                        1.0,
                        if selected {
                            theme::alpha(theme::GREEN_BRIGHT, 110)
                        } else {
                            theme::white_alpha(24)
                        },
                    );
                    // 取该字符自身的颜色，并据此计算预览字号（按 0.25 缩放，限制在 9~18px）。
                    let color = self.settings.hero_title_char_colors[character_index];
                    let preview_size =
                        (self.settings.hero_title_char_sizes[character_index] as f32 * 0.25)
                            .clamp(9.0, 18.0);
                    theme::center_text_exact(
                        painter,
                        button.center(),
                        // 在按钮中央绘制字符（空格用“·”表示，便于辨识）。
                        if character == ' ' {
                            "·".to_owned()
                        } else {
                            character.to_string()
                        },
                        preview_size,
                        Color32::from_rgb(color[0], color[1], color[2]),
                        true,
                    );
                    if response.clicked() {
                        self.hero_title_selected_char = character_index;
                    }
                }

                // 若当前选中的字符仍存在于标题中，则在面板右侧绘制其独立的颜色与字号控件。
                if let Some((character_index, character)) = selectable
                    .iter()
                    .copied()
                    .find(|(index, _)| *index == self.hero_title_selected_char)
                {
                    // 控件区位于面板右侧 60% 处起，到右边距止。
                    let controls_left = row.left() + row.width() * 0.60;
                    let controls_width = row.right() - controls_left - SETTING_CARD_PADDING_X;
                    let character_label = if character == ' ' {
                        "空格".to_owned()
                    } else {
                        character.to_string()
                    };
                    theme::bold_text(
                        painter,
                        Pos2::new(controls_left, row.top() + 8.0),
                        format!("当前：{character_label}"),
                        8.2,
                        theme::ink(),
                    );
                    // 该字符当前的颜色（可独立修改）。
                    let color = &mut self.settings.hero_title_char_colors[character_index];
                    let swatch = Rect::from_min_size(
                        Pos2::new(row.right() - 34.0, row.top() + 8.0),
                        Vec2::new(20.0, 20.0),
                    );
                    theme::fill_round(
                        painter,
                        swatch,
                        theme::corner_radius(),
                        Color32::from_rgb(color[0], color[1], color[2]),
                    );
                    theme::stroke_round(
                        painter,
                        swatch,
                        theme::corner_radius(),
                        1.0,
                        theme::white_alpha(42),
                    );
                    // 三个通道的强调色与名称、控件 id。
                    let channel_accents = [
                        Color32::from_rgb(232, 86, 86),
                        Color32::from_rgb(75, 194, 112),
                        Color32::from_rgb(81, 137, 239),
                    ];
                    let channel_names = ["R", "G", "B"];
                    let channel_ids = [
                        "hero-title-character-red",
                        "hero-title-character-green",
                        "hero-title-character-blue",
                    ];
                    // 依次为 R / G / B 通道绘制独立数值滑块（0~255），并把修改写回颜色数组。
                    for channel in 0..3 {
                        let mut value = color[channel] as u16;
                        let before = value;
                        paint_compact_value_slider(
                            ui,
                            painter,
                            Rect::from_min_size(
                                Pos2::new(controls_left, row.top() + 30.0 + channel as f32 * 18.0),
                                Vec2::new(controls_width, 17.0),
                            ),
                            channel_ids[channel],
                            channel_names[channel],
                            &mut value,
                            0,
                            255,
                            "",
                            channel_accents[channel],
                        );
                        // 若滑块值发生变化，则写回该通道的颜色。
                        if value != before {
                            color[channel] = value as u8;
                        }
                    }
                    paint_compact_value_slider(
                        ui,
                        painter,
                        Rect::from_min_size(
                            Pos2::new(controls_left, row.top() + 86.0),
                            Vec2::new(controls_width, 18.0),
                        ),
                        "hero-title-character-size",
                        "字号",
                        &mut self.settings.hero_title_char_sizes[character_index],
                        8,
                        96,
                        " px",
                        theme::GREEN,
                    );
                    // “应用到全部文字”按钮：把当前字符的颜色与字号批量套用到所有字符。
                    let apply_all = Rect::from_min_size(
                        Pos2::new(controls_left, row.top() + 106.0),
                        Vec2::new(controls_width.min(126.0), 18.0),
                    );
                    if settings_button(
                        ui,
                        painter,
                        apply_all,
                        "hero-title-character-apply-all",
                        "应用到全部文字",
                        false,
                    ) {
                        let selected_color = self.settings.hero_title_char_colors[character_index];
                        let selected_size = self.settings.hero_title_char_sizes[character_index];
                        self.settings.hero_title_char_colors.fill(selected_color);
                        self.settings.hero_title_char_sizes.fill(selected_size);
                    }
                } else {
                    theme::center_text(
                        painter,
                        Pos2::new(row.center().x, row.center().y + 8.0),
                        "输入标题文字后可逐字设置颜色与大小",
                        8.0,
                        theme::muted(),
                        false,
                    );
                }
            }
            // ===== 外观区块：以两列卡片网格的形式排布各项外观设置 =====
            SettingsSection::Appearance => {
                // 单张外观卡片的标准高度（像素）。
                let card_height = 70.0;
                // 两列卡片之间的水平间隙。
                let column_gap = 12.0;
                // 上下两行卡片之间的垂直间隙（注意：本区块内的 row_gap 与外层 74px 的行距不同）。
                let row_gap = 8.0;
                // 单张卡片宽度：可用宽度减去间隙后平分两列，且最小不低于 260px 防止过窄。
                let card_width = ((source_row_width - column_gap) * 0.5).max(260.0);
                // 构造一个闭包：按 (行索引, 列索引) 计算某张外观卡片的矩形。
                // 行从 0 开始计数，列 0 表示左列、列 1 表示右列；坐标会经内容运动参数变换（转场动画）。
                let appearance_row = |row_index: usize, column: usize| {
                    view.content_motion.rect(Rect::from_min_size(
                        Pos2::new(
                            // 横向：内容区左缘 + 列偏移（每列宽度 + 间隙）。
                            source_row_left + column as f32 * (card_width + column_gap),
                            // 纵向：内容区顶部 96px 起，每行下移 (卡片高度 + 行间隙)。
                            source_panel.top() + 96.0 + row_index as f32 * (card_height + row_gap),
                        ),
                        Vec2::new(card_width, card_height),
                    ))
                };

                // ===== 第 0 行（整行宽）：界面风格选择（简洁 / 磨砂 / 液态玻璃） =====
                let row = view.content_motion.rect(Rect::from_min_size(
                    Pos2::new(source_row_left, source_panel.top() + 96.0),
                    Vec2::new(source_row_width, card_height),
                ));
                // 绘制该行的玻璃卡片背景。
                paint_setting_card(painter, row, time);
                // 描述文字随当前选中的外观模式动态变化。
                paint_setting_label(
                    painter,
                    row,
                    "界面风格",
                    match self.settings.appearance_mode {
                        // 简洁模式：纯白背景、黑色文字，无模糊和折射。
                        theme::AppearanceMode::Simple => "纯白背景、黑色文字，无模糊和折射",
                        // 磨砂模式：纯白底上的实时模糊、透光与玻璃折射效果。
                        theme::AppearanceMode::Frosted => {
                            "纯白底上的实时模糊、透光与当前玻璃折射效果"
                        }
                        // 液态玻璃模式尚未实装，仅作预告展示。
                        theme::AppearanceMode::Liquid => "液态玻璃将在后续版本开放",
                    },
                );
                // “简洁”按钮矩形（三个按钮中最靠左的一个）。
                let liquid = setting_right_control_rect(row, Vec2::new(102.0, 46.0));
                // “磨砂”按钮矩形（居中）。
                let frosted_size = Vec2::new(88.0, 36.0);
                let frosted_center_y = setting_card_rect(row).center().y;
                let frosted = Rect::from_center_size(
                    Pos2::new(liquid.left() - 8.0 - frosted_size.x * 0.5, frosted_center_y),
                    frosted_size,
                );
                let simple_size = Vec2::new(76.0, 36.0);
                let simple = Rect::from_center_size(
                    Pos2::new(frosted.left() - 8.0 - simple_size.x * 0.5, frosted_center_y),
                    simple_size,
                );
                // 遍历“简洁”与“磨砂”两个可选模式，绘制可点击的选择按钮。
                // 按钮的选中态（第三参数）取决于当前 appearance_mode 是否等于该模式；
                // 点击后把设置切换到对应模式（会立即持久化）。
                for (button, mode, id) in [
                    (simple, theme::AppearanceMode::Simple, "appearance-simple"),
                    (
                        frosted,
                        theme::AppearanceMode::Frosted,
                        "appearance-frosted",
                    ),
                ] {
                    if settings_button(
                        ui,
                        painter,
                        button,
                        id,
                        mode.label(),
                        self.settings.appearance_mode == mode,
                    ) {
                        // 点击命中：切换界面风格模式。
                        self.settings.appearance_mode = mode;
                    }
                }
                // ===== “液态玻璃”占位卡片（不可点击，纯展示） =====
                // 浅灰色圆角底 + 深色描边，营造“未开放”的灰置观感。
                theme::fill_round(
                    painter,
                    liquid,
                    theme::corner_radius(),
                    Color32::from_rgb(236, 238, 235),
                );
                theme::stroke_round(
                    painter,
                    liquid,
                    theme::corner_radius(),
                    1.0,
                    Color32::from_black_alpha(28),
                );
                // 上半部分显示模式名称。
                theme::center_text(
                    painter,
                    Pos2::new(liquid.center().x, liquid.top() + 13.0),
                    "液态玻璃",
                    8.5,
                    theme::ink_soft(),
                    false,
                );
                // 下半部分显示“敬请期待”小字提示。
                theme::center_text(
                    painter,
                    Pos2::new(liquid.center().x, liquid.bottom() - 10.0),
                    "敬请期待",
                    7.2,
                    theme::muted(),
                    false,
                );

                // ===== 第 1 行左列：动态界面（动画强度四档选择） =====
                let row = appearance_row(1, 0);
                paint_setting_card(painter, row, time);
                paint_setting_label(
                    painter,
                    row,
                    "动态界面",
                    "控制界面切换、弹窗和环境动画的幅度",
                );
                // 遍历四档动画等级（关/弱/中/强），从左到右依次绘制等宽按钮。
                let animation_card = setting_card_rect(row);
                let animation_button_size = Vec2::new(42.0, 36.0);
                let animation_button_gap = 6.0;
                let animation_right_margin =
                    (animation_card.height() - animation_button_size.y) * 0.5;
                let animation_group_left = animation_card.right()
                    - animation_right_margin
                    - 4.0 * animation_button_size.x
                    - 3.0 * animation_button_gap;
                for (index, level) in AnimationLevel::ALL.into_iter().enumerate() {
                    // 最右侧控制到顶部、底部、右侧的距离保持一致。
                    let button = Rect::from_min_size(
                        Pos2::new(
                            animation_group_left
                                + index as f32 * (animation_button_size.x + animation_button_gap),
                            animation_card.center().y - animation_button_size.y * 0.5,
                        ),
                        animation_button_size,
                    );
                    // 绘制并处理点击：选中态由当前 animation_level 决定，点击后切换等级。
                    if settings_button(
                        ui,
                        painter,
                        button,
                        // 每个等级对应一个唯一的控件 id，保证 egui 状态互不干扰。
                        match level {
                            AnimationLevel::Off => "animation-level-off",
                            AnimationLevel::Weak => "animation-level-weak",
                            AnimationLevel::Medium => "animation-level-medium",
                            AnimationLevel::Strong => "animation-level-strong",
                        },
                        level.label(),
                        self.settings.animation_level == level,
                    ) {
                        // 点击命中：更新动画等级设置。
                        self.settings.animation_level = level;
                    }
                }

                // ===== 第 1 行右列：导航提示开关（悬停左侧导航时显示功能名称） =====
                let row = appearance_row(1, 1);
                paint_setting_card(painter, row, time);
                paint_toggle_row(
                    ui,
                    painter,
                    row,
                    "setting-tooltips",
                    "导航提示",
                    "悬停左侧导航时显示功能名称",
                    &mut self.settings.tooltips,
                );

                // ===== 第 2 行左列：卡片透明度滑块（简洁模式下不适用） =====
                let row = appearance_row(2, 0);
                paint_setting_card(painter, row, time);
                // 简洁模式没有玻璃卡片，透明度无意义，显示“不适用”徽标。
                if self.settings.appearance_mode == theme::AppearanceMode::Simple {
                    paint_setting_label(painter, row, "卡片透明度", "简洁模式不使用透明效果");
                    paint_status_badge(painter, row, "不适用");
                } else {
                    // 非简洁模式：绘制 20%~100%（步进 5）的透明度滑块，直接绑定到设置。
                    paint_effect_slider(
                        ui,
                        painter,
                        row,
                        "setting-glass-transparency",
                        "卡片透明度",
                        "数值越高，背景越清晰",
                        &mut self.settings.glass_transparency,
                        20,
                        100,
                        5,
                        "%",
                    );
                }

                // ===== 第 2 行右列：鼠标光亮度滑块（简洁模式下不适用） =====
                let row = appearance_row(2, 1);
                paint_setting_card(painter, row, time);
                if self.settings.appearance_mode == theme::AppearanceMode::Simple {
                    paint_setting_label(painter, row, "鼠标光亮度", "简洁模式关闭鼠标光效");
                    paint_status_badge(painter, row, "不适用");
                } else {
                    // 范围 0%~200%（步进 10）：超过 100% 表示比默认更亮的光晕。
                    paint_effect_slider(
                        ui,
                        painter,
                        row,
                        "setting-pointer-brightness",
                        "鼠标光亮度",
                        "0% 可完全关闭鼠标光晕",
                        &mut self.settings.pointer_brightness,
                        0,
                        200,
                        10,
                        "%",
                    );
                }

                // ===== 第 3 行左列：光效范围滑块（简洁模式下不适用） =====
                let row = appearance_row(3, 0);
                paint_setting_card(painter, row, time);
                if self.settings.appearance_mode == theme::AppearanceMode::Simple {
                    paint_setting_label(painter, row, "光效范围", "简洁模式关闭鼠标光效");
                    paint_status_badge(painter, row, "不适用");
                } else {
                    // 范围 40~320px（步进 10）：控制光晕与玻璃受光的半径。
                    paint_effect_slider(
                        ui,
                        painter,
                        row,
                        "setting-pointer-radius",
                        "光效范围",
                        "同时影响光晕和玻璃受光范围",
                        &mut self.settings.pointer_radius,
                        40,
                        320,
                        10,
                        " px",
                    );
                }

                // ===== 第 3 行右列：光效刷新率滑块（简洁模式下不适用） =====
                let row = appearance_row(3, 1);
                paint_setting_card(painter, row, time);
                if self.settings.appearance_mode == theme::AppearanceMode::Simple {
                    paint_setting_label(painter, row, "光效刷新率", "简洁模式关闭鼠标光效");
                    paint_status_badge(painter, row, "不适用");
                } else {
                    // 范围 20~144 FPS（步进 4）：限制鼠标快速移动时光效引发的重绘频率。
                    paint_effect_slider(
                        ui,
                        painter,
                        row,
                        "setting-pointer-refresh",
                        "光效刷新率",
                        "快速移动鼠标时的最高重绘频率",
                        &mut self.settings.pointer_refresh_rate,
                        20,
                        144,
                        4,
                        " FPS",
                    );
                }

                // ===== 第 4 行左列：描边清晰度滑块 =====
                let row = appearance_row(4, 0);
                paint_setting_card(painter, row, time);
                paint_effect_slider(
                    ui,
                    painter,
                    row,
                    "setting-card-edge-clarity",
                    "描边清晰度",
                    "0 为无描边；数值越高，卡片边线越清晰",
                    &mut self.settings.card_edge_clarity,
                    0,
                    100,
                    5,
                    "%",
                );

                // ===== 第 4 行右列：自定义首页背景图片 =====
                let row = appearance_row(4, 1);
                paint_setting_card(painter, row, time);
                let remove_size = Vec2::new(66.0, 32.0);
                let import_size = Vec2::new(74.0, 32.0);
                let remove = setting_right_control_rect(row, remove_size);
                let import = Rect::from_min_size(
                    Pos2::new(remove.left() - 6.0 - import_size.x, remove.top()),
                    import_size,
                );
                let label_painter = painter.with_clip_rect(Rect::from_min_max(
                    row.left_top(),
                    Pos2::new(import.left() - 8.0, row.bottom()),
                ));
                let background_description = if self.settings.background_image.is_some() {
                    "已导入 · 铺满整个启动器背景"
                } else {
                    "默认无背景 · 支持 PNG/JPEG/WebP，铺满整个窗口"
                };
                paint_setting_label(&label_painter, row, "背景图片", background_description);
                let import_label = if self.settings.background_image.is_some() {
                    "更换"
                } else {
                    "导入"
                };
                if settings_button(
                    ui,
                    painter,
                    import,
                    "appearance-background-import",
                    import_label,
                    false,
                ) {
                    let context = ui.ctx().clone();
                    self.import_background_image(&context);
                }
                if self.settings.background_image.is_some()
                    && settings_button(
                        ui,
                        painter,
                        remove,
                        "appearance-background-remove",
                        "移除",
                        false,
                    )
                {
                    self.remove_background_image();
                    ui.ctx().request_repaint();
                }

                // ===== 第 5 行（整行宽）：窗口与全部控件的统一圆角 =====
                // 增加的设置行使用与其他外观卡片相同的纵向节奏，并压缩卡片高度与间隙，
                // 保证在 720px 最小窗口高度下仍完整位于设置内容区内。
                let row = view.content_motion.rect(Rect::from_min_size(
                    Pos2::new(
                        source_row_left,
                        source_panel.top() + 96.0 + 5.0 * (card_height + row_gap),
                    ),
                    Vec2::new(source_row_width, card_height),
                ));
                paint_setting_card(painter, row, time);
                paint_effect_slider(
                    ui,
                    painter,
                    row,
                    "setting-corner-radius",
                    "圆角大小",
                    "同时作用于窗口、卡片、按钮与输入框，修改后立即生效",
                    &mut self.settings.corner_radius,
                    theme::MIN_CORNER_RADIUS as u16,
                    theme::MAX_CORNER_RADIUS as u16,
                    2,
                    " px",
                );
            }
            // ===== 关于区块：单张大卡片，展示品牌信息与功能清单 =====
            SettingsSection::About => {
                // 关于卡片：高度 258px，宽度与普通设置行一致。
                let card = setting_row(row_y, 258.0);
                paint_setting_card(painter, card, time);
                // 应用图标中心：位于卡片左侧、距顶部 76px 处。
                let icon_center = Pos2::new(card.left() + 70.0, card.top() + 76.0);
                // 在图标下方绘制一团柔和的绿色光晕（半径 64px），增强品牌氛围。
                theme::soft_orb(
                    painter,
                    icon_center + Vec2::new(0.0, 8.0),
                    64.0,
                    theme::GREEN,
                    13,
                );
                // 应用图标区域：以 icon_center 为中心、边长 92px 的正方形。
                let about_icon = Rect::from_center_size(icon_center, Vec2::splat(92.0));
                // 优先绘制加载好的应用图标纹理（release 图标图片）。
                if let Some(texture) = self.release_icon_texture.as_ref() {
                    paint_rounded_image(
                        painter,
                        about_icon,
                        theme::corner_radius() as f32,
                        texture,
                        texture.size(),
                    );
                } else {
                    // 纹理尚未加载时回退绘制一个程序生成的“方块”像素图案占位。
                    paint_block(
                        painter,
                        icon_center,
                        72.0,
                        Color32::from_rgb(128, 216, 110),
                        Color32::from_rgb(89, 66, 48),
                        Color32::from_rgb(71, 150, 72),
                    );
                }
                // 右侧信息区起点：图标右侧 140px 处（左右留出图标宽度）。
                let info_left = card.left() + 140.0;
                // 品牌名称（大号粗体）。
                theme::bold_text(
                    painter,
                    Pos2::new(info_left, card.top() + 34.0),
                    brand::NAME,
                    25.0,
                    theme::ink(),
                );
                // 副标题：产品名 + 版本号 + 平台说明。
                theme::text(
                    painter,
                    Pos2::new(info_left + 1.0, card.top() + 68.0),
                    "方块启动器 · Version 0.1.0 · 原生桌面",
                    9.4,
                    theme::muted(),
                );
                // 一句话功能概述。
                theme::text(
                    painter,
                    Pos2::new(info_left + 1.0, card.top() + 91.0),
                    "原生桌面启动器 · 离线游戏启动与本地目录管理",
                    9.0,
                    theme::ink_soft(),
                );
                // 信息区下方画一条分隔线，把简介与功能清单隔开。
                painter.line_segment(
                    [
                        Pos2::new(info_left, card.top() + 119.0),
                        Pos2::new(card.right() - 24.0, card.top() + 119.0),
                    ],
                    Stroke::new(1.0, theme::line()),
                );
                // “支持的功能”小节标题。
                theme::text(
                    painter,
                    Pos2::new(info_left, card.top() + 129.0),
                    "支持的功能",
                    8.2,
                    theme::muted(),
                );
                // 功能清单（共 6 条，按两列三行排布为胶囊标签）。
                let detail = [
                    "官方版本下载与文件补全",
                    "Fabric / Forge / NeoForge",
                    "模组、光影、材质与数据包",
                    "整合包导入与版本隔离",
                    "3D 皮肤编辑与 RGB 调色",
                    "模组工程创建与 Java 选择",
                ];
                // 胶囊标签之间的水平间隙。
                let chip_gap = 12.0;
                // 单个胶囊宽度：可用宽度减去间隙后平分两列，最小不低于 160px。
                let chip_width = (card.right() - info_left - 24.0 - chip_gap).max(160.0) * 0.5;
                // 逐个绘制功能胶囊标签：偶数索引在左列、奇数索引在右列，每两条下移一行。
                for (index, label) in detail.into_iter().enumerate() {
                    // 列索引（0=左列，1=右列）与行索引（0/1/2）。
                    let column = index % 2;
                    let row = index / 2;
                    // 胶囊矩形：顶部从 151px 起，每行下移 31px。
                    let chip = Rect::from_min_size(
                        Pos2::new(
                            info_left + column as f32 * (chip_width + chip_gap),
                            card.top() + 151.0 + row as f32 * 31.0,
                        ),
                        Vec2::new(chip_width, 25.0),
                    );
                    // 胶囊底色与描边均为淡绿色调，呼应品牌色。
                    theme::fill_round(
                        painter,
                        chip,
                        theme::corner_radius(),
                        theme::alpha(theme::GREEN, 18),
                    );
                    theme::stroke_round(
                        painter,
                        chip,
                        theme::corner_radius(),
                        1.0,
                        theme::alpha(theme::GREEN, 34),
                    );
                    // 居中绘制标签文字；文字过长时自动缩小字号以适配胶囊宽度。
                    theme::center_text(
                        painter,
                        chip.center(),
                        fit_text_to_width(painter, label, theme::font(7.4), chip.width() - 18.0),
                        7.4,
                        theme::ink_soft(),
                        false,
                    );
                }
            }
        }

        // ===== 底部页脚：保存状态提示 + “恢复默认”按钮（仅完整视图绘制） =====
        if view.draw_footer {
            // 页脚基线 Y：内容面板底部上方 56px 处。
            let footer_y = panel.bottom() - 56.0;
            // 保存状态提示：比较当前设置与已保存快照，不同则说明正在自动保存。
            theme::text(
                painter,
                Pos2::new(panel.left() + 26.0, footer_y + 9.0),
                if self.settings == self.saved_settings {
                    "修改会自动保存到本机"
                } else {
                    "正在自动保存…"
                },
                8.0,
                theme::muted(),
            );
            // “恢复默认”按钮矩形（页脚右侧）。
            let reset = Rect::from_min_size(
                Pos2::new(panel.right() - 114.0, footer_y),
                Vec2::new(96.0, 38.0),
            );
            // 点击“恢复默认”：把所有设置重置为出厂默认值并刷新相关状态。
            if settings_button(ui, painter, reset, "settings-reset", "恢复默认", false) {
                // 用默认设置整体替换当前设置。
                self.settings = LauncherSettings::default();
                // 恢复默认后重新扫描系统常见 Minecraft 目录并选中第一个。
                self.settings.game_directories = crate::game::discover_game_directories();
                self.settings.game_directory = self.settings.game_directories.first().cloned();
                // 根据新目录重新加载版本列表。
                self.versions = available_versions(self.settings.game_directory.as_deref());
                // 刷新首页的最近游玩记录（目录变了，记录可能失效）。
                self.refresh_recent_sessions();
                // 默认选中第一个“完整可启动”的版本；找不到则回退到第 0 项。
                self.selected_version = self
                    .versions
                    .iter()
                    .position(|version| version.launchable)
                    .unwrap_or(0);
                // 版本列表滚动位置复位。
                self.version_scroll = 0.0;
                // 弹出提示告知用户恢复完成。
                self.show_toast("设置已恢复为默认值");
            }
        }
    }
}
