//! # `src/app/bootstrap.rs`
//!
//! **功能域：** 启动器应用外壳、页面状态、交互流程和原生 egui 绘制。
//!
//! 解析启动参数并构造 LauncherApp 初始状态
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `new`
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
    /// 实现 new 相关的业务流程。
    ///
    /// # 参数
    /// - `cc`：由调用方传入的cc。
    /// # 返回
    /// 返回 `Self`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub fn new(cc: &CreationContext<'_>) -> Self {
        theme::setup(&cc.egui_ctx);
        if let Some(gl) = cc.gl.clone() {
            theme::setup_glass(gl);
        }
        if let Some(render_state) = cc.wgpu_render_state.as_ref() {
            theme::setup_wgpu_window(render_state);
        }

        let args: Vec<String> = std::env::args().collect();
        let screenshot_path = args
            .iter()
            .position(|arg| arg == "--screenshot")
            .and_then(|index| args.get(index + 1))
            .map(PathBuf::from);
        let initial_menu = if args.iter().any(|arg| arg == "--profile-menu") {
            Menu::Profile
        } else if args.iter().any(|arg| arg == "--version-menu") {
            Menu::Version
        } else {
            Menu::None
        };
        let preview_tooltip = args.iter().any(|arg| arg == "--preview-tooltip");
        let pointer_glow_preview = args.iter().any(|arg| arg == "--pointer-glow");
        let report_dpi = args.iter().any(|arg| arg == "--dpi-report");
        let official_browser_open = args.iter().any(|arg| arg == "--official-browser");
        let launch_settings_open = args.iter().any(|arg| arg == "--launch-settings");
        let loader_dialog_open = args.iter().any(|arg| arg == "--loader-dialog");
        let download_loader_open = args.iter().any(|arg| arg == "--download-loader");
        let resource_version_menu_open = args.iter().any(|arg| arg == "--resource-version-menu");
        let favorite_editor_open = args.iter().any(|arg| arg == "--favorite-editor");
        let account_name_dialog_open = args.iter().any(|arg| arg == "--account-name");
        let initial_create_tab = if args.iter().any(|arg| arg == "--create-mod") {
            1
        } else {
            0
        };
        let version_folder_menu_open = args.iter().any(|arg| arg == "--version-folders");
        let download_resource_category = args
            .iter()
            .position(|arg| arg == "--download-category")
            .and_then(|index| args.get(index + 1))
            .and_then(|category| match category.as_str() {
                "mod" => Some(ResourceCategory::Mod),
                "modpack" => Some(ResourceCategory::Modpack),
                "shader" => Some(ResourceCategory::Shader),
                "resourcepack" => Some(ResourceCategory::ResourcePack),
                "datapack" => Some(ResourceCategory::Datapack),
                _ => None,
            });
        let version_content_dialog = if args.iter().any(|arg| arg == "--version-export") {
            Some(VersionContentKind::Export)
        } else {
            args.iter()
                .position(|arg| arg == "--version-content")
                .and_then(|index| args.get(index + 1))
                .and_then(|kind| match kind.as_str() {
                    "mods" => Some(VersionContentKind::Mods),
                    "saves" => Some(VersionContentKind::Saves),
                    "shaders" => Some(VersionContentKind::ShaderPacks),
                    "resourcepacks" => Some(VersionContentKind::ResourcePacks),
                    "datapacks" => Some(VersionContentKind::Datapacks),
                    "export" => Some(VersionContentKind::Export),
                    _ => None,
                })
        };
        let loader_base_version = args
            .iter()
            .position(|arg| arg == "--loader-base")
            .and_then(|index| args.get(index + 1))
            .cloned()
            .unwrap_or_else(|| "1.21.1".to_owned());
        let loader_kind = args
            .iter()
            .position(|arg| arg == "--loader-kind")
            .and_then(|index| args.get(index + 1))
            .and_then(|value| match value.as_str() {
                "fabric" => Some(LoaderKind::Fabric),
                "forge" => Some(LoaderKind::Forge),
                "neoforge" => Some(LoaderKind::NeoForge),
                _ => None,
            });
        let loader_versions_receiver = loader_kind
            .map(|loader| crate::loader::start_fetch_versions(loader_base_version.clone(), loader));
        let page = if args.iter().any(|arg| {
            matches!(
                arg.as_str(),
                "--settings"
                    | "--appearance"
                    | "--about"
                    | "--version-settings"
                    | "--game-settings"
                    | "--home-settings"
            )
        }) {
            Page::Settings
        } else if args.iter().any(|arg| arg == "--download") {
            Page::Download
        } else if args.iter().any(|arg| arg == "--versions") || official_browser_open {
            Page::Versions
        } else if args.iter().any(|arg| arg == "--community") {
            Page::Community
        } else if args.iter().any(|arg| arg == "--create") {
            Page::Create
        } else {
            Page::Home
        };
        let settings_section = if args.iter().any(|arg| arg == "--about") {
            SettingsSection::About
        } else if args.iter().any(|arg| arg == "--version-settings") {
            SettingsSection::Versions
        } else if args.iter().any(|arg| arg == "--game-settings") {
            SettingsSection::Game
        } else if args.iter().any(|arg| arg == "--home-settings") {
            SettingsSection::Home
        } else if args.iter().any(|arg| arg == "--appearance") {
            SettingsSection::Appearance
        } else if args.iter().any(|arg| arg == "--launch-settings") {
            SettingsSection::Launch
        } else {
            SettingsSection::General
        };
        let mut settings = LauncherSettings::load();
        if args.iter().any(|arg| arg == "--no-liquid-glass") {
            settings.appearance_mode = theme::AppearanceMode::Simple;
        }
        if args.iter().any(|arg| arg == "--clear-liquid-glass") {
            settings.appearance_mode = theme::AppearanceMode::Frosted;
        }
        if args.iter().any(|arg| arg == "--frosted-glass") {
            settings.appearance_mode = theme::AppearanceMode::Frosted;
        }
        let initial_nav = match page {
            Page::Versions => 1,
            Page::Download => 2,
            Page::Community => 3,
            Page::Create => 4,
            Page::Settings => 5,
            Page::Home => 0,
        };
        let saved_settings = settings.clone();
        let hero_background_texture = settings
            .background_image
            .as_deref()
            .and_then(|path| load_background_texture(&cc.egui_ctx, path));
        let release_icon_texture = load_embedded_texture(
            &cc.egui_ctx,
            include_bytes!("../../Grass.png"),
            "release-icon",
        );
        let snapshot_icon_texture = load_embedded_texture(
            &cc.egui_ctx,
            include_bytes!("../../img.png"),
            "snapshot-icon",
        );
        let loader_icon_texture = load_embedded_texture(
            &cc.egui_ctx,
            include_bytes!("../../img_1.png"),
            "loader-icon",
        );
        let mut favorite_icon_textures = HashMap::new();
        for (url, bytes, name) in builtin_favorite_icon_assets() {
            if let Some(texture) = load_embedded_texture(&cc.egui_ctx, bytes, name) {
                favorite_icon_textures.insert(url.to_owned(), texture);
            }
        }
        let versions = available_versions(settings.game_directory.as_deref());
        let java_runtimes = crate::game::discover_java_runtimes(&settings.game_directories);
        let recency = version_recency_map(&settings, settings.game_directory.as_deref());
        let selected_version = version_menu_order(&versions, &recency)
            .into_iter()
            .find(|index| versions[*index].launchable)
            .unwrap_or(0);
        let initial_resource_version = args
            .iter()
            .position(|arg| arg == "--resource-version")
            .and_then(|index| args.get(index + 1))
            .cloned()
            .unwrap_or_default();
        let initial_resource_category = download_resource_category;
        let resource_search_receiver = initial_resource_category.and_then(|category| {
            if initial_resource_version.is_empty() {
                return None;
            }
            let selected = versions.get(selected_version);
            let loader = selected
                .filter(|version| version.loader_family != "原版")
                .map(|version| version.loader_family.to_lowercase());
            Some(crate::resources::start_search(
                category,
                String::new(),
                initial_resource_version.clone(),
                loader,
            ))
        });
        let initial_profile = versions
            .get(selected_version)
            .map(|version| version.version.as_str())
            .unwrap_or_default();
        let initial_data_directory = settings.game_directory.as_ref().map(|base| {
            if settings.version_isolation {
                base.join("versions").join(initial_profile)
            } else {
                base.clone()
            }
        });
        let recent_sessions = initial_data_directory
            .as_deref()
            .map(crate::sessions::discover_recent_sessions)
            .unwrap_or_default();
        let official_manifest = Some(crate::download::start_manifest_fetch());
        let news_receiver = Some(crate::news::start_fetch());
        let launcher_update_receiver = Some(update::start_latest_release_check(&cc.egui_ctx));
        let mut initial_skin_status = None;
        let initial_skin_pixels = if let Some(path) = settings.player_skin.as_ref() {
            match load_player_skin(path) {
                Ok(pixels) => pixels,
                Err(error) => {
                    initial_skin_status = Some(error);
                    default_skin_pixels()
                }
            }
        } else {
            default_skin_pixels()
        };
        let initial_skin_model_alex =
            if settings.player_skin.is_some() && initial_skin_status.is_none() {
                detect_slim_skin(&initial_skin_pixels)
            } else {
                settings.player_skin_model_alex
            };

        Self {
            selected_version,
            menu: initial_menu,
            version_popup_value: 0.0,
            version_popup_velocity: 0.0,
            profile_popup_value: 0.0,
            profile_popup_velocity: 0.0,
            news: None,
            launch_started: None,
            launch_complete: false,
            toast: None,
            started_at: Instant::now(),
            screenshot_path,
            screenshot_requested: false,
            window_shape_signature: None,
            window_border_color: None,
            game_process_id: None,
            launch_recent: false,
            preview_tooltip,
            pointer_glow_preview,
            pointer_glow_position: Pos2::ZERO,
            pointer_glow_strength: 0.0,
            pointer_glow_initialized: false,
            report_dpi,
            dpi_reported: false,
            page,
            page_history: Vec::new(),
            nav_selected: initial_nav,
            nav_highlight_y: 0.0,
            nav_highlight_velocity: 0.0,
            nav_highlight_initialized: false,
            nav_tooltip_id: u8::MAX,
            nav_tooltip_label: "",
            nav_tooltip_button: Rect::NOTHING,
            nav_tooltip_value: 0.0,
            nav_tooltip_velocity: 0.0,
            nav_tooltip_target: false,
            previous_page: None,
            page_transition_progress: 1.0,
            page_transition_velocity: 0.0,
            page_transition_direction: 1.0,
            community_tab: 0,
            community_scroll: 0.0,
            favorite_editor_open,
            favorite_title_input: String::new(),
            favorite_url_input: String::new(),
            favorite_icon_input: String::new(),
            favorite_editor_error: None,
            favorite_editor_value: if favorite_editor_open { 1.0 } else { 0.0 },
            favorite_editor_velocity: 0.0,
            favorite_editor_source: Rect::NOTHING,
            favorite_editor_target: Rect::NOTHING,
            favorite_editor_card_target: Rect::NOTHING,
            favorite_editor_target_scroll: 0.0,
            favorite_editor_pending_site: None,
            account_name_dialog_open,
            account_name_dialog_value: 0.0,
            account_name_dialog_velocity: 0.0,
            account_name_input: if account_name_dialog_open {
                settings.player_name.clone()
            } else {
                String::new()
            },
            account_name_error: None,
            version_name_target: None,
            version_name_dialog_open: false,
            version_name_dialog_value: 0.0,
            version_name_dialog_velocity: 0.0,
            version_name_dialog_source: Rect::NOTHING,
            version_name_input: String::new(),
            version_name_error: None,
            hero_title_selected_char: 0,
            favorite_icon_receiver: None,
            favorite_icon_attempted: Vec::new(),
            favorite_icon_textures,
            previous_resource_detail: None,
            resource_detail_progress: 1.0,
            resource_detail_velocity: 0.0,
            resource_detail_direction: 1.0,
            create_tab: initial_create_tab,
            skin_pixels: initial_skin_pixels,
            skin_model_alex: initial_skin_model_alex,
            skin_color: [76, 175, 80, 255],
            skin_erasing: false,
            skin_preview_yaw: -0.42,
            skin_preview_pitch: -0.12,
            skin_texture: None,
            skin_dirty: true,
            skin_last_pixel: None,
            skin_preview_last_face: None,
            skin_history: SkinEditHistory::default(),
            skin_stroke_before: None,
            skin_status: initial_skin_status,
            mod_name_input: "My Mod".to_owned(),
            mod_id_input: "mymod".to_owned(),
            mod_package_input: "com.example.mymod".to_owned(),
            mod_version_input: "1.0.0".to_owned(),
            mod_minecraft_input: versions
                .get(selected_version)
                .map(|version| version.game_version.clone())
                .unwrap_or_else(|| "1.21.1".to_owned()),
            mod_loader_index: 0,
            mod_output_directory: None,
            mod_status: None,
            previous_create_tab: None,
            create_transition_progress: 1.0,
            create_transition_velocity: 0.0,
            create_transition_direction: 1.0,
            create_tab_position: initial_create_tab as f32,
            create_tab_velocity: 0.0,
            settings_section,
            settings_highlight_y: 0.0,
            settings_highlight_velocity: 0.0,
            settings_highlight_initialized: false,
            previous_settings_section: None,
            settings_transition_progress: 1.0,
            settings_transition_velocity: 0.0,
            settings_transition_direction: 1.0,
            settings,
            saved_settings,
            launch_settings_open,
            launch_popup_value: 0.0,
            launch_popup_velocity: 0.0,
            hero_background_texture,
            release_icon_texture,
            snapshot_icon_texture,
            loader_icon_texture,
            java_runtimes,
            java_picker_target: None,
            java_picker_anchor: Rect::NOTHING,
            java_picker_scroll: 0.0,
            version_export_open: false,
            export_include_mods: true,
            export_include_config: true,
            export_include_saves: false,
            export_include_resourcepacks: true,
            export_include_shaderpacks: true,
            export_format: ModpackExportFormat::Zip,
            export_format_menu_open: false,
            versions,
            recent_sessions,
            news_receiver,
            news_items: Vec::new(),
            news_error: None,
            news_textures: Vec::new(),
            news_texture_keys: Vec::new(),
            launcher_update_receiver,
            launcher_update_result: None,
            launcher_update_error: None,
            launcher_update_manual: false,
            forced_update_release: None,
            forced_update_receiver: None,
            forced_update_progress: 0.0,
            forced_update_error: None,
            forced_update_installing: false,
            version_scroll: 0.0,
            version_page_selected: selected_version,
            version_page_scroll: (selected_version as f32 * 64.0 - 180.0).max(0.0),
            version_folder_menu_open,
            version_folder_scroll: 0.0,
            version_content_dialog,
            version_content_dialog_open: version_content_dialog.is_some(),
            version_content_dialog_instance: 0,
            version_content_popup_value: if version_content_dialog.is_some() {
                1.0
            } else {
                0.0
            },
            version_content_popup_velocity: 0.0,
            version_content_popup_source: Rect::NOTHING,
            version_content_close_elapsed: 0.0,
            version_content_scroll: 0.0,
            previous_version_detail: None,
            version_detail_progress: 1.0,
            version_detail_velocity: 0.0,
            version_detail_direction: 1.0,
            version_filter: VersionFilter::All,
            official_manifest,
            official_versions: Vec::new(),
            official_error: None,
            official_browser_open,
            official_selected: 0,
            official_scroll: 0.0,
            previous_download_detail: None,
            download_detail_progress: 1.0,
            download_detail_velocity: 0.0,
            download_detail_direction: 1.0,
            download_loader_open,
            download_loader_value: if download_loader_open { 1.0 } else { 0.0 },
            download_loader_velocity: 0.0,
            download_search: String::new(),
            download_resource_category: initial_resource_category,
            download_category_position: resource_category_index(initial_resource_category) as f32,
            download_category_velocity: 0.0,
            download_page_progress: 1.0,
            download_page_velocity: 0.0,
            download_page_direction: 1.0,
            download_switch_pending: None,
            download_switch_receiver: None,
            download_switch_requested_at: None,
            download_switch_texture: None,
            download_switch_texture_size: [0, 0],
            download_switch_queued: None,
            resource_results: Vec::new(),
            resource_search_receiver,
            resource_search_error: None,
            resource_prefetched_key: None,
            resource_version_filter: initial_resource_version,
            resource_target_profile: None,
            resource_version_menu_open,
            resource_version_anchor: Rect::NOTHING,
            resource_version_scroll: 0.0,
            resource_selected: 0,
            resource_scroll: 0.0,
            resource_download_receiver: None,
            resource_download_name: None,
            resource_download_directories: HashMap::new(),
            download_error_log: None,
            official_filter: VersionFilter::All,
            install_receiver: None,
            install_progress: None,
            install_version_id: None,
            install_launch_after: false,
            modpack_import_receiver: None,
            modpack_import_update: None,
            modpack_export_receiver: None,
            loader_dialog_open,
            loader_base_version,
            loader_preinstall: None,
            pending_loader_after_base: None,
            loader_kind,
            loader_versions_receiver,
            loader_versions: Vec::new(),
            loader_versions_error: None,
            loader_version_selected: 0,
            loader_version_scroll: 0.0,
            loader_install_receiver: None,
            loader_install_update: None,
        }
    }
}
