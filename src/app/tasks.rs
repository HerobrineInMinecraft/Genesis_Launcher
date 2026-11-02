//! # 任务与后台逻辑模块（LauncherApp 的方法实现）
//!
//! 本文件是 `LauncherApp` 实现的一部分（通过 `impl LauncherApp` 组织），集中定义
//! 启动器在「资源下载、收藏网站、设置/创建/版本切换动画、页面导航、启动器自更新、
//! 游戏版本/加载器安装、整合包导入导出」等场景下所需的各项动作与状态轮询方法。
//!
//! 主要包含以下几类职责：
//! - **资源搜索与下载**：触发/轮询 Mod、Modpack 等资源的搜索与下载任务（`start_resource_search`、`poll_resource_tasks` 等）。
//! - **收藏网站管理**：收藏站点的列举、编辑器开关与图标预取（`all_favorite_sites`、`ensure_favorite_icons` 等）。
//! - **页面与分类切换动画**：下载分类、设置分区、创建页签、版本详情等的弹性动画切换（`begin_download_category_switch`、`switch_settings_section` 等）。
//! - **导航与历史**：底部导航页切换及返回历史（`switch_nav_page`、`go_back_page` 等）。
//! - **版本/加载器/整合包安装**：发起并轮询各类安装任务（`start_official_install`、`start_loader_install`、`poll_modpack_import` 等）。
//! - **启动器自更新**：检查 GitHub Release 并执行强制更新（`start_launcher_update_check`、`poll_forced_update` 等）。
//! - **统一后台轮询**：`poll_background_tasks` 将多个后台接收器统一轮询、收口。
//!
//! 这些方法大多通过 `std::sync::mpsc` 通道与后台线程通信：方法发起任务时会拿到
//! `Receiver`，并在各自的 `poll_*` 函数中用 `try_recv` 非阻塞地取出结果，再据此更新 UI 状态。

use super::*;

impl LauncherApp {
    /// 发起一次资源（Mod / Modpack 等）搜索任务。
    ///
    /// 本函数会先确定目标游戏版本（`resource_version_filter`），若其为空则尝试取官方最新稳定版，
    /// 否则回退到当前版本或用户已有清单。随后调用 `crate::resources::start_search` 在后台线程执行搜索，
    /// 并把返回的 `Receiver` 存入 `resource_search_receiver` 供 `poll_resource_tasks` 轮询。
    ///
    /// 调用前会清空上一轮搜索结果、错误、选中项及滚动位置等状态，确保一次全新的搜索。
    pub(super) fn start_resource_search(&mut self) {
        let Some(category) = self.download_resource_category else {
            return;
        };
        // 若资源版本筛选为空，需要自动决定一个目标版本
        if self.resource_version_filter.trim().is_empty() {
            // 优先使用官方清单里最新的 release 版本 id
            if let Some(latest) = self
                .official_versions
                .iter()
                .find(|version| version.kind == "release")
                .map(|version| version.id.clone())
            {
                self.resource_version_filter = latest;
            } else if self.official_manifest.is_some() {
                // 清单仍在后台加载中：提示等待并暂缓搜索
                self.resource_search_error = Some("正在获取 Minecraft 版本列表，请稍候".to_owned());
                return;
            } else {
                // 清单尚未开始获取：退回到当前正在使用的游戏版本
                self.resource_version_filter = self.version().game_version.clone();
            }
        }
        let loader = self.resource_loader();
        self.resource_search_receiver = Some(crate::resources::start_search(
            category,
            self.download_search.trim().to_owned(),
            self.resource_version_filter.clone(),
            loader,
        ));
        self.resource_results.clear();
        self.resource_search_error = None;
        self.resource_selected = 0;
        self.resource_scroll = 0.0;
        self.previous_resource_detail = None;
        self.resource_detail_progress = 1.0;
        self.resource_detail_velocity = 0.0;
    }

    /// 根据给定的游戏版本号，找出一个用于资源下载/加载器判断的「目标版本」。
    ///
    /// # 参数
    /// - `game_version`：要匹配的游戏版本（如 `"1.20.1"`）。
    ///
    /// # 返回值
    /// 返回匹配到的 `GameVersion` 引用；若找不到则返回 `None`。
    ///
    /// # 选择优先级（按代码顺序）
    /// 1. 若设定了 `resource_target_profile`，优先匹配该 profile 且游戏版本一致的版本；
    /// 2. 否则若当前版本的游戏版本相同，直接返回当前版本；
    /// 3. 否则匹配「同游戏版本且可启动」的版本；
    /// 4. 否则匹配任意「同游戏版本」的版本。
    ///
    /// 该顺序保证尽量选取用户明确指定的、可启动的版本作为资源目标。
    pub(super) fn resource_target_version(&self, game_version: &str) -> Option<&GameVersion> {
        self.resource_target_profile
            .as_deref()
            .and_then(|profile| {
                self.versions
                    .iter()
                    .find(|version| version.version == profile)
            })
            .filter(|version| version.game_version == game_version)
            .or_else(|| (self.version().game_version == game_version).then(|| self.version()))
            .or_else(|| {
                self.versions
                    .iter()
                    .find(|version| version.game_version == game_version && version.launchable)
            })
            .or_else(|| {
                self.versions
                    .iter()
                    .find(|version| version.game_version == game_version)
            })
    }

    /// 计算当前资源搜索对应的「加载器家族名」（小写字符串），用于区分原版/Forge/Fabric 等。
    ///
    /// # 返回值
    /// - 仅当下载分类为 `Mod` 或 `Modpack`，且目标版本存在、加载器家族不是「原版」时，
    ///   才返回 `Some(loader_family.to_lowercase())`；
    /// - 其余情况（分类不是 Mod/Modpack，或加载器为原版）返回 `None`，表示「无需按加载器过滤」。
    pub(super) fn resource_loader(&self) -> Option<String> {
        let version = self.resource_target_version(&self.resource_version_filter)?;
        matches!(
            self.download_resource_category,
            Some(ResourceCategory::Mod | ResourceCategory::Modpack)
        )
        .then(|| version.loader_family.to_lowercase())
        .filter(|loader| loader != "原版")
    }

    /// 计算某类资源（Mod/Modpack/…）默认下载到哪个目录。
    ///
    /// # 参数
    /// - `category`：资源分类（决定目标子目录，如 `mods`、`modpacks`）。
    /// - `game_version`：游戏版本，用于在开启「版本隔离」时定位到对应版本目录。
    ///
    /// # 返回值
    /// 返回目标目录的 `PathBuf`；若游戏主目录未设置则返回 `None`。
    ///
    /// # 行为说明
    /// - 若 `resource_download_directories` 中已缓存该分类目录，直接返回缓存；
    /// - 若关闭「版本隔离」，资源直接放在游戏主目录下；
    /// - 若开启「版本隔离」，则放到 `versions/<profile>` 子目录，`profile` 优先取目标版本，
    ///   否则回退为 `game_version`；
    /// - 最终目录为 `基础目录 / category.target_folder()`。
    pub(super) fn resource_default_directory(
        &self,
        category: ResourceCategory,
        game_version: &str,
    ) -> Option<PathBuf> {
        if let Some(cached) = self.resource_download_directories.get(&category) {
            return Some(cached.clone());
        }
        let base = self.settings.game_directory.as_ref()?;
        let data_directory = if !self.settings.version_isolation {
            base.clone()
        } else {
            let profile = self
                .resource_target_version(game_version)
                .map(|version| version.version.as_str())
                .unwrap_or(game_version);
            base.join("versions").join(profile)
        };
        Some(data_directory.join(category.target_folder()))
    }

    /// 从任意入口「打开资源下载页」并定位到指定分类/版本。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文，用于请求重绘。
    /// - `category`：要切换到的资源分类。
    /// - `game_version`：目标游戏版本字符串。
    /// - `target_profile`：资源下载所针对的版本 profile 名。
    ///
    /// # 副作用
    /// 可能切换导航页到「下载页」(`Page::Download`)、清空搜索框、必要时重新发起资源搜索，
    /// 并通过 `ctx.request_repaint()` 触发一次重绘。
    pub(super) fn open_resource_download(
        &mut self,
        ctx: &egui::Context,
        category: ResourceCategory,
        game_version: String,
        target_profile: String,
    ) {
        let version_changed = self.resource_version_filter != game_version;
        self.resource_target_profile = Some(target_profile);
        if version_changed {
            self.resource_version_filter = game_version;
            self.resource_search_receiver = None;
            self.resource_results.clear();
        }
        // 判断分类是否发生变化：若变了则直接应用切换（不带动画）
        let category_changed = self.download_resource_category != Some(category);
        if category_changed {
            self.apply_download_category_switch(Some(category));
        }
        self.switch_nav_page(2, Page::Download);
        self.download_search.clear();
        // 仅当分类未变、但版本变了时，才在此处重新发起搜索（分类变了由 apply 内部处理）
        if !category_changed && version_changed {
            self.start_resource_search();
        }
        ctx.request_repaint();
    }

