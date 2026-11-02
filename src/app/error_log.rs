//! 下载错误日志。
//!
//! 把下载、安装和导入流程中的完整错误写入本地文件，并通过统一的模态卡片
//! 提供滚动阅读、复制和打开日志文件的操作。

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

/// 最近一次下载错误的完整上下文。
pub(super) struct DownloadErrorLog {
    source: String,
    details: String,
    saved_path: Option<PathBuf>,
    scroll: f32,
}

impl LauncherApp {
    /// 保存并显示一次下载错误。
    ///
    /// `source` 表示失败环节；`details` 保留底层错误、URL、HTTP 状态或文件校验结果。
    /// 即使日志文件创建失败，界面仍会保留并展示原始错误文本。
    pub(super) fn report_download_error(
        &mut self,
        source: impl Into<String>,
        details: impl Into<String>,
    ) {
        let source = source.into();
        let details = details.into();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let contents = format!(
            "Genesis Launcher download error\nversion={}\ntime_unix_ms={}\nplatform={} {}\nsource={}\n\n{}\n",
            env!("CARGO_PKG_VERSION"),
            timestamp.as_millis(),
            std::env::consts::OS,
            std::env::consts::ARCH,
            source,
            details
        );
        let directory = brand::data_directory().join("logs").join("download-errors");
        let saved_path = std::fs::create_dir_all(&directory).ok().and_then(|()| {
            let path = directory.join(format!(
                "download-error-{}-{}.log",
                timestamp.as_millis(),
                std::process::id()
            ));
            std::fs::write(&path, contents.as_bytes())
                .ok()
                .map(|()| path)
        });
        self.download_error_log = Some(DownloadErrorLog {
            source: source.clone(),
            details,
            saved_path,
            scroll: 0.0,
        });
        self.show_toast(format!("{source}失败，已显示错误日志"));
    }

    /// 绘制下载错误日志模态卡片。
    pub(super) fn paint_download_error_log(
        &mut self,
        ctx: &egui::Context,
        screen: Rect,
        time: f64,
    ) {
        let Some(log) = self.download_error_log.as_ref() else {
            return;
        };
        let source = log.source.clone();
        let details = log.details.clone();
        let saved_path = log.saved_path.clone();
        let mut scroll = log.scroll;
        let desired_size = Vec2::new(
            820.0_f32.min(screen.width() - 34.0),
            560.0_f32.min(screen.height() - 38.0),
        );
        let mut copy_requested = false;
        let mut open_requested = false;
        let mut close_requested = false;

        // `Modal` 会在整个窗口上建立可交互背景层，并自动阻止点击、拖动和滚轮
        // 穿透到后面的页面、菜单或其他弹窗。
        egui::Modal::new(Id::unique("download-error-log-modal"))
            .frame(egui::Frame::NONE)
            .backdrop_color(theme::black_alpha(128))
            .show(ctx, |modal_ui| {
                let (dialog, _) =
                    modal_ui.allocate_exact_size(desired_size, Sense::click_and_drag());
                let painter = modal_ui.painter().clone();
                theme::glass_panel(
                    &painter,
                    dialog,
                    theme::corner_radius(),
                    theme::GlassSpec::new(
                        Color32::from_rgb(27, 35, 29),
                        Color32::from_rgb(226, 128, 96),
                        time,
                        38.0,
                    )
                    .backdrop_blur(18.0),
                );
                let content = card_clip(&painter, dialog);
                theme::bold_text(
                    &content,
                    Pos2::new(dialog.left() + 22.0, dialog.top() + 18.0),
                    format!("下载错误 · {source} · LOG"),
                    14.0,
                    Color32::from_rgb(244, 226, 216),
                );
                let saved_text = saved_path
                    .as_ref()
                    .map(|path| format!("日志文件：{}", path.display()))
                    .unwrap_or_else(|| "日志文件保存失败，下方仍保留完整错误内容".to_owned());
                theme::text(
                    &content,
                    Pos2::new(dialog.left() + 22.0, dialog.top() + 42.0),
                    fit_text_to_width(
                        &content,
                        &saved_text,
                        theme::font(7.5),
                        dialog.width() - 44.0,
                    ),
                    7.5,
                    theme::muted(),
                );

                let body = Rect::from_min_max(
                    Pos2::new(dialog.left() + 22.0, dialog.top() + 65.0),
                    Pos2::new(dialog.right() - 22.0, dialog.bottom() - 64.0),
                );
                theme::fill_round(
                    &content,
                    body,
                    theme::corner_radius(),
                    Color32::from_rgb(15, 21, 17),
                );
                theme::stroke_round(
                    &content,
                    body,
                    theme::corner_radius(),
                    1.0,
                    theme::white_alpha(28),
                );
                let text_rect = body.shrink(13.0);
                let text_painter = card_clip(&content, text_rect);
                let galley = text_painter.layout(
                    details.clone(),
                    theme::mono_font(8.0),
                    Color32::from_rgb(220, 229, 220),
                    text_rect.width(),
                );
                let content_height = galley.size().y;
                let max_scroll = (content_height - text_rect.height()).max(0.0);
                if modal_ui
                    .interact(body, Id::unique("download-error-log-body"), Sense::hover())
                    .hovered()
                {
                    let delta = ctx.input(|input| input.smooth_scroll_delta.y);
                    if delta.abs() > 0.0 {
                        scroll = (scroll - delta).clamp(0.0, max_scroll);
                        ctx.request_repaint();
                    }
                }
                scroll = scroll.clamp(0.0, max_scroll);
                let log_color = theme::adaptive_text_color_at(
                    &text_painter,
                    text_rect.center(),
                    Color32::from_rgb(220, 229, 220),
                    8.0,
                );
                text_painter.galley(
                    text_rect.left_top() - Vec2::new(0.0, scroll),
                    galley,
                    log_color,
                );
                paint_list_scrollbar(
                    &text_painter,
                    body.right() - 3.0,
                    body.top() + 6.0,
                    body.height() - 12.0,
                    content_height,
                    scroll,
                    max_scroll,
                );

                let button_y = dialog.bottom() - 43.0;
                let copy_rect = Rect::from_min_size(
                    Pos2::new(dialog.left() + 22.0, button_y),
                    Vec2::new(96.0, 29.0),
                );
                copy_requested = settings_button(
                    modal_ui,
                    &content,
                    copy_rect,
                    "download-error-copy",
                    "复制 LOG",
                    false,
                );
                if saved_path.is_some() {
                    let open_rect = Rect::from_min_size(
                        Pos2::new(copy_rect.right() + 8.0, button_y),
                        Vec2::new(112.0, 29.0),
                    );
                    open_requested = settings_button(
                        modal_ui,
                        &content,
                        open_rect,
                        "download-error-open-file",
                        "打开日志文件",
                        false,
                    );
                }
                let close_rect = Rect::from_min_size(
                    Pos2::new(dialog.right() - 104.0, button_y),
                    Vec2::new(82.0, 29.0),
                );
                close_requested = settings_button(
                    modal_ui,
                    &content,
                    close_rect,
                    "download-error-close",
                    "关闭",
                    true,
                );
            });

        if close_requested {
            self.download_error_log = None;
            return;
        }
        if let Some(log) = self.download_error_log.as_mut() {
            log.scroll = scroll;
        }
        if copy_requested {
            ctx.copy_text(details);
            self.show_toast("错误日志已复制");
        }
        if open_requested && let Some(path) = saved_path {
            open_path(&path);
        }
    }
}
