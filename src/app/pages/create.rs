//! LauncherApp 的「创造页面」相关方法实现
//!
//! 本文件为 `crate::app::LauncherApp` 实现「创造」功能模块的全部逻辑，主要包含：
//! 1. **皮肤编辑器**：64×64 Minecraft 玩家皮肤（Steve/Alex 模型）的像素绘制、导入/导出、
//!    撤销/重做、以及基于 UV 映射的 3D 角色预览与「直接在模型上绘制」能力。
//! 2. **模组工程生成器**：根据表单（名称、模组 ID、Java 包名、版本、目标 MC 版本、加载器）
//!    生成可编译的 Java + Gradle 工程脚手架。
//! 3. **旧版创造首页**（已标记 `#[allow(dead_code)]`）：启动游戏创建世界，以及指向
//!    存档(saves)/资源包(resourcepacks)/截图(screenshots) 目录的快捷卡片。
//!
//! 在启动器中的角色：这是「创造」标签页（与首页、收藏等并列）的核心 UI 与交互逻辑层，
//! 所有绘制均通过 `Painter` 直接作矢量图形（非 egui 默认控件），交互通过 `ui.interact`
//! 在自定义矩形上模拟按钮/画布。
//!
//! 涉及的主要类型与辅助函数（定义于其它模块，此处仅调用）：
//! - `self.skin_pixels`：长度 64*64*4 的 RGBA 字节数组，皮肤编辑器的唯一真相来源。
//! - `self.skin_history`：撤销/重做历史栈（`SkinHistory`）。
//! - `skin_preview_faces` / `paint_skin_model` / `skin_face_at`：3D 预览几何与拾取。
//! - `write_mod_project`：把 `ModProjectSettings` 落地为磁盘工程。
use super::super::*;

impl LauncherApp {
    /// 刷新皮肤纹理：把内存像素数组上传到 egui 纹理，NEAREST 邻近采样保证像素硬边
    pub(in crate::app) fn ensure_skin_texture(&mut self, ctx: &egui::Context) {
        // 无脏标记且纹理已存在，直接跳过
        if !self.skin_dirty && self.skin_texture.is_some() {
            return;
        }
        // 从 RGBA 像素数组构造 64x64 非预乘颜色图像
        let image = egui::ColorImage::from_rgba_unmultiplied([64, 64], &self.skin_pixels);
        // 上传纹理，使用邻近插值，像素画风格
        self.skin_texture =
            Some(ctx.load_texture("skin-editor-texture", image, egui::TextureOptions::NEAREST));
        // 清除脏标记，代表纹理和像素数据同步
        self.skin_dirty = false;
    }

    /// 开始一笔绘制：记录当前画布快照，用于撤销
    pub(in crate::app) fn begin_skin_stroke(&mut self) {
        // 仅在没有当前笔画、没有暂存快照时保存
        if self.skin_last_pixel.is_none() && self.skin_stroke_before.is_none() {
            self.skin_stroke_before = Some(self.skin_pixels.clone());
        }
    }

    /// 结束一笔绘制：将本次笔画的前后快照存入历史栈
    pub(in crate::app) fn finish_skin_stroke(&mut self) {
        if let Some(before) = self.skin_stroke_before.take() {
            self.skin_history.record(before, &self.skin_pixels);
        }
    }

    /// 撤销皮肤编辑上一步
    pub(in crate::app) fn undo_skin_edit(&mut self) {
        // 清空绘制状态缓存
        self.skin_last_pixel = None;
        self.skin_preview_last_face = None;
        self.skin_stroke_before = None;
        // 从历史栈取出上一版像素
        if let Some(previous) = self.skin_history.undo(&self.skin_pixels) {
            self.skin_pixels = previous;
            self.skin_dirty = true;
            self.skin_status = Some("已撤销上一步皮肤编辑".to_owned());
        }
    }

    /// 重做皮肤编辑（恢复被撤销的操作）
    pub(in crate::app) fn redo_skin_edit(&mut self) {
        self.skin_last_pixel = None;
        self.skin_preview_last_face = None;
        self.skin_stroke_before = None;
        if let Some(next) = self.skin_history.redo(&self.skin_pixels) {
            self.skin_pixels = next;
            self.skin_dirty = true;
            self.skin_status = Some("已重做皮肤编辑".to_owned());
        }
    }

    /// 设置单个像素颜色；橡皮擦模式写入全透明
    pub(in crate::app) fn set_skin_pixel(&mut self, x: usize, y: usize) {
        // 越界直接丢弃
        if x >= 64 || y >= 64 {
            return;
        }
        // RGBA 字节数组布局：每个像素 4 字节（R,G,B,A），行优先，故第 (x,y) 像素
        // 的起始下标 = (y * 宽度 + x) * 4，宽度固定为 64。
        let offset = (y * 64 + x) * 4;
        let color = if self.skin_erasing {
            [0, 0, 0, 0]
        } else {
            self.skin_color
        };
        // 只有颜色发生变化才写入，减少脏标记频繁触发
        if self.skin_pixels[offset..offset + 4] != color {
            self.skin_pixels[offset..offset + 4].copy_from_slice(&color);
            self.skin_dirty = true;
        }
    }

