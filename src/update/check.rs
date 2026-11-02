//! # `src/update/check.rs`
//!
//! **功能域：** Genesis Launcher GitHub Release 检查、下载和程序替换。
//!
//! 发起发布检查并把结果送回 UI
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `start_latest_release_check`
//! - `fetch_latest_release`
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

use super::model::LauncherRelease;
use super::release::{LATEST_RELEASE_PATH, parse_release_body};

/// 实现 启动最新发行版检查 相关的业务流程。
///
/// # 参数
/// - `ctx`：由调用方传入的ctx。
/// # 返回
/// 返回 `mpsc::Receiver<Result<LauncherRelease, String>>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn start_latest_release_check(
    ctx: &egui::Context,
) -> mpsc::Receiver<Result<LauncherRelease, String>> {
    let (sender, receiver) = mpsc::channel();
    let repaint = ctx.clone();
    thread::spawn(move || {
        let result = fetch_latest_release();
        let _ = sender.send(result);
        repaint.request_repaint();
    });
    receiver
}

/// 实现 获取最新发行版 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn fetch_latest_release() -> Result<LauncherRelease, String> {
    #[cfg(target_os = "windows")]
    let body = super::winhttp::latest_release_body(LATEST_RELEASE_PATH)?;
    #[cfg(not(target_os = "windows"))]
    let body = super::github_api::latest_release_body(LATEST_RELEASE_PATH)?;
    parse_release_body(&body)
}
