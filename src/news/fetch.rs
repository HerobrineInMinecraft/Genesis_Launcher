//! # `src/news/fetch.rs`
//!
//! **功能域：** Minecraft 官方动态获取、解析、缓存和图片加载。
//!
//! 提供同步和异步数据获取入口
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `start_fetch`
//! - `fetch_latest_news`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 实现中包含线程、通道或共享状态；同步边界必须与现有轮询和生命周期保持一致。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::fs;
use std::sync::mpsc;
use std::thread;

use super::cache::{NEWS_URL, cache_directory, fetch_json_with_etag};
use super::model::MinecraftNews;
use super::parse::parse_news;

/// 实现 启动获取 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `mpsc::Receiver<Result<Vec<MinecraftNews>, String>>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn start_fetch() -> mpsc::Receiver<Result<Vec<MinecraftNews>, String>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(fetch_latest_news());
    });
    receiver
}

/// 实现 获取最新新闻 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn fetch_latest_news() -> Result<Vec<MinecraftNews>, String> {
    let root = cache_directory();
    fs::create_dir_all(&root).map_err(|error| format!("无法创建动态缓存目录：{error}"))?;
    let body = fetch_json_with_etag(NEWS_URL, &root.join("news.json"), &root.join("news.etag"))?;
    parse_news(&body, &root)
}
