//! 首页（Home）页面绘制模块
//!
//! 本文件为 `LauncherApp` 提供首页相关 UI 的绘制与交互实现，是启动器“首页”场景的核心。
//! 主要职责：
//! - 绘制首屏大图（hero）区：背景图 / 动态世界、标题、最近动态徽标、查看更新入口。
//! - 绘制“最近游玩”卡片（服务器或单人存档），提供“继续游戏 / 开始游戏”入口。
//! - 绘制底部“启动坞”（launch dock）：当前版本、加载器、内存与启动按钮，并处理启动/强制关闭逻辑。
//! - 绘制“启动设置”弹卡（内存 / 分辨率 / 全屏 / 启动后隐藏）。
//! - 绘制“安装加载器”对话框（选择加载器类型与版本，触发下载安装）。
//! - 绘制“最新动态”（news）卡片并支持点击进入详情。
//!
//! 本文件所有函数均为 `pub(in crate::app)`，仅对 `crate::app` 模块可见，供上层调度代码
//! （`shell.rs` / `input.rs`）按当前页面状态调用绘制。
//! 大量使用 `theme::` 主题绘制原语（玻璃面板、圆角填充 / 描边、文字、图标、渐变、软光球等）
//! 以及 `Painter`/`Ui`/`Layout`/`CardMotion` 等启动器自绘 UI 基础设施。
//!
//! # 本文件定义的条目一览
//! 全部条目都是同一个 `impl LauncherApp` 块内的方法，没有额外定义结构体、枚举或常量：
//! - `LauncherApp::paint_hero`：首页 hero 首屏总入口，按顺序绘制背景 / 徽标 / 标题 / 元信息 chip，
//!   并在最后派发 `paint_server_card` 与 `paint_launch_dock`。
//! - `LauncherApp::paint_server_card`：hero 右上角「最近游玩」卡片（服务器或单人存档）。
//! - `LauncherApp::paint_launch_dock`：hero 底部「启动坞」，承载启动 / 强制关闭主按钮。
//! - `LauncherApp::paint_launch_settings_card`：「启动设置」浮层卡片（内存 / 分辨率 / 全屏 / 启动后隐藏）。
//! - `LauncherApp::paint_loader_dialog`：「安装加载器」模态对话框（选择加载器类型与版本）。
//! - `LauncherApp::paint_latest`：底部「最新动态」区域，绘制 3 张新闻卡片并支持打开详情。
//!
//! # 绘制约定（阅读本文件前建议先了解）
//! - 主要区块（hero、最近卡片、启动坞、动态卡片）的矩形来自 `Layout` 预先算好的坐标，
//!   再经 `CardMotion::rect` 做入场 / 切换动画变换；区块内部的按钮、文字则在这些矩形上做相对偏移。
//!   因此同一份布局在动画不同阶段会呈现不同位置，但布局计算本身与动画无关。
//! - 每个视觉区块的标准流程为：`theme::glass_panel` 画底板 → `card_clip` 建立裁剪层 →
//!   在裁剪层内绘制内容。裁剪层的作用是保证内容不会越出圆角边界。
//! - 交互统一通过 `ui.interact(rect, Id::unique(...), Sense::click())` 注册，
//!   命中区域与绘制区域分离：绘制用 `painter`，命中用 `ui`。
//! - 颜色、圆角、字体等视觉常量尽量取自 `theme::` 模块，本文件只保留少量与具体排版强相关的魔数
//!   （如 8px 内缩、42px 左内边距、边框 1.0px 等），这些均以行内注释说明其含义。

// 通配导入父模块（scene）重新导出的全部条目：包括 LauncherApp、theme 模块、scene 模块、
// Layout / CardMotion 等布局类型、Painter / Rect / Pos2 / Vec2 / Color32 / Stroke 等 egui 类型，
// 以及 draw_icon、paint_rounded_image、fit_text_to_width 等自绘辅助函数。
// 使用通配而非逐项导入，是为了与本项目其它 pages 子模块保持一致的写法。
use super::super::*;

// 本 impl 块集中实现 LauncherApp 在首页场景下的所有绘制与交互方法。
// 各方法接收 egui 的 Context/Ui、自绘 Painter、布局 Layout、动画时间 time 与卡片动效 motion，
// 负责把首页的各个视觉区块（hero / 最近卡片 / 启动坞 / 设置卡 / 加载器对话框 / 动态）画到屏幕上，
// 并通过点击响应驱动启动、切换页面、打开弹窗等状态变化。
impl LauncherApp {
    /// 绘制首屏大图（hero）区域：背景、标题、最近徽标、查看更新入口，并派生命令调用其它子区块。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文，用于触发重绘、切换页面等。
    /// - `ui`：egui 的 Ui 句柄，用于交互命中检测（interact）。
    /// - `painter`：自绘 Painter，所有图形与文字都画在它上面。
    /// - `layout`：首页布局信息，提供 hero 等区块的源矩形（未应用动画前的位置）。
    /// - `time`：全局动画时间（秒），驱动玻璃面板高光、渐变等动效。
    /// - `motion`：卡片动效，将布局里的源矩形变换为当前动画位置/尺寸。
    ///
    /// # 副作用
    /// 可能在用户点击“查看更新”时将导航切到下载页（`switch_nav_page`）并刷新官方版本清单。
    /// 当 hero 足够宽时，会调用 `paint_server_card` 绘制最近卡片；始终调用 `paint_launch_dock` 绘制启动坞。
    pub(in crate::app) fn paint_hero(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
        motion: CardMotion,
    ) {
        // 取出布局中 hero 的原始矩形，并通过动效换算成当前实际绘制矩形
        let source_hero = layout.hero;
        let hero = motion.rect(source_hero);
        // 背景图片由应用最外层统一绘制，主卡片自身不重复绘制背景。
        // 给整个 hero 描一圈强色圆角边框，强化卡片轮廓
        theme::stroke_round(
            painter,
            hero,
            theme::corner_radius(),
            1.0,
            theme::line_strong(),
        );
        // 裁剪出 hero 内容层，后续所有文字/控件都画在裁剪层里，避免溢出圆角边界
        let hero_content = card_clip(painter, hero);
        let painter = &hero_content;

        // 标题字号来自设置，pad 为内容区统一左侧/内边距
        let title_size = self.settings.hero_title_size as f32;
        let pad = 42.0;
        // 顶部“徽标”玻璃小条：左侧状态圆点 + 图标 + 文本 + 右侧“查看更新”入口
        let badge = Rect::from_min_size(
            Pos2::new(hero.left() + pad, hero.top() + 34.0),
            Vec2::new(218.0, 28.0),
        );
        // 绘制徽标玻璃面板（深色底 + 主题绿描边高光）
        theme::glass_panel(
            painter,
            badge,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(15, 30, 21), theme::GREEN, time, 3.1),
        );
        // 徽标左侧的状态圆点：半径 9.5px 的主题绿实心圆，圆心距徽标左边 15px、垂直居中
        painter.circle_filled(
            Pos2::new(badge.left() + 15.0, badge.center().y),
            9.5,
            theme::GREEN,
        );
        // 徽标左侧：先画一个绿色实心小圆点，再叠加一颗“星光”图标
        // （此处的圆与上一段参数完全相同，属同位置重复绘制，视觉结果不变）
        painter.circle_filled(
            Pos2::new(badge.left() + 15.0, badge.center().y),
            9.5,
            theme::GREEN,
        );
        draw_icon(
            painter,
            NavIcon::Sparkles,
            Rect::from_center_size(
                Pos2::new(badge.left() + 15.0, badge.center().y),
                Vec2::splat(10.0),
            ),
            Color32::from_rgb(235, 255, 226),
        );
        // 计算徽标要展示的状态文案：优先取官方正式版版本号，其次按清单/无清单给出占位提示
        let latest_status = if let Some(latest) = self
            .official_versions
            .iter()
            .find(|version| version.kind == "release")
        {
            format!("{} 现已正式可玩", latest.id)
        } else if self.official_manifest.is_some() {
            "正在获取最新版本".to_owned()
        } else {
            "最新版本暂不可用".to_owned()
        };
        // 在徽标内绘制加粗状态文本，并按可用宽度裁剪/压缩，保证不超出徽标
        theme::bold_text(
            painter,
            Pos2::new(badge.left() + 30.0, badge.top() + 7.0),
            fit_text_to_width(
                painter,
                &latest_status,
                theme::bold_font(8.4),
                (badge.width() - 93.0).max(70.0),
            ),
            8.4,
            theme::ink(),
        );
        // 徽标右侧的“查看更新”可点击热区，悬停时显示浅色背景
        let update_button = Rect::from_min_size(
            Pos2::new(badge.right() - 57.0, badge.top() + 3.0),
            Vec2::new(52.0, 22.0),
        );
        // 注册点击交互并设定悬停指针为手型
        let update_response = ui
            .interact(
                update_button,
                Id::unique("hero-view-update"),
                Sense::click(),
            )
            .on_hover_cursor(CursorIcon::PointingHand);
        // 悬停时填充浅色圆角背景作为视觉反馈
        if update_response.hovered() {
            theme::fill_round(
                painter,
                update_button,
                theme::corner_radius(),
                theme::white_alpha(12),
            );
        }
        // 画一条竖分隔线，把状态文案与“查看更新”入口分开
        painter.line_segment(
            [
                Pos2::new(badge.right() - 54.0, badge.top() + 8.0),
                Pos2::new(badge.right() - 54.0, badge.bottom() - 8.0),
            ],
            Stroke::new(1.0, theme::white_alpha(32)),
        );
        // 绘制“查看更新”文字：悬停时变亮绿，否则为浅绿
        theme::text(
            painter,
            Pos2::new(badge.right() - 45.0, badge.top() + 7.0),
            "查看更新",
            8.0,
            if update_response.hovered() {
                theme::GREEN_BRIGHT
            } else {
                Color32::from_rgb(168, 220, 162)
            },
        );
        // 点击“查看更新”：跳转到下载页并定位到正式版，刷新官方清单
        if update_response.clicked() {
            // 把官方版本筛选设为“正式版”，并复位下载详情相关状态
            self.official_filter = VersionFilter::Release;
            self.official_scroll = 0.0;
            self.resource_target_profile = None;
            // 找到正式版在官方版本列表中的位置并设为当前选中项
            if let Some(index) = self
                .official_versions
                .iter()
                .position(|version| version.kind == "release")
            {
                self.official_selected = index;
            }
            self.previous_download_detail = None;
            self.download_detail_progress = 1.0;
            // 切换到第 2 个导航页（下载页）并触发官方清单刷新
            self.switch_nav_page(2, Page::Download);
            self.refresh_official_manifest();
        }

