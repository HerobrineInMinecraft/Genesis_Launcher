//! 下载页面渲染模块。
//!
//! 本文件属于 `app::pages` 子模块，承载启动器"下载"标签页的全部界面绘制逻辑。
//! 它通过为 `LauncherApp` 实现一系列 `paint_*` 方法，在 egui 的自定义 Painter
//! 上绘制"下载游戏版本"与"下载社区资源（Modrinth）"两类界面。
//!
//! 主要定义的公开方法（受 `pub(in crate::app)` 限制，仅对 app 模块可见）：
//! - `LauncherApp::paint_download`：下载页总入口，负责顶部分类切换条、
//!   页面滑动转场动画，并根据当前所选分类分发到原版列表或资源下载界面。
//! - `LauncherApp::paint_resource_download`：Modrinth 资源搜索/下载界面。
//! - `LauncherApp::paint_resource_detail`：单个资源的详情与下载卡片。
//! - `LauncherApp::paint_resource_version_menu`：资源适用的 Minecraft 版本
//!   下拉选择菜单（弹窗）。
//! - `LauncherApp::paint_download_detail`：原版版本详情，含模组加载器
//!   （Fabric/Forge/NeoForge）选择与安装按钮。
//!
//! 在启动器整体角色中，本模块只负责"绘制"与"交互响应"，所有网络请求与状态
//! 变更都委托给 `self` 上的相应方法（如 `refresh_official_manifest`、
//! `start_resource_search`、`start_official_install` 等），自身不发起异步任务。

use super::super::*;

