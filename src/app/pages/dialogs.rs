//! 下拉菜单与各类对话框 / 弹窗的绘制与交互逻辑。
//!
//! 本文件是启动器主应用状态 `LauncherApp` 的一个 `impl` 实现块，集中处理：
//! - 顶部下拉菜单（版本选择、玩家资料）的绘制与开合动画；
//! - 若干浮动弹层（模态对话框、Toast 提示、官方版本浏览器、安装/导入进度等）
//!   的绘制与事件响应。
//!
//! 主要定义的函数（均为 `pub(in crate::app)` 的方法）：
//! - `paint_menus`：根据当前菜单状态绘制版本或资料下拉面板。
//! - `paint_version_menu` / `paint_profile_menu`：两个下拉面板的具体内容。
//! - `paint_account_name_dialog`：更改玩家昵称的模态对话框。
//! - `open_*` / `save_*` / `choose_*`：打开与处理各类对话框/交互的辅助方法。
//! - `paint_news_dialog` / `paint_forced_update` / `paint_toast` /
//!   `paint_official_browser` / `paint_install_progress` /
//!   `paint_modpack_import_progress`：各类浮层与进度的绘制。
//!
//! 在启动器中的角色：
//! 这些函数是 UI 层与状态层之间的桥梁——读取 `LauncherApp` 中的字段
//! （如 `menu`、`selected_version`、`settings`、`toast` 等），把它们渲染成
//! egui 图形，并把用户的点击、输入等交互回写到对应的状态字段，从而驱动
//! 启动器的界面与行为。

use super::super::*;

