//! # `src/app/shell.rs`
//!
//! **功能域：** 启动器应用外壳、页面状态、交互流程和原生 egui 绘制。
//!
//! 应用最外层绘制链。组合背景、标题栏、侧边栏、页头、页面内容、弹窗和指针光效。
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `render`
//! - `handle_window_frame`
//! - `paint_titlebar`
//! - `paint_rail`
//! - `paint_nav_tooltip`
//! - `paint_header`
//! - `paint_version_trigger`
//! - `handle_outside_click`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use super::*;

impl LauncherApp {
    /// 实现 渲染 相关的业务流程。
    ///
    /// # 参数
    /// - `ctx`：由调用方传入的ctx。
    /// - `ui`：由调用方传入的ui。
    /// - `screen`：由调用方传入的screen。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn render(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, screen: Rect) {
        self.settings.sync_hero_title_char_styles();
        let hero_char_count = self.settings.hero_title.chars().count();
        self.hero_title_selected_char = self
            .hero_title_selected_char
            .min(hero_char_count.saturating_sub(1));
        theme::configure_effects(
            ctx,
            self.settings.glass_transparency,
            self.settings.card_edge_clarity,
            self.settings.pointer_brightness,
            self.settings.pointer_radius,
            self.settings.corner_radius,
            self.settings.appearance_mode,
        );
        configure_animation_strength(
            (self.settings.animation_level.strength() * 100.0).round() as u16
        );
        let animation_strength = animation_strength();
        let time = if self.screenshot_requested || !self.settings.animations_enabled() {
            2.2
        } else {
            self.started_at.elapsed().as_secs_f64() * animation_strength as f64
        };
        let painter = ui.painter().clone();
        if self.forced_update_release.is_some() {
            theme::begin_text_frame(screen, false);
            painter.rect_filled(screen, 0, theme::background());
            paint_ambient_background(&painter, screen, time);
            self.paint_forced_update(ctx, ui, screen, time);
            self.sync_window_border_color(&painter, screen);
            theme::paint_window_mask(&painter, screen);
            return;
        }
        let mut layout = Layout::new(screen);
        let version_label = self.version_selector_label(self.version());
        let max_trigger_width = (screen.width() * 0.34).clamp(220.0, 380.0);
        let version_label = fit_text_to_width(
            &painter,
            &version_label,
            theme::bold_font(10.5),
            (max_trigger_width - 86.0).max(100.0),
        );
        let version_label_width = painter
            .layout_no_wrap(
                version_label.clone(),
                theme::bold_font(10.5),
                theme::ink_soft(),
            )
            .size()
            .x;
        let trigger_width = (50.0 + version_label_width + 36.0)
            .ceil()
            .clamp(170.0, max_trigger_width);
        layout.version_trigger = Rect::from_min_size(
            Pos2::new(
                layout.version_trigger.right() - trigger_width,
                layout.version_trigger.top(),
            ),
            Vec2::new(trigger_width, layout.version_trigger.height()),
        );
        self.handle_outside_click(ctx, &layout);

