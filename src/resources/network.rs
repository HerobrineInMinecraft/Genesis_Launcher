//! # `src/resources/network.rs`
//!
//! **功能域：** Modrinth 资源搜索、详情解析、镜像测速和内容下载。
//!
//! 封装 HTTP 客户端、超时、响应读取和错误转换
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `request_text`
//! - `request_bytes`
//! - `http_agent`
//! - `allowed_url`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 实现中包含线程、通道或共享状态；同步边界必须与现有轮询和生命周期保持一致。
//!
//! ## 安全与平台
//! - 本文件位于外部数据或文件写入边界，不得绕过 URL、路径、哈希和大小限制。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::io::Read;
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

use crate::brand;

/// 提供 USERAGENT 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const USER_AGENT: &str = brand::USER_AGENT;

/// 实现 请求文本 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn request_text(url: &str) -> Result<String, String> {
    request(url)?
        .into_string()
        .map_err(|error| format!("读取下载源响应失败：{error}"))
}

/// 实现 请求bytes 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn request_bytes(url: &str) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    request(url)?
        .into_reader()
        .read_to_end(&mut bytes)
        .map_err(|error| format!("读取下载文件失败：{error}"))?;
    Ok(bytes)
}

/// 实现 httpagent 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `&'static ureq::Agent`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn http_agent() -> &'static ureq::Agent {
    /// 提供 AGENT 使用的进程级静态状态。
    ///
    /// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(6))
            .timeout_read(Duration::from_secs(20))
            .build()
    })
}

/// 实现 allowed地址 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `bool`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn allowed_url(url: &str) -> bool {
    url.starts_with("https://api.modrinth.com/")
        || url.starts_with("https://cdn.modrinth.com/")
        || url.starts_with("https://mod.mcimirror.top/modrinth/")
        || url.starts_with("https://mod.mcimirror.top/data/")
}

/// 实现 请求 相关的业务流程。
///
/// # 参数
/// - `url`：由调用方传入的地址。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn request(url: &str) -> Result<ureq::Response, String> {
    if !allowed_url(url) {
        return Err("拒绝非受支持的资源下载源地址".to_owned());
    }
    let mut last_error = String::new();
    for attempt in 0..3 {
        match http_agent().get(url).set("User-Agent", USER_AGENT).call() {
            Ok(response) => return Ok(response),
            Err(error) => last_error = error.to_string(),
        }
        if attempt < 2 {
            thread::sleep(Duration::from_millis(350 * (1 << attempt)));
        }
    }
    Err(format!("资源下载源请求失败：{last_error}"))
}
