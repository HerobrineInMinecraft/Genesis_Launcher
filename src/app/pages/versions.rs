//! 版本页面（Versions Page）的 UI 绘制实现。
//!
//! 本文件是 `LauncherApp` 的 `impl` 块之一，专注于“版本”标签页的全部界面渲染与
//! 交互逻辑。它并不包含启动游戏的业务流程，而是把已有的版本数据（`self.versions`）、
//! 设置（`self.settings`）与主题系统（`theme`）组合成可见的卡片、列表、弹层与对话框。
//!
//! 主要定义的公开方法（`pub(in crate::app)`，仅当前 `app` 模块内可见）：
//! - `paint_versions`：版本主页面的整体绘制（左侧版本列表 + 右侧详情）。
//! - `paint_version_folder_menu`：游戏文件夹选择下拉菜单的绘制。
//! - `paint_version_content_dialog`：模组/存档/光影等内容浏览与导出对话框的绘制。
//! - `paint_java_picker`：Java 运行时选择浮层的绘制。
//! - `paint_export_contents`：导出整合包时的选项勾选区绘制（被内容对话框复用）。
//! - `paint_version_export_dialog`：老式整合包导出对话框（当前已标记 dead_code）。
//! - `paint_version_detail`：右侧单个版本详情面板的绘制（含内存/Java/操作入口）。
//!
//! 这些函数大量依赖 `theme`（玻璃面板、文字、图标绘制）、`Layout`（布局几何）、
//! `Painter`/`CardMotion`（动画与裁剪）等模块；它们通过 `self` 上的字段读写状态，
//! 并通过 `ctx.request_repaint()` 等方式触发重绘。所有绘制坐标均为即时模式的
//! egui 几何运算，不涉及持久化。

use super::super::*;

impl LauncherApp {
    /// 绘制“版本”标签页的完整界面。
    ///
    /// 页面从左向右分为两块：左侧是版本列表（含筛选器、文件夹选择、刷新/导入按钮），
    /// 右侧是选中版本的详情面板。列表项可点击切换详情，详情区可调整内存、Java，
    /// 并打开模组/存档/光影等内容对话框或导出对话框。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文，用于请求重绘、读取输入与创建浮层（Area）。
    /// - `ui`：当前界面的 `Ui`，用于交互检测（`interact`）与鼠标光标。
    /// - `painter`：底层绘制器，负责直接绘制图形与文字。
    /// - `layout`：整体布局几何信息（内容区边界、头部高度、屏幕尺寸）。
    /// - `time`：动画用的连续时间（秒），传给 `theme` 以驱动玻璃质感流动效果。
    /// - `motion`：卡片入场/切换动画状态，用于把“源矩形”映射为“当前帧矩形”。
    ///
    /// # 副作用
    /// 读取并可能修改 `self` 上大量 UI 状态（如 `version_filter`、`version_page_scroll`、
    /// `selected_version`、`version_folder_menu_open` 等），点击交互会触发版本切换、
    /// 文件夹菜单、刷新列表、导入整合包等动作。
    ///
    /// # 注意
    /// 当没有匹配当前筛选的版本时，函数会提前 `return` 而不绘制右侧详情。
    pub(in crate::app) fn paint_versions(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
        motion: CardMotion,
    ) {
        // 计算整个页面可用区域：顶部在头部下方 14px，底部离屏幕底 22px
        let page = Rect::from_min_max(
            Pos2::new(layout.content_left, layout.header.bottom() + 14.0),
            Pos2::new(layout.content_right, layout.screen.bottom() - 22.0),
        );
        // 列表区占页面宽度的 49%，并限制在 430~590 的逻辑像素之间，保证可读性
        let list_width = (page.width() * 0.49).clamp(430.0, 590.0);
        let source_list_panel = Rect::from_min_max(
            page.left_top(),
            Pos2::new(page.left() + list_width, page.bottom()),
        );
        // 通过动画把“静止源矩形”转换为当前帧应显示的矩形（处理入场/切换动画）
        let list_panel = motion.rect(source_list_panel);
        // 列表顶部 150px 高的“标题区”（已安装数量、刷新、导入、文件夹选择、筛选器）
        let list_heading =
            Rect::from_min_size(list_panel.left_top(), Vec2::new(list_panel.width(), 150.0));
        let base_painter = painter;
        // 在标题区绘制一块带绿色描边的玻璃面板作为背景
        theme::glass_panel(
            painter,
            list_heading,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(20, 32, 24), theme::GREEN, time, 16.0),
        );
        // 创建一个裁剪区域，后续标题区内容都只能画在该矩形内，防止溢出
        let heading_content = card_clip(painter, list_heading);
        let painter = &heading_content;

        // 已安装版本数：若设置了游戏目录，则统计该目录下已安装版本的数量
        let installed_count = self
            .settings
            .game_directory
            .as_deref()
            .map(crate::game::installed_versions)
            .map(|versions| versions.len())
            .unwrap_or(0);
        // 可启动版本数：遍历内存中的版本列表，统计满足“可启动”条件的条目
        let launchable_count = self
            .versions
            .iter()
            .filter(|version| version.launchable)
            .count();
        // 标题“已安装版本”粗体文字
        theme::bold_text(
            painter,
            Pos2::new(list_heading.left() + 22.0, list_heading.top() + 15.0),
            "已安装版本",
            19.0,
            theme::ink(),
        );
        // 副标题：显示“X 个版本 · Y 个可启动”，过长则尾部省略
        theme::text(
            painter,
            Pos2::new(list_heading.left() + 22.0, list_heading.top() + 48.0),
            ellipsize_end(
                &format!("{installed_count} 个版本 · {launchable_count} 个可启动"),
                42,
            ),
            8.5,
            theme::muted(),
        );
        // 右上角“刷新”按钮的点击区域（距标题区右边缘 91px 起，宽 69 高 30）
        let refresh = Rect::from_min_size(
            Pos2::new(list_heading.right() - 91.0, list_heading.top() + 14.0),
            Vec2::new(69.0, 30.0),
        );
        // “导入整合包”按钮区域，位于刷新按钮左侧（距右边缘 197px）
        let import = Rect::from_min_size(
            Pos2::new(list_heading.right() - 197.0, list_heading.top() + 14.0),
            Vec2::new(98.0, 30.0),
        );
        // 点击“导入整合包”：启动整合包导入流程并请求重绘
        if settings_button(
            ui,
            painter,
            import,
            "version-import-modpack",
            "导入整合包",
            false,
        ) {
            self.start_modpack_import();
            ctx.request_repaint();
        }
        // 点击“刷新”：重新扫描本地版本、同步选中项、刷新官方清单并提示
        if settings_button(ui, painter, refresh, "version-refresh", "刷新", false) {
            // 记住当前激活版本名，刷新后尽量保持选中同一版本
            let active_name = self.version().version.clone();
            // 重新枚举游戏目录下的可用版本
            self.versions = available_versions(self.settings.game_directory.as_deref());
            // 优先按原版本名定位，否则退回到第一个可启动版本，再不行取第 0 项
            self.selected_version = self
                .versions
                .iter()
                .position(|version| version.version == active_name)
                .or_else(|| self.versions.iter().position(|version| version.launchable))
                .unwrap_or(0);
            self.version_page_selected = self.selected_version;
            // 重置列表滚动与内容弹窗状态
            self.version_page_scroll = 0.0;
            // 异步刷新 Mojang 官方版本清单（用于后续安装新版本）
            self.refresh_official_manifest();
            self.show_toast(format!("版本列表已刷新 · {} 个版本", installed_count));
        }

        // 游戏文件夹选择条：位于标题区，左侧 22px 缩进，高 34px
        let folder_selector = Rect::from_min_size(
            Pos2::new(list_heading.left() + 22.0, list_heading.top() + 68.0),
            Vec2::new(list_heading.width() - 44.0, 34.0),
        );
        // 注册点击交互，并把光标设为手型提示可点击
        let folder_response = ui
            .interact(
                folder_selector,
                Id::unique("version-folder-selector"),
                Sense::click(),
            )
            .on_hover_cursor(CursorIcon::PointingHand);
        // 填充选择条背景：悬停或菜单打开时更亮，否则用较暗底色
        theme::fill_round(
            painter,
            folder_selector,
            theme::corner_radius(),
            if folder_response.hovered() || self.version_folder_menu_open {
                Color32::from_rgb(31, 48, 36)
            } else {
                Color32::from_rgb(25, 38, 29)
            },
        );
        // 选择条描边：菜单打开时显示绿色高亮，否则用半透明白边
        theme::stroke_round(
            painter,
            folder_selector,
            theme::corner_radius(),
            1.0,
            if self.version_folder_menu_open {
                theme::alpha(theme::GREEN, 66)
            } else {
                theme::white_alpha(28)
            },
        );
        // 选择条左侧画一个文件夹图标
        draw_icon(
            painter,
            NavIcon::Folder,
            Rect::from_center_size(
                Pos2::new(folder_selector.left() + 16.0, folder_selector.center().y),
                Vec2::splat(13.0),
            ),
            theme::GREEN_BRIGHT,
        );
        // 选择条文字：有游戏目录时显示路径（开头省略过长部分），否则显示提示语
        let folder_text = self
            .settings
            .game_directory
            .as_ref()
            .map(|path| ellipsize_start(&path.display().to_string(), 42))
            .unwrap_or_else(|| "选择 Minecraft 游戏文件夹".to_owned());
        theme::text(
            painter,
            Pos2::new(folder_selector.left() + 29.0, folder_selector.top() + 9.0),
            folder_text,
            8.2,
            theme::ink_soft(),
        );
        // 选择条右侧画一个下箭头（Chevron）图标，表示可展开菜单
        draw_icon(
            painter,
            NavIcon::Chevron,
            Rect::from_center_size(
                Pos2::new(folder_selector.right() - 17.0, folder_selector.center().y),
                Vec2::splat(12.0),
            ),
            theme::muted(),
        );
        // 点击选择条：关闭其他浮层并切换文件夹菜单的显隐状态
        if folder_response.clicked() {
            self.menu = Menu::None;
            self.news = None;
            self.version_folder_menu_open = !self.version_folder_menu_open;
        }

        // 三个筛选标签：全部 / 正式版 / 快照
        let filters = [
            (VersionFilter::All, "全部"),
            (VersionFilter::Release, "正式版"),
            (VersionFilter::Snapshot, "快照"),
        ];
        // 记录当前按钮的 x 坐标，依次向右排布（每个按钮间距 7px）
        let mut filter_x = list_heading.left() + 22.0;
        for (index, (filter, label)) in filters.into_iter().enumerate() {
            // 每个筛选按钮宽 66 高 27，位于标题区 y=114 处
            let button = Rect::from_min_size(
                Pos2::new(filter_x, list_heading.top() + 114.0),
                Vec2::new(66.0, 27.0),
            );
            // 注册点击交互与手型光标
            let response = ui
                .interact(
                    button,
                    Id::unique(("version-filter", index)),
                    Sense::click(),
                )
                .on_hover_cursor(CursorIcon::PointingHand);
            // 判断该筛选是否当前选中
            let selected = self.version_filter == filter;
            // 按钮背景：选中绿色淡底，悬停浅白，否则极淡白底
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
            // 居中绘制标签文字；选中时用主色，否则用灰色
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
            // 点击某个筛选：切换筛选条件并重置详情/滚动状态
            if response.clicked() {
                self.version_filter = filter;
                self.version_page_scroll = 0.0;
                self.previous_version_detail = None;
                self.version_detail_progress = 1.0;
            }
            // 移动到下一个按钮位置（右侧 + 间距）
            filter_x = button.right() + 7.0;
        }

