//! # `src/resources/search.rs`
//!
//! **功能域：** Modrinth 资源搜索、详情解析、镜像测速和内容下载。
//!
//! 构造查询、执行检索并解析结果
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `start_search`
//! - `start_prefetch`
//! - `search_facets`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 实现中包含线程、通道或共享状态；同步边界必须与现有轮询和生命周期保持一致。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::sync::mpsc;
use std::thread;

use serde_json::Value;

use super::cache::{SearchKey, cached_search, store_search};
use super::mirror::request_text_with_mirror;
use super::model::{ResourceCategory, ResourceProject};
use super::util::{escape_query, string_array};

/// 实现 启动搜索 相关的业务流程。
///
/// # 参数
/// - `category`：由调用方传入的category。
/// - `query`：由调用方传入的query。
/// - `game_version`：由调用方传入的游戏版本。
/// - `loader`：由调用方传入的加载器。
/// # 返回
/// 返回 `mpsc::Receiver<Result<Vec<ResourceProject>, String>>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn start_search(
    category: ResourceCategory,
    query: String,
    game_version: String,
    loader: Option<String>,
) -> mpsc::Receiver<Result<Vec<ResourceProject>, String>> {
    let (sender, receiver) = mpsc::channel();
    let key = SearchKey {
        category,
        query: query.clone(),
        game_version: game_version.clone(),
        loader: loader.clone(),
    };
    if let Some(results) = cached_search(&key) {
        let _ = sender.send(Ok(results));
        return receiver;
    }
    thread::spawn(move || {
        if let Some(results) = cached_search(&key) {
            let _ = sender.send(Ok(results));
            return;
        }
        let result = search(category, &query, &game_version, loader.as_deref());
        if let Ok(results) = &result {
            store_search(key, results);
        }
        let _ = sender.send(result);
    });
    receiver
}

/// 实现 启动prefetch 相关的业务流程。
///
/// # 参数
/// - `excluded`：由调用方传入的excluded。
/// - `game_version`：由调用方传入的游戏版本。
/// - `loader`：由调用方传入的加载器。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn start_prefetch(excluded: ResourceCategory, game_version: String, loader: Option<String>) {
    thread::spawn(move || {
        for category in ResourceCategory::ALL {
            if category == excluded {
                continue;
            }
            let key = SearchKey {
                category,
                query: String::new(),
                game_version: game_version.clone(),
                loader: loader.clone(),
            };
            if cached_search(&key).is_some() {
                continue;
            }
            if let Ok(results) = search(category, "", &game_version, loader.as_deref()) {
                store_search(key, &results);
            }
        }
    });
}

/// 实现 搜索 相关的业务流程。
///
/// # 参数
/// - `category`：由调用方传入的category。
/// - `query`：由调用方传入的query。
/// - `game_version`：由调用方传入的游戏版本。
/// - `loader`：由调用方传入的加载器。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn search(
    category: ResourceCategory,
    query: &str,
    game_version: &str,
    loader: Option<&str>,
) -> Result<Vec<ResourceProject>, String> {
    let facets = search_facets(category, game_version, loader);
    let url = format!(
        "https://api.modrinth.com/v2/search?query={}&limit=20&index=relevance&facets={}",
        escape_query(query),
        escape_query(&facets)
    );
    let body = request_text_with_mirror(&url)?;
    let value: Value = serde_json::from_str(&body)
        .map_err(|error| format!("Modrinth 搜索结果格式无效：{error}"))?;
    let hits = value
        .get("hits")
        .and_then(Value::as_array)
        .ok_or_else(|| "Modrinth 搜索结果缺少 hits".to_owned())?;
    Ok(hits
        .iter()
        .filter_map(|hit| {
            Some(ResourceProject {
                id: hit.get("project_id")?.as_str()?.to_owned(),
                title: hit.get("title")?.as_str()?.to_owned(),
                description: hit
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                author: hit
                    .get("author")
                    .and_then(Value::as_str)
                    .unwrap_or("未知作者")
                    .to_owned(),
                downloads: hit.get("downloads").and_then(Value::as_u64).unwrap_or(0),
                categories: string_array(hit.get("categories")),
                game_versions: string_array(hit.get("versions")),
            })
        })
        .collect())
}

/// 实现 搜索facets 相关的业务流程。
///
/// # 参数
/// - `game_version`：由调用方传入的游戏版本。
/// - `loader`：由调用方传入的加载器。
/// # 返回
/// 返回 `String`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn search_facets(
    category: ResourceCategory,
    game_version: &str,
    loader: Option<&str>,
) -> String {
    let mut facet_groups = vec![
        format!("\"project_type:{}\"", category.project_type()),
        format!("\"versions:{}\"", escape_query(game_version)),
    ];
    if let Some(loader) = loader
        && matches!(category, ResourceCategory::Mod | ResourceCategory::Modpack)
    {
        facet_groups.push(format!(
            "\"categories:{}\"",
            escape_query(&loader.to_lowercase())
        ));
    }
    format!(
        "[{}]",
        facet_groups
            .iter()
            .map(|facet| format!("[{facet}]"))
            .collect::<Vec<_>>()
            .join(",")
    )
}
