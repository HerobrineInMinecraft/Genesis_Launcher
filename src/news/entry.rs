//! # `src/news/entry.rs`
//!
//! **功能域：** Minecraft 官方动态获取、解析、缓存和图片加载。
//!
//! 解析单条记录并规范化可选字段
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `parse_entry`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use serde_json::Value;
use sha1_smol::Sha1;

use super::image_cache::image_cache_key;
use super::image_url::image_url;
use super::model::MinecraftNews;
use super::text::plain_text;

/// 实现 解析条目 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Option<MinecraftNews>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn parse_entry(entry: &Value) -> Option<MinecraftNews> {
    let title = plain_text(entry.get("title")?.as_str()?);
    if title.is_empty() {
        return None;
    }
    let id = entry
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| Sha1::from(title.as_bytes()).digest().to_string());
    let category = entry
        .get("category")
        .and_then(Value::as_str)
        .or_else(|| {
            entry
                .get("newsType")
                .and_then(Value::as_array)
                .and_then(|types| types.first())
                .and_then(Value::as_str)
        })
        .or_else(|| entry.get("tag").and_then(Value::as_str))
        .map(plain_text)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Minecraft".to_owned());
    let date = entry
        .get("date")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let summary = entry
        .get("text")
        .and_then(Value::as_str)
        .or_else(|| entry.get("description").and_then(Value::as_str))
        .map(plain_text)
        .unwrap_or_default();
    let body = entry
        .get("body")
        .and_then(Value::as_str)
        .or_else(|| entry.get("content").and_then(Value::as_str))
        .map(plain_text)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| summary.clone());
    let article_url = entry
        .get("readMoreLink")
        .and_then(Value::as_str)
        .or_else(|| entry.get("articleUrl").and_then(Value::as_str))
        .map(str::to_owned);
    let image_url = image_url(entry);
    let image_key = image_url.as_ref().map(|url| image_cache_key(url));
    Some(MinecraftNews {
        id,
        title,
        category,
        date,
        summary,
        body,
        article_url,
        image_url,
        image_key,
        image_bytes: None,
    })
}