        // 按当前筛选条件过滤出要显示的版本；只保留可启动版本，并按类型细分
        // 返回 (全局索引, 版本副本) 列表，便于后续点击时映射回原始数据
        let filtered: Vec<(usize, GameVersion)> = self
            .versions
            .iter()
            .enumerate()
            .filter(|(_, version)| match self.version_filter {
                // 全部：仅可启动版本
                VersionFilter::All => version.launchable,
                // 正式版：可启动且非快照版本（快照版本号通常以 "snapshot" 等标识）
                VersionFilter::Release => {
                    version.launchable && !is_snapshot_version(&version.version)
                }
                // 快照：可启动且是快照版本
                VersionFilter::Snapshot => {
                    version.launchable && is_snapshot_version(&version.version)
                }
            })
            .map(|(index, version)| (index, version.clone()))
            .collect();
        // 若当前选中的全局索引已不在筛选结果内，则回退到筛选结果的第一个版本
        if !filtered.is_empty()
            && !filtered
                .iter()
                .any(|(index, _)| *index == self.version_page_selected)
        {
            self.version_page_selected = filtered[0].0;
            self.previous_version_detail = None;
            self.version_detail_progress = 1.0;
        }

        // 列表可视区顶部：标题区占 158px（150 标题 + 余量）
        let source_list_top = source_list_panel.top() + 158.0;
        let list_top = list_panel.top() + 158.0;
        // 列表可视区底部：距面板底 13px（留出滚动条空间）
        let list_bottom = list_panel.bottom() - 13.0;
        let list_height = (list_bottom - list_top).max(1.0);
        // 每行高 64px；据此算出内容总高与最大可滚动距离
        let row_height = 64.0;
        let content_height = filtered.len() as f32 * row_height;
        let max_scroll = (content_height - list_height).max(0.0);
        // 在列表区域悬停并滚动滚轮时，按平滑滚动增量调整列表滚动（乘 1.05 略加速）
        // 仅在内容对话框未打开时才响应，避免与对话框滚动冲突
        if self.version_content_dialog.is_none()
            && ui.ctx().input(|input| {
                input
                    .pointer
                    .hover_pos()
                    .is_some_and(|pointer| list_panel.contains(pointer))
                    && input.smooth_scroll_delta.y.abs() > 0.0
            })
        {
            self.version_page_scroll -= ui.ctx().input(|input| input.smooth_scroll_delta.y) * 1.05;
        }
        // 把滚动值钳制在 [0, max_scroll] 范围内，避免越界
        self.version_page_scroll = self.version_page_scroll.clamp(0.0, max_scroll);
        // 恢复为未裁剪的基础 painter，用于绘制列表可视区与滚动条
        let painter = base_painter;
        // 列表可视区矩形（左右留出边距），用于裁剪超出部分实现虚拟滚动
        let list_viewport = Rect::from_min_max(
            Pos2::new(list_panel.left() + 4.0, list_top),
            Pos2::new(list_panel.right() - 10.0, list_bottom),
        );
        // 创建带视口裁剪的列表绘制器：超出可视区的行不会被画出
        let list_painter = painter.with_clip_rect(list_viewport);

        // 遍历筛选后的版本，逐行绘制列表项（采用虚拟滚动：只绘制可视区内的行）
        for (visible_index, (global_index, version)) in filtered.iter().enumerate() {
            // 计算该行在当前滚动偏移下的 y 坐标
            let source_y =
                source_list_top + visible_index as f32 * row_height - self.version_page_scroll;
            // 视口裁剪：若整行在可视区上方或下方，则跳过绘制（提升性能）
            if source_y + row_height < source_list_top
                || source_y > source_list_panel.bottom() - 13.0
            {
                continue;
            }
            // 由动画把静止矩形映射到当前帧位置，行宽比面板窄 14px
            let row = motion.rect(Rect::from_min_size(
                Pos2::new(source_list_panel.left() + 4.0, source_y),
                Vec2::new(source_list_panel.width() - 14.0, row_height - 5.0),
            ));
            // 注册该行点击交互（以全局索引作为唯一 id 区分各行）
            let response = ui
                .interact(
                    row,
                    Id::unique(("page-version", global_index)),
                    Sense::click(),
                )
                .on_hover_cursor(CursorIcon::PointingHand);
            // 判断本行是否为当前选中行
            let selected = self.version_page_selected == *global_index;
            // 行内边距缩小 2px 形成卡片观感
            let card = row.shrink2(Vec2::new(2.0, 2.5));
            // 绘制行卡片玻璃面板：选中或悬停时底色更深
            theme::glass_panel(
                &list_painter,
                card,
                theme::corner_radius(),
                theme::GlassSpec::new(
                    if selected || response.hovered() {
                        Color32::from_rgb(28, 47, 32)
                    } else {
                        Color32::from_rgb(20, 32, 24)
                    },
                    theme::GREEN,
                    time,
                    row.top() * 0.01,
                )
                .hovered(response.hovered()),
            );
            // 在卡片内裁剪，确保内容不溢出圆角
            let row_painter = card_clip(&list_painter, card);
            // 选中或悬停时给卡片加描边（选中为绿色高亮，悬停为白色淡边）
            if selected || response.hovered() {
                theme::stroke_round(
                    &list_painter,
                    card,
                    theme::corner_radius(),
                    1.0,
                    if selected {
                        theme::alpha(theme::GREEN, 82)
                    } else {
                        theme::white_alpha(32)
                    },
                );
            }
            // 左侧 30px 见方的“版本标记”方块（图标底座）
            let mark = Rect::from_center_size(
                Pos2::new(row.left() + 24.0, row.center().y),
                Vec2::splat(30.0),
            );
            // 标记底色：可启动为绿色，否则灰色
            theme::fill_round(
                &row_painter,
                mark,
                theme::corner_radius(),
                if version.launchable {
                    Color32::from_rgb(48, 104, 57)
                } else {
                    Color32::from_rgb(66, 70, 61)
                },
            );
            // 若版本有自定义图标则贴图，否则画一个方块（Box）图标
            if let Some(texture) = self.version_icon(version) {
                paint_rounded_image(&row_painter, mark.shrink(2.0), 9.0, texture, texture.size());
            } else {
                draw_icon(
                    &row_painter,
                    NavIcon::Box,
                    Rect::from_center_size(mark.center(), Vec2::splat(16.0)),
                    if version.launchable {
                        theme::GREEN_BRIGHT
                    } else {
                        Color32::from_rgb(150, 150, 132)
                    },
                );
            }
            // 计算版本主标题（按宽度自适应裁剪）
            let row_label = self.version_selector_label(version);
            let row_label = fit_text_to_width(
                &row_painter,
                &row_label,
                theme::font(11.0),
                (row.width() - 118.0).max(100.0),
            );
            // 计算版本副标题/详情文字
            let row_detail = self.version_selector_detail(version);
            // 绘制粗体标题（可启动用主色，否则用柔和色）
            theme::bold_text(
                &row_painter,
                Pos2::new(row.left() + 48.0, row.top() + 12.0),
                row_label,
                11.0,
                if version.launchable {
                    theme::ink()
                } else {
                    theme::ink_soft()
                },
            );
            // 绘制灰色副标题，过长尾部省略
            theme::text(
                &row_painter,
                Pos2::new(row.left() + 48.0, row.top() + 33.0),
                ellipsize_end(&row_detail, 42),
                8.0,
                theme::muted(),
            );
            // 右侧状态徽标：可启动显示“可启动”，否则显示“需补全”
            let status = if version.launchable {
                "可启动"
            } else {
                "需补全"
            };
            // 两种状态文字宽度不同，单独给定徽标宽度
            let status_width = if version.launchable { 47.0 } else { 50.0 };
            let status_rect = Rect::from_center_size(
                Pos2::new(row.right() - status_width * 0.5 - 12.0, row.center().y),
                Vec2::new(status_width, 23.0),
            );
            // 状态徽标底色：可启动为绿色淡底，否则为琥珀色淡底
            theme::fill_round(
                &row_painter,
                status_rect,
                theme::corner_radius(),
                if version.launchable {
                    theme::alpha(theme::GREEN, 22)
                } else {
                    theme::alpha(Color32::from_rgb(225, 178, 84), 20)
                },
            );
            // 居中绘制状态文字
            theme::center_text(
                &row_painter,
                status_rect.center(),
                status,
                7.5,
                if version.launchable {
                    theme::GREEN_BRIGHT
                } else {
                    Color32::from_rgb(208, 172, 102)
                },
                false,
            );
            // 点击某行：切换到该版本的详情视图（带切换动画）
            if response.clicked() {
                self.switch_version_detail(*global_index);
            }
        }
        // 列表为空时显示占位文字
        if filtered.is_empty() {
            theme::center_text(
                &list_painter,
                Pos2::new(list_panel.center().x, list_top + 60.0),
                "当前分类没有版本",
                9.0,
                theme::muted(),
                false,
            );
        }
        // 内容超过一屏时才绘制右侧滚动条
        if max_scroll > 0.0 {
            paint_list_scrollbar(
                painter,
                list_panel.right(),
                list_top,
                list_height,
                content_height,
                self.version_page_scroll,
                max_scroll,
            );
        }

        // 找到当前选中版本对应的 (全局索引, 版本)；若找不到（如筛选后无匹配）则提前返回
        let Some((selected_index, selected)) = filtered
            .iter()
            .find(|(index, _)| *index == self.version_page_selected)
        else {
            return;
        };
        // 克隆选中的版本数据，避免后续借用冲突（后面还要以 &mut self 绘制详情）
        let current = selected.clone();
        // 若存在“上一个详情版本”，则先以“出场(outgoing)”动画绘制旧详情，再绘制新详情实现切换过渡
        let detail_motion = if let Some(previous_index) = self.previous_version_detail {
            // 旧详情的动画：标记为 outgoing=true，随 progress 退场
            let outgoing = CardMotion {
                screen: layout.screen,
                direction: self.version_detail_direction,
                progress: self.version_detail_progress,
                outgoing: true,
            };
            // 绘制旧版本详情（非激活态，仅做退场动画）
            if let Some(previous) = self.versions.get(previous_index).cloned() {
                self.paint_version_detail(
                    ui,
                    painter,
                    layout,
                    time,
                    VersionDetailView {
                        index: previous_index,
                        version: previous,
                        motion: outgoing,
                        active: false,
                    },
                );
            }
            // 新详情使用同样的动画进度（但 outgoing=false，表现为入场）
            CardMotion {
                screen: layout.screen,
                direction: self.version_detail_direction,
                progress: self.version_detail_progress,
                outgoing: false,
            }
        } else {
            // 无旧详情时直接使用本页面传入的动画状态
            motion
        };
        // 绘制当前选中的版本详情（激活态，可交互）
        self.paint_version_detail(
            ui,
            painter,
            layout,
            time,
            VersionDetailView {
                index: *selected_index,
                version: current,
                motion: detail_motion,
                active: true,
            },
        );