impl LauncherApp {
    /// 绘制顶部下拉菜单（版本列表或玩家资料），并驱动其开合动画。
    ///
    /// 根据 `self.menu` 以及两个弹出值（`version_popup_value` /
    /// `profile_popup_value`）判断是否需要在前景层绘制下拉面板：当菜单处于激活态，
    /// 或对应的弹出动画值仍大于阈值（`> 0.002`，说明正在播放关闭动画）时，会继续绘制。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文，用于创建浮动层、对图层做缩放/位移变换以及请求重绘。
    /// - `layout`：当前界面的布局信息，用于计算菜单面板的几何位置。
    /// - `time`：累积时间（秒），用于玻璃质感等随时间变化的效果。
    ///
    /// # 副作用
    /// 可能调用 `ctx.transform_layer_shapes` 对菜单层做缩放变换，并在动画进行中
    /// 调用 `ctx.request_repaint_after` 以持续刷新画面。
    ///
    /// # 不变量
    /// 当 `show_version` 与 `show_profile` 均为假（两个菜单都完全关闭）时直接返回，
    /// 不绘制任何内容，也不请求重绘。
    pub(in crate::app) fn paint_menus(&mut self, ctx: &egui::Context, layout: &Layout, time: f64) {
        // 是否显示版本菜单：当前激活态为版本菜单，或版本菜单的关闭动画尚未结束（值 > 0.002）。
        let show_version = self.menu == Menu::Version || self.version_popup_value > 0.002;
        // 是否显示资料菜单：同样逻辑，针对 profile 的弹出值。
        let show_profile = self.menu == Menu::Profile || self.profile_popup_value > 0.002;
        // 两个菜单都完全关闭（处于 None 且动画已归零）时无需绘制，直接返回。
        if !show_version && !show_profile {
            return;
        }
        // 判断当前到底绘制哪个菜单：版本菜单优先；否则若当前处于资料菜单、或资料菜单的
        // 关闭动画仍在进行（show_version 在此作为兜底）时，则绘制资料菜单。
        let is_version = self.menu == Menu::Version || (self.menu != Menu::Profile && show_version);
        // 根据要绘制的菜单类型，取对应的菜单面板矩形、弹簧动画值与"是否处于打开态"：
        // - 版本菜单：矩形大小与可选择的版本数量相关（行数决定高度）；
        // - 资料菜单：矩形为固定尺寸。
        // `open` 用于区分"正在打开/保持打开"与"正在播放关闭动画"两种状态。
        let (menu_rect, spring_value, open) = if is_version {
            (
                version_menu_rect(layout, self.selectable_version_order().len()),
                self.version_popup_value,
                self.menu == Menu::Version,
            )
        } else {
            (
                profile_menu_rect(layout),
                self.profile_popup_value,
                self.menu == Menu::Profile,
            )
        };
        // 由弹簧动画值与开合状态计算面板整体不透明度（关闭动画时逐渐变透明）。
        let opacity = popup_opacity(spring_value, open);
        // 在"前景层"创建一个浮动 Area 承载下拉面板：
        // - order=Foreground 保证面板盖在普通内容之上；
        // - fixed_pos 固定在面板矩形左上角、movable(false) 禁止拖动；
        // - interactable(true) 允许面板内部接收点击/悬停事件。
        egui::Area::new(Id::unique("menu-foreground-layer"))
            .order(egui::Order::Foreground)
            .fixed_pos(menu_rect.min)
            .movable(false)
            .interactable(true)
            .show(ctx, |ui| {
                // 把 Area 的内容区域限制为菜单面板的实际大小，并按不透明度渲染。
                ui.set_min_size(menu_rect.size());
                ui.set_opacity(opacity);
                // 克隆一份画笔供后续直接绘制（面板内容不依赖 egui 控件布局）。
                let painter = ui.painter().clone();
                // 弹出动画值映射到缩放系数：0 → 0.82（略小），1 → 1.0（原始大小）；
                // clamp 到 [-0.05, 1.12] 允许轻微"过冲"回弹，使开合更有弹性。
                let scale = 0.82 + 0.18 * spring_value.clamp(-0.05, 1.12);
                if is_version {
                    self.paint_version_menu(ui, &painter, layout, time);
                    // 以右上角为锚点做缩放变换，使面板像从菜单按钮"展开"出来。
                    let anchor = menu_rect.right_top().to_vec2();
                    let transform = egui::emath::TSTransform::new(anchor - anchor * scale, scale);
                    ctx.transform_layer_shapes(ui.layer_id(), transform);
                } else {
                    self.paint_profile_menu(ctx, ui, &painter, layout, time);
                    // 以左下角为锚点做缩放变换，使资料面板从对应按钮处展开。
                    let anchor = menu_rect.left_bottom().to_vec2();
                    let transform = egui::emath::TSTransform::new(anchor - anchor * scale, scale);
                    ctx.transform_layer_shapes(ui.layer_id(), transform);
                }
            });
        // 动画尚未结束（任一菜单仍可见/正在关闭）时，以约 60fps 的节奏请求重绘，
        // 使弹簧动画与缩放变换能够平滑推进。
        if (self.menu == Menu::Version || self.version_popup_value > 0.002)
            || (self.menu == Menu::Profile || self.profile_popup_value > 0.002)
        {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    /// 绘制"选择游戏版本"下拉面板的具体内容。
    ///
    /// 列出所有可启动（或需要补全）的游戏版本，支持鼠标滚轮滚动、悬停高亮、
    /// 选中标记（"✓"）与"需补全"提示。点击某一项会切换 `selected_version` 并关闭菜单。
    ///
    /// # 参数
    /// - `ui`：egui UI 句柄，用于创建可交互区域与捕获滚轮滚动。
    /// - `painter`：用于直接绘制图形/文字的画笔（来自上层 `Area`）。
    /// - `layout`：界面布局，决定面板尺寸与位置。
    /// - `time`：累积时间（秒），供玻璃质感动画使用。
    ///
    /// # 副作用
    /// 点击版本项时会写入 `selected_version`，调用 `switch_version_detail`、
    /// 关闭菜单（置 `menu = Menu::None`）、重置 `launch_complete`，并弹出 Toast 提示。
    ///
    /// # 关键不变量
    /// 列表采用"视口裁剪"：仅绘制处于可见区域内的行（行顶越界上方或下方时 `continue`），
    /// 以支撑长版本列表的绘制性能。
    pub(in crate::app) fn paint_version_menu(
        &mut self,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
    ) {
        // 收集可选择的版本：按"最近游玩优先"的顺序，得到 (原始索引, 版本) 的列表，
        // 原始索引用于在点击时回写 selected_version 并定位真正的版本数据。
        let versions: Vec<(usize, GameVersion)> = self
            .selectable_version_order()
            .into_iter()
            .map(|index| (index, self.versions[index].clone()))
            .collect();
        // 绘制整块玻璃质感面板底板：深绿底色 + 绿色调 + 背景模糊，
        // time 传入使面板内的噪声/光泽随时间轻微流动。
        let rect = version_menu_rect(layout, versions.len());
        theme::glass_panel(
            painter,
            rect,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(22, 31, 25), theme::GREEN, time, 6.4)
                .backdrop_blur(14.0),
        );
        // 在面板上裁剪出一个内容画笔：后续绘制都会被限制在面板矩形内（圆角裁剪），
        // 避免列表滚动时内容溢出面板边界。
        let content = card_clip(painter, rect);
        let painter = &content;
        // 面板标题：左上角说明"最近游玩优先"的排序规则。
        theme::text(
            painter,
            Pos2::new(rect.left() + 14.0, rect.top() + 11.0),
            "选择游戏版本 · 最近游玩优先",
            9.4,
            theme::ink_soft(),
        );
        // 右上角统计文案：展示当前可启动版本的总数。
        theme::text(
            painter,
            Pos2::new(rect.right() - 104.0, rect.top() + 11.0),
            format!("{} 个可启动版本", versions.len()),
            7.3,
            theme::muted(),
        );
        // 列表可视区域的上/下边界（留出标题与底部内边距）；可视高度至少 1px 避免除零。
        let list_top = rect.top() + 38.0;
        let list_bottom = rect.bottom() - 7.0;
        let visible_height = (list_bottom - list_top).max(1.0);
        // 内容总高度 = 行数 × 单行高度；可滚动的最大距离 = 内容高度 - 可视高度（不小于 0）。
        let content_height = versions.len() as f32 * VERSION_ROW_HEIGHT;
        let max_scroll = (content_height - visible_height).max(0.0);
        // 当指针悬停在面板内且产生平滑滚轮增量时，按 1.05 倍系数（略加速）更新滚动偏移。
        if ui.ctx().input(|input| {
            input
                .pointer
                .hover_pos()
                .is_some_and(|pointer| rect.contains(pointer))
                && input.smooth_scroll_delta.y.abs() > 0.0
        }) {
            self.version_scroll -= ui.ctx().input(|input| input.smooth_scroll_delta.y) * 1.05;
        }
        // 将滚动偏移限制在 [0, max_scroll]，避免滚出内容范围。
        self.version_scroll = self.version_scroll.clamp(0.0, max_scroll);
        // 克隆内容画笔供循环内绘制各版本行（避免可变借用冲突）。
        let list_painter = painter.clone();
        // 遍历所有可选版本行：(version_index, version) 为原始索引与版本数据。
        for (index, (version_index, version)) in versions.iter().enumerate() {
            let item_y = list_top + index as f32 * VERSION_ROW_HEIGHT - self.version_scroll;
            // 视口裁剪：本行（含 47px 行高）完全在可视区上方或下方时跳过绘制，
            // 只绘制可见部分，提升长版本列表的绘制性能。
            if item_y + 47.0 < list_top || item_y > list_bottom {
                continue;
            }
            let item = Rect::from_min_size(
                Pos2::new(rect.left() + 7.0, item_y),
                Vec2::new(rect.width() - 22.0, 47.0),
            );
            // 计算本行的可交互矩形（左右各留 7px/15px 边距），注册点击区域并
            // 设置悬停时为"手型"光标，提示该行可点击。
            let response = ui
                .interact(item, Id::unique(("version-option", index)), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand);
            // 判断该行是否为当前选中的版本（对比原始索引）。
            let selected = self.selected_version == *version_index;
            // 行卡片矩形：在 item 基础上四周略收缩，形成视觉内边距。
            let item_card = item.shrink2(Vec2::new(2.0, 1.5));
            // 绘制行卡片底板：选中或悬停时用更亮的绿色底，否则用普通深色底；
            // GlassSpec 的最后一个参数（index * 0.8）让各行动画相位略有错开。
            theme::glass_panel(
                &list_painter,
                item_card,
                theme::corner_radius(),
                theme::GlassSpec::new(
                    if selected || response.hovered() {
                        Color32::from_rgb(29, 47, 33)
                    } else {
                        Color32::from_rgb(21, 34, 25)
                    },
                    theme::GREEN,
                    time,
                    index as f32 * 0.8,
                )
                .hovered(response.hovered()),
            );
            // 选中的版本行额外描一圈半透明绿色圆角边框以突出显示。
            if selected {
                theme::stroke_round(
                    &list_painter,
                    item_card,
                    theme::corner_radius(),
                    1.0,
                    theme::alpha(theme::GREEN, 68),
                );
            }
            // 行左侧的版本图标区域：优先使用该版本对应的图标贴图（圆角裁剪绘制）；
            // 若没有图标则回退为一块深灰色圆角占位块。
            let mark = Rect::from_center_size(
                Pos2::new(item.left() + 22.0, item.center().y),
                Vec2::splat(28.0),
            );
            if let Some(texture) = self.version_icon(version) {
                paint_rounded_image(&list_painter, mark, 8.0, texture, texture.size());
            } else {
                theme::fill_round(
                    &list_painter,
                    mark,
                    theme::corner_radius(),
                    Color32::from_rgb(53, 66, 57),
                );
            }
            // 取版本的显示标题，并按行可用宽度（减去图标与右侧标记的空间）截断/缩放，
            // 防止过长的版本名溢出行边界。
            let selector_label = self.version_selector_label(version);
            let selector_label = fit_text_to_width(
                &list_painter,
                &selector_label,
                theme::font(10.2),
                (item.width() - 70.0).max(80.0),
            );
            // 主标题文字：可启动的版本用正常文字色，不可启动的用更弱的灰色。
            let selector_detail = self.version_selector_detail(version);
            theme::text(
                &list_painter,
                Pos2::new(item.left() + 42.0, item.top() + 8.0),
                selector_label,
                10.2,
                if version.launchable {
                    theme::ink_soft()
                } else {
                    theme::muted()
                },
            );
            // 副标题（版本详情说明）：按 38 字符截断，避免过长换行。
            theme::text(
                &list_painter,
                Pos2::new(item.left() + 42.0, item.top() + 25.0),
                ellipsize_end(&selector_detail, 38),
                7.4,
                theme::muted(),
            );
            // 行右侧状态标记：选中版本显示"✓"；未启动完成的版本显示"需补全"提示。
            if selected {
                theme::text(
                    &list_painter,
                    Pos2::new(item.right() - 20.0, item.top() + 16.0),
                    "✓",
                    10.0,
                    theme::GREEN,
                );
            } else if !version.launchable {
                theme::text(
                    &list_painter,
                    Pos2::new(item.right() - 57.0, item.top() + 16.0),
                    "需补全",
                    7.4,
                    Color32::from_rgb(164, 148, 118),
                );
            }
            // 点击该版本行：切换选中版本、刷新版本详情视图、关闭菜单，
            // 并根据版本是否可直接启动给出不同的 Toast 提示文案。
            if response.clicked() {
                self.selected_version = *version_index;
                self.switch_version_detail(*version_index);
                self.menu = Menu::None;
                self.launch_complete = false;
                self.show_toast(if version.launchable {
                    format!("已切换至 {} · {}", version.version, version.loader)
                } else {
                    format!("已选择 {}，启动时将自动补全文件", version.version)
                });
            }
        }
        // 没有任何可选择版本时，在列表区域居中显示占位提示。
        if versions.is_empty() {
            theme::center_text(
                &list_painter,
                Pos2::new(rect.center().x, list_top + 58.0),
                "没有可启动的完整版本",
                8.8,
                theme::muted(),
                false,
            );
        }
        // 内容超出可视高度时在面板右侧绘制滚动条：先画轨道，再按滚动进度画滑块。
        if max_scroll > 0.0 {
            // 轨道矩形：紧贴面板右缘、位于列表可视区内。
            let track = Rect::from_min_size(
                Pos2::new(rect.right() - 6.0, list_top + 3.0),
                Vec2::new(3.0, visible_height - 6.0),
            );
            theme::fill_round(
                painter,
                track,
                theme::corner_radius(),
                theme::white_alpha(16),
            );
            // 滚动条滑块高度按"可视区占内容比例"的平方估算（内容越长滑块越短），最小 32px。
            let thumb_height = (visible_height * visible_height / content_height).max(32.0);
            let progress = self.version_scroll / max_scroll;
            let thumb = Rect::from_min_size(
                Pos2::new(
                    track.left(),
                    track.top() + (track.height() - thumb_height) * progress,
                ),
                Vec2::new(track.width(), thumb_height),
            );
            theme::fill_round(
                painter,
                thumb,
                theme::corner_radius(),
                theme::alpha(theme::GREEN, 105),
            );
        }
    }

    /// 绘制"玩家资料"下拉面板的具体内容。
    ///
    /// 展示当前离线玩家头像、昵称与账户类型，并列出若干操作入口行
    /// （更改昵称、更换皮肤、打开游戏目录、切换账户）。点击不同入口会触发对应动作。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文，用于在绘制前确保皮肤贴图就绪（`ensure_skin_texture`）。
    /// - `ui`：egui UI 句柄，用于各操作行的可交互区域。
    /// - `painter`：绘制画笔。
    /// - `layout`：界面布局，决定面板位置。
    /// - `time`：累积时间（秒），供玻璃质感动画使用。
    ///
    /// # 副作用
    /// 点击不同入口会分别调用 `open_account_name_dialog`、`choose_player_skin`、
    /// `open_game_folder`，或对"切换账户"弹出"暂不支持"提示，并在点击后关闭菜单。
    pub(in crate::app) fn paint_profile_menu(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
    ) {
        // 确保玩家皮肤头像贴图已加载（首次调用时异步上传到 GPU，之后直接复用）。
        self.ensure_skin_texture(ctx);
        // 计算资料面板矩形并绘制玻璃质感底板。
        let rect = profile_menu_rect(layout);
        theme::glass_panel(
            painter,
            rect,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(22, 31, 25), theme::GREEN, time, 7.2)
                .backdrop_blur(14.0),
        );
        let content = card_clip(painter, rect);
        let painter = &content;
        // 头像区域：48×48 圆角方块，使用皮肤头部渲染出的贴图（可能为 None，则显示占位）。
        paint_avatar(
            painter,
            Rect::from_min_size(
                Pos2::new(rect.left() + 14.0, rect.top() + 13.0),
                Vec2::splat(48.0),
            ),
            theme::corner_radius(),
            self.skin_texture.as_ref(),
        );
        // 头像右侧的信息区：小字"当前玩家"标签 + 加粗昵称（超宽截断）+ 账户类型说明。
        theme::text(
            painter,
            Pos2::new(rect.left() + 74.0, rect.top() + 16.0),
            "当前玩家",
            7.3,
            Color32::from_rgb(119, 171, 116),
        );
        theme::bold_text(
            painter,
            Pos2::new(rect.left() + 74.0, rect.top() + 27.0),
            fit_text_to_width(
                painter,
                &self.settings.player_name,
                theme::bold_font(12.3),
                rect.width() - 88.0,
            ),
            12.3,
            theme::ink_soft(),
        );
        theme::text(
            painter,
            Pos2::new(rect.left() + 74.0, rect.top() + 44.0),
            "离线账户 · 本地资料",
            7.1,
            theme::muted(),
        );
        // 信息区与操作列表之间画一条水平分隔线。
        painter.line_segment(
            [
                Pos2::new(rect.left() + 12.0, rect.top() + 72.0),
                Pos2::new(rect.right() - 12.0, rect.top() + 72.0),
            ],
            Stroke::new(1.0, theme::line()),
        );

        // 资料菜单的操作入口行：每个元素为 (图标, 主标签, 右端后缀文案)。
        // 后缀文案非空（如"暂不支持"）会在行右侧以灰色小字显示。
        let rows = [
            (NavIcon::User, "更改昵称", ""),
            (NavIcon::Sparkles, "更换皮肤", ""),
            (NavIcon::Folder, "打开游戏目录", ""),
            (NavIcon::Logout, "切换账户", "暂不支持"),
        ];
        // 逐行绘制操作入口：计算行矩形、注册点击交互、绘制悬停高亮、
        // 图标、标签文字与右侧灰色后缀文案。
        for (index, (icon, label, suffix)) in rows.into_iter().enumerate() {
            // 每行高 38px、行距 42px，从面板顶部 80px 处开始排列。
            let row = Rect::from_min_size(
                Pos2::new(rect.left() + 7.0, rect.top() + 80.0 + index as f32 * 42.0),
                Vec2::new(rect.width() - 14.0, 38.0),
            );
            let response = ui
                .interact(row, Id::unique(("profile-row", index)), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand);
            // 悬停时给整行铺一层极淡的白色高亮，提示可点击。
            if response.hovered() {
                theme::fill_round(painter, row, theme::corner_radius(), theme::white_alpha(10));
            }
            // 行左侧绘制操作图标（居中于 15×15 方块内）。
            draw_icon(
                painter,
                icon,
                Rect::from_center_size(
                    Pos2::new(row.left() + 14.0, row.center().y),
                    Vec2::splat(15.0),
                ),
                theme::muted(),
            );
            theme::text(
                painter,
                Pos2::new(row.left() + 28.0, row.top() + 10.0),
                label,
                8.8,
                theme::ink_soft(),
            );
            if !suffix.is_empty() {
                theme::text(
                    painter,
                    Pos2::new(row.right() - 43.0, row.top() + 10.0),
                    suffix,
                    7.2,
                    theme::muted(),
                );
            }
            if response.clicked() {
                // 先关闭下拉菜单，再按行号分发到对应的动作处理。
                self.menu = Menu::None;
                // 行号与上方 rows 数组顺序一致：0=昵称 1=皮肤 2=目录 3=切换账户。
                match index {
                    0 => self.open_account_name_dialog(),
                    1 => self.choose_player_skin(ctx),
                    2 => self.open_game_folder(ctx),
                    _ => self.show_toast("暂不支持切换账户"),
                }
            }
        }
    }