    /// 发起「下载分类切换」流程，带有可选的过场动画（截图→纹理淡入淡出）。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文，用于请求截图与重绘。
    /// - `category`：要切换到的目标分类（`None` 表示回到无分类状态）。
    ///
    /// # 行为说明
    /// - 若已有待切换任务在排队，则更新/清空待切换目标；
    /// - 若动画被禁用，立即应用切换；
    /// - 若当前切换动画仍在进行（进度未完成、速度未归零、或截图纹理未清），
    ///   则把目标暂存到 `download_switch_queued`，等动画结束后再处理；
    /// - 否则请求一次窗口截图，把截图作为「离场纹理」，并记下起始时间，供
    ///   `poll_download_category_switch` 在截图就绪后真正应用切换。
    pub(super) fn begin_download_category_switch(
        &mut self,
        ctx: &egui::Context,
        category: Option<ResourceCategory>,
    ) {
        if self.download_switch_pending.is_some() {
            if self.download_resource_category == category {
                self.download_switch_pending = None;
                self.download_switch_receiver = None;
                self.download_switch_requested_at = None;
            } else {
                self.download_switch_pending = Some(category);
            }
            ctx.request_repaint();
            return;
        }
        if self.download_resource_category == category {
            self.download_switch_queued = None;
            return;
        }
        if !self.settings.animations_enabled() {
            self.download_switch_queued = None;
            self.apply_download_category_switch(category);
            return;
        }
        if self.download_page_progress < 0.999
            || self.download_page_velocity.abs() > 0.01
            || self.download_switch_texture.is_some()
        {
            self.download_switch_queued = Some(category);
            ctx.request_repaint();
            return;
        }
        // 创建一个 mpsc 通道，用于把截图结果从 egui 的回调线程传回主线程
        let (sender, receiver) = std::sync::mpsc::channel();
        // 克隆一份 egui 上下文，供回调中请求重绘（回调运行在 egui 内部线程）
        let repaint_ctx = ctx.clone();
        // 向 egui 请求一次窗口截图；截图准备好后通过闭包把图像发送到通道并触发重绘
        ctx.request_screenshot(move |image| {
            let _ = sender.send(image);
            repaint_ctx.request_repaint();
        });
        self.download_switch_pending = Some(category);
        self.download_switch_receiver = Some(receiver);
        self.download_switch_requested_at = Some(Instant::now());
        ctx.request_repaint();
    }

    /// 真正执行下载分类的切换（动画状态重置 + 数据状态切换）。
    ///
    /// # 参数
    /// - `category`：目标分类。
    ///
    /// # 副作用
    /// 重置切换动画方向/进度/速度，写入 `download_resource_category`，关闭版本菜单并清空搜索框；
    /// 若分类为 `Some` 则立即发起资源搜索，否则清空已有搜索结果/错误/选中项。
    /// 最后清理 `download_switch_pending` 等「待切换」临时状态。
    pub(super) fn apply_download_category_switch(&mut self, category: Option<ResourceCategory>) {
        let previous_index = resource_category_index(self.download_resource_category);
        let next_index = resource_category_index(category);
        self.download_page_direction = if next_index > previous_index {
            1.0
        } else {
            -1.0
        };
        self.download_page_progress = 0.0;
        self.download_page_velocity = 0.0;
        self.download_resource_category = category;
        self.resource_version_menu_open = false;
        self.download_search.clear();
        if category.is_some() {
            self.start_resource_search();
        } else {
            self.resource_search_receiver = None;
            self.resource_results.clear();
            self.resource_search_error = None;
            self.resource_selected = 0;
            self.resource_scroll = 0.0;
        }
        self.download_switch_pending = None;
        self.download_switch_receiver = None;
        self.download_switch_requested_at = None;
    }

