//! # `src/resources/mod.rs`
//!
//! **功能域：** Modrinth 资源搜索、详情解析、镜像测速和内容下载。
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
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

mod cache;
mod details;
mod download;
mod mirror;
mod model;
mod network;
mod route;
mod search;
#[cfg(test)]
mod tests;
mod util;

pub use download::{sanitize_download_filename, start_download};
pub use model::{ResourceCategory, ResourceProject};
pub use search::{start_prefetch, start_search};
