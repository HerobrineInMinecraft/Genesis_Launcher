//! # `src/loader/util.rs`
//!
//! **功能域：** Fabric、Forge 和 NeoForge 的版本查询、下载与安装。
//!
//! 实现 util 相关逻辑，并向同目录模块提供必要的类型与操作。
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `request_with_retry`
//! - `request_once`
//! - `write_atomic`
//! - `maven_path`
//! - `neoforge_prefix`
//! - `forge_loader_part`
//! - `validate_component`
//! - `xml_versions`
//! - `version_key`
//! - `ensure_allowed_loader_url`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 实现中包含线程、通道或共享状态；同步边界必须与现有轮询和生命周期保持一致。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use super::*;
use std::fs;
use std::path::Path;
use std::thread;
use std::time::Duration;

/// 实现 请求withretry 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn request_with_retry(url: &str) -> Result<ureq::Response, String> {
    let mut last_error = String::new();
    for attempt in 0..3 {
        match request_once(url) {
            Ok(response) => return Ok(response),
            Err(error) => last_error = error,
        }
        if attempt < 2 {
            thread::sleep(Duration::from_millis(350 * (1 << attempt)));
        }
    }
    Err(format!("请求失败 {url}：{last_error}"))
}

/// 实现 请求once 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn request_once(url: &str) -> Result<ureq::Response, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(6))
        .timeout_read(Duration::from_secs(20))
        .build();
    agent
        .get(url)
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(|error| error.to_string())
}

/// 实现 写入atomic 相关的业务流程。
///
/// # 参数
/// - `bytes`：由调用方传入的bytes。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let part = path.with_extension("part");
    fs::write(&part, bytes).map_err(|error| format!("写入临时文件失败：{error}"))?;
    if path.exists() {
        fs::remove_file(path).map_err(|error| format!("替换旧文件失败：{error}"))?;
    }
    fs::rename(&part, path).map_err(|error| format!("提交文件失败：{error}"))
}

/// 实现 maven路径 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn maven_path(name: &str) -> Result<String, String> {
    let (coordinates, extension) = name.split_once('@').unwrap_or((name, "jar"));
    let parts: Vec<&str> = coordinates.split(':').collect();
    if parts.len() < 3 {
        return Err(format!("无效的 Maven 坐标：{name}"));
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts
        .get(3)
        .map(|value| format!("-{value}"))
        .unwrap_or_default();
    Ok(format!(
        "{group}/{artifact}/{version}/{artifact}-{version}{classifier}.{extension}"
    ))
}

/// 实现 neoforgeprefix 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn neoforge_prefix(game_version: &str) -> Result<String, String> {
    if game_version == "1.20.1" {
        return Ok("1.20.1-47.".to_owned());
    }
    let version = game_version
        .split('-')
        .next()
        .unwrap_or(game_version)
        .strip_prefix("1.")
        .unwrap_or_else(|| game_version.split('-').next().unwrap_or(game_version));
    let mut parts = version.split('.');
    let major = parts
        .next()
        .ok_or_else(|| "无效的 Minecraft 版本".to_owned())?;
    let minor = parts
        .next()
        .ok_or_else(|| "无效的 Minecraft 版本".to_owned())?;
    Ok(format!("{major}.{minor}."))
}

/// 实现 forge加载器part 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `&str`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn forge_loader_part(version: &str) -> &str {
    version
        .split_once('-')
        .map(|(_, loader)| loader)
        .unwrap_or(version)
}

/// 实现 校验component 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn validate_component(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.contains(['/', '\\'])
        || value.contains("..")
        || value.contains(':')
    {
        Err(format!("拒绝不安全的加载器版本标识：{value}"))
    } else {
        Ok(())
    }
}

/// 实现 xml版本 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Vec<String>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn xml_versions(xml: &str) -> Vec<String> {
    xml.split("<version>")
        .skip(1)
        .filter_map(|entry| entry.split("</version>").next().map(str::to_owned))
        .collect()
}

/// 实现 版本key 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Vec<u32>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn version_key(version: &str) -> Vec<u32> {
    version
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .collect()
}

/// 实现 确保allowed加载器地址 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn ensure_allowed_loader_url(url: &str) -> Result<(), String> {
    /// 提供 HOSTS 使用的只读常量。
    ///
    /// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
    const HOSTS: [&str; 11] = [
        "https://meta.fabricmc.net/",
        "https://maven.fabricmc.net/",
        "https://bmclapi2.bangbang93.com/",
        "https://maven.minecraftforge.net/",
        "https://maven.neoforged.net/",
        "https://repo1.maven.org/",
        "https://maven.aliyun.com/",
        "https://repo.huaweicloud.com/",
        "https://mirrors.cloud.tencent.com/",
        "https://api.github.com/",
        "https://libraries.minecraft.net/",
    ];
    if HOSTS.iter().any(|host| url.starts_with(host)) {
        Ok(())
    } else {
        Err(format!("拒绝非受信任加载器地址：{url}"))
    }
}
