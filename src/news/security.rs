//! # `src/news/security.rs`
//!
//! **功能域：** Minecraft 官方动态获取、解析、缓存和图片加载。
//!
//! 执行域名、哈希和路径边界校验
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `allowed_url`
//! - `allowed_article_url`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件位于外部数据或文件写入边界，不得绕过 URL、路径、哈希和大小限制。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

/// 实现 allowed地址 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `bool`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn allowed_url(url: &str) -> bool {
    [
        "https://launchercontent.mojang.com/",
        "https://www.minecraft.net/",
        "https://minecraft.net/",
    ]
    .iter()
    .any(|prefix| url.starts_with(prefix))
}

/// 实现 allowedarticle地址 相关的业务流程。
///
/// # 参数
/// - `url`：由调用方传入的地址。
/// # 返回
/// 返回 `bool`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn allowed_article_url(url: &str) -> bool {
    [
        "https://www.minecraft.net/",
        "https://minecraft.net/",
        "https://www.mojang.com/",
        "https://launchercontent.mojang.com/",
    ]
    .iter()
    .any(|prefix| url.starts_with(prefix))
}