    /// 绘制"更改玩家昵称"模态对话框。
    ///
    /// 该对话框包含标题、说明文案、昵称输入框、错误/提示文字以及"取消/保存"按钮。
    /// 在打开（`account_name_dialog_open`）时输入框可编辑；关闭或动画进行中则只读展示。
    /// 支持回车保存、点击"保存"保存、点击"取消"关闭。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文，用于放置文本编辑控件与请求重绘。
    /// - `screen`：整个屏幕的矩形，用作半透明遮罩层与居中参考。
    /// - `time`：累积时间（秒），供玻璃质感动画使用。
    ///
    /// # 副作用
    /// 可能写入 `account_name_dialog_open`、`account_name_error`，调用 `save_account_name`，
    /// 并通过 `ctx.request_repaint` 触发刷新。
    ///
    /// # 不变量
    /// 当对话框既未打开、弹出值又趋近 0 时直接返回，不绘制任何内容。
    pub(in crate::app) fn paint_account_name_dialog(
        &mut self,
        ctx: &egui::Context,
        screen: Rect,
        time: f64,
    ) {
        if !self.account_name_dialog_open && self.account_name_dialog_value <= 0.002 {
            return;
        }
        // 弹出动画值映射到 [−0.05, 1.12]（允许轻微过冲回弹），用于底层透明度/缩放计算。
        let progress = self.account_name_dialog_value.clamp(-0.05, 1.12);
        // 由动画值与"是否打开"共同决定整张卡片的不透明度（关闭趋近 0）。
        let opacity = popup_opacity(
            self.account_name_dialog_value,
            self.account_name_dialog_open,
        );
        // eased 把 progress 夹到 [0,1]，再映射为卡片缩放：0 → 0.88（略小），1 → 1.0。
        let eased = progress.clamp(0.0, 1.0);
        let scale = 0.88 + 0.12 * eased;
        // 计算对话框在设计尺寸下的"原矩形"，再按缩放系数从中心收缩得到动画中的卡片矩形。
        let source = account_name_dialog_rect(screen);
        let card = Rect::from_center_size(
            source.center(),
            Vec2::new(source.width() * scale, source.height() * scale),
        );
        // 创建覆盖全屏的前景 Area 作为模态层：遮罩 + 居中卡片都绘制在这里。
        egui::Area::new(Id::unique("account-name-dialog-modal"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .movable(false)
            .interactable(true)
            .show(ctx, |ui| {
                // 先铺一层随动画渐变的黑色半透明遮罩（打开时最深 alpha=118），
                // 视觉上把对话框与背景内容隔离开。
                ui.set_min_size(screen.size());
                ui.painter().rect_filled(
                    screen,
                    0,
                    Color32::from_black_alpha((118.0 * eased) as u8),
                );
                ui.set_opacity(opacity);
                // 计算底部"取消/保存"两个按钮的固定位置（右下角并排）。
                let cancel = Rect::from_min_size(
                    Pos2::new(card.right() - 176.0, card.bottom() - 52.0),
                    Vec2::new(70.0, 36.0),
                );
                let save = Rect::from_min_size(
                    Pos2::new(card.right() - 98.0, card.bottom() - 52.0),
                    Vec2::new(76.0, 36.0),
                );
                // 仅当对话框处于打开态时才创建"取消/保存"按钮的可交互区域；
                // 关闭或动画中返回 None，避免动画期间误触按钮。
                let button_responses = self.account_name_dialog_open.then(|| {
                    (
                        ui.interact(cancel, Id::unique("account-name-cancel"), Sense::click())
                            .on_hover_cursor(CursorIcon::PointingHand),
                        ui.interact(save, Id::unique("account-name-save"), Sense::click())
                            .on_hover_cursor(CursorIcon::PointingHand),
                    )
                });
                // 克隆画笔绘制玻璃卡片底板，并裁剪出内容画笔。
                let painter = ui.painter().clone();
                theme::glass_panel(
                    &painter,
                    card,
                    theme::corner_radius(),
                    theme::GlassSpec::new(Color32::from_rgb(21, 34, 25), theme::GREEN, time, 29.0)
                        .backdrop_blur(18.0),
                );
                let content = card_clip(&painter, card);
                // 卡片标题与说明文案。
                theme::bold_text(
                    &content,
                    Pos2::new(card.left() + 22.0, card.top() + 18.0),
                    "更改玩家昵称",
                    18.0,
                    theme::ink(),
                );
                theme::text(
                    &content,
                    Pos2::new(card.left() + 22.0, card.top() + 47.0),
                    "昵称会用于后续启动的离线游戏，长度 3–16 个字符",
                    8.2,
                    theme::muted(),
                );
                // 昵称输入框：先画深色背景 + 细边框的圆角矩形。
                let field = Rect::from_min_size(
                    Pos2::new(card.left() + 22.0, card.top() + 78.0),
                    Vec2::new(card.width() - 44.0, 38.0),
                );
                theme::fill_round(
                    &content,
                    field,
                    theme::corner_radius(),
                    Color32::from_rgb(24, 38, 28),
                );
                theme::stroke_round(
                    &content,
                    field,
                    theme::corner_radius(),
                    1.0,
                    theme::white_alpha(30),
                );
                // 对话框打开时放置真正的单行文本编辑控件（无边框，嵌在 field 内部）；
                // 关闭/动画中只读展示当前输入内容（用纯文字代替编辑框）。
                if self.account_name_dialog_open {
                    ui.put(
                        field.shrink2(Vec2::new(11.0, 6.0)),
                        egui::TextEdit::singleline(&mut self.account_name_input)
                            .frame(egui::Frame::NONE)
                            .font(theme::font(10.0))
                            .text_color(theme::adaptive_text_color_at(
                                &content,
                                field.center(),
                                theme::ink(),
                                10.0,
                            ))
                            .hint_text("3–16 位字母、数字或下划线"),
                    );
                } else {
                    theme::text(
                        &content,
                        Pos2::new(field.left() + 11.0, field.top() + 11.0),
                        &self.account_name_input,
                        10.0,
                        theme::ink(),
                    );
                }
                // 提示/错误文字行：有错误时显示橙色错误信息（超宽截断），
                // 否则显示灰色的一般性说明（更改昵称不影响存档 UUID）。
                theme::text(
                    &content,
                    Pos2::new(card.left() + 22.0, card.top() + 126.0),
                    if let Some(error) = self.account_name_error.as_ref() {
                        fit_text_to_width(&content, error, theme::font(8.0), card.width() - 44.0)
                    } else {
                        "更改昵称不会改变本地存档的玩家 UUID".to_owned()
                    },
                    8.0,
                    if self.account_name_error.is_some() {
                        Color32::from_rgb(226, 172, 91)
                    } else {
                        theme::muted()
                    },
                );
                // 打开态：绘制可交互的"取消/保存"按钮（取消为暗色玻璃按钮，
                // 保存为带液体流动高光的绿色主按钮），并处理点击逻辑。
                if self.account_name_dialog_open {
                    if let Some((cancel_response, save_response)) = button_responses.as_ref() {
                        theme::fill_round(
                            &content,
                            cancel,
                            theme::corner_radius(),
                            if cancel_response.hovered() {
                                Color32::from_rgb(37, 53, 42)
                            } else {
                                Color32::from_rgb(28, 42, 33)
                            },
                        );
                        theme::stroke_round(
                            &content,
                            cancel,
                            theme::corner_radius(),
                            1.0,
                            theme::white_alpha(34),
                        );
                        theme::center_text(
                            &content,
                            cancel.center(),
                            "取消",
                            8.8,
                            theme::ink_soft(),
                            false,
                        );
                        theme::liquid_button(
                            &content,
                            save,
                            theme::corner_radius(),
                            2.2,
                            save_response.hovered(),
                        );
                        theme::center_text(
                            &content,
                            save.center(),
                            "保存",
                            9.0,
                            Color32::WHITE,
                            true,
                        );
                    }
                    // 点击"取消"：关闭对话框并清空错误提示。
                    if button_responses
                        .as_ref()
                        .is_some_and(|(response, _)| response.clicked())
                    {
                        self.account_name_dialog_open = false;
                        self.account_name_error = None;
                    }
                    // 点击"保存"按钮，或按下回车键，都会触发昵称保存逻辑。
                    if button_responses
                        .as_ref()
                        .is_some_and(|(_, response)| response.clicked())
                        || ui.input(|input| input.key_pressed(egui::Key::Enter))
                    {
                        self.save_account_name(ctx);
                    }
                } else {
                    // 关闭/动画态：按钮不可交互，仅以静态形式绘制（保持视觉完整）。
                    for (button, label) in [(cancel, "取消"), (save, "保存")] {
                        theme::fill_round(
                            &content,
                            button,
                            theme::corner_radius(),
                            Color32::from_rgb(30, 43, 34),
                        );
                        theme::center_text(
                            &content,
                            button.center(),
                            label,
                            8.8,
                            theme::ink_soft(),
                            false,
                        );
                    }
                }
            });
    }

    /// 打开游戏目录所在的文件管理器窗口。
    ///
    /// 优先使用 `settings.game_directory`；若为空则回退到当前工作目录
    /// （再失败则回退到 `"."`）。在 Windows 上调用 `explorer`，在其余平台调用
    /// `xdg-open`，均为异步派生进程（spawn，不等待其结束）。
    ///
    /// # 参数
    /// - `_ctx`：egui 上下文（当前未使用，保留以统一接口签名）。
    ///
    /// # 副作用
    /// 派生一个外部进程打开目录，并弹出 Toast "已打开启动器所在目录"。
    /// 进程派生失败仅以 `let _ = ...` 静默忽略，不影响后续逻辑。
    pub(in crate::app) fn open_game_folder(&mut self, _ctx: &egui::Context) {
        // 优先用设置中的游戏目录；若为空则回退到当前工作目录，再失败则用 "." 兜底。
        let folder = self
            .settings
            .game_directory
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        #[cfg(target_os = "windows")]
        {
            let _ = std::process::Command::new("explorer").arg(folder).spawn();
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = std::process::Command::new("xdg-open").arg(folder).spawn();
        }
        self.show_toast("已打开启动器所在目录");
    }

    /// 打开"更改玩家昵称"对话框并初始化其状态。
    ///
    /// 关闭顶部菜单、将对话框标记为打开、重置弹出动画值（值归 0、速度归 0），
    /// 并把输入框预填为当前玩家昵称，同时清空历史错误信息。
    ///
    /// # 副作用
    /// 写入 `menu`、`account_name_dialog_open`、`account_name_dialog_value`、
    /// `account_name_dialog_velocity`、`account_name_input`、`account_name_error`。
    pub(in crate::app) fn open_account_name_dialog(&mut self) {
        self.menu = Menu::None;
        self.account_name_dialog_open = true;
        self.account_name_dialog_value = 0.0;
        self.account_name_dialog_velocity = 0.0;
        self.account_name_input = self.settings.player_name.clone();
        self.account_name_error = None;
    }

    /// 校验并保存玩家昵称。
    ///
    /// 先用 `normalize_player_name` 规整输入；规整失败则把错误信息写入
    /// `account_name_error` 并显示在对话框内。规整成功则更新 `settings.player_name`
    /// 并持久化设置；若保存失败则回滚昵称并展示错误。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文，保存成功后调用 `request_repaint` 刷新界面。
    ///
    /// # 副作用
    /// 成功时更新 `settings` 与 `saved_settings`，关闭对话框并弹出成功 Toast；
    /// 失败时回滚 `settings.player_name` 并设置 `account_name_error` 以在界面提示。
    pub(in crate::app) fn save_account_name(&mut self, ctx: &egui::Context) {
        match normalize_player_name(&self.account_name_input) {
            Ok(name) => {
                // 先备份旧昵称，便于持久化失败时回滚，避免丢失原设置。
                let previous = self.settings.player_name.clone();
                self.settings.player_name = name.clone();
                match self.settings.save() {
                    Ok(()) => {
                        self.saved_settings = self.settings.clone();
                        self.account_name_dialog_open = false;
                        self.account_name_error = None;
                        self.show_toast(format!("玩家昵称已更改为 {name}"));
                        ctx.request_repaint();
                    }
                    Err(error) => {
                        self.settings.player_name = previous;
                        self.account_name_error = Some(format!("无法保存昵称：{error}"));
                    }
                }
            }
            Err(error) => self.account_name_error = Some(error),
        }
    }

    /// 推进"更改昵称"对话框的开合弹簧动画。
    ///
    /// 根据 `account_name_dialog_open` 设定目标值（打开为 `1.0`，关闭为 `0.0`），
    /// 使用弹簧模型 `spring_popup` 更新 `account_name_dialog_value` 与速度。
    /// 若用户关闭了动画（`animations_enabled()` 为假），则直接跳到目标值而不做插值。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文，用于读取帧间隔 `stable_dt` 与在动画中请求持续重绘。
    ///
    /// # 副作用
    /// 更新 `account_name_dialog_value` 与 `account_name_dialog_velocity`；
    /// 当关闭且动画归零时清空 `account_name_error`；动画进行中通过
    /// `request_repaint_after` 请求下一帧重绘。
    pub(in crate::app) fn update_account_name_dialog_animation(&mut self, ctx: &egui::Context) {
        let target = if self.account_name_dialog_open {
            1.0
        } else {
            0.0
        };
        // 用户关闭了动画设置：直接把动画值瞬移到目标，无插值过程；
        // 关闭状态下顺带清空遗留的错误提示。
        if !self.settings.animations_enabled() {
            self.account_name_dialog_value = target;
            self.account_name_dialog_velocity = 0.0;
            if !self.account_name_dialog_open {
                self.account_name_error = None;
            }
            return;
        }
        // 对话框已关闭且动画值衰减到阈值以下：视为动画彻底结束，
        // 归零所有动画状态并清空错误提示，不再继续模拟。
        if !self.account_name_dialog_open && self.account_name_dialog_value <= 0.002 {
            self.account_name_dialog_value = 0.0;
            self.account_name_dialog_velocity = 0.0;
            self.account_name_error = None;
            return;
        }
        // 取稳定的帧间隔作为弹簧模拟的时间步长，夹在 [1ms, 50ms] 之间，
        // 防止掉帧时步长过大导致弹簧数值发散。
        let delta = ctx.input(|input| input.stable_dt).clamp(0.001, 0.05);
        // 推进一步弹簧模拟；返回 true 表示动画仍在进行，需请求下一帧重绘；
        // 返回 false 且处于关闭态时，动画已收敛，可安全清空错误提示。
        if spring_popup(
            &mut self.account_name_dialog_value,
            &mut self.account_name_dialog_velocity,
            target,
            delta,
        ) {
            ctx.request_repaint_after(Duration::from_millis(16));
        } else if !self.account_name_dialog_open {
            self.account_name_error = None;
        }
    }

    /// 通过文件选择器更换本地玩家皮肤。
    ///
    /// 先关闭顶部菜单，弹出图片选择对话框；读取并解析所选皮肤的像素，
    /// 自动检测是否为细臂（Alex）模型，最后应用皮肤并弹出提示。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文，供 `apply_player_skin_pixels` 落地皮肤与重绘使用。
    ///
    /// # 副作用
    /// 设置 `skin_model_alex`（是否细臂模型），并调用 `apply_player_skin_pixels`
    /// 落地皮肤；若用户取消选择或读取/解析失败，则弹出错误 Toast 并提前返回。
    pub(in crate::app) fn choose_player_skin(&mut self, ctx: &egui::Context) {
        // 先收起顶部下拉菜单，避免遮挡文件选择对话框。
        self.menu = Menu::None;
        // 弹出图片文件选择框；用户取消选择（返回 None）时静默结束。
        let Some(path) = pick_image_file(false) else {
            return;
        };
        // 读取并解码皮肤图片为 RGBA 像素；失败时以 Toast 展示错误信息。
        let pixels = match load_player_skin(&path) {
            Ok(pixels) => pixels,
            Err(error) => {
                self.show_toast(error);
                return;
            }
        };
        // 根据像素自动检测该皮肤是否为细臂（Alex）模型，影响后续渲染与提示文案。
        self.skin_model_alex = detect_slim_skin(&pixels);
        // 根据模型类型拼装提示文案：Alex 为细臂模型，Steve 为经典宽臂模型。
        let model = if self.skin_model_alex {
            "Alex 细臂"
        } else {
            "Steve 宽臂"
        };
        // 应用皮肤像素并弹出"已更换本地玩家皮肤 · <模型>"提示。
        self.apply_player_skin_pixels(pixels, ctx, &format!("已更换本地玩家皮肤 · {model}"));
    }

    /// 将皮肤编辑器中的像素直接应用到本地玩家账户。
    ///
    /// 复用 `skin_pixels` 中已编辑好的像素，调用 `apply_player_skin_pixels`
    /// 完成落地、资源包安装、状态提示与持久化。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文，供应用皮肤时使用。
    ///
    /// # 副作用
    /// 经由 `apply_player_skin_pixels` 写入皮肤文件、设置状态字段并请求重绘。
    pub(in crate::app) fn apply_editor_skin_to_account(&mut self, ctx: &egui::Context) {
        self.apply_player_skin_pixels(self.skin_pixels.clone(), ctx, "当前皮肤已应用到本地玩家");
    }

    /// 把给定皮肤像素落地为本地玩家皮肤，并安装皮肤资源包、持久化设置。
    ///
    /// 保存像素到皮肤文件，更新 `settings` 中的皮肤路径与模型类型，重置皮肤编辑历史
    /// 与相关缓存，并尝试把皮肤资源包安装到版本数据目录。最终设置 `skin_status`、
    /// 弹出 Toast、持久化设置并请求重绘。
    ///
    /// # 参数
    /// - `pixels`：RGBA 皮肤像素数据（长度应为宽×高×4）。
    /// - `ctx`：egui 上下文，用于请求重绘。
    /// - `message`：成功时使用的 Toast / 状态基础文案（资源包安装结果会追加其后）。
    ///
    /// # 副作用
    /// 写入皮肤文件、更新 `skin_pixels` / `settings` / `skin_status` 等多个字段，
    /// 重置皮肤编辑状态（`skin_history`、`skin_last_pixel`、`skin_preview_last_face`、
    /// `skin_stroke_before`、`skin_dirty`），调用 `install_local_skin_resource_pack`
    /// 安装资源包，并执行 `persist_settings`。
    ///
    /// # 错误处理
    /// 保存文件失败则弹出错误 Toast 并直接返回；资源包安装失败仅体现在 `skin_status`
    /// 文案中，不阻断皮肤本身的应用与保存。
    pub(in crate::app) fn apply_player_skin_pixels(
        &mut self,
        pixels: Vec<u8>,
        ctx: &egui::Context,
        message: &str,
    ) {
        // 保存皮肤像素到本地文件；失败时弹出错误 Toast 并放弃应用。
        let path = match save_player_skin(&pixels) {
            Ok(path) => path,
            Err(error) => {
                self.show_toast(error);
                return;
            }
        };
        // 同步内存中的皮肤状态：更新像素缓存、设置中的皮肤路径与模型类型。
        self.skin_pixels = pixels;
        self.settings.player_skin = Some(path);
        self.settings.player_skin_model_alex = self.skin_model_alex;
        // 重置皮肤编辑器的撤销历史与画笔/预览缓存，使编辑器以新皮肤为基准。
        self.skin_history = SkinEditHistory::default();
        self.skin_last_pixel = None;
        self.skin_preview_last_face = None;
        self.skin_stroke_before = None;
        // 标记皮肤预览为"脏"，下一帧重新生成预览贴图。
        self.skin_dirty = true;
        // 若版本数据目录存在，则把皮肤打包为资源包并安装进去（这样游戏内立即生效）；
        // 否则（目录不可用）返回 None，由下方 None 分支提示"将在启动游戏时应用"。
        let pack_result = self
            .version_data_directory()
            .map(|directory| install_local_skin_resource_pack(&directory, &self.skin_pixels));
        match pack_result {
            Some(Ok(_)) => self.skin_status = Some(message.to_owned()),
            Some(Err(error)) => {
                self.skin_status = Some(format!("{message}，但资源包安装失败：{error}"));
            }
            None => {
                self.skin_status = Some(format!("{message}，将在启动游戏时应用"));
            }
        }
        // 以皮肤状态文案展示最终结果（成功 Toast 也使用同一文案）。
        self.show_toast(
            self.skin_status
                .clone()
                .unwrap_or_else(|| message.to_owned()),
        );
        // 持久化设置并请求立即重绘，使新头像立刻显示在界面上。
        self.persist_settings();
        ctx.request_repaint();
    }

    /// 绘制官方资讯（新闻）详情对话框。
    ///
    /// 展示某条资讯的配图（若无贴图则用程序绘制的占位插画）、分类/日期、标题与正文，
    /// 并提供关闭按钮（含 Esc）与"前往查看"按钮（打开外部链接）。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文，用于确保资讯贴图就绪（`ensure_news_textures`）与捕获按键。
    /// - `ui`：egui UI 句柄，用于关闭/前往按钮的可交互区域。
    /// - `painter`：绘制画笔。
    /// - `screen`：屏幕矩形，用于居中对话框与遮罩。
    /// - `time`：累积时间（秒），供插画与玻璃质感动画使用。
    ///
    /// # 副作用
    /// 点击关闭或按 Esc 会清空 `news`（置 `None`）；点击"前往查看"会尝试用
    /// `open_external_url` 打开链接并清空 `news`，打开失败则弹出 Toast。
    pub(in crate::app) fn paint_news_dialog(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        screen: Rect,
        time: f64,
    ) {
        // 没有正在查看的资讯（news 为 None）时无需绘制。
        let Some(news_index) = self.news else {
            return;
        };
        // 按索引取资讯内容；若索引越界（如列表已刷新）则清空状态并返回。
        let Some(story) = self.news_items.get(news_index).cloned() else {
            self.news = None;
            return;
        };
        // 确保资讯列表的配图贴图已加载就绪。
        self.ensure_news_textures(ctx);
        // 取该条资讯对应的配图贴图：news_textures 元素为 Option<Texture>（可能尚未加载），
        // 经 cloned().flatten() 折叠为 Option<Texture>，无图时为 None。
        let image = self.news_textures.get(news_index).cloned().flatten();
        // 对话框尺寸：设计尺寸 540×390，但不超过屏幕（各留 28px 边距），居中放置。
        let dialog_size = Vec2::new(
            540.0_f32.min(screen.width() - 28.0),
            390.0_f32.min(screen.height() - 28.0),
        );
        // 绘制玻璃质感对话框底板并裁剪内容画笔。
        let dialog = Rect::from_center_size(screen.center(), dialog_size);
        theme::glass_panel(
            painter,
            dialog,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(22, 32, 25), theme::GREEN, time, 8.6)
                .backdrop_blur(18.0),
        );
        let content = card_clip(painter, dialog);
        let painter = &content;
        // 顶部配图区域：宽度撑满对话框（左右各留 7px），高 116px。
        let art = Rect::from_min_size(
            dialog.left_top() + Vec2::new(7.0, 7.0),
            Vec2::new(dialog.width() - 14.0, 116.0),
        );
        // 优先使用资讯配图贴图；若缺失则按 news_index 用程序绘制的占位插画
        // （0=版本 1=模组 其余=社区）作为封面。
        if let Some(image) = image {
            paint_rounded_image(painter, art, 12.0, &image, image.size());
        } else {
            scene::paint_news_art(
                painter,
                art,
                match news_index {
                    0 => scene::NewsArt::Version,
                    1 => scene::NewsArt::Mod,
                    _ => scene::NewsArt::Community,
                },
                time,
            );
        }
        // 配图外描一圈细边框，与面板圆角保持一致。
        theme::stroke_round(painter, art, 12, 1.0, theme::white_alpha(30));

        // 右上角关闭按钮：交互区域 + 悬停变色底 + 关闭图标。
        let close = Rect::from_min_size(
            Pos2::new(dialog.right() - 44.0, dialog.top() + 14.0),
            Vec2::splat(30.0),
        );
        let close_response = ui
            .interact(close, Id::unique("dialog-close"), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        theme::fill_round(
            painter,
            close,
            theme::corner_radius(),
            if close_response.hovered() {
                theme::white_alpha(24)
            } else {
                theme::black_alpha(76)
            },
        );
        draw_icon(
            painter,
            NavIcon::Close,
            Rect::from_center_size(close.center(), Vec2::splat(14.0)),
            theme::ink_soft(),
        );
        // 点击关闭按钮：清空当前查看的资讯（下帧不再绘制对话框）。
        if close_response.clicked() {
            self.news = None;
        }

        // 正文区域起始 y 坐标（配图底部再留 22px 间距）。
        let body_y = art.bottom() + 22.0;
        // 分类/日期小字：日期为空时只显示分类，否则"分类 · 日期"。
        theme::text(
            painter,
            Pos2::new(dialog.left() + 25.0, body_y),
            ellipsize_end(
                &if story.date.is_empty() {
                    story.category.clone()
                } else {
                    format!("{} · {}", story.category, story.date)
                },
                46,
            ),
            7.8,
            Color32::from_rgb(126, 176, 121),
        );
        // 资讯标题：加粗大号文字，超长按 34 字符截断。
        theme::bold_text(
            painter,
            Pos2::new(dialog.left() + 25.0, body_y + 18.0),
            ellipsize_end(&story.title, 34),
            21.0,
            theme::ink(),
        );
        // 资讯正文：优先用完整正文 body，为空时回退到摘要 summary；
        // 按 320 字符截断并以对话框宽度换行。
        theme::wrapped_text(
            painter,
            Pos2::new(dialog.left() + 25.0, body_y + 54.0),
            ellipsize_end(
                if story.body.is_empty() {
                    &story.summary
                } else {
                    &story.body
                },
                320,
            ),
            9.4,
            theme::ink_soft(),
            dialog.width() - 50.0,
        );

        // 左下角"前往查看"按钮：绿色主按钮，悬停时切换为更亮的绿色。
        let action = Rect::from_min_size(
            Pos2::new(dialog.left() + 25.0, dialog.bottom() - 57.0),
            Vec2::new(112.0, 36.0),
        );
        let action_response = ui
            .interact(action, Id::unique("dialog-action"), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        theme::fill_round(
            painter,
            action,
            theme::corner_radius(),
            if action_response.hovered() {
                theme::GREEN_BRIGHT
            } else {
                theme::GREEN
            },
        );
        theme::center_text(
            painter,
            action.center(),
            "前往查看  →",
            8.8,
            Color32::from_rgb(15, 48, 19),
            true,
        );
        if action_response.clicked() {
            // 取资讯的文章链接；若为空则回退到 Minecraft 官网文章页。
            let url = story
                .article_url
                .as_deref()
                .unwrap_or("https://www.minecraft.net/article");
            match open_external_url(url) {
                Ok(()) => self.news = None,
                Err(error) => self.show_toast(format!("无法打开官方详情：{error}")),
            }
        }

        // Esc 键与关闭按钮等效：按下后关闭资讯详情对话框。
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.news = None;
        }
    }

    /// 绘制"强制更新"对话框。
    ///
    /// 当检测到启动器新版本且必须更新时显示：展示当前/目标版本号、下载进度条与状态，
    /// 失败时提供"重试"按钮。该更新在检测后通常不可跳过（会下载、替换并重启启动器）。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文，用于（重试时）启动下载任务与请求重绘。
    /// - `ui`：egui UI 句柄，用于重试按钮的交互区域。
    /// - `screen`：屏幕矩形，用于居中卡片与半透明遮罩。
    /// - `time`：累积时间（秒），供玻璃质感动画使用。
    ///
    /// # 副作用
    /// 失败时点击"重试"会重启下载任务（`update::start_update_download`）或重新触发
    /// 更新检查（`start_launcher_update_check`）；成功下载阶段展示
    /// "下载完成，正在关闭并重新启动…"状态。
    pub(in crate::app) fn paint_forced_update(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        screen: Rect,
        time: f64,
    ) {
        // 没有待强更的发布信息时不绘制（该对话框只在强制更新流程中显示）。
        let Some(release) = self.forced_update_release.as_ref() else {
            return;
        };
        // 全屏近黑色遮罩：阻断用户对背景内容的注意力与交互。
        let painter = ui.painter().clone();
        painter.rect_filled(screen, 0, Color32::from_black_alpha(150));
        // 更新卡片：设计宽 520px（不超过屏幕减 40px），高 286px，居中放置。
        let card = Rect::from_center_size(
            screen.center(),
            Vec2::new(520.0_f32.min(screen.width() - 40.0), 286.0),
        );
        theme::glass_panel(
            &painter,
            card,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(20, 32, 24), theme::GREEN, time, 21.0)
                .backdrop_blur(24.0),
        );
        let content = card_clip(&painter, card);
        // 卡片左上角装饰光球 + 闪光图标，构成更新提示的视觉主体。
        theme::soft_orb(
            &content,
            Pos2::new(card.left() + 72.0, card.top() + 62.0),
            72.0,
            theme::GREEN,
            18,
        );
        let icon = Rect::from_center_size(
            Pos2::new(card.left() + 72.0, card.top() + 62.0),
            Vec2::splat(52.0),
        );
        draw_icon(&content, NavIcon::Sparkles, icon, theme::GREEN_BRIGHT);
        // 标题与版本信息：当前版本取自编译期环境变量 CARGO_PKG_VERSION，
        // 目标版本来自更新检查获取到的发布信息。
        theme::bold_text(
            &content,
            Pos2::new(card.left() + 126.0, card.top() + 30.0),
            "Genesis Launcher 强制更新",
            19.0,
            theme::ink(),
        );
        theme::text(
            &content,
            Pos2::new(card.left() + 127.0, card.top() + 63.0),
            format!(
                "当前 v{}  →  最新 v{}",
                env!("CARGO_PKG_VERSION"),
                release.version
            ),
            9.2,
            theme::muted(),
        );
        theme::text(
            &content,
            Pos2::new(card.left() + 127.0, card.top() + 84.0),
            "检测到新版本后将自动下载、替换并重新启动，无法跳过",
            8.3,
            theme::ink_soft(),
        );
        // 进度条：先画轨道，下载进度 > 0 时按比例填充绿色进度。
        let track = Rect::from_min_size(
            Pos2::new(card.left() + 30.0, card.top() + 139.0),
            Vec2::new(card.width() - 60.0, 11.0),
        );
        theme::fill_round(
            &content,
            track,
            theme::corner_radius(),
            theme::white_alpha(18),
        );
        if self.forced_update_progress > 0.0 {
            // 进度填充矩形：宽度 = 轨道宽 × 进度比例（夹到 [0,1] 防止越界）。
            theme::fill_round(
                &content,
                Rect::from_min_size(
                    track.left_top(),
                    Vec2::new(
                        track.width() * self.forced_update_progress.clamp(0.0, 1.0),
                        track.height(),
                    ),
                ),
                theme::corner_radius(),
                theme::GREEN_BRIGHT,
            );
        }
        // 状态行文案优先级：正在安装（下载完成）> 出错 > 正在下载（带百分比）。
        let percent = (self.forced_update_progress * 100.0).round() as u32;
        let status = if self.forced_update_installing {
            "下载完成，正在关闭并重新启动…".to_owned()
        } else if let Some(error) = self.forced_update_error.as_ref() {
            format!("更新失败：{error}")
        } else {
            format!("正在下载更新 · {percent}%")
        };
        theme::text(
            &content,
            Pos2::new(card.left() + 30.0, card.top() + 161.0),
            fit_text_to_width(&content, &status, theme::font(8.5), card.width() - 60.0),
            8.5,
            if self.forced_update_error.is_some() {
                Color32::from_rgb(221, 170, 91)
            } else {
                theme::GREEN_BRIGHT
            },
        );
        // 出错且未在安装时：显示"重试"按钮；否则显示等待重启的说明文字。
        if self.forced_update_error.is_some() && !self.forced_update_installing {
            let retry = Rect::from_center_size(
                Pos2::new(card.center().x, card.bottom() - 52.0),
                Vec2::new(150.0, 38.0),
            );
            // 点击"重试强制更新"：若仍持有发布信息与下载资产，则重置进度并重新启动下载任务；
            // 否则（信息缺失）回退到重新发起更新检查流程。
            if settings_button(
                ui,
                &content,
                retry,
                "forced-update-retry",
                "重试强制更新",
                true,
            ) {
                if let Some(release) = self.forced_update_release.as_ref()
                    && let Some(asset) = release.asset.clone()
                {
                    self.forced_update_error = None;
                    self.forced_update_progress = 0.0;
                    self.forced_update_receiver = Some(update::start_update_download(asset, ctx));
                    ctx.request_repaint();
                } else {
                    self.forced_update_release = None;
                    self.start_launcher_update_check(ctx, true);
                }
            }
        } else {
            theme::center_text(
                &content,
                Pos2::new(card.center().x, card.bottom() - 43.0),
                "更新完成后将自动重新打开启动器",
                8.1,
                theme::muted(),
                false,
            );
        }
    }

    /// 绘制右下角的轻量提示（Toast）气泡。
    ///
    /// 若 `toast` 为 `None` 则直接返回。气泡宽度根据消息长度自适应
    /// （220–390px，且不超过屏幕宽度），包含绿色对勾图标与消息文字。
    ///
    /// # 参数
    /// - `painter`：绘制画笔。
    /// - `screen`：屏幕矩形，用于定位（右下角）与宽度上限计算。
    ///
    /// # 不变量
    /// 纯读取 `self.toast`，无任何状态写入，可安全在每帧调用。
    pub(in crate::app) fn paint_toast(&self, painter: &Painter, screen: Rect) {
        let Some(toast) = &self.toast else {
            return;
        };
        // 气泡宽度按消息字符数估算（每字符约 8.1px + 两侧图标/内边距 70px），
        // 夹在 [220, 390] 之间且不超过屏幕宽度减 24px。
        let width = (toast.message.chars().count() as f32 * 8.1 + 70.0)
            .clamp(220.0, 390.0)
            .min(screen.width() - 24.0);
        // 气泡固定在屏幕右下角（右/下各留 24px/70px 边距），高 47px。
        let rect = Rect::from_min_size(
            Pos2::new(screen.right() - width - 24.0, screen.bottom() - 70.0),
            Vec2::new(width, 47.0),
        );
        theme::glass_panel(
            painter,
            rect,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(27, 39, 30), theme::GREEN, 0.0, 9.3),
        );
        let content = card_clip(painter, rect);
        let painter = &content;
        // 左侧绿色圆形底 + 对勾图标，构成"成功"语义的视觉标记。
        painter.circle_filled(
            Pos2::new(rect.left() + 23.0, rect.center().y),
            13.0,
            theme::alpha(theme::GREEN, 34),
        );
        theme::center_text(
            painter,
            Pos2::new(rect.left() + 23.0, rect.center().y),
            "✓",
            10.5,
            theme::GREEN,
            true,
        );
        theme::text(
            painter,
            Pos2::new(rect.left() + 44.0, rect.top() + 14.0),
            &toast.message,
            8.5,
            theme::ink_soft(),
        );
    }

