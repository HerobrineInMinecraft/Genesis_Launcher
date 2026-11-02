//! # `src/app/animations.rs`
//!
//! **功能域：** 启动器应用外壳、页面状态、交互流程和原生 egui 绘制。
//!
//! 推进各类弹窗、页面和列表的弹性动画状态
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `version`
//! - `version_icon`
//! - `visible_loader_label`
//! - `version_selector_label`
//! - `version_selector_detail`
//! - `version_data_directory`
//! - `version_data_directory_for`
//! - `java_path_for_version`
//! - `java_runtime_label`
//! - `refresh_java_runtimes`
//! - `select_game_directory`
//! - `set_version_isolation`
//! - `refresh_recent_sessions`
//! - `ensure_news_textures`
//! - `version_recency`
//! - `version_menu_order`
//! - `selectable_version_order`
//! - `update_popup_animation`
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
    /// 实现 版本 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `&GameVersion`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn version(&self) -> &GameVersion {
        &self.versions[self.selected_version]
    }

    /// 实现 版本图标 相关的业务流程。
    ///
    /// # 参数
    /// - `version`：由调用方传入的版本。
    /// # 返回
    /// 返回 `Option<&egui::TextureHandle>`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn version_icon(&self, version: &GameVersion) -> Option<&egui::TextureHandle> {
        if version.loader_family != "原版" {
            self.loader_icon_texture.as_ref()
        } else if is_snapshot_version(&version.version) {
            self.snapshot_icon_texture.as_ref()
        } else {
            self.release_icon_texture.as_ref()
        }
    }

    /// 实现 visible加载器标签 相关的业务流程。
    ///
    /// # 参数
    /// - `version`：由调用方传入的版本。
    /// # 返回
    /// 返回 `String`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn visible_loader_label(&self, version: &GameVersion) -> String {
        if version.loader_family == "原版" {
            String::new()
        } else if self.settings.show_loader_versions {
            version.loader.clone()
        } else {
            version.loader_family.clone()
        }
    }

    /// 实现 版本selector标签 相关的业务流程。
    ///
    /// # 参数
    /// - `version`：由调用方传入的版本。
    /// # 返回
    /// 返回 `String`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn version_selector_label(&self, version: &GameVersion) -> String {
        if let Some(custom_name) = self.version_custom_name(version) {
            return custom_name;
        }
        if version.loader_family == "原版" {
            version.game_version.clone()
        } else {
            format!("{} · {}", version.game_version, version.loader)
        }
    }

    /// 实现 版本selectordetail 相关的业务流程。
    ///
    /// # 参数
    /// - `version`：由调用方传入的版本。
    /// # 返回
    /// 返回 `String`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn version_selector_detail(&self, version: &GameVersion) -> String {
        let detail = if self.settings.show_loader_versions {
            version.description.clone()
        } else if version.loader_family == "原版" {
            "原版 · 已安装 · 可启动".to_owned()
        } else {
            "已安装 · 可启动".to_owned()
        };
        if self.version_custom_name(version).is_some() {
            format!("原版本 {} · {detail}", version.game_version)
        } else {
            detail
        }
    }

    /// 实现 版本数据目录 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `Option<PathBuf>`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn version_data_directory(&self) -> Option<PathBuf> {
        let profile = self.versions.get(self.selected_version)?.version.as_str();
        self.version_data_directory_for(profile)
    }

    /// 实现 版本数据目录for 相关的业务流程。
    ///
    /// # 参数
    /// - `profile`：由调用方传入的配置。
    /// # 返回
    /// 返回 `Option<PathBuf>`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn version_data_directory_for(&self, profile: &str) -> Option<PathBuf> {
        let base = self.settings.game_directory.as_ref()?;
        if self.settings.version_isolation {
            Some(base.join("versions").join(profile))
        } else {
            Some(base.clone())
        }
    }

    /// 实现 Java路径for版本 相关的业务流程。
    ///
    /// # 参数
    /// - `directory`：由调用方传入的目录。
    /// - `profile`：由调用方传入的配置。
    /// # 返回
    /// 返回 `Option<PathBuf>`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn java_path_for_version(
        &self,
        directory: &std::path::Path,
        profile: &str,
    ) -> Option<PathBuf> {
        self.settings
            .version_java_path(directory, profile)
            .or_else(|| self.settings.java_path.clone())
    }

    /// 实现 Javaruntime标签 相关的业务流程。
    ///
    /// # 参数
    /// - `path`：由调用方传入的路径。
    /// # 返回
    /// 返回 `String`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn java_runtime_label(&self, path: &std::path::Path) -> String {
        if let Some(runtime) = self
            .java_runtimes
            .iter()
            .find(|runtime| same_path(&runtime.path, path))
        {
            format!(
                "Java {} · {}",
                runtime.major,
                runtime
                    .path
                    .parent()
                    .and_then(std::path::Path::parent)
                    .and_then(std::path::Path::file_name)
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| runtime.path.display().to_string())
            )
        } else {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string())
        }
    }

    /// 实现 刷新Javaruntimes 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn refresh_java_runtimes(&mut self) {
        self.java_runtimes = crate::game::discover_java_runtimes(&self.settings.game_directories);
    }

    /// 实现 选择游戏目录 相关的业务流程。
    ///
    /// # 参数
    /// - `directory`：由调用方传入的目录。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn select_game_directory(&mut self, directory: PathBuf) {
        if !crate::game::is_game_directory(&directory) {
            self.show_toast(format!(
                "不是有效的 Minecraft 文件夹：{}",
                directory.display()
            ));
            return;
        }
        self.settings.add_game_directory(directory.clone());
        self.settings.game_directory = Some(directory);
        self.versions = available_versions(self.settings.game_directory.as_deref());
        self.selected_version = self
            .version_menu_order()
            .into_iter()
            .find(|index| {
                self.versions
                    .get(*index)
                    .is_some_and(|version| version.launchable)
            })
            .unwrap_or(0);
        self.version_page_selected = self.selected_version;
        self.version_page_scroll = 0.0;
        self.version_content_dialog = None;
        self.previous_version_detail = None;
        self.version_detail_progress = 1.0;
        self.refresh_recent_sessions();
        self.show_toast(format!(
            "已切换到 {}",
            self.settings
                .game_directory
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default()
        ));
    }

    /// 实现 设置版本isolation 相关的业务流程。
    ///
    /// # 参数
    /// - `enabled`：由调用方传入的enabled。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn set_version_isolation(&mut self, enabled: bool) {
        self.settings.version_isolation = enabled;
        if enabled && let Some(base) = self.settings.game_directory.as_ref() {
            for version in &self.versions {
                let directory = base.join("versions").join(&version.version);
                let _ = std::fs::create_dir_all(directory.join("mods"));
                let _ = std::fs::create_dir_all(directory.join("saves"));
            }
        }
        self.refresh_recent_sessions();
        self.show_toast(if enabled {
            "已为所有版本开启隔离"
        } else {
            "已关闭全部版本隔离"
        });
    }

    /// 实现 刷新recentsessions 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn refresh_recent_sessions(&mut self) {
        self.recent_sessions = self
            .version_data_directory()
            .map(|directory| crate::sessions::discover_recent_sessions(&directory))
            .unwrap_or_default();
    }

    /// 实现 确保新闻textures 相关的业务流程。
    ///
    /// # 参数
    /// - `ctx`：由调用方传入的ctx。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn ensure_news_textures(&mut self, ctx: &egui::Context) {
        let keys: Vec<Option<String>> = self
            .news_items
            .iter()
            .take(3)
            .map(|news| news.image_key.clone())
            .collect();
        if keys == self.news_texture_keys {
            return;
        }
        self.news_texture_keys = keys;
        self.news_textures = Vec::with_capacity(3);
        for news in self.news_items.iter().take(3) {
            let Some(bytes) = news.image_bytes.as_deref() else {
                self.news_textures.push(None);
                continue;
            };
            let texture = image::load_from_memory(bytes).ok().map(|image| {
                let color_image = color_image_for_context(ctx, image);
                ctx.load_texture(
                    format!(
                        "minecraft-news-{}",
                        news.image_key.as_deref().unwrap_or(&news.id)
                    ),
                    color_image,
                    egui::TextureOptions::LINEAR,
                )
            });
            self.news_textures.push(texture);
        }
    }

    /// 实现 版本recency 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `HashMap<String, u64>`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn version_recency(&self) -> HashMap<String, u64> {
        version_recency_map(&self.settings, self.settings.game_directory.as_deref())
    }

    /// 实现 版本菜单order 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `Vec<usize>`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn version_menu_order(&self) -> Vec<usize> {
        version_menu_order(&self.versions, &self.version_recency())
    }

    /// 实现 selectable版本order 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `Vec<usize>`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn selectable_version_order(&self) -> Vec<usize> {
        self.version_menu_order()
            .into_iter()
            .filter(|index| self.versions[*index].launchable)
            .collect()
    }

    /// 实现 更新popup动画 相关的业务流程。
    ///
    /// # 参数
    /// - `ctx`：由调用方传入的ctx。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn update_popup_animation(&mut self, ctx: &egui::Context) {
        let version_target = if self.menu == Menu::Version { 1.0 } else { 0.0 };
        let profile_target = if self.menu == Menu::Profile { 1.0 } else { 0.0 };
        let launch_target = if self.launch_settings_open { 1.0 } else { 0.0 };
        if !self.settings.animations_enabled() {
            self.version_popup_value = version_target;
            self.version_popup_velocity = 0.0;
            self.profile_popup_value = profile_target;
            self.profile_popup_velocity = 0.0;
            self.launch_popup_value = launch_target;
            self.launch_popup_velocity = 0.0;
            return;
        }
        let delta = ctx.input(|input| input.stable_dt).clamp(0.001, 0.05);
        let version_active = spring_popup(
            &mut self.version_popup_value,
            &mut self.version_popup_velocity,
            version_target,
            delta,
        );
        let profile_active = spring_popup(
            &mut self.profile_popup_value,
            &mut self.profile_popup_velocity,
            profile_target,
            delta,
        );
        let launch_active = spring_popup(
            &mut self.launch_popup_value,
            &mut self.launch_popup_velocity,
            launch_target,
            delta,
        );
        if version_active || profile_active || launch_active {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    /// 实现 更新navigation动画 相关的业务流程。
    ///
    /// # 参数
    /// - `ctx`：由调用方传入的ctx。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn update_navigation_animation(&mut self, ctx: &egui::Context) {
        if !self.settings.animations_enabled() {
            self.nav_highlight_velocity = 0.0;
            self.previous_page = None;
            self.page_transition_progress = 1.0;
            self.page_transition_velocity = 0.0;
            return;
        }
        let delta = ctx.input(|input| input.stable_dt).clamp(0.001, 0.05);
        let page_active = if self.previous_page.is_some() {
            let active = spring_smooth(
                &mut self.page_transition_progress,
                &mut self.page_transition_velocity,
                1.0,
                delta,
            );
            if !active {
                self.previous_page = None;
                self.page_transition_progress = 1.0;
            }
            active
        } else {
            false
        };
        if page_active {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    /// 实现 更新下载animations 相关的业务流程。
    ///
    /// # 参数
    /// - `ctx`：由调用方传入的ctx。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn update_download_animations(&mut self, ctx: &egui::Context) {
        let category_target = resource_category_index(self.download_resource_category) as f32;
        if !self.settings.animations_enabled() {
            self.download_category_position = category_target;
            self.download_category_velocity = 0.0;
            self.download_page_progress = 1.0;
            self.download_page_velocity = 0.0;
            self.download_switch_texture = None;
            self.download_switch_texture_size = [0, 0];
            self.download_switch_queued = None;
            return;
        }
        let delta = ctx.input(|input| input.stable_dt).clamp(0.001, 0.05);
        let category_active = spring_smooth(
            &mut self.download_category_position,
            &mut self.download_category_velocity,
            category_target,
            delta,
        );
        let page_active =
            if self.download_page_progress < 1.0 || self.download_page_velocity.abs() > 0.01 {
                spring_smooth(
                    &mut self.download_page_progress,
                    &mut self.download_page_velocity,
                    1.0,
                    delta,
                )
            } else {
                false
            };
        if !page_active {
            self.download_page_progress = 1.0;
            self.download_page_velocity = 0.0;
            self.download_switch_texture = None;
            self.download_switch_texture_size = [0, 0];
        }
        let queued = if page_active {
            None
        } else {
            self.download_switch_queued.take()
        };
        if let Some(category) = queued {
            self.begin_download_category_switch(ctx, category);
        } else if category_active || page_active {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    /// 实现 更新navtooltip 相关的业务流程。
    ///
    /// # 参数
    /// - `ctx`：由调用方传入的ctx。
    /// - `hovered`：由调用方传入的hovered。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn update_nav_tooltip(&mut self, ctx: &egui::Context, hovered: Option<NavTooltip>) {
        if let Some(tooltip) = hovered {
            if self.nav_tooltip_id != tooltip.id {
                self.nav_tooltip_id = tooltip.id;
                self.nav_tooltip_value = 0.0;
                self.nav_tooltip_velocity = 0.0;
            }
            self.nav_tooltip_label = tooltip.label;
            self.nav_tooltip_button = tooltip.button;
            self.nav_tooltip_target = true;
        } else {
            self.nav_tooltip_target = false;
        }
        let target = if self.nav_tooltip_target { 1.0 } else { 0.0 };
        if !self.settings.animations_enabled() {
            self.nav_tooltip_value = target;
            self.nav_tooltip_velocity = 0.0;
        } else {
            let delta = ctx.input(|input| input.stable_dt).clamp(0.001, 0.05);
            if spring_smooth(
                &mut self.nav_tooltip_value,
                &mut self.nav_tooltip_velocity,
                target,
                delta,
            ) {
                ctx.request_repaint_after(Duration::from_millis(16));
            }
        }
    }

    /// 实现 更新settings动画 相关的业务流程。
    ///
    /// # 参数
    /// - `ctx`：由调用方传入的ctx。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn update_settings_animation(&mut self, ctx: &egui::Context) {
        if !self.settings.animations_enabled() {
            self.previous_settings_section = None;
            self.settings_transition_progress = 1.0;
            self.settings_transition_velocity = 0.0;
            return;
        }
        if self.previous_settings_section.is_none() {
            return;
        }
        let delta = ctx.input(|input| input.stable_dt).clamp(0.001, 0.05);
        let active = spring_smooth(
            &mut self.settings_transition_progress,
            &mut self.settings_transition_velocity,
            1.0,
            delta,
        );
        if !active {
            self.previous_settings_section = None;
            self.settings_transition_progress = 1.0;
        } else {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    /// 实现 更新detailanimations 相关的业务流程。
    ///
    /// # 参数
    /// - `ctx`：由调用方传入的ctx。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn update_detail_animations(&mut self, ctx: &egui::Context) {
        if !self.settings.animations_enabled() {
            self.previous_version_detail = None;
            self.version_detail_progress = 1.0;
            self.version_detail_velocity = 0.0;
            self.previous_download_detail = None;
            self.download_detail_progress = 1.0;
            self.download_detail_velocity = 0.0;
            self.previous_resource_detail = None;
            self.resource_detail_progress = 1.0;
            self.resource_detail_velocity = 0.0;
            self.download_loader_value = if self.download_loader_open { 1.0 } else { 0.0 };
            self.download_loader_velocity = 0.0;
            return;
        }
        let delta = ctx.input(|input| input.stable_dt).clamp(0.001, 0.05);
        let mut repaint = false;
        if self.previous_version_detail.is_some() {
            let active = spring_smooth(
                &mut self.version_detail_progress,
                &mut self.version_detail_velocity,
                1.0,
                delta,
            );
            if active {
                repaint = true;
            } else {
                self.previous_version_detail = None;
                self.version_detail_progress = 1.0;
            }
        }
        if self.previous_download_detail.is_some() {
            let active = spring_smooth(
                &mut self.download_detail_progress,
                &mut self.download_detail_velocity,
                1.0,
                delta,
            );
            if active {
                repaint = true;
            } else {
                self.previous_download_detail = None;
                self.download_detail_progress = 1.0;
            }
        }
        if self.previous_resource_detail.is_some() {
            let active = spring_smooth(
                &mut self.resource_detail_progress,
                &mut self.resource_detail_velocity,
                1.0,
                delta,
            );
            if active {
                repaint = true;
            } else {
                self.previous_resource_detail = None;
                self.resource_detail_progress = 1.0;
            }
        }
        let loader_target = if self.download_loader_open { 1.0 } else { 0.0 };
        if spring_smooth(
            &mut self.download_loader_value,
            &mut self.download_loader_velocity,
            loader_target,
            delta,
        ) {
            repaint = true;
        }
        if repaint {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    /// 实现 更新版本内容动画 相关的业务流程。
    ///
    /// # 参数
    /// - `ctx`：由调用方传入的ctx。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn update_version_content_animation(&mut self, ctx: &egui::Context) {
        if self.version_content_dialog.is_none() {
            return;
        }
        let target = if self.version_content_dialog_open {
            1.0
        } else {
            0.0
        };
        if !self.settings.animations_enabled() {
            self.version_content_popup_value = target;
            self.version_content_popup_velocity = 0.0;
            self.version_content_close_elapsed = 0.0;
            if !self.version_content_dialog_open {
                self.version_content_dialog = None;
            }
            return;
        }
        let delta = ctx.input(|input| input.stable_dt).clamp(0.001, 0.05);
        if self.version_content_dialog_open {
            self.version_content_close_elapsed = 0.0;
            if spring_popup(
                &mut self.version_content_popup_value,
                &mut self.version_content_popup_velocity,
                target,
                delta,
            ) {
                ctx.request_repaint_after(Duration::from_millis(16));
            }
        } else if self.version_content_popup_value <= 0.002 {
            self.version_content_popup_value = 0.0;
            self.version_content_popup_velocity = 0.0;
            self.version_content_close_elapsed += delta;
            if self.version_content_close_elapsed >= CONTENT_CLOSE_BOUNCE_SECONDS {
                self.version_content_dialog = None;
                self.version_content_close_elapsed = 0.0;
                self.version_content_popup_source = Rect::NOTHING;
            } else {
                ctx.request_repaint_after(Duration::from_millis(16));
            }
        } else {
            self.version_content_close_elapsed = 0.0;
            if spring_popup(
                &mut self.version_content_popup_value,
                &mut self.version_content_popup_velocity,
                target,
                delta,
            ) {
                ctx.request_repaint_after(Duration::from_millis(16));
            }
        }
    }
}
