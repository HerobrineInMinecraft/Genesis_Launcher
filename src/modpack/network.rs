//! # `src/modpack/network.rs`
//!
//! **功能域：** Modrinth 整合包读取、依赖补全、文件下载和覆盖层解压。
//!
//! 封装 HTTP 客户端、超时、响应读取和错误转换
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `download_urls`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件位于外部数据或文件写入边界，不得绕过 URL、路径、哈希和大小限制。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::io::Read;
use std::time::Duration;

use super::mirror::mirror_url;

/// 提供 USERAGENT 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const USER_AGENT: &str = crate::brand::USER_AGENT;

/// 实现 下载urls 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn download_urls(urls: &[String]) -> Result<Vec<u8>, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(8))
        .timeout_read(Duration::from_secs(45))
        .build();
    let mut last_error = String::new();
    for url in urls {
        let mirror = mirror_url(url);
        for candidate in mirror.iter().chain(std::iter::once(url)) {
            match agent.get(candidate).set("User-Agent", USER_AGENT).call() {
                Ok(response) => {
                    let mut bytes = Vec::new();
                    if response.into_reader().read_to_end(&mut bytes).is_ok() {
                        return Ok(bytes);
                    }
                }
                Err(error) => last_error = error.to_string(),
            }
        }
    }
    Err(format!("下载整合包文件失败：{last_error}"))
}