    /// Bresenham 直线算法：拖动时连续填充像素，实现画笔拖线
    pub(in crate::app) fn paint_skin_line(&mut self, from: (usize, usize), to: (usize, usize)) {
        // 将端点坐标转为带符号整数，方便处理任意方向的增量
        let (mut x0, mut y0) = (from.0 as i32, from.1 as i32);
        let (x1, y1) = (to.0 as i32, to.1 as i32);
        // dx = x 方向总步数（恒为正，用 abs 取绝对值）
        let dx = (x1 - x0).abs();
        // sx = x 方向单步符号：向右为 +1，向左为 -1
        let sx = if x0 < x1 { 1 } else { -1 };
        // dy 取负值：Bresenham 标准写法中保持误差为整数，便于后续与 doubled 比较
        let dy = -(y1 - y0).abs();
        // sy = y 方向单步符号：向下为 +1，向上为 -1
        let sy = if y0 < y1 { 1 } else { -1 };
        // 初始误差 = dx + dy（注意 dy 为负），代表从起点开始累积的偏离量
        let mut error = dx + dy;
        loop {
            // 在画布范围内才绘制（坐标落在 0..64 内）
            if (0..64).contains(&x0) && (0..64).contains(&y0) {
                self.set_skin_pixel(x0 as usize, y0 as usize);
            }
            // 到达终点，结束整条直线
            if x0 == x1 && y0 == y1 {
                break;
            }
            // 用 2 倍误差与 dx/dy 比较，决定本步向 x 还是 y 推进（可同时推进两者以走对角）
            let doubled = 2 * error;
            // 误差偏向 y 轴侧时，x 方向走一步
            if doubled >= dy {
                error += dy;
                x0 += sx;
            }
            // 误差偏向 x 轴侧时，y 方向走一步
            if doubled <= dx {
                error += dx;
                y0 += sy;
            }
        }
    }

    /// 导入外部PNG皮肤图片，自动识别 Alex(细臂)/Steve(宽臂)
    pub(in crate::app) fn import_skin_file(&mut self) {
        let Some(path) = pick_image_file(false) else {
            return;
        };
        match load_player_skin(&path) {
            Ok(resized) => {
                // 检测是否为细臂皮肤
                self.skin_model_alex = detect_slim_skin(&resized);
                let before = self.skin_pixels.clone();
                self.skin_pixels = resized;
                self.skin_history.record(before, &self.skin_pixels);
                // 清空绘制缓存
                self.skin_last_pixel = None;
                self.skin_preview_last_face = None;
                self.skin_stroke_before = None;
                self.skin_dirty = true;
                self.skin_status = Some(format!(
                    "已导入 {} · {}",
                    path.display(),
                    if self.skin_model_alex {
                        "Alex 细臂"
                    } else {
                        "Steve 宽臂"
                    }
                ));
            }
            Err(error) => self.skin_status = Some(format!("导入皮肤失败：{error}")),
        }
    }

    /// 将当前编辑的64x64皮肤导出为PNG文件
    pub(in crate::app) fn export_skin_file(&mut self) {
        let Some(path) = pick_image_file(true) else {
            return;
        };
        let Some(image) = image::RgbaImage::from_raw(64, 64, self.skin_pixels.clone()) else {
            self.skin_status = Some("皮肤数据无效".to_owned());
            return;
        };
        match image.save(&path) {
            Ok(()) => self.skin_status = Some(format!("已导出 {}", path.display())),
            Err(error) => self.skin_status = Some(format!("导出皮肤失败：{error}")),
        }
    }

    /// 根据表单配置生成模组Gradle工程目录
    pub(in crate::app) fn create_mod_project(&mut self) {
        let Some(output_root) = self.mod_output_directory.clone() else {
            self.mod_status = Some("请先选择工程保存位置".to_owned());
            return;
        };
        let settings = ModProjectSettings {
            name: self.mod_name_input.clone(),
            id: self.mod_id_input.to_ascii_lowercase(),
            package: self.mod_package_input.clone(),
            version: self.mod_version_input.clone(),
            minecraft: self.mod_minecraft_input.clone(),
            loader: self.mod_loader_index,
        };
        match write_mod_project(&output_root, &settings) {
            Ok(path) => {
                self.mod_status = Some(format!("工程已创建：{}", path.display()));
                open_path(&path);
            }
            Err(error) => self.mod_status = Some(format!("创建模组工程失败：{error}")),
        }
    }

