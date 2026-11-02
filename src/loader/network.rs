//! # `src/loader/network.rs`
//!
//! **功能域：** Fabric、Forge 和 NeoForge 的版本查询、下载与安装。
//!
//! 封装 HTTP 客户端、超时、响应读取和错误转换
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `download_verified`
//! - `fetch_text`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 实现中包含线程、通道或共享状态；同步边界必须与现有轮询和生命周期保持一致。
//!
//! ## 安全与平台
//! - 本文件位于外部数据或文件写入边界，不得绕过 URL、路径、哈希和大小限制。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use super::util::{ensure_allowed_loader_url, request_once, request_with_retry, write_atomic};
use super::*;
use sha1_smol::Sha1;
use std::fs;
use std::io::Read;
use std::thread;
use std::time::Duration;

/// 实现 下载verified 相关的业务流程。
///
/// # 参数
/// - `target`：由调用方传入的target。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn download_verified(url: &str, target: &Path) -> Result<(), String> {
    ensure_allowed_loader_url(url)?;
    let mut last_error = String::new();
    for attempt in 0..3 {
        let result = (|| -> Result<(), String> {
            let sha_url = format!("{url}.sha1");
            let expected = fetch_text(&sha_url)?
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_owned();
            if expected.len() != 40 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(format!("SHA1 无效：{url}"));
            }
            let response = request_once(url)?;
            let mut bytes = Vec::new();
            response
                .into_reader()
                .read_to_end(&mut bytes)
                .map_err(|error| format!("读取下载文件失败：{error}"))?;
            let actual = Sha1::from(&bytes).digest().to_string();
            if !actual.eq_ignore_ascii_case(&expected) {
                return Err(format!("SHA1 校验失败：{url}"));
            }
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|error| format!("无法创建下载目录：{error}"))?;
            }
            write_atomic(target, &bytes)
        })();
        match result {
            Ok(()) => return Ok(()),
            Err(error) => last_error = error,
        }
        if attempt < 2 {
            thread::sleep(Duration::from_millis(500 * (1 << attempt)));
        }
    }
    Err(last_error)
}

/// 实现 获取文本 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn fetch_text(url: &str) -> Result<String, String> {
    ensure_allowed_loader_url(url)?;
    request_with_retry(url)?
        .into_string()
        .map_err(|error| format!("读取网络响应失败：{error}"))
}
