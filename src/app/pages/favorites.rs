//! LauncherApp 的「收藏」页面相关方法实现
//!
//! 本文件为 `crate::app::LauncherApp` 实现「收藏」功能模块的 UI 与交互逻辑，主要包含：
//! 1. **收藏网站网格**（`paint_favorites`）：以卡片网格展示内置与用户自定义收藏站点，
//!    支持点击打开网站、删除自定义站点、虚拟滚动（只绘制可视区内的卡片）。
//! 2. **添加收藏弹窗**（`paint_favorite_editor`）：从"添加"按钮位置形变为居中的对话框，
//!    填写名称/网址/图标后加入收藏；支持自动抓取 favicon 或手动指定图标。
//! 3. **旧版社区页**（`paint_community`，已标记 `#[allow(dead_code)]`）：整合收藏模组列表
//!    与社区动态，并展示当前版本模组环境概览。
//!
//! 在启动器中的角色：这是「收藏」标签页的核心 UI 层。渲染同样采用 `Painter` 直接矢量绘制 +
//! `ui.interact` 自定义交互的方案，与首页/创造等页面风格一致。
//!
//! 依赖的主要外部项（定义于其它模块，此处调用）：
//! - `crate::favorites::BUILTIN_SITES`：内置不可删除的收藏站点列表。
//! - `crate::favorites::site_domain` / `open_site`：解析域名、用系统默认程序打开网址。
//! - `self.settings.favorite_sites`：用户自定义收藏（可增删，持久化到设置）。
//! - `self.favorite_icon_textures`：已下载的站点图标纹理缓存（按 url 索引）。
use super::super::*;

impl LauncherApp {
    /// 绘制「收藏」主页面：顶部标题栏 + 三列网站卡片网格 + 虚拟滚动 + 添加收藏弹窗
    /// 参数：
    /// - ctx：egui 上下文，用于加载图标纹理、请求重绘
    /// - ui：交互句柄，用于 hit-test 卡片/按钮
    /// - painter：矢量绘制器，负责所有卡片外观
    /// - layout：整体页面布局（内容区左右边界、顶栏等）
    /// - time：当前时间戳（秒），用于玻璃面板动画
    /// - motion：卡片进出场动画（位置/透明度插值）
    pub(in crate::app) fn paint_favorites(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
        motion: CardMotion,
    ) {
        let page = Rect::from_min_max(
            Pos2::new(layout.content_left, layout.header.bottom() + 14.0),
            Pos2::new(layout.content_right, layout.screen.bottom() - 22.0),
        );
        let base_painter = painter;
        let header = motion.rect(Rect::from_min_size(
            page.left_top(),
            Vec2::new(page.width(), 92.0),
        ));
        theme::glass_panel(
            painter,
            header,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(20, 32, 24), theme::GREEN, time, 24.5),
        );
        let header_content = card_clip(painter, header);
        theme::bold_text(
            &header_content,
            Pos2::new(header.left() + 24.0, header.top() + 16.0),
            "收藏",
            21.0,
            theme::ink(),
        );
        theme::text(
            &header_content,
            Pos2::new(header.left() + 24.0, header.top() + 47.0),
            "收藏 Minecraft 社区网站，或创建自己的快捷入口",
            8.5,
            theme::muted(),
        );
        let add = Rect::from_min_size(
            Pos2::new(header.right() - 142.0, header.top() + 27.0),
            Vec2::new(116.0, 38.0),
        );
        if settings_button(ui, &header_content, add, "favorite-add", "添加收藏", true) {
            self.open_favorite_editor(add);
        }

