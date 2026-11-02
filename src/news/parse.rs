//! # `src/news/parse.rs`
//!
//! **功能域：** Minecraft 官方动态获取、解析、缓存和图片加载。
//!
//! 把原始响应解析为应用内部模型
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `parse_news`
//! - `parse_news_inner`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::path::Path;

use serde_json::Value;

use super::entry::parse_entry;
use super::images::attach_images;
use super::model::MinecraftNews;

/// 实现 解析新闻 相关的业务流程。
///
/// # 参数
/// - `cache`：由调用方传入的缓存。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn parse_news(body: &str, cache: &Path) -> Result<Vec<MinecraftNews>, String> {
    let mut news = parse_news_inner(body)?;
    attach_images(&mut news, cache);
    Ok(news)
}

/// 实现 解析新闻inner 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn parse_news_inner(body: &str) -> Result<Vec<MinecraftNews>, String> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| format!("Minecraft 动态 JSON 格式无效：{error}"))?;
    let entries = value
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(|| "Minecraft 动态缺少 entries".to_owned())?;
    let mut news = entries.iter().filter_map(parse_entry).collect::<Vec<_>>();
    news.sort_by(|left, right| right.date.cmp(&left.date));
    news.dedup_by(|left, right| left.id == right.id);
    news.truncate(12);
    Ok(news)
}