impl LauncherApp {
    /// 绘制"下载"标签页的主界面。
    ///
    /// 这是下载页面绘制的统一入口。它先绘制顶部"资源分类切换条"
    /// （原版 + 各社区资源分类），并通过一个滑动的指示块体现当前选中项；
    /// 随后在内容区渲染"页面转场动画"（旧页面以截图纹理滑出、新页面滑入）。
    ///
    /// 当 `download_resource_category` 为 `Some` 时，调用
    /// `paint_resource_download` 渲染社区资源下载；否则渲染原版
    /// （Mojang 官方）版本列表与版本详情。
    ///
    /// # 参数
    /// - `_ctx`：egui 上下文（当前函数未直接使用，保留以便与页面绘制签名一致）。
    /// - `ui`：egui 的 UI 句柄，用于注册交互与放置文本输入框。
    /// - `painter`：当前层级的绘制器，所有图形都画在它上面。
    /// - `layout`：整体布局信息（内容区左右边界、屏幕矩形、头部矩形等）。
    /// - `time`：当前时间（秒），用于玻璃面板等随时间变化的动画/着色。
    /// - `motion`：卡片转场动画状态（方向、进度、是否离场），用于详情切换。
    ///
    /// # 副作用
    /// 可能修改 `self` 上的下载状态（如选中版本、滚动偏移、触发分类切换等），
    /// 但不会发起网络请求——网络相关动作由本函数调用的 `self` 方法负责。
    pub(in crate::app) fn paint_download(
        &mut self,
        _ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
        motion: CardMotion,
    ) {
        // 内容区矩形：顶部从头部下方 14px 起，底部距屏幕下沿 22px 止。
        let outer_page = Rect::from_min_max(
            Pos2::new(layout.content_left, layout.header.bottom() + 14.0),
            Pos2::new(layout.content_right, layout.screen.bottom() - 22.0),
        );
        // 构造顶部分类选项：固定第一项"原版"（None 表示官方版本），
        // 随后拼接所有社区资源分类（每个分类附带其展示名 label()）。
        let options: Vec<(Option<ResourceCategory>, &str)> = std::iter::once((None, "原版"))
            .chain(
                ResourceCategory::ALL
                    .into_iter()
                    .map(|category| (Some(category), category.label())),
            )
            .collect();
        // 分类条矩形（高 44）：应用当前转场动画 motion 得到其在屏幕上的实际位置。
        let category_bar = motion.rect(Rect::from_min_size(
            outer_page.left_top(),
            Vec2::new(outer_page.width(), 44.0),
        ));
        // 保存页面级绘制器（稍后内容区会借用它），避免被下面的影子绑定覆盖。
        let page_base = painter;
        // 绘制分类条的玻璃背景面板（深绿底色 + 绿色高光，随时间轻微变化）。
        theme::glass_panel(
            painter,
            category_bar,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(19, 31, 23), theme::GREEN, time, 27.2),
        );
        // 在分类条范围内裁剪绘制器，防止标签文字溢出圆角区域。
        let category_content = card_clip(painter, category_bar);
        let painter = &category_content;
        // 标签之间的间距与单个标签宽度：6 个标签，左右各留 10px，间隙 5 个。
        let tab_gap = 7.0;
        let tab_width = (category_bar.width() - 20.0 - tab_gap * 5.0) / 6.0;
        // 由当前分类计算选中标签序号（原版为 0，资源分类顺延）。
        let selected_index = resource_category_index(self.download_resource_category);
        // 指示块中心 X：左边缘 + 10px 内边距 + 归一化位置 × 步距（含标签宽度一半居中）。
        let indicator_center = category_bar.left()
            + 10.0
            + self.download_category_position.clamp(0.0, 5.0) * (tab_width + tab_gap)
            + tab_width * 0.5;
        // 指示块随切换速度产生的额外拉伸宽度（速度越快越宽，最多 16px），营造惯性感。
        let indicator_stretch = (self.download_category_velocity.abs() * 4.0).clamp(0.0, 16.0);
        let indicator = Rect::from_center_size(
            Pos2::new(indicator_center, category_bar.top() + 22.0),
            Vec2::new(tab_width + indicator_stretch, 32.0),
        );
        theme::fill_round(
            painter,
            indicator,
            theme::corner_radius(),
            theme::alpha(theme::GREEN, 28),
        );
        theme::stroke_round(
            painter,
            indicator,
            theme::corner_radius(),
            1.0,
            theme::alpha(theme::GREEN_BRIGHT, 56),
        );
        // 遍历每个分类标签，绘制可点击的标签按钮。
        for (index, (category, label)) in options.into_iter().enumerate() {
            // 计算标签矩形：按序号排布，顶部留 6px，宽 tab_width 高 32。
            let tab = Rect::from_min_size(
                Pos2::new(
                    category_bar.left() + 10.0 + index as f32 * (tab_width + tab_gap),
                    category_bar.top() + 6.0,
                ),
                Vec2::new(tab_width, 32.0),
            );
            // 注册点击交互，鼠标悬停时显示"手指"光标。
            let response = ui
                .interact(
                    tab,
                    Id::unique(("download-category", index)),
                    Sense::click(),
                )
                .on_hover_cursor(CursorIcon::PointingHand);
            let selected = index == selected_index;
            // 非选中标签绘制背景填充：悬停略亮、否则极淡。
            if !selected {
                theme::fill_round(
                    painter,
                    tab,
                    theme::corner_radius(),
                    if response.hovered() {
                        theme::white_alpha(11)
                    } else {
                        theme::white_alpha(5)
                    },
                );
            }
            // 居中绘制标签文字：选中用主色、否则用次要色；selected 控制是否加粗。
            theme::center_text(
                painter,
                tab.center(),
                label,
                8.3,
                if selected {
                    theme::ink()
                } else {
                    theme::muted()
                },
                selected,
            );
            // 点击且并非当前标签时，发起分类切换（含转场动画）。
            if response.clicked() && !selected {
                self.begin_download_category_switch(ui.ctx(), category);
            }
        }
        // 恢复页面级绘制器，用于绘制分类条下方的内容区。
        let painter = page_base;
        // 内容区顶边：至少为分类条底边，并在内容区范围内夹紧。
        let content_top = category_bar
            .bottom()
            .clamp(outer_page.top(), outer_page.bottom());
        // 内容裁剪框：从 content_top 到内容区右下角。
        let content_clip = Rect::from_min_max(
            Pos2::new(outer_page.left(), content_top),
            outer_page.right_bottom(),
        );
        // 页面主体基准矩形：顶部从内容区上沿下移 54px（避开分类条）到底部。
        let body_base = Rect::from_min_max(
            Pos2::new(outer_page.left(), outer_page.top() + 54.0),
            outer_page.right_bottom(),
        );
        // 在内容裁剪框内绘制，确保转场画面不会溢出。
        let transition_layer = painter.with_clip_rect(content_clip);
        let painter = &transition_layer;
        // 若存在"切换前页面"的截图纹理，且尺寸有效，则绘制离场（旧）页面。
        if let Some(snapshot) = self.download_switch_texture.as_ref()
            && self.download_switch_texture_size[0] > 0
            && self.download_switch_texture_size[1] > 0
        {
            // 旧页面水平偏移：沿转场方向滑出；进度越接近 1 越完全离开。
            // 进度上限 1.08 以允许略微过冲，使动画更自然。
            let outgoing_offset = -self.download_page_direction
                * content_clip.width()
                * self.download_page_progress.clamp(0.0, 1.08);
            let outgoing_rect = content_clip.translate(Vec2::new(outgoing_offset, 0.0));
            // 计算截图纹理的 UV：把内容框相对整屏的位置映射到 [0,1] 纹理坐标。
            let uv = Rect::from_min_max(
                Pos2::new(
                    ((content_clip.left() - layout.screen.left()) / layout.screen.width())
                        .clamp(0.0, 1.0),
                    ((content_clip.top() - layout.screen.top()) / layout.screen.height())
                        .clamp(0.0, 1.0),
                ),
                Pos2::new(
                    ((content_clip.right() - layout.screen.left()) / layout.screen.width())
                        .clamp(0.0, 1.0),
                    ((content_clip.bottom() - layout.screen.top()) / layout.screen.height())
                        .clamp(0.0, 1.0),
                ),
            );
            // 用裁剪后的纹理把旧页面画到滑出位置，纯白着色表示原样显示。
            painter.image(snapshot.id(), outgoing_rect, uv, Color32::WHITE);
        }
        // 新页面（当前要显示的分类）的水平偏移：进度越小越靠外侧，随转场完成归零。
        let slide_offset = self.download_page_direction
            * content_clip.width()
            * (1.0 - self.download_page_progress.clamp(0.0, 1.08));
        let page = body_base.translate(Vec2::new(slide_offset, 0.0));
        // 若当前选中的是社区资源分类，直接绘制资源下载界面并结束本函数。
        if self.download_resource_category.is_some() {
            self.paint_resource_download(ui, painter, page, time, motion);
            return;
        }
        // 左侧列表宽度：占页面 44%，并限制在 390~540px 之间。
        let list_width = (page.width() * 0.44).clamp(390.0, 540.0);
        // 左侧"版本列表"区域：从页面左上到左+list_width 处。
        let source_list = Rect::from_min_max(
            page.left_top(),
            Pos2::new(page.left() + list_width, page.bottom()),
        );
        // 应用转场动画得到列表面板实际位置。
        let list_panel = motion.rect(source_list);
        // 列表顶部标题/搜索区（高 148）：展示标题、状态、刷新按钮与搜索框。
        let list_heading =
            Rect::from_min_size(list_panel.left_top(), Vec2::new(list_panel.width(), 148.0));
        // 保存页面级绘制器，稍后列表内容区会复用它。
        let page_painter = painter;
        // 绘制列表标题区的玻璃面板。
        theme::glass_panel(
            painter,
            list_heading,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(20, 32, 24), theme::GREEN, time, 28.0),
        );
        // 裁剪标题区内部绘制器，限制文字在圆角面板内。
        let list_heading_content = card_clip(painter, list_heading);
        let painter = &list_heading_content;
        // 主标题"下载游戏版本"。
        theme::bold_text(
            painter,
            Pos2::new(list_heading.left() + 22.0, list_heading.top() + 15.0),
            "下载游戏版本",
            19.0,
            theme::ink(),
        );
        // 副标题：优先显示错误，其次显示"连接中"，最后显示版本总数。
        theme::text(
            painter,
            Pos2::new(list_heading.left() + 22.0, list_heading.top() + 47.0),
            if let Some(error) = &self.official_error {
                ellipsize_end(error, 42)
            } else if self.official_manifest.is_some() {
                "正在连接 Mojang Piston Meta…".to_owned()
            } else {
                format!(
                    "Mojang 官方源 · {} 个可下载版本",
                    self.official_versions.len()
                )
            },
            8.4,
            theme::muted(),
        );
        // 右上角"刷新"按钮矩形。
        let refresh = Rect::from_min_size(
            Pos2::new(list_heading.right() - 91.0, list_heading.top() + 14.0),
            Vec2::new(69.0, 30.0),
        );
        // 仅在尚未获取清单时，点击刷新才发起对 Mojang Piston Meta 的重新拉取。
        if settings_button(ui, painter, refresh, "download-refresh", "刷新", false)
            && self.official_manifest.is_none()
        {
            self.refresh_official_manifest();
        }

        // 搜索框矩形：标题区左缘 + 22px，顶部下方 64px，宽为标题区宽减 44px。
        let search = Rect::from_min_size(
            Pos2::new(list_heading.left() + 22.0, list_heading.top() + 64.0),
            Vec2::new(list_heading.width() - 44.0, 34.0),
        );
        // 搜索框背景与描边。
        theme::fill_round(
            painter,
            search,
            theme::corner_radius(),
            Color32::from_rgb(25, 38, 29),
        );
        theme::stroke_round(
            painter,
            search,
            theme::corner_radius(),
            1.0,
            theme::white_alpha(30),
        );
        // 搜索框左侧的放大镜图标。
        draw_icon(
            painter,
            NavIcon::Search,
            Rect::from_center_size(
                Pos2::new(search.left() + 17.0, search.center().y),
                Vec2::splat(13.0),
            ),
            theme::muted(),
        );
        // 把单行文本编辑框（无边框、自定义字体/颜色/占位符）嵌入搜索框预留区域。
        let _ = ui.put(
            Rect::from_min_max(
                Pos2::new(search.left() + 31.0, search.top() + 5.0),
                Pos2::new(search.right() - 10.0, search.bottom() - 5.0),
            ),
            egui::TextEdit::singleline(&mut self.download_search)
                .frame(egui::Frame::NONE)
                .font(theme::font(8.8))
                .text_color(theme::adaptive_text_color_at(
                    painter,
                    search.center(),
                    theme::ink(),
                    8.8,
                ))
                .hint_text("搜索 Minecraft 版本号"),
        );

        // 类型筛选按钮组：全部 / 正式版 / 快照。
        let filters = [
            (VersionFilter::All, "全部"),
            (VersionFilter::Release, "正式版"),
            (VersionFilter::Snapshot, "快照"),
        ];
        // 从左到右依次排布三个筛选按钮。
        let mut filter_x = list_heading.left() + 22.0;
        for (index, (filter, label)) in filters.into_iter().enumerate() {
            // 每个按钮宽 66 高 27，从 filter_x 起，固定在顶部 108px 处。
            let button = Rect::from_min_size(
                Pos2::new(filter_x, list_heading.top() + 108.0),
                Vec2::new(66.0, 27.0),
            );
            // 注册点击交互与手指光标。
            let response = ui
                .interact(
                    button,
                    Id::unique(("download-filter", index)),
                    Sense::click(),
                )
                .on_hover_cursor(CursorIcon::PointingHand);
            // 判断是否为当前生效的筛选条件。
            let selected = self.official_filter == filter;
            theme::fill_round(
                painter,
                button,
                theme::corner_radius(),
                if selected {
                    theme::alpha(theme::GREEN, 27)
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
            // 点击筛选：切换筛选条件并重置滚动/详情状态，避免残留旧选中项的动画。
            if response.clicked() {
                self.official_filter = filter;
                self.official_scroll = 0.0;
                self.previous_download_detail = None;
                self.download_detail_progress = 1.0;
            }
            // 下一个按钮的起点：当前按钮右边缘 + 7px 间距。
            filter_x = button.right() + 7.0;
        }

        // 计算搜索关键词（去空格转小写，用于不区分大小写的包含匹配）。
        let search_query = self.download_search.trim().to_lowercase();
        // 按"版本号包含关键词"且"符合当前类型筛选"过滤官方版本，保留原始下标。
        let filtered: Vec<(usize, crate::download::OfficialVersion)> = self
            .official_versions
            .iter()
            .enumerate()
            .filter(|(_, version)| {
                version.id.to_lowercase().contains(&search_query)
                    && match self.official_filter {
                        VersionFilter::All => true,
                        VersionFilter::Release => version.kind == "release",
                        VersionFilter::Snapshot => version.kind == "snapshot",
                    }
            })
            .map(|(index, version)| (index, version.clone()))
            .collect();
        // 若筛选结果非空但当前选中项已不在结果中，则把选中项归位到第一项，
        // 并重置加载器与详情切换状态，使详情面板同步到新的选中版本。
        if !filtered.is_empty()
            && !filtered
                .iter()
                .any(|(index, _)| *index == self.official_selected)
        {
            self.official_selected = filtered[0].0;
            self.reset_download_loader();
            self.previous_download_detail = None;
            self.download_detail_progress = 1.0;
        }

        // 恢复页面级绘制器，用于绘制列表主体与详情区。
        let painter = page_painter;
        // 列表内容区起点：避开标题区（下移 156px）。
        let source_list_top = source_list.top() + 156.0;
        let list_top = list_panel.top() + 156.0;
        let list_bottom = list_panel.bottom();
        // 可视高度，至少 1px 避免除零/负值。
        let list_height = (list_bottom - list_top).max(1.0);
        let row_height = 58.0;
        // 全部内容总高度与最大可滚动距离（内容超出可视区时为正）。
        let content_height = filtered.len() as f32 * row_height;
        let max_scroll = (content_height - list_height).max(0.0);
        // 鼠标悬停在列表内且存在平滑滚动增量时，按 1.05 倍系数累加滚动偏移。
        if ui.ctx().input(|input| {
            input
                .pointer
                .hover_pos()
                .is_some_and(|pointer| list_panel.contains(pointer))
                && input.smooth_scroll_delta.y.abs() > 0.0
        }) {
            self.official_scroll -= ui.ctx().input(|input| input.smooth_scroll_delta.y) * 1.05;
        }
        // 将滚动偏移夹紧到 [0, max_scroll]，避免越界。
        self.official_scroll = self.official_scroll.clamp(0.0, max_scroll);
        let list_viewport = Rect::from_min_max(
            Pos2::new(list_panel.left() + 2.0, list_top),
            Pos2::new(list_panel.right() - 9.0, list_bottom),
        );
        // 在可视裁剪框内绘制列表，超出部分不会显示。
        let list_painter = painter.with_clip_rect(list_viewport);
        // 遍历筛选结果的每一个版本，计算其在滚动后的 Y 坐标。
        for (visible_index, (global_index, version)) in filtered.iter().enumerate() {
            // 该行实际 Y：列表起点 + 序号×行高 - 滚动偏移。
            let source_y =
                source_list_top + visible_index as f32 * row_height - self.official_scroll;
            // 视口裁剪：整行都在可视区上方或下方时跳过绘制（提升性能）。
            if source_y + row_height < source_list_top || source_y > source_list.bottom() {
                continue;
            }
            let row = motion.rect(Rect::from_min_size(
                Pos2::new(source_list.left() + 4.0, source_y),
                Vec2::new(source_list.width() - 14.0, row_height - 4.0),
            ));
            // 注册版本行的点击交互（用 global_index 保证唯一 Id）。
            let response = ui
                .interact(
                    row,
                    Id::unique(("download-version", global_index)),
                    Sense::click(),
                )
                .on_hover_cursor(CursorIcon::PointingHand);
            // 是否为当前选中的版本。
            let selected = self.official_selected == *global_index;
            // 行内卡片：整体收缩 2px 形成行间距留白。
            let card = row.shrink2(Vec2::new(2.0, 2.0));
            // 绘制玻璃卡片：选中或悬停时底色更亮，并据悬停状态调整高光。
            theme::glass_panel(
                &list_painter,
                card,
                theme::corner_radius(),
                theme::GlassSpec::new(
                    if selected || response.hovered() {
                        Color32::from_rgb(29, 48, 33)
                    } else {
                        Color32::from_rgb(20, 33, 24)
                    },
                    theme::GREEN,
                    time,
                    row.top() * 0.01,
                )
                .hovered(response.hovered()),
            );
            // 选中项额外描一道绿色边框以突出。
            let row_painter = card_clip(&list_painter, card);
            if selected {
                theme::stroke_round(
                    &list_painter,
                    card,
                    theme::corner_radius(),
                    1.0,
                    theme::alpha(theme::GREEN, 74),
                );
            }
            // 版本图标矩形（左侧 28px 见方）。
            let icon = Rect::from_center_size(
                Pos2::new(row.left() + 21.0, row.center().y),
                Vec2::splat(28.0),
            );
            // 按版本类型选择图标纹理：快照用 snapshot 图标，其余用 release 图标。
            let texture = if version.kind == "snapshot" {
                self.snapshot_icon_texture.as_ref()
            } else {
                self.release_icon_texture.as_ref()
            };
            // 若对应图标纹理已加载，绘制圆角图标；否则留空（不绘制占位）。
            if let Some(texture) = texture {
                paint_rounded_image(&row_painter, icon, 8.0, texture, texture.size());
            }
            // 版本号（加粗）。
            theme::bold_text(
                &row_painter,
                Pos2::new(row.left() + 42.0, row.top() + 9.0),
                &version.id,
                10.3,
                theme::ink_soft(),
            );
            // 版本类型描述（如"正式版"/"快照"，由 download_version_kind 翻译）。
            theme::text(
                &row_painter,
                Pos2::new(row.left() + 42.0, row.top() + 29.0),
                download_version_kind(&version.kind),
                7.6,
                theme::muted(),
            );
            // 点击该行：切换到该版本的详情视图（含转场动画）。
            if response.clicked() {
                self.switch_download_detail(*global_index);
            }
        }
        // 列表为空时，按不同状态显示对应提示文案（加载中/错误/无匹配/无版本）。
        if filtered.is_empty() {
            theme::center_text(
                &list_painter,
                Pos2::new(source_list.center().x, list_top + 64.0),
                if self.official_manifest.is_some() {
                    "正在获取官方版本…"
                } else if let Some(error) = &self.official_error {
                    error
                } else if !self.download_search.trim().is_empty() {
                    "没有匹配的 Minecraft 版本"
                } else {
                    "当前分类没有可下载版本"
                },
                9.0,
                theme::muted(),
                false,
            );
        }
        // 取出当前选中的版本；若找不到（理论上已被前面逻辑复位）则结束绘制。
        let Some((_, current)) = filtered
            .iter()
            .find(|(index, _)| *index == self.official_selected)
            .cloned()
        else {
            return;
        };
        // 选中版本变化时，更新加载器基准版本并重置已选加载器状态。
        if self.loader_base_version != current.id {
            self.loader_base_version = current.id.clone();
            self.reset_download_loader();
        }
        // 若存在"上一个详情"（正在切换中），先绘制离场的旧详情，再绘制进入的新详情，
        // 形成同一区域内新旧卡片的转场动画。
        let current_motion = if let Some(previous_index) = self.previous_download_detail {
            // 旧详情的运动状态：标记为 outgoing（离场），使用当前转场方向与进度。
            let outgoing = CardMotion {
                screen: layout.screen,
                direction: self.download_detail_direction,
                progress: self.download_detail_progress,
                outgoing: true,
            };
            // 取出上一个版本数据并绘制其离场详情（active=false 不响应安装等交互）。
            if let Some(previous) = self.official_versions.get(previous_index).cloned() {
                self.paint_download_detail(
                    ui,
                    painter,
                    page,
                    time,
                    DownloadDetailView {
                        version: previous,
                        motion: outgoing,
                        active: false,
                    },
                );
            }
            // 新详情的运动状态：incoming（进入），使用相同方向与进度。
            CardMotion {
                screen: layout.screen,
                direction: self.download_detail_direction,
                progress: self.download_detail_progress,
                outgoing: false,
            }
        } else {
            // 无切换时直接复用页面整体 motion。
            motion
        };
        // 绘制当前选中版本的详情（active=true，可交互）。
        self.paint_download_detail(
            ui,
            painter,
            page,
            time,
            DownloadDetailView {
                version: current,
                motion: current_motion,
                active: true,
            },
        );
    }

    /// 绘制社区资源（Modrinth）下载界面。
    ///
    /// 在 `download_resource_category` 为某个资源分类时由 `paint_download`
    /// 调用。界面分为左右两栏：左侧是版本筛选按钮 + 搜索框 + 资源结果列表，
    /// 右侧（由 `paint_resource_detail` 绘制）是选中项目的详情与下载入口。
    ///
    /// # 参数
    /// - `ui`：egui UI 句柄，用于交互与文本输入。
    /// - `painter`：当前层级绘制器。
    /// - `page`：内容区矩形（已由上层完成转场位移）。
    /// - `time`：当前时间（秒），用于动画着色。
    /// - `motion`：转场动画状态。
    ///
    /// # 副作用
    /// 可能触发 `resource_version_menu_open` 的切换、发起搜索、切换选中资源，
    /// 以及调用 `paint_resource_detail` 与 `paint_resource_version_menu` 绘制子界面。
    /// 不直接发起网络请求（搜索由 `start_resource_search` 负责）。
    pub(in crate::app) fn paint_resource_download(
        &mut self,
        ui: &mut egui::Ui,
        painter: &Painter,
        page: Rect,
        time: f64,
        motion: CardMotion,
    ) {
        // 若当前分类为空（理论上不会进入本函数），直接返回。
        let Some(category) = self.download_resource_category else {
            return;
        };
        // 左侧列表宽度同原版逻辑：页面 44%，限定 390~540px。
        let list_width = (page.width() * 0.44).clamp(390.0, 540.0);
        // 左侧列表区域：从页面左边到左+list_width。
        let source_list = Rect::from_min_max(
            page.left_top(),
            Pos2::new(page.left() + list_width, page.bottom()),
        );
        // 右侧详情区域：从列表右缘 +14px 间距到页面右下角。
        let source_detail = Rect::from_min_max(
            Pos2::new(source_list.right() + 14.0, page.top()),
            page.right_bottom(),
        );
        // 保存页面级绘制器供后续复用。
        let page_painter = painter;
        // 应用转场得到列表面板实际位置。
        let list_panel = motion.rect(source_list);
        // 左侧顶部标题/筛选/搜索区（高 130）。
        let heading =
            Rect::from_min_size(list_panel.left_top(), Vec2::new(list_panel.width(), 130.0));
        // 绘制标题区的玻璃背景面板。
        theme::glass_panel(
            painter,
            heading,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(20, 32, 24), theme::GREEN, time, 32.0),
        );
        // 裁剪出标题区内部绘制器，限制文字在圆角面板内。
        let heading_content = card_clip(painter, heading);
        let painter = &heading_content;
        // 右上角的"版本筛选"按钮矩形（150×28，靠右对齐）。
        let version_filter_button = Rect::from_min_size(
            Pos2::new(heading.right() - 170.0, heading.top() + 10.0),
            Vec2::new(150.0, 28.0),
        );
        // 记录该按钮矩形，供弹出版本菜单时定位（见 paint_resource_version_menu）。
        self.resource_version_anchor = version_filter_button;
        // 注册版本筛选按钮的点击交互。
        let version_filter_response = ui
            .interact(
                version_filter_button,
                Id::unique("resource-version-filter"),
                Sense::click(),
            )
            .on_hover_cursor(CursorIcon::PointingHand);
        // 按钮背景：悬停或菜单打开时为绿色高亮，否则极淡。
        theme::fill_round(
            painter,
            version_filter_button,
            theme::corner_radius(),
            if version_filter_response.hovered() || self.resource_version_menu_open {
                theme::alpha(theme::GREEN, 25)
            } else {
                theme::white_alpha(7)
            },
        );
        // 按钮描边：菜单打开时用更亮的绿色以示激活状态。
        theme::stroke_round(
            painter,
            version_filter_button,
            theme::corner_radius(),
            1.0,
            if self.resource_version_menu_open {
                theme::alpha(theme::GREEN_BRIGHT, 70)
            } else {
                theme::white_alpha(26)
            },
        );
        // 按钮文字："版本 1.20.4"（过长自动省略，预留右侧箭头空间）。
        theme::text(
            painter,
            Pos2::new(
                version_filter_button.left() + 11.0,
                version_filter_button.top() + 8.0,
            ),
            fit_text_to_width(
                painter,
                &format!("版本 {}", self.resource_version_filter),
                theme::font(8.0),
                version_filter_button.width() - 34.0,
            ),
            8.0,
            theme::ink_soft(),
        );
        // 按钮右侧的下拉箭头图标。
        draw_icon(
            painter,
            NavIcon::Chevron,
            Rect::from_center_size(
                Pos2::new(
                    version_filter_button.right() - 14.0,
                    version_filter_button.center().y,
                ),
                Vec2::splat(12.0),
            ),
            theme::muted(),
        );
        // 点击版本筛选按钮：切换菜单开关并重置其滚动；请求重绘以反映状态变化。
        if version_filter_response.clicked() {
            self.resource_version_menu_open = !self.resource_version_menu_open;
            self.resource_version_scroll = 0.0;
            ui.ctx().request_repaint();
        }
        // 标题：如"下载 Mod"/"下载 整合包"，按可用宽度自动省略过长文本。
        theme::bold_text(
            painter,
            Pos2::new(heading.left() + 20.0, heading.top() + 13.0),
            fit_text_to_width(
                painter,
                &format!("下载{}", category.label()),
                theme::bold_font(18.0),
                heading.width() - 200.0,
            ),
            18.0,
            theme::ink(),
        );
        // 副标题：显示所筛选的游戏版本、来源 Modrinth，以及（当分类为 Mod/整合包
        // 且当前已选用非原版加载器时）追加加载器家族名，提示将按该加载器筛选资源。
        theme::text(
            painter,
            Pos2::new(heading.left() + 20.0, heading.top() + 40.0),
            format!(
                "Minecraft {} · Modrinth · 自动选路{}{}",
                self.resource_version_filter,
                if matches!(category, ResourceCategory::Mod | ResourceCategory::Modpack)
                    && self.version().loader_family != "原版"
                {
                    " · "
                } else {
                    ""
                },
                if matches!(category, ResourceCategory::Mod | ResourceCategory::Modpack)
                    && self.version().loader_family != "原版"
                {
                    self.version().loader_family.as_str()
                } else {
                    ""
                }
            ),
            8.1,
            theme::muted(),
        );
        // 搜索框矩形：标题区左缘 + 20px，顶部下方 61px，宽为标题区宽减 110px（给右侧按钮留位）。
        let search = Rect::from_min_size(
            Pos2::new(heading.left() + 20.0, heading.top() + 61.0),
            Vec2::new(heading.width() - 110.0, 34.0),
        );
        // 搜索框背景与描边。
        theme::fill_round(
            painter,
            search,
            theme::corner_radius(),
            Color32::from_rgb(25, 38, 29),
        );
        theme::stroke_round(
            painter,
            search,
            theme::corner_radius(),
            1.0,
            theme::white_alpha(28),
        );
        // 搜索框左侧的放大镜图标。
        draw_icon(
            painter,
            NavIcon::Search,
            Rect::from_center_size(
                Pos2::new(search.left() + 16.0, search.center().y),
                Vec2::splat(13.0),
            ),
            theme::muted(),
        );
        // 把单行文本编辑框嵌入搜索框预留区域，占位符随分类变化（如"搜索模组"）。
        let search_response = ui.put(
            Rect::from_min_max(
                Pos2::new(search.left() + 30.0, search.top() + 5.0),
                Pos2::new(search.right() - 8.0, search.bottom() - 5.0),
            ),
            egui::TextEdit::singleline(&mut self.download_search)
                .frame(egui::Frame::NONE)
                .font(theme::font(8.8))
                .text_color(theme::adaptive_text_color_at(
                    painter,
                    search.center(),
                    theme::ink(),
                    8.8,
                ))
                .hint_text(format!("搜索{}", category.label())),
        );
        // 右侧"搜索"按钮矩形。
        let search_button = Rect::from_min_size(
            Pos2::new(heading.right() - 82.0, heading.top() + 61.0),
            Vec2::new(62.0, 34.0),
        );
        // 触发搜索的条件：点击搜索按钮，或在输入框失去焦点且按下回车键。
        let search_clicked =
            settings_button(ui, painter, search_button, "resource-search", "搜索", true)
                || (search_response.lost_focus()
                    && ui.ctx().input(|input| input.key_pressed(egui::Key::Enter)));
        // 任一条件满足即发起 Modrinth 资源搜索。
        if search_clicked {
            self.start_resource_search();
        }

        // 恢复页面级绘制器。
        let painter = page_painter;
        // 资源列表内容起点：避开标题区（下移 140px）。
        let source_list_top = source_list.top() + 140.0;
        let list_top = list_panel.top() + 140.0;
        let list_bottom = list_panel.bottom();
        let list_height = (list_bottom - list_top).max(1.0);
        let row_height = 64.0;
        // 内容总高度与最大滚动距离。
        let content_height = self.resource_results.len() as f32 * row_height;
        let max_scroll = (content_height - list_height).max(0.0);
        // 悬停在列表内且存在平滑滚动时累加滚动偏移（1.05 倍系数）。
        if ui.ctx().input(|input| {
            input
                .pointer
                .hover_pos()
                .is_some_and(|pointer| list_panel.contains(pointer))
                && input.smooth_scroll_delta.y.abs() > 0.0
        }) {
            self.resource_scroll -= ui.ctx().input(|input| input.smooth_scroll_delta.y) * 1.05;
        }
        // 夹紧滚动范围。
        self.resource_scroll = self.resource_scroll.clamp(0.0, max_scroll);
        let list_viewport = Rect::from_min_max(
            Pos2::new(list_panel.left() + 4.0, list_top),
            Pos2::new(list_panel.right() - 10.0, list_bottom),
        );
        // 在可视裁剪框内绘制资源列表。
        let list_painter = painter.with_clip_rect(list_viewport);
        // 记录本次点击到的资源下标（在循环外统一处理，避免在绘制中修改状态）。
        let mut clicked_resource = None;
        // 遍历搜索结果，绘制每一行资源卡片并处理点击。
        for (index, project) in self.resource_results.iter().enumerate() {
            // 该行实际 Y：列表起点 + 序号×行高 - 滚动偏移。
            let source_y = source_list_top + index as f32 * row_height - self.resource_scroll;
            // 视口裁剪：整行在可视区外则跳过绘制（提升性能）。
            if source_y + row_height < source_list_top || source_y > source_list.bottom() {
                continue;
            }
            // 行矩形（应用转场动画），左右各留少量边距。
            let row = motion.rect(Rect::from_min_size(
                Pos2::new(source_list.left() + 5.0, source_y),
                Vec2::new(source_list.width() - 16.0, row_height - 5.0),
            ));
            // 注册行点击交互与手指光标（用 index 保证 Id 唯一）。
            let response = ui
                .interact(row, Id::unique(("resource-project", index)), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand);
            // 是否为当前选中的资源。
            let selected = self.resource_selected == index;
            // 绘制玻璃卡片：选中或悬停时底色更亮，并据悬停状态调整高光。
            theme::glass_panel(
                &list_painter,
                row.shrink2(Vec2::new(2.0, 2.0)),
                theme::corner_radius(),
                theme::GlassSpec::new(
                    if selected || response.hovered() {
                        Color32::from_rgb(29, 48, 33)
                    } else {
                        Color32::from_rgb(20, 33, 24)
                    },
                    theme::GREEN,
                    time,
                    row.top() * 0.01,
                )
                .hovered(response.hovered()),
            );
            // 裁剪出行内部绘制器，用于绘制行内文字。
            let row_painter = card_clip(&list_painter, row.shrink2(Vec2::new(2.0, 2.0)));
            // 第一行文字：资源标题（过长按行宽省略，给右侧下载量留空间）。
            theme::bold_text(
                &row_painter,
                Pos2::new(row.left() + 16.0, row.top() + 10.0),
                fit_text_to_width(
                    &row_painter,
                    &project.title,
                    theme::bold_font(10.0),
                    row.width() - 126.0,
                ),
                10.0,
                theme::ink_soft(),
            );
            // 第二行文字：资源简介（过长省略）。
            theme::text(
                &row_painter,
                Pos2::new(row.left() + 16.0, row.top() + 30.0),
                fit_text_to_width(
                    &list_painter,
                    &project.description,
                    theme::font(7.5),
                    row.width() - 32.0,
                ),
                7.5,
                theme::muted(),
            );
            // 右上角显示下载量（用紧凑计数如 "1.2k"）。
            theme::text(
                &row_painter,
                Pos2::new(row.right() - 84.0, row.top() + 10.0),
                format!("↓ {}", compact_count(project.downloads)),
                7.4,
                theme::GREEN_BRIGHT,
            );
            // 记录被点击的资源下标（真正切换在循环之后执行）。
            if response.clicked() {
                clicked_resource = Some(index);
            }
        }
        // 若本次有点击，切换到对应资源的详情视图（含转场）。
        if let Some(index) = clicked_resource {
            self.switch_resource_detail(index);
        }
        // 列表空内容的三种提示：搜索进行中 / 搜索出错 / 结果为空。
        if self.resource_search_receiver.is_some() {
            theme::center_text(
                &list_painter,
                Pos2::new(list_panel.center().x, list_top + 70.0),
                "正在自动选择 Modrinth 线路并搜索…",
                9.0,
                theme::muted(),
                false,
            );
        } else if let Some(error) = self.resource_search_error.as_ref() {
            // 错误文案用偏橙色以引起注意，并按宽度省略。
            theme::center_text(
                &list_painter,
                Pos2::new(list_panel.center().x, list_top + 70.0),
                fit_text_to_width(
                    &list_painter,
                    error,
                    theme::font(8.0),
                    (list_panel.width() - 40.0).max(120.0),
                ),
                8.0,
                Color32::from_rgb(208, 172, 102),
                false,
            );
        } else if self.resource_results.is_empty() {
            theme::center_text(
                &list_painter,
                Pos2::new(list_panel.center().x, list_top + 70.0),
                format!("没有找到符合条件的{}", category.label()),
                9.0,
                theme::muted(),
                false,
            );
        }

        // 若存在上一个详情（切换中），先绘制离场的旧详情再绘制进入的新详情。
        let detail_motion = if let Some(previous_index) = self.previous_resource_detail {
            let outgoing = CardMotion {
                screen: motion.screen,
                direction: self.resource_detail_direction,
                progress: self.resource_detail_progress,
                outgoing: true,
            };
            if let Some(previous) = self.resource_results.get(previous_index).cloned() {
                self.paint_resource_detail(
                    ui,
                    painter,
                    source_detail,
                    time,
                    ResourceDetailView {
                        project: Some(previous),
                        motion: outgoing,
                        active: false,
                    },
                );
            }
            CardMotion {
                screen: motion.screen,
                direction: self.resource_detail_direction,
                progress: self.resource_detail_progress,
                outgoing: false,
            }
        } else {
            // 无切换时复用页面整体 motion。
            motion
        };
        // 绘制右侧详情（取当前选中结果，可能是 None 时显示"等待选择"）。
        self.paint_resource_detail(
            ui,
            painter,
            source_detail,
            time,
            ResourceDetailView {
                project: self.resource_results.get(self.resource_selected).cloned(),
                motion: detail_motion,
                active: true,
            },
        );
        // 若版本筛选菜单处于打开状态，绘制该弹出菜单（覆盖在最上层）。
        self.paint_resource_version_menu(ui.ctx(), time);
    }

    /// 绘制社区资源详情卡片。
    ///
    /// 由 `paint_resource_download` 调用，渲染右侧详情区：顶部标题（资源名）、
    /// 信息卡片（作者、简介、下载量与分类），以及右下角的"下载"按钮。点击下载会
    /// 弹出系统文件保存对话框，并把下载任务交给后台线程。
    ///
    /// # 参数
    /// - `ui`：egui UI 句柄，用于注册下载按钮交互与文件选择对话框。
    /// - `painter`：当前层级绘制器。
    /// - `source_detail`：右侧详情区域的矩形。
    /// - `time`：当前时间（秒），用于动画着色。
    /// - `view`：详情视图数据，含 `project`（可空，None 时显示占位）、`motion`（转场）、
    ///   `active`（是否为当前活动视图，非活动时不响应下载点击）。
    ///
    /// # 副作用
    /// 点击下载可能创建下载目录、弹出保存文件对话框、记录用户选择的目录，并启动后台
    /// 下载接收器（`resource_download_receiver`）。
    pub(in crate::app) fn paint_resource_detail(
        &mut self,
        ui: &mut egui::Ui,
        painter: &Painter,
        source_detail: Rect,
        time: f64,
        view: ResourceDetailView,
    ) {
        // 当前资源分类为空时无法绘制详情，直接返回。
        let Some(category) = self.download_resource_category else {
            return;
        };
        // 取出本次详情的转场动画状态（方向/进度/是否离场）。
        let motion = view.motion;
        // 保存详情区基础绘制器（下载按钮/页脚使用，不受卡片裁剪影响）。
        let detail_base = painter;
        // 详情标题条（高 76），应用转场得到实际位置。
        let detail_heading = motion.rect(Rect::from_min_size(
            source_detail.left_top(),
            Vec2::new(source_detail.width(), 76.0),
        ));
        // 绘制详情标题条的玻璃背景面板。
        theme::glass_panel(
            painter,
            detail_heading,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(20, 32, 24), theme::GREEN, time, 32.5),
        );
        // 裁剪出标题条内部绘制器。
        let detail_heading_content = card_clip(painter, detail_heading);
        theme::text(
            &detail_heading_content,
            Pos2::new(detail_heading.left() + 22.0, detail_heading.top() + 12.0),
            format!("{}详情", category.label()),
            8.0,
            theme::muted(),
        );
        // 大标题：显示资源标题（过长省略）；无选中项目时显示"等待选择项目"。
        theme::bold_text(
            &detail_heading_content,
            Pos2::new(detail_heading.left() + 22.0, detail_heading.top() + 31.0),
            view.project
                .as_ref()
                .map(|project| {
                    fit_text_to_width(
                        &detail_heading_content,
                        &project.title,
                        theme::bold_font(19.0),
                        detail_heading.width() - 44.0,
                    )
                })
                .unwrap_or_else(|| "等待选择项目".to_owned()),
            19.0,
            theme::ink(),
        );
        // 信息卡片矩形：位于详情标题条下方 90px，高 220，应用转场得到实际位置。
        let detail_card = motion.rect(Rect::from_min_size(
            Pos2::new(source_detail.left(), source_detail.top() + 90.0),
            Vec2::new(source_detail.width(), 220.0),
        ));
        // 绘制信息卡片的玻璃背景面板。
        theme::glass_panel(
            painter,
            detail_card,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(21, 34, 25), theme::GREEN, time, 33.0),
        );
        // 裁剪出卡片内部绘制器。
        let detail_content = card_clip(painter, detail_card);
        // 仅在选中了具体资源时填充详情卡片内容。
        if let Some(project) = view.project.as_ref() {
            let info_x = detail_card.left() + 24.0;
            // 作者行（绿色高亮）。
            theme::text(
                &detail_content,
                Pos2::new(info_x, detail_card.top() + 24.0),
                format!("作者 {}", project.author),
                8.2,
                theme::GREEN_BRIGHT,
            );
            // 资源简介（自动换行，限制宽度）。
            theme::wrapped_text(
                &detail_content,
                Pos2::new(info_x, detail_card.top() + 50.0),
                &project.description,
                9.0,
                theme::ink_soft(),
                detail_card.width() - 48.0,
            );
            // 底部信息行：下载次数、适用版本数、前 4 个分类（用 " / " 连接）。
            theme::text(
                &detail_content,
                Pos2::new(info_x, detail_card.top() + 145.0),
                fit_text_to_width(
                    &detail_content,
                    &format!(
                        "{} 次下载 · {} 个适用版本 · {}",
                        compact_count(project.downloads),
                        project.game_versions.len(),
                        project
                            .categories
                            .iter()
                            .take(4)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(" / ")
                    ),
                    theme::font(7.8),
                    detail_card.width() - 48.0,
                ),
                7.8,
                theme::muted(),
            );
        }
        // "下载"按钮矩形：详情区右下角（132×38），应用转场得到实际位置。
        let download = motion.rect(Rect::from_min_size(
            Pos2::new(source_detail.right() - 150.0, source_detail.bottom() - 46.0),
            Vec2::new(132.0, 38.0),
        ));
        // 是否正在下载（后台接收器存在即表示进行中）。
        let downloading = self.resource_download_receiver.is_some();
        // 下载按钮文案：下载中显示进度名（省略过长），否则显示"下载{分类}"。
        let label = if downloading {
            self.resource_download_name
                .as_deref()
                .map(|name| format!("正在下载 {}", ellipsize_end(name, 14)))
                .unwrap_or_else(|| "正在下载…".to_owned())
        } else {
            format!("下载{}", category.label())
        };
        // 按按钮宽度省略文案，避免溢出。
        let label = fit_text_to_width(
            detail_base,
            &label,
            theme::bold_font(9.0),
            download.width() - 22.0,
        );
        // 仅当为活动视图、点击了下载按钮、未在下载中、且确有选中项目时，才启动下载。
        if view.active
            && settings_button(ui, detail_base, download, "resource-download", &label, true)
            && !downloading
            && let Some(project) = view.project
        {
            // 确定适用的加载器家族：仅 Mod/整合包 且当前非原版时，传小写家族名；
            // 其他情况（如资源包/光影）不需要加载器，传 None。
            let loader = if matches!(category, ResourceCategory::Mod | ResourceCategory::Modpack)
                && self.version().loader_family != "原版"
            {
                Some(self.version().loader_family.to_lowercase())
            } else {
                None
            };
            // 取得该分类在对应版本下的默认保存目录，存在则继续。
            if let Some(default_directory) =
                self.resource_default_directory(category, &self.resource_version_filter)
            {
                // 确保默认目录存在（忽略已存在/创建失败的错误）。
                let _ = fs::create_dir_all(&default_directory);
                // 文件扩展名：Mod 为 jar，其余（整合包等）为 zip。
                let extension = if category == ResourceCategory::Mod {
                    "jar"
                } else {
                    "zip"
                };
                // 根据项目名与版本构造默认文件名并清洗（去掉非法字符）。
                let mut default_name = crate::resources::sanitize_download_filename(&format!(
                    "{}-{}",
                    project.title, self.resource_version_filter
                ));
                // Mod 文件名中把 '~' 替换为 '-'（'~' 在部分路径处理中有歧义）。
                if category == ResourceCategory::Mod {
                    default_name = default_name.replace('~', "-");
                }
                let default_path = default_directory.join(format!("{default_name}.{extension}"));
                // 弹出系统"保存文件"对话框，让用户选择实际保存位置与文件名。
                if let Some(destination_file) = pick_save_file(
                    &default_path,
                    &format!("保存{}", category.label()),
                    extension,
                    &format!("{}文件", category.label()),
                ) {
                    // 记录用户选择的目录（下次同分类复用），以父目录为准。
                    let destination_directory = destination_file
                        .parent()
                        .unwrap_or(&default_directory)
                        .to_path_buf();
                    self.resource_download_directories
                        .insert(category, destination_directory.clone());
                    // 记录正在下载的资源名，并启动后台下载任务（返回接收器轮询进度）。
                    self.resource_download_name = Some(project.title.clone());
                    self.resource_download_receiver = Some(crate::resources::start_download(
                        project,
                        category,
                        self.resource_version_filter.clone(),
                        loader,
                        destination_file,
                    ));
                }
            } else {
                // 找不到可用游戏目录时提示用户。
                self.show_toast("当前没有可用的游戏目录");
            }
        }
        // 详情底部页脚：提示默认保存目录（取不到目录时回退到分类的目标文件夹名）。
        let footer = motion.rect(Rect::from_min_size(
            Pos2::new(source_detail.left() + 4.0, source_detail.bottom() - 36.0),
            Vec2::new(source_detail.width() - 160.0, 20.0),
        ));
        let footer_text = format!(
            "默认 {} · 下载前可选择其他位置和文件名",
            self.resource_default_directory(category, &self.resource_version_filter)
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| category.target_folder().to_owned())
        );
        theme::text(
            detail_base,
            footer.left_top(),
            fit_text_to_width(detail_base, &footer_text, theme::font(8.0), footer.width()),
            8.0,
            theme::muted(),
        );
    }

    /// 绘制资源版本筛选弹出菜单。
    ///
    /// 当 `resource_version_menu_open` 为 true 且锚点有效时，在版本筛选按钮下方
    /// 以 egui `Area`（前景层）弹出菜单，列出可筛选的 Minecraft 版本。用户选择一个
    /// 版本后会更新 `resource_version_filter` 并重新发起资源搜索。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文，用于创建独立的 `Area` 弹窗、读取输入与请求重绘。
    /// - `time`：当前时间（秒），用于菜单玻璃面板的动画着色。
    ///
    /// # 副作用
    /// 选中版本会设置 `resource_version_filter`、关闭菜单、重置滚动并发起搜索。
    pub(in crate::app) fn paint_resource_version_menu(&mut self, ctx: &egui::Context, time: f64) {
        // 菜单未打开或锚点无效（Rect::NOTHING 表示未定位）时直接返回。
        if !self.resource_version_menu_open || self.resource_version_anchor == Rect::NOTHING {
            return;
        }
        // 取得内容矩形与菜单应放置的矩形（位于锚点按钮下方）。
        let screen = ctx.content_rect();
        let menu = resource_version_menu_rect(self.resource_version_anchor, screen);
        // 由官方版本列表构造可选版本列表（id, kind）。
        let mut versions = self
            .official_versions
            .iter()
            .map(|version| (version.id.clone(), version.kind.clone()))
            .collect::<Vec<_>>();
        // 若列表为空，或当前筛选版本不在列表中，则把当前筛选版本作为首项插入
        // （确保总是能选回当前版本，避免菜单里没有选中项）。
        if versions.is_empty()
            || !versions
                .iter()
                .any(|(id, _)| id == &self.resource_version_filter)
        {
            versions.insert(
                0,
                (self.resource_version_filter.clone(), "release".to_owned()),
            );
        }
        // 按版本 id 去重，避免官方列表里有重复项时显示多遍。
        versions.dedup_by(|left, right| left.0 == right.0);

        // 创建独立的前景层 Area 弹窗：固定在锚点下方、不可拖动。
        egui::Area::new(Id::unique("resource-version-menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(menu.min)
            .movable(false)
            .show(ctx, |ui| {
                // 限制弹窗内容区大小为预先计算好的菜单矩形。
                ui.set_min_size(menu.size());
                // 克隆 UI 绘制器用于绘制玻璃面板。
                let painter = ui.painter().clone();
                // 菜单玻璃背景：附加 16px 背景模糊，增强弹窗层次感。
                theme::glass_panel(
                    &painter,
                    menu,
                    theme::corner_radius(),
                    theme::GlassSpec::new(Color32::from_rgb(20, 32, 24), theme::GREEN, time, 34.2)
                        .backdrop_blur(16.0),
                );
                // 裁剪出菜单内部绘制器，限制内容在圆角面板内。
                let content = card_clip(&painter, menu);
                // 菜单标题。
                theme::bold_text(
                    &content,
                    Pos2::new(menu.left() + 16.0, menu.top() + 13.0),
                    "选择 Minecraft 版本",
                    9.2,
                    theme::ink(),
                );
                // 菜单副标题。
                theme::text(
                    &content,
                    Pos2::new(menu.left() + 16.0, menu.top() + 34.0),
                    "筛选适用于指定游戏版本的资源",
                    7.4,
                    theme::muted(),
                );

                // 版本列表区域：菜单内部四周各留 8px，顶部避开标题区。
                let list = Rect::from_min_max(
                    Pos2::new(menu.left() + 8.0, menu.top() + 57.0),
                    Pos2::new(menu.right() - 8.0, menu.bottom() - 8.0),
                );
                // 列表行高 38px。
                let row_height = 38.0;
                // 最大滚动距离（内容超出可视区时为正）。
                let max_scroll = (versions.len() as f32 * row_height - list.height()).max(0.0);
                // 鼠标悬停在列表内且存在平滑滚动增量时，累加滚动偏移。
                if ui.ctx().input(|input| {
                    input
                        .pointer
                        .hover_pos()
                        .is_some_and(|pointer| list.contains(pointer))
                        && input.smooth_scroll_delta.y.abs() > 0.0
                }) {
                    self.resource_version_scroll -=
                        ui.ctx().input(|input| input.smooth_scroll_delta.y);
                }
                // 夹紧滚动范围到 [0, max_scroll]。
                self.resource_version_scroll = self.resource_version_scroll.clamp(0.0, max_scroll);
                // 限制列表绘制在可视范围内，超出部分不显示。
                let list_painter = content.with_clip_rect(list);
                // 记录本次点选的版本（循环外统一处理，避免在绘制循环中修改状态）。
                let mut selected_version = None;
                // 遍历版本列表，绘制每一行并处理点击。
                for (index, (id, kind)) in versions.iter().enumerate() {
                    // 该行实际 Y：列表起点 + 序号×行高 - 滚动偏移。
                    let y = list.top() + index as f32 * row_height - self.resource_version_scroll;
                    // 视口裁剪：整行在可视区外则跳过绘制。
                    if y + row_height < list.top() || y > list.bottom() {
                        continue;
                    }
                    // 行矩形：占满列表宽度，高为行高减 4px 间距。
                    let row = Rect::from_min_size(
                        Pos2::new(list.left(), y),
                        Vec2::new(list.width(), row_height - 4.0),
                    );
                    // 注册行点击交互与手指光标（以版本 id 保证 Id 唯一）。
                    let response = ui
                        .interact(
                            row,
                            Id::unique(("resource-version-option", id)),
                            Sense::click(),
                        )
                        .on_hover_cursor(CursorIcon::PointingHand);
                    // 是否为当前生效的筛选版本。
                    let selected = id == &self.resource_version_filter;
                    // 裁剪出行内部绘制器。
                    let row_content = card_clip(&list_painter, row);
                    // 行背景：选中为绿色高亮，悬停次亮，否则极淡。
                    theme::fill_round(
                        &row_content,
                        row,
                        theme::corner_radius(),
                        if selected {
                            theme::alpha(theme::GREEN, 26)
                        } else if response.hovered() {
                            theme::white_alpha(11)
                        } else {
                            theme::white_alpha(4)
                        },
                    );
                    // 行左侧：版本号（加粗），选中时用主色。
                    theme::bold_text(
                        &row_content,
                        Pos2::new(row.left() + 13.0, row.top() + 5.0),
                        id,
                        9.0,
                        if selected {
                            theme::ink()
                        } else {
                            theme::ink_soft()
                        },
                    );
                    // 行右侧：版本类型描述（如"正式版"/"快照"），选中时用绿色。
                    theme::text(
                        &row_content,
                        Pos2::new(row.right() - 54.0, row.top() + 8.0),
                        download_version_kind(kind),
                        7.2,
                        if selected {
                            theme::GREEN_BRIGHT
                        } else {
                            theme::muted()
                        },
                    );
                    // 记录被点击的版本 id。
                    if response.clicked() {
                        selected_version = Some(id.clone());
                    }
                }
                // 有点选时：更新筛选版本、关闭菜单、重置滚动，并以新版本重新发起搜索。
                if let Some(version) = selected_version {
                    self.resource_version_filter = version;
                    self.resource_version_menu_open = false;
                    self.resource_version_scroll = 0.0;
                    self.start_resource_search();
                    ctx.request_repaint();
                }
            });
    }

    /// 绘制原版（Mojang 官方）版本的详情面板。
    ///
    /// 由 `paint_download` 在渲染官方版本列表后调用（也在详情切换动画中用于绘制
    /// 离场的旧详情）。右侧详情区自上而下包含：
    /// - 顶部标题条：显示"下载详情"与当前版本号；
    /// - 信息卡片：版本类型（正式版/快照）、下载来源（Mojang Piston Meta）
    ///   与安装流程说明；
    /// - 可折叠的"模组加载器"卡片：可在原版/Fabric/Forge/NeoForge 之间选择，
    ///   选择加载器后列出其适用于当前 Minecraft 版本的版本号；
    /// - 右下角"下载并安装"按钮与左下角安全提示文案。
    ///
    /// # 参数
    /// - `ui`：egui UI 句柄，用于注册加载器折叠、选项、版本列表与安装按钮的交互。
    /// - `painter`：当前层级绘制器。
    /// - `page`：内容区矩形（已由上层完成转场位移）。
    /// - `time`：当前时间（秒），用于玻璃面板动画着色。
    /// - `view`：详情视图数据，含 `version`（当前版本）、`motion`（转场动画）、
    ///   `active`（是否为活动视图；非活动视图仅绘制信息卡片，不响应交互）。
    ///
    /// # 副作用
    /// 可能修改加载器选择相关状态（`loader_kind`、`loader_version_selected`、
    /// `download_loader_open` 等）、通过 `choose_loader` 触发加载器版本列表拉取，
    /// 以及通过 `start_official_install` 启动官方版本安装任务。
    pub(in crate::app) fn paint_download_detail(
        &mut self,
        ui: &mut egui::Ui,
        painter: &Painter,
        page: Rect,
        time: f64,
        view: DownloadDetailView,
    ) {
        // 左侧版本列表宽度（与 paint_download 中一致）：页面宽 44%，限定在 390~540px。
        // 详情区从列表右侧开始，因此先算出列表宽度以确定详情区左边界。
        let list_width = (page.width() * 0.44).clamp(390.0, 540.0);
        // 右侧详情区矩形：列表右缘 + 14px 间距，直到页面右下角。
        let source_detail = Rect::from_min_max(
            Pos2::new(page.left() + list_width + 14.0, page.top()),
            page.right_bottom(),
        );
        // 克隆绘制器并改用引用：后续会派生多个子绘制器（裁剪等），
        // 克隆可避免与调用方传入的 painter 产生借用冲突。
        let painter = painter.clone();
        let painter = &painter;
        // 取出本次详情的转场动画状态（方向/进度/是否离场）。
        let motion = view.motion;
        // 详情标题条矩形（高 76），应用转场得到实际绘制位置。
        let detail_heading = motion.rect(Rect::from_min_size(
            source_detail.left_top(),
            Vec2::new(source_detail.width(), 76.0),
        ));
        // 保存详情区基准绘制器（信息卡片/加载器卡片/安装按钮均以它为底）。
        let detail_base = painter;
        // 绘制标题条的玻璃背景面板。
        theme::glass_panel(
            painter,
            detail_heading,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(20, 32, 24), theme::GREEN, time, 29.0),
        );
        // 裁剪出标题条内部绘制器，限制文字在圆角面板内。
        let detail_heading_content = card_clip(painter, detail_heading);
        let painter = &detail_heading_content;
        // 小标题"下载详情"。
        theme::text(
            painter,
            Pos2::new(detail_heading.left() + 22.0, detail_heading.top() + 12.0),
            "下载详情",
            8.0,
            theme::muted(),
        );
        // 大标题：当前选中版本的版本号（如 "1.20.4"）。
        theme::bold_text(
            painter,
            Pos2::new(detail_heading.left() + 22.0, detail_heading.top() + 30.0),
            &view.version.id,
            20.0,
            theme::ink(),
        );

        // 信息卡片矩形：位于标题条下方 90px，高 198，应用转场得到实际位置。
        let detail_card = motion.rect(Rect::from_min_size(
            Pos2::new(source_detail.left(), source_detail.top() + 90.0),
            Vec2::new(source_detail.width(), 198.0),
        ));
        // 恢复详情基准绘制器，用于绘制信息卡片。
        let painter = detail_base;
        // 绘制信息卡片的玻璃背景面板。
        theme::glass_panel(
            painter,
            detail_card,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(21, 34, 25), theme::GREEN, time, 29.8),
        );
        // 裁剪出卡片内部绘制器，限制后续内容在圆角范围内。
        let detail_card_content = card_clip(painter, detail_card);
        let painter = &detail_card_content;
        // 版本图标中心：卡片左侧 68px、垂直居中。
        let icon_center = Pos2::new(detail_card.left() + 68.0, detail_card.center().y);
        // 在图标后方绘制一团柔和的绿色光晕，增强立体感（向下偏移 12px）。
        theme::soft_orb(
            painter,
            icon_center + Vec2::new(0.0, 12.0),
            64.0,
            theme::GREEN,
            13,
        );
        // 版本图标矩形：72×72，以 icon_center 为中心。
        let icon = Rect::from_center_size(icon_center, Vec2::splat(72.0));
        // 按版本类型挑选图标纹理：快照版用 snapshot 图标，正式版用 release 图标。
        let icon_texture = if view.version.kind == "snapshot" {
            self.snapshot_icon_texture.as_ref()
        } else {
            self.release_icon_texture.as_ref()
        };
        // 有纹理则绘制圆角图标；否则绘制程序生成的方块占位图
        // （草方块风格：绿色顶面 + 土色侧面）。
        if let Some(texture) = icon_texture {
            paint_rounded_image(painter, icon, 12.0, texture, texture.size());
        } else {
            paint_block(
                painter,
                icon.center(),
                72.0,
                Color32::from_rgb(125, 216, 106),
                Color32::from_rgb(87, 65, 46),
                Color32::from_rgb(69, 148, 70),
            );
        }
        // 信息文字列的 X 坐标：位于图标右侧（卡片左缘 + 128px）。
        let info_x = detail_card.left() + 128.0;
        // 信息行 1：版本类型标签与值（"正式版"/"快照"等，由 download_version_kind 翻译）。
        theme::text(
            painter,
            Pos2::new(info_x, detail_card.top() + 24.0),
            "版本类型",
            8.0,
            theme::muted(),
        );
        theme::bold_text(
            painter,
            Pos2::new(info_x, detail_card.top() + 40.0),
            download_version_kind(&view.version.kind),
            12.0,
            theme::ink_soft(),
        );
        // 信息行 2：下载来源标签与值（官方版本固定来自 Mojang 的版本清单服务）。
        theme::text(
            painter,
            Pos2::new(info_x, detail_card.top() + 76.0),
            "下载来源",
            8.0,
            theme::muted(),
        );
        theme::bold_text(
            painter,
            Pos2::new(info_x, detail_card.top() + 92.0),
            "Mojang Piston Meta",
            11.0,
            theme::ink_soft(),
        );
        // 信息行 3：安装流程说明（提示用户可继续选择模组加载器）。
        theme::text(
            painter,
            Pos2::new(info_x, detail_card.top() + 128.0),
            "安装流程",
            8.0,
            theme::muted(),
        );
        theme::text(
            painter,
            Pos2::new(info_x, detail_card.top() + 145.0),
            "下载原版文件后，可选择 Fabric、Forge 或 NeoForge",
            8.5,
            Color32::from_rgb(176, 194, 178),
        );

        // 非活动视图（如转场中的旧详情）只绘制到信息卡片为止，
        // 不绘制加载器卡片与安装按钮，避免转场期间出现重复交互。
        if !view.active {
            return;
        }

        // 加载器卡片的展开进度：允许短暂过冲到 1.08 再回落，动画更自然。
        let loader_progress = self.download_loader_value.clamp(0.0, 1.08);
        // 对进度做 smoothstep 缓动（3t²-2t³），使展开/收起先加速后减速。
        let loader_eased = loader_progress.clamp(0.0, 1.0);
        let loader_eased = loader_eased * loader_eased * (3.0 - 2.0 * loader_eased);
        // 卡片高度随缓动进度从收起时的 94px 展开到 342px。
        let loader_height = 94.0 + (342.0 - 94.0) * loader_eased;
        // 加载器卡片矩形：位于详情区顶部下方 298px，高度由展开进度决定。
        let loader_card = motion.rect(Rect::from_min_size(
            Pos2::new(source_detail.left(), source_detail.top() + 298.0),
            Vec2::new(source_detail.width(), loader_height),
        ));
        // 恢复详情基准绘制器，绘制加载器卡片。
        let painter = detail_base;
        // 绘制加载器卡片的玻璃背景面板。
        theme::glass_panel(
            painter,
            loader_card,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(21, 34, 25), theme::GREEN, time, 30.1),
        );
        // 裁剪出卡片内部绘制器。
        let loader_content = card_clip(painter, loader_card);
        let painter = &loader_content;
        // 卡片头部区域（高 58）：整块可点击，用于展开/收起加载器选项。
        let loader_header =
            Rect::from_min_size(loader_card.left_top(), Vec2::new(loader_card.width(), 58.0));
        // 注册头部点击交互与手指光标。
        let loader_response = ui
            .interact(
                loader_header,
                Id::unique("download-loader-toggle"),
                Sense::click(),
            )
            .on_hover_cursor(CursorIcon::PointingHand);
        // 头部左侧的加载器图标（28×28）。
        let loader_icon = Rect::from_center_size(
            Pos2::new(loader_card.left() + 22.0, loader_card.top() + 29.0),
            Vec2::splat(28.0),
        );
        // 图标纹理已加载则绘制圆角图标，否则留空。
        if let Some(texture) = self.loader_icon_texture.as_ref() {
            paint_rounded_image(painter, loader_icon, 8.0, texture, texture.size());
        }
        // 头部小标题"模组加载器"。
        theme::text(
            painter,
            Pos2::new(loader_card.left() + 42.0, loader_card.top() + 11.0),
            "模组加载器",
            8.0,
            theme::muted(),
        );
        // 头部主文字：已选加载器时显示如 "Fabric · Minecraft 1.20.4"，
        // 未选时提示"原版 / 不使用加载器"。
        theme::bold_text(
            painter,
            Pos2::new(loader_card.left() + 42.0, loader_card.top() + 26.0),
            self.loader_kind
                .map(|kind| format!("{} · Minecraft {}", kind.label(), self.loader_base_version))
                .unwrap_or_else(|| "原版 / 不使用加载器".to_owned()),
            12.0,
            theme::ink_soft(),
        );
        // 头部右侧的折叠箭头：由两段线段拼成 V 形。
        let arrow_center = Pos2::new(loader_card.right() - 28.0, loader_card.top() + 29.0);
        // 箭头端点的 Y 偏移随展开进度变化：收起时朝下（+4），展开时朝上（-4）。
        let arrow_y = 4.0 - 8.0 * loader_eased;
        // 绘制箭头的左半段与右半段（顶点均为 arrow_center）。
        painter.line_segment(
            [arrow_center + Vec2::new(-5.0, arrow_y), arrow_center],
            Stroke::new(1.6, theme::ink_soft()),
        );
        painter.line_segment(
            [arrow_center, arrow_center + Vec2::new(5.0, arrow_y)],
            Stroke::new(1.6, theme::ink_soft()),
        );
        // 点击头部：切换加载器选项区的展开/收起状态（动画由 download_loader_value 驱动）。
        if loader_response.clicked() {
            self.download_loader_open = !self.download_loader_open;
        }

        // 展开进度超过 2% 后才绘制加载器类型选项，避免收起状态下残留内容。
        if loader_progress > 0.02 {
            // 克隆绘制器供选项区独立使用。
            let painter = painter.clone();
            let painter = &painter;
            // 加载器类型选项：None 表示保持原版，其余为三种可选加载器。
            let options = [
                (None, "原版"),
                (Some(LoaderKind::Fabric), "Fabric"),
                (Some(LoaderKind::Forge), "Forge"),
                (Some(LoaderKind::NeoForge), "NeoForge"),
            ];
            // 选项按钮的起始 X：卡片左缘 + 20px，之后依次向右排布。
            let mut option_x = loader_card.left() + 20.0;
            // 遍历每个加载器选项，绘制按钮并处理点击。
            for (index, (kind, label)) in options.into_iter().enumerate() {
                // "原版"按钮较窄（64px），加载器按钮宽 78px。
                let width = if index == 0 { 64.0 } else { 78.0 };
                // 选项按钮矩形：位于卡片顶部下方 62px，高 30。
                let option = Rect::from_min_size(
                    Pos2::new(option_x, loader_card.top() + 62.0),
                    Vec2::new(width, 30.0),
                );
                // 注册点击交互与手指光标（用 index 保证 Id 唯一）。
                let response = ui
                    .interact(
                        option,
                        Id::unique(("download-loader-kind", index)),
                        Sense::click(),
                    )
                    .on_hover_cursor(CursorIcon::PointingHand);
                // 是否为当前选中的加载器类型。
                let selected = self.loader_kind == kind;
                // 绘制按钮背景：选中为绿色高亮，悬停次亮，否则极淡。
                theme::fill_round(
                    painter,
                    option,
                    theme::corner_radius(),
                    if selected {
                        theme::alpha(theme::GREEN, 28)
                    } else if response.hovered() {
                        theme::white_alpha(13)
                    } else {
                        theme::white_alpha(7)
                    },
                );
                // 居中绘制选项文字，选中时用主色并加粗。
                theme::center_text(
                    painter,
                    option.center(),
                    label,
                    8.2,
                    if selected {
                        theme::ink()
                    } else {
                        theme::muted()
                    },
                    selected,
                );
                // 点击未选中的选项时切换加载器：
                // - 选择具体加载器：调用 choose_loader 发起版本列表拉取，并保持展开；
                // - 切回"原版"：清空全部加载器相关状态（拉取接收器/版本列表/错误/选中项/滚动）。
                if response.clicked() && !selected {
                    if let Some(kind) = kind {
                        self.choose_loader(kind);
                        self.download_loader_open = true;
                    } else {
                        self.loader_kind = None;
                        self.loader_versions_receiver = None;
                        self.loader_versions.clear();
                        self.loader_versions_error = None;
                        self.loader_version_selected = 0;
                        self.loader_version_scroll = 0.0;
                    }
                }
                // 下一个按钮起点：当前按钮右缘 + 8px 间距。
                option_x = option.right() + 8.0;
            }

            // 已选择具体加载器时，显示该加载器的可用版本列表。
            if let Some(kind) = self.loader_kind {
                // 版本列表区域：卡片内部左右各留 20px，顶部下方 102px。
                let list = Rect::from_min_max(
                    Pos2::new(loader_card.left() + 20.0, loader_card.top() + 102.0),
                    Pos2::new(loader_card.right() - 20.0, loader_card.bottom() - 12.0),
                );
                // 列表行高 34px。
                let row_height = 34.0;
                // 内容总高度与最大滚动距离（内容超出可视区时为正）。
                let content_height = self.loader_versions.len() as f32 * row_height;
                let max_scroll = (content_height - list.height()).max(0.0);
                // 鼠标悬停在列表内且存在平滑滚动增量时，按 1.0 倍系数累加滚动偏移。
                if ui.ctx().input(|input| {
                    input
                        .pointer
                        .hover_pos()
                        .is_some_and(|pointer| list.contains(pointer))
                        && input.smooth_scroll_delta.y.abs() > 0.0
                }) {
                    self.loader_version_scroll -=
                        ui.ctx().input(|input| input.smooth_scroll_delta.y) * 1.0;
                }
                // 夹紧滚动范围到 [0, max_scroll]，避免越界。
                self.loader_version_scroll = self.loader_version_scroll.clamp(0.0, max_scroll);
                // 克隆绘制器并限制在列表可视范围内，超出部分不显示。
                let list_painter = painter.clone();
                // 拉取中：显示"正在获取 XX 版本…"。
                if self.loader_versions_receiver.is_some() {
                    theme::center_text(
                        &list_painter,
                        list.center(),
                        format!("正在获取 {} 版本…", kind.label()),
                        8.5,
                        theme::muted(),
                        false,
                    );
                } else if let Some(error) = &self.loader_versions_error {
                    // 拉取失败：显示错误信息（过长省略），用偏橙色提示。
                    theme::center_text(
                        &list_painter,
                        list.center(),
                        ellipsize_end(error, 64),
                        8.0,
                        Color32::from_rgb(207, 170, 103),
                        false,
                    );
                } else if self.loader_versions.is_empty() {
                    // 拉取成功但为空：提示该加载器没有适配当前 MC 版本的版本。
                    theme::center_text(
                        &list_painter,
                        list.center(),
                        format!(
                            "{} 没有适用于 Minecraft {} 的版本",
                            kind.label(),
                            self.loader_base_version
                        ),
                        8.2,
                        Color32::from_rgb(207, 170, 103),
                        false,
                    );
                } else {
                    // 遍历加载器版本列表，绘制每一行并处理选中。
                    for (index, version) in self.loader_versions.iter().enumerate() {
                        // 该行实际 Y：列表起点 + 序号×行高 - 滚动偏移。
                        let y = list.top() + index as f32 * row_height - self.loader_version_scroll;
                        // 视口裁剪：整行在可视区外则跳过绘制。
                        if y + row_height < list.top() || y > list.bottom() {
                            continue;
                        }
                        // 行矩形：占满列表宽度，高为行高减 4px 间距。
                        let row = Rect::from_min_size(
                            Pos2::new(list.left(), y),
                            Vec2::new(list.width(), row_height - 4.0),
                        );
                        // 注册行点击交互与手指光标（用 index 保证 Id 唯一）。
                        let response = ui
                            .interact(
                                row,
                                Id::unique(("download-loader-version", index)),
                                Sense::click(),
                            )
                            .on_hover_cursor(CursorIcon::PointingHand);
                        // 是否为当前选中的加载器版本。
                        let selected = self.loader_version_selected == index;
                        // 行背景：选中为绿色高亮，悬停次亮，否则深色底。
                        theme::fill_round(
                            &list_painter,
                            row,
                            theme::corner_radius(),
                            if selected {
                                theme::alpha(theme::GREEN, 24)
                            } else if response.hovered() {
                                theme::white_alpha(10)
                            } else {
                                Color32::from_rgb(18, 29, 21)
                            },
                        );
                        // 行文字：该加载器版本的显示名（由 loader_display_version 格式化）。
                        theme::text(
                            &list_painter,
                            Pos2::new(row.left() + 14.0, row.top() + 7.0),
                            loader_display_version(&self.loader_base_version, version),
                            8.5,
                            if selected {
                                theme::ink()
                            } else {
                                theme::ink_soft()
                            },
                        );
                        // 选中行右侧显示绿色对勾。
                        if selected {
                            theme::text(
                                &list_painter,
                                Pos2::new(row.right() - 18.0, row.top() + 7.0),
                                "✓",
                                9.0,
                                theme::GREEN_BRIGHT,
                            );
                        }
                        // 点击行：更新选中的加载器版本下标。
                        if response.clicked() {
                            self.loader_version_selected = index;
                        }
                    }
                }
            } else {
                // 未选择加载器时显示保持原版的提示文字。
                theme::text(
                    painter,
                    Pos2::new(loader_card.left() + 20.0, loader_card.top() + 104.0),
                    "保持原版安装，不添加模组加载器。",
                    8.5,
                    theme::muted(),
                );
            }
        }

        // 恢复详情基准绘制器，绘制安装按钮与底部提示。
        let painter = detail_base;
        // 组合"已选加载器 + 已选加载器版本"：仅在两者都确定时为 Some。
        let selected_loader_version = self.loader_kind.and_then(|loader| {
            self.loader_versions
                .get(self.loader_version_selected)
                .cloned()
                .map(|version| (loader, version))
        });
        // 是否允许安装：没有进行中的安装任务，且（未选加载器 或 已选定加载器版本）。
        let can_install = self.install_receiver.is_none()
            && (self.loader_kind.is_none() || selected_loader_version.is_some());
        // 按钮文案随状态变化：安装中 / 已选加载器 / 纯原版。
        let install_label = if self.install_receiver.is_some() {
            "正在安装…"
        } else if self.loader_kind.is_some() {
            "下载并安装加载器"
        } else {
            "下载原版"
        };
        // 安装按钮矩形：详情区右下角（132×38），应用转场得到实际位置。
        let install = motion.rect(Rect::from_min_size(
            Pos2::new(source_detail.right() - 150.0, source_detail.bottom() - 46.0),
            Vec2::new(132.0, 38.0),
        ));
        // 点击安装按钮且允许安装时：
        // 先记下待安装的加载器组合（原版下载完成后再安装加载器），
        // 然后启动官方版本安装流程（第二个参数 false 表示非静默安装）。
        if settings_button(
            ui,
            painter,
            install,
            "download-install",
            install_label,
            true,
        ) && can_install
        {
            self.pending_loader_after_base = selected_loader_version;
            self.start_official_install(view.version, false);
        }
        // 按钮下方提示：已选加载器但版本列表为空时提示先选版本，
        // 否则说明仅使用 Mojang 官方下载域名并在安装前校验文件。
        theme::text(
            painter,
            Pos2::new(source_detail.left() + 4.0, source_detail.bottom() - 36.0),
            if self.loader_kind.is_some() && self.loader_versions.is_empty() {
                "请先选择一个加载器版本"
            } else {
                "只使用 Mojang 官方下载域名，并在安装前校验文件"
            },
            8.0,
            theme::muted(),
        );
    }
}