    /// 绘制"官方版本浏览器"对话框。
    ///
    /// 连接到 Mojang Piston Meta（`official_*` 字段），列出可下载的官方版本，
    /// 支持"全部 / 正式版 / 快照"筛选、滚轮滚动列表、选中高亮与"下载并安装"按钮。
    /// 点击安装会打开加载器（loader）前置选择对话框。
    ///
    /// # 参数
    /// - `ui`：egui UI 句柄，用于筛选按钮、列表行与安装按钮的交互区域。
    /// - `painter`：绘制画笔。
    /// - `screen`：屏幕矩形，用于居中对话框与宽度上限。
    /// - `time`：累积时间（秒），供玻璃质感动画使用。
    ///
    /// # 副作用
    /// 切换筛选会重置 `official_scroll`；选中行更新 `official_selected`；
    /// 点击关闭会设置 `official_browser_open = false`；点击安装会关闭浏览器并
    /// 调用 `open_preinstall_loader_dialog` 进入下一步。
    pub(in crate::app) fn paint_official_browser(
        &mut self,
        ui: &mut egui::Ui,
        painter: &Painter,
        screen: Rect,
        time: f64,
    ) {
        // 对话框尺寸：设计尺寸 650×600，不超过屏幕（各留 30px/36px 边距），居中放置。
        let dialog_size = Vec2::new(
            650.0_f32.min(screen.width() - 30.0),
            600.0_f32.min(screen.height() - 36.0),
        );
        // 绘制玻璃质感底板并裁剪内容画笔。
        let dialog = Rect::from_center_size(screen.center(), dialog_size);
        theme::glass_panel(
            painter,
            dialog,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(22, 33, 26), theme::GREEN, time, 20.0)
                .backdrop_blur(18.0),
        );
        let content = card_clip(painter, dialog);
        let painter = &content;
        theme::bold_text(
            painter,
            Pos2::new(dialog.left() + 24.0, dialog.top() + 20.0),
            "官方版本",
            21.0,
            theme::ink(),
        );
        // 副标题三态：出错时显示错误信息；清单未加载完显示连接提示；
        // 正常时显示可下载版本总数。
        theme::text(
            painter,
            Pos2::new(dialog.left() + 24.0, dialog.top() + 48.0),
            if let Some(error) = &self.official_error {
                error.clone()
            } else if self.official_manifest.is_some() {
                "正在连接 Mojang Piston Meta…".to_owned()
            } else {
                format!(
                    "Piston Meta · {} 个可下载版本",
                    self.official_versions.len()
                )
            },
            8.4,
            theme::muted(),
        );
        // 右上角关闭按钮：交互区域 + 悬停变色底 + 关闭图标。
        let close = Rect::from_min_size(
            Pos2::new(dialog.right() - 46.0, dialog.top() + 16.0),
            Vec2::splat(30.0),
        );
        let close_response = ui
            .interact(close, Id::unique("official-close"), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        theme::fill_round(
            painter,
            close,
            theme::corner_radius(),
            if close_response.hovered() {
                theme::white_alpha(22)
            } else {
                theme::black_alpha(72)
            },
        );
        draw_icon(
            painter,
            NavIcon::Close,
            Rect::from_center_size(close.center(), Vec2::splat(14.0)),
            theme::ink_soft(),
        );
        // 点击关闭：关闭浏览器对话框并立即返回（跳过本帧后续绘制）。
        if close_response.clicked() {
            self.official_browser_open = false;
            return;
        }

        // 三个筛选标签：全部 / 正式版 / 快照。逐个绘制按钮并处理点击切换。
        let filters = [
            (VersionFilter::All, "全部"),
            (VersionFilter::Release, "正式版"),
            (VersionFilter::Snapshot, "快照"),
        ];
        let mut filter_x = dialog.left() + 24.0;
        // 遍历筛选按钮：x 坐标逐个右移排列（每个按钮宽 66px、间距 7px）。
        for (index, (filter, label)) in filters.into_iter().enumerate() {
            let button = Rect::from_min_size(
                Pos2::new(filter_x, dialog.top() + 76.0),
                Vec2::new(66.0, 27.0),
            );
            let response = ui
                .interact(
                    button,
                    Id::unique(("official-filter", index)),
                    Sense::click(),
                )
                .on_hover_cursor(CursorIcon::PointingHand);
            // 选中项用绿色淡底 + 亮色文字；悬停/普通态用不同深浅的白色底。
            let selected = self.official_filter == filter;
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
            theme::center_text(
                painter,
                button.center(),
                label,
                8.4,
                if selected {
                    theme::ink()
                } else {
                    theme::muted()
                },
                selected,
            );
            // 点击筛选按钮：切换筛选条件并重置列表滚动位置到顶部。
            if response.clicked() {
                self.official_filter = filter;
                self.official_scroll = 0.0;
            }
            // 更新下一个按钮的起始 x 坐标。
            filter_x = button.right() + 7.0;
        }

        // 按当前筛选条件（全部/正式版/快照）过滤官方版本，保留其在原列表中的全局索引，
        // 便于点击时与 self.official_selected、self.versions 对照。
        let filtered: Vec<(usize, crate::download::OfficialVersion)> = self
            .official_versions
            .iter()
            .enumerate()
            .filter(|(_, version)| match self.official_filter {
                VersionFilter::All => true,
                VersionFilter::Release => version.kind == "release",
                VersionFilter::Snapshot => version.kind == "snapshot",
            })
            .map(|(index, version)| (index, version.clone()))
            .collect();
        // 若当前选中项在当前筛选结果中已不存在（被筛掉），则回退选中筛选后的第一项。
        if !filtered.is_empty()
            && !filtered
                .iter()
                .any(|(index, _)| *index == self.official_selected)
        {
            self.official_selected = filtered[0].0;
        }
        // 列表区域上下边界与滚动参数：行高 49px，
        // 内容高度超出可视高度时允许滚动。
        let list_top = dialog.top() + 116.0;
        let list_bottom = dialog.bottom() - 70.0;
        let list_height = (list_bottom - list_top).max(1.0);
        let row_height = 49.0;
        let content_height = filtered.len() as f32 * row_height;
        let max_scroll = (content_height - list_height).max(0.0);
        // 指针悬停在对话框内且滚轮滚动时，按 1.05 倍系数更新滚动偏移。
        if ui.ctx().input(|input| {
            input
                .pointer
                .hover_pos()
                .is_some_and(|pointer| dialog.contains(pointer))
                && input.smooth_scroll_delta.y.abs() > 0.0
        }) {
            self.official_scroll -= ui.ctx().input(|input| input.smooth_scroll_delta.y) * 1.05;
        }
        // 将滚动偏移限制在 [0, max_scroll]，避免滚出内容范围。
        self.official_scroll = self.official_scroll.clamp(0.0, max_scroll);
        let list_painter = painter.clone();
        // 遍历筛选后的版本行：visible_index 为可见序号（用于布局），
        // global_index 为原始全局索引（用于选中状态与点击回写）。
        for (visible_index, (global_index, version)) in filtered.iter().enumerate() {
            let y = list_top + visible_index as f32 * row_height - self.official_scroll;
            // 视口裁剪：整行在可视区之外时跳过绘制，提升长列表性能。
            if y + row_height < list_top || y > list_bottom {
                continue;
            }
            let row = Rect::from_min_size(
                Pos2::new(dialog.left() + 22.0, y),
                Vec2::new(dialog.width() - 44.0, 43.0),
            );
            let response = ui
                .interact(
                    row,
                    Id::unique(("official-version", global_index)),
                    Sense::click(),
                )
                .on_hover_cursor(CursorIcon::PointingHand);
            // 行卡片底板：选中/悬停时用更亮的绿色底；GlassSpec 相位参数取
            // row.top() * 0.01，使各行动画随位置错开。
            let selected = self.official_selected == *global_index;
            let card = row.shrink2(Vec2::new(0.0, 1.5));
            theme::glass_panel(
                &list_painter,
                card,
                theme::corner_radius(),
                theme::GlassSpec::new(
                    if selected || response.hovered() {
                        Color32::from_rgb(29, 48, 33)
                    } else {
                        Color32::from_rgb(21, 34, 25)
                    },
                    theme::GREEN,
                    time,
                    row.top() * 0.01,
                )
                .hovered(response.hovered()),
            );
            if selected {
                theme::stroke_round(
                    &list_painter,
                    card,
                    theme::corner_radius(),
                    1.0,
                    theme::alpha(theme::GREEN, 72),
                );
            }
            // 查询该官方版本是否已在本地安装（按版本号匹配），决定右侧状态文案：
            // 已安装且可启动 / 已存在但需补全 / 尚未安装。
            let installed = self
                .versions
                .iter()
                .find(|installed| installed.version == version.id);
            let status = match installed {
                Some(installed) if installed.launchable => "已安装",
                Some(_) => "需补全",
                None => "可安装",
            };
            theme::bold_text(
                &list_painter,
                Pos2::new(row.left() + 14.0, row.top() + 7.0),
                &version.id,
                10.4,
                theme::ink_soft(),
            );
            // 版本类型中文标签：Piston Meta 的 kind 字段直接映射，未知类型原样显示。
            theme::text(
                &list_painter,
                Pos2::new(row.left() + 14.0, row.top() + 25.0),
                match version.kind.as_str() {
                    "release" => "正式版",
                    "snapshot" => "快照",
                    "old_beta" => "旧测试版",
                    "old_alpha" => "旧 Alpha",
                    _ => &version.kind,
                },
                7.7,
                theme::muted(),
            );
            theme::text(
                &list_painter,
                Pos2::new(row.right() - 67.0, row.top() + 14.0),
                status,
                7.8,
                if status == "已安装" {
                    theme::GREEN_BRIGHT
                } else {
                    Color32::from_rgb(201, 171, 105)
                },
            );
            // 点击版本行：更新当前选中的官方版本（全局索引）。
            if response.clicked() {
                self.official_selected = *global_index;
            }
        }
        // 筛选结果为空时的占位提示：区分"加载中 / 出错 / 该分类为空"三种情况。
        if filtered.is_empty() {
            theme::center_text(
                &list_painter,
                Pos2::new(dialog.center().x, list_top + 56.0),
                if self.official_manifest.is_some() {
                    "正在获取官方版本…"
                } else if let Some(error) = &self.official_error {
                    error
                } else {
                    "当前分类没有官方版本"
                },
                9.0,
                theme::muted(),
                false,
            );
        }
        // 内容可滚动时绘制右侧滚动条（轨道 + 按滚动进度定位的滑块）。
        if max_scroll > 0.0 {
            let track = Rect::from_min_size(
                Pos2::new(dialog.right() - 11.0, list_top + 3.0),
                Vec2::new(3.0, list_height - 6.0),
            );
            theme::fill_round(
                painter,
                track,
                theme::corner_radius(),
                theme::white_alpha(16),
            );
            // 滑块高度按"可视区占内容比例"的平方估算，最小 32px。
            let thumb_height = (list_height * list_height / content_height).max(32.0);
            let progress = self.official_scroll / max_scroll;
            let thumb = Rect::from_min_size(
                Pos2::new(
                    track.left(),
                    track.top() + (track.height() - thumb_height) * progress,
                ),
                Vec2::new(track.width(), thumb_height),
            );
            theme::fill_round(
                painter,
                thumb,
                theme::corner_radius(),
                theme::alpha(theme::GREEN, 105),
            );
        }

        let selected_version = self.official_versions.get(self.official_selected).cloned();
        theme::text(
            painter,
            Pos2::new(dialog.left() + 24.0, dialog.bottom() - 48.0),
            selected_version
                .as_ref()
                .map(|version| format!("将安装 Minecraft {}", version.id))
                .unwrap_or_else(|| "请选择要安装的版本".to_owned()),
            8.5,
            theme::muted(),
        );
        let install = Rect::from_min_size(
            Pos2::new(dialog.right() - 140.0, dialog.bottom() - 52.0),
            Vec2::new(116.0, 36.0),
        );
        // 仅在已选中某个版本且点击了"下载并安装"按钮时：关闭浏览器对话框，
        // 并打开加载器（loader）前置选择对话框，进入安装流程的下一步。
        if let Some(version) = selected_version
            && settings_button(
                ui,
                painter,
                install,
                "official-install",
                "下载并安装",
                false,
            )
        {
            self.official_browser_open = false;
            self.open_preinstall_loader_dialog(version);
        }
    }