        let has_background_image = self.hero_background_texture.is_some();
        theme::begin_text_frame(screen, has_background_image);
        painter.rect_filled(screen, 0, theme::background());
        if let Some(background) = self.hero_background_texture.as_ref() {
            paint_rounded_image(&painter, screen, 0.0, background, background.size());
            painter.rect_filled(screen, 0, Color32::from_black_alpha(52));
        }
        paint_ambient_background(&painter, screen, time);
        self.paint_pointer_light(ctx, &painter, screen);
        self.handle_window_frame(ctx, ui, &layout);
        if let Some(previous_page) = self.previous_page {
            let outgoing = CardMotion {
                screen: layout.screen,
                direction: self.page_transition_direction,
                progress: self.page_transition_progress,
                outgoing: true,
            };
            let mut outgoing_painter = painter.clone();
            outgoing_painter.set_opacity(outgoing_page_opacity(self.page_transition_progress));
            self.paint_page_body(
                ctx,
                ui,
                &outgoing_painter,
                &layout,
                time,
                previous_page,
                outgoing,
            );
            let incoming = CardMotion {
                screen: layout.screen,
                direction: self.page_transition_direction,
                progress: self.page_transition_progress,
                outgoing: false,
            };
            self.paint_page_body(ctx, ui, &painter, &layout, time, self.page, incoming);
        } else {
            self.paint_page_body(
                ctx,
                ui,
                &painter,
                &layout,
                time,
                self.page,
                CardMotion {
                    screen: layout.screen,
                    direction: 1.0,
                    progress: 1.0,
                    outgoing: false,
                },
            );
        }
        self.paint_titlebar(ctx, ui, &painter, &layout, time);
        self.paint_rail(ctx, ui, &painter, &layout, time);
        self.paint_header(ctx, ui, &painter, &layout, time);
        if self.version_folder_menu_open && self.page == Page::Versions {
            self.paint_version_folder_menu(ctx, &layout, time);
        }
        if self.launch_settings_open || self.launch_popup_value > 0.002 {
            let card = launch_settings_card_rect(&layout);
            let layer = egui::LayerId::new(
                egui::Order::Foreground,
                Id::unique("launch-settings-popup-layer"),
            );
            let scale = 0.82 + 0.18 * self.launch_popup_value.clamp(-0.05, 1.12);
            let anchor = card.right_bottom().to_vec2();
            let transform = egui::emath::TSTransform::new(anchor - anchor * scale, scale);
            let mut popup_painter = painter.clone().with_layer_id(layer);
            popup_painter.set_opacity(popup_opacity(
                self.launch_popup_value,
                self.launch_settings_open,
            ));
            self.paint_launch_settings_card(ui, &popup_painter, &layout, time);
            ctx.transform_layer_shapes(layer, transform);
        }
        if self.news.is_none() {
            self.paint_menus(ctx, &layout, time);
        } else {
            self.paint_news_dialog(ctx, ui, &painter, screen, time);
        }
        if let Some(kind) = self.version_content_dialog {
            self.paint_version_content_dialog(ctx, screen, time, kind);
        }
        if self.java_picker_target.is_some() {
            self.paint_java_picker(ctx, screen, time);
        }

