//! # `src/update/mod.rs`
//!
//! **功能域：** Genesis Launcher GitHub Release 检查、下载和程序替换。
//!
//! 组织本模块声明并导出稳定公共接口
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - 本文件主要为同模块内部实现，不导出额外公共符号。
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件位于外部数据或文件写入边界，不得绕过 URL、路径、哈希和大小限制。
//! - 包含平台相关分支；跨平台修改必须同时保持各 ``cfg`` 分支可编译。

mod check;
mod download;
#[cfg(not(target_os = "windows"))]
mod github_api;
mod installer;
mod model;
mod release;
#[cfg(test)]
mod tests;
mod version;
#[cfg(target_os = "windows")]
mod winhttp;

pub use check::{fetch_latest_release, start_latest_release_check};
pub use download::{download_release_asset_for_report, start_update_download};
pub use installer::launch_update_installer;
#[allow(unused_imports)]
pub use model::ReleaseAsset;
pub use model::{ForcedUpdateEvent, LauncherRelease};
pub use release::RELEASES_URL;
pub use version::is_newer_version;
