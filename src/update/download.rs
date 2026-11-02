//! # `src/update/download.rs`
//!
//! **功能域：** Genesis Launcher GitHub Release 检查、下载和程序替换。
//!
//! 下载文件、校验内容并提交到目标位置
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `start_update_download`
//! - `download_release_asset_for_report`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 实现中包含线程、通道或共享状态；同步边界必须与现有轮询和生命周期保持一致。
//!
//! ## 安全与平台
//! - 本文件位于外部数据或文件写入边界，不得绕过 URL、路径、哈希和大小限制。
//! - 包含平台相关分支；跨平台修改必须同时保持各 ``cfg`` 分支可编译。

use std::sync::mpsc;
use std::thread;

use eframe::egui;

use crate::brand;

use super::model::{ForcedUpdateEvent, ReleaseAsset};

/// 实现 启动更新下载 相关的业务流程。
///
/// # 参数
/// - `asset`：由调用方传入的asset。
/// - `ctx`：由调用方传入的ctx。
/// # 返回
/// 返回 `mpsc::Receiver<ForcedUpdateEvent>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn start_update_download(
    asset: ReleaseAsset,
    ctx: &egui::Context,
) -> mpsc::Receiver<ForcedUpdateEvent> {
    let (sender, receiver) = mpsc::channel();
    let repaint = ctx.clone();
    thread::spawn(move || {
        let progress_sender = sender.clone();
        let progress_repaint = repaint.clone();
        let result = download_release_asset(&asset, move |downloaded, total| {
            let _ = progress_sender.send(ForcedUpdateEvent::Progress { downloaded, total });
            progress_repaint.request_repaint();
        })
        .and_then(|bytes| {
            let directory = brand::data_directory().join("updates");
            std::fs::create_dir_all(&directory)
                .map_err(|error| format!("无法创建更新目录：{error}"))?;
            let path = directory.join(format!(
                "Genesis-Launcher-{}-{}.exe",
                asset.version_suffix(),
                std::process::id()
            ));
            std::fs::write(&path, bytes).map_err(|error| format!("无法保存更新文件：{error}"))?;
            Ok(path)
        });
        let _ = sender.send(ForcedUpdateEvent::Finished(result));
        repaint.request_repaint();
    });
    receiver
}

/// 实现 下载发行版assetforreport 相关的业务流程。
///
/// # 参数
/// - `asset`：由调用方传入的asset。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn download_release_asset_for_report(
    asset: &ReleaseAsset,
) -> Result<std::path::PathBuf, String> {
    let bytes = download_release_asset(asset, |_, _| {})?;
    let path = std::env::temp_dir().join(format!(
        "genesis-launcher-update-{}.exe",
        std::process::id()
    ));
    std::fs::write(&path, bytes).map_err(|error| format!("无法保存测试更新文件：{error}"))?;
    Ok(path)
}

/// 实现 下载发行版asset 相关的业务流程。
///
/// # 参数
/// - `asset`：由调用方传入的asset。
/// - `on_progress`：由调用方传入的进度。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn download_release_asset(
    asset: &ReleaseAsset,
    on_progress: impl FnMut(u64, u64),
) -> Result<Vec<u8>, String> {
    #[cfg(target_os = "windows")]
    {
        super::winhttp::download_release_asset(asset, on_progress)
    }
    #[cfg(not(target_os = "windows"))]
    {
        super::github_api::download_release_asset(asset, on_progress)
    }
}
