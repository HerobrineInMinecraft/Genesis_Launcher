//! # `src/news/cache.rs`
//!
//! **功能域：** Minecraft 官方动态获取、解析、缓存和图片加载。
//!
//! 维护短期缓存、过期清理和离线回退
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `NEWS_URL`
//! - `fetch_json_with_etag`
//! - `cache_directory`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件位于外部数据或文件写入边界，不得绕过 URL、路径、哈希和大小限制。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::brand;

/// 提供 新闻地址 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub(super) const NEWS_URL: &str = "https://launchercontent.mojang.com/v2/news.json";
/// 提供 USERAGENT 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const USER_AGENT: &str = brand::USER_AGENT;

/// 实现 获取jsonwithetag 相关的业务流程。
///
/// # 参数
/// - `cache_path`：由调用方传入的缓存路径。
/// - `etag_path`：由调用方传入的etag路径。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn fetch_json_with_etag(
    url: &str,
    cache_path: &Path,
    etag_path: &Path,
) -> Result<String, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(30))
        .build();
    let etag = fs::read_to_string(etag_path).ok();
    let mut request = agent.get(url).set("User-Agent", USER_AGENT);
    if let Some(etag) = &etag {
        request = request.set("If-None-Match", etag.trim());
    }
    match request.call() {
        Ok(response) => {
            let new_etag = response.header("etag").map(str::to_owned);
            let body = response
                .into_string()
                .map_err(|error| format!("读取 Minecraft 动态失败：{error}"))?;
            if let Some(parent) = cache_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("无法创建动态缓存目录：{error}"))?;
            }
            fs::write(cache_path, &body)
                .map_err(|error| format!("缓存 Minecraft 动态失败：{error}"))?;
            if let Some(etag) = new_etag {
                let _ = fs::write(etag_path, etag);
            }
            Ok(body)
        }
        Err(ureq::Error::Status(304, _)) => {
            fs::read_to_string(cache_path).map_err(|error| format!("读取动态缓存失败：{error}"))
        }
        Err(error) => fs::read_to_string(cache_path)
            .ok()
            .filter(|body| !body.trim().is_empty())
            .ok_or_else(|| format!("请求 Minecraft 动态失败：{error}")),
    }
}

/// 实现 缓存目录 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `PathBuf`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn cache_directory() -> PathBuf {
    brand::data_directory().join("cache").join("minecraft-news")
}