        // ctx 在本函数中未直接使用，保留引用以满足签名（避免未使用告警）
        let _ = ctx;
    }

    /// 绘制“选择 Minecraft 文件夹”下拉浮层菜单。
    ///
    /// 该菜单由版本页标题区的文件夹选择条触发（`self.version_folder_menu_open`）。
    /// 浮层列出所有已保存的游戏目录，可点击切换当前目录，也可“浏览并添加文件夹”
    /// 或“使用官方游戏目录”自动扫描发现系统中的 Minecraft 安装位置。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文，用于创建前景 `Area` 浮层。
    /// - `layout`：布局几何，决定菜单弹出位置。
    /// - `time`：动画时间（秒），用于玻璃面板流动效果。
    ///
    /// # 副作用
    /// 点击选项会调用 `self.select_game_directory` 改变当前目录并关闭菜单；
    /// 扫描官方目录会向 `self.settings.game_directories` 追加发现的目录并弹出提示。
    pub(in crate::app) fn paint_version_folder_menu(
        &mut self,
        ctx: &egui::Context,
        layout: &Layout,
        time: f64,
    ) {
        // 根据目录数量计算菜单浮层尺寸与位置
        let rect = version_folder_menu_rect(layout, self.settings.game_directories.len());
        // 创建一个置于最前、位置固定（不可拖动）的浮层
        egui::Area::new(Id::unique("version-folder-menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .movable(false)
            .show(ctx, |ui| {
                ui.set_min_size(rect.size());
                let painter = ui.painter().clone();
                // 绘制带背景模糊的玻璃面板作为菜单底
                theme::glass_panel(
                    &painter,
                    rect,
                    theme::corner_radius(),
                    theme::GlassSpec::new(Color32::from_rgb(20, 32, 24), theme::GREEN, time, 18.4)
                        .backdrop_blur(14.0),
                );
                // 在菜单内裁剪内容
                let content = card_clip(&painter, rect);
                let painter = &content;
                // 菜单标题
                theme::bold_text(
                    &painter,
                    Pos2::new(rect.left() + 16.0, rect.top() + 11.0),
                    "选择 Minecraft 文件夹",
                    9.2,
                    theme::ink(),
                );
                // 取出所有已保存目录；最多直接展示 5 个（其余需滚动）
                let directories = self.settings.game_directories.clone();
                let visible = directories.len().min(5);
                let list_height = visible as f32 * 42.0;
                let max_scroll = (directories.len() as f32 * 42.0 - list_height).max(0.0);
                // 悬停菜单内滚动时调整滚动偏移
                if ui.ctx().input(|input| {
                    input
                        .pointer
                        .hover_pos()
                        .is_some_and(|pointer| rect.contains(pointer))
                        && input.smooth_scroll_delta.y.abs() > 0.0
                }) {
                    self.version_folder_scroll -=
                        ui.ctx().input(|input| input.smooth_scroll_delta.y);
                }
                // 钳制滚动范围
                self.version_folder_scroll = self.version_folder_scroll.clamp(0.0, max_scroll);
                // 列表起始 y（标题下方 34px）
                let list_top = rect.top() + 34.0;
                // 逐目录绘制选项行（虚拟滚动裁剪）
                for (index, directory) in directories.iter().enumerate() {
                    // 计算该行在当前滚动下的 y 坐标
                    let row_top = list_top + index as f32 * 42.0 - self.version_folder_scroll;
                    // 不在可视区内的行跳过
                    if row_top + 37.0 < list_top || row_top > list_top + list_height {
                        continue;
                    }
                    // 每行宽比菜单窄 16px，高 37px
                    let row = Rect::from_min_size(
                        Pos2::new(rect.left() + 8.0, row_top),
                        Vec2::new(rect.width() - 16.0, 37.0),
                    );
                    // 在行内裁剪
                    let row_painter = card_clip(painter, row);
                    // 注册行点击交互
                    let response = ui
                        .interact(
                            row,
                            Id::unique(("version-folder-option", index)),
                            Sense::click(),
                        )
                        .on_hover_cursor(CursorIcon::PointingHand);
                    // 判断本行是否为当前选中的游戏目录（按路径是否相同）
                    let selected = self
                        .settings
                        .game_directory
                        .as_ref()
                        .is_some_and(|current| same_path(current, directory));
                    // 行背景：选中绿底，悬停浅白，否则透明
                    theme::fill_round(
                        &row_painter,
                        row,
                        theme::corner_radius(),
                        if selected {
                            theme::alpha(theme::GREEN, 24)
                        } else if response.hovered() {
                            theme::white_alpha(10)
                        } else {
                            Color32::TRANSPARENT
                        },
                    );
                    // 行左侧文件夹图标，选中为亮绿否则灰色
                    draw_icon(
                        &row_painter,
                        NavIcon::Folder,
                        Rect::from_center_size(
                            Pos2::new(row.left() + 17.0, row.center().y),
                            Vec2::splat(14.0),
                        ),
                        if selected {
                            theme::GREEN_BRIGHT
                        } else {
                            theme::muted()
                        },
                    );
                    // 行文字显示目录路径（开头省略过长部分）
                    theme::text(
                        &row_painter,
                        Pos2::new(row.left() + 32.0, row.top() + 6.0),
                        ellipsize_start(&directory.display().to_string(), 48),
                        7.9,
                        if selected {
                            theme::ink()
                        } else {
                            theme::ink_soft()
                        },
                    );
                    // 当前选中的目录在右侧显示对勾
                    if selected {
                        theme::text(
                            &row_painter,
                            Pos2::new(row.right() - 17.0, row.top() + 7.0),
                            "✓",
                            9.0,
                            theme::GREEN_BRIGHT,
                        );
                    }
                    // 点击某目录：切换当前游戏目录并关闭菜单
                    if response.clicked() {
                        self.select_game_directory(directory.clone());
                        self.version_folder_menu_open = false;
                    }
                }
                // 操作区 y：在列表下方 6px 处
                let action_y = list_top + list_height + 6.0;
                // “浏览并添加文件夹…”按钮区域
                let browse = Rect::from_min_size(
                    Pos2::new(rect.left() + 8.0, action_y),
                    Vec2::new(rect.width() - 16.0, 34.0),
                );
                // 点击后弹出系统文件夹选择框；选定则设为当前目录并关闭菜单
                if settings_button(
                    ui,
                    &painter,
                    browse,
                    "version-folder-browse",
                    "浏览并添加文件夹…",
                    false,
                ) && let Some(directory) = crate::game::pick_game_directory()
                {
                    self.select_game_directory(directory);
                    self.version_folder_menu_open = false;
                }
                // “使用官方游戏目录”按钮区域（位于浏览按钮下方 42px）
                let scan = Rect::from_min_size(
                    Pos2::new(rect.left() + 8.0, action_y + 42.0),
                    Vec2::new(rect.width() - 16.0, 34.0),
                );
                // 点击后自动发现系统内的 Minecraft 目录并全部加入设置，然后提示
                if settings_button(
                    ui,
                    &painter,
                    scan,
                    "version-folder-scan",
                    "使用官方游戏目录",
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
            });
    }

    /// 绘制版本内容浏览/导出对话框（模组、存档、光影、材质包、数据包或导出）。
    ///
    /// 这是一个从屏幕中心放大展开的浮层对话框，带玻璃质感与缩放/位移动画。它既用于
    /// 浏览某版本各类数据目录中的条目，也用于在 `kind == Export` 时展示导出整合包的选项。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文，用于创建浮层、变换图层形状（缩放动画）。
    /// - `screen`：整个屏幕的矩形，用作背景遮罩与展开动画的起点/终点参考。
    /// - `time`：动画时间（秒），驱动玻璃面板流动效果。
    /// - `kind`：要展示的内容类别（Mods/Saves/ShaderPacks/ResourcePacks/Datapacks/Export）。
    ///
    /// # 副作用
    /// 根据 `self.version_content_popup_value` 等字段播放开合动画；点击关闭按钮或遮罩
    /// 外部会调用 `close_version_content_dialog`；`Export` 类别会委托 `paint_export_contents`
    /// 绘制并可能启动导出任务。
    pub(in crate::app) fn paint_version_content_dialog(
        &mut self,
        ctx: &egui::Context,
        screen: Rect,
        time: f64,
        kind: VersionContentKind,
    ) {
        // 取当前选中版本；若版本列表里没有选中项，则关闭对话框并退出
        let Some(version) = self.versions.get(self.version_page_selected).cloned() else {
            self.version_content_dialog = None;
            return;
        };
        // 收集内容条目：导出模式无条目；否则读取对应数据目录下的内容列表
        let entries = if kind == VersionContentKind::Export {
            Vec::new()
        } else {
            self.version_data_directory_for(&version.version)
                .map(|directory| version_content_entries(&directory.join(kind.folder()), kind))
                .unwrap_or_default()
        };
        // 计算对话框目标矩形（屏幕中央固定尺寸）
        let dialog = version_content_dialog_rect(screen);
        // 开合动画进度（0 关闭 → 1 完全展开）
        let progress = self.version_content_popup_value;
        // 动画起点：若记录了触发源矩形（被点击的卡片）则用之，否则用对话框本身
        let source = if self.version_content_popup_source.width() > 0.0
            && self.version_content_popup_source.height() > 0.0
        {
            self.version_content_popup_source
        } else {
            dialog
        };
        // 动画进度做轻微越界容差（-0.05~1.08）让动画首尾更顺滑
        let animated = progress.clamp(-0.05, 1.08);
        // 位移插值：smoothstep 缓动（progress 平方乘 (3-2p)）
        let travel = progress.clamp(0.0, 1.0);
        let travel = travel * travel * (3.0 - 2.0 * travel);
        // 当前中心点：从源中心线性插值到屏幕中心
        let mut animated_center = source.center() + (screen.center() - source.center()) * travel;
        // 关闭弹跳效果：仅在关闭动画尾声（progress 很小且已标记关闭）时产生轻微回弹
        let close_bounce = if !self.version_content_dialog_open && progress <= 0.002 {
            let t =
                (self.version_content_close_elapsed / CONTENT_CLOSE_BOUNCE_SECONDS).clamp(0.0, 1.0);
            (t * std::f32::consts::PI).sin() * (1.0 - t) * (1.0 - t)
        } else {
            0.0
        };
        // 关闭时的回弹方向：源相对对话框的方向，避免为零时使用朝上（Y 轴）
        let return_direction = {
            let direction = source.center() - dialog.center();
            if direction.length_sq() > 0.001 {
                direction.normalized()
            } else {
                Vec2::Y
            }
        };
        // 把回弹位移叠加到中心点上（幅度 5px，随回弹系数衰减）
        animated_center += return_direction * 5.0 * close_bounce;
        // 当前尺寸：由源尺寸按动画进度插值到对话框尺寸，并叠加回弹轻微放大
        let animated_size = (source.size() + (dialog.size() - source.size()) * animated)
            * (1.0 + close_bounce * 0.012);
        // 由中心点 + 尺寸得到当前帧的变形矩形（用于绘制玻璃面板）
        let morph_rect = Rect::from_center_size(animated_center, animated_size);
        // 外壳（背景遮罩）整体不透明度，当前恒为 1（淡入淡出由尺寸/位移动画承担）
        let opacity = 1.0;
        // 内容文字的不透明度随进度淡入
        let content_opacity = version_content_text_opacity(progress);
        // 内容初始缩放：根据源与对话框面积比开方得到（钳制在 0.08~0.48），模拟“从小变大”
        let content_start_scale = ((source.width() / dialog.width())
            * (source.height() / dialog.height()))
        .sqrt()
        .clamp(0.08, 0.48);
        // 内容随动画进度从初始缩放到 1.0
        let content_scale = content_start_scale + (1.0 - content_start_scale) * animated;
        // 缩放围绕的锚点：当前动画中心（内容随面板中心一起缩放）
        let content_center = animated_center;
        // 构造仿射变换（缩放+平移），供后续把对话框图层整体缩放做动画
        let content_transform = egui::emath::TSTransform::new(
            content_center - dialog.center() * content_scale,
            content_scale,
        );
        // 背景遮罩层：覆盖整个屏幕，绘制变形中的玻璃面板，并捕获点击
        let dialog_instance = self.version_content_dialog_instance;
        egui::Area::new(Id::unique(("version-content-backdrop", dialog_instance)))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .movable(false)
            .show(ctx, |ui| {
                ui.set_min_size(screen.size());
                let mut shell_painter = ui.painter().clone();
                shell_painter.set_opacity(opacity);
                // 绘制随动画变形的玻璃背景面板（带较强背景模糊）
                theme::glass_panel(
                    &shell_painter,
                    morph_rect,
                    theme::corner_radius(),
                    theme::GlassSpec::new(Color32::from_rgb(21, 34, 25), theme::GREEN, time, 31.0)
                        .backdrop_blur(22.0),
                );
                // 在整个屏幕注册点击交互
                let response = ui.interact(
                    screen,
                    Id::unique(("version-content-backdrop-interact", dialog_instance)),
                    Sense::click(),
                );
                // 若点击且落点不在对话框矩形内，则视为点击遮罩关闭对话框
                if response.clicked()
                    && ui
                        .ctx()
                        .input(|input| input.pointer.interact_pos())
                        .is_some_and(|pointer| !dialog.contains(pointer))
                {
                    self.close_version_content_dialog();
                }
            });
        // 内容对话框图层 id，供后续对整层做缩放动画（transform_layer_shapes）
        let mut dialog_layer = egui::LayerId::new(
            egui::Order::Foreground,
            Id::unique(("version-content-dialog", dialog_instance)),
        );
        // 内容对话框浮层：固定位置居中
        egui::Area::new(Id::unique(("version-content-dialog", dialog_instance)))
            .order(egui::Order::Foreground)
            .fixed_pos(dialog.min)
            .movable(false)
            .show(ctx, |ui| {
                ui.set_min_size(dialog.size());
                // 记录本图层 id，供函数末尾做整体缩放动画
                dialog_layer = ui.layer_id();
                let mut painter = ui.painter().clone();
                // 内容整体不透明度（随开合动画淡入）
                painter.set_opacity(content_opacity);
                // 在对话框内裁剪内容
                let content = card_clip(&painter, dialog);
                let painter = &content;
                // 根据内容类别选择标题区左侧图标
                let icon = match kind {
                    VersionContentKind::Mods => NavIcon::Box,
                    VersionContentKind::Saves => NavIcon::Folder,
                    // 光影/材质/数据包均用 Sparkles 图标
                    VersionContentKind::ShaderPacks
                    | VersionContentKind::ResourcePacks
                    | VersionContentKind::Datapacks => NavIcon::Sparkles,
                    VersionContentKind::Export => NavIcon::Layers,
                };
                // 绘制标题区图标
                draw_icon(
                    &painter,
                    icon,
                    Rect::from_center_size(
                        Pos2::new(dialog.left() + 31.0, dialog.top() + 30.0),
                        Vec2::splat(22.0),
                    ),
                    theme::GREEN_BRIGHT,
                );
                // 标题文字：内容类别名（如“模组”“存档”）
                theme::bold_text(
                    &painter,
                    Pos2::new(dialog.left() + 52.0, dialog.top() + 17.0),
                    kind.label(),
                    15.0,
                    theme::ink(),
                );
                // 副标题：导出模式显示“选择要包含的内容”，否则显示版本号·项数·目录模式
                theme::text(
                    &painter,
                    Pos2::new(dialog.left() + 52.0, dialog.top() + 39.0),
                    if kind == VersionContentKind::Export {
                        format!("{} · 选择要包含的内容", version.game_version)
                    } else {
                        format!(
                            "{} · {} 项 · {}",
                            version.game_version,
                            entries.len(),
                            if self.settings.version_isolation {
                                "版本独立目录"
                            } else {
                                "公共数据目录"
                            }
                        )
                    },
                    7.8,
                    theme::muted(),
                );
                // 右上角关闭按钮区域
                let close = version_content_close_rect(dialog);
                let close_response = ui
                    .interact(
                        close,
                        Id::unique(("version-content-close", dialog_instance)),
                        Sense::click(),
                    )
                    .on_hover_cursor(CursorIcon::PointingHand);
                // 关闭按钮底色：悬停浅白，否则半透明黑
                theme::fill_round(
                    &painter,
                    close,
                    theme::corner_radius(),
                    if close_response.hovered() {
                        theme::white_alpha(18)
                    } else {
                        theme::black_alpha(60)
                    },
                );
                // 关闭按钮内的叉号图标
                draw_icon(
                    &painter,
                    NavIcon::Close,
                    close.shrink(10.0),
                    theme::ink_soft(),
                );
                // 点击关闭按钮：触发关闭动画
                if close_response.clicked() {
                    self.close_version_content_dialog();
                }
                // 导出模式直接委托给导出选项绘制并提前返回（不渲染条目网格）
                if kind == VersionContentKind::Export {
                    self.paint_export_contents(ui, &painter, dialog, time, &version);
                    return;
                }

                // 条目网格区域（对话框内，左右留 18px，顶部标题下 68px）
                let list = Rect::from_min_max(
                    Pos2::new(dialog.left() + 18.0, dialog.top() + 68.0),
                    Pos2::new(dialog.right() - 18.0, dialog.bottom() - 16.0),
                );
                // 两列布局：列间距 10px，卡片高 72px
                let gap = 10.0;
                let card_width = (list.width() - gap) * 0.5;
                let card_height = 72.0;
                let row_height = card_height + gap;
                // 内容总高：按两列向上取整算出行数
                let content_height = entries.len().div_ceil(2) as f32 * row_height;
                let max_scroll = (content_height - list.height()).max(0.0);
                // 悬停网格内滚动时调整滚动偏移
                if ui.ctx().input(|input| {
                    input
                        .pointer
                        .hover_pos()
                        .is_some_and(|pointer| list.contains(pointer))
                        && input.smooth_scroll_delta.y.abs() > 0.0
                }) {
                    self.version_content_scroll -=
                        ui.ctx().input(|input| input.smooth_scroll_delta.y);
                }
                // 钳制滚动范围
                self.version_content_scroll = self.version_content_scroll.clamp(0.0, max_scroll);
                // 为网格创建裁剪绘制器，避免卡片溢出
                let list_painter = painter.with_clip_rect(list);
                // 无条目时显示空状态提示
                if entries.is_empty() {
                    theme::center_text(
                        &list_painter,
                        list.center(),
                        format!("这里还没有{}", kind.label()),
                        9.0,
                        theme::muted(),
                        false,
                    );
                }
                // 逐条目绘制卡片（两列网格，虚拟滚动裁剪）
                for (index, entry) in entries.iter().enumerate() {
                    // 计算该条目所在列（0/1）与行号
                    let column = index % 2;
                    let row = index / 2;
                    // 由列/行与滚动偏移得到卡片矩形
                    let card = Rect::from_min_size(
                        Pos2::new(
                            list.left() + column as f32 * (card_width + gap),
                            list.top() + row as f32 * row_height - self.version_content_scroll,
                        ),
                        Vec2::new(card_width, card_height),
                    );
                    // 不在可视区内的卡片跳过
                    if card.bottom() < list.top() || card.top() > list.bottom() {
                        continue;
                    }
                    // 仅悬停交互（卡片本身不可点击进入）
                    let response = ui.interact(
                        card,
                        Id::unique((
                            "version-content-entry",
                            dialog_instance,
                            kind.index(),
                            index,
                        )),
                        Sense::hover(),
                    );
                    // 绘制卡片玻璃面板：悬停时底色略亮
                    theme::glass_panel(
                        &list_painter,
                        card,
                        theme::corner_radius(),
                        theme::GlassSpec::new(
                            if response.hovered() {
                                Color32::from_rgb(29, 48, 33)
                            } else {
                                Color32::from_rgb(21, 35, 26)
                            },
                            theme::GREEN,
                            time,
                            31.5 + index as f32 * 0.15,
                        )
                        .hovered(response.hovered()),
                    );
                    // 卡片左侧类别图标
                    draw_icon(
                        &list_painter,
                        icon,
                        Rect::from_center_size(
                            Pos2::new(card.left() + 25.0, card.center().y),
                            Vec2::splat(18.0),
                        ),
                        theme::GREEN_BRIGHT,
                    );
                    // 卡片主标题（条目名，过长省略）
                    theme::bold_text(
                        &list_painter,
                        Pos2::new(card.left() + 44.0, card.top() + 15.0),
                        ellipsize_end(&entry.name, 38),
                        9.2,
                        theme::ink_soft(),
                    );
                    // 卡片副标题（条目详情，过长省略）
                    theme::text(
                        &list_painter,
                        Pos2::new(card.left() + 44.0, card.top() + 39.0),
                        ellipsize_end(&entry.detail, 42),
                        7.6,
                        theme::muted(),
                    );
                }
            });
        // 将整个对话框图层按之前构造的仿射变换（缩放/平移）做动画，实现放大展开效果
        ctx.transform_layer_shapes(dialog_layer, content_transform);
    }

    /// 绘制 Java 运行时选择浮层。
    ///
    /// 由版本详情或全局设置触发（`self.java_picker_target` 决定作用对象）。列出“自动选择”
    /// 选项与所有已扫描到的 Java 运行时（`self.java_runtimes`），点击即可为全局或指定版本
    /// 设定/清除 Java 路径；底部提供“重新扫描 Java”按钮。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文，用于创建前景浮层。
    /// - `screen`：屏幕矩形，决定浮层锚定位置（`java_picker_anchor`）与尺寸。
    /// - `time`：动画时间（秒），用于玻璃面板流动效果。
    ///
    /// # 副作用
    /// 点击选项会修改 `self.settings.java_path` 或为某版本设置/清除 `version_java_path`，
    /// 并关闭浮层；点击“重新扫描”会调用 `refresh_java_runtimes` 重新枚举系统中的 Java。
    pub(in crate::app) fn paint_java_picker(
        &mut self,
        ctx: &egui::Context,
        screen: Rect,
        time: f64,
    ) {
        // 若未设置选择目标（没有要配置的 Java 作用对象），直接不绘制
        let Some(target) = self.java_picker_target.clone() else {
            return;
        };
        // 选项总数 = 运行时数量 + 1（加“自动选择”那一行）
        let option_count = self.java_runtimes.len() + 1;
        // 最多直接展示 6 个（其余滚动）
        let visible = option_count.min(6);
        // 根据锚点、屏幕与数量计算浮层矩形
        let rect = java_picker_rect(self.java_picker_anchor, screen, option_count);
        // 前景浮层（固定不可拖动）
        egui::Area::new(Id::unique("java-picker"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .movable(false)
            .show(ctx, |ui| {
                ui.set_min_size(rect.size());
                let painter = ui.painter().clone();
                // 带背景模糊的玻璃面板底
                theme::glass_panel(
                    &painter,
                    rect,
                    theme::corner_radius(),
                    theme::GlassSpec::new(Color32::from_rgb(21, 34, 25), theme::GREEN, time, 34.0)
                        .backdrop_blur(16.0),
                );
                // 内容裁剪
                let content = card_clip(&painter, rect);
                let painter = &content;
                // 浮层标题
                theme::bold_text(
                    &painter,
                    Pos2::new(rect.left() + 16.0, rect.top() + 10.0),
                    "选择 Java 运行时",
                    9.2,
                    theme::ink(),
                );
                // 列表起始 y（标题下 34px），一个选项占 42px
                let list_top = rect.top() + 34.0;
                let list_height = visible as f32 * 42.0;
                let list_rect = Rect::from_min_size(
                    Pos2::new(rect.left() + 6.0, list_top),
                    Vec2::new(rect.width() - 12.0, list_height),
                );
                let max_scroll = (option_count as f32 * 42.0 - list_height).max(0.0);
                // 悬停列表内滚动时调整偏移
                if ui.ctx().input(|input| {
                    input
                        .pointer
                        .hover_pos()
                        .is_some_and(|pointer| list_rect.contains(pointer))
                        && input.smooth_scroll_delta.y.abs() > 0.0
                }) {
                    self.java_picker_scroll -= ui.ctx().input(|input| input.smooth_scroll_delta.y);
                }
                // 钳制滚动范围
                self.java_picker_scroll = self.java_picker_scroll.clamp(0.0, max_scroll);
                // 列表绘制器（克隆以便后续循环复用，避免被裁剪）
                let list_painter = painter.clone();
                // “自动选择”选项行：始终位于列表顶部（随滚动偏移移动）
                let automatic = Rect::from_min_size(
                    Pos2::new(rect.left() + 9.0, list_top - self.java_picker_scroll),
                    Vec2::new(rect.width() - 18.0, 38.0),
                );
                // 注册点击交互
                let automatic_response = ui
                    .interact(automatic, Id::unique("java-picker-auto"), Sense::click())
                    .on_hover_cursor(CursorIcon::PointingHand);
                // 判断“自动”选项是否被选中：作用于全局且无指定路径，或作用于版本且其无独立路径
                let automatic_selected = match &target {
                    JavaPickerTarget::Global => self.settings.java_path.is_none(),
                    JavaPickerTarget::Version { directory, profile } => self
                        .settings
                        .version_java_path(directory, profile)
                        .is_none(),
                };
                // 行背景：选中绿底，悬停浅白，否则透明
                theme::fill_round(
                    &list_painter,
                    automatic,
                    theme::corner_radius(),
                    if automatic_selected {
                        theme::alpha(theme::GREEN, 23)
                    } else if automatic_response.hovered() {
                        theme::white_alpha(10)
                    } else {
                        Color32::TRANSPARENT
                    },
                );
                // 标题文字：全局目标显示“自动选择兼容 Java”，版本目标显示“跟随全局 Java 设置”
                theme::bold_text(
                    &list_painter,
                    Pos2::new(automatic.left() + 12.0, automatic.top() + 5.0),
                    match target {
                        JavaPickerTarget::Global => "自动选择兼容 Java",
                        JavaPickerTarget::Version { .. } => "跟随全局 Java 设置",
                    },
                    8.6,
                    theme::ink_soft(),
                );
                // 副标题：根据全局是否已指定 Java 给出相应说明
                theme::text(
                    &list_painter,
                    Pos2::new(automatic.left() + 12.0, automatic.top() + 21.0),
                    if self.settings.java_path.is_some() {
                        "当前全局已指定 Java"
                    } else {
                        "由启动器根据版本要求选择"
                    },
                    7.2,
                    theme::muted(),
                );
                // 点击“自动”：清除对应作用对象的 Java 路径设定，并关闭浮层
                if automatic_response.clicked() {
                    match &target {
                        JavaPickerTarget::Global => self.settings.java_path = None,
                        JavaPickerTarget::Version { directory, profile } => self
                            .settings
                            .set_version_java_path(directory, profile, None),
                    }
                    self.java_picker_target = None;
                }
                // 遍历每个已发现的 Java 运行时，绘制一行（第 index 个运行时位于 automatic 下一行）
                for (index, runtime) in self.java_runtimes.iter().enumerate() {
                    // 计算该行矩形：从列表顶部起，第 (index+1) 行，随滚动偏移
                    let row = Rect::from_min_size(
                        Pos2::new(
                            rect.left() + 9.0,
                            list_top + (index + 1) as f32 * 42.0 - self.java_picker_scroll,
                        ),
                        Vec2::new(rect.width() - 18.0, 38.0),
                    );
                    // 不在可视区内的行跳过
                    if row.bottom() < list_top || row.top() > list_rect.bottom() {
                        continue;
                    }
                    // 注册点击交互
                    let response = ui
                        .interact(row, Id::unique(("java-runtime", index)), Sense::click())
                        .on_hover_cursor(CursorIcon::PointingHand);
                    // 判断该运行时是否正是当前目标所选的 Java 路径
                    let selected = match &target {
                        JavaPickerTarget::Global => self
                            .settings
                            .java_path
                            .as_ref()
                            .is_some_and(|path| same_path(path, &runtime.path)),
                        JavaPickerTarget::Version { directory, profile } => self
                            .settings
                            .version_java_path(directory, profile)
                            .as_ref()
                            .is_some_and(|path| same_path(path, &runtime.path)),
                    };
                    // 行背景：选中绿底，悬停浅白，否则透明
                    theme::fill_round(
                        &list_painter,
                        row,
                        theme::corner_radius(),
                        if selected {
                            theme::alpha(theme::GREEN, 23)
                        } else if response.hovered() {
                            theme::white_alpha(10)
                        } else {
                            Color32::TRANSPARENT
                        },
                    );
                    // 标题：“Java {主版本号}”
                    theme::bold_text(
                        &list_painter,
                        Pos2::new(row.left() + 12.0, row.top() + 5.0),
                        format!("Java {}", runtime.major),
                        8.6,
                        theme::ink_soft(),
                    );
                    // 副标题：运行时路径（开头省略过长部分）
                    theme::text(
                        &list_painter,
                        Pos2::new(row.left() + 12.0, row.top() + 21.0),
                        ellipsize_start(&runtime.path.display().to_string(), 48),
                        7.1,
                        theme::muted(),
                    );
                    // 点击某运行时：为对应目标设定该 Java 路径并关闭浮层
                    if response.clicked() {
                        match &target {
                            JavaPickerTarget::Global => {
                                self.settings.java_path = Some(runtime.path.clone())
                            }
                            JavaPickerTarget::Version { directory, profile } => self
                                .settings
                                .set_version_java_path(directory, profile, Some(&runtime.path)),
                        }
                        self.java_picker_target = None;
                    }
                }
                // 列表下方的“重新扫描 Java”按钮区域
                let refresh = Rect::from_min_size(
                    Pos2::new(rect.left() + 9.0, list_top + list_height + 4.0),
                    Vec2::new(rect.width() - 18.0, 32.0),
                );
                // 点击后重新枚举系统中所有 Java 运行时
                if settings_button(
                    ui,
                    &painter,
                    refresh,
                    "java-picker-refresh",
                    "重新扫描 Java",
                    false,
                ) {
                    self.refresh_java_runtimes();
                }
            });
    }

    /// 绘制“导出整合包”对话框内的选项区域（被 `paint_version_content_dialog` 在
    /// `kind == Export` 时调用，也会在老式 `paint_version_export_dialog` 中复用类似逻辑）。
    ///
    /// 展示若干可勾选的内容开关（模组/配置/存档/材质包/光影包）、导出格式选择（Modrinth/ZIP）、
    /// 许可提示与“取消/导出”按钮。点击“导出”会弹出文件保存框并启动后台导出任务。
    ///
    /// # 参数
    /// - `ui`：当前界面的 `Ui`，用于交互与按钮。
    /// - `painter`：绘制器，负责绘制卡片与文字。
    /// - `dialog`：对话框矩形，用于定位各控件。
    /// - `time`：动画时间（秒），用于玻璃卡片流动效果。
    /// - `version`：要导出的目标版本。
    ///
    /// # 副作用
    /// 读取并修改 `self.export_include_*`、`self.export_format` 等选项；点击导出会调用
    /// `start_modpack_export_worker` 启动后台导出任务并弹出提示；导出进行中按钮禁用。
    pub(in crate::app) fn paint_export_contents(
        &mut self,
        ui: &mut egui::Ui,
        painter: &Painter,
        dialog: Rect,
        time: f64,
        version: &GameVersion,
    ) {
        // 是否正在导出（后台任务接收端存在即表示导出进行中）
        let exporting = self.modpack_export_receiver.is_some();
        // 五个可选导出内容：每项为一个 (按钮id, 标题, 描述, 可变的开关值) 元组
        let options = [
            (
                "export-mods",
                "模组",
                "包含 mods 目录中的已启用模组",
                &mut self.export_include_mods,
            ),
            (
                "export-config",
                "模组配置",
                "包含 config 与默认配置",
                &mut self.export_include_config,
            ),
            (
                "export-saves",
                "存档",
                "包含 saves 目录中的世界（可能较大）",
                &mut self.export_include_saves,
            ),
            (
                "export-resourcepacks",
                "材质包",
                "包含 resourcepacks 目录",
                &mut self.export_include_resourcepacks,
            ),
            (
                "export-shaderpacks",
                "光影包",
                "包含 shaderpacks 目录",
                &mut self.export_include_shaderpacks,
            ),
        ];
        // 逐个绘制可勾选内容开关（每行 58px 高，从偏移 68 起依次向下）
        let inner_left = dialog.left() + 24.0;
        let inner_width = dialog.width() - 48.0;
        let options_top = dialog.top() + 76.0;
        let option_height = 58.0;
        let option_gap = 6.0;
        for (index, (id, title, description, value)) in options.into_iter().enumerate() {
            // 计算该行矩形
            let row = Rect::from_min_size(
                Pos2::new(
                    inner_left,
                    options_top + index as f32 * (option_height + option_gap),
                ),
                Vec2::new(inner_width, option_height),
            );
            // 先画设置卡片底，再画带开关的行
            paint_setting_card(painter, row, time);
            paint_toggle_row(ui, painter, row, id, title, description, value);
        }
        // 导出格式行：与内容选项保持统一间距，不贴住上一张卡片。
        let format_row = export_format_selector_rect(dialog);
        paint_setting_card(painter, format_row, time);
        let selector = Rect::from_min_size(
            Pos2::new(
                format_row.right() - SETTING_CARD_PADDING_X - 190.0,
                format_row.top() + 11.0,
            ),
            Vec2::new(190.0, 34.0),
        );
        // 格式说明只为左侧文字分配空间，并自动截断到下拉选择器之前。
        let format_text_width = (selector.left() - format_row.left() - 42.0).max(100.0);
        let format_title = fit_text_to_width(
            painter,
            "导出格式",
            theme::bold_font(10.4),
            format_text_width,
        );
        let format_description = fit_text_to_width(
            painter,
            "Modrinth 自动识别托管文件，ZIP 将内容完整内置",
            theme::font(8.0),
            format_text_width,
        );
        theme::bold_text(
            painter,
            Pos2::new(
                format_row.left() + SETTING_CARD_PADDING_X,
                format_row.top() + 13.0,
            ),
            format_title,
            10.4,
            theme::ink(),
        );
        theme::text(
            painter,
            Pos2::new(
                format_row.left() + SETTING_CARD_PADDING_X,
                format_row.top() + 33.0,
            ),
            format_description,
            8.0,
            theme::muted(),
        );
        let format_response = ui
            .interact(
                selector,
                Id::unique("export-format-selector"),
                Sense::click(),
            )
            .on_hover_cursor(CursorIcon::PointingHand);
        theme::fill_round(
            painter,
            selector,
            theme::corner_radius(),
            if format_response.hovered() || self.export_format_menu_open {
                Color32::from_rgb(37, 55, 41)
            } else {
                Color32::from_rgb(28, 43, 33)
            },
        );
        theme::stroke_round(
            painter,
            selector,
            theme::corner_radius(),
            1.0,
            if self.export_format_menu_open {
                theme::alpha(theme::GREEN, 72)
            } else {
                theme::white_alpha(32)
            },
        );
        theme::text(
            painter,
            Pos2::new(selector.left() + 12.0, selector.top() + 10.0),
            match self.export_format {
                ModpackExportFormat::Modrinth => "Modrinth 整合包",
                ModpackExportFormat::Zip => "ZIP 压缩包",
            },
            8.8,
            theme::ink_soft(),
        );
        draw_icon(
            painter,
            NavIcon::Chevron,
            Rect::from_center_size(
                Pos2::new(selector.right() - 16.0, selector.center().y),
                Vec2::splat(12.0),
            ),
            theme::muted(),
        );
        if format_response.clicked() {
            self.export_format_menu_open = !self.export_format_menu_open;
        }
        // 许可提示条（琥珀色底）位于底部上方 92px
        let hint = Rect::from_min_size(
            Pos2::new(inner_left, dialog.bottom() - 92.0),
            Vec2::new(inner_width, 30.0),
        );
        theme::fill_round(
            painter,
            hint,
            theme::corner_radius(),
            theme::alpha(Color32::from_rgb(220, 178, 89), 15),
        );
        theme::text(
            painter,
            Pos2::new(hint.left() + 12.0, hint.top() + 8.0),
            fit_text_to_width(
                painter,
                "请确认整合包中的资源分发许可，避免公开上传受限内容。",
                theme::font(7.8),
                hint.width() - 24.0,
            ),
            7.8,
            Color32::from_rgb(208, 176, 112),
        );
        // “取消”按钮（底部左）
        let cancel = Rect::from_min_size(
            Pos2::new(inner_left, dialog.bottom() - 50.0),
            Vec2::new(96.0, 36.0),
        );
        if settings_button(ui, painter, cancel, "export-cancel", "取消", false) {
            self.close_version_content_dialog();
        }
        // “选择位置并导出”按钮（底部右）
        let export = Rect::from_min_size(
            Pos2::new(dialog.right() - 24.0 - 138.0, dialog.bottom() - 50.0),
            Vec2::new(138.0, 36.0),
        );
        // 点击导出（导出进行中禁用该按钮）：弹出保存框并启动后台导出
        if settings_button(
            ui,
            painter,
            export,
            "export-start",
            if exporting {
                "正在导出…"
            } else {
                "选择位置并导出"
            },
            !exporting,
        ) && !exporting
        {
            // 用版本号（清洗非法字符）拼出默认文件名 + 对应扩展名
            let default_name = format!(
                "{}.{}",
                sanitize_filename(&version.version),
                self.export_format.extension()
            );
            // 让用户选择导出目标路径，并确保存在游戏目录（作为基准目录）
            if let Some(destination) =
                pick_export_file(&default_name, self.export_format.extension())
                && let Some(base) = self.settings.game_directory.clone()
            {
                // 数据目录：版本独立目录优先，否则回退到游戏目录
                let data_directory = self
                    .version_data_directory_for(&version.version)
                    .unwrap_or_else(|| base.clone());
                // 汇总导出选项
                let options = VersionExportOptions {
                    mods: self.export_include_mods,
                    config: self.export_include_config,
                    saves: self.export_include_saves,
                    resourcepacks: self.export_include_resourcepacks,
                    shaderpacks: self.export_include_shaderpacks,
                    format: self.export_format,
                };
                // 启动后台导出任务，保存其接收端以便查询进度
                self.modpack_export_receiver = Some(start_modpack_export_worker(
                    base,
                    data_directory,
                    version.clone(),
                    destination,
                    options,
                ));
                // 根据格式给出对应提示
                self.show_toast(if self.export_format == ModpackExportFormat::Modrinth {
                    "正在识别 Modrinth 文件并生成整合包…"
                } else {
                    "正在整理并生成 ZIP 整合包…"
                });
            }
        }

        if self.export_format_menu_open {
            let menu = export_format_menu_rect(format_row);
            egui::Area::new(Id::unique("export-format-list-layer"))
                .order(egui::Order::Foreground)
                .fixed_pos(menu.min)
                .interactable(true)
                .show(ui.ctx(), |list_ui| {
                    list_ui.set_min_size(menu.size());
                    let list_painter = list_ui.painter().clone();
                    theme::glass_panel(
                        &list_painter,
                        menu,
                        theme::corner_radius(),
                        theme::GlassSpec::new(
                            Color32::from_rgb(21, 34, 25),
                            theme::GREEN,
                            time,
                            44.0,
                        )
                        .backdrop_blur(16.0),
                    );
                    let list_content = card_clip(&list_painter, menu);
                    let options = [
                        (
                            ModpackExportFormat::Modrinth,
                            "Modrinth 整合包",
                            "自动识别远程托管文件，减小体积",
                        ),
                        (
                            ModpackExportFormat::Zip,
                            "ZIP 压缩包",
                            "完整内置模组、配置和其他内容",
                        ),
                    ];
                    for (index, (format, label, description)) in options.into_iter().enumerate() {
                        let row = Rect::from_min_size(
                            Pos2::new(menu.left() + 7.0, menu.top() + 7.0 + index as f32 * 43.0),
                            Vec2::new(menu.width() - 14.0, 39.0),
                        );
                        let response = list_ui
                            .interact(
                                row,
                                Id::unique(("export-format-option", index)),
                                Sense::click(),
                            )
                            .on_hover_cursor(CursorIcon::PointingHand);
                        let selected = self.export_format == format;
                        if selected || response.hovered() {
                            theme::fill_round(
                                &list_content,
                                row,
                                theme::corner_radius(),
                                if selected {
                                    theme::alpha(theme::GREEN, 22)
                                } else {
                                    theme::white_alpha(10)
                                },
                            );
                        }
                        theme::bold_text(
                            &list_content,
                            Pos2::new(row.left() + 10.0, row.top() + 5.0),
                            label,
                            9.0,
                            if selected {
                                theme::GREEN_BRIGHT
                            } else {
                                theme::ink_soft()
                            },
                        );
                        theme::text(
                            &list_content,
                            Pos2::new(row.left() + 10.0, row.top() + 20.0),
                            fit_text_to_width(
                                &list_content,
                                description,
                                theme::font(7.2),
                                row.width() - 38.0,
                            ),
                            7.2,
                            theme::muted(),
                        );
                        if selected {
                            theme::text(
                                &list_content,
                                Pos2::new(row.right() - 19.0, row.top() + 11.0),
                                "✓",
                                9.0,
                                theme::GREEN,
                            );
                        }
                        if response.clicked() {
                            self.export_format = format;
                            self.export_format_menu_open = false;
                        }
                    }
                });
        }
    }

    /// 旧版“导出整合包”对话框绘制（当前已被内容对话框的导出流程取代，故标记 `#[allow(dead_code)]`）。
    ///
    /// 结构与 `paint_export_contents` 类似：选择要包含的内容与导出格式，点击后同步执行
    /// 导出（阻塞式调用 `export_version_archive`）并打开结果所在文件夹。保留此方法以便
    /// 将来需要时复用，或作为导出逻辑的参考实现。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文，用于创建前景浮层。
    /// - `screen`：屏幕矩形，用于定位对话框。
    /// - `time`：动画时间（秒），用于玻璃面板流动效果。
    ///
    /// # 副作用
    /// 点击导出会同步生成归档文件，成功后打开所在文件夹并提示，失败则提示错误。
    #[allow(dead_code)]
    pub(in crate::app) fn paint_version_export_dialog(
        &mut self,
        ctx: &egui::Context,
        screen: Rect,
        time: f64,
    ) {
        // 取当前选中的版本；若没有则关闭导出对话框并退出
        let Some(version) = self.versions.get(self.version_page_selected).cloned() else {
            self.version_export_open = false;
            return;
        };
        // 计算对话框矩形
        let dialog = version_export_dialog_rect(screen);
        // 前景浮层（固定不可拖动）
        egui::Area::new(Id::unique("version-export-dialog"))
            .order(egui::Order::Foreground)
            .fixed_pos(dialog.min)
            .movable(false)
            .show(ctx, |ui| {
                ui.set_min_size(dialog.size());
                let painter = ui.painter().clone();
                // 带背景模糊的玻璃面板底
                theme::glass_panel(
                    &painter,
                    dialog,
                    theme::corner_radius(),
                    theme::GlassSpec::new(Color32::from_rgb(21, 33, 25), theme::GREEN, time, 35.0)
                        .backdrop_blur(18.0),
                );
                // 内容裁剪
                let content = card_clip(&painter, dialog);
                let painter = &content;
                // 标题“导出整合包”
                theme::bold_text(
                    &painter,
                    Pos2::new(dialog.left() + 22.0, dialog.top() + 17.0),
                    "导出整合包",
                    17.0,
                    theme::ink(),
                );
                // 副标题：说明版本配置与加载器信息始终包含
                theme::text(
                    &painter,
                    Pos2::new(dialog.left() + 22.0, dialog.top() + 44.0),
                    format!(
                        "{} · 版本配置和加载器信息始终包含",
                        self.version_selector_label(&version)
                    ),
                    8.1,
                    theme::muted(),
                );
                // 右上角关闭按钮区域（28x28）
                let close = Rect::from_min_size(
                    Pos2::new(dialog.right() - 44.0, dialog.top() + 16.0),
                    Vec2::splat(28.0),
                );
                let close_response = ui
                    .interact(close, Id::unique("export-close"), Sense::click())
                    .on_hover_cursor(CursorIcon::PointingHand);
                // 关闭按钮底色：悬停浅白，否则半透明黑
                theme::fill_round(
                    &painter,
                    close,
                    theme::corner_radius(),
                    if close_response.hovered() {
                        theme::white_alpha(18)
                    } else {
                        theme::black_alpha(60)
                    },
                );
                // 关闭按钮叉号图标
                draw_icon(
                    &painter,
                    NavIcon::Close,
                    close.shrink(8.0),
                    theme::ink_soft(),
                );
                // 点击关闭：标记导出对话框关闭
                if close_response.clicked() {
                    self.version_export_open = false;
                }
                // 五个可选导出内容（与内容对话框中一致）
                let options = [
                    (
                        "export-mods",
                        "模组",
                        "包含 mods 目录中的已启用模组",
                        &mut self.export_include_mods,
                    ),
                    (
                        "export-config",
                        "模组配置",
                        "包含 config 与默认配置",
                        &mut self.export_include_config,
                    ),
                    (
                        "export-saves",
                        "存档",
                        "包含 saves 目录中的世界（可能较大）",
                        &mut self.export_include_saves,
                    ),
                    (
                        "export-resourcepacks",
                        "材质包",
                        "包含 resourcepacks 目录",
                        &mut self.export_include_resourcepacks,
                    ),
                    (
                        "export-shaderpacks",
                        "光影包",
                        "包含 shaderpacks 目录",
                        &mut self.export_include_shaderpacks,
                    ),
                ];
                // 逐个绘制可勾选内容开关（每行 46px 高，从偏移 75 起）
                for (index, (id, title, description, value)) in options.into_iter().enumerate() {
                    let row = Rect::from_min_size(
                        Pos2::new(
                            dialog.left() + 20.0,
                            dialog.top() + 75.0 + index as f32 * 52.0,
                        ),
                        Vec2::new(dialog.width() - 40.0, 46.0),
                    );
                    paint_setting_card(&painter, row, time);
                    paint_toggle_row(ui, &painter, row, id, title, description, value);
                }
                // 许可提示条（琥珀色底）
                let hint = Rect::from_min_size(
                    Pos2::new(dialog.left() + 20.0, dialog.bottom() - 92.0),
                    Vec2::new(dialog.width() - 40.0, 30.0),
                );
                theme::fill_round(
                    &painter,
                    hint,
                    theme::corner_radius(),
                    theme::alpha(Color32::from_rgb(220, 178, 89), 15),
                );
                theme::text(
                    &painter,
                    Pos2::new(hint.left() + 12.0, hint.top() + 8.0),
                    "请确认整合包中的资源分发许可，避免公开上传受限内容。",
                    7.8,
                    Color32::from_rgb(208, 176, 112),
                );
                // “取消”按钮（底部左）
                let cancel = Rect::from_min_size(
                    Pos2::new(dialog.left() + 20.0, dialog.bottom() - 50.0),
                    Vec2::new(96.0, 36.0),
                );
                if settings_button(ui, &painter, cancel, "export-cancel", "取消", false) {
                    self.version_export_open = false;
                }
                // “选择位置并导出”按钮（底部右）
                let export = Rect::from_min_size(
                    Pos2::new(dialog.right() - 158.0, dialog.bottom() - 50.0),
                    Vec2::new(138.0, 36.0),
                );
                // 点击导出：同步生成归档
                if settings_button(ui, &painter, export, "export-start", "选择位置并导出", true)
                {
                    // 默认文件名 + 扩展名
                    let default_name = format!(
                        "{}.{}",
                        sanitize_filename(&version.version),
                        self.export_format.extension()
                    );
                    // 选目标路径并确保存在游戏目录
                    if let Some(destination) =
                        pick_export_file(&default_name, self.export_format.extension())
                        && let Some(base) = self.settings.game_directory.clone()
                    {
                        // 数据目录：版本独立目录优先，否则回退游戏目录
                        let data_directory = self
                            .version_data_directory_for(&version.version)
                            .unwrap_or_else(|| base.clone());
                        // 同步执行导出
                        let result = export_version_archive(
                            &base,
                            &data_directory,
                            &version,
                            &destination,
                            &VersionExportOptions {
                                mods: self.export_include_mods,
                                config: self.export_include_config,
                                saves: self.export_include_saves,
                                resourcepacks: self.export_include_resourcepacks,
                                shaderpacks: self.export_include_shaderpacks,
                                format: self.export_format,
                            },
                        );
                        // 根据结果提示并（成功时）打开所在文件夹
                        match result {
                            Ok(()) => {
                                self.version_export_open = false;
                                self.show_toast(format!("已导出到 {}", destination.display()));
                                open_path(destination.parent().unwrap_or(&base));
                            }
                            Err(error) => self.show_toast(format!("导出失败：{error}")),
                        }
                    }
                }
            });
    }

    /// 绘制右侧单个版本的详情面板。
    ///
    /// 展示选中版本的标题、加载器、Java 要求、数据目录模式，并提供内存分配（增减/重置）、
    /// Java 运行时选择，以及模组/存档/光影/材质包/数据包/导出的快捷入口与“打开目录/设为当前版本”
    /// 操作。详情面板支持切换动画（由 `view.motion` 控制）与“激活态”（`view.active` 决定
    /// 是否可交互）。
    ///
    /// # 参数
    /// - `ui`：当前界面的 `Ui`，用于交互与按钮。
    /// - `painter`：绘制器，负责绘制面板与文字。
    /// - `layout`：布局几何（内容区与屏幕边界），决定右侧详情面板位置。
    /// - `time`：动画时间（秒），用于玻璃面板流动效果。
    /// - `view`：本次要绘制的版本视图（含版本数据、动画状态、是否激活、全局索引）。
    ///
    /// # 副作用
    /// 仅在 `view.active` 时，交互会修改设置（内存 `set_version_memory`/清除、Java 选择、
    /// 打开文件夹、设为当前版本、打开内容对话框或资源下载页）。非激活态只做退场动画绘制。
    pub(in crate::app) fn paint_version_detail(
        &mut self,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
        view: VersionDetailView,
    ) {
        // 取出要展示的版本与动画状态
        let selected = view.version;
        let motion = view.motion;
        // 整个页面区域（与左侧列表共用同一页范围）
        let page = Rect::from_min_max(
            Pos2::new(layout.content_left, layout.header.bottom() + 14.0),
            Pos2::new(layout.content_right, layout.screen.bottom() - 22.0),
        );
        // 列表区宽度（与左侧一致）
        let list_width = (page.width() * 0.49).clamp(430.0, 590.0);
        // 右侧详情面板：从列表右侧 + 14px 间距一直到页面右下角
        let source_detail_panel = Rect::from_min_max(
            Pos2::new(page.left() + list_width + 14.0, page.top()),
            page.right_bottom(),
        );
        // 复制 painter 以免后续重新绑定可变借用冲突
        let painter = painter.clone();
        let painter = &painter;
        // 顶部标题区矩形（高 76px），由动画映射当前帧位置
        let detail_heading = motion.rect(Rect::from_min_size(
            source_detail_panel.left_top(),
            Vec2::new(source_detail_panel.width(), 76.0),
        ));
        // 保存基 painter，供后续多个面板复用
        let painter_base = painter;
        // 绘制标题区玻璃面板
        theme::glass_panel(
            painter,
            detail_heading,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(20, 32, 24), theme::GREEN, time, 17.0),
        );
        // 在标题区内裁剪
        let detail_heading_content = card_clip(painter, detail_heading);
        let painter = &detail_heading_content;
        // 标题区左上角“版本详情”小标题
        let heading = Pos2::new(detail_heading.left() + 22.0, detail_heading.top() + 12.0);
        theme::text(painter, heading, "版本详情", 8.0, theme::muted());
        // 版本主标题（按宽度自适应）
        let detail_title = self.version_selector_label(&selected);
        let detail_title = fit_text_to_width(
            painter,
            &detail_title,
            theme::bold_font(20.0),
            detail_heading.width() - 142.0,
        );
        // 绘制 20px 粗体版本标题
        theme::bold_text(
            painter,
            heading + Vec2::new(0.0, 18.0),
            detail_title,
            20.0,
            theme::ink(),
        );
        if view.active {
            let rename = Rect::from_min_size(
                Pos2::new(detail_heading.right() - 100.0, detail_heading.top() + 24.0),
                Vec2::new(78.0, 29.0),
            );
            if settings_button(ui, painter, rename, "version-rename", "重命名", false) {
                self.open_version_name_dialog(rename);
            }
        }
        // 内容区可用宽度（两侧各留 26px）
        let content_width = (source_detail_panel.width() - 52.0).max(1.0);
        // 摘要卡片：位于标题区下方（偏移 94px），高 116px
        let summary = motion.rect(Rect::from_min_size(
            Pos2::new(
                source_detail_panel.left() + 26.0,
                source_detail_panel.top() + 94.0,
            ),
            Vec2::new(content_width, 116.0),
        ));
        let painter = painter_base;
        // 绘制摘要卡片玻璃面板
        theme::glass_panel(
            painter,
            summary,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(22, 35, 26), theme::GREEN, time, 17.8),
        );
        // 在摘要卡片内裁剪
        let summary_content = card_clip(painter, summary);
        let painter = &summary_content;
        // 摘要卡片左侧 64px 见方的版本图标
        let detail_icon = Rect::from_center_size(
            Pos2::new(summary.left() + 54.0, summary.center().y),
            Vec2::splat(64.0),
        );
        // 有自定义图标则贴图，否则画一个“草方块”图标
        if let Some(texture) = self.version_icon(&selected) {
            paint_rounded_image(painter, detail_icon, 11.0, texture, texture.size());
        } else {
            paint_block(
                painter,
                detail_icon.center(),
                64.0,
                Color32::from_rgb(122, 210, 103),
                Color32::from_rgb(86, 64, 44),
                Color32::from_rgb(68, 143, 69),
            );
        }
        // 信息区起始 x（图标右侧）
        let info_x = summary.left() + 100.0;
        // “加载器”标签
        theme::text(
            painter,
            Pos2::new(info_x, summary.top() + 20.0),
            "加载器",
            7.9,
            theme::muted(),
        );
        // 加载器名称（如 Forge / Fabric / 原版），过长省略
        theme::bold_text(
            painter,
            Pos2::new(info_x, summary.top() + 35.0),
            ellipsize_end(&selected.loader, 30),
            11.0,
            theme::ink_soft(),
        );
        // “Java 要求”标签
        theme::text(
            painter,
            Pos2::new(info_x, summary.top() + 64.0),
            "Java 要求",
            7.9,
            theme::muted(),
        );
        // Java 要求文字
        theme::text(
            painter,
            Pos2::new(info_x, summary.top() + 79.0),
            &selected.java,
            9.5,
            theme::ink_soft(),
        );
        // 右上角显示该版本使用的数据目录模式（独立/公共）
        theme::text(
            painter,
            Pos2::new(summary.right() - 150.0, summary.top() + 20.0),
            if self.settings.version_isolation {
                "独立数据目录"
            } else {
                "公共数据目录"
            },
            7.9,
            theme::GREEN_BRIGHT,
        );

        // 游戏目录（作为后续数据目录/路径操作的基础）
        let base_directory = self.settings.game_directory.clone();
        // 该版本的数据目录：版本独立目录优先，否则回退到游戏目录（再不行为空）
        let data_directory = self
            .version_data_directory_for(&selected.version)
            .unwrap_or_else(|| base_directory.clone().unwrap_or_default());
        // 全局默认内存（MB）
        let default_memory = self.settings.memory_mb;
        // 该版本的独立内存覆盖值（若有）
        let override_memory = base_directory.as_ref().and_then(|directory| {
            self.settings
                .version_memory_mb(directory, &selected.version)
        });
        // 实际使用的内存：有覆盖用覆盖，否则用全局默认
        let memory_limit = memory_limit_mb();
        let memory = override_memory.unwrap_or(default_memory).min(memory_limit);
        // 内存卡片：位于摘要卡片下方 10px，高 78px
        let memory_card = motion.rect(Rect::from_min_size(
            Pos2::new(summary.left(), summary.bottom() + 10.0),
            Vec2::new(content_width, 78.0),
        ));
        let painter = painter_base;
        // 绘制内存卡片玻璃面板
        theme::glass_panel(
            painter,
            memory_card,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(21, 34, 25), theme::GREEN, time, 18.1),
        );
        // 内存卡片内裁剪
        let memory_content = card_clip(painter, memory_card);
        let painter = &memory_content;
        // 卡片标题“内存分配”
        theme::bold_text(
            painter,
            Pos2::new(memory_card.left() + 18.0, memory_card.top() + 13.0),
            "内存分配",
            10.0,
            theme::ink(),
        );
        // 副标题：说明当前是独立设置还是跟随全局
        theme::text(
            painter,
            Pos2::new(memory_card.left() + 18.0, memory_card.top() + 36.0),
            if override_memory.is_some() {
                "当前版本独立设置"
            } else {
                "当前跟随全局设置"
            },
            7.8,
            theme::muted(),
        );
        // 内存数值显示中心（卡片右侧偏左）
        let memory_control_center = Pos2::new(memory_card.right() - 135.0, memory_card.center().y);
        // 居中显示当前内存（MB 转 GB，保留 1 位小数）
        theme::center_text(
            painter,
            memory_control_center,
            format!("{:.1} GB", memory as f32 / 1024.0),
            13.0,
            theme::ink(),
            true,
        );
        // 仅激活态允许交互（调整内存）
        if view.active {
            // “-”按钮（每次减 512MB）
            let minus = Rect::from_center_size(
                Pos2::new(memory_card.right() - 220.0, memory_card.center().y),
                Vec2::splat(30.0),
            );
            // “+”按钮（每次加 512MB）
            let plus = Rect::from_center_size(
                Pos2::new(memory_card.right() - 50.0, memory_card.center().y),
                Vec2::splat(30.0),
            );
            // 点击减号：内存下限 2048MB，步长 512MB（saturating 防下溢）
            if compact_icon_button(ui, painter, minus, "version-memory-minus", false)
                && let Some(directory) = base_directory.as_ref()
            {
                self.settings.set_version_memory(
                    directory,
                    &selected.version,
                    memory.saturating_sub(512).max(2048),
                );
            }
            // 点击加号：内存上限来自本机物理内存，步长 512MB
            if compact_icon_button(ui, painter, plus, "version-memory-plus", true)
                && let Some(directory) = base_directory.as_ref()
            {
                self.settings.set_version_memory(
                    directory,
                    &selected.version,
                    (memory + 512).min(memory_limit),
                );
            }
            // 若该版本有独立内存设置，则显示“跟随全局”重置按钮
            if override_memory.is_some() {
                let reset = Rect::from_min_size(
                    Pos2::new(minus.left() - 96.0, memory_card.center().y - 12.0),
                    Vec2::new(88.0, 24.0),
                );
                // 点击重置：清除该版本的内存覆盖，恢复全局设置
                if settings_button(
                    ui,
                    painter,
                    reset,
                    "version-memory-reset",
                    "跟随全局",
                    false,
                ) && let Some(directory) = base_directory.as_ref()
                {
                    self.settings
                        .clear_version_memory(directory, &selected.version);
                }
            }
        }

        // 该版本独立的 Java 路径覆盖值：需要游戏目录存在，且设置中为该版本保存过路径
        let version_java = base_directory.as_ref().and_then(|directory| {
            self.settings
                .version_java_path(directory, &selected.version)
        });
        // 实际生效的 Java 路径：版本独立设置优先，其次回退到全局 Java 路径，否则为 None（自动选择）
        let effective_java = version_java
            .clone()
            .or_else(|| self.settings.java_path.clone());
        // Java 卡片：位于内存卡片下方 10px，高 78px（与内存卡片同宽同风格）
        let java_card = motion.rect(Rect::from_min_size(
            Pos2::new(memory_card.left(), memory_card.bottom() + 10.0),
            Vec2::new(content_width, 78.0),
        ));
        let painter = painter_base;
        // 绘制 Java 卡片玻璃面板
        theme::glass_panel(
            painter,
            java_card,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(21, 34, 25), theme::GREEN, time, 18.2),
        );
        // 在 Java 卡片内裁剪，防止内容溢出圆角
        let java_content = card_clip(painter, java_card);
        let painter = &java_content;
        // 卡片标题“Java 运行时”
        theme::bold_text(
            painter,
            Pos2::new(java_card.left() + 18.0, java_card.top() + 13.0),
            "Java 运行时",
            10.0,
            theme::ink(),
        );
        // 构造 Java 卡片的描述文字：
        // - 有生效路径时显示来源前缀（“版本指定”/“跟随全局”）+ 运行时标签（如 "Java 17"）
        // - 没有任何指定路径时提示“自动选择兼容版本”
        let java_description = match effective_java.as_ref() {
            Some(path) => {
                // 依据“版本独立设置是否存在”判断该路径来自版本覆盖还是全局配置
                let prefix = if version_java.is_some() {
                    "版本指定"
                } else {
                    "跟随全局"
                };
                format!("{prefix} · {}", self.java_runtime_label(path))
            }
            None => "自动选择兼容版本".to_owned(),
        };
        // 绘制描述文字（过长尾部省略）
        theme::text(
            painter,
            Pos2::new(java_card.left() + 18.0, java_card.top() + 37.0),
            ellipsize_end(&java_description, 58),
            7.8,
            theme::muted(),
        );
        // 仅激活态允许交互：点击“选择”按钮打开 Java 运行时选择浮层
        if view.active {
            // “选择”按钮区域（卡片右对齐，宽 78 高 32）
            let select = Rect::from_min_size(
                Pos2::new(java_card.right() - 96.0, java_card.center().y - 16.0),
                Vec2::new(78.0, 32.0),
            );
            // 点击后：把浮层锚点设为按钮矩形，目标设为“本版本”，并重置浮层滚动
            if settings_button(ui, painter, select, "version-java-select", "选择", false)
                && let Some(directory) = base_directory.clone()
            {
                // 浮层弹出位置（以按钮矩形为锚点）
                self.java_picker_anchor = select;
                // 指定本次选择的作用对象：该游戏目录下的这个版本
                self.java_picker_target = Some(JavaPickerTarget::Version {
                    directory,
                    profile: selected.version.clone(),
                });
                // 打开浮层时列表从头开始滚动
                self.java_picker_scroll = 0.0;
            }
        }

        // 内容控制卡区域起始 y：Java 卡片下方 10px
        let control_top = java_card.bottom() + 10.0;
        // 两列布局的列间距
        let gap = 10.0;
        // 每列卡片宽度：可用宽度减去间距后均分
        let control_width = (content_width - gap) * 0.5;
        // 六个内容控制卡的定义：每项为 (目录标识, 标签, 条目数, 图标) 元组
        // 条目数实时扫描数据目录得到（导出卡片固定为 0，不显示计数）
        let controls = [
            (
                "mods",
                "模组",
                installed_mod_files(&data_directory).len(),
                NavIcon::Box,
            ),
            (
                "saves",
                "存档",
                count_directories(&data_directory.join("saves")),
                NavIcon::Folder,
            ),
            (
                "shaderpacks",
                "光影",
                count_content_packs(&data_directory.join("shaderpacks")),
                NavIcon::Sparkles,
            ),
            (
                "resourcepacks",
                "材质包",
                count_content_packs(&data_directory.join("resourcepacks")),
                NavIcon::Sparkles,
            ),
            (
                "datapacks",
                "数据包",
                count_content_packs(&data_directory.join("datapacks")),
                NavIcon::Sparkles,
            ),
            ("export", "导出", 0, NavIcon::Layers),
        ];
        // 遍历六个控制卡，逐个绘制（两列三行的网格布局）
        for (index, (folder, label, count, icon)) in controls.into_iter().enumerate() {
            // 把目录标识映射为对应的内容对话框类别（用于点击后打开相应弹窗）
            let content_kind = match folder {
                "mods" => Some(VersionContentKind::Mods),
                "saves" => Some(VersionContentKind::Saves),
                "shaderpacks" => Some(VersionContentKind::ShaderPacks),
                "resourcepacks" => Some(VersionContentKind::ResourcePacks),
                "datapacks" => Some(VersionContentKind::Datapacks),
                "export" => Some(VersionContentKind::Export),
                // 兜底分支：目录标识不在上述列表中时不关联任何类别
                _ => None,
            };
            // 由索引算出该卡所在的列（0/1）与行号（0/1/2）
            let column = index % 2;
            let row = index / 2;
            // 计算卡片矩形：每行高 92px（卡片 82px + 余量），经动画映射到当前帧位置
            let card = motion.rect(Rect::from_min_size(
                Pos2::new(
                    summary.left() + column as f32 * (control_width + gap),
                    control_top + row as f32 * 92.0,
                ),
                Vec2::new(control_width, 82.0),
            ));
            // 若对应的内容对话框已打开，则隐藏该卡（它是弹出动画的“源”卡片，
            // 避免源卡片与展开的对话框同时可见造成视觉重叠）
            // 源卡片必须隐藏到关闭动画彻底结束。否则关闭过程中卡片会提前恢复
            // 可点击状态，与新开窗口的遮罩和图层动画竞争，造成下次打开内容空白。
            let source_card_hidden =
                content_kind.is_some_and(|kind| self.version_content_dialog == Some(kind));
            if source_card_hidden {
                continue;
            }
            // 仅激活态注册点击交互；`then` 把 Option<Response> 作为“是否可交互”的标记
            let response = view.active.then(|| {
                ui.interact(
                    card,
                    Id::unique(("version-control", view.index, index)),
                    Sense::click(),
                )
                .on_hover_cursor(CursorIcon::PointingHand)
            });
            // 悬停状态：未注册交互（非激活态）时恒为 false
            let hovered = response.as_ref().is_some_and(|response| response.hovered());
            let painter = painter_base;
            // 绘制卡片玻璃面板：悬停时底色略亮
            theme::glass_panel(
                painter,
                card,
                theme::corner_radius(),
                theme::GlassSpec::new(
                    if hovered {
                        Color32::from_rgb(28, 46, 32)
                    } else {
                        Color32::from_rgb(21, 34, 25)
                    },
                    theme::GREEN,
                    time,
                    // 每张卡片的流动相位略有差异，让玻璃高光不完全同步
                    18.4 + index as f32 * 0.3,
                )
                .hovered(hovered),
            );
            // 在卡片内裁剪，防止内容溢出圆角
            let card_content = card_clip(painter, card);
            let painter = &card_content;
            // 卡片左侧类别图标
            draw_icon(
                painter,
                icon,
                Rect::from_center_size(
                    Pos2::new(card.left() + 27.0, card.center().y),
                    Vec2::splat(20.0),
                ),
                theme::GREEN_BRIGHT,
            );
            // 卡片标题（类别名）
            theme::bold_text(
                painter,
                Pos2::new(card.left() + 48.0, card.top() + 17.0),
                label,
                10.0,
                theme::ink_soft(),
            );
            // 卡片副标题：按类别显示“N 个 xx”，导出卡固定显示“导出整合包”
            theme::text(
                painter,
                Pos2::new(card.left() + 48.0, card.top() + 39.0),
                match folder {
                    "mods" => format!("{count} 个模组"),
                    "saves" => format!("{count} 个存档"),
                    "shaderpacks" => format!("{count} 个光影包"),
                    "resourcepacks" => format!("{count} 个材质包"),
                    "datapacks" => format!("{count} 个数据包"),
                    // 兜底分支：导出（及其余未知目录标识）
                    _ => "导出整合包".to_owned(),
                },
                8.0,
                theme::muted(),
            );
            // 把目录标识映射为资源下载页的类别（导出卡无对应类别，返回 None）
            let download_category = match folder {
                "mods" => Some(ResourceCategory::Mod),
                "shaderpacks" => Some(ResourceCategory::Shader),
                "resourcepacks" => Some(ResourceCategory::ResourcePack),
                "datapacks" => Some(ResourceCategory::Datapack),
                _ => None,
            };
            // 若该卡有对应下载类别，则在右下角绘制“下载”按钮；否则画一个箭头提示可点击整卡
            let download_clicked = if let Some(category) = download_category {
                // “下载”小按钮区域（卡片右下角，56x25）
                let button = Rect::from_min_size(
                    Pos2::new(card.right() - 72.0, card.top() + 49.0),
                    Vec2::new(56.0, 25.0),
                );
                // 仅激活态可点击；点击后打开资源下载页（携带游戏版本与目录版本标识）
                view.active
                    && settings_button(
                        ui,
                        painter,
                        button,
                        // 按类别分配不同的按钮 id，保证交互状态互不干扰
                        match category {
                            ResourceCategory::Mod => "version-download-mods",
                            ResourceCategory::Shader => "version-download-shaders",
                            ResourceCategory::ResourcePack => "version-download-resourcepacks",
                            ResourceCategory::Datapack => "version-download-datapacks",
                            ResourceCategory::Modpack => "version-download-modpacks",
                        },
                        "下载",
                        false,
                    )
                    && {
                        self.open_resource_download(
                            ui.ctx(),
                            category,
                            selected.game_version.clone(),
                            selected.version.clone(),
                        );
                        true
                    }
            } else {
                // 无下载类别的卡片（存档/导出）：右上角画“→”箭头暗示整卡可点击
                theme::text(
                    painter,
                    Pos2::new(card.right() - 17.0, card.top() + 17.0),
                    "→",
                    9.0,
                    theme::muted(),
                );
                false
            };
            // 点击“下载”按钮时不再响应整卡点击，避免同时打开内容对话框；
            // 否则整卡被点击且处于激活态时，打开对应类别的内容浏览对话框
            if !download_clicked
                && response.as_ref().is_some_and(|response| response.clicked())
                && view.active
                && let Some(kind) = content_kind
            {
                // 以卡片矩形作为弹出动画的“源”，实现从卡片放大展开的过渡
                self.open_version_content_dialog(kind, card);
                ui.ctx().request_repaint();
            }
        }

        let painter = painter_base;
        // 底部操作行仅在激活态绘制（退场动画期间不可操作）
        if view.active {
            // 操作行所在的详情面板矩形（动画映射后的当前帧位置）
            let detail_panel = motion.rect(source_detail_panel);
            // 操作按钮的起始 y：距面板底 57px（按钮高 38px，底部留 19px 余量）
            let actions_y = detail_panel.bottom() - 57.0;
            // “打开目录”按钮（底部左侧）
            let open = Rect::from_min_size(
                Pos2::new(detail_panel.left() + 26.0, actions_y),
                Vec2::new(104.0, 38.0),
            );
            // “设为当前版本”按钮（底部右侧）
            let apply = Rect::from_min_size(
                Pos2::new(detail_panel.right() - 142.0, actions_y),
                Vec2::new(116.0, 38.0),
            );
            // 点击“打开目录”：用系统文件管理器打开该版本的 versions/<版本名> 文件夹
            if settings_button(ui, painter, open, "version-open-folder", "打开目录", false)
                && let Some(game_directory) = self.settings.game_directory.as_ref()
            {
                // 拼出版本目录路径：<游戏目录>/versions/<版本名>
                let path = game_directory.join("versions").join(&selected.version);
                // 按平台选择启动命令：Windows 用 explorer，其余类 Unix 用 xdg-open；
                // 忽略启动失败（spawn 返回 Result），不阻塞 UI
                #[cfg(target_os = "windows")]
                let _ = std::process::Command::new("explorer").arg(path).spawn();
                #[cfg(not(target_os = "windows"))]
                let _ = std::process::Command::new("xdg-open").arg(path).spawn();
            }
            // 判断该版本是否已是当前激活版本（决定按钮文案与是否可用）
            let already_active = self.selected_version == view.index;
            let apply_label = if already_active {
                "当前版本"
            } else {
                "设为当前版本"
            };
            // 点击“设为当前版本”（已是当前版本时按钮禁用）：
            // 切换激活版本、刷新最近会话记录并提示
            if settings_button(
                ui,
                painter,
                apply,
                "version-apply",
                apply_label,
                !already_active,
            ) && !already_active
            {
                // 把详情面板对应的全局索引设为当前激活版本
                self.selected_version = view.index;
                // 最近会话列表依赖当前版本，需要同步刷新
                self.refresh_recent_sessions();
                self.show_toast(format!("已将 {} 设为当前版本", selected.version));
            }
        }
    }
}
