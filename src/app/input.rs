//! # `src/app/input.rs`
//!
//! **功能域：** 启动器应用外壳、页面状态、交互流程和原生 egui 绘制。
//!
//! 处理快捷键、计时器、启动、截图和指针光效
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `update_timers`
//! - `handle_shortcuts`
//! - `start_launch`
//! - `resume_launch_after_repair`
//! - `handle_screenshot`
//! - `paint_pointer_light`
//! - `paint_page_body`
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
    /// 实现 更新timers 相关的业务流程。
    ///
    /// # 参数
    /// - `ctx`：由调用方传入的ctx。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn update_timers(&mut self, ctx: &egui::Context) {
        self.poll_background_tasks(ctx);
        self.poll_forced_update(ctx);
        self.poll_resource_tasks();
        self.poll_modpack_export();
        self.poll_favorite_icons(ctx);
        self.update_popup_animation(ctx);
        self.update_account_name_dialog_animation(ctx);
        self.update_version_name_animation(ctx);
        self.update_favorite_editor_animation(ctx);
        self.update_navigation_animation(ctx);
        self.poll_download_category_switch(ctx);
        self.update_download_animations(ctx);
        self.update_settings_animation(ctx);
        self.update_create_animation(ctx);
        self.update_detail_animations(ctx);
        self.update_version_content_animation(ctx);
        if self
            .game_process_id
            .is_some_and(|process_id| !crate::game::is_game_running(process_id))
        {
            self.game_process_id = None;
            self.launch_started = None;
            self.launch_complete = false;
            self.refresh_recent_sessions();
            if self.settings.notify_after_launch {
                self.show_toast("游戏已退出");
            }
        }
        if let Some(started) = self.launch_started
            && started.elapsed() >= Duration::from_millis(1550)
        {
            self.launch_started = None;
            self.launch_complete = true;
            if self.settings.notify_after_launch {
                self.show_toast("Minecraft 游戏进程已启动");
            }
            if self.settings.hide_after_launch {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
        }

        if self
            .toast
            .as_ref()
            .is_some_and(|toast| toast.shown_at.elapsed() >= Duration::from_secs(3))
        {
            self.toast = None;
        }

        let animate = self.settings.animations_enabled()
            && (self.launch_started.is_some()
                || self.toast.is_some()
                || self.news.is_some()
                || self.launcher_update_receiver.is_some()
                || self.forced_update_receiver.is_some()
                || self.menu != Menu::None
                || self.game_process_id.is_some()
                || self.news_receiver.is_some()
                || self.official_manifest.is_some()
                || self.install_receiver.is_some()
                || self.loader_versions_receiver.is_some()
                || self.loader_install_receiver.is_some()
                || self.modpack_import_receiver.is_some()
                || self.modpack_export_receiver.is_some()
                || self.favorite_icon_receiver.is_some()
                || self.favorite_editor_value > 0.002
                || self.resource_search_receiver.is_some()
                || self.resource_download_receiver.is_some());
        ctx.request_repaint_after(if animate {
            Duration::from_millis(33)
        } else {
            Duration::from_millis(80)
        });
    }

    /// 实现 handleshortcuts 相关的业务流程。
    ///
    /// # 参数
    /// - `ctx`：由调用方传入的ctx。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            if self.account_name_dialog_open {
                self.account_name_dialog_open = false;
                self.account_name_error = None;
            } else if self.java_picker_target.is_some() {
                self.java_picker_target = None;
            } else if self.version_export_open {
                self.version_export_open = false;
            } else if self.version_content_dialog.is_some() {
                self.close_version_content_dialog();
            } else if self.official_browser_open {
                self.official_browser_open = false;
            } else if self.news.is_some() {
                self.news = None;
            } else if self.launch_settings_open {
                self.launch_settings_open = false;
            } else if self.version_folder_menu_open {
                self.version_folder_menu_open = false;
            } else if self.loader_dialog_open && self.loader_install_receiver.is_none() {
                self.loader_dialog_open = false;
                self.loader_versions_receiver = None;
            } else if self.favorite_editor_open {
                self.cancel_favorite_editor();
            } else if self.go_back_page() {
            } else if self.page != Page::Home {
                self.switch_nav_page(0, Page::Home);
            } else {
                self.menu = Menu::None;
            }
        }

        if self.page == Page::Create && self.create_tab == 0 {
            let (redo, undo) = ctx.input(|input| {
                let command = input.modifiers.command;
                (
                    command && input.modifiers.shift && input.key_pressed(egui::Key::Z)
                        || command && input.key_pressed(egui::Key::Y),
                    command && !input.modifiers.shift && input.key_pressed(egui::Key::Z),
                )
            });
            if undo {
                self.undo_skin_edit();
                ctx.request_repaint();
            } else if redo {
                self.redo_skin_edit();
                ctx.request_repaint();
            }
        }

        if self.page == Page::Home
            && self.news.is_none()
            && ctx.input(|input| input.key_pressed(egui::Key::Enter))
            && self.launch_started.is_none()
        {
            self.launch_recent = false;
            self.start_launch(ctx);
        }
    }

    /// 实现 启动启动 相关的业务流程。
    ///
    /// # 参数
    /// - `_ctx`：由调用方传入的ctx。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn start_launch(&mut self, _ctx: &egui::Context) {
        let Some(game_directory) = self.settings.game_directory.clone() else {
            self.show_toast("未找到游戏目录，请先在设置中重新发现");
            return;
        };
        let version = self.version().clone();
        if self.install_receiver.is_some() {
            return;
        }
        if !crate::game::installation_complete(
            &game_directory,
            &version.version,
            &version.loader_family,
        ) {
            self.start_version_install(version.version, &version.loader_family, true);
            return;
        }
        let resume_recent = self.launch_recent;
        let data_directory = self.version_data_directory();
        if self.settings.player_skin.is_some()
            && let Some(directory) = data_directory.as_deref()
            && let Err(error) = install_local_skin_resource_pack(directory, &self.skin_pixels)
        {
            self.show_toast(format!("本地皮肤资源包安装失败：{error}"));
        }
        let memory_mb = self
            .settings
            .version_memory_mb(&game_directory, &version.version)
            .unwrap_or(self.settings.memory_mb);
        let java_path = self.java_path_for_version(&game_directory, &version.version);
        let launch_options = crate::game::LaunchOptions {
            max_memory_mb: memory_mb,
            window_width: self.settings.window_width,
            window_height: self.settings.window_height,
            fullscreen: self.settings.fullscreen,
            extra_jvm_args: self.settings.extra_jvm_args.clone(),
            quick_play: if resume_recent {
                self.recent_sessions
                    .first()
                    .map(|session| match session.kind {
                        SessionKind::World => {
                            crate::game::QuickPlayTarget::World(session.id.clone())
                        }
                        SessionKind::Server => {
                            crate::game::QuickPlayTarget::Server(session.id.clone())
                        }
                    })
            } else {
                None
            },
            data_directory,
            java_path,
            player_name: self.settings.player_name.clone(),
            skin_slim: self.skin_model_alex,
        };
        match crate::game::launch(
            &game_directory,
            &version.version,
            &version.loader_family,
            &launch_options,
        ) {
            Ok(details) => {
                self.launch_recent = false;
                self.record_version_play(&version.version);
                self.game_process_id = Some(details.process_id);
                self.launch_started = Some(Instant::now());
                self.launch_complete = false;
                self.menu = Menu::None;
                self.show_toast(format!(
                    "正在启动 {} · {} · PID {}",
                    details.version, details.java_name, details.process_id
                ));
            }
            Err(error) => {
                self.launch_recent = false;
                self.game_process_id = None;
                self.launch_started = None;
                self.launch_complete = false;
                if ["缺少", "客户端文件", "游戏库", "资源索引", "读取版本配置"]
                    .iter()
                    .any(|pattern| error.contains(pattern))
                {
                    self.start_version_install(version.version, &version.loader_family, true);
                } else {
                    self.show_toast(format!("启动失败：{error}"));
                }
            }
        }
    }

    /// 实现 resume启动afterrepair 相关的业务流程。
    ///
    /// # 参数
    /// - `ctx`：由调用方传入的ctx。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn resume_launch_after_repair(&mut self, ctx: &egui::Context) {
        let Some(game_directory) = self.settings.game_directory.as_ref() else {
            return;
        };
        let version = self.version().clone();
        if crate::game::installation_complete(
            game_directory,
            &version.version,
            &version.loader_family,
        ) {
            self.start_launch(ctx);
        } else {
            self.show_toast(format!(
                "{} 的官方基础文件已补齐，但模组加载器文件仍不完整",
                version.version
            ));
        }
    }

    /// 实现 handlescreenshot 相关的业务流程。
    ///
    /// # 参数
    /// - `ctx`：由调用方传入的ctx。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn handle_screenshot(&mut self, ctx: &egui::Context) {
        if self.screenshot_requested {
            return;
        }
        let Some(path) = self.screenshot_path.clone() else {
            return;
        };
        if self.page == Page::Download
            && self.download_loader_open
            && self.loader_versions_receiver.is_some()
            && self.started_at.elapsed() < Duration::from_secs(8)
        {
            ctx.request_repaint_after(Duration::from_millis(50));
            return;
        }
        if self.page == Page::Download
            && self.resource_search_receiver.is_some()
            && self.started_at.elapsed() < Duration::from_secs(8)
        {
            ctx.request_repaint_after(Duration::from_millis(50));
            return;
        }
        if (matches!(self.page, Page::Home | Page::Download) || self.official_browser_open)
            && self.official_manifest.is_some()
            && self.started_at.elapsed() < Duration::from_secs(5)
        {
            ctx.request_repaint_after(Duration::from_millis(50));
            return;
        }
        if self.page == Page::Home
            && self.news_receiver.is_some()
            && self.started_at.elapsed() < Duration::from_secs(6)
        {
            ctx.request_repaint_after(Duration::from_millis(50));
            return;
        }
        if self.launcher_update_receiver.is_some()
            && self.started_at.elapsed() < Duration::from_secs(5)
        {
            ctx.request_repaint_after(Duration::from_millis(50));
            return;
        }
        if self.account_name_dialog_open
            && self.account_name_dialog_value < 0.98
            && self.started_at.elapsed() < Duration::from_secs(2)
        {
            ctx.request_repaint_after(Duration::from_millis(16));
            return;
        }
        if self.page == Page::Community
            && self.favorite_icon_receiver.is_some()
            && self.started_at.elapsed() < Duration::from_secs(8)
        {
            ctx.request_repaint_after(Duration::from_millis(50));
            return;
        }
        self.screenshot_requested = true;
        let close_ctx = ctx.clone();
        ctx.request_screenshot(move |image| {
            let [width, height] = image.size;
            let mut rgba = Vec::with_capacity(width * height * 4);
            for pixel in &image.pixels {
                rgba.extend_from_slice(&pixel.to_array());
            }
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = image::save_buffer(
                &path,
                &rgba,
                width as u32,
                height as u32,
                image::ColorType::Rgba8,
            );
            close_ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        });
        ctx.request_repaint();
    }

    /// 实现 绘制指针light 相关的业务流程。
    ///
    /// # 参数
    /// - `ctx`：由调用方传入的ctx。
    /// - `painter`：由调用方传入的painter。
    /// - `screen`：由调用方传入的screen。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn paint_pointer_light(
        &mut self,
        ctx: &egui::Context,
        painter: &Painter,
        screen: Rect,
    ) {
        let target = if self.pointer_glow_preview {
            Some(Pos2::new(screen.right() - 120.0, screen.top() + 92.0))
        } else {
            ctx.input(|input| {
                input
                    .pointer
                    .hover_pos()
                    .filter(|position| screen.contains(*position))
            })
        };
        if let Some(target) = target {
            self.pointer_glow_position = target;
            self.pointer_glow_strength = 1.0;
            self.pointer_glow_initialized = true;
            if self.settings.pointer_brightness > 0 && ctx.input(|input| input.pointer.is_moving())
            {
                let refresh_rate = self.settings.pointer_refresh_rate.clamp(20, 144) as f32;
                ctx.request_repaint_after(Duration::from_secs_f32(1.0 / refresh_rate));
            }
        } else {
            self.pointer_glow_strength = 0.0;
        }
        if self.pointer_glow_strength > 0.01 && self.pointer_glow_initialized {
            theme::pointer_glow(
                painter,
                self.pointer_glow_position,
                self.pointer_glow_strength,
                screen,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    /// 实现 绘制page主体 相关的业务流程。
    ///
    /// # 参数
    /// - `ctx`：由调用方传入的ctx。
    /// - `ui`：由调用方传入的ui。
    /// - `body_painter`：由调用方传入的主体painter。
    /// - `layout`：由调用方传入的布局。
    /// - `time`：由调用方传入的time。
    /// - `page`：由调用方传入的page。
    /// - `motion`：由调用方传入的motion。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn paint_page_body(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        body_painter: &Painter,
        layout: &Layout,
        time: f64,
        page: Page,
        motion: CardMotion,
    ) {
        let painter = body_painter.clone();
        match page {
            Page::Home => {
                self.paint_hero(ctx, ui, &painter, layout, time, motion);
                self.paint_latest(ctx, ui, &painter, layout, time, motion);
            }
            Page::Versions => self.paint_versions(ctx, ui, &painter, layout, time, motion),
            Page::Download => self.paint_download(ctx, ui, &painter, layout, time, motion),
            Page::Community => self.paint_favorites(ctx, ui, &painter, layout, time, motion),
            Page::Create => self.paint_create_workspace(ctx, ui, &painter, layout, time, motion),
            Page::Settings => self.paint_settings(ctx, ui, &painter, layout, time, motion),
        }
    }
}
