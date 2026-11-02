//! # `src/download/manifest.rs`
//!
//! **功能域：** Minecraft 官方清单、客户端、资源、Java 运行时安装与完整性校验。
//!
//! 读取远端清单并维护本地缓存
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `start_manifest_fetch`
//! - `fetch_official_versions`
//! - `fetch_cached_text`
//! - `parse_manifest`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 实现中包含线程、通道或共享状态；同步边界必须与现有轮询和生命周期保持一致。
//!
//! ## 安全与平台
//! - 本文件位于外部数据或文件写入边界，不得绕过 URL、路径、哈希和大小限制。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::fs;
use std::path::Path;
use std::sync::mpsc;
use std::thread;

use serde_json::Value;

use super::filesystem::{part_path, replace_file};
use super::model::OfficialVersion;
use super::security::ensure_allowed_url;
use super::transfer::{USER_AGENT, agent, backoff, cache_directory};

/// 提供 MANIFEST地址 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

/// 实现 启动manifest获取 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `mpsc::Receiver<Result<Vec<OfficialVersion>, String>>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn start_manifest_fetch() -> mpsc::Receiver<Result<Vec<OfficialVersion>, String>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(fetch_official_versions());
    });
    receiver
}

/// 实现 获取official版本 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn fetch_official_versions() -> Result<Vec<OfficialVersion>, String> {
    let cache_path = cache_directory().join("version_manifest_v2.json");
    let etag_path = cache_directory().join("version_manifest_v2.etag");
    let body = match fetch_cached_text(MANIFEST_URL, &cache_path, &etag_path, "版本清单") {
        Ok(body) => body,
        Err(network_error) => fs::read_to_string(&cache_path)
            .ok()
            .filter(|body| !body.trim().is_empty())
            .ok_or(network_error)?,
    };
    parse_manifest(&body)
}

/// 实现 获取cached文本 相关的业务流程。
///
/// # 参数
/// - `cache_path`：由调用方传入的缓存路径。
/// - `etag_path`：由调用方传入的etag路径。
/// - `label`：由调用方传入的标签。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn fetch_cached_text(
    url: &str,
    cache_path: &Path,
    etag_path: &Path,
    label: &str,
) -> Result<String, String> {
    ensure_allowed_url(url)?;
    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("无法创建 {label} 缓存目录：{error}"))?;
    }
    let agent = agent();
    let etag = fs::read_to_string(etag_path).ok();
    let mut last_error = String::new();
    for attempt in 0..3 {
        let mut request = agent.get(url).set("User-Agent", USER_AGENT);
        if let Some(etag) = &etag {
            request = request.set("If-None-Match", etag.trim());
        }
        match request.call() {
            Ok(response) => {
                let response_etag = response.header("etag").map(str::to_owned);
                let body = response
                    .into_string()
                    .map_err(|error| format!("读取{label}失败：{error}"))?;
                let part = part_path(cache_path);
                fs::write(&part, body.as_bytes())
                    .map_err(|error| format!("写入{label}缓存失败：{error}"))?;
                replace_file(&part, cache_path)?;
                if let Some(etag) = response_etag {
                    let _ = fs::write(etag_path, etag);
                }
                return Ok(body);
            }
            Err(ureq::Error::Status(304, _)) => {
                return fs::read_to_string(cache_path)
                    .map_err(|error| format!("读取{label}缓存失败：{error}"));
            }
            Err(ureq::Error::Status(status, _)) => {
                last_error = format!("HTTP {status}");
                if !is_retryable_status(status) {
                    return Err(format!("请求{label}失败：HTTP {status}"));
                }
            }
            Err(error) => last_error = error.to_string(),
        }
        if attempt < 2 {
            thread::sleep(backoff(attempt));
        }
    }
    Err(format!("请求{label}失败：{last_error}"))
}

/// 实现 retryablestatus 相关的业务流程。
///
/// # 参数
/// - `status`：由调用方传入的status。
/// # 返回
/// 返回 `bool`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn is_retryable_status(status: u16) -> bool {
    matches!(status, 429 | 500 | 502 | 503)
}

/// 实现 解析manifest 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn parse_manifest(body: &str) -> Result<Vec<OfficialVersion>, String> {
    let value: Value =
        serde_json::from_str(body).map_err(|error| format!("版本清单 JSON 无效：{error}"))?;
    let versions = value
        .get("versions")
        .and_then(Value::as_array)
        .ok_or_else(|| "版本清单缺少 versions".to_owned())?;
    Ok(versions
        .iter()
        .filter_map(|version| {
            Some(OfficialVersion {
                id: version.get("id")?.as_str()?.to_owned(),
                kind: version.get("type")?.as_str()?.to_owned(),
                url: version.get("url")?.as_str()?.to_owned(),
                sha1: version.get("sha1")?.as_str()?.to_owned(),
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// 实现 parsespistonmeta版本 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn parses_piston_meta_versions() {
        let versions = parse_manifest(
            r#"{"versions":[{"id":"1.21.1","type":"release","url":"https://example/1.21.1.json","sha1":"abc"}]}"#,
        )
        .unwrap();
        assert_eq!(versions[0].id, "1.21.1");
    }
}
