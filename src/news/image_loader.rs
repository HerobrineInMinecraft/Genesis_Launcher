//! # `src/news/image_loader.rs`
//!
//! **功能域：** Minecraft 官方动态获取、解析、缓存和图片加载。
//!
//! 下载图片、限制大小并接入缓存
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `load_image`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::io::Read;
use std::path::Path;
use std::time::Duration;

use crate::brand;

use super::image_cache::{load_cached_image, store_cached_image};
use super::security::allowed_url;

/// 提供 USERAGENT 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const USER_AGENT: &str = brand::USER_AGENT;
/// 提供 MAX图片BYTES 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const MAX_IMAGE_BYTES: usize = 12 * 1024 * 1024;

/// 实现 加载图片 相关的业务流程。
///
/// # 参数
/// - `cache`：由调用方传入的缓存。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn load_image(url: &str, cache: &Path) -> Result<Vec<u8>, String> {
    if !allowed_url(url) {
        return Err("拒绝非 Mojang 官方动态图片".to_owned());
    }
    if let Some(bytes) = load_cached_image(url, cache) {
        return Ok(bytes);
    }
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(8))
        .timeout_read(Duration::from_secs(18))
        .build();
    let response = agent
        .get(url)
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(|error| format!("下载动态图片失败：{error}"))?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut bytes)
        .map_err(|error| format!("读取动态图片失败：{error}"))?;
    if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES {
        return Err("Minecraft 动态图片大小异常".to_owned());
    }
    store_cached_image(url, cache, &bytes)?;
    Ok(bytes)
}
