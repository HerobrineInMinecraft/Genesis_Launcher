//! # `src/resources/cache.rs`
//!
//! **功能域：** Modrinth 资源搜索、详情解析、镜像测速和内容下载。
//!
//! 维护短期缓存、过期清理和离线回退
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `SearchKey`
//! - `cached_search`
//! - `store_search`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 实现中包含线程、通道或共享状态；同步边界必须与现有轮询和生命周期保持一致。
//!
//! ## 安全与平台
//! - 本文件位于外部数据或文件写入边界，不得绕过 URL、路径、哈希和大小限制。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::model::{ResourceCategory, ResourceProject};

/// 提供 缓存TTL 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const CACHE_TTL: Duration = Duration::from_secs(300);
/// 提供 缓存LIMIT 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const CACHE_LIMIT: usize = 48;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
/// 定义 SearchKey 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub(super) struct SearchKey {
    pub(super) category: ResourceCategory,
    pub(super) query: String,
    pub(super) game_version: String,
    pub(super) loader: Option<String>,
}

/// 声明 SearchCache 的类型别名。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
type SearchCache = HashMap<SearchKey, (Instant, Vec<ResourceProject>)>;

/// 实现 cached搜索 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Option<Vec<ResourceProject>>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn cached_search(key: &SearchKey) -> Option<Vec<ResourceProject>> {
    let mut cache = search_cache().lock().ok()?;
    cache.retain(|_, (stored_at, _)| stored_at.elapsed() < CACHE_TTL);
    cache.get(key).map(|(_, results)| results.clone())
}

/// 实现 存储搜索 相关的业务流程。
///
/// # 参数
/// - `results`：由调用方传入的results。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn store_search(key: SearchKey, results: &[ResourceProject]) {
    let Ok(mut cache) = search_cache().lock() else {
        return;
    };
    cache.retain(|_, (stored_at, _)| stored_at.elapsed() < CACHE_TTL);
    if cache.len() >= CACHE_LIMIT
        && let Some(oldest_key) = cache
            .iter()
            .min_by_key(|(_, (stored_at, _))| *stored_at)
            .map(|(key, _)| key.clone())
    {
        cache.remove(&oldest_key);
    }
    cache.insert(key, (Instant::now(), results.to_vec()));
}

/// 实现 搜索缓存 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `&'static Mutex<SearchCache>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn search_cache() -> &'static Mutex<SearchCache> {
    /// 提供 缓存 使用的进程级静态状态。
    ///
    /// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
    static CACHE: OnceLock<Mutex<SearchCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}
