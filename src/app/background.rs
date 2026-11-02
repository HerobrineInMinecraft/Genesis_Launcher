//! 用户导入的首页背景图片。
//!
//! 背景图默认不存在；导入后复制到启动器数据目录，避免依赖原始文件位置。

use std::path::{Path, PathBuf};

use super::*;

const MAX_BACKGROUND_FILE_BYTES: u64 = 32 * 1024 * 1024;

/// 从磁盘加载用户背景并上传为 egui 纹理。
pub(super) fn load_background_texture(
    ctx: &egui::Context,
    path: &Path,
) -> Option<egui::TextureHandle> {
    let metadata = std::fs::metadata(path).ok()?;
    if metadata.len() > MAX_BACKGROUND_FILE_BYTES || !metadata.is_file() {
        return None;
    }
    let image = image::open(path).ok()?;
    let image = color_image_for_context(ctx, image);
    // 背景纹理上传前同步生成对比度采样表。文字绘制每帧会读取该表，
    // 因此更换或移除背景图后无需重启或额外刷新纹理即可获得新的文字颜色。
    theme::set_background_image(&image);
    Some(ctx.load_texture("user-home-background", image, egui::TextureOptions::LINEAR))
}

impl LauncherApp {
    /// 选择、复制并立即应用一张首页背景图。
    pub(super) fn import_background_image(&mut self, ctx: &egui::Context) {
        let Some(source) = pick_background_image() else {
            return;
        };
        let Ok(metadata) = std::fs::metadata(&source) else {
            self.show_toast("无法读取背景图片");
            return;
        };
        if !metadata.is_file() || metadata.len() > MAX_BACKGROUND_FILE_BYTES {
            self.show_toast("背景图片无效或超过 32 MB");
            return;
        }
        let Ok(image) = image::open(&source) else {
            self.show_toast("不支持的背景图片格式");
            return;
        };
        let directory = brand::data_directory();
        if let Err(error) = std::fs::create_dir_all(&directory) {
            self.show_toast(format!("无法创建背景图片目录：{error}"));
            return;
        }
        let destination = directory.join("background.png");
        if let Err(error) = image.save(&destination) {
            self.show_toast(format!("无法保存背景图片：{error}"));
            return;
        }
        self.settings.background_image = Some(destination.clone());
        self.hero_background_texture = load_background_texture(ctx, &destination);
        self.show_toast("首页背景图片已导入");
        ctx.request_repaint();
    }

    /// 移除自定义背景图并恢复无背景状态。
    pub(super) fn remove_background_image(&mut self) {
        if let Some(path) = self.settings.background_image.take() {
            let managed = brand::data_directory().join("background.png");
            if path == managed {
                let _ = std::fs::remove_file(path);
            }
        }
        self.hero_background_texture = None;
        theme::clear_background_image();
        self.show_toast("已移除首页背景图片");
    }
}

/// 调用 Windows 图片选择器。
fn pick_background_image() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let script = "[Console]::OutputEncoding=[Text.Encoding]::UTF8; Add-Type -AssemblyName System.Windows.Forms; $d=New-Object System.Windows.Forms.OpenFileDialog; $d.Title='选择首页背景图片'; $d.Filter='图片文件 (*.png;*.jpg;*.jpeg;*.webp)|*.png;*.jpg;*.jpeg;*.webp|所有文件 (*.*)|*.*'; $d.Multiselect=$false; if($d.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK){[Console]::Out.Write($d.FileName)}";
        let mut command = std::process::Command::new("powershell.exe");
        command
            .args(["-NoProfile", "-STA", "-Command", script])
            .creation_flags(0x0800_0000);
        let output = command.output().ok()?;
        let selected = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        (!selected.is_empty()).then(|| PathBuf::from(selected))
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}