    /// 绘制右下角的游戏 / 资源安装进度卡片。
    ///
    /// 展示安装阶段文案、版本名、剩余文件数、实时下载速度、百分比与进度条，
    /// 以及已完成 / 总字节数。若 `install_progress` 为 `None` 则不绘制。
    ///
    /// # 参数
    /// - `painter`：绘制画笔。
    /// - `screen`：屏幕矩形，用于定位（右下角）与宽度上限。
    /// - `time`：累积时间（秒），用于装饰性光球（`soft_orb`）的水平漂移动画。
    ///
    /// # 不变量
    /// 纯读取 `self.install_progress` 与 `self.install_version_id`，不写入状态。
    pub(in crate::app) fn paint_install_progress(
        &self,
        painter: &Painter,
        screen: Rect,
        time: f64,
    ) {
        // 没有进行中的安装任务时不绘制。
        let Some(progress) = &self.install_progress else {
            return;
        };
        // 卡片宽度 390px（不超过屏幕减 36px），固定在屏幕右下角，高 108px。
        let width = 390.0_f32.min(screen.width() - 36.0);
        let rect = Rect::from_min_size(
            Pos2::new(screen.right() - width - 24.0, screen.bottom() - 178.0),
            Vec2::new(width, 108.0),
        );
        theme::glass_panel(
            painter,
            rect,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(22, 34, 26), theme::GREEN, time, 19.0),
        );
        let content = card_clip(painter, rect);
        let painter = &content;
        theme::soft_orb(
            painter,
            Pos2::new(
                rect.left() + 24.0 + ((time * 0.08) % 1.0) as f32 * (rect.width() - 48.0),
                rect.top() + 30.0,
            ),
            70.0,
            theme::GREEN,
            13,
        );
        // 卡片标题：安装阶段文案（如"下载资源文件"）。
        theme::bold_text(
            painter,
            Pos2::new(rect.left() + 18.0, rect.top() + 15.0),
            &progress.stage,
            10.5,
            theme::ink(),
        );
        let version = self.install_version_id.as_deref().unwrap_or("Minecraft");
        let file_status = if progress.files_total == 0 {
            "正在统计文件".to_owned()
        } else {
            // 1_048_576.0 = 1 MiB 的字节数，将下载速度（字节/秒）换算为 MB/秒。
            format!(
                "剩余 {} 个文件 · {:.1} MB/s",
                progress.files_total.saturating_sub(progress.files_done),
                progress.bytes_per_second as f64 / 1_048_576.0
            )
        };
        theme::text(
            painter,
            Pos2::new(rect.left() + 18.0, rect.top() + 37.0),
            format!("{version} · {file_status}"),
            8.2,
            theme::muted(),
        );
        // 整体进度比例：已完成字节 / 总字节，夹到 [0,1]；总字节未知时视为 0%。
        let progress_ratio = if progress.total_bytes > 0 {
            (progress.completed_bytes as f32 / progress.total_bytes as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };
        theme::text(
            painter,
            Pos2::new(rect.right() - 58.0, rect.top() + 15.0),
            format!("{}%", (progress_ratio * 100.0).round() as u32),
            9.0,
            theme::GREEN_BRIGHT,
        );
        // 进度条与百分比：填充宽度随 progress_ratio 增长；比例 > 0 才绘制填充。
        let track = Rect::from_min_size(
            Pos2::new(rect.left() + 18.0, rect.top() + 64.0),
            Vec2::new(rect.width() - 36.0, 9.0),
        );
        theme::fill_round(
            painter,
            track,
            theme::corner_radius(),
            theme::white_alpha(17),
        );
        if progress_ratio > 0.0 {
            theme::fill_round(
                painter,
                Rect::from_min_size(
                    track.left_top(),
                    Vec2::new(track.width() * progress_ratio, track.height()),
                ),
                theme::corner_radius(),
                theme::GREEN,
            );
        }
        // 底部字节数明细：已完成 / 总量，统一换算为 MB 显示（保留 1 位小数）。
        theme::text(
            painter,
            Pos2::new(rect.left() + 18.0, rect.bottom() - 22.0),
            format!(
                "{:.1} MB / {:.1} MB",
                progress.completed_bytes as f64 / 1_048_576.0,
                progress.total_bytes as f64 / 1_048_576.0
            ),
            8.0,
            theme::muted(),
        );
    }

