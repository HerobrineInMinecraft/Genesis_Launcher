//! # `src/exporter/modrinth.rs`
//!
//! **功能域：** 版本和整合包导出、文件收集、哈希计算与压缩包生成。
//!
//! 查询 Modrinth 托管信息并生成整合包清单
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `query_hosted_files`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 实现中包含线程、通道或共享状态；同步边界必须与现有轮询和生命周期保持一致。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::collections::HashMap;
use std::time::Duration;

use crate::brand;

use super::model::{CollectedPackFile, HostedPackFile};

/// 提供 MODRINTHAPI 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const MODRINTH_API: &str = "https://api.modrinth.com/v2/version_files";
/// 提供 MCIMAPI 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const MCIM_API: &str = "https://mod.mcimirror.top/modrinth/v2/version_files";

/// 实现 queryhosted文件 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `HashMap<String, HostedPackFile>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn query_hosted_files(files: &[CollectedPackFile]) -> HashMap<String, HostedPackFile> {
    let mut hashes = files
        .iter()
        .filter(|file| file.queryable)
        .filter_map(|file| file.sha1.as_ref())
        .cloned()
        .collect::<Vec<_>>();
    hashes.sort();
    hashes.dedup();
    let mut hosted = HashMap::new();
    for chunk in hashes.chunks(128) {
        let Ok(value) = request_version_files(chunk) else {
            continue;
        };
        let Some(object) = value.as_object() else {
            continue;
        };
        for (sha1, version) in object {
            let Some(candidates) = version.get("files").and_then(serde_json::Value::as_array)
            else {
                continue;
            };
            let Some(file) = candidates.iter().find(|file| {
                file.pointer("/hashes/sha1")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|value| value.eq_ignore_ascii_case(sha1))
            }) else {
                continue;
            };
            let Some(url) = file.get("url").and_then(serde_json::Value::as_str) else {
                continue;
            };
            if !trusted_download_url(url) {
                continue;
            }
            hosted.insert(
                sha1.to_ascii_lowercase(),
                HostedPackFile {
                    downloads: vec![url.to_owned()],
                },
            );
        }
    }
    hosted
}

/// 实现 请求版本文件 相关的业务流程。
///
/// # 参数
/// - `hashes`：由调用方传入的hashes。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn request_version_files(hashes: &[String]) -> Result<serde_json::Value, String> {
    let payload = serde_json::json!({
        "hashes": hashes,
        "algorithm": "sha1",
    })
    .to_string();
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(8))
        .timeout_read(Duration::from_secs(30))
        .build();
    let mut last_error = String::new();
    for url in [MODRINTH_API, MCIM_API] {
        for attempt in 0..2 {
            match agent
                .post(url)
                .set("User-Agent", brand::USER_AGENT)
                .set("Content-Type", "application/json")
                .send_string(&payload)
            {
                Ok(response) => {
                    let body = response
                        .into_string()
                        .map_err(|error| format!("读取 Modrinth 文件信息失败：{error}"))?;
                    return serde_json::from_str(&body)
                        .map_err(|error| format!("Modrinth 文件信息格式无效：{error}"));
                }
                Err(error) => last_error = error.to_string(),
            }
            if attempt == 0 {
                std::thread::sleep(Duration::from_millis(300));
            }
        }
    }
    Err(format!("无法连接 Modrinth 文件接口：{last_error}"))
}

/// 实现 trusted下载地址 相关的业务流程。
///
/// # 参数
/// - `url`：由调用方传入的地址。
/// # 返回
/// 返回 `bool`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn trusted_download_url(url: &str) -> bool {
    url.starts_with("https://cdn.modrinth.com/") || url.starts_with("https://cdn-raw.modrinth.com/")
}
