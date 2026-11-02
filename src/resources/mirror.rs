//! # `src/resources/mirror.rs`
//!
//! **功能域：** Modrinth 资源搜索、详情解析、镜像测速和内容下载。
//!
//! 处理官方源与国内镜像之间的地址映射和回退
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `MCIM_HOST`
//! - `ModrinthRoute`
//! - `other`
//! - `request_text_with_mirror`
//! - `request_bytes_with_mirror`
//! - `mcim_mirror_url`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件位于外部数据或文件写入边界，不得绕过 URL、路径、哈希和大小限制。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use super::network::{request_bytes, request_text};
use super::route::{preferred_modrinth_route, remember_modrinth_route};

/// 提供 MCIMHOST 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub(super) const MCIM_HOST: &str = "https://mod.mcimirror.top";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// 定义 ModrinthRoute 可选状态或事件集合。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub(super) enum ModrinthRoute {
    Official,
    Mirror,
}

impl ModrinthRoute {
    /// 实现 标签 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `&'static str`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn label(self) -> &'static str {
        match self {
            Self::Official => "Modrinth 官方源",
            Self::Mirror => "MCIM 国内镜像",
        }
    }

    /// 实现 other 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `Self`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn other(self) -> Self {
        match self {
            Self::Official => Self::Mirror,
            Self::Mirror => Self::Official,
        }
    }
}

/// 实现 请求文本with镜像 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn request_text_with_mirror(official_url: &str) -> Result<String, String> {
    let preferred = preferred_modrinth_route();
    let mut errors = Vec::new();
    for route in [preferred, preferred.other()] {
        let Some(url) = routed_url(official_url, route) else {
            continue;
        };
        match request_text(&url) {
            Ok(body) => {
                remember_modrinth_route(route);
                return Ok(body);
            }
            Err(error) => errors.push(format!("{}：{error}", route.label())),
        }
    }
    Err(format!("Modrinth 线路均不可用：{}", errors.join("；")))
}

/// 实现 请求byteswith镜像 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn request_bytes_with_mirror(official_url: &str) -> Result<Vec<u8>, String> {
    let preferred = preferred_modrinth_route();
    let mut errors = Vec::new();
    for route in [preferred, preferred.other()] {
        let Some(url) = routed_url(official_url, route) else {
            continue;
        };
        match request_bytes(&url) {
            Ok(bytes) => {
                remember_modrinth_route(route);
                return Ok(bytes);
            }
            Err(error) => errors.push(format!("{}：{error}", route.label())),
        }
    }
    Err(format!("Modrinth 下载线路均不可用：{}", errors.join("；")))
}

/// 实现 mcim镜像地址 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Option<String>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn mcim_mirror_url(url: &str) -> Option<String> {
    if let Some(path) = url.strip_prefix("https://api.modrinth.com") {
        Some(format!("{MCIM_HOST}/modrinth{path}"))
    } else {
        url.strip_prefix("https://cdn.modrinth.com")
            .map(|path| format!("{MCIM_HOST}{path}"))
    }
}

/// 实现 routed地址 相关的业务流程。
///
/// # 参数
/// - `official_url`：由调用方传入的official地址。
/// - `route`：由调用方传入的线路。
/// # 返回
/// 返回 `Option<String>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn routed_url(official_url: &str, route: ModrinthRoute) -> Option<String> {
    match route {
        ModrinthRoute::Official => Some(official_url.to_owned()),
        ModrinthRoute::Mirror => mcim_mirror_url(official_url),
    }
}