        // 主标题绘制起点：hero 顶部下方 76px
        let title_y = hero.top() + 76.0;
        // 计算标题可用宽度：足够宽时给右侧最近卡片/启动坞预留空间（减 370），否则用左右内边距
        let available_title_width = if hero.width() > 1010.0 {
            (hero.width() - 370.0).max(240.0)
        } else {
            (hero.width() - pad * 2.0).max(240.0)
        };
        // 由设置中的标题文本、各字符字号与颜色，生成“逐字符样式化”的多行标题
        let styled_lines = styled_hero_title_lines(
            &self.settings.hero_title,
            &self.settings.hero_title_char_sizes,
            &self.settings.hero_title_char_colors,
        );
        // 逐行绘制标题，累加每行高度得到整块标题高度
        let mut current_y = title_y;
        for line in &styled_lines {
            current_y += paint_styled_hero_title_line(
                painter,
                Pos2::new(hero.left() + pad, current_y),
                available_title_width,
                1.0,
                line,
            );
        }
        let title_lines = styled_lines;
        // 标题块高度：无标题时给一个最小高度，否则用实际绘制高度加少量行距
        let title_block_height = if title_lines.is_empty() {
            12.0
        } else {
            current_y - title_y + title_size * 0.20
        };
        // 描述区与元信息（模组/光影/Java）行的纵向位置
        let description_y = title_y + title_block_height;
        let meta_y = description_y + 48.0;
        // 取当前版本信息与其数据目录，统计已安装模组和光影数量
        let version = self.version().clone();
        let data_directory = self.version_data_directory().unwrap_or_default();
        let mod_count = installed_mod_files(&data_directory).len();
        let shader_count = count_content_packs(&data_directory.join("shaderpacks"));
        // 推断 Java 标签：先根据游戏目录解析出该版本所需 Java 路径，再在已安装运行时里匹配出主版本号；
        // 匹配不到则回退为“自动 {java}”的自动管理描述
        let java_label = self
            .settings
            .game_directory
            .as_ref()
            .and_then(|directory| self.java_path_for_version(directory, &version.version))
            .and_then(|path| {
                self.java_runtimes
                    .iter()
                    .find(|runtime| same_path(&runtime.path, &path))
                    .map(|runtime| format!("Java {}", runtime.major))
            })
            .unwrap_or_else(|| format!("自动 {}", version.java));
        // 元信息 chip 列表：模组数 / 光影数 / Java 描述，每个带一个图标
        let meta = [
            (NavIcon::Box, format!("{mod_count} 个模组")),
            (
                NavIcon::Sparkles,
                if shader_count == 0 {
                    "无光影".to_owned()
                } else {
                    format!("{shader_count} 个光影")
                },
            ),
            (NavIcon::Monitor, java_label),
        ];
        // 从左到右依次绘制每个元信息 chip，绘制后累加 x 坐标并留出间距
        let mut x = hero.left() + pad;
        for (index, (icon, label)) in meta.into_iter().enumerate() {
            let text_size = 7.7;
            let text_color = Color32::from_rgb(207, 222, 202);
            // 先排版文字得到其像素宽度，据此确定 chip 的尺寸
            let text_galley =
                painter.layout_no_wrap(label.clone(), theme::font(text_size), text_color);
            let chip_height = (text_galley.size().y + 14.0).max(22.0);
            let chip = Rect::from_min_size(
                Pos2::new(x, meta_y),
                Vec2::new(text_galley.size().x + 37.0, chip_height),
            );
            // 绘制该 chip 的玻璃面板，相位随序号递增以形成错落的流光效果
            theme::glass_panel(
                painter,
                chip,
                theme::corner_radius(),
                theme::GlassSpec::new(
                    Color32::from_rgb(14, 29, 19),
                    theme::GREEN,
                    time,
                    10.2 + index as f32 * 0.3,
                ),
            );
            // chip 内左侧画图标，右侧画文案
            draw_icon(
                painter,
                icon,
                Rect::from_center_size(
                    Pos2::new(chip.left() + 12.0, chip.center().y),
                    Vec2::splat(11.0),
                ),
                Color32::from_rgb(158, 200, 153),
            );
            let text_position = Pos2::new(chip.left() + 21.0, chip.center().y);
            let adapted_text_color =
                theme::adaptive_text_color_at(painter, text_position, text_color, text_size);
            painter.text(
                text_position,
                egui::Align2::LEFT_CENTER,
                label,
                theme::font(text_size),
                adapted_text_color,
            );
            // 累加 x 到下个 chip 起点（右侧留 8px 间距）
            x = chip.right() + 8.0;
        }

        // hero 足够宽时，在右侧绘制“最近游玩”卡片
        if hero.width() > 1010.0 {
            self.paint_server_card(ctx, ui, painter, source_hero, time, motion);
        }