        if self.official_browser_open {
            self.paint_official_browser(ui, &painter, screen, time);
        }
        if self.loader_dialog_open {
            self.paint_loader_dialog(ui, &painter, screen, time);
        }
        self.paint_install_progress(&painter, screen, time);
        self.paint_modpack_import_progress(&painter, screen, time);
        self.paint_account_name_dialog(ctx, screen, time);
        self.paint_version_name_dialog(ctx, screen, time);
        self.paint_download_error_log(ctx, screen, time);
        self.paint_toast(&painter, screen);
        if self.pointer_glow_strength > 0.01 {
            theme::pointer_glow_overlay(
                &painter,
                self.pointer_glow_position,
                self.pointer_glow_strength,
                screen,
            );
        }
        self.sync_window_border_color(&painter, screen);
        theme::paint_window_mask(&painter, screen);
    }

    /// 同步 DWM 顶部 1px 边框与状态栏背景色。
    fn sync_window_border_color(&mut self, painter: &Painter, screen: Rect) {
        let color = theme::color_at_position(painter, Pos2::new(screen.center().x, 2.0));
        if self.window_border_color != Some(color) && crate::set_window_border_color(color) {
            self.window_border_color = Some(color);
        }
    }

    /// 实现 handle窗口frame 相关的业务流程。
    ///
    /// # 参数
    /// - `ctx`：由调用方传入的ctx。
    /// - `ui`：由调用方传入的ui。
    /// - `layout`：由调用方传入的布局。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn handle_window_frame(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        layout: &Layout,
    ) {
        let drag_rect = Rect::from_min_max(
            Pos2::ZERO,
            Pos2::new(layout.screen.right() - 145.0, TITLEBAR_HEIGHT),
        );
        let drag = ui.interact(drag_rect, Id::unique("native-window-drag"), Sense::drag());
        if drag.drag_started() {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
    }

    /// 实现 绘制titlebar 相关的业务流程。
    ///
    /// # 参数
    /// - `ctx`：由调用方传入的ctx。
    /// - `ui`：由调用方传入的ui。
    /// - `painter`：由调用方传入的painter。
    /// - `layout`：由调用方传入的布局。
    /// - `time`：由调用方传入的time。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn paint_titlebar(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
    ) {
        theme::frosted_glass_panel(
            painter,
            layout.titlebar,
            // 标题栏不单独裁圆角：Windows 原生窗口轮廓是唯一外轮廓，避免两套 R 角
            // 相交后在左上/右上出现尖角。
            0,
            theme::GlassSpec::new(Color32::from_rgb(242, 246, 242), theme::GREEN, time, 0.2)
                .backdrop_blur(24.0)
                .top_highlight(0.0)
                .card_edge(false),
        );
        theme::horizontal_fade(
            painter,
            Rect::from_min_max(
                Pos2::new(
                    layout.titlebar.left() + 86.0,
                    layout.titlebar.bottom() - 1.0,
                ),
                Pos2::new(layout.titlebar.right() - 137.0, layout.titlebar.bottom()),
            ),
            theme::alpha(theme::GREEN, 10),
            theme::white_alpha(38),
            theme::alpha(theme::GREEN, 14),
        );
        painter.line_segment(
            [
                layout.titlebar.left_bottom(),
                layout.titlebar.right_bottom(),
            ],
            Stroke::new(1.0, theme::white_alpha(12)),
        );

        let title_icon = Rect::from_center_size(Pos2::new(23.0, 21.0), Vec2::splat(22.0));
        if let Some(texture) = self.release_icon_texture.as_ref() {
            paint_rounded_image(painter, title_icon, 6.0, texture, texture.size());
        } else {
            paint_block(
                painter,
                title_icon.center(),
                13.0,
                Color32::from_rgb(121, 210, 105),
                Color32::from_rgb(61, 142, 67),
                Color32::from_rgb(46, 104, 55),
            );
        }
        theme::bold_text(
            painter,
            Pos2::new(36.0, 13.0),
            brand::NAME,
            11.5,
            Color32::from_rgb(217, 226, 216),
        );
        let tag_rect = Rect::from_min_size(Pos2::new(145.0, 14.5), Vec2::new(55.0, 15.0));
        theme::fill_round(
            painter,
            tag_rect,
            theme::corner_radius(),
            theme::alpha(theme::GREEN, 17),
        );
        theme::stroke_round(
            painter,
            tag_rect,
            theme::corner_radius(),
            1.0,
            theme::alpha(theme::GREEN, 48),
        );
        theme::mono_text(
            painter,
            Pos2::new(153.0, 16.0),
            "PREVIEW",
            7.5,
            Color32::from_rgb(144, 198, 137),
        );

        painter.circle_filled(
            Pos2::new(layout.screen.center().x - 45.0, 21.0),
            2.6,
            Color32::from_rgb(105, 211, 98),
        );
        theme::text(
            painter,
            Pos2::new(layout.screen.center().x - 37.0, 14.0),
            if self.install_receiver.is_some() {
                "正在补全游戏文件"
            } else if self.game_process_id.is_some() {
                "游戏运行中"
            } else if self.launch_started.is_some() {
                "正在准备游戏"
            } else if self.launch_complete {
                "启动准备完成"
            } else {
                "准备就绪"
            },
            9.2,
            theme::muted(),
        );

        let button_size = Vec2::new(45.0, TITLEBAR_HEIGHT);
        for (index, action) in WindowAction::ALL.into_iter().enumerate() {
            let rect = Rect::from_min_size(
                Pos2::new(
                    // 控件组整体左移，固定的关闭按钮落在原控件组的左端。
                    layout.screen.right() - 137.0 + index as f32 * button_size.x,
                    0.0,
                ),
                button_size,
            );
            let response = ui
                .interact(rect, Id::unique(("window", index)), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand);
            let hovered = response.hovered();
            if hovered {
                theme::fill_round(
                    painter,
                    Rect::from_center_size(
                        rect.center(),
                        Vec2::new(rect.width() - 8.0, rect.height() - 8.0),
                    ),
                    theme::corner_radius(),
                    if matches!(action, WindowAction::Close) {
                        Color32::from_rgb(160, 57, 55)
                    } else {
                        theme::white_alpha(14)
                    },
                );
            }
            paint_window_icon(
                painter,
                rect.center(),
                action,
                if hovered {
                    theme::ink()
                } else {
                    theme::muted()
                },
            );
            if response.clicked() {
                match action {
                    WindowAction::Minimize => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true))
                    }
                    WindowAction::Close => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                }
            }
        }
    }

    /// 实现 绘制rail 相关的业务流程。
    ///
    /// # 参数
    /// - `ctx`：由调用方传入的ctx。
    /// - `ui`：由调用方传入的ui。
    /// - `painter`：由调用方传入的painter。
    /// - `layout`：由调用方传入的布局。
    /// - `time`：由调用方传入的time。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn paint_rail(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
    ) {
        let mut hovered_tooltip: Option<NavTooltip> = None;
        theme::frosted_glass_panel(
            painter,
            layout.rail,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(242, 246, 242), theme::GREEN, time, 0.8)
                .backdrop_blur(24.0),
        );
        painter.line_segment(
            [layout.rail.right_top(), layout.rail.right_bottom()],
            Stroke::new(1.0, theme::white_alpha(18)),
        );

        let logo = Rect::from_center_size(
            Pos2::new(layout.rail.center().x, TITLEBAR_HEIGHT + 40.0),
            Vec2::splat(50.0),
        );
        theme::glass_panel(
            painter,
            logo,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(28, 42, 31), theme::GREEN, time, 0.3),
        );
        let logo_content = card_clip(painter, logo);
        if let Some(texture) = self.release_icon_texture.as_ref() {
            paint_rounded_image(
                &logo_content,
                logo.shrink(8.0),
                12.0,
                texture,
                texture.size(),
            );
        } else {
            paint_block(
                &logo_content,
                logo.center() + Vec2::new(0.0, 1.0),
                28.0,
                Color32::from_rgb(133, 226, 114),
                Color32::from_rgb(85, 68, 50),
                Color32::from_rgb(68, 145, 70),
            );
        }

        let nav_top = logo.bottom() + 25.0;
        let nav_kinds = [
            NavKind::Home,
            NavKind::Versions,
            NavKind::Download,
            NavKind::Community,
            NavKind::Create,
        ];
        let settings_rect = Rect::from_center_size(
            Pos2::new(layout.rail.center().x, layout.rail_profile.top() - 30.5),
            Vec2::new(48.0, 47.0),
        );
        let target_y = if self.nav_selected < 5 {
            nav_top + 23.0 + self.nav_selected as f32 * 57.0
        } else {
            settings_rect.center().y
        };
        if !self.nav_highlight_initialized || !self.settings.animations_enabled() {
            self.nav_highlight_y = target_y;
            self.nav_highlight_velocity = 0.0;
            self.nav_highlight_initialized = true;
        } else {
            let delta = ctx.input(|input| input.stable_dt).clamp(0.001, 0.05);
            let active = spring_smooth(
                &mut self.nav_highlight_y,
                &mut self.nav_highlight_velocity,
                target_y,
                delta,
            );
            if active {
                ctx.request_repaint_after(Duration::from_millis(16));
            }
        }
        let highlight = Rect::from_center_size(
            Pos2::new(layout.rail.center().x, self.nav_highlight_y),
            Vec2::new(48.0, 47.0),
        );
        theme::glass_panel(
            painter,
            highlight,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(27, 47, 31), theme::GREEN, time, 0.8),
        );
        painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(layout.rail.left() - 1.0, highlight.center().y - 9.5),
                Vec2::new(3.0, 19.0),
            ),
            theme::corner_radius(),
            Color32::from_rgb(126, 227, 110),
        );
        for (index, kind) in nav_kinds.into_iter().enumerate() {
            let rect = Rect::from_center_size(
                Pos2::new(layout.rail.center().x, nav_top + 23.0 + index as f32 * 57.0),
                Vec2::new(48.0, 47.0),
            );
            let response = ui
                .interact(rect, Id::unique(("nav", index)), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand);
            let active = index == self.nav_selected;
            draw_icon(
                painter,
                NavIcon::from(kind),
                Rect::from_center_size(rect.center(), Vec2::splat(20.0)),
                if active {
                    Color32::from_rgb(215, 255, 203)
                } else if response.hovered() {
                    theme::ink_soft()
                } else {
                    Color32::from_rgb(113, 128, 119)
                },
            );

            if self.settings.tooltips
                && (response.hovered() || (self.preview_tooltip && index == 1))
            {
                hovered_tooltip = Some(NavTooltip {
                    id: index as u8,
                    label: kind.label(),
                    button: rect,
                });
            }
            if response.clicked() {
                ctx.request_repaint();
                self.menu = Menu::None;
                self.news = None;
                match index {
                    0 => self.switch_nav_page(0, Page::Home),
                    1 => {
                        self.switch_nav_page(1, Page::Versions);
                        self.version_page_selected = self.selected_version;
                        self.version_page_scroll =
                            (self.selected_version as f32 * 64.0 - 180.0).max(0.0);
                    }
                    2 => {
                        self.resource_target_profile = None;
                        self.switch_nav_page(2, Page::Download);
                        self.refresh_official_manifest();
                    }
                    3 => self.switch_nav_page(3, Page::Community),
                    _ => self.switch_nav_page(4, Page::Create),
                }
            }
        }

        let settings = ui
            .interact(settings_rect, Id::unique("settings"), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        let settings_active = self.page == Page::Settings;
        if settings.hovered() && self.settings.tooltips {
            hovered_tooltip = Some(NavTooltip {
                id: 5,
                label: "设置",
                button: settings_rect,
            });
        }
        draw_icon(
            painter,
            NavIcon::Settings,
            Rect::from_center_size(settings_rect.center(), Vec2::splat(20.0)),
            if settings_active {
                Color32::from_rgb(215, 255, 203)
            } else if settings.hovered() {
                theme::ink_soft()
            } else {
                theme::muted()
            },
        );
        if settings.clicked() {
            self.menu = Menu::None;
            self.news = None;
            self.switch_nav_page(5, Page::Settings);
            ctx.request_repaint();
        }

        let avatar = ui
            .interact(
                layout.rail_profile,
                Id::unique("rail-profile"),
                Sense::click(),
            )
            .on_hover_cursor(CursorIcon::PointingHand);
        theme::fill_round(
            painter,
            layout.rail_profile,
            theme::corner_radius(),
            Color32::from_rgb(26, 36, 29),
        );
        theme::stroke_round(
            painter,
            layout.rail_profile,
            theme::corner_radius(),
            1.0,
            if avatar.hovered() {
                theme::alpha(theme::GREEN, 80)
            } else {
                theme::white_alpha(26)
            },
        );
        paint_avatar(
            painter,
            Rect::from_center_size(layout.rail_profile.center(), Vec2::splat(29.0)),
            theme::corner_radius(),
            self.skin_texture.as_ref(),
        );
        painter.circle_filled(
            layout.rail_profile.right_bottom() + Vec2::new(-4.0, -4.0),
            4.2,
            Color32::from_rgb(101, 218, 96),
        );
        if avatar.hovered() {
            hovered_tooltip = Some(NavTooltip {
                id: 6,
                label: "账户",
                button: layout.rail_profile,
            });
        }
        if avatar.clicked() {
            if self.menu == Menu::Profile {
                self.menu = Menu::None;
            } else {
                self.version_popup_value = 0.0;
                self.version_popup_velocity = 0.0;
                self.menu = Menu::Profile;
            }
        }
        self.update_nav_tooltip(ctx, hovered_tooltip);
        self.paint_nav_tooltip(ctx, layout, time);
    }

    /// 实现 绘制navtooltip 相关的业务流程。
    ///
    /// # 参数
    /// - `ctx`：由调用方传入的ctx。
    /// - `layout`：由调用方传入的布局。
    /// - `time`：由调用方传入的time。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn paint_nav_tooltip(&self, ctx: &egui::Context, layout: &Layout, time: f64) {
        if self.nav_tooltip_value <= 0.002 {
            return;
        }
        let progress = self.nav_tooltip_value.clamp(0.0, 1.08);
        let eased = progress.clamp(0.0, 1.0);
        let eased = eased * eased * (3.0 - 2.0 * eased);
        let full_width = self.nav_tooltip_label.chars().count() as f32 * 8.2 + 38.0;
        let width = 22.0 + (full_width - 22.0) * eased;
        let height = 22.0 + 8.0 * eased;
        let origin = Pos2::new(
            self.nav_tooltip_button.right() + 4.0,
            self.nav_tooltip_button.center().y,
        );
        let target = Pos2::new(
            layout.rail.right() + 13.0 + full_width * 0.5,
            self.nav_tooltip_button.center().y,
        );
        let center = origin + (target - origin) * eased;
        let rect = Rect::from_center_size(center, Vec2::new(width, height));
        let opacity = (progress / 0.38).clamp(0.0, 1.0);
        egui::Area::new(Id::unique("animated-nav-tooltip-layer"))
            .order(egui::Order::Tooltip)
            .fixed_pos(rect.min)
            .interactable(false)
            .show(ctx, |ui| {
                let mut tooltip_painter = ui.painter().clone();
                tooltip_painter.set_opacity(opacity);
                theme::glass_panel(
                    &tooltip_painter,
                    rect,
                    (height * 0.5).round() as u8,
                    theme::GlassSpec::new(Color32::from_rgb(25, 39, 29), theme::GREEN, time, 10.8)
                        .backdrop_blur(12.0),
                );
                if eased > 0.42 {
                    tooltip_painter.set_opacity(opacity * ((eased - 0.42) / 0.58).clamp(0.0, 1.0));
                    let content = card_clip(&tooltip_painter, rect);
                    theme::center_text(
                        &content,
                        rect.center(),
                        self.nav_tooltip_label,
                        8.0,
                        theme::ink_soft(),
                        true,
                    );
                }
            });
    }

    /// 实现 绘制header 相关的业务流程。
    ///
    /// # 参数
    /// - `ctx`：由调用方传入的ctx。
    /// - `ui`：由调用方传入的ui。
    /// - `painter`：由调用方传入的painter。
    /// - `layout`：由调用方传入的布局。
    /// - `time`：由调用方传入的time。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn paint_header(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
    ) {
        self.ensure_skin_texture(ctx);
        let heading_painter = painter.clone();
        let game_directory_ready = self.settings.game_directory.is_some();
        if let Some(previous_page) = self.previous_page {
            let outgoing = CardMotion {
                screen: layout.screen,
                direction: self.page_transition_direction,
                progress: self.page_transition_progress,
                outgoing: true,
            };
            let incoming = CardMotion {
                screen: layout.screen,
                direction: self.page_transition_direction,
                progress: self.page_transition_progress,
                outgoing: false,
            };
            let mut outgoing_heading_painter = heading_painter.clone();
            outgoing_heading_painter
                .set_opacity(outgoing_page_opacity(self.page_transition_progress));
            paint_page_heading(
                &outgoing_heading_painter,
                layout,
                previous_page,
                game_directory_ready,
                outgoing,
            );
            paint_page_heading(
                &heading_painter,
                layout,
                self.page,
                game_directory_ready,
                incoming,
            );
        } else {
            paint_page_heading(
                &heading_painter,
                layout,
                self.page,
                game_directory_ready,
                CardMotion {
                    screen: layout.screen,
                    direction: 1.0,
                    progress: 1.0,
                    outgoing: false,
                },
            );
        }

        if let Some(previous_page) = self.previous_page {
            if previous_page == Page::Home {
                let outgoing = CardMotion {
                    screen: layout.screen,
                    direction: self.page_transition_direction,
                    progress: self.page_transition_progress,
                    outgoing: true,
                };
                let mut trigger_painter = painter.clone();
                trigger_painter.set_opacity(outgoing_page_opacity(self.page_transition_progress));
                self.paint_version_trigger(
                    ctx,
                    ui,
                    &trigger_painter,
                    layout,
                    outgoing.rect(layout.version_trigger),
                    time,
                    false,
                );
            }
            if self.page == Page::Home {
                let incoming = CardMotion {
                    screen: layout.screen,
                    direction: self.page_transition_direction,
                    progress: self.page_transition_progress,
                    outgoing: false,
                };
                self.paint_version_trigger(
                    ctx,
                    ui,
                    painter,
                    layout,
                    incoming.rect(layout.version_trigger),
                    time,
                    false,
                );
            }
        } else if self.page == Page::Home {
            self.paint_version_trigger(
                ctx,
                ui,
                painter,
                layout,
                layout.version_trigger,
                time,
                true,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    /// 实现 绘制版本trigger 相关的业务流程。
    ///
    /// # 参数
    /// - `_ctx`：由调用方传入的ctx。
    /// - `ui`：由调用方传入的ui。
    /// - `painter`：由调用方传入的painter。
    /// - `layout`：由调用方传入的布局。
    /// - `trigger`：由调用方传入的trigger。
    /// - `time`：由调用方传入的time。
    /// - `active`：由调用方传入的active。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn paint_version_trigger(
        &mut self,
        _ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        trigger: Rect,
        time: f64,
        active: bool,
    ) {
        let response = active.then(|| {
            ui.interact(trigger, Id::unique("version-trigger"), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand)
        });
        let hovered = response.as_ref().is_some_and(|response| response.hovered());
        theme::glass_panel(
            painter,
            trigger,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(27, 39, 31), theme::GREEN, time, 1.2)
                .hovered(hovered),
        );
        let content = card_clip(painter, trigger);
        let painter = &content;

        let badge = Rect::from_center_size(
            Pos2::new(trigger.left() + 23.0, trigger.center().y),
            Vec2::new(34.0, 34.0),
        );
        theme::fill_round(
            painter,
            badge,
            theme::corner_radius(),
            Color32::from_rgb(28, 45, 30),
        );
        let version = self.version();
        if let Some(texture) = self.version_icon(version) {
            paint_rounded_image(painter, badge.shrink(3.0), 8.0, texture, texture.size());
        } else {
            paint_block(
                painter,
                badge.center(),
                22.0,
                Color32::from_rgb(135, 222, 111),
                Color32::from_rgb(91, 67, 47),
                Color32::from_rgb(78, 155, 76),
            );
        }
        theme::text(
            painter,
            Pos2::new(badge.right() + 10.0, trigger.top() + 7.0),
            "当前版本",
            7.2,
            theme::muted(),
        );
        let version_label = self.version_selector_label(version);
        let version_label = fit_text_to_width(
            painter,
            &version_label,
            theme::bold_font(10.5),
            (trigger.width() - 94.0).max(70.0),
        );
        theme::bold_text(
            painter,
            Pos2::new(badge.right() + 10.0, trigger.top() + 20.0),
            version_label,
            10.5,
            theme::ink_soft(),
        );
        draw_icon(
            painter,
            NavIcon::Chevron,
            Rect::from_center_size(
                Pos2::new(trigger.right() - 17.0, trigger.center().y),
                Vec2::splat(13.0),
            ),
            theme::muted(),
        );
        if response.as_ref().is_some_and(|response| response.clicked()) {
            let opening = self.menu != Menu::Version;
            let selectable = self.selectable_version_order();
            if opening {
                self.profile_popup_value = 0.0;
                self.profile_popup_velocity = 0.0;
                let visible_height =
                    (version_menu_rect(layout, selectable.len()).height() - 39.0).max(1.0);
                let content_height = selectable.len() as f32 * VERSION_ROW_HEIGHT;
                let max_scroll = (content_height - visible_height).max(0.0);
                let selected_menu_index = selectable
                    .iter()
                    .position(|index| *index == self.selected_version)
                    .unwrap_or(0);
                self.version_scroll = (selected_menu_index as f32 * VERSION_ROW_HEIGHT
                    - (visible_height - VERSION_ROW_HEIGHT) * 0.5)
                    .clamp(0.0, max_scroll);
            }
            self.menu = if opening { Menu::Version } else { Menu::None };
        }
    }

    /// 实现 handleoutsideclick 相关的业务流程。
    ///
    /// # 参数
    /// - `ctx`：由调用方传入的ctx。
    /// - `layout`：由调用方传入的布局。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn handle_outside_click(&mut self, ctx: &egui::Context, layout: &Layout) {
        if !ctx.input(|input| input.pointer.primary_pressed()) {
            return;
        }
        if self.news.is_some() {
            return;
        }
        let Some(pointer) = ctx.input(|input| input.pointer.interact_pos()) else {
            return;
        };
        if self.account_name_dialog_open
            && !account_name_dialog_rect(layout.screen).contains(pointer)
        {
            self.account_name_dialog_open = false;
            self.account_name_error = None;
            return;
        }
        if self.favorite_editor_open && !favorite_editor_rect(layout.screen).contains(pointer) {
            self.cancel_favorite_editor();
        }
        if self.java_picker_target.is_some() {
            let menu = java_picker_rect(
                self.java_picker_anchor,
                layout.screen,
                self.java_runtimes.len() + 2,
            );
            if !menu.contains(pointer) && !self.java_picker_anchor.contains(pointer) {
                self.java_picker_target = None;
            }
        }
        if self.version_export_open && !version_export_dialog_rect(layout.screen).contains(pointer)
        {
            self.version_export_open = false;
        }
        if self.version_content_dialog.is_some() && self.version_content_dialog_open {
            let dialog = version_content_dialog_rect(layout.screen);
            if self.export_format_menu_open {
                let format_row = export_format_selector_rect(dialog);
                let selector = Rect::from_min_size(
                    Pos2::new(format_row.right() - 14.0 - 190.0, format_row.top() + 11.0),
                    Vec2::new(190.0, 34.0),
                );
                let menu = export_format_menu_rect(format_row);
                if !menu.contains(pointer) && !selector.contains(pointer) {
                    self.export_format_menu_open = false;
                }
            }
            if version_content_close_rect(dialog).contains(pointer) {
                self.close_version_content_dialog();
                return;
            }
            if !dialog.contains(pointer) {
                self.close_version_content_dialog();
            }
        }
        if self.resource_version_menu_open {
            let menu = resource_version_menu_rect(self.resource_version_anchor, layout.screen);
            if !menu.contains(pointer) && !self.resource_version_anchor.contains(pointer) {
                self.resource_version_menu_open = false;
            }
        }
        if self.launch_settings_open
            && !launch_settings_card_rect(layout).contains(pointer)
            && !launch_more_rect(layout).contains(pointer)
        {
            self.launch_settings_open = false;
        }
        if self.version_folder_menu_open {
            let menu = version_folder_menu_rect(layout, self.settings.game_directories.len());
            let selector = version_folder_selector_rect(layout);
            if !menu.contains(pointer) && !selector.contains(pointer) {
                self.version_folder_menu_open = false;
            }
        }
        let version_menu = version_menu_rect(layout, self.selectable_version_order().len());
        let profile_menu = profile_menu_rect(layout);
        if self.menu == Menu::Version
            && !version_menu.contains(pointer)
            && !layout.version_trigger.contains(pointer)
        {
            self.menu = Menu::None;
        }
        if self.menu == Menu::Profile
            && !profile_menu.contains(pointer)
            && !layout.rail_profile.contains(pointer)
        {
            self.menu = Menu::None;
        }
    }
}