    /// 绘制整合包（modpack）导入进度卡片。
    ///
    /// 当存在导入接收器（`modpack_import_receiver`）且已有最新进度更新
    /// （`modpack_import_update`）时绘制：展示阶段文案、消息以及一段循环脉动的
    /// "运行"进度条（用于表示非确定性的导入过程）。
    ///
    /// # 参数
    /// - `painter`：绘制画笔。
    /// - `screen`：屏幕矩形，用于定位（右下角）与宽度上限。
    /// - `time`：累积时间（秒），驱动脉动条的位置与宽度的正弦动画。
    ///
    /// # 不变量
    /// 纯读取 `self.modpack_import_receiver` 与 `self.modpack_import_update`，
    /// 不写入状态。两者均缺失时不绘制。
    pub(in crate::app) fn paint_modpack_import_progress(
        &self,
        painter: &Painter,
        screen: Rect,
        time: f64,
    ) {
        // 没有进行中的导入任务（接收器不存在）时不绘制。
        if self.modpack_import_receiver.is_none() {
            return;
        }
        // 尚未收到第一批进度更新时不绘制，避免闪现空卡片。
        let Some(update) = self.modpack_import_update.as_ref() else {
            return;
        };
        let width = 390.0_f32.min(screen.width() - 36.0);
        let rect = Rect::from_min_size(
            Pos2::new(screen.right() - width - 24.0, screen.bottom() - 178.0),
            Vec2::new(width, 108.0),
        );
        theme::glass_panel(
            painter,
            rect,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(22, 34, 26), theme::GREEN, time, 19.5)
                .backdrop_blur(18.0),
        );
        let content = card_clip(painter, rect);
        let painter = &content;
        // 阶段标题 + 截断后的详细消息。
        theme::bold_text(
            painter,
            Pos2::new(rect.left() + 18.0, rect.top() + 15.0),
            &update.stage,
            10.5,
            theme::ink(),
        );
        theme::text(
            painter,
            Pos2::new(rect.left() + 18.0, rect.top() + 39.0),
            fit_text_to_width(
                painter,
                &update.message,
                theme::font(8.0),
                rect.width() - 36.0,
            ),
            8.0,
            theme::muted(),
        );
        let track = Rect::from_min_size(
            Pos2::new(rect.left() + 18.0, rect.top() + 72.0),
            Vec2::new(rect.width() - 36.0, 9.0),
        );
        theme::fill_round(
            painter,
            track,
            theme::corner_radius(),
            theme::white_alpha(17),
        );
        // 非确定性进度条：用正弦波驱动一段"光带"在轨道内左右移动与伸缩，表示导入进行中
        // （pulse 控制宽度，run_left 控制位置；两者相位相差以避免同步）。
        let pulse = (0.18 + (time * 0.28).sin().abs() as f32 * 0.42).clamp(0.0, 0.72);
        let run_width = track.width() * pulse;
        let run_left =
            track.left() + (track.width() - run_width) * (0.5 + (time * 0.37).sin() as f32 * 0.5);
        theme::fill_round(
            painter,
            Rect::from_min_size(
                Pos2::new(run_left, track.top()),
                Vec2::new(run_width, track.height()),
            ),
            theme::corner_radius(),
            theme::GREEN,
        );
    }
}
