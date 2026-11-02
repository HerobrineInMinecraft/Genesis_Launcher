//! # `src/favorites/icons.rs`
//!
//! **功能域：** 收藏站点模型、地址规范化、图标抓取和外部链接打开。
//!
//! 获取、验证并缓存站点图标
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `start_fetch_icons`
//! - `open_site`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 实现中包含线程、通道或共享状态；同步边界必须与现有轮询和生命周期保持一致。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 包含平台相关分支；跨平台修改必须同时保持各 ``cfg`` 分支可编译。

use std::io::Read;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use super::model::FavoriteIconRequest;
use super::urls::{normalize_site_url, site_domain};
use crate::brand;

/// 提供 USERAGENT 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const USER_AGENT: &str = brand::USER_AGENT;

/// 实现 启动获取icons 相关的业务流程。
///
/// # 参数
/// - `requests`：由调用方传入的requests。
/// # 返回
/// 返回 `mpsc::Receiver<Vec<(String, Vec<u8>)>>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn start_fetch_icons(
    requests: Vec<FavoriteIconRequest>,
) -> mpsc::Receiver<Vec<(String, Vec<u8>)>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut icons = Vec::new();
        for request in requests {
            if !request.icon_source.trim().is_empty() {
                let custom = if request.icon_source.starts_with("http://")
                    || request.icon_source.starts_with("https://")
                {
                    fetch_bytes(&request.icon_source)
                } else {
                    fetch_local_icon(&request.icon_source)
                };
                if let Ok(bytes) = custom
                    && image::load_from_memory(&bytes).is_ok()
                {
                    icons.push((request.key, bytes));
                    continue;
                }
            }
            let domain = site_domain(&request.site_url);
            if domain.is_empty() {
                continue;
            }
            let candidates = [
                format!("https://unavatar.io/{domain}"),
                format!("https://{domain}/favicon.ico"),
                format!("https://{domain}/apple-touch-icon.png"),
                format!("https://www.google.com/s2/favicons?domain={domain}&sz=128"),
                format!("https://icons.duckduckgo.com/ip3/{domain}.ico"),
            ];
            for candidate in candidates {
                if let Ok(bytes) = fetch_bytes(&candidate)
                    && image::load_from_memory(&bytes).is_ok()
                {
                    icons.push((request.key.clone(), bytes));
                    break;
                }
            }
        }
        let _ = sender.send(icons);
    });
    receiver
}

/// 实现 获取local图标 相关的业务流程。
///
/// # 参数
/// - `path`：由调用方传入的路径。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn fetch_local_icon(path: &str) -> Result<Vec<u8>, String> {
    let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
    if metadata.len() > 4 * 1024 * 1024 {
        return Err("本地图标不能超过 4 MB".to_owned());
    }
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    if bytes.is_empty() {
        Err("图标文件为空".to_owned())
    } else {
        Ok(bytes)
    }
}

/// 实现 打开site 相关的业务流程。
///
/// # 参数
/// - `url`：由调用方传入的地址。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn open_site(url: &str) -> Result<(), String> {
    let url = normalize_site_url(url)?;
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("rundll32.exe")
            .args(["url.dll,FileProtocolHandler", &url])
            .creation_flags(0x0800_0000)
            .spawn()
            .map_err(|error| error.to_string())?;
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}

/// 实现 获取bytes 相关的业务流程。
///
/// # 参数
/// - `url`：由调用方传入的地址。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout_read(Duration::from_secs(10))
        .build();
    let mut bytes = Vec::new();
    agent
        .get(url)
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(|error| error.to_string())?
        .into_reader()
        .take(4 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.is_empty() {
        Err("图标响应为空".to_owned())
    } else {
        Ok(bytes)
    }
}