    /// 绘制「创造」子页面（皮肤制作 / 模组制作 标签页容器），带标签切换过渡动画
    pub(in crate::app) fn paint_create_workspace(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
        motion: CardMotion,
    ) {
        // 页面主体区域
        let page = Rect::from_min_max(
            Pos2::new(layout.content_left, layout.header.bottom() + 14.0),
            Pos2::new(layout.content_right, layout.screen.bottom() - 22.0),
        );
        // 顶部标题栏玻璃面板，带动画位移
        let header = motion.rect(Rect::from_min_size(
            page.left_top(),
            Vec2::new(page.width(), 92.0),
        ));
        theme::glass_panel(
            painter,
            header,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(20, 32, 24), theme::GREEN, time, 26.2),
        );
        let header_content = card_clip(painter, header);
        theme::bold_text(
            &header_content,
            Pos2::new(header.left() + 24.0, header.top() + 16.0),
            "创造",
            21.0,
            theme::ink(),
        );
        theme::text(
            &header_content,
            Pos2::new(header.left() + 24.0, header.top() + 47.0),
            "制作 Minecraft 皮肤与模组工程",
            8.5,
            theme::muted(),
        );
        // 标签下划线高亮：其水平位置随"弹簧动画"的 tab_position(0~1) 左右滑动，
        // 速度绝对值越大则高亮块越被横向拉宽，制造弹性拉伸的视觉反馈。
        // 232.0 = 右侧两个标签按钮整体向左的偏移基准；108.0 = 相邻标签中心间距；
        // 50.0 = 高亮块中心相对标签左缘的内缩补偿；100.0 = 高亮块基础宽度；
        // 5.0 = 速度到宽度拉伸的系数；12.0 = 最大额外拉伸量；46.0/38.0 = 高亮块纵向中心与高度。
        let tab_center =
            header.right() - 232.0 + self.create_tab_position.clamp(0.0, 1.0) * 108.0 + 50.0;
        let tab_highlight = Rect::from_center_size(
            Pos2::new(tab_center, header.top() + 46.0),
            Vec2::new(
                100.0 + (self.create_tab_velocity.abs() * 5.0).clamp(0.0, 12.0),
                38.0,
            ),
        );
        theme::fill_round(
            &header_content,
            tab_highlight,
            theme::corner_radius(),
            theme::alpha(theme::GREEN, 28),
        );
        // 渲染标签：皮肤制作 / 模组制作
        for (index, label) in ["皮肤制作", "模组制作"].into_iter().enumerate() {
            let tab = Rect::from_min_size(
                Pos2::new(
                    header.right() - 232.0 + index as f32 * 108.0,
                    header.top() + 27.0,
                ),
                Vec2::new(100.0, 38.0),
            );
            let response = ui
                .interact(tab, Id::unique(("create-tab", index)), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand);
            let selected = self.create_tab == index;
            if !selected {
                theme::fill_round(
                    &header_content,
                    tab,
                    theme::corner_radius(),
                    if response.hovered() {
                        theme::white_alpha(12)
                    } else {
                        theme::white_alpha(5)
                    },
                );
            }
            theme::center_text(
                &header_content,
                tab.center(),
                label,
                8.8,
                if selected {
                    theme::ink()
                } else {
                    theme::muted()
                },
                selected,
            );
            // 点击切换标签，触发弹簧动画
            if response.clicked() {
                self.switch_create_tab(index);
                ui.ctx().request_repaint();
            }
        }