        // 确保各站点的图标纹理已加载（必要时异步下载 favicon）
        self.ensure_favorite_icons(ctx);
        // 合并内置站点与用户自定义站点，得到待展示的完整列表
        let sites = self.all_favorite_sites();
        // 内置站点数量：列表前 builtin_count 个为内置、不可删除，用于下方区分删除按钮
        let builtin_count = crate::favorites::BUILTIN_SITES.len();
        // 网格内容区：顶栏高 92 + 间距 12，故从 page.top()+104 开始到页面底部
        let page_base = motion.rect(Rect::from_min_max(
            Pos2::new(page.left(), page.top() + 104.0),
            page.right_bottom(),
        ));
        // 布局参数：3 列、卡片间距 12、单卡高度 112
        let columns = 3;
        let gap = 12.0;
        let card_height = 112.0;
        // 单列宽度 = (总宽 - 列间空隙) / 列数，保证三列等宽并留间隙
        let card_width = (page_base.width() - gap * (columns - 1) as f32) / columns as f32;
        // 行高 = 卡片高 + 间距，用于按行列计算每张卡片位置
        let row_height = card_height + gap;
        // 网格内边距：顶部留 16，其余为可视高度（.max(1.0) 防止除零/负高）
        let grid_top = page_base.top() + 16.0;
        let grid_height = (page_base.bottom() - grid_top).max(1.0);
        // 内容总高度 = 行数(向上取整) * 行高；最大可滚动距离 = 内容高 - 可视高（负则取 0）
        let content_height = sites.len().div_ceil(columns) as f32 * row_height;
        let max_scroll = (content_height - grid_height).max(0.0);
        // "添加后"的预览布局：若再新增一个站点，它应排在第几个位置。
        // next_row = 当前站点数 / 列数（整数除法）= 新增站点所在的行；
        // 新增后内容总高度与可滚动距离也相应 +1 行，用于弹窗关闭时把视图滚动到新卡片处。
        let next_row = sites.len() / columns;
        let next_content_height = (sites.len() + 1).div_ceil(columns) as f32 * row_height;
        let next_scroll = (next_content_height - grid_height).max(0.0);
        // 记录弹窗关闭动画的目标滚动位置与新卡片目标矩形，供形变/定位使用
        self.favorite_editor_target_scroll = next_scroll;
        self.favorite_editor_card_target = motion.rect(Rect::from_min_size(
            Pos2::new(
                page_base.left() + (sites.len() % columns) as f32 * (card_width + gap),
                grid_top + next_row as f32 * row_height - next_scroll,
            ),
            Vec2::new(card_width, card_height),
        ));
        // 滚动处理：弹窗打开时锁定滚动范围；否则仅当鼠标悬停在网格区域时，
        // 用平滑滚轮增量(向上为正)调整 community_scroll（乘 1.25 调节灵敏度），并夹到 [0, max_scroll]
        if self.favorite_editor_open {
            self.community_scroll = self.community_scroll.clamp(0.0, max_scroll);
        } else if ui.ctx().input(|input| {
            input
                .pointer
                .hover_pos()
                .is_some_and(|pointer| page_base.contains(pointer))
        }) {
            self.community_scroll -= ui.ctx().input(|input| input.smooth_scroll_delta.y) * 1.25;
            self.community_scroll = self.community_scroll.clamp(0.0, max_scroll);
        }
        // 视口：在 page_base 基础上上下各内缩 2px 作为裁剪区，只绘制可见卡片
        let viewport = page_base.shrink2(Vec2::new(0.0, 2.0));
        let list_painter = base_painter.with_clip_rect(viewport);
        // 遍历所有站点，按 列=index%列数、行=index/列数 计算网格坐标
        for (index, site) in sites.iter().enumerate() {
            let column = index % columns;
            let row = index / columns;
            // 卡片矩形：水平按列推进，垂直按行推进，再减去当前滚动偏移
            let card = motion.rect(Rect::from_min_size(
                Pos2::new(
                    page_base.left() + column as f32 * (card_width + gap),
                    grid_top + row as f32 * row_height - self.community_scroll,
                ),
                Vec2::new(card_width, card_height),
            ));
            // 虚拟滚动：卡片完全在视口上方或下方时跳过绘制，提升大列表性能
            if card.bottom() < viewport.top() || card.top() > viewport.bottom() {
                continue;
            }
            let response = ui
                .interact(
                    card,
                    Id::unique(("favorite-site", &site.url)),
                    Sense::click(),
                )
                .on_hover_cursor(CursorIcon::PointingHand);
            let card_painter = card_clip(&list_painter, card);
            theme::glass_panel(
                &card_painter,
                card,
                theme::corner_radius(),
                theme::GlassSpec::new(
                    if response.hovered() {
                        Color32::from_rgb(29, 48, 34)
                    } else {
                        Color32::from_rgb(20, 33, 24)
                    },
                    theme::GREEN,
                    time,
                    25.0 + index as f32 * 0.2,
                )
                .hovered(response.hovered()),
            );
            let icon = Rect::from_center_size(
                Pos2::new(card.left() + 42.0, card.top() + 44.0),
                Vec2::splat(46.0),
            );
            if let Some(texture) = self.favorite_icon_textures.get(&site.url) {
                paint_rounded_image(
                    &card_painter,
                    icon,
                    theme::corner_radius() as f32,
                    texture,
                    texture.size(),
                );
            } else {
                theme::fill_round(
                    &card_painter,
                    icon,
                    theme::corner_radius(),
                    theme::alpha(theme::GREEN, 24),
                );
                theme::center_text(
                    &card_painter,
                    icon.center(),
                    site.title
                        .chars()
                        .next()
                        .unwrap_or('W')
                        .to_uppercase()
                        .to_string(),
                    17.0,
                    theme::GREEN_BRIGHT,
                    true,
                );
            }
            theme::bold_text(
                &card_painter,
                Pos2::new(card.left() + 75.0, card.top() + 24.0),
                fit_text_to_width(
                    &card_painter,
                    &site.title,
                    theme::bold_font(12.0),
                    card.width() - 100.0,
                ),
                12.0,
                theme::ink(),
            );
            theme::text(
                &card_painter,
                Pos2::new(card.left() + 75.0, card.top() + 48.0),
                crate::favorites::site_domain(&site.url),
                8.0,
                theme::muted(),
            );
            theme::text(
                &card_painter,
                Pos2::new(card.left() + 18.0, card.bottom() - 28.0),
                "打开网站",
                8.2,
                if response.hovered() {
                    theme::GREEN_BRIGHT
                } else {
                    Color32::from_rgb(157, 204, 151)
                },
            );
            theme::text(
                &card_painter,
                Pos2::new(card.right() - 20.0, card.bottom() - 30.0),
                "→",
                9.0,
                theme::muted(),
            );
            // 仅对用户自定义站点（index >= 内置数量）显示删除按钮
            if index >= builtin_count {
                // 右上角圆形删除按钮（关闭图标）
                let remove = Rect::from_center_size(
                    Pos2::new(card.right() - 18.0, card.top() + 18.0),
                    Vec2::splat(24.0),
                );
                let remove_response = ui
                    .interact(
                        remove,
                        Id::unique(("favorite-remove", &site.url)),
                        Sense::click(),
                    )
                    .on_hover_cursor(CursorIcon::PointingHand);
                theme::fill_round(
                    &card_painter,
                    remove,
                    theme::corner_radius(),
                    if remove_response.hovered() {
                        theme::white_alpha(18)
                    } else {
                        theme::black_alpha(48)
                    },
                );
                draw_icon(
                    &card_painter,
                    NavIcon::Close,
                    remove.shrink(6.0),
                    theme::muted(),
                );
                if remove_response.clicked() {
                    // 从设置中移除该收藏（按 url 精确匹配后保留其余项）
                    self.settings
                        .favorite_sites
                        .retain(|favorite| favorite.url != site.url);
                    // 同步清理图标纹理缓存与"已尝试下载"记录，避免内存泄漏/重复下载
                    self.favorite_icon_textures.remove(&site.url);
                    self.favorite_icon_attempted
                        .retain(|known| known != &site.url);
                    // 重新夹取滚动位置：站点数 -1 后可能内容变短，防止滚到空白区
                    self.community_scroll = self.community_scroll.clamp(
                        0.0,
                        (sites.len().saturating_sub(1).div_ceil(columns) as f32 * row_height
                            - grid_height)
                            .max(0.0),
                    );
                    ui.ctx().request_repaint();
                    // 跳过本卡片剩余绘制（已被删除），直接进入下一张
                    continue;
                }
            }
            // 点击卡片主体：用系统默认浏览器打开该站点网址；失败则弹出 toast 提示
            if response.clicked()
                && let Err(error) = crate::favorites::open_site(&site.url)
            {
                self.show_toast(format!("无法打开网站：{error}"));
            }
        }
        self.paint_favorite_editor(ctx, layout.screen, time);
    }

    /// 绘制"添加收藏"弹窗：从触发按钮位置形变为居中对话框（形态/缩放/透明度插值动画）
    /// 参数：
    /// - ctx：egui 上下文，用于创建前景 Area、应用图层变换
    /// - screen：整个屏幕矩形，用于背景遮罩与对话框居中
    /// - time：当前时间戳，用于玻璃面板动画
    pub(in crate::app) fn paint_favorite_editor(
        &mut self,
        ctx: &egui::Context,
        screen: Rect,
        time: f64,
    ) {
        // 弹窗完全关闭（未打开且动画值已衰减到约 0）时直接跳过绘制，避免空开销
        if !self.favorite_editor_open && self.favorite_editor_value <= 0.002 {
            return;
        }
        // 目标对话框矩形（居中、固定尺寸）
        let dialog = favorite_editor_rect(screen);
        // 动画起点矩形：若记录了触发按钮位置则用它，否则退化为对话框本身
        let source = if self.favorite_editor_source == Rect::NOTHING {
            dialog
        } else {
            self.favorite_editor_source
        };
        // 动画终点矩形：打开时指向对话框；关闭且无显式 target 时回退到 source
        let target = if self.favorite_editor_target == Rect::NOTHING {
            if self.favorite_editor_open {
                dialog
            } else {
                source
            }
        } else {
            self.favorite_editor_target
        };
        // 归一化进度：favorite_editor_value 在 0(关)~1(开) 之间，放宽到 [-0.05,1.08] 留缓冲
        let progress = self.favorite_editor_value.clamp(-0.05, 1.08);
        // raw_travel：打开时随 progress 从 0→1；关闭时反向 1→0
        let raw_travel = if self.favorite_editor_open {
            progress.clamp(0.0, 1.0)
        } else {
            1.0 - progress.clamp(0.0, 1.0)
        };
        // 平滑插值（smoothstep）：使动画起止更柔和，避免线性突兀
        let travel = raw_travel * raw_travel * (3.0 - 2.0 * raw_travel);
        // 形变方向：打开时从 source 到 dialog；关闭时从 dialog 到 target（反向）
        let from = if self.favorite_editor_open {
            source
        } else {
            dialog
        };
        let to = if self.favorite_editor_open {
            dialog
        } else {
            target
        };
        // 按 travel 在 from/to 之间线性插值出当前帧的矩形（min/max 角点分别插值）
        let morph_rect = Rect::from_min_max(
            from.min + (to.min - from.min) * travel,
            from.max + (to.max - from.max) * travel,
        );
        // 内容文字透明度随进度淡入淡出
        let content_opacity = card_text_opacity(progress);
        // 起始缩放 = 起点相对对话框的几何平均比例，夹到 [0.08, 0.55] 防止过小/过大；
        // 再随 travel 从 start_scale 过渡到 1.0（满尺寸）
        let start_scale = ((from.width() / dialog.width()) * (from.height() / dialog.height()))
            .sqrt()
            .clamp(0.08, 0.55);
        let scale = start_scale + (1.0 - start_scale) * travel;
        // 构造平移+缩放变换：把对话框中心对齐到形变矩形中心，整体缩放 scale
        let transform =
            egui::emath::TSTransform::new(morph_rect.center() - dialog.center() * scale, scale);

        egui::Area::new(Id::unique("favorite-editor-backdrop"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .movable(false)
            .show(ctx, |ui| {
                ui.set_min_size(screen.size());
                let painter = ui.painter();
                painter.rect_filled(
                    screen,
                    0,
                    Color32::from_black_alpha((22.0 * progress.clamp(0.0, 1.0)) as u8),
                );
                if self.favorite_editor_open || progress >= 0.16 {
                    theme::glass_panel(
                        painter,
                        morph_rect,
                        theme::corner_radius(),
                        theme::GlassSpec::new(
                            Color32::from_rgb(21, 34, 25),
                            theme::GREEN,
                            time,
                            26.0,
                        )
                        .backdrop_blur(18.0),
                    );
                } else {
                    let fade = (progress / 0.16).clamp(0.0, 1.0);
                    theme::fill_round(
                        painter,
                        morph_rect,
                        theme::corner_radius(),
                        Color32::from_black_alpha((28.0 * fade) as u8),
                    );
                }
            });

        let mut editor_layer =
            egui::LayerId::new(egui::Order::Foreground, Id::unique("favorite-editor-layer"));
        egui::Area::new(Id::unique("favorite-editor"))
            .order(egui::Order::Foreground)
            .fixed_pos(dialog.min)
            .movable(false)
            .show(ctx, |ui| {
                ui.set_min_size(dialog.size());
                editor_layer = ui.layer_id();
                let mut painter = ui.painter().clone();
                painter.set_opacity(content_opacity);
                let content = card_clip(&painter, dialog);
                theme::bold_text(
                    &content,
                    Pos2::new(dialog.left() + 22.0, dialog.top() + 18.0),
                    "添加收藏网站",
                    18.0,
                    theme::ink(),
                );
                theme::text(
                    &content,
                    Pos2::new(dialog.left() + 22.0, dialog.top() + 46.0),
                    "自动读取网站图标，也可指定图片 URL 或本地图片",
                    8.0,
                    theme::muted(),
                );
                // 两个单行输入框：上=网站名称，下=网址（两者均左右留 22 边距、宽度=对话框宽-44）
                let title = Rect::from_min_size(
                    Pos2::new(dialog.left() + 22.0, dialog.top() + 78.0),
                    Vec2::new(dialog.width() - 44.0, 34.0),
                );
                let url = Rect::from_min_size(
                    Pos2::new(dialog.left() + 22.0, dialog.top() + 124.0),
                    Vec2::new(dialog.width() - 44.0, 34.0),
                );
                // 统一绘制名称/网址输入框的底色与圆角描边
                for field in [title, url] {
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
                        theme::white_alpha(28),
                    );
                }
                // 图标行：左侧为图标 URL 输入框（留空=自动），右侧两个按钮："选择图片"(本地文件) / "自动图标"(清空)
                let icon = Rect::from_min_size(
                    Pos2::new(dialog.left() + 22.0, dialog.top() + 170.0),
                    Vec2::new(dialog.width() - 220.0, 34.0),
                );
                let choose_icon = Rect::from_min_size(
                    Pos2::new(icon.right() + 8.0, dialog.top() + 170.0),
                    Vec2::new(82.0, 34.0),
                );
                let auto_icon = Rect::from_min_size(
                    Pos2::new(choose_icon.right() + 8.0, dialog.top() + 170.0),
                    Vec2::new(78.0, 34.0),
                );
                theme::fill_round(
                    &content,
                    icon,
                    theme::corner_radius(),
                    Color32::from_rgb(24, 38, 28),
                );
                theme::stroke_round(
                    &content,
                    icon,
                    theme::corner_radius(),
                    1.0,
                    theme::white_alpha(28),
                );
                ui.put(
                    title.shrink2(Vec2::new(10.0, 5.0)),
                    egui::TextEdit::singleline(&mut self.favorite_title_input)
                        .frame(egui::Frame::NONE)
                        .font(theme::font(8.8))
                        .text_color(theme::adaptive_text_color_at(
                            &content,
                            title.center(),
                            theme::ink(),
                            8.8,
                        ))
                        .hint_text("网站名称（可留空）"),
                );
                ui.put(
                    url.shrink2(Vec2::new(10.0, 5.0)),
                    egui::TextEdit::singleline(&mut self.favorite_url_input)
                        .frame(egui::Frame::NONE)
                        .font(theme::font(8.8))
                        .text_color(theme::adaptive_text_color_at(
                            &content,
                            url.center(),
                            theme::ink(),
                            8.8,
                        ))
                        .hint_text("https://example.com"),
                );
                ui.put(
                    icon.shrink2(Vec2::new(10.0, 5.0)),
                    egui::TextEdit::singleline(&mut self.favorite_icon_input)
                        .frame(egui::Frame::NONE)
                        .font(theme::font(8.0))
                        .text_color(theme::adaptive_text_color_at(
                            &content,
                            icon.center(),
                            theme::ink(),
                            8.0,
                        ))
                        .hint_text("图标 URL（留空自动）"),
                );
                // "选择图片"：弹出系统文件选择框，选中的本地图片路径写入图标输入框
                if self.favorite_editor_open
                    && settings_button(
                        ui,
                        &content,
                        choose_icon,
                        "favorite-editor-icon-file",
                        "选择图片",
                        false,
                    )
                    && let Some(path) = pick_image_file(false)
                {
                    self.favorite_icon_input = path.display().to_string();
                    self.favorite_editor_error = None;
                }
                // "自动图标"：清空图标输入，恢复为按网站 favicon 自动获取
                if self.favorite_editor_open
                    && settings_button(
                        ui,
                        &content,
                        auto_icon,
                        "favorite-editor-icon-auto",
                        "自动图标",
                        false,
                    )
                {
                    self.favorite_icon_input.clear();
                    self.favorite_editor_error = None;
                }
                theme::text(
                    &content,
                    Pos2::new(dialog.left() + 22.0, dialog.top() + 211.0),
                    "图标留空时优先读取对应网站 favicon，支持 PNG、JPG、WebP 和 ICO",
                    7.3,
                    theme::muted(),
                );
                if let Some(error) = self.favorite_editor_error.as_ref() {
                    theme::text(
                        &content,
                        Pos2::new(dialog.left() + 22.0, dialog.top() + 231.0),
                        fit_text_to_width(&content, error, theme::font(7.8), dialog.width() - 44.0),
                        7.8,
                        Color32::from_rgb(208, 172, 102),
                    );
                }
                let cancel = Rect::from_min_size(
                    Pos2::new(dialog.right() - 178.0, dialog.bottom() - 54.0),
                    Vec2::new(70.0, 36.0),
                );
                let add = Rect::from_min_size(
                    Pos2::new(dialog.right() - 98.0, dialog.bottom() - 54.0),
                    Vec2::new(76.0, 36.0),
                );
                // 底部右侧两个按钮：取消（关闭弹窗、丢弃输入）/ 添加（校验并加入收藏）
                if self.favorite_editor_open
                    && settings_button(
                        ui,
                        &content,
                        cancel,
                        "favorite-editor-cancel",
                        "取消",
                        false,
                    )
                {
                    self.cancel_favorite_editor();
                }
                if self.favorite_editor_open
                    && settings_button(ui, &content, add, "favorite-editor-add", "添加", true)
                {
                    // 加入收藏（内部会做校验，失败则设置 favorite_editor_error 显示在弹窗内）
                    self.add_favorite_site(self.favorite_editor_card_target);
                    ctx.request_repaint();
                }
            });
        ctx.transform_layer_shapes(editor_layer, transform);
    }

    /// 【旧版社区页，已弃用】整合展示：当前版本的收藏模组列表 / 社区动态，以及侧栏的模组环境概览
    /// 注意：此函数当前未被调用（标 `#[allow(dead_code)]`），保留仅作历史/兼容参考。
    /// 参数同 `paint_favorites`：ctx 上下文、ui 交互、painter 绘制、layout 布局、time 动画时间、motion 进出场动画。
    #[allow(dead_code)]
    pub(in crate::app) fn paint_community(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
        motion: CardMotion,
    ) {
        let page = Rect::from_min_max(
            Pos2::new(layout.content_left, layout.header.bottom() + 14.0),
            Pos2::new(layout.content_right, layout.screen.bottom() - 22.0),
        );
        let data_directory = self.version_data_directory();
        let side_width = (page.width() * 0.28).clamp(260.0, 330.0);
        let source_main = Rect::from_min_max(
            page.left_top(),
            Pos2::new(page.right() - side_width - 14.0, page.bottom()),
        );
        let source_side = Rect::from_min_max(
            Pos2::new(source_main.right() + 14.0, page.top()),
            page.right_bottom(),
        );
        let main = motion.rect(source_main);
        let side = motion.rect(source_side);
        let community_base = painter;
        theme::glass_panel(
            painter,
            main,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(18, 28, 22), theme::GREEN, time, 24.0),
        );
        theme::glass_panel(
            painter,
            side,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(19, 30, 23), theme::GREEN, time, 25.0),
        );
        let main_content = card_clip(painter, main);
        let painter = &main_content;
        theme::bold_text(
            painter,
            Pos2::new(main.left() + 24.0, main.top() + 20.0),
            "收藏与模组",
            21.0,
            theme::ink(),
        );
        theme::text(
            painter,
            Pos2::new(main.left() + 24.0, main.top() + 48.0),
            "管理收藏内容与当前版本的模组",
            8.5,
            theme::muted(),
        );
        for (index, label) in ["收藏模组", "收藏动态"].into_iter().enumerate() {
            let tab = Rect::from_min_size(
                Pos2::new(main.left() + 24.0 + index as f32 * 105.0, main.top() + 73.0),
                Vec2::new(98.0, 31.0),
            );
            let response = ui
                .interact(tab, Id::unique(("community-tab", index)), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand);
            let selected = self.community_tab == index;
            theme::fill_round(
                painter,
                tab,
                theme::corner_radius(),
                if selected {
                    theme::alpha(theme::GREEN, 26)
                } else if response.hovered() {
                    theme::white_alpha(10)
                } else {
                    theme::black_alpha(32)
                },
            );
            theme::center_text(
                painter,
                tab.center(),
                label,
                8.5,
                if selected {
                    theme::ink()
                } else {
                    theme::muted()
                },
                selected,
            );
            if response.clicked() {
                self.community_tab = index;
                self.community_scroll = 0.0;
            }
        }

        let source_list = Rect::from_min_max(
            Pos2::new(source_main.left() + 18.0, source_main.top() + 118.0),
            Pos2::new(source_main.right() - 18.0, source_main.bottom() - 18.0),
        );
        let list = Rect::from_min_max(
            Pos2::new(main.left() + 18.0, main.top() + 118.0),
            Pos2::new(main.right() - 18.0, main.bottom() - 18.0),
        );
        // 根据当前子标签构建列表行：
        // - tab 0 = 收藏模组：扫描版本目录下的已安装模组文件，得到 (名称, 格式化大小, 无新闻索引)
        // - tab 1 = 收藏动态：取前 8 条新闻，得到 (标题, "分类 · 日期", 新闻在原列表中的下标)
        let rows = if self.community_tab == 0 {
            data_directory
                .as_ref()
                .map(|directory| installed_mod_files(directory))
                .unwrap_or_default()
                .into_iter()
                .map(|(name, size)| (name, format_size(size), None))
                .collect::<Vec<_>>()
        } else {
            self.news_items
                .iter()
                .take(8)
                .enumerate()
                .map(|(index, news)| {
                    (
                        news.title.clone(),
                        format!("{} · {}", news.category, news.date),
                        Some(index),
                    )
                })
                .collect::<Vec<_>>()
        };
        // 列表滚动布局：行高 48，内容高 = 行数*行高，最大滚动 = 内容高 - 可视高
        let row_height = 48.0;
        let content_height = rows.len() as f32 * row_height;
        let max_scroll = (content_height - list.height()).max(0.0);
        // 鼠标悬停在列表区时，用平滑滚轮增量调整 community_scroll（灵敏度系数 1.25）
        if ui.ctx().input(|input| {
            input
                .pointer
                .hover_pos()
                .is_some_and(|pointer| list.contains(pointer))
        }) {
            self.community_scroll -= ui.ctx().input(|input| input.smooth_scroll_delta.y) * 1.25;
        }
        self.community_scroll = self.community_scroll.clamp(0.0, max_scroll);
        let list_painter = painter.clone();
        // 列表为空时显示提示文案（依当前 tab 区分）
        if rows.is_empty() {
            theme::center_text(
                &list_painter,
                Pos2::new(list.center().x, list.top() + 80.0),
                if self.community_tab == 0 {
                    "收藏中还没有模组"
                } else {
                    "正在获取收藏动态"
                },
                10.0,
                theme::muted(),
                false,
            );
        }
        // 虚拟滚动：仅绘制可视范围内的行。每行 42 高（< 48 行距，留 6px 间隔）
        for (index, (title, detail, news_index)) in rows.iter().enumerate() {
            let source_y = source_list.top() + index as f32 * row_height - self.community_scroll;
            // 行完全在可视区上方或下方时跳过
            if source_y + 42.0 < source_list.top() || source_y > source_list.bottom() {
                continue;
            }
            let row = motion.rect(Rect::from_min_size(
                Pos2::new(source_list.left(), source_y),
                Vec2::new(source_list.width(), 42.0),
            ));
            let response = ui
                .interact(
                    row,
                    Id::unique(("community-row", self.community_tab, index)),
                    Sense::click(),
                )
                .on_hover_cursor(CursorIcon::PointingHand);
            let row_content = card_clip(&list_painter, row);
            if response.hovered() {
                theme::fill_round(
                    &row_content,
                    row,
                    theme::corner_radius(),
                    theme::white_alpha(9),
                );
            }
            let icon = Rect::from_center_size(
                Pos2::new(row.left() + 22.0, row.center().y),
                Vec2::splat(28.0),
            );
            theme::fill_round(
                &row_content,
                icon,
                theme::corner_radius(),
                theme::alpha(theme::GREEN, 22),
            );
            if self.community_tab == 1 {
                draw_icon(
                    &row_content,
                    NavIcon::Sparkles,
                    icon.shrink(7.0),
                    theme::GREEN_BRIGHT,
                );
            } else {
                paint_block(
                    &row_content,
                    icon.center(),
                    18.0,
                    Color32::from_rgb(125, 205, 101),
                    Color32::from_rgb(90, 66, 47),
                    Color32::from_rgb(73, 145, 69),
                );
            }
            theme::bold_text(
                &row_content,
                Pos2::new(row.left() + 42.0, row.top() + 7.0),
                ellipsize_end(title, 42),
                9.4,
                theme::ink_soft(),
            );
            theme::text(
                &row_content,
                Pos2::new(row.left() + 42.0, row.top() + 25.0),
                ellipsize_end(detail, 50),
                7.4,
                theme::muted(),
            );
            // 仅动态行(news_index 有值)可点击：将其在 news_items 中的下标存入 self.news，供详情页读取
            if response.clicked()
                && let Some(news_index) = news_index
            {
                self.news = Some(*news_index);
            }
        }

        let painter = community_base;
        let side_content = card_clip(painter, side);
        let painter = &side_content;
        theme::bold_text(
            painter,
            Pos2::new(side.left() + 20.0, side.top() + 20.0),
            "模组环境",
            16.0,
            theme::ink(),
        );
        // 统计当前版本目录下已安装模组数量（无目录则按 0 计）
        let mod_count = self
            .version_data_directory()
            .as_ref()
            .map(|directory| installed_mod_files(directory).len())
            .unwrap_or(0);
        // 左侧圆形徽标内显示模组数量
        painter.circle_filled(
            Pos2::new(side.left() + 26.0, side.top() + 72.0),
            19.0,
            theme::alpha(theme::GREEN, 28),
        );
        theme::center_text(
            painter,
            Pos2::new(side.left() + 26.0, side.top() + 72.0),
            mod_count.to_string(),
            13.0,
            theme::GREEN_BRIGHT,
            true,
        );
        theme::text(
            painter,
            Pos2::new(side.left() + 52.0, side.top() + 56.0),
            "已安装模组",
            8.2,
            theme::muted(),
        );
        theme::bold_text(
            painter,
            Pos2::new(side.left() + 52.0, side.top() + 71.0),
            format!("{} · {}", self.version().version, self.version().loader),
            10.5,
            theme::ink_soft(),
        );
        painter.line_segment(
            [
                Pos2::new(side.left() + 20.0, side.top() + 108.0),
                Pos2::new(side.right() - 20.0, side.top() + 108.0),
            ],
            Stroke::new(1.0, theme::line()),
        );
        // 模组目录路径 = 版本数据目录 / "mods"
        let mods_dir = self
            .version_data_directory()
            .as_ref()
            .map(|base| base.join("mods"));
        let open_mods = Rect::from_min_size(
            Pos2::new(side.left() + 20.0, side.top() + 126.0),
            Vec2::new(side.width() - 40.0, 36.0),
        );
        // "打开模组目录"：确保目录存在后用系统文件管理器打开（便于手动增删模组）
        if settings_button(
            ui,
            painter,
            open_mods,
            "community-open-mods",
            "打开模组目录",
            false,
        ) && let Some(path) = mods_dir
        {
            let _ = std::fs::create_dir_all(&path);
            open_path(&path);
        }
        let refresh = Rect::from_min_size(
            Pos2::new(side.left() + 20.0, side.top() + 170.0),
            Vec2::new(side.width() - 40.0, 36.0),
        );
        if settings_button(
            ui,
            painter,
            refresh,
            "community-refresh",
            "刷新模组列表",
            false,
        ) {
            self.show_toast("模组列表已刷新");
        }
        theme::wrapped_text(
            painter,
            Pos2::new(side.left() + 20.0, side.top() + 225.0),
            "收藏内容与模组管理已在同一页面整合。安装模组后，刷新即可在上方列表中查看。",
            8.2,
            theme::muted(),
            side.width() - 40.0,
        );
        let _ = ctx;
    }
}
