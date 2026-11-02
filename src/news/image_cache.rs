//! # `src/news/image_cache.rs`
//!
//! **功能域：** Minecraft 官方动态获取、解析、缓存和图片加载。
//!
//! 以稳定键读写本地图片缓存
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `image_cache_key`
//! - `load_cached_image`
//! - `store_cached_image`
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

use sha1_smol::Sha1;

/// 提供 MAX图片BYTES 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const MAX_IMAGE_BYTES: usize = 12 * 1024 * 1024;

/// 实现 图片缓存key 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `String`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn image_cache_key(url: &str) -> String {
    Sha1::from(url.as_bytes()).digest().to_string()
}

/// 实现 加载cached图片 相关的业务流程。
///
/// # 参数
/// - `cache`：由调用方传入的缓存。
/// # 返回
/// 返回 `Option<Vec<u8>>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn load_cached_image(url: &str, cache: &Path) -> Option<Vec<u8>> {
    let bytes = fs::read(image_cache_path(url, cache)).ok()?;
    (!bytes.is_empty() && bytes.len() <= MAX_IMAGE_BYTES).then_some(bytes)
}

/// 实现 存储cached图片 相关的业务流程。
///
/// # 参数
/// - `cache`：由调用方传入的缓存。
/// - `bytes`：由调用方传入的bytes。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn store_cached_image(url: &str, cache: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::write(image_cache_path(url, cache), bytes)
        .map_err(|error| format!("缓存动态图片失败：{error}"))
}

/// 实现 图片缓存路径 相关的业务流程。
///
/// # 参数
/// - `url`：由调用方传入的地址。
/// - `cache`：由调用方传入的缓存。
/// # 返回
/// 返回 `PathBuf`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn image_cache_path(url: &str, cache: &Path) -> PathBuf {
    cache.join(format!("{}.img", image_cache_key(url)))
}