        // 始终在底部绘制启动坞（版本信息 + 启动按钮）
        self.paint_launch_dock(ctx, ui, painter, source_hero, time, motion);
    }

    /// 绘制 hero 右侧的“最近游玩”卡片：展示最近一次会话（服务器或单人存档）的信息，并提供“继续/开始游戏”入口。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文，用于点击后触发启动流程。
    /// - `ui`：egui Ui，用于交互命中检测。
    /// - `painter`：自绘 Painter。
    /// - `hero`：hero 的原始（未动效）矩形，用于相对定位本卡片。
    /// - `time`：动画时间，驱动玻璃面板高光。
    /// - `motion`：卡片动效，将源矩形换算为实际绘制位置。
    ///
    /// # 副作用
    /// 点击“继续游戏/开始游戏”时，若当前没有正在启动/安装/运行中的游戏，会设置 `launch_recent`
    /// 并调用 `start_launch` 启动最近会话。
    pub(in crate::app) fn paint_server_card(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        hero: Rect,
        time: f64,
        motion: CardMotion,
    ) {
        // 取最近一次会话的克隆副本（最近游玩记录的第一条）
        let recent = self.recent_sessions.first().cloned();
        // 判断最近会话是否服务器类型，并据此选择强调色（服务器青绿、单人草绿）
        let is_server = recent
            .as_ref()
            .is_some_and(|session| session.kind == SessionKind::Server);
        let accent = if is_server {
            Color32::from_rgb(89, 190, 168)
        } else {
            Color32::from_rgb(137, 211, 111)
        };
        // 卡片定位在 hero 右上角，使用固定的 RECENT_CARD_SIZE 尺寸并应用动效
        let rect = motion.rect(Rect::from_min_size(
            Pos2::new(hero.right() - 274.0, hero.top() + 28.0),
            RECENT_CARD_SIZE,
        ));
        // 绘制卡片玻璃面板
        theme::glass_panel(
            painter,
            rect,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(15, 28, 21), accent, time, 4.5),
        );
        // 裁剪卡片内容层
        let content = card_clip(painter, rect);
        let painter = &content;
        // 卡片右上角柔光球装饰
        theme::soft_orb(
            &content,
            Pos2::new(rect.right() - 35.0, rect.top() + 53.0),
            76.0,
            accent,
            11,
        );
        // 卡片左上角的“版本图标”圆角方块区域
        let recent_icon = Rect::from_center_size(
            Pos2::new(rect.left() + 27.0, rect.top() + 31.0),
            Vec2::splat(36.0),
        );
        // 取出最近会话关联的游戏版本字符串（若有）
        let recent_version = recent
            .as_ref()
            .and_then(|session| session.version.as_deref());
        // 根据会话版本字符串查找对应本地版本（先按完整版本号、再按游戏版本号且可启动），找不到则用当前版本
        let icon_version = recent_version
            .and_then(|session_version| {
                self.versions
                    .iter()
                    .find(|version| version.version == session_version)
                    .or_else(|| {
                        self.versions.iter().find(|version| {
                            version.game_version == session_version && version.launchable
                        })
                    })
            })
            .cloned()
            .unwrap_or_else(|| self.version().clone());
        // 若该版本有图标贴图则绘制圆角图片，否则用强调色方块 + 盒子图标兜底
        if let Some(texture) = self.version_icon(&icon_version) {
            paint_rounded_image(painter, recent_icon, 10.0, texture, texture.size());
        } else {
            // 无图标贴图：用强调色半透明圆角方块 + 盒子图标兜底
            theme::fill_round(
                painter,
                recent_icon,
                theme::corner_radius(),
                theme::alpha(accent, 32),
            );
            draw_icon(
                painter,
                NavIcon::Box,
                Rect::from_center_size(recent_icon.center(), Vec2::splat(21.0)),
                accent,
            );
        }
        // 标题“最近游玩” + 右上角相对上次游玩时间（无记录则显示占位）
        theme::bold_text(
            painter,
            Pos2::new(rect.left() + 51.0, rect.top() + 16.0),
            "最近游玩",
            8.0,
            Color32::from_rgb(201, 219, 204),
        );
        let played_position = Pos2::new(rect.right() - 14.0, rect.top() + 17.0);
        let played_color = theme::adaptive_text_color_at(
            painter,
            played_position,
            Color32::from_rgb(185, 224, 188),
            7.2,
        );
        painter.text(
            played_position,
            egui::Align2::RIGHT_TOP,
            recent
                .as_ref()
                .and_then(|session| session.last_played)
                .map(relative_play_time)
                .unwrap_or_else(|| "尚无记录".to_owned()),
            theme::font(7.2),
            played_color,
        );
        // 主标题：最近会话名称（截断 19 字），无记录时显示引导文案
        let title = recent
            .as_ref()
            .map(|session| ellipsize_end(&session.name, 19))
            .unwrap_or_else(|| "还没有游玩记录".to_owned());
        // 副标题：根据会话类型展示——单人存档 / 服务器地址 / 引导文案
        let subtitle = match &recent {
            Some(session) => match session.kind {
                SessionKind::World => "单人存档".to_owned(),
                SessionKind::Server => session
                    .address
                    .as_deref()
                    .map(|address| ellipsize_end(address, 28))
                    .unwrap_or_else(|| "最近连接的服务器".to_owned()),
            },
            None => "创建世界或添加服务器后会显示在这里".to_owned(),
        };
        theme::bold_text(
            painter,
            Pos2::new(rect.left() + 16.0, rect.top() + 57.0),
            title,
            17.0,
            Color32::WHITE,
        );
        theme::text(
            painter,
            Pos2::new(rect.left() + 16.0, rect.top() + 82.0),
            subtitle,
            8.0,
            Color32::from_rgb(186, 207, 190),
        );

        // 版本标签：根据是否服务器展示“服务器 · 版本”或“存档 · 版本”，无记录则用当前版本
        let recent_version = recent
            .as_ref()
            .and_then(|session| session.version.as_deref())
            .unwrap_or_else(|| self.version().version.as_str());
        let version_label = if is_server {
            format!("服务器 · {recent_version}")
        } else {
            format!("存档 · {recent_version}")
        };
        // 估算标签宽度（按字符数 * 7 + 边距），并限制最大 190px
        let version_width = version_label.chars().count() as f32 * 7.0 + 18.0;
        let version_chip = Rect::from_min_size(
            Pos2::new(rect.left() + 16.0, rect.top() + 105.0),
            Vec2::new(version_width.min(190.0), 21.0),
        );
        // 绘制版本 chip：强调色半透明填充 + 略亮描边
        theme::fill_round(
            painter,
            version_chip,
            theme::corner_radius(),
            theme::alpha(accent, 22),
        );
        theme::stroke_round(
            painter,
            version_chip,
            theme::corner_radius(),
            0.8,
            theme::alpha(accent, 42),
        );
        theme::center_text(
            painter,
            version_chip.center(),
            ellipsize_end(&version_label, 24),
            7.2,
            Color32::from_rgb(197, 224, 195),
            false,
        );
        // 卡片中部的分隔细线
        painter.line_segment(
            [
                Pos2::new(rect.left() + 14.0, rect.top() + 136.0),
                Pos2::new(rect.right() - 14.0, rect.top() + 136.0),
            ],
            Stroke::new(1.0, theme::white_alpha(19)),
        );
        // 底部状态行：最近连接 / 最近存档 / 等待第一次冒险
        theme::text(
            painter,
            Pos2::new(rect.left() + 16.0, rect.top() + 151.0),
            if recent.is_some() {
                if is_server {
                    "● 最近连接"
                } else {
                    "● 最近存档"
                }
            } else {
                "● 等待第一次冒险"
            },
            7.6,
            Color32::from_rgb(174, 194, 177),
        );
        // 右下角的“继续游戏 / 开始游戏”按钮及其交互
        let join = Rect::from_min_size(
            Pos2::new(rect.right() - 94.0, rect.top() + 145.0),
            Vec2::new(78.0, 27.0),
        );
        let response = ui
            .interact(join, Id::unique("server-join"), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        // 悬停时填充浅色背景反馈
        if response.hovered() {
            theme::fill_round(
                painter,
                join,
                theme::corner_radius(),
                theme::white_alpha(13),
            );
        }
        // 绘制按钮文字：有记录显示“继续游戏”，无记录显示“开始游戏”；悬停变亮
        theme::text(
            painter,
            Pos2::new(join.left() + 7.0, join.top() + 5.0),
            if recent.is_some() {
                "继续游戏 →"
            } else {
                "开始游戏 →"
            },
            7.6,
            if response.hovered() {
                theme::GREEN_BRIGHT
            } else {
                Color32::from_rgb(160, 215, 154)
            },
        );
        // 点击：若当前没有任何启动/安装/运行中的游戏，则标记 launch_recent 并启动
        if response.clicked() {
            // starting 表示：正在启动 / 正在安装 / 游戏进程已存在，三者任一为真则禁止再次启动
            let starting = self.launch_started.is_some()
                || self.install_receiver.is_some()
                || self.game_process_id.is_some();
            if !starting {
                // launch_recent=true 表示直接进入最近会话，否则启动当前版本
                self.launch_recent = recent.is_some();
                self.start_launch(ctx);
            }
        }
    }

    /// 绘制 hero 底部的“启动坞”（launch dock）：当前版本图标/名称/加载器、内存设置入口、启动/强制关闭按钮。
    ///
    /// # 参数
    /// - `ctx`：egui 上下文，用于点击启动后请求重绘。
    /// - `ui`：egui Ui，用于交互命中检测（更多按钮、启动按钮等）。
    /// - `painter`：自绘 Painter。
    /// - `hero`：hero 原始矩形，用于相对定位启动坞（位于 hero 底部内缩区域）。
    /// - `time`：动画时间，驱动玻璃高光与液态按钮动效。
    /// - `motion`：卡片动效。
    ///
    /// # 副作用
    /// - 点击“更多”按钮切换 `launch_settings_open` 启动设置卡。
    /// - 点击启动按钮：若游戏正在运行则强制关闭（`force_close_game`），否则调用 `start_launch` 启动并请求重绘。
    pub(in crate::app) fn paint_launch_dock(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        hero: Rect,
        time: f64,
        motion: CardMotion,
    ) {
        // 启动坞定位在 hero 底部：上下各内缩，形成一个横向长条玻璃面板
        let dock = motion.rect(Rect::from_min_max(
            Pos2::new(hero.left() + 19.0, hero.bottom() - 96.0),
            Pos2::new(hero.right() - 19.0, hero.bottom() - 18.0),
        ));
        // 绘制启动坞玻璃面板
        theme::glass_panel(
            painter,
            dock,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(19, 31, 23), theme::GREEN, time, 5.7),
        );
        // 裁剪内容层
        let dock_content = card_clip(painter, dock);
        let painter = &dock_content;

        // 取出当前版本信息（用于显示图标、名称、描述、加载器标签）
        let version = self.version().clone();
        // 左侧版本图标方块（44px），优先用版本贴图，否则用草方块图标兜底
        let version_icon = Rect::from_center_size(
            Pos2::new(dock.left() + 43.0, dock.center().y + 2.0),
            Vec2::splat(44.0),
        );
        // 有版本图标贴图时按 11px 圆角绘制图片；没有则走下面的草方块兜底分支
        if let Some(texture) = self.version_icon(&version) {
            paint_rounded_image(painter, version_icon, 11.0, texture, texture.size());
        } else {
            // 无贴图：绘制一个“草方块”（顶面草绿、侧面棕色）作为默认图标
            paint_block(
                painter,
                version_icon.center(),
                43.0,
                Color32::from_rgb(128, 205, 105),
                Color32::from_rgb(91, 67, 49),
                Color32::from_rgb(111, 81, 54),
            );
        }
        // 给图标描一圈淡淡的白色圆角边框
        theme::stroke_round(painter, version_icon, 11, 1.0, theme::white_alpha(28));
        // 文字区左右边界：右边界为 dock 右侧留出 282px 给“更多/启动”按钮区
        let text_left = dock.left() + 76.0;
        let text_right = dock.right() - 282.0;
        let text_width = (text_right - text_left).max(100.0);
        // 取加载器标签（如 Forge / Fabric / Quilt 等），并判断是否有加载器
        let loader_font = theme::font(7.5);
        let mut loader_text = self.visible_loader_label(&version);
        let has_loader = !loader_text.is_empty();
        // 测量加载器标签宽度（无则 0）
        let mut loader_text_width = if has_loader {
            painter
                .layout_no_wrap(loader_text.clone(), loader_font.clone(), theme::ink())
                .size()
                .x
        } else {
            0.0
        };
        // 加载器文字最大宽度：不超过文字区的 48%，并留出边距，至少 34px
        let max_loader_text = (text_width * 0.48 - 14.0).max(34.0);
        // 若过长则裁剪到最大宽度，并重新测量
        if has_loader && loader_text_width > max_loader_text {
            loader_text =
                fit_text_to_width(painter, &loader_text, loader_font.clone(), max_loader_text);
            loader_text_width = painter
                .layout_no_wrap(loader_text.clone(), loader_font, theme::ink())
                .size()
                .x;
        }
        // 加载器 chip 宽度 = 文字宽 + 内边距；版本名可用宽度需减去加载器宽度
        let loader_width = if has_loader {
            loader_text_width + 14.0
        } else {
            0.0
        };
        let version_max_width = (text_width - loader_width - 10.0).max(80.0);
        // 版本名裁剪到可用宽度，再测量实际宽度用于后续布局加载器 chip 的位置
        let display_version = self.version_display_name(&version);
        let version_text = fit_text_to_width(
            painter,
            &display_version,
            theme::bold_font(14.0),
            version_max_width,
        );
        let version_width = painter
            .layout_no_wrap(version_text.clone(), theme::bold_font(14.0), theme::ink())
            .size()
            .x;
        // 绘制版本名（大号加粗）
        theme::bold_text(
            painter,
            Pos2::new(text_left, dock.top() + 17.0),
            version_text,
            14.0,
            theme::ink(),
        );
        // 若有加载器，在版本名右侧绘制一个加载器 chip；空间足够时再显示“✓ 已校验”
        if has_loader {
            let loader_rect = Rect::from_min_size(
                Pos2::new(text_left + version_width + 8.0, dock.top() + 19.0),
                Vec2::new(loader_width, 17.0),
            );
            theme::fill_round(
                painter,
                loader_rect,
                theme::corner_radius(),
                theme::alpha(theme::GREEN, 20),
            );
            theme::center_text(
                painter,
                loader_rect.center(),
                loader_text,
                7.5,
                Color32::from_rgb(168, 221, 160),
                false,
            );
            // 仅在加载器右侧仍有充足空间时才显示“已校验”提示，避免与右侧按钮区重叠
            if loader_rect.right() + 62.0 < text_right {
                theme::text(
                    painter,
                    Pos2::new(loader_rect.right() + 9.0, dock.top() + 20.0),
                    "✓ 已校验",
                    7.2,
                    theme::muted(),
                );
            }
        }
        // 版本描述（截断 58 字），位于版本名下方
        theme::text(
            painter,
            Pos2::new(text_left, dock.top() + 42.0),
            ellipsize_end(&version.description, 58),
            8.3,
            Color32::from_rgb(128, 142, 133),
        );

        // 「更多」按钮：34×34 的方形热区，中心位于 dock 右边缘左移 295px 处（即启动按钮左侧）。
        // 点击它切换「启动设置」浮层；不会直接启动游戏。
        let more = Rect::from_center_size(
            Pos2::new(dock.right() - 295.0, dock.center().y),
            Vec2::new(34.0, 34.0),
        );
        // 注册点击交互并显示手型光标
        let more_response = ui
            .interact(more, Id::unique("launch-more"), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        // 悬停或设置卡已展开时，给按钮加浅色底，表示当前处于激活状态
        if more_response.hovered() || self.launch_settings_open {
            theme::fill_round(
                painter,
                more,
                theme::corner_radius(),
                theme::white_alpha(13),
            );
        }
        // 绘制「⋯」图标：三个半径 1.5px 的圆点在水平方向 -4 / 0 / +4 处排列
        for offset in [-4.0, 0.0, 4.0] {
            painter.circle_filled(
                Pos2::new(more.center().x + offset, more.center().y),
                1.5,
                theme::muted(),
            );
        }
        // 点击「更多」：关闭可能打开的菜单 / 新闻详情，再切换启动设置卡的显隐（开关式）
        if more_response.clicked() {
            self.menu = Menu::None;
            self.news = None;
            self.launch_settings_open = !self.launch_settings_open;
        }

        // 竖直分隔线：把左侧版本信息区与右侧「更多 + 启动」按钮区分开，
        // 距 dock 右边缘 267px，上下各留 18px 内边距。
        painter.line_segment(
            [
                Pos2::new(dock.right() - 267.0, dock.top() + 18.0),
                Pos2::new(dock.right() - 267.0, dock.bottom() - 18.0),
            ],
            Stroke::new(1.0, theme::white_alpha(22)),
        );

        // 启动按钮区域：右侧宽 235px（dock 右边缘内缩 15px 到左缩 250px），上下各留 9px 内边距，
        // 是启动坞中最主要的交互区。
        let launch = Rect::from_min_max(
            Pos2::new(dock.right() - 250.0, dock.top() + 9.0),
            Pos2::new(dock.right() - 15.0, dock.bottom() - 9.0),
        );
        // 注册启动按钮点击交互
        let response = ui
            .interact(launch, Id::unique("launch-game"), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        // 游戏是否正在运行：有进程 ID 且不处于「启动中」阶段
        // （启动成功约 1.55 秒后 launch_started 会被清空，而 game_process_id 保留，即进入运行态）。
        let game_running = self.game_process_id.is_some() && self.launch_started.is_none();
        // 是否正在补全文件（安装 / 下载进度接收端存在）
        let installing = self.install_receiver.is_some();
        // 启动中 = 启动流程尚未结束，或正在安装；两者都禁止重复触发启动
        let starting = self.launch_started.is_some() || installing;
        // 仅在非启动中且悬停时提亮按钮，避免误导用户以为可以再次点击
        let brighten = response.hovered() && !starting;
        // 按钮外观分支一：游戏运行中 → 用实色按钮（默认绿色 67,129,74；悬停转为砖红 164,77,66，
        // 暗示再次点击将强制关闭），并加一圈高对比描边。
        if game_running {
            theme::fill_round(
                painter,
                launch,
                theme::corner_radius(),
                if response.hovered() {
                    Color32::from_rgb(164, 77, 66)
                } else {
                    Color32::from_rgb(67, 129, 74)
                },
            );
            theme::stroke_round(
                painter,
                launch,
                theme::corner_radius(),
                1.0,
                theme::white_alpha(72),
            );
        // 分支二：非运行态 → 使用主题的「液态」按钮，随时间与悬停状态产生流动高光
        } else {
            theme::liquid_button(painter, launch, theme::corner_radius(), time, brighten);
        }
        // 启动中：再覆盖一层半透明黑，整体压暗，形成不可用的视觉状态
        if starting {
            theme::fill_round(
                painter,
                launch,
                theme::corner_radius(),
                theme::black_alpha(54),
            );
        }
        // 按钮左侧的 44×44 圆形图标底托，圆心距按钮左边 29px，垂直居中
        let play_rect = Rect::from_center_size(
            Pos2::new(launch.left() + 29.0, launch.center().y),
            Vec2::new(44.0, 44.0),
        );
        // 底托填充深绿色半透明，衬托上层的白色图标
        theme::fill_round(
            painter,
            play_rect,
            theme::corner_radius(),
            theme::alpha(Color32::from_rgb(13, 61, 26), 62),
        );
        // 图标三分支：
        // 1) 游戏运行中 → 白色方块“停止”符号（15×15，圆角）
        if game_running {
            painter.rect_filled(
                Rect::from_center_size(play_rect.center(), Vec2::splat(15.0)),
                theme::corner_radius(),
                Color32::WHITE,
            );
        // 2) 启动 / 安装中 → 两个同心圆环（半径 9 与 5.5）作为加载指示
        } else if starting {
            painter.circle_stroke(
                play_rect.center(),
                9.0,
                Stroke::new(2.0, theme::white_alpha(195)),
            );
            painter.circle_stroke(
                play_rect.center(),
                5.5,
                Stroke::new(1.0, theme::white_alpha(45)),
            );
        // 3) 空闲 → 白色三角形“播放”符号：三个顶点相对中心分别为 (-4,-8) / (8,0) / (-4,8)
        } else {
            painter.add(Shape::convex_polygon(
                vec![
                    play_rect.center() + Vec2::new(-4.0, -8.0),
                    play_rect.center() + Vec2::new(8.0, 0.0),
                    play_rect.center() + Vec2::new(-4.0, 8.0),
                ],
                Color32::WHITE,
                Stroke::NONE,
            ));
        }
        // 主标题文案按状态优先级选取：
        // 运行中 > 安装 / 补全文件中 > 启动中 > 空闲（开始游戏）。
        theme::bold_text(
            painter,
            Pos2::new(launch.left() + 59.0, launch.top() + 13.0),
            if game_running {
                "游戏运行中"
            } else if installing {
                "正在补全文件…"
            } else if starting {
                "正在启动…"
            } else {
                "开始游戏"
            },
            13.0,
            Color32::WHITE,
        );
        // 副标题：运行中提示可强制关闭；安装中显示当前安装阶段（无进度对象时给出默认描述）；
        // 启动中显示环境检查提示；空闲时显示该版本所需的 Java 版本字符串。
        theme::text(
            painter,
            Pos2::new(launch.left() + 59.0, launch.top() + 36.0),
            if game_running {
                "再次点击强制关闭"
            } else if installing {
                // install_progress 为 None 时（例如刚进入安装流程）使用兜底文案
                self.install_progress
                    .as_ref()
                    .map(|progress| progress.stage.as_str())
                    .unwrap_or("连接 Mojang 官方下载源")
            } else if starting {
                "检查运行环境与模组"
            } else {
                &version.java
            },
            7.3,
            theme::alpha(Color32::WHITE, 174),
        );
        // 点击处理：同一个按钮在不同状态下承担两种语义。
        if response.clicked() {
            // 情况 A：游戏正在运行 → 本按钮变为「强制关闭」。
            if game_running {
                // 再次读取进程 ID，确保本次点击确有可关闭的进程
                if let Some(process_id) = self.game_process_id {
                    // 调用 Win32 侧实现的强制结束进程；成功与失败都通过 Toast 反馈
                    match crate::game::force_close_game(process_id) {
                        Ok(()) => {
                            // 关闭成功后清空所有运行态标记，使按钮回到「开始游戏」
                            self.game_process_id = None;
                            self.launch_started = None;
                            self.launch_complete = false;
                            self.show_toast("已强制关闭游戏");
                        }
                        // 关闭失败：保留 game_process_id，让用户仍可重试
                        Err(error) => self.show_toast(format!("关闭游戏失败：{error}")),
                    }
                }
            // 情况 B：空闲且非启动 / 安装中 → 正常启动当前版本
            } else if !starting {
                // launch_recent = false 表示启动「当前选中版本」而非最近会话
                self.launch_recent = false;
                self.start_launch(ctx);
                // 立即请求重绘，让「正在启动…」状态尽快反馈到界面
                ctx.request_repaint();
            }
        }
    }

    /// 绘制「启动设置」浮层卡片：内存上限、窗口分辨率、全屏启动、启动后隐藏启动器。
    ///
    /// 该卡片由启动坞右侧的「更多」按钮（三个圆点）切换显示，显示状态由
    /// `LauncherApp::launch_settings_open` 控制；卡片本身不负责显隐判断。
    /// 调用方在 `launch_settings_open` 为真、或关闭动画尚未结束时才会调用本方法
    /// （见 `shell.rs` 中的绘制分支），并在调用后对整个图层施加缩放动效，
    /// 因此本方法内部使用的是未变换的布局矩形。
    ///
    /// # 参数
    /// - `ui`：egui Ui，用于为关闭按钮、加减按钮、切换开关等注册点击交互。
    /// - `painter`：自绘 Painter，卡片底板与全部文本图形都画在它上面。
    /// - `layout`：首页布局，提供启动设置卡的固定矩形（见 `launch_settings_card_rect`）。
    /// - `time`：全局动画时间（秒），驱动玻璃面板的流光高光。
    ///
    /// # 副作用
    /// - 直接修改 `self.settings` 中的 `memory_mb` / `window_width` / `window_height` /
    ///   `fullscreen` / `hide_after_launch` 字段；按界面文案「修改后自动保存」的约定，
    ///   这些字段的持久化由设置模块的保存逻辑负责，本方法不主动写盘。
    /// - 点击右上角关闭按钮会把 `launch_settings_open` 置为 `false`。
    pub(in crate::app) fn paint_launch_settings_card(
        &mut self,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
    ) {
        // 由布局统一给出卡片矩形，保证与启动坞「更多」按钮的位置关系稳定
        let card = launch_settings_card_rect(layout);
        // 卡片底板：深绿半透明玻璃面板；backdrop_blur(16.0) 让卡片后方的首页内容被模糊，
        // 从而突出浮层层级（数值单位为逻辑像素的模糊半径）。相位 23.0 用于错开高光位置。
        theme::glass_panel(
            painter,
            card,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(18, 29, 22), theme::GREEN, time, 23.0)
                .backdrop_blur(16.0),
        );
        // 建立卡片裁剪层，并用局部变量遮蔽外层 painter，
        // 之后的绘制代码都自动被限制在卡片圆角范围内。
        let card_content = card_clip(painter, card);
        let painter = &card_content;
        // 卡片标题「启动设置」
        theme::bold_text(
            painter,
            Pos2::new(card.left() + 20.0, card.top() + 17.0),
            "启动设置",
            16.0,
            Color32::from_rgb(174, 185, 177),
        );
        theme::text(
            painter,
            Pos2::new(card.left() + 20.0, card.top() + 42.0),
            "修改后自动保存",
            7.8,
            Color32::from_rgb(125, 139, 129),
        );
        // 右上角关闭按钮：30×30 的点击热区，距卡片右边缘 44px、上边缘 14px
        // （44 = 按钮宽 30 + 右侧 14px 留白）。
        let close = Rect::from_min_size(
            Pos2::new(card.right() - 44.0, card.top() + 14.0),
            Vec2::splat(30.0),
        );
        // 注册关闭按钮点击交互，悬停时显示手型光标
        let close_response = ui
            .interact(close, Id::unique("launch-settings-close"), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        // 按钮底色：悬停为半透明白（提亮），否则为半透明黑（凹陷感）
        theme::fill_round(
            painter,
            close,
            theme::corner_radius(),
            if close_response.hovered() {
                theme::white_alpha(22)
            } else {
                theme::black_alpha(56)
            },
        );
        // 按钮中央的「×」图标
        draw_icon(
            painter,
            NavIcon::Close,
            Rect::from_center_size(close.center(), Vec2::splat(13.0)),
            Color32::from_rgb(148, 160, 151),
        );
        // 点击关闭：仅改变显示标志，不触碰任何设置值
        if close_response.clicked() {
            self.launch_settings_open = false;
        }

        // 设置行统一布局参数：左右各留 20px 内边距，
        // 首行起始 y 为卡片顶部 +67px（标题与副标题占用的高度），每行固定高 55px。
        // 各行按固定行高依次向下排布，卡片高度由 `launch_settings_card_rect` 的布局决定。
        let row_left = card.left() + 20.0;
        let row_width = card.width() - 40.0;
        let mut row_y = card.top() + 67.0;
        let row_height = 55.0;

        // ===== 第 1 行：游戏内存 =====
        let row = Rect::from_min_size(Pos2::new(row_left, row_y), Vec2::new(row_width, row_height));
        let memory_limit = memory_limit_mb();
        let memory_description = format!("本机上限 {:.1} GB", memory_limit as f32 / 1024.0);
        // 左侧绘制标题「游戏内存」与说明文字「Java 虚拟机最大可用内存」
        paint_setting_label(painter, row, "游戏内存", &memory_description);
        // 右侧两个 28×28 的圆角加减按钮：减号在 row 右边缘左移 145px，
        // 加号在右边缘左移 27px，两者之间留出显示当前内存值的空间。
        let minus = Rect::from_center_size(
            Pos2::new(row.right() - 145.0, row.center().y),
            Vec2::splat(28.0),
        );
        let plus = Rect::from_center_size(
            Pos2::new(row.right() - 27.0, row.center().y),
            Vec2::splat(28.0),
        );
        // 点击减号：内存按 512MB 步长递减，下限 2048MB（2GB）。
        // saturating_sub 先防止 u32 下溢，再用 max(2048) 抬回下限，两者顺序不可交换。
        if compact_icon_button(ui, painter, minus, "popup-memory-minus", false) {
            self.settings.memory_mb = self.settings.memory_mb.saturating_sub(512).max(2048);
        }
        // 点击加号：内存按 512MB 步长递增，上限来自本机物理内存。
        if compact_icon_button(ui, painter, plus, "popup-memory-plus", true) {
            self.settings.memory_mb = (self.settings.memory_mb + 512).min(memory_limit);
        }
        // 当前内存值展示：把 MB 换算成 GB 并保留一位小数（1024MB = 1GB）
        theme::bold_text(
            painter,
            Pos2::new(row.right() - 111.0, row.center().y - 8.0),
            format!("{:.1} GB", self.settings.memory_mb as f32 / 1024.0),
            11.0,
            Color32::from_rgb(183, 194, 186),
        );
        // 行指针下移到下一行
        row_y += row_height;

        // ===== 第 2 行：窗口尺寸（点击循环切换预设分辨率）=====
        let row = Rect::from_min_size(Pos2::new(row_left, row_y), Vec2::new(row_width, row_height));
        paint_setting_label(painter, row, "窗口尺寸", "点击切换常用启动分辨率");
        // 右侧 143×32 的按钮，用于显示当前分辨率并承接点击
        let resolution = Rect::from_min_size(
            Pos2::new(row.right() - 143.0, row.top() + 12.0),
            Vec2::new(143.0, 32.0),
        );
        // 按钮文案：「宽 × 高」，乘号使用全角「×」以与界面其它位置保持一致
        let label = format!(
            "{} × {}",
            self.settings.window_width, self.settings.window_height
        );
        // 点击时在 4 个常用预设间循环：找到当前值所在下标后取下一个（取模回到首位），
        // 若当前值不在预设中（用户手工改过），则回落到第 0 项 (1280×720)。
        if settings_button(ui, painter, resolution, "popup-resolution", &label, false) {
            // 预设分辨率表：720p / 900p / 1080p / 1440p 四档，覆盖常见显示器
            /// 提供 PRESETS 使用的只读常量。
            ///
            /// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
            const PRESETS: [(u32, u32); 4] = [(1280, 720), (1600, 900), (1920, 1080), (2560, 1440)];
            // position 返回当前分辨率的索引；map 中 (index + 1) % len 实现循环递增
            let next = PRESETS
                .iter()
                .position(|size| *size == (self.settings.window_width, self.settings.window_height))
                .map(|index| (index + 1) % PRESETS.len())
                .unwrap_or(0);
            // 同时写回宽高，保证两者始终成对更新，不会出现宽高来自不同预设的中间态
            (self.settings.window_width, self.settings.window_height) = PRESETS[next];
        }
        // 行指针下移到下一行
        row_y += row_height;

        // ===== 第 3 行：全屏启动开关 =====
        // paint_toggle_row 内部自行绘制标题 / 说明 / 开关，并直接写入传入的 bool 字段；
        // 这里传入 &mut self.settings.fullscreen，即点击开关立即生效。
        let row = Rect::from_min_size(Pos2::new(row_left, row_y), Vec2::new(row_width, row_height));
        paint_toggle_row(
            ui,
            painter,
            row,
            "popup-fullscreen",
            "全屏启动",
            "游戏启动后直接进入全屏",
            &mut self.settings.fullscreen,
        );
        // 行指针下移到下一行
        row_y += row_height;

        // ===== 第 4 行：启动后隐藏启动器开关 =====
        // 与上一行结构相同，作用于 settings.hide_after_launch：
        // 该字段为 true 时，游戏进程启动完成后启动器会自动最小化。
        let row = Rect::from_min_size(Pos2::new(row_left, row_y), Vec2::new(row_width, row_height));
        paint_toggle_row(
            ui,
            painter,
            row,
            "popup-hide-launcher",
            "启动后隐藏启动器",
            "游戏进程启动完成后自动最小化",
            &mut self.settings.hide_after_launch,
        );
    }

    /// 绘制「安装加载器」模态对话框：选择加载器类型（Forge / Fabric 等）与具体版本，并触发安装。
    ///
    /// 该对话框有两种进入方式，由 `LauncherApp::loader_preinstall` 区分：
    /// - 普通模式（`loader_preinstall` 为 `None`）：用户主动点击「安装加载器」，
    ///   对话框中可以「仅保留原版」直接关闭；
    /// - 预安装模式（`loader_preinstall.is_some()`）：安装游戏本体之前顺带选择加载器，
    ///   此时底部左按钮变为「仅安装原版」，关闭对话框会跳过加载器环节。
    ///
    /// 调用方（`shell.rs`）在 `loader_dialog_open` 为真时才会调用本方法；
    /// 本方法内部不会自行判断对话框是否应当显示。
    ///
    /// # 参数
    /// - `ui`：egui Ui，用于加载器类型按钮、版本行、底部两个操作按钮的点击命中检测。
    /// - `painter`：自绘 Painter，对话框底板与全部内容绘制在其上。
    /// - `screen`：调用方传入的可用屏幕矩形，用于把对话框居中并按屏幕尺寸收缩。
    /// - `time`：全局动画时间（秒），用于玻璃高光与安装中进度条的循环动画。
    ///
    /// # 副作用
    /// - 点击类型按钮调用 `choose_loader`（会重置已选版本并重新拉取版本列表）。
    /// - 点击版本行更新 `loader_version_selected`。
    /// - 滚轮滚动更新 `loader_version_scroll`。
    /// - 底部按钮可能调用 `begin_preinstall` / `start_loader_install`，
    ///   或关闭对话框并清理 `loader_versions_receiver` / `loader_install_update`。
    pub(in crate::app) fn paint_loader_dialog(
        &mut self,
        ui: &mut egui::Ui,
        painter: &Painter,
        screen: Rect,
        time: f64,
    ) {
        // 对话框理想尺寸 590×480，但必须同时收进屏幕：
        // 宽高各减去 34 / 38 像素的安全边距（左右共 34、上下共 38），再用 min 取较小值，
        // 因此在小窗口下对话框会自动缩小而不会溢出屏幕。
        let size = Vec2::new(
            590.0_f32.min(screen.width() - 34.0),
            480.0_f32.min(screen.height() - 38.0),
        );
        // 以屏幕中心为锚点放置对话框，保证无论窗口大小都居中显示
        let dialog = Rect::from_center_size(screen.center(), size);
        // 对话框底板：玻璃面板 + 18px 背景模糊，强调这是覆盖在首页之上的模态层
        theme::glass_panel(
            painter,
            dialog,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(20, 31, 24), theme::GREEN, time, 31.0)
                .backdrop_blur(18.0),
        );
        // 建立对话框裁剪层，后续内容（尤其是可滚动列表）被限制在圆角边框内
        let dialog_content = card_clip(painter, dialog);
        let painter = &dialog_content;
        // 标题「安装加载器」
        theme::bold_text(
            painter,
            Pos2::new(dialog.left() + 24.0, dialog.top() + 20.0),
            "安装加载器",
            21.0,
            theme::ink(),
        );
        // 副标题：统一带上即将安装的基础游戏版本号（loader_base_version）；
        // 预安装模式下提示语为「先选择加载器，再开始下载安装」（因为此时尚未安装游戏本体），
        // 普通模式下为「选择加载器或仅保留原版」（此时游戏已存在，可以跳过加载器）。
        theme::text(
            painter,
            Pos2::new(dialog.left() + 24.0, dialog.top() + 48.0),
            if self.loader_preinstall.is_some() {
                format!(
                    "Minecraft {} · 先选择加载器，再开始下载安装",
                    self.loader_base_version
                )
            } else {
                format!(
                    "Minecraft {} · 选择加载器或仅保留原版",
                    self.loader_base_version
                )
            },
            8.2,
            theme::muted(),
        );

        // 是否正在安装：只要存在加载器安装进度接收端就视为安装中。
        // 安装中会禁用类型切换与底部按钮，避免并发修改安装参数。
        let installing = self.loader_install_receiver.is_some();
        // 遍历全部加载器类型，横向排布成一行按钮：
        // 每个按钮宽 121、间距 10（步进 131），起始左边距 24，垂直位置固定在对话框顶部 +78。
        for (index, loader) in LoaderKind::ALL.into_iter().enumerate() {
            let button = Rect::from_min_size(
                Pos2::new(
                    dialog.left() + 24.0 + index as f32 * 131.0,
                    dialog.top() + 78.0,
                ),
                Vec2::new(121.0, 34.0),
            );
            // 注册按钮交互：安装进行中时光标显示「禁止」图标，否则显示手型
            let response = ui
                .interact(button, Id::unique(("loader-type", index)), Sense::click())
                .on_hover_cursor(if installing {
                    CursorIcon::NotAllowed
                } else {
                    CursorIcon::PointingHand
                });
            // 当前按钮对应的加载器是否即为已选加载器（用于高亮样式）
            let selected = self.loader_kind == Some(loader);
            // 按钮底色优先级：选中态（绿色半透明）> 悬停且可交互（白色微亮）> 普通态（黑色半透明）
            theme::fill_round(
                painter,
                button,
                theme::corner_radius(),
                if selected {
                    theme::alpha(theme::GREEN, 34)
                } else if response.hovered() && !installing {
                    theme::white_alpha(13)
                } else {
                    theme::black_alpha(42)
                },
            );
            theme::stroke_round(
                painter,
                button,
                theme::corner_radius(),
                1.0,
                if selected {
                    theme::alpha(theme::GREEN, 75)
                } else {
                    theme::white_alpha(24)
                },
            );
            theme::center_text(
                painter,
                button.center(),
                loader.label(),
                9.0,
                if selected {
                    theme::GREEN_BRIGHT
                } else {
                    theme::ink_soft()
                },
                selected,
            );
            if response.clicked() && !installing {
                self.choose_loader(loader);
            }
        }

        // 版本列表区域：顶部距对话框上沿 128px（让出标题 + 类型按钮行），
        // 底部距下沿 78px（让出底部两个操作按钮），左右各留 20px 内边距。
        let list_top = dialog.top() + 128.0;
        let list_bottom = dialog.bottom() - 78.0;
        let list_clip = Rect::from_min_max(
            Pos2::new(dialog.left() + 20.0, list_top),
            Pos2::new(dialog.right() - 20.0, list_bottom),
        );
        // 列表内容使用克隆的 painter：克隆与父 painter 共享同一图层，
        // 但单独持有便于在后续代码中自由遮蔽局部变量名。
        let list_painter = painter.clone();
        // 状态分支 1：存在安装进度 → 用进度视图完全替代列表
        if let Some(update) = &self.loader_install_update {
            // 进度视图背景：在列表区铺一层黑色半透明遮罩，遮住下方可能残留的列表内容
            theme::fill_round(
                &list_painter,
                list_clip,
                theme::corner_radius(),
                theme::black_alpha(45),
            );
            // 居中显示当前阶段文案（update.stage，例如「正在下载安装包」），最后一个参数为加粗标志
            theme::center_text(
                &list_painter,
                Pos2::new(list_clip.center().x, list_clip.top() + 70.0),
                &update.stage,
                13.0,
                theme::ink(),
                true,
            );
            // 状态分支 1a：已结束但失败 → 在阶段文案下方用错误色展示失败原因（截断 240 字符）
            if update.finished && !update.success {
                theme::wrapped_text(
                    &list_painter,
                    Pos2::new(list_clip.left() + 24.0, list_clip.top() + 102.0),
                    ellipsize_end(&update.message, 240),
                    8.5,
                    Color32::from_rgb(229, 148, 132),
                    list_clip.width() - 48.0,
                );
            // 状态分支 1b：尚未结束 → 绘制不确定进度条（indeterminate bar）
            } else if !update.finished {
                // 进度条轨道：左右各留 80px，宽度至少 80px，高 7px，位于阶段文案下方
                let track = Rect::from_min_size(
                    Pos2::new(list_clip.left() + 80.0, list_clip.top() + 112.0),
                    Vec2::new((list_clip.width() - 160.0).max(80.0), 7.0),
                );
                theme::fill_round(
                    &list_painter,
                    track,
                    theme::corner_radius(),
                    theme::white_alpha(18),
                );
                // 动画相位：time 每秒推进 0.35 个周期并对 1.0 取模，得到 0..1 的循环值；
                // 0.35 为每秒周期数，一个完整循环约 2.9 秒。
                let phase = ((time * 0.35) % 1.0) as f32;
                // 滑块宽 72px，在轨道可用宽度（track.width() - 72）内按 phase 从左滑到右
                let knob = Rect::from_min_size(
                    Pos2::new(track.left() + (track.width() - 72.0) * phase, track.top()),
                    Vec2::new(72.0, track.height()),
                );
                // 用主题绿绘制滑块，形成来回扫动的加载效果
                theme::fill_round(&list_painter, knob, theme::corner_radius(), theme::GREEN);
            }
        // 状态分支 2：尚未选择加载器 → 提示用户先在上方选择类型
        } else if self.loader_kind.is_none() {
            theme::center_text(
                &list_painter,
                Pos2::new(list_clip.center().x, list_clip.top() + 74.0),
                "选择上方加载器以查看可用版本",
                9.3,
                theme::muted(),
                false,
            );
        // 状态分支 3：已选择加载器但版本列表仍在拉取中
        } else if self.loader_versions_receiver.is_some() {
            theme::center_text(
                &list_painter,
                Pos2::new(list_clip.center().x, list_clip.top() + 74.0),
                "正在获取加载器版本…",
                9.3,
                theme::muted(),
                false,
            );
        // 状态分支 4：拉取失败 → 用错误色换行显示错误信息（左内边距 28px，宽度减去两侧内边距）
        } else if let Some(error) = &self.loader_versions_error {
            theme::wrapped_text(
                &list_painter,
                Pos2::new(list_clip.left() + 28.0, list_clip.top() + 48.0),
                error,
                8.5,
                Color32::from_rgb(229, 148, 132),
                list_clip.width() - 56.0,
            );
        // 状态分支 5：正常情况 → 绘制可滚动的版本列表
        } else {
            // 每行固定高 38px（行内容高 34px，另有 4px 间隔），用于计算总内容高度
            let row_height = 38.0;
            // 内容总高度 = 版本数量 × 行高；最大滚动量 = 内容高度 - 可视高度，负数归零
            let content_height = self.loader_versions.len() as f32 * row_height;
            let max_scroll = (content_height - list_clip.height()).max(0.0);
            // 滚轮只在指针位于对话框内时生效，避免鼠标在对话框外时也改动列表偏移；
            // 乘 1.2 略微放大滚动灵敏度，smooth_scroll_delta 为 egui 提供的平滑滚动增量。
            if ui.ctx().input(|input| {
                input
                    .pointer
                    .hover_pos()
                    .is_some_and(|pointer| dialog.contains(pointer))
            }) {
                self.loader_version_scroll -=
                    ui.ctx().input(|input| input.smooth_scroll_delta.y) * 1.2;
            }
            // 把滚动偏移夹在 [0, max_scroll] 内，防止滚出上下边界后出现空白
            self.loader_version_scroll = self.loader_version_scroll.clamp(0.0, max_scroll);
            // 逐行绘制版本条目：y 为考虑滚动偏移后的屏幕坐标
            for (index, version) in self.loader_versions.iter().enumerate() {
                let y = list_top + index as f32 * row_height - self.loader_version_scroll;
                // 简易视口剔除：整行完全在可视区上方（底部仍小于顶部）或下方（顶部超出底部）时跳过绘制，
                // 这样版本很多时也能保持每帧开销稳定。
                // 上方判断使用 y + 34.0（行实际高度）而非 row_height，避免行间空隙造成闪烁。
                if y + 34.0 < list_top || y > list_bottom {
                    continue;
                }
                // 单行条目矩形：对话框左右各内缩 24px，高度 34px（与视口剔除时使用的 34 一致）
                let row = Rect::from_min_size(
                    Pos2::new(dialog.left() + 24.0, y),
                    Vec2::new(dialog.width() - 48.0, 34.0),
                );
                // 注册行点击交互
                let response = ui
                    .interact(row, Id::unique(("loader-version", index)), Sense::click())
                    .on_hover_cursor(CursorIcon::PointingHand);
                // 该行是否为当前选中项
                let selected = self.loader_version_selected == index;
                // 选中或悬停时绘制底色：选中最深（绿色），悬停较浅（白色 8/255 透明度）
                if selected || response.hovered() {
                    theme::fill_round(
                        &list_painter,
                        row,
                        theme::corner_radius(),
                        if selected {
                            theme::alpha(theme::GREEN, 24)
                        } else {
                            theme::white_alpha(8)
                        },
                    );
                }
                // 版本号文本：选中项用亮绿，其余用柔和前景色
                theme::text(
                    &list_painter,
                    Pos2::new(row.left() + 13.0, row.top() + 8.0),
                    version,
                    9.0,
                    if selected {
                        theme::GREEN_BRIGHT
                    } else {
                        theme::ink_soft()
                    },
                );
                // 点击即把选中下标更新为该行，供「安装」按钮判断是否可安装
                if response.clicked() {
                    self.loader_version_selected = index;
                }
            }
        }

        // ===== 底部左侧按钮：跳过加载器 / 仅安装原版 =====
        // 位置：左下角，距左 24px、距底 54px，尺寸 118×34。
        // 语义随模式变化：
        // - 预安装模式：按钮文字「仅安装原版」，点击后调用 begin_preinstall(false) 跳过加载器继续装游戏；
        // - 普通模式：按钮文字「仅保留原版」，点击后直接关闭对话框并清理版本拉取 / 安装状态。
        let skip = Rect::from_min_size(
            Pos2::new(dialog.left() + 24.0, dialog.bottom() - 54.0),
            Vec2::new(118.0, 34.0),
        );
        // 预安装模式标志：本函数内多次使用，先缓存为局部变量避免重复调用 is_some
        let preinstall = self.loader_preinstall.is_some();
        if settings_button(
            ui,
            painter,
            skip,
            "loader-skip",
            if preinstall {
                "仅安装原版"
            } else {
                "仅保留原版"
            },
            // 第三个布尔参数为 false，表示该按钮始终可点击（禁用态仅由后续 && !installing 控制）
            false,
        ) && !installing
        {
            if preinstall {
                self.begin_preinstall(false);
            } else {
                // 普通模式关闭对话框：同时清空接收端与进度，避免后台任务结果污染下次打开的状态
                self.loader_dialog_open = false;
                self.loader_versions_receiver = None;
                self.loader_install_update = None;
            }
        }
        // ===== 底部右侧按钮：开始安装 =====
        // 位置：右下角，距右 150px（按钮宽 126，故右侧留 24px 内边距），距底 54px。
        let install = Rect::from_min_size(
            Pos2::new(dialog.right() - 150.0, dialog.bottom() - 54.0),
            Vec2::new(126.0, 34.0),
        );
        // 可安装的四个条件必须同时满足：
        // 1) 当前不在安装中；
        // 2) 已选择加载器类型；
        // 3) 版本列表非空（至少拉到一个可选版本）；
        // 4) 没有进行中的安装任务，或上一次安装已结束（成功，或已结束但失败——允许重试）。
        // 其中 is_none_or 表示「None 视为满足条件」，即从未安装过也允许点击。
        let can_install = !installing
            && self.loader_kind.is_some()
            && !self.loader_versions.is_empty()
            && self
                .loader_install_update
                .as_ref()
                .is_none_or(|update| update.success || update.finished);
        // 注意：这里把 can_install 同时作为按钮的可用态与点击后的执行条件，
        // 保证状态在两次读取之间发生变化时也不会误触发安装。
        if settings_button(
            ui,
            painter,
            install,
            "loader-install",
            if preinstall {
                "开始安装"
            } else {
                "安装加载器"
            },
            can_install,
        ) && can_install
        {
            if preinstall {
                // 预安装模式：带着当前选择的加载器继续安装游戏本体
                self.begin_preinstall(true);
            } else {
                // 普通模式：对已有游戏版本安装加载器
                self.start_loader_install();
            }
        }
    }

    /// 绘制首页底部「最新动态」区域：标题行（DISCOVER / 最新动态 / 查看全部）+ 3 张新闻卡片。
    ///
    /// 数据来源为 `LauncherApp::news_items`（由 `crate::news` 模块从 Mojang 官方内容接口
    /// 拉取并解析的新闻列表），最多取前 3 条渲染；配图字节随新闻一起下载，
    /// `ensure_news_textures` 负责把字节解码为 egui 纹理并缓存在 `news_textures` 中，
    /// 贴图尚未就绪时回退到 `scene::paint_news_art` 程序化插画。
    ///
    /// # 参数
    /// - `_ctx`：egui 上下文。名字带下划线只是沿用既有签名，本函数内部仍会用它调用
    ///   `ensure_news_textures`（把新闻配图上传为 egui 纹理）。点击卡片本身不直接使用它。
    /// - `ui`：egui Ui，用于标题行「查看全部」与 3 张卡片的点击命中检测。
    /// - `painter`：自绘 Painter，全部图形与文字绘制在其上。
    /// - `layout`：首页布局，提供 `latest` 区域顶部与 `news_cards[0..3]` 三张卡片的矩形。
    /// - `time`：全局动画时间（秒），驱动玻璃高光与程序化插画的流动效果。
    /// - `motion`：卡片动效，用于让标题与卡片跟随页面切换动画一起位移。
    ///
    /// # 副作用
    /// - 「查看全部」点击时调用 `open_external_url` 打开系统浏览器；
    ///   打开失败会通过 `show_toast` 提示用户。
    /// - 点击某张新闻卡片会写入 `self.news = Some(global_index)`（进入详情视图）
    ///   并把 `self.menu` 重置为 `Menu::None`，同时关闭可能打开的菜单。
    /// - `ensure_news_textures` 在前 3 条新闻的 `image_key` 发生变化时重新解码配图，
    ///   并把结果写入 `news_textures`（同步解码，不发起网络请求）。
    pub(in crate::app) fn paint_latest(
        &mut self,
        _ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
        motion: CardMotion,
    ) {
        // 标题行与卡片共用同一入场位移：取第 0 张卡片在动效中的偏移量，
        // 保证「DISCOVER / 最新动态 / 查看全部」与卡片同步滑入，不会出现割裂感。
        let heading_shift = motion.offset(layout.news_cards[0]);
        // 小号等宽英文眉题「DISCOVER」，紧贴 latest 区域顶部
        theme::mono_text(
            painter,
            Pos2::new(layout.content_left, layout.latest.top() + heading_shift),
            "DISCOVER",
            7.2,
            theme::muted(),
        );
        // 中文主标题「最新动态」，位于眉题下方 11px
        theme::bold_text(
            painter,
            Pos2::new(
                layout.content_left,
                layout.latest.top() + 11.0 + heading_shift,
            ),
            "最新动态",
            15.0,
            theme::ink_soft(),
        );
        // 右上角「查看全部 →」热区：宽 78、高 24，右对齐内容区（content_right）
        let all_news = Rect::from_min_size(
            Pos2::new(
                layout.content_right - 78.0,
                layout.latest.top() + 10.0 + heading_shift,
            ),
            Vec2::new(78.0, 24.0),
        );
        // 注册点击交互并在悬停时显示手型光标
        let all_response = ui
            .interact(all_news, Id::unique("all-news"), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        // 按钮文字：悬停时提亮为浅绿，否则使用弱化色
        theme::text(
            painter,
            Pos2::new(all_news.left(), all_news.top() + 2.0),
            "查看全部  →",
            8.3,
            if all_response.hovered() {
                Color32::from_rgb(188, 224, 183)
            } else {
                theme::muted()
            },
        );
        // 点击「查看全部」：用系统默认浏览器打开 Minecraft 官网文章列表页。
        // 这是唯一一处外链跳转的兴趣点，失败（如系统无默认浏览器）时以 Toast 告知用户，不中断渲染。
        if all_response.clicked() {
            match open_external_url("https://www.minecraft.net/article") {
                // 成功时不额外提示，避免打断用户
                Ok(()) => {}
                Err(error) => self.show_toast(format!("无法打开 Minecraft 动态：{error}")),
            }
        }

        // 确保新闻配图纹理与当前前 3 条新闻的 image_key 保持一致：
        // 内部仅在 key 变化时重新解码并上传纹理，结果写入 self.news_textures。
        // 本调用放在绘制卡片之前，使同一帧内就能用上刚生成的纹理。
        self.ensure_news_textures(_ctx);
        // 取前 3 条新闻的 (全局下标, 新闻副本)。收集成独立 Vec 的原因：
        // 后续循环中需要同时可变借用 self（写 news 字段）与不可变借用新闻数据，
        // 先克隆出来可避免借用冲突。
        let entries: Vec<(usize, MinecraftNews)> = self
            .news_items
            .iter()
            .take(3)
            .cloned()
            .enumerate()
            .collect();
        // 克隆贴图表（Option<TextureHandle> 列表），同样用于避免循环内与 self 的借用冲突。
        // 未加载完成的槽位为 None，下标与新闻条目一一对应。
        let textures = self.news_textures.clone();

        // 固定绘制 3 张卡片（布局同样固定预留 3 个位置）：
        // 即使新闻不足 3 条，也保留空位并显示占位文案，避免布局跳动。
        for index in 0..3 {
            // 卡片矩形：由布局给出源矩形，再经动效换算为当前绘制位置
            let rect = motion.rect(layout.news_cards[index]);
            // 整张卡片即为点击热区（点击进入新闻详情）
            let response = ui
                .interact(rect, Id::unique(("news", index)), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand);
            // 第 index 张卡片在主题中的强调色（用于玻璃面板描边高光）
            let accent = news_accent(index);
            // 卡片底板：玻璃面板，hovered 会增强高光；相位 index*1.37 让三张卡的流光彼此错开
            theme::glass_panel(
                painter,
                rect,
                theme::corner_radius(),
                theme::GlassSpec::new(
                    Color32::from_rgb(20, 29, 23),
                    accent,
                    time,
                    index as f32 * 1.37,
                )
                .hovered(response.hovered()),
            );
            // 裁剪层：卡片内容（封面图、文字）不越出圆角
            let news_content = card_clip(painter, rect);
            let painter = &news_content;
            // 左侧封面图区域：四周各留 7px 内边距，宽固定 92px，
            // 高度随卡片高度自适应（上下各去掉 7px）。
            let art = Rect::from_min_size(
                rect.left_top() + Vec2::new(7.0, 7.0),
                Vec2::new(92.0, rect.height() - 14.0),
            );
            // 若该条新闻的贴图已加载（Some(Some(_))），绘制真实图片；
            // 否则按卡片序号回退到三种程序化插画之一（版本 / 模组 / 社区）。
            if let Some(Some(texture)) = textures.get(index) {
                paint_rounded_image(painter, art, 12.0, texture, texture.size());
            } else {
                scene::paint_news_art(
                    painter,
                    art,
                    match index {
                        0 => scene::NewsArt::Version,
                        1 => scene::NewsArt::Mod,
                        // 其余情况（index == 2）使用社区主题插画
                        _ => scene::NewsArt::Community,
                    },
                    time,
                );
            }
            // 给封面图描一圈淡白边框，便于与卡片底色分离
            theme::stroke_round(painter, art, 12, 1.0, theme::white_alpha(28));
            // 文字区起始 x：封面图右侧再留 13px 间距
            let copy_x = art.right() + 13.0;
            // 取该卡片对应的新闻条目；数量不足时为 None，走占位文案分支
            let entry = entries.get(index).map(|(_, entry)| entry);
            // 眉题（kicker）：正常显示「分类 · 日期」，日期为空时只显示分类；
            // 无数据时按错误 / 加载中 / 默认三级回退，让占位文案也能传达当前状态。
            let kicker = entry
                .map(|entry| {
                    if entry.date.is_empty() {
                        entry.category.clone()
                    } else {
                        format!("{} · {}", entry.category, entry.date)
                    }
                })
                .unwrap_or_else(|| {
                    if let Some(error) = &self.news_error {
                        // 拉取失败：直接把错误信息作为眉题展示
                        error.clone()
                    } else if self.news_receiver.is_some() {
                        // 请求进行中
                        "正在连接 Minecraft 官方动态".to_owned()
                    } else {
                        // 初始 / 空闲状态
                        "Minecraft 官方动态".to_owned()
                    }
                });
            // 标题：截断到 24 字符，避免超出卡片宽度；无数据时提示正在获取
            let title = entry
                .map(|entry| ellipsize_end(&entry.title, 24))
                .unwrap_or_else(|| "正在获取最新内容".to_owned());
            // 摘要：截断 52 字符，并过滤掉空字符串（有时摘要字段为空）；
            // 最终为空则使用默认引导文案。
            let description = entry
                .map(|entry| ellipsize_end(&entry.summary, 52))
                .filter(|description| !description.is_empty())
                .unwrap_or_else(|| "稍后刷新即可查看 Mojang 发布的最新消息。".to_owned());
            // 眉题文字：小字号、偏绿色，距卡片顶部 14px
            theme::text(
                painter,
                Pos2::new(copy_x, rect.top() + 14.0),
                ellipsize_end(&kicker, 36),
                7.3,
                Color32::from_rgb(119, 169, 114),
            );
            // 标题文字：加粗，距顶部 31px
            theme::bold_text(
                painter,
                Pos2::new(copy_x, rect.top() + 31.0),
                &title,
                11.3,
                theme::ink_soft(),
            );
            // 摘要文字：自动换行，可用宽度为卡片右边缘减去文字起点再留 12px 内边距
            theme::wrapped_text(
                painter,
                Pos2::new(copy_x, rect.top() + 50.0),
                &description,
                7.6,
                Color32::from_rgb(124, 138, 128),
                rect.right() - copy_x - 12.0,
            );
            // 仅当确实有新闻数据时才显示「阅读官方详情 →」，避免占位卡片出现可点击暗示
            if entry.is_some() {
                theme::text(
                    painter,
                    Pos2::new(copy_x, rect.bottom() - 20.0),
                    "阅读官方详情  →",
                    7.5,
                    Color32::from_rgb(159, 210, 152),
                );
            }
            // 点击卡片：仅在确有新闻数据时进入详情。
            // entries 中保存的是 enumerate 产生的全局下标；写回 self.news 时以元组中的下标为准。
            if response.clicked()
                && let Some((global_index, _)) = entries.get(index)
            {
                self.news = Some(*global_index);
                // 同步关闭可能展开的菜单，避免详情视图与菜单叠加
                self.menu = Menu::None;
            }
        }
    }
}