        // 主体内容区域，带页面切换横向滑动动画
        let body_base = motion.rect(Rect::from_min_max(
            Pos2::new(page.left(), page.top() + 104.0),
            page.right_bottom(),
        ));
        theme::glass_panel(
            painter,
            body_base,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(18, 29, 22), theme::GREEN, time, 26.8),
        );
        let body_clip = card_clip(painter, body_base);
        // 页面切换进度：0 = 完全停留在旧页，1 = 完全切换到新页（上限 1.08 留一点缓冲余量）
        let progress = self.create_transition_progress.clamp(0.0, 1.08);
        // 旧标签页：随进度向左滑出（direction 控制方向），并整体淡出
        if let Some(previous) = self.previous_create_tab {
            let mut outgoing_painter = body_clip.clone();
            outgoing_painter.set_opacity((1.0 - progress).clamp(0.0, 1.0));
            let outgoing_body = body_base.translate(Vec2::new(
                -self.create_transition_direction * body_base.width() * progress,
                0.0,
            ));
            // active=false：旧页在过渡期间不再响应交互
            self.paint_create_tab_content(
                previous,
                ctx,
                ui,
                &outgoing_painter,
                outgoing_body,
                time,
                false,
            );
        }
        // 新标签页：从右侧（或相反方向）滑入，并随进度淡入
        let mut incoming_painter = body_clip;
        incoming_painter.set_opacity(progress.clamp(0.0, 1.0));
        let incoming_body = body_base.translate(Vec2::new(
            self.create_transition_direction * body_base.width() * (1.0 - progress),
            0.0,
        ));
        self.paint_create_tab_content(
            self.create_tab,
            ctx,
            ui,
            &incoming_painter,
            incoming_body,
            time,
            self.previous_create_tab.is_none(),
        );
    }

    /// 根据当前标签序号，分发渲染皮肤编辑器 / 模组表单
    #[allow(clippy::too_many_arguments)]
    pub(in crate::app) fn paint_create_tab_content(
        &mut self,
        tab: usize,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        body: Rect,
        time: f64,
        active: bool,
    ) {
        let layer = painter.with_clip_rect(body.shrink(1.0));
        if tab == 0 {
            // 0 = 皮肤制作
            self.paint_skin_workspace(ctx, ui, &layer, body, time, active);
        } else {
            // 1 = 模组制作
            self.paint_mod_workspace(ui, &layer, body, time, active);
        }
    }

    /// 皮肤编辑器主画布、色板、按钮、3D预览渲染逻辑
    pub(in crate::app) fn paint_skin_workspace(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        body: Rect,
        _time: f64,
        active: bool,
    ) {
        // 同步纹理
        self.ensure_skin_texture(ctx);
        // 2D画布区域，正方形
        let canvas = Rect::from_min_size(
            Pos2::new(body.left() + 24.0, body.top() + 24.0),
            Vec2::splat((body.height() - 48.0).min(360.0)),
        );
        theme::fill_round(
            painter,
            canvas,
            theme::corner_radius(),
            Color32::TRANSPARENT,
        );
        // 绘制64x64皮肤纹理
        if let Some(texture) = self.skin_texture.as_ref() {
            painter.image(
                texture.id(),
                canvas,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        // 绘制8×8辅助网格线，方便对齐像素
        for index in 1..8 {
            let x = canvas.left() + canvas.width() * index as f32 / 8.0;
            let y = canvas.top() + canvas.height() * index as f32 / 8.0;
            painter.line_segment(
                [Pos2::new(x, canvas.top()), Pos2::new(x, canvas.bottom())],
                Stroke::new(0.5, theme::white_alpha(18)),
            );
            painter.line_segment(
                [Pos2::new(canvas.left(), y), Pos2::new(canvas.right(), y)],
                Stroke::new(0.5, theme::white_alpha(18)),
            );
        }
        // 画布交互：点击/拖动绘制像素
        let response = ui
            .interact(canvas, Id::unique("skin-canvas"), Sense::click_and_drag())
            .on_hover_cursor(CursorIcon::Crosshair);
        if active
            && (response.clicked() || response.dragged())
            && let Some(pointer) = response.interact_pointer_pos()
            && canvas.contains(pointer)
        {
            // 屏幕坐标映射到 0~63 像素坐标
            let x = (((pointer.x - canvas.left()) / canvas.width()) * 64.0)
                .floor()
                .clamp(0.0, 63.0) as usize;
            let y = (((pointer.y - canvas.top()) / canvas.height()) * 64.0)
                .floor()
                .clamp(0.0, 63.0) as usize;
            self.begin_skin_stroke();
            // 拖动时画线，单点直接填色
            if let Some(previous) = self.skin_last_pixel {
                self.paint_skin_line(previous, (x, y));
            } else {
                self.set_skin_pixel(x, y);
            }
            self.finish_skin_stroke();
            self.skin_last_pixel = Some((x, y));
            self.skin_preview_last_face = None;
            ui.ctx().request_repaint();
        }
        // 鼠标抬起，结束当前笔画
        if ui.ctx().input(|input| input.pointer.any_released()) {
            self.skin_last_pixel = None;
            self.skin_preview_last_face = None;
            self.skin_stroke_before = None;
        }
        theme::stroke_round(
            painter,
            canvas,
            theme::corner_radius(),
            1.0,
            theme::white_alpha(28),
        );

        // 右侧3D预览卡片
        let preview_card = Rect::from_min_size(
            Pos2::new(body.right() - 372.0, body.top() + 24.0),
            Vec2::new(348.0, body.height() - 48.0),
        );
        theme::glass_panel(
            painter,
            preview_card,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(20, 32, 24), theme::GREEN, 2.2, 32.8),
        );
        let preview_content = card_clip(painter, preview_card);
        let tools_x = canvas.right() + 28.0;
        let tools_width = (preview_card.left() - tools_x - 24.0).max(260.0);
        // 工具栏标题
        theme::bold_text(
            painter,
            Pos2::new(tools_x, body.top() + 24.0),
            "64 × 64 皮肤画布",
            16.0,
            theme::ink(),
        );
        theme::text(
            painter,
            Pos2::new(tools_x, body.top() + 51.0),
            "在画布上点击或拖动绘制像素 · Ctrl+Z 撤销",
            8.2,
            theme::muted(),
        );
        // 取色预览方块
        let color_swatch =
            Rect::from_min_size(Pos2::new(tools_x, body.top() + 92.0), Vec2::splat(36.0));
        theme::fill_round(
            painter,
            color_swatch,
            theme::corner_radius(),
            Color32::from_rgba_unmultiplied(
                self.skin_color[0],
                self.skin_color[1],
                self.skin_color[2],
                self.skin_color[3],
            ),
        );
        theme::stroke_round(
            painter,
            color_swatch,
            theme::corner_radius(),
            1.0,
            theme::white_alpha(42),
        );
        theme::bold_text(
            painter,
            Pos2::new(tools_x + 48.0, body.top() + 91.0),
            skin_rgb_hex(self.skin_color),
            11.0,
            theme::ink(),
        );
        theme::text(
            painter,
            Pos2::new(tools_x + 48.0, body.top() + 113.0),
            "完整 RGB · 每个通道 0–255",
            7.6,
            theme::muted(),
        );
        let slider_width = tools_width.clamp(250.0, 366.0);
        // R/G/B 三个颜色滑块
        let channels = [
            ("R", Color32::from_rgb(235, 91, 91)),
            ("G", Color32::from_rgb(83, 199, 121)),
            ("B", Color32::from_rgb(88, 142, 244)),
        ];
        for (index, (label, accent)) in channels.into_iter().enumerate() {
            let mut channel = self.skin_color[index];
            let before = channel;
            paint_skin_rgb_slider(
                ui,
                painter,
                Rect::from_min_size(
                    Pos2::new(tools_x, body.top() + 145.0 + index as f32 * 38.0),
                    Vec2::new(slider_width, 30.0),
                ),
                match index {
                    0 => "skin-rgb-red",
                    1 => "skin-rgb-green",
                    _ => "skin-rgb-blue",
                },
                label,
                &mut channel,
                accent,
                active,
            );
            if channel != before {
                self.skin_color[index] = channel;
                self.skin_color[3] = 255;
                self.skin_erasing = false;
            }
        }
        // 橡皮擦 / 模型切换 / 撤销 / 重做 一行按钮
        let eraser = Rect::from_min_size(
            Pos2::new(tools_x, body.top() + 267.0),
            Vec2::new(72.0, 34.0),
        );
        if active
            && settings_button(
                ui,
                painter,
                eraser,
                "skin-eraser",
                "橡皮擦",
                self.skin_erasing,
            )
        {
            self.skin_erasing = !self.skin_erasing;
        }
        let model = Rect::from_min_size(
            Pos2::new(tools_x + 82.0, body.top() + 267.0),
            Vec2::new(110.0, 34.0),
        );
        if active
            && settings_button(
                ui,
                painter,
                model,
                "skin-model",
                if self.skin_model_alex {
                    "Alex 模型"
                } else {
                    "Steve 模型"
                },
                false,
            )
        {
            self.skin_model_alex = !self.skin_model_alex;
        }
        let undo = Rect::from_min_size(
            Pos2::new(tools_x + 202.0, body.top() + 267.0),
            Vec2::new(74.0, 34.0),
        );
        if active
            && skin_history_button(
                ui,
                painter,
                undo,
                "skin-undo",
                "撤销",
                self.skin_history.can_undo(),
            )
        {
            self.undo_skin_edit();
            ui.ctx().request_repaint();
        }
        let redo = Rect::from_min_size(
            Pos2::new(tools_x + 286.0, body.top() + 267.0),
            Vec2::new(74.0, 34.0),
        );
        if active
            && skin_history_button(
                ui,
                painter,
                redo,
                "skin-redo",
                "重做",
                self.skin_history.can_redo(),
            )
        {
            self.redo_skin_edit();
            ui.ctx().request_repaint();
        }
        // 导入 / 导出 / 清空 / 应用皮肤
        let buttons = [
            ("导入 PNG", "skin-import"),
            ("导出 PNG", "skin-export"),
            ("清空画布", "skin-clear"),
            ("应用皮肤", "skin-apply"),
        ];
        for (index, (label, id)) in buttons.into_iter().enumerate() {
            let button = Rect::from_min_size(
                Pos2::new(tools_x + index as f32 * 118.0, body.top() + 317.0),
                Vec2::new(108.0, 36.0),
            );
            if active && settings_button(ui, painter, button, id, label, false) {
                match index {
                    0 => self.import_skin_file(),
                    1 => self.export_skin_file(),
                    2 => {
                        // 清空画布并记录撤销快照
                        let before = self.skin_pixels.clone();
                        self.skin_pixels.fill(0);
                        self.skin_history.record(before, &self.skin_pixels);
                        self.skin_last_pixel = None;
                        self.skin_preview_last_face = None;
                        self.skin_stroke_before = None;
                        self.skin_dirty = true;
                        self.skin_status = Some("画布已清空".to_owned());
                    }
                    _ => self.apply_editor_skin_to_account(ui.ctx()),
                }
            }
        }
        // 底部状态提示文字
        if let Some(status) = self.skin_status.as_ref() {
            theme::text(
                painter,
                Pos2::new(tools_x, body.top() + 373.0),
                fit_text_to_width(painter, status, theme::font(8.0), tools_width),
                8.0,
                theme::GREEN_BRIGHT,
            );
        }
        // 3D预览区域标题、提示
        let preview_inner = preview_content;
        theme::bold_text(
            &preview_inner,
            Pos2::new(preview_card.left() + 18.0, preview_card.top() + 17.0),
            "3D 角色预览",
            10.0,
            theme::ink(),
        );
        theme::text(
            &preview_inner,
            Pos2::new(preview_card.left() + 18.0, preview_card.top() + 39.0),
            "右键拖动旋转 · 左键在模型上绘制",
            7.5,
            theme::muted(),
        );
        let preview_stage = Rect::from_min_max(
            Pos2::new(preview_card.left() + 14.0, preview_card.top() + 58.0),
            Pos2::new(preview_card.right() - 14.0, preview_card.bottom() - 24.0),
        );
        let preview_response = ui
            .interact(
                preview_stage,
                Id::unique("skin-preview-3d"),
                Sense::click_and_drag(),
            )
            .on_hover_cursor(CursorIcon::Crosshair);
        // 右键拖拽旋转模型（偏航、俯仰）
        if active && preview_response.dragged_by(egui::PointerButton::Secondary) {
            let delta = ui.ctx().input(|input| input.pointer.delta());
            self.skin_preview_yaw =
                (self.skin_preview_yaw + delta.x * 0.012).rem_euclid(std::f32::consts::TAU);
            self.skin_preview_pitch =
                (self.skin_preview_pitch + delta.y * 0.009).clamp(-1.15, 1.15);
        }
        // 自适应缩放模型大小：根据预览区宽高与"模型设计基准尺寸"换算出缩放系数。
        // 44.0 = 横向留白；18.2 = 模型横向基准单位；30.0 = 纵向留白；33.5 = 模型纵向基准单位。
        // 取宽高比中较小者，保证模型在两种维度下都不会超出预览区。.max(1.0) 避免除零/负值。
        let model_scale = ((preview_stage.width() - 44.0).max(1.0) / 18.2)
            .min((preview_stage.height() - 30.0).max(1.0) / 33.5);
        // 绘制角色底部阴影：阴影中心位于预览区水平中央、角色脚下（用缩放系数推算纵向偏移）
        let shadow_center = Pos2::new(
            preview_stage.center().x,
            preview_stage.center().y + model_scale * 15.8,
        );
        preview_inner.add(Shape::ellipse_filled(
            shadow_center + Vec2::new(0.0, 4.0),
            Vec2::new(model_scale * 6.4, model_scale * 1.45),
            Color32::from_black_alpha(52),
        ));
        preview_inner.add(Shape::ellipse_filled(
            shadow_center,
            Vec2::new(model_scale * 4.8, model_scale * 0.82),
            Color32::from_black_alpha(30),
        ));
        // 计算当前视角下所有UV面：依据预览区矩形、缩放、偏航(yaw)/俯仰(pitch)角度
        // 以及 Alex 细臂或 Steve 宽臂模型，生成每个可见面片的屏幕多边形与对应皮肤 UV 坐标。
        let preview_faces = skin_preview_faces(
            preview_stage,
            model_scale,
            self.skin_preview_yaw,
            self.skin_preview_pitch,
            self.skin_model_alex,
        );
        // 绘制模型面片
        if let Some(texture) = self.skin_texture.as_ref() {
            paint_skin_model(&preview_inner, texture, &preview_faces);
        }
        // 拾取鼠标悬浮的面片，绘制高亮框
        let hovered_face = if active {
            preview_response
                .hover_pos()
                .and_then(|pointer| skin_face_at(preview_faces.iter().rev(), pointer))
        } else {
            None
        };
        if let Some(face) = hovered_face {
            preview_inner.add(Shape::convex_polygon(
                face.points.to_vec(),
                theme::white_alpha(10),
                Stroke::new(1.0, theme::white_alpha(72)),
            ));
        }
        // 左键直接在3D预览模型上绘制，拾取UV坐标映射回64x64画布像素
        if active
            && (preview_response.clicked_by(egui::PointerButton::Primary)
                || preview_response.dragged_by(egui::PointerButton::Primary))
            && let Some(pointer) = preview_response.interact_pointer_pos()
            && let Some(face) = skin_face_at(preview_faces.iter().rev(), pointer)
            && let Some(uv) = face.texture_position(pointer)
        {
            // 把命中的 UV 坐标（范围 0.0~1.0）换算回 64×64 画布像素坐标：
            // 乘以 64 后向下取整，并用 clamp 把边界值限制在 0~63，防止越界。
            let pixel = (
                (uv.x * 64.0).floor().clamp(0.0, 63.0) as usize,
                (uv.y * 64.0).floor().clamp(0.0, 63.0) as usize,
            );
            self.begin_skin_stroke();
            // 同一面片拖动时画线
            if self.skin_preview_last_face == Some(face.key)
                && let Some(previous) = self.skin_last_pixel
            {
                self.paint_skin_line(previous, pixel);
            } else {
                self.set_skin_pixel(pixel.0, pixel.1);
            }
            self.finish_skin_stroke();
            self.skin_last_pixel = Some(pixel);
            self.skin_preview_last_face = Some(face.key);
            ui.ctx().request_repaint();
        }
    }

    /// 模组工程表单渲染：名称、ID、包名、版本、MC版本、加载器、输出目录、创建按钮
    pub(in crate::app) fn paint_mod_workspace(
        &mut self,
        ui: &mut egui::Ui,
        painter: &Painter,
        body: Rect,
        _time: f64,
        active: bool,
    ) {
        let left = body.left() + 28.0;
        let width = (body.width() - 56.0).min(760.0);
        theme::bold_text(
            painter,
            Pos2::new(left, body.top() + 24.0),
            "模组工程",
            17.0,
            theme::ink(),
        );
        theme::text(
            painter,
            Pos2::new(left, body.top() + 52.0),
            "生成 Java 源码与 Gradle 工程 · Minecraft 目标版本独立设置",
            8.3,
            theme::muted(),
        );
        // 输入表单行：名称 / 模组ID / 包名 / 版本 / MC版本
        let fields = [
            ("模组名称", 0),
            ("模组 ID", 1),
            ("Java 包名", 2),
            ("模组版本", 3),
            ("目标 MC 版本", 4),
        ];
        for (index, (label, kind)) in fields.into_iter().enumerate() {
            let y = body.top() + 82.0 + index as f32 * 48.0;
            theme::text(
                painter,
                Pos2::new(left, y + 7.0),
                label,
                8.0,
                theme::muted(),
            );
            let field =
                Rect::from_min_size(Pos2::new(left + 128.0, y), Vec2::new(width - 128.0, 34.0));
            theme::fill_round(
                painter,
                field,
                theme::corner_radius(),
                Color32::from_rgb(24, 38, 28),
            );
            theme::stroke_round(
                painter,
                field,
                theme::corner_radius(),
                1.0,
                theme::white_alpha(26),
            );
            let target = match kind {
                0 => &mut self.mod_name_input,
                1 => &mut self.mod_id_input,
                2 => &mut self.mod_package_input,
                3 => &mut self.mod_version_input,
                _ => &mut self.mod_minecraft_input,
            };
            if active {
                ui.put(
                    field.shrink2(Vec2::new(10.0, 5.0)),
                    egui::TextEdit::singleline(target)
                        .frame(egui::Frame::NONE)
                        .font(theme::font(8.8))
                        .text_color(theme::adaptive_text_color_at(
                            painter,
                            field.center(),
                            theme::ink(),
                            8.8,
                        )),
                );
            }
        }
        // 加载器单选：Fabric / Forge / NeoForge
        let loader_y = body.top() + 82.0 + fields.len() as f32 * 48.0;
        theme::text(
            painter,
            Pos2::new(left, loader_y + 7.0),
            "加载器",
            8.0,
            theme::muted(),
        );
        for (index, label) in ["Fabric", "Forge", "NeoForge"].into_iter().enumerate() {
            let button = Rect::from_min_size(
                Pos2::new(left + 128.0 + index as f32 * 94.0, loader_y),
                Vec2::new(84.0, 34.0),
            );
            let selected = self.mod_loader_index == index;
            if active
                && settings_button(
                    ui,
                    painter,
                    button,
                    match index {
                        0 => "mod-loader-fabric",
                        1 => "mod-loader-forge",
                        _ => "mod-loader-neoforge",
                    },
                    label,
                    selected,
                )
            {
                self.mod_loader_index = index;
            }
        }
        // 输出目录选择行
        let output_y = loader_y + 52.0;
        let output = Rect::from_min_size(
            Pos2::new(left + 128.0, output_y),
            Vec2::new(width - 238.0, 34.0),
        );
        theme::fill_round(
            painter,
            output,
            theme::corner_radius(),
            Color32::from_rgb(22, 34, 26),
        );
        theme::text(
            painter,
            Pos2::new(output.left() + 10.0, output.top() + 8.0),
            self.mod_output_directory
                .as_ref()
                .map(|path| ellipsize_start(&path.display().to_string(), 60))
                .unwrap_or_else(|| "尚未选择工程保存位置".to_owned()),
            8.0,
            if self.mod_output_directory.is_some() {
                theme::ink_soft()
            } else {
                theme::muted()
            },
        );
        let choose = Rect::from_min_size(
            Pos2::new(output.right() + 8.0, output_y),
            Vec2::new(92.0, 34.0),
        );
        if active
            && settings_button(ui, painter, choose, "mod-choose-output", "选择位置", false)
            && let Some(directory) = pick_project_directory()
        {
            self.mod_output_directory = Some(directory);
        }
        // 创建工程按钮
        let create = Rect::from_min_size(
            Pos2::new(left + 128.0, output_y + 48.0),
            Vec2::new(124.0, 38.0),
        );
        if active && settings_button(ui, painter, create, "mod-create-project", "创建工程", true)
        {
            self.create_mod_project();
        }
        // 状态提示
        if let Some(status) = self.mod_status.as_ref() {
            theme::text(
                painter,
                Pos2::new(left + 264.0, output_y + 57.0),
                fit_text_to_width(painter, status, theme::font(8.0), width - 280.0),
                8.0,
                Color32::from_rgb(169, 205, 165),
            );
        }
    }

    /// 【旧版创造首页】创造总览页面：启动游戏创建世界、存档/资源包/截图快捷卡片
    #[allow(dead_code)]
    pub(in crate::app) fn paint_create(
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
        // 顶部Hero横幅
        let hero = motion.rect(Rect::from_min_max(
            page.left_top(),
            Pos2::new(page.right(), page.top() + 218.0),
        ));
        let create_base = painter;
        theme::glass_panel(
            painter,
            hero,
            theme::corner_radius(),
            theme::GlassSpec::new(Color32::from_rgb(19, 31, 23), theme::GREEN, time, 26.0),
        );
        let hero_content = card_clip(painter, hero);
        let painter = &hero_content;
        // 装饰光晕 + 草方块图标
        theme::soft_orb(
            painter,
            Pos2::new(hero.right() - 170.0, hero.center().y),
            120.0,
            theme::GREEN,
            15,
        );
        paint_block(
            painter,
            Pos2::new(hero.right() - 170.0, hero.center().y + 4.0),
            82.0,
            Color32::from_rgb(132, 218, 108),
            Color32::from_rgb(91, 66, 46),
            Color32::from_rgb(73, 151, 70),
        );
        theme::mono_text(
            painter,
            Pos2::new(hero.left() + 30.0, hero.top() + 30.0),
            "CREATE",
            7.5,
            Color32::from_rgb(139, 196, 133),
        );
        theme::bold_text(
            painter,
            Pos2::new(hero.left() + 30.0, hero.top() + 48.0),
            "创造，从一个新世界开始。",
            27.0,
            theme::ink(),
        );
        theme::wrapped_text(
            painter,
            Pos2::new(hero.left() + 30.0, hero.top() + 92.0),
            "创建新的生存或创造世界，导入已有存档，也可以从资源包和截图开始整理你的创作。",
            9.5,
            theme::ink_soft(),
            (hero.width() * 0.58).max(330.0),
        );
        // 启动游戏按钮
        let create_world = Rect::from_min_size(
            Pos2::new(hero.left() + 30.0, hero.bottom() - 58.0),
            Vec2::new(147.0, 38.0),
        );
        if settings_button(
            ui,
            painter,
            create_world,
            "create-new-world",
            "启动并创建世界",
            true,
        ) {
            self.show_toast("正在启动 Minecraft，请在游戏中选择创建新世界");
            self.launch_recent = false;
            self.start_launch(ctx);
        }

        let painter = create_base;
        let gap = 12.0;
        let card_width = (page.width() - gap * 2.0) / 3.0;
        // 三卡片：存档 / 资源包 / 截图
        let cards = [
            ("导入存档", "将存档放入 saves 目录", "saves"),
            ("资源包", "打开 resourcepacks 目录", "resourcepacks"),
            ("截图画廊", "浏览游戏内截图", "screenshots"),
        ];
        for (index, (title, description, folder)) in cards.into_iter().enumerate() {
            let rect = motion.rect(Rect::from_min_size(
                Pos2::new(
                    page.left() + index as f32 * (card_width + gap),
                    page.top() + 218.0 + 14.0,
                ),
                Vec2::new(card_width, 132.0),
            ));
            let response = ui
                .interact(rect, Id::unique(("create-card", index)), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand);
            theme::glass_panel(
                painter,
                rect,
                theme::corner_radius(),
                theme::GlassSpec::new(
                    Color32::from_rgb(18, 29, 22),
                    theme::GREEN,
                    time,
                    27.0 + index as f32,
                )
                .hovered(response.hovered()),
            );
            let card_content = card_clip(painter, rect);
            let painter = &card_content;
            draw_icon(
                painter,
                match index {
                    0 => NavIcon::Folder,
                    1 => NavIcon::Box,
                    _ => NavIcon::Sparkles,
                },
                Rect::from_center_size(
                    Pos2::new(rect.left() + 32.0, rect.top() + 34.0),
                    Vec2::splat(22.0),
                ),
                theme::GREEN_BRIGHT,
            );
            theme::bold_text(
                painter,
                Pos2::new(rect.left() + 55.0, rect.top() + 22.0),
                title,
                12.5,
                theme::ink(),
            );
            theme::wrapped_text(
                painter,
                Pos2::new(rect.left() + 20.0, rect.top() + 62.0),
                description,
                8.2,
                theme::muted(),
                rect.width() - 40.0,
            );
            theme::text(
                painter,
                Pos2::new(rect.left() + 20.0, rect.bottom() - 27.0),
                "打开目录  →",
                8.0,
                if response.hovered() {
                    theme::GREEN_BRIGHT
                } else {
                    Color32::from_rgb(157, 204, 151)
                },
            );
            // 点击打开对应文件夹
            if response.clicked()
                && let Some(base) = self.version_data_directory()
            {
                let path = base.join(folder);
                let _ = std::fs::create_dir_all(&path);
                open_path(&path);
            }
        }
    }
}
