//! # `src/download/mod.rs`
//!
//! **功能域：** Minecraft 官方清单、客户端、资源、Java 运行时安装与完整性校验。
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
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

mod filesystem;
mod install;
mod java;
mod manifest;
mod model;
mod plan;
mod security;
mod transfer;

#[cfg(test)]
mod tests;

pub use install::{start_install, start_install_by_id};
pub use java::{install_java_runtime_for_report, java_runtime_manifest_report};
pub use manifest::{fetch_official_versions, start_manifest_fetch};
pub use model::{InstallUpdate, OfficialVersion};