    /// 轮询下载分类切换：等待截图就绪后再应用切换，或处理超时/断连。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文，用于在截图纹理创建后加载到 GPU 并请求重绘。
    ///
    /// # 行为说明
    /// - 若无 `download_switch_pending` 则直接返回；
    /// - 若无接收器（异常情况），立即应用切换；
    /// - 收到截图：将其作为离场纹理上传，并真正应用分类切换；
    /// - 截图尚未就绪但距请求不足 550ms：继续等待；
    /// - 截图为空且已超时，或通道断开：放弃等待，直接应用切换。
    pub(super) fn poll_download_category_switch(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.download_switch_pending else {
            return;
        };
        let Some(receiver) = self.download_switch_receiver.take() else {
            self.apply_download_category_switch(pending);
            return;
        };
        match receiver.try_recv() {
            Ok(image) => {
                self.download_switch_texture_size = image.size;
                self.download_switch_texture = Some(ctx.load_texture(
                    "download-category-outgoing",
                    image.as_ref().clone(),
                    egui::TextureOptions::LINEAR,
                ));
                self.apply_download_category_switch(pending);
                ctx.request_repaint();
            }
            // 截图尚未就绪，但只要距截图请求不超过 550ms，就继续等待（避免无限挂起）
            Err(std::sync::mpsc::TryRecvError::Empty)
                if self
                    .download_switch_requested_at
                    .is_some_and(|started| started.elapsed() < Duration::from_millis(550)) =>
            {
                self.download_switch_receiver = Some(receiver);
            }
            Err(std::sync::mpsc::TryRecvError::Empty)
            | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.apply_download_category_switch(pending);
                ctx.request_repaint();
            }
        }
    }

    /// 预取（提前缓存）当前分类/版本/加载器组合下的资源列表，避免用户进入时等待。
    ///
    /// 通过 `resource_prefetched_key`（由游戏版本 + 加载器组合而成）判断是否已预取过，
    /// 已预取则跳过；否则记录 key 并调用 `crate::resources::start_prefetch` 在后台预拉取。
    pub(super) fn prefetch_resource_categories(&mut self) {
        let Some(category) = self.download_resource_category else {
            return;
        };
        let game_version = self.resource_version_filter.clone();
        let loader = self.resource_loader();
        let prefetch_key = format!("{game_version}:{}", loader.as_deref().unwrap_or_default());
        if self.resource_prefetched_key.as_deref() == Some(&prefetch_key) {
            return;
        }
        self.resource_prefetched_key = Some(prefetch_key);
        crate::resources::start_prefetch(category, game_version, loader);
    }

    /// 轮询两类资源相关后台任务：资源搜索结果、资源下载完成情况。
    ///
    /// 对 `resource_search_receiver`：成功则填充结果并触发预取；失败则记录错误；
    /// 通道空则保留接收器下次再查；断开则提示「连接已断开」。
    /// 对 `resource_download_receiver`：成功/失败都以 toast 提示，断开同样提示。
    /// 两个接收器各自独立处理，互不影响。
    pub(super) fn poll_resource_tasks(&mut self) {
        if let Some(receiver) = self.resource_search_receiver.take() {
            match receiver.try_recv() {
                Ok(Ok(results)) => {
                    self.resource_results = results;
                    self.resource_search_error = None;
                    self.resource_selected = 0;
                    self.resource_scroll = 0.0;
                    self.previous_resource_detail = None;
                    self.resource_detail_progress = 1.0;
                    self.resource_detail_velocity = 0.0;
                    self.prefetch_resource_categories();
                }
                Ok(Err(error)) => {
                    self.resource_search_error = Some(error.clone());
                    self.report_download_error("资源搜索", error);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.resource_search_receiver = Some(receiver);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    let error = "资源下载源搜索连接已断开";
                    self.resource_search_error = Some(error.to_owned());
                    self.report_download_error("资源搜索", error);
                }
            }
        }
        if let Some(receiver) = self.resource_download_receiver.take() {
            match receiver.try_recv() {
                Ok(Ok(path)) => {
                    self.resource_download_name = None;
                    self.show_toast(format!("已下载到 {}", path.display()));
                }
                Ok(Err(error)) => {
                    let name = self
                        .resource_download_name
                        .take()
                        .unwrap_or_else(|| "未知资源".to_owned());
                    self.show_toast(format!("资源下载失败：{error}"));
                    self.report_download_error("资源下载", format!("资源：{name}\n{error}"));
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.resource_download_receiver = Some(receiver);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.resource_download_name = None;
                    self.show_toast("资源下载连接已断开");
                    self.report_download_error("资源下载", "资源下载连接已断开");
                }
            }
        }
    }

    /// 确保所有收藏站点都发起过图标获取请求（去重 + 去重已尝试集合）。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文，用于请求重绘。
    ///
    /// # 行为说明
    /// 若已有图标获取任务在进行则跳过；否则遍历所有收藏站点，过滤掉「已有纹理」或
    /// 「已尝试过」的站点，剩余的封装为 `FavoriteIconRequest` 提交后台获取，
    /// 并把其 key 加入 `favorite_icon_attempted` 防止重复请求。
    pub(super) fn ensure_favorite_icons(&mut self, ctx: &egui::Context) {
        if self.favorite_icon_receiver.is_some() {
            return;
        }
        let requests = self
            .all_favorite_sites()
            .into_iter()
            // 过滤掉：已有图标纹理的站点，以及曾经尝试过（可能失败）的站点，避免重复请求
            .filter(|site| {
                !self.favorite_icon_textures.contains_key(&site.url)
                    && !self
                        .favorite_icon_attempted
                        .iter()
                        .any(|known| known == &site.url)
            })
            .map(|site| crate::favorites::FavoriteIconRequest {
                key: site.url.clone(),
                site_url: site.url,
                icon_source: site.icon_source,
            })
            .collect::<Vec<_>>();
        if requests.is_empty() {
            return;
        }
        self.favorite_icon_attempted
            .extend(requests.iter().map(|request| request.key.clone()));
        self.favorite_icon_receiver = Some(crate::favorites::start_fetch_icons(requests));
        ctx.request_repaint();
    }

    /// 轮询收藏站点图标下载结果，将下载到的字节解码为图片并上传为 egui 纹理。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文，用于创建纹理并请求重绘。
    ///
    /// # 行为说明
    /// 收到图标字节后，用 `image::load_from_memory` 解码（失败则跳过该站点），
    /// 经 `color_image_for_context` 转为 egui 可用纹理并以 `favorite-icon:<url>` 命名缓存；
    /// 通道空则保留接收器；断开则静默结束（不再重试）。
    pub(super) fn poll_favorite_icons(&mut self, ctx: &egui::Context) {
        let Some(receiver) = self.favorite_icon_receiver.take() else {
            return;
        };
        match receiver.try_recv() {
            Ok(icons) => {
                for (url, bytes) in icons {
                    // 把下载到的原始字节解码为图像；解码失败（非图片）则跳过该站点
                    let Ok(image) = image::load_from_memory(&bytes) else {
                        continue;
                    };
                    // 将解码后的图像转换为 egui 当前上下文所需的 ColorImage 像素格式
                    let color_image = color_image_for_context(ctx, image);
                    // 以「favorite-icon:<url>」为键把图像上传为 GPU 纹理，使用线性过滤
                    let texture = ctx.load_texture(
                        format!("favorite-icon:{url}"),
                        color_image,
                        egui::TextureOptions::LINEAR,
                    );
                    self.favorite_icon_textures.insert(url, texture);
                }
                ctx.request_repaint();
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.favorite_icon_receiver = Some(receiver);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {}
        }
    }

    /// 汇总所有收藏站点：内置站点（`BUILTIN_SITES`）在前，用户自定义站点（`settings.favorite_sites`）在后。
    ///
    /// # 返回值
    /// 返回 `FavoriteSite` 列表。内置站点只提供标题与 URL，`icon_source` 留空（交由图标获取逻辑自动解析）。
    pub(super) fn all_favorite_sites(&self) -> Vec<FavoriteSite> {
        let mut sites = crate::favorites::BUILTIN_SITES
            .iter()
            .map(|(title, url)| FavoriteSite {
                title: (*title).to_owned(),
                url: (*url).to_owned(),
                icon_source: String::new(),
            })
            .collect::<Vec<_>>();
        sites.extend(self.settings.favorite_sites.iter().cloned());
        sites
    }

    /// 打开「添加收藏网站」编辑器，并重置其动画与输入状态。
    ///
    /// # 参数
    /// - `source`：编辑器弹出动画的起点矩形（用于弹性展开动画），通常对应触发按钮的位置。
    pub(super) fn open_favorite_editor(&mut self, source: Rect) {
        self.favorite_editor_open = true;
        self.favorite_editor_value = 0.0;
        self.favorite_editor_velocity = 0.0;
        self.favorite_editor_source = source;
        self.favorite_editor_target = source;
        self.favorite_editor_pending_site = None;
        self.favorite_editor_error = None;
        self.favorite_title_input.clear();
        self.favorite_url_input.clear();
        self.favorite_icon_input.clear();
    }

    /// 取消收藏编辑器：标记为关闭并设置收起动画目标，必要时直接结束。
    ///
    /// 若当前未打开则直接返回。关闭时将 `favorite_editor_target` 设回 `source`（即缩回原处）；
    /// 若动画被禁用，则立即调用 `finish_favorite_editor` 收尾（清理待添加站点与输入）。
    pub(super) fn cancel_favorite_editor(&mut self) {
        if !self.favorite_editor_open {
            return;
        }
        self.favorite_editor_open = false;
        self.favorite_editor_target = self.favorite_editor_source;
        self.favorite_editor_pending_site = None;
        self.favorite_editor_error = None;
        if !self.settings.animations_enabled() {
            self.finish_favorite_editor();
        }
    }

    /// 校验并添加一个新的收藏网站。仅在编辑器打开时由确认按钮调用。
    ///
    /// # 参数
    /// - `target_card`：添加动画的落点矩形（新收藏卡片出现的位置）。
    ///
    /// # 行为说明（会修改多处状态）
    /// 1. 用 `normalize_site_url` 规范化用户输入的 URL，失败则写入错误提示并返回；
    /// 2. 若 URL 与已有收藏（忽略大小写）重复，提示「已经收藏过了」；
    /// 3. 标题为空时用域名作为标题；图标来源用 `normalize_icon_source` 规范化；
    /// 4. 通过校验后，关闭编辑器、设置落点、将站点暂存到 `favorite_editor_pending_site`，
    ///    若动画禁用则立即 `finish_favorite_editor` 真正写入设置。
    pub(super) fn add_favorite_site(&mut self, target_card: Rect) {
        // 规范化用户输入的网址（补全协议、去除尾随空白等）；失败则记录错误并返回
        let url = match crate::favorites::normalize_site_url(&self.favorite_url_input) {
            Ok(url) => url,
            Err(error) => {
                self.favorite_editor_error = Some(error);
                return;
            }
        };
        if self
            .all_favorite_sites()
            .iter()
            .any(|site| site.url.eq_ignore_ascii_case(&url))
        {
            self.favorite_editor_error = Some("这个网站已经收藏过了".to_owned());
            return;
        }
        let title = self.favorite_title_input.trim();
        let title = if title.is_empty() {
            crate::favorites::site_domain(&url)
        } else {
            title.to_owned()
        };
        // 规范化图标来源（可为空、URL 或 data URI）；失败则记录错误并返回
        let icon_source = match crate::favorites::normalize_icon_source(&self.favorite_icon_input) {
            Ok(source) => source,
            Err(error) => {
                self.favorite_editor_error = Some(error);
                return;
            }
        };
        self.favorite_editor_open = false;
        self.favorite_editor_target = target_card;
        self.community_scroll = self.favorite_editor_target_scroll;
        self.favorite_editor_pending_site = Some(FavoriteSite {
            title,
            url,
            icon_source,
        });
        self.favorite_editor_error = None;
        if !self.settings.animations_enabled() {
            self.finish_favorite_editor();
        }
    }

    /// 收藏编辑器动画/流程的最终收尾：把待添加站点真正写入设置并弹出提示，清空输入。
    ///
    /// 若存在 `favorite_editor_pending_site`，则推入 `settings.favorite_sites`，
    /// 并根据是否带自定义图标提示「正在加载自定义图标」或「正在获取网站图标」。
    /// 随后清空标题/URL/图标输入框、错误、动画值及起点/落点矩形。
    pub(super) fn finish_favorite_editor(&mut self) {
        if let Some(site) = self.favorite_editor_pending_site.take() {
            let custom_icon = !site.icon_source.is_empty();
            self.settings.favorite_sites.push(site);
            self.show_toast(if custom_icon {
                "收藏网站已添加，正在加载自定义图标"
            } else {
                "收藏网站已添加，正在获取网站图标"
            });
        }
        self.favorite_title_input.clear();
        self.favorite_url_input.clear();
        self.favorite_icon_input.clear();
        self.favorite_editor_error = None;
        self.favorite_editor_value = 0.0;
        self.favorite_editor_velocity = 0.0;
        self.favorite_editor_source = Rect::NOTHING;
        self.favorite_editor_target = Rect::NOTHING;
    }

    /// 每帧更新收藏编辑器的弹性弹出/收起动画。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文，用于读取帧间隔并安排后续重绘。
    ///
    /// # 行为说明
    /// - 动画禁用时直接把动画值设为目标（0 或 1），关闭完成则收尾；
    /// - 当动画值已接近 0 且编辑器关闭、且起点非 `NOTHING` 时，执行收尾；
    /// - 否则用 `spring_popup` 计算弹性动画，未稳定时请求 16ms 后重绘；
    ///   稳定且已关闭时调用 `finish_favorite_editor` 收尾。
    pub(super) fn update_favorite_editor_animation(&mut self, ctx: &egui::Context) {
        let target = if self.favorite_editor_open { 1.0 } else { 0.0 };
        if !self.settings.animations_enabled() {
            self.favorite_editor_value = target;
            self.favorite_editor_velocity = 0.0;
            if !self.favorite_editor_open {
                self.finish_favorite_editor();
            }
            return;
        }
        if self.favorite_editor_value <= 0.002 && !self.favorite_editor_open {
            if self.favorite_editor_source != Rect::NOTHING {
                self.finish_favorite_editor();
            }
            return;
        }
        // 取本帧稳定时间增量并夹紧到 [1ms, 50ms]，避免帧率异常导致动画跳变
        let delta = ctx.input(|input| input.stable_dt).clamp(0.001, 0.05);
        // 用弹性（spring）动画把弹出值推向目标值 0/1，返回是否尚未稳定
        if spring_popup(
            &mut self.favorite_editor_value,
            &mut self.favorite_editor_velocity,
            target,
            delta,
        ) {
            ctx.request_repaint_after(Duration::from_millis(16));
        } else if !self.favorite_editor_open {
            self.finish_favorite_editor();
        }
    }

    /// 切换设置页面的子分区，并准备过渡动画。
    ///
    /// # 参数
    /// - `section`：目标设置分区。
    ///
    /// # 行为说明
    /// 若目标与当前相同则忽略。开启动画时记录上一分区、重置过渡进度/速度，并根据
    /// 目标索引与当前索引的大小决定动画方向（下/上）；关闭动画时直接置进度为 1（无过渡）。
    pub(super) fn switch_settings_section(&mut self, section: SettingsSection) {
        if self.settings_section == section {
            return;
        }
        if self.settings.animations_enabled() {
            self.previous_settings_section = Some(self.settings_section);
            self.settings_transition_progress = 0.0;
            self.settings_transition_velocity = 0.0;
            self.settings_transition_direction = if section.index() > self.settings_section.index()
            {
                1.0
            } else {
                -1.0
            };
        } else {
            self.previous_settings_section = None;
            self.settings_transition_progress = 1.0;
            self.settings_transition_velocity = 0.0;
        }
        self.settings_section = section;
    }

    /// 切换「创建」页内的子页签（如创建整合包/实例等）。
    ///
    /// # 参数
    /// - `index`：目标页签索引。
    ///
    /// # 行为说明
    /// 相同索引则忽略。开启动画时记录上一页签、重置过渡进度/速度、按索引大小设方向；
    /// 关闭动画时直接把 `create_tab_position` 设为目标索引（无滑动效果）。
    pub(super) fn switch_create_tab(&mut self, index: usize) {
        if self.create_tab == index {
            return;
        }
        if self.settings.animations_enabled() {
            self.previous_create_tab = Some(self.create_tab);
            self.create_transition_progress = 0.0;
            self.create_transition_velocity = 0.0;
            self.create_transition_direction = if index > self.create_tab { 1.0 } else { -1.0 };
        } else {
            self.previous_create_tab = None;
            self.create_transition_progress = 1.0;
            self.create_transition_velocity = 0.0;
            self.create_tab_position = index as f32;
            self.create_tab_velocity = 0.0;
        }
        self.create_tab = index;
    }

    /// 每帧更新「创建」页的页签滑块位置与内容过渡动画。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文，用于读取帧间隔并安排 16ms 后重绘。
    ///
    /// # 行为说明
    /// 动画禁用时直接把状态设为「已完成」。否则用 `spring_smooth` 分别平滑：
    /// - 页签滑块位置 `create_tab_position` 趋向当前 `create_tab`；
    /// - 内容过渡进度 `create_transition_progress` 趋向 1（若有上一页签）；
    ///   内容过渡完成后清除 `previous_create_tab`。
    ///
    /// 任一动画仍在进行就请求后续重绘。
    pub(super) fn update_create_animation(&mut self, ctx: &egui::Context) {
        if !self.settings.animations_enabled() {
            self.previous_create_tab = None;
            self.create_transition_progress = 1.0;
            self.create_transition_velocity = 0.0;
            self.create_tab_position = self.create_tab as f32;
            self.create_tab_velocity = 0.0;
            return;
        }
        let delta = ctx.input(|input| input.stable_dt).clamp(0.001, 0.05);
        let tab_active = spring_smooth(
            &mut self.create_tab_position,
            &mut self.create_tab_velocity,
            self.create_tab as f32,
            delta,
        );
        let content_active = if self.previous_create_tab.is_some() {
            let active = spring_smooth(
                &mut self.create_transition_progress,
                &mut self.create_transition_velocity,
                1.0,
                delta,
            );
            if !active {
                self.previous_create_tab = None;
                self.create_transition_progress = 1.0;
            }
            active
        } else {
            false
        };
        if tab_active || content_active {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    /// 切换「版本」页内的版本详情视图（不同实例/版本之间的左右切换动画）。
    ///
    /// # 参数
    /// - `index`：目标版本在列表中的索引。
    ///
    /// # 副作用
    /// 重置版本内容对话框、滚动位置、导出对话框等临时状态，并调用
    /// `refresh_recent_sessions` 刷新最近游玩会话。开启动画时记录上一索引与方向。
    pub(super) fn switch_version_detail(&mut self, index: usize) {
        if self.version_page_selected == index {
            return;
        }
        if self.settings.animations_enabled() {
            self.previous_version_detail = Some(self.version_page_selected);
            self.version_detail_progress = 0.0;
            self.version_detail_velocity = 0.0;
            self.version_detail_direction = if index > self.version_page_selected {
                1.0
            } else {
                -1.0
            };
        }
        self.version_page_selected = index;
        self.version_content_dialog = None;
        self.version_content_scroll = 0.0;
        self.version_export_open = false;
        self.refresh_recent_sessions();
    }

    /// 打开版本内容对话框（如模组列表、截图等），并重置其弹出动画状态。
    ///
    /// # 参数
    /// - `kind`：对话框类型，决定展示哪类版本内容。
    /// - `source`：弹出动画起点矩形（用于弹性展开）。
    pub(super) fn open_version_content_dialog(&mut self, kind: VersionContentKind, source: Rect) {
        self.version_content_dialog = Some(kind);
        self.version_content_dialog_open = true;
        self.version_content_dialog_instance = self.version_content_dialog_instance.wrapping_add(1);
        // 启用动画时从完全收起位置重新展开；禁用动画时直接以完整内容打开。
        self.version_content_popup_value = if self.settings.animations_enabled() {
            0.0
        } else {
            1.0
        };
        self.version_content_popup_velocity = 0.0;
        self.version_content_popup_source = source;
        self.version_content_close_elapsed = 0.0;
        self.version_content_scroll = 0.0;
        self.export_format_menu_open = false;
    }

    /// 关闭版本内容对话框，触发收起动画（将关闭计时清零并标记未打开）。
    ///
    /// 实际收尾由动画系统后续处理；此处只是启动「关闭」过渡。
    pub(super) fn close_version_content_dialog(&mut self) {
        self.version_content_close_elapsed = 0.0;
        self.version_content_dialog_open = false;
        self.export_format_menu_open = false;
    }

    /// 切换下载页中选中的官方版本（用于后续安装/加载器选择）。
    ///
    /// # 参数
    /// - `index`：目标官方版本在 `official_versions` 中的索引。
    ///
    /// # 副作用
    /// 同步更新 `loader_base_version` 为基础版本号，重置加载器选择状态
    /// （`reset_download_loader`），并记录切换动画方向。相同索引则忽略。
    pub(super) fn switch_download_detail(&mut self, index: usize) {
        if self.official_selected == index {
            return;
        }
        if let Some(version) = self.official_versions.get(index) {
            self.loader_base_version = version.id.clone();
        }
        if self.settings.animations_enabled() {
            self.previous_download_detail = Some(self.official_selected);
            self.download_detail_progress = 0.0;
            self.download_detail_velocity = 0.0;
            self.download_detail_direction = if index > self.official_selected {
                1.0
            } else {
                -1.0
            };
        }
        self.reset_download_loader();
        self.official_selected = index;
    }

    /// 切换资源搜索结果中的选中项，并准备详情切换动画。
    ///
    /// # 参数
    /// - `index`：目标资源项在 `resource_results` 中的索引。
    ///
    /// # 行为说明
    /// 相同索引则忽略。开启动画时记录上一选中项、重置进度/速度、按索引大小设方向；
    /// 关闭动画时直接把进度置 1（无过渡）并清空上一选中项。
    pub(super) fn switch_resource_detail(&mut self, index: usize) {
        if self.resource_selected == index {
            return;
        }
        if self.settings.animations_enabled() {
            self.previous_resource_detail = Some(self.resource_selected);
            self.resource_detail_progress = 0.0;
            self.resource_detail_velocity = 0.0;
            self.resource_detail_direction = if index > self.resource_selected {
                1.0
            } else {
                -1.0
            };
        } else {
            self.previous_resource_detail = None;
            self.resource_detail_progress = 1.0;
            self.resource_detail_velocity = 0.0;
        }
        self.resource_selected = index;
    }

    /// 重置下载页中「加载器」相关的所有临时状态（关闭对话框、清空已选加载器与版本列表等）。
    ///
    /// 在切换官方版本或重新选择时被调用，避免残留上一版本的加载器选择。
    pub(super) fn reset_download_loader(&mut self) {
        self.download_loader_open = false;
        self.loader_kind = None;
        self.loader_versions_receiver = None;
        self.loader_versions.clear();
        self.loader_versions_error = None;
        self.loader_version_selected = 0;
        self.loader_version_scroll = 0.0;
    }

    /// 切换底部导航页（记录历史版本）。是 `set_nav_page` 的便捷封装。
    ///
    /// # 参数
    /// - `index`：导航项索引（对应底部导航栏的位置）。
    /// - `page`：要切换到的页面枚举。
    pub(super) fn switch_nav_page(&mut self, index: usize, page: Page) {
        self.set_nav_page(index, page, true);
    }

    /// 设置当前导航页与索引，可选地记录历史，并重置各页面的临时状态。
    ///
    /// # 参数
    /// - `index`：导航项索引。
    /// - `page`：目标页面。
    /// - `record_history`：为 `true` 且页面发生切换时，把旧页面压入 `page_history`（最多保留 32 项）。
    ///
    /// # 副作用（状态清理）
    /// 切换页面时会根据目标页面关闭不相关的弹层：离开设置页清空设置过渡状态；
    /// 离开版本页关闭文件夹菜单/内容对话框/导出对话框；离开首页时关闭版本菜单；
    /// 离开下载页关闭版本筛选菜单；离开社区页关闭收藏编辑器。同时清空 Java 选择器目标并写入最终状态。
    pub(super) fn set_nav_page(&mut self, index: usize, page: Page, record_history: bool) {
        if record_history
            && page != self.page
            && self.page_history.last().copied() != Some(self.page)
        {
            self.page_history.push(self.page);
            if self.page_history.len() > 32 {
                self.page_history.remove(0);
            }
        }
        if self.settings.animations_enabled() && self.nav_selected != index {
            self.previous_page = Some(self.page);
            self.page_transition_progress = 0.0;
            self.page_transition_velocity = 0.0;
            self.page_transition_direction = if index > self.nav_selected { 1.0 } else { -1.0 };
        } else {
            self.previous_page = None;
            self.page_transition_progress = 1.0;
            self.page_transition_velocity = 0.0;
        }
        if page != Page::Settings {
            self.previous_settings_section = None;
            self.settings_transition_progress = 1.0;
            self.settings_transition_velocity = 0.0;
        }
        if page != Page::Versions {
            self.version_folder_menu_open = false;
            self.version_content_dialog = None;
            self.version_export_open = false;
        }
        if page != Page::Home && self.menu == Menu::Version {
            self.menu = Menu::None;
        }
        if page != Page::Download {
            self.resource_version_menu_open = false;
        }
        if page != Page::Community {
            self.favorite_editor_open = false;
            self.favorite_editor_pending_site = None;
            self.favorite_editor_value = 0.0;
            self.favorite_editor_velocity = 0.0;
            self.favorite_editor_source = Rect::NOTHING;
            self.favorite_editor_target = Rect::NOTHING;
            self.favorite_editor_error = None;
        }
        self.java_picker_target = None;
        self.nav_selected = index;
        self.page = page;
    }

    /// 从导航历史中返回上一页。
    ///
    /// # 返回值
    /// - `true`：成功弹出历史并返回；`false`：历史为空，无操作。
    ///
    /// # 说明
    /// 将 `Page` 映射回其底部导航索引（Home=0、Versions=1、Download=2、Community=3、Create=4、Settings=5），
    /// 并以 `record_history=false` 调用 `set_nav_page` 避免把「返回」本身再记入历史。
    pub(super) fn go_back_page(&mut self) -> bool {
        let Some(page) = self.page_history.pop() else {
            return false;
        };
        let index = match page {
            Page::Home => 0,
            Page::Versions => 1,
            Page::Download => 2,
            Page::Community => 3,
            Page::Create => 4,
            Page::Settings => 5,
        };
        self.set_nav_page(index, page, false);
        true
    }

    /// 记录某版本被游玩的时间，用于「最近游玩」列表（去重 + 按时间倒序，最多保留 32 条）。
    ///
    /// # 参数
    /// - `version`：被游玩的版本名。
    ///
    /// # 行为说明
    /// 用 `SystemTime::now()` 取 Unix 时间戳（失败则为 0）；先移除该版本的旧记录，
    /// 再压入新记录，按 `played_at` 倒序排序后截断到 32 条，保证最新游玩排在最前。
    pub(super) fn record_version_play(&mut self, version: &str) {
        // 取当前 Unix 时间戳（自 1970-01-01 起的秒数）；若系统时钟早于纪元则退化为 0
        let played_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        self.settings
            .recent_versions
            .retain(|recent| recent.version != version);
        self.settings.recent_versions.push(RecentPlay {
            version: version.to_owned(),
            played_at,
        });
        self.settings
            .recent_versions
            .sort_by_key(|recent| std::cmp::Reverse(recent.played_at));
        self.settings.recent_versions.truncate(32);
    }

    /// 弹出一个短暂提示（toast），消息内容与出现时间被记录到 `self.toast`。
    ///
    /// # 参数
    /// - `message`：任意可转为 `String` 的提示文本（如 `&str`、格式化后的 `String`）。
    pub(super) fn show_toast(&mut self, message: impl Into<String>) {
        self.toast = Some(Toast {
            message: message.into(),
            shown_at: Instant::now(),
        });
    }

    /// 将设置自动持久化到磁盘（若与已保存副本不同）。
    ///
    /// # 行为说明
    /// 当 `settings` 与 `saved_settings` 相等时直接跳过写入。否则调用 `settings.save()`：
    /// 成功则把 `saved_settings` 更新为当前副本；失败则弹出 toast 提示「设置自动保存失败」。
    pub(super) fn persist_settings(&mut self) {
        if self.settings == self.saved_settings {
            return;
        }
        match self.settings.save() {
            Ok(()) => {
                self.saved_settings = self.settings.clone();
            }
            Err(error) => {
                self.show_toast(format!("设置自动保存失败：{error}"));
            }
        }
    }

    /// 在官方版本清单尚未加载时，发起一次清单获取任务。
    ///
    /// 仅当 `official_manifest` 为 `None` 时才创建获取任务（避免重复请求），
    /// 同时清空旧的错误状态，把后台接收器存入 `official_manifest` 供
    /// `poll_background_tasks` 轮询。
    pub(super) fn refresh_official_manifest(&mut self) {
        if self.official_manifest.is_none() {
            self.official_error = None;
            self.official_manifest = Some(crate::download::start_manifest_fetch());
        }
    }

    /// 发起一次启动器自身（Genesis Launcher）的更新检查。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文，用于请求重绘。
    /// - `manual`：是否为用户手动触发；手动时若已在检查中会给 toast 提示。
    ///
    /// # 行为说明
    /// 若已有检查任务在进行，仅当用户手动触发时提示「正在检查」，然后返回。
    /// 否则重置手动标记/错误/结果，调用 `update::start_latest_release_check` 在后台
    /// 查询 GitHub 最新 Release，并请求一次重绘。
    pub(super) fn start_launcher_update_check(&mut self, ctx: &egui::Context, manual: bool) {
        if self.launcher_update_receiver.is_some() {
            if manual {
                self.show_toast("正在检查 Genesis Launcher 更新");
            }
            return;
        }
        self.launcher_update_manual = manual;
        self.launcher_update_error = None;
        self.launcher_update_result = None;
        self.launcher_update_receiver = Some(update::start_latest_release_check(ctx));
        ctx.request_repaint();
    }

    /// 针对检测到的新版本，发起「强制更新」：下载新版本安装包并准备安装。
    ///
    /// # 参数
    /// - `release`：检测到的 `LauncherRelease`（含可下载的资源资产）。
    /// - `ctx`：egui 上下文，用于请求重绘。
    ///
    /// # 行为说明
    /// 若已在进行强制更新则忽略。记录 release、重置进度/错误；
    /// 若 release 含 Windows EXE 资产，则调用 `update::start_update_download` 下载，
    /// 否则记录错误「新版本 Release 中没有可用的 Windows EXE 文件」。
    pub(super) fn begin_forced_update(&mut self, release: LauncherRelease, ctx: &egui::Context) {
        if self.forced_update_release.is_some() {
            return;
        }
        self.forced_update_release = Some(release.clone());
        self.forced_update_progress = 0.0;
        self.forced_update_error = None;
        if let Some(asset) = release.asset {
            self.forced_update_receiver = Some(update::start_update_download(asset, ctx));
        } else {
            self.forced_update_error =
                Some("新版本 Release 中没有可用的 Windows EXE 文件".to_owned());
        }
        ctx.request_repaint();
    }

    /// 轮询启动器更新检查结果，并根据结果决定是否触发强制更新或提示。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文，用于请求重绘。
    ///
    /// # 行为说明
    /// - 收到成功结果：用 `update::is_newer_version` 比较当前版本与最新版本；
    ///   若更新则调用 `begin_forced_update` 并提示「正在强制更新」，否则（且为手动检查）提示「已是最新」；
    /// - 收到错误：记录错误，手动检查时提示失败；
    /// - 通道空：保留接收器下次再查；断开：记录「更新检查连接已断开」。
    ///
    /// 无论成功失败，最后都会把 `launcher_update_manual` 复位。
    pub(super) fn poll_launcher_update(&mut self, ctx: &egui::Context) {
        let Some(receiver) = self.launcher_update_receiver.take() else {
            return;
        };
        match receiver.try_recv() {
            Ok(Ok(release)) => {
                let newer = update::is_newer_version(env!("CARGO_PKG_VERSION"), &release.version);
                self.launcher_update_result = Some(release.clone());
                self.launcher_update_error = None;
                if newer {
                    self.begin_forced_update(release, ctx);
                    self.show_toast("检测到新版本，正在强制更新 Genesis Launcher");
                } else if self.launcher_update_manual {
                    self.show_toast("Genesis Launcher 已是最新版本");
                }
                self.launcher_update_manual = false;
                ctx.request_repaint();
            }
            Ok(Err(error)) => {
                self.launcher_update_error = Some(error.clone());
                self.launcher_update_result = None;
                if self.launcher_update_manual {
                    self.show_toast(format!("检查更新失败：{error}"));
                }
                self.launcher_update_manual = false;
                ctx.request_repaint();
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.launcher_update_receiver = Some(receiver);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.launcher_update_error = Some("更新检查连接已断开".to_owned());
                self.launcher_update_manual = false;
            }
        }
    }

    /// 轮询强制更新的下载进度与完成事件，下载完成后启动安装程序并关闭窗口。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文，用于重绘、并在完成时发送关闭视口命令。
    ///
    /// # 行为说明（用 `loop` 一次性排空通道）
    /// - `Progress` 事件：按 `downloaded/total` 更新进度（total 为 0 时不更新），请求重绘；
    /// - `Finished(Ok(path))`：用 `update::launch_update_installer` 校验并启动安装包；
    ///   成功则进度置 1、标记正在安装并 `Close` 视口；失败记录错误；随后 `break`；
    /// - `Finished(Err)`：记录错误并 `break`；
    /// - 通道空：保留接收器并 `break`；断开：记录「更新下载连接意外断开」并 `break`。
    pub(super) fn poll_forced_update(&mut self, ctx: &egui::Context) {
        let Some(receiver) = self.forced_update_receiver.take() else {
            return;
        };
        loop {
            match receiver.try_recv() {
                Ok(update::ForcedUpdateEvent::Progress { downloaded, total }) => {
                    if total > 0 {
                        self.forced_update_progress =
                            (downloaded as f32 / total as f32).clamp(0.0, 1.0);
                    }
                    ctx.request_repaint();
                }
                Ok(update::ForcedUpdateEvent::Finished(Ok(path))) => {
                    // 优先取 release 资产中声明的 sha256 作为校验期望值（可能为 None）
                    let expected = self
                        .forced_update_release
                        .as_ref()
                        .and_then(|release| release.asset.as_ref())
                        .and_then(|asset| asset.sha256.as_deref());
                    // 校验下载文件的哈希并启动安装程序；成功则关闭当前启动器视口由安装包接管
                    match update::launch_update_installer(&path, expected) {
                        Ok(()) => {
                            self.forced_update_progress = 1.0;
                            self.forced_update_installing = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        Err(error) => self.forced_update_error = Some(error),
                    }
                    ctx.request_repaint();
                    break;
                }
                Ok(update::ForcedUpdateEvent::Finished(Err(error))) => {
                    self.forced_update_error = Some(error);
                    ctx.request_repaint();
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.forced_update_receiver = Some(receiver);
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.forced_update_error = Some("更新下载连接意外断开".to_owned());
                    break;
                }
            }
        }
    }

    /// 发起「通过版本 id 安装/补全游戏版本」的任务（可指定加载器家族）。
    ///
    /// # 参数
    /// - `version_id`：要安装或补全的版本 id。
    /// - `loader`：加载器家族名（原版为空串），用于定位官方基础版本。
    /// - `launch_after`：安装完成后是否立即启动游戏。
    ///
    /// # 行为说明
    /// 若已有安装任务则提示并返回；游戏目录缺失也提示并返回。
    /// 初始化 `install_progress` 进度结构（初始阶段文案「连接 Mojang 官方源」），
    /// 用 `official_base_version` 解析出官方基础版本号（失败回退为原 id），
    /// 然后调用 `start_install_by_id` 在后台下载缺失文件。
    pub(super) fn start_version_install(
        &mut self,
        version_id: String,
        loader: &str,
        launch_after: bool,
    ) {
        if self.install_receiver.is_some() {
            self.show_toast("已有安装任务正在运行");
            return;
        }
        let Some(game_directory) = self.settings.game_directory.clone() else {
            self.show_toast("未找到游戏目录，无法下载版本");
            return;
        };
        self.install_progress = Some(crate::download::InstallUpdate {
            stage: "连接 Mojang 官方源".to_owned(),
            completed_bytes: 0,
            total_bytes: 0,
            files_done: 0,
            files_total: 0,
            bytes_per_second: 0,
            finished: false,
            success: false,
            message: String::new(),
        });
        let official_id = crate::game::official_base_version(&game_directory, &version_id, loader)
            .unwrap_or(version_id);
        self.install_version_id = Some(official_id.clone());
        self.install_launch_after = launch_after;
        self.install_receiver = Some(crate::download::start_install_by_id(
            game_directory,
            official_id,
        ));
    }

    /// 发起「安装一个官方版本」的任务（直接基于 `OfficialVersion` 结构）。
    ///
    /// # 参数
    /// - `version`：要安装的官方版本对象（含 id、类型等）。
    /// - `launch_after`：安装完成后是否立即启动游戏。
    ///
    /// # 行为说明
    /// 若有安装任务/游戏目录缺失则提示返回；初始化进度结构（文案含版本 id），
    /// 记录 `install_version_id` 与 `install_launch_after`，调用 `start_install` 后台安装。
    pub(super) fn start_official_install(
        &mut self,
        version: crate::download::OfficialVersion,
        launch_after: bool,
    ) {
        if self.install_receiver.is_some() {
            self.show_toast("已有安装任务正在运行");
            return;
        }
        let Some(game_directory) = self.settings.game_directory.clone() else {
            self.show_toast("未找到游戏目录，无法下载版本");
            return;
        };
        self.install_progress = Some(crate::download::InstallUpdate {
            stage: format!("准备安装 {}", version.id),
            completed_bytes: 0,
            total_bytes: 0,
            files_done: 0,
            files_total: 0,
            bytes_per_second: 0,
            finished: false,
            success: false,
            message: String::new(),
        });
        self.install_version_id = Some(version.id.clone());
        self.install_launch_after = launch_after;
        self.install_receiver = Some(crate::download::start_install(game_directory, version));
    }

    /// 打开「加载器安装」对话框，并重置所有加载器相关状态。
    ///
    /// # 参数
    /// - `base_version`：要安装加载器所基于的官方基础版本号。
    ///
    /// 该函数会把预安装版本、待安装加载器、版本列表接收器、安装接收器等全部清空，
    /// 确保每次打开对话框都是干净状态。
    pub(super) fn open_loader_dialog(&mut self, base_version: String) {
        self.loader_dialog_open = true;
        self.loader_base_version = base_version;
        self.loader_preinstall = None;
        self.pending_loader_after_base = None;
        self.loader_kind = None;
        self.loader_versions_receiver = None;
        self.loader_versions.clear();
        self.loader_versions_error = None;
        self.loader_version_selected = 0;
        self.loader_version_scroll = 0.0;
        self.loader_install_receiver = None;
        self.loader_install_update = None;
    }

    /// 打开「先装官方版本、再装加载器」流程的加载器对话框，并记住预安装版本。
    ///
    /// # 参数
    /// - `version`：要预安装的官方版本；打开对话框后会存入 `loader_preinstall`，
    ///   待官方版本安装完成（`begin_preinstall`）时再据此继续安装加载器。
    pub(super) fn open_preinstall_loader_dialog(
        &mut self,
        version: crate::download::OfficialVersion,
    ) {
        self.open_loader_dialog(version.id.clone());
        self.loader_preinstall = Some(version);
    }

    /// 选定要安装的加载器类型（Forge/Fabric/…），并拉取该加载器可用的版本列表。
    ///
    /// # 参数
    /// - `loader`：用户选定的加载器种类。
    ///
    /// # 副作用
    /// 写入 `loader_kind`，清空已列版本/错误/选中项，并调用
    /// `loader::start_fetch_versions` 在后台获取该加载器针对 `loader_base_version` 的版本列表。
    pub(super) fn choose_loader(&mut self, loader: LoaderKind) {
        self.loader_kind = Some(loader);
        self.loader_versions.clear();
        self.loader_versions_error = None;
        self.loader_version_selected = 0;
        self.loader_version_scroll = 0.0;
        self.loader_versions_receiver = Some(crate::loader::start_fetch_versions(
            self.loader_base_version.clone(),
            loader,
        ));
    }

    /// 开始安装加载器：需要游戏目录、已选加载器种类、已选加载器版本三者齐备。
    ///
    /// # 行为说明
    /// 用单个 `let ... else` 同时校验三项是否齐全，缺失任一项则直接返回；
    /// 若已有加载器安装任务也返回。初始化 `loader_install_update` 进度结构，
    /// 调用 `loader::start_install` 后台安装（传入基础版本、加载器种类与版本）。
    pub(super) fn start_loader_install(&mut self) {
        let (Some(game_directory), Some(loader), Some(loader_version)) = (
            self.settings.game_directory.clone(),
            self.loader_kind,
            self.loader_versions
                .get(self.loader_version_selected)
                .cloned(),
        ) else {
            return;
        };
        if self.loader_install_receiver.is_some() {
            return;
        }
        self.loader_install_update = Some(LoaderInstallUpdate {
            stage: format!("准备安装 {}", loader.label()),
            finished: false,
            success: false,
            profile_hint: None,
            message: String::new(),
        });
        self.loader_install_receiver = Some(crate::loader::start_install(
            game_directory,
            self.loader_base_version.clone(),
            loader,
            loader_version,
        ));
    }

    /// 发起整合包（modpack）导入：让用户选择归档文件并在后台导入为可玩实例。
    ///
    /// # 行为说明
    /// 若已有导入任务则忽略；游戏目录缺失提示返回；通过 `modpack::pick_modpack_file`
    /// 弹出文件选择（用户取消则取消）。若未开启版本隔离则自动开启。
    /// 初始化 `modpack_import_update` 进度结构并调用 `modpack::start_import` 后台导入。
    pub(super) fn start_modpack_import(&mut self) {
        if self.modpack_import_receiver.is_some() {
            return;
        }
        let Some(game_directory) = self.settings.game_directory.clone() else {
            self.show_toast("请先选择 Minecraft 游戏目录");
            return;
        };
        let Some(archive_path) = crate::modpack::pick_modpack_file() else {
            return;
        };
        if !self.settings.version_isolation {
            self.set_version_isolation(true);
        }
        self.modpack_import_update = Some(crate::modpack::ModpackImportUpdate {
            stage: "准备导入".to_owned(),
            message: archive_path.display().to_string(),
            finished: false,
            success: false,
            profile: None,
        });
        self.modpack_import_receiver =
            Some(crate::modpack::start_import(game_directory, archive_path));
    }

    /// 结束「预安装官方版本」流程：决定是否附带安装加载器，然后开始安装官方版本。
    ///
    /// # 参数
    /// - `include_loader`：是否在官方版本安装完成后继续安装加载器。
    ///
    /// # 行为说明
    /// 取出 `loader_preinstall` 中的官方版本；若 `include_loader` 为真，则把当前
    /// 已选加载器种类与版本打包为 `pending_loader_after_base`，待官方版本安装完成后接着装；
    /// 然后清空预安装/对话框临时状态，调用 `start_official_install` 开始官方版本安装。
    pub(super) fn begin_preinstall(&mut self, include_loader: bool) {
        let Some(version) = self.loader_preinstall.clone() else {
            return;
        };
        // 若用户选择「附带加载器」，则把当前已选加载器种类与版本凑成一对，留待官方版本装完后安装
        let choice = if include_loader {
            self.loader_kind.and_then(|loader| {
                self.loader_versions
                    .get(self.loader_version_selected)
                    .cloned()
                    .map(|loader_version| (loader, loader_version))
            })
        } else {
            None
        };
        self.pending_loader_after_base = choice;
        self.loader_preinstall = None;
        self.loader_dialog_open = false;
        self.loader_versions_receiver = None;
        self.start_official_install(version, false);
    }

    /// 轮询加载器两类任务：加载器版本列表获取、加载器安装进度。安装成功后会刷新版本列表并选中新实例。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文，用于重绘。
    ///
    /// # 行为说明
    /// - 版本列表：收到后填充并重置选中；失败记录错误；空则保留接收器；断开则结束。
    /// - 安装进度：持续取出更新（`loop` 排空），记录最后一个「已完成」的更新；
    ///   成功时关闭对话框、刷新 `available_versions`、按 profile 提示或版本名匹配选中新实例并提示；
    ///   失败则保留错误更新。断开且无完成记录时用占位更新标记「加载器安装中断」。
    pub(super) fn poll_loader_tasks(&mut self, ctx: &egui::Context) {
        if let Some(receiver) = self.loader_versions_receiver.take() {
            loop {
                match receiver.try_recv() {
                    Ok(Ok(versions)) => {
                        self.loader_versions = versions;
                        self.loader_versions_error = None;
                        self.loader_version_selected = 0;
                        self.loader_version_scroll = 0.0;
                        ctx.request_repaint();
                    }
                    Ok(Err(error)) => {
                        self.loader_versions_error = Some(error.clone());
                        self.report_download_error("加载器版本获取", error);
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        self.loader_versions_receiver = Some(receiver);
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
                }
            }
        }

        let mut finished = None;
        if let Some(receiver) = self.loader_install_receiver.take() {
            loop {
                match receiver.try_recv() {
                    Ok(update) => {
                        finished = update.finished.then_some(update.clone());
                        self.loader_install_update = Some(update);
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        self.loader_install_receiver = Some(receiver);
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        if finished.is_none() {
                            finished = Some(LoaderInstallUpdate {
                                stage: "加载器安装中断".to_owned(),
                                finished: true,
                                success: false,
                                profile_hint: None,
                                message: "安装器连接已断开".to_owned(),
                            });
                        }
                        break;
                    }
                }
            }
        }
        let Some(update) = finished else {
            return;
        };
        self.loader_install_receiver = None;
        if update.success {
            self.loader_dialog_open = false;
            self.loader_install_update = None;
            self.versions = available_versions(self.settings.game_directory.as_deref());
            let loader_label = self.loader_kind.map(LoaderKind::label).unwrap_or("");
            // 决定安装完成后选中哪个版本：优先按安装器给出的 profile 名精确匹配
            let selected = update
                .profile_hint
                .as_deref()
                .and_then(|profile| {
                    self.versions
                        .iter()
                        .position(|version| version.version == profile)
                })
                // 否则按「基础版本号 + 加载器标签」都出现在版本名中来模糊匹配
                .or_else(|| {
                    self.versions.iter().position(|version| {
                        version.version.contains(&self.loader_base_version)
                            && version.version.contains(loader_label)
                    })
                })
                // 再退而求其次：选中任意一个可启动版本
                .or_else(|| self.versions.iter().position(|version| version.launchable))
                // 实在没有则选中第 0 项
                .unwrap_or(0);
            self.selected_version = selected;
            self.version_page_selected = selected;
            self.show_toast(update.message);
        } else {
            let details = format!("阶段：{}\n{}", update.stage, update.message);
            self.loader_install_update = Some(update);
            self.report_download_error("加载器安装", details);
        }
        ctx.request_repaint();
    }

    /// 轮询整合包导入进度，完成后刷新版本列表、选中导入出的实例并提示。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文，用于重绘。
    ///
    /// # 行为说明
    /// 用 `loop` 排空通道，记录最后一个完成的更新。成功后刷新 `available_versions`，
    /// 若更新带 profile 提示则选中对应实例；刷新最近会话并提示成功消息。
    /// 失败则提示「整合包导入失败」。断开且无完成记录时用占位更新标记「导入中断」。
    pub(super) fn poll_modpack_import(&mut self, ctx: &egui::Context) {
        let Some(receiver) = self.modpack_import_receiver.take() else {
            return;
        };
        let mut finished = None;
        loop {
            match receiver.try_recv() {
                Ok(update) => {
                    finished = update.finished.then_some(update.clone());
                    self.modpack_import_update = Some(update);
                    ctx.request_repaint();
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.modpack_import_receiver = Some(receiver);
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    if finished.is_none() {
                        finished = Some(crate::modpack::ModpackImportUpdate {
                            stage: "导入中断".to_owned(),
                            message: "整合包导入连接已断开".to_owned(),
                            finished: true,
                            success: false,
                            profile: None,
                        });
                    }
                    break;
                }
            }
        }
        let Some(update) = finished else {
            return;
        };
        self.modpack_import_receiver = None;
        self.modpack_import_update = Some(update.clone());
        if update.success {
            self.versions = available_versions(self.settings.game_directory.as_deref());
            if let Some(profile) = update.profile.as_ref() {
                self.selected_version = self
                    .versions
                    .iter()
                    .position(|version| version.version == *profile)
                    .unwrap_or(self.selected_version);
                self.version_page_selected = self.selected_version;
            }
            self.refresh_recent_sessions();
            self.show_toast(update.message);
        } else {
            self.show_toast(format!("整合包导入失败：{}", update.message));
            self.report_download_error("整合包导入", update.message);
        }
        ctx.request_repaint();
    }

    /// 统一的后台任务轮询入口：每帧调用，依次轮询加载器、整合包导入、启动器更新，以及
    /// 新闻动态、官方版本清单、游戏版本安装等任务，集中收口所有非阻塞通道结果。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文，用于重绘。
    ///
    /// # 行为说明（分接收器处理）
    /// - `news_receiver`：新闻动态列表，成功则填充并清空纹理缓存；失败/断开记录错误；
    /// - `official_manifest`：官方版本清单，成功则填充并把默认选中设为首个 release，
    ///   若资源版本筛选为空则顺带触发资源搜索；失败则回退到当前游戏版本并搜索；
    /// - `install_receiver`：游戏版本安装进度（`loop` 排空），完成后刷新版本列表、
    ///   按需选中新版本、提示成功，并按 `launch_after`/`pending_loader_after_base` 决定下一步
    ///   （立即启动游戏或继续安装加载器）；失败则提示「文件补全失败」。
    pub(super) fn poll_background_tasks(&mut self, ctx: &egui::Context) {
        self.poll_loader_tasks(ctx);
        self.poll_modpack_import(ctx);
        self.poll_launcher_update(ctx);
        if let Some(receiver) = self.news_receiver.take() {
            match receiver.try_recv() {
                Ok(Ok(news)) => {
                    self.news_items = news;
                    self.news_error = None;
                    self.news_textures.clear();
                    self.news_texture_keys.clear();
                    ctx.request_repaint();
                }
                Ok(Err(error)) => {
                    self.news_error = Some(error);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.news_receiver = Some(receiver);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.news_error = Some("Minecraft 动态连接已断开".to_owned());
                }
            }
        }
        if let Some(receiver) = self.official_manifest.take() {
            match receiver.try_recv() {
                Ok(Ok(versions)) => {
                    self.official_versions = versions;
                    self.official_selected = self
                        .official_versions
                        .iter()
                        .position(|version| version.kind == "release")
                        .unwrap_or(0);
                    self.official_error = None;
                    // 若资源版本筛选仍为空，且官方清单已就绪，则用最新 release 版本作为默认筛选值
                    if self.resource_version_filter.is_empty()
                        && let Some(latest) = self
                            .official_versions
                            .iter()
                            .find(|version| version.kind == "release")
                            .map(|version| version.id.clone())
                    {
                        self.resource_version_filter = latest;
                        if self.download_resource_category.is_some()
                            && self.resource_search_receiver.is_none()
                        {
                            self.start_resource_search();
                        }
                    }
                    ctx.request_repaint();
                }
                Ok(Err(error)) => {
                    self.official_error = Some(error.clone());
                    if self.page == Page::Download || self.official_browser_open {
                        self.report_download_error("官方版本清单", error);
                    }
                    if self.resource_version_filter.is_empty()
                        && self.download_resource_category.is_some()
                    {
                        self.resource_version_filter = self.version().game_version.clone();
                        self.start_resource_search();
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.official_manifest = Some(receiver);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    let error = "官方版本清单连接已断开";
                    self.official_error = Some(error.to_owned());
                    if self.page == Page::Download || self.official_browser_open {
                        self.report_download_error("官方版本清单", error);
                    }
                }
            }
        }

        let mut finished = None;
        if let Some(receiver) = self.install_receiver.take() {
            loop {
                match receiver.try_recv() {
                    Ok(update) => {
                        finished = update.finished.then_some(update.clone());
                        self.install_progress = Some(update);
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        self.install_receiver = Some(receiver);
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        if finished.is_none() {
                            finished = Some(crate::download::InstallUpdate {
                                stage: "安装中断".to_owned(),
                                completed_bytes: 0,
                                total_bytes: 0,
                                files_done: 0,
                                files_total: 0,
                                bytes_per_second: 0,
                                finished: true,
                                success: false,
                                message: "安装任务意外结束".to_owned(),
                            });
                        }
                        break;
                    }
                }
            }
        }
        let Some(update) = finished else {
            return;
        };
        self.install_progress = None;
        self.install_receiver = None;
        let launch_after = self.install_launch_after;
        self.install_launch_after = false;
        let version_id = self.install_version_id.take();
        if update.success {
            // 安装成功：重新扫描游戏目录，得到最新的可玩版本列表
            self.versions = available_versions(self.settings.game_directory.as_deref());
            if !launch_after
                && let Some(version_id) = version_id.as_ref()
                && let Some(index) = self
                    .versions
                    .iter()
                    .position(|version| version.version == *version_id)
            {
                // 若不需要安装后启动，则把新安装的版本选中并展示在版本页
                self.selected_version = index;
                self.version_page_selected = index;
            }
            self.show_toast(update.message);
            if launch_after {
                // 用户要求安装后立即启动：恢复「修复后启动」流程
                self.resume_launch_after_repair(ctx);
            } else if let Some((loader, loader_version)) = self.pending_loader_after_base.take() {
                // 之前是「装完官方版本接着装加载器」流程：把待装加载器填入并启动加载器安装
                self.loader_kind = Some(loader);
                self.loader_versions = vec![loader_version];
                self.loader_version_selected = 0;
                self.loader_dialog_open = false;
                self.loader_install_update = None;
                self.start_loader_install();
            }
        } else {
            self.pending_loader_after_base = None;
            self.show_toast(format!("文件补全失败：{}", update.message));
            let version = version_id
                .as_deref()
                .map(|version| format!("版本：{version}\n"))
                .unwrap_or_default();
            self.report_download_error(
                "Minecraft 文件补全",
                format!("{version}阶段：{}\n{}", update.stage, update.message),
            );
        }
    }

    /// 轮询整合包导出任务结果，完成后关闭内容对话框、打开导出目录并提示。
    ///
    /// # 行为说明
    /// 非阻塞地尝试接收导出结果：空则保留接收器下次再查；断开则视为「任务意外中断」错误。
    /// 成功后关闭版本内容对话框、提示「整合包导出完成」，并用 `open_path` 打开导出所在父目录；
    /// 失败则提示「导出失败」。本函数不接收 `egui::Context`，不主动请求重绘。
    pub(super) fn poll_modpack_export(&mut self) {
        let result = match self.modpack_export_receiver.as_ref() {
            Some(receiver) => match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Some(Err("整合包导出任务意外中断".to_owned()))
                }
            },
            None => return,
        };
        let Some(result) = result else {
            return;
        };
        self.modpack_export_receiver = None;
        match result {
            Ok(destination) => {
                self.close_version_content_dialog();
                self.show_toast("整合包导出完成");
                if let Some(parent) = destination.parent() {
                    open_path(parent);
                }
            }
            Err(error) => self.show_toast(format!("导出失败：{error}")),
        }
    }
}
