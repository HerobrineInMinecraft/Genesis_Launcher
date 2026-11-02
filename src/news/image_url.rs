//! # `src/news/image_url.rs`
//!
//! **功能域：** Minecraft 官方动态获取、解析、缓存和图片加载。
//!
//! 提取和规范化图片地址
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `image_url`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use serde_json::Value;

/// 提供 内容HOST 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const CONTENT_HOST: &str = "https://launchercontent.mojang.com";

/// 实现 图片地址 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Option<String>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn image_url(entry: &Value) -> Option<String> {
    for pointer in [
        "/newsPageImage/url",
        "/playPageImage/url",
        "/image/url",
        "/imageUrl",
    ] {
        if let Some(url) = entry.pointer(pointer).and_then(Value::as_str) {
            return Some(if url.starts_with('/') {
                format!("{CONTENT_HOST}{url}")
            } else {
                url.to_owned()
            });
        }
    }
    None
}
