//! # `src/loader/versions.rs`
//!
//! **功能域：** Fabric、Forge 和 NeoForge 的版本查询、下载与安装。
//!
//! 实现 versions 相关逻辑，并向同目录模块提供必要的类型与操作。
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `start_fetch_versions`
//! - `fetch_forge_versions`
//! - `parse_bmcl_forge_versions`
//! - `parse_bmcl_maven_directories`
//! - `fetch_neoforge_versions`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 实现中包含线程、通道或共享状态；同步边界必须与现有轮询和生命周期保持一致。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use super::install::fetch_versions;
use super::network::fetch_text;
use super::util::{version_key, xml_versions};
use super::*;

/// 实现 启动获取版本 相关的业务流程。
///
/// # 参数
/// - `game_version`：由调用方传入的游戏版本。
/// - `loader`：由调用方传入的加载器。
/// # 返回
/// 返回 `mpsc::Receiver<Result<Vec<String>, String>>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn start_fetch_versions(
    game_version: String,
    loader: LoaderKind,
) -> mpsc::Receiver<Result<Vec<String>, String>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(fetch_versions(&game_version, loader));
    });
    receiver
}

/// 实现 获取forge版本 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn fetch_forge_versions(game_version: &str) -> Result<Vec<String>, String> {
    let bmcl_url = format!(
        "https://bmclapi2.bangbang93.com/forge/minecraft/{}",
        game_version.replace('-', "_")
    );
    if let Ok(body) = fetch_text(&bmcl_url)
        && let Ok(versions) = parse_bmcl_forge_versions(&body, game_version)
        && !versions.is_empty()
    {
        return Ok(versions);
    }
    let body =
        fetch_text("https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml")?;
    let mut versions = xml_versions(&body)
        .into_iter()
        .filter(|version| version.starts_with(&format!("{game_version}-")))
        .collect::<Vec<_>>();
    versions.sort_by_key(|version| std::cmp::Reverse(version_key(version)));
    versions.dedup();
    if versions.is_empty() {
        Err(format!("Forge 暂不支持 Minecraft {game_version}"))
    } else {
        Ok(versions)
    }
}

/// 实现 解析bmclforge版本 相关的业务流程。
///
/// # 参数
/// - `game_version`：由调用方传入的游戏版本。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn parse_bmcl_forge_versions(
    body: &str,
    game_version: &str,
) -> Result<Vec<String>, String> {
    let entries: Value = serde_json::from_str(body)
        .map_err(|error| format!("BMCL Forge 版本列表格式无效：{error}"))?;
    let mut versions = entries
        .as_array()
        .ok_or_else(|| "BMCL Forge 版本列表无效".to_owned())?
        .iter()
        .filter(|entry| entry.get("mcversion").and_then(Value::as_str) == Some(game_version))
        .filter(|entry| {
            entry
                .get("files")
                .and_then(Value::as_array)
                .is_some_and(|files| {
                    files.iter().any(|file| {
                        file.get("category").and_then(Value::as_str) == Some("installer")
                            && file.get("format").and_then(Value::as_str) == Some("jar")
                    })
                })
        })
        .filter_map(|entry| {
            let version = entry.get("version")?.as_str()?;
            let branch = entry
                .get("branch")
                .and_then(Value::as_str)
                .filter(|branch| !branch.is_empty());
            Some(match branch {
                Some(branch) => format!("{game_version}-{version}-{branch}"),
                None => format!("{game_version}-{version}"),
            })
        })
        .collect::<Vec<_>>();
    versions.sort_by_key(|version| std::cmp::Reverse(version_key(version)));
    versions.dedup();
    Ok(versions)
}

/// 实现 解析bmclmavendirectories 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn parse_bmcl_maven_directories(body: &str) -> Result<Vec<String>, String> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| format!("BMCL Maven 版本列表格式无效：{error}"))?;
    Ok(value
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(|| "BMCL Maven 版本列表缺少 files".to_owned())?
        .iter()
        .filter(|file| file.get("type").and_then(Value::as_str) == Some("DIRECTORY"))
        .filter_map(|file| file.get("name").and_then(Value::as_str))
        .map(str::to_owned)
        .collect())
}

/// 实现 获取neoforge版本 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn fetch_neoforge_versions(game_version: &str) -> Result<Vec<String>, String> {
    let mut errors = Vec::new();
    let artifact = if game_version == "1.20.1" {
        "forge"
    } else {
        "neoforge"
    };
    if let Ok(body) = fetch_text(&format!(
        "https://bmclapi2.bangbang93.com/neoforge/meta/api/maven/details/releases/net/neoforged/{artifact}"
    )) && let Ok(versions) = parse_bmcl_maven_directories(&body)
        && !versions.is_empty()
    {
        return Ok(versions);
    }
    if let Ok(body) = fetch_text(&format!(
        "https://maven.neoforged.net/releases/net/neoforged/{artifact}/maven-metadata.xml"
    )) {
        let versions = xml_versions(&body);
        if !versions.is_empty() {
            return Ok(versions);
        }
    } else {
        errors.push("NeoForge Maven metadata");
    }
    if let Ok(body) = fetch_text(&format!(
        "https://repo1.maven.org/maven2/net/neoforged/{artifact}/maven-metadata.xml"
    )) {
        let versions = xml_versions(&body);
        if !versions.is_empty() {
            return Ok(versions);
        }
    } else {
        errors.push("Maven Central metadata");
    }
    for mirror in [
        format!(
            "https://maven.aliyun.com/repository/releases/net/neoforged/{artifact}/maven-metadata.xml"
        ),
        format!(
            "https://maven.aliyun.com/repository/public/net/neoforged/{artifact}/maven-metadata.xml"
        ),
        format!(
            "https://repo.huaweicloud.com/repository/maven/net/neoforged/{artifact}/maven-metadata.xml"
        ),
        format!(
            "https://mirrors.cloud.tencent.com/nexus/repository/maven-public/net/neoforged/{artifact}/maven-metadata.xml"
        ),
    ] {
        if let Ok(body) = fetch_text(&mirror) {
            let versions = xml_versions(&body);
            if !versions.is_empty() {
                return Ok(versions);
            }
        } else {
            errors.push("NeoForge Maven 镜像");
        }
    }
    if let Ok(body) =
        fetch_text("https://api.github.com/repos/neoforged/NeoForge/releases?per_page=100")
    {
        let value: Value = serde_json::from_str(&body)
            .map_err(|error| format!("NeoForge GitHub 版本列表格式无效：{error}"))?;
        let versions = value
            .as_array()
            .ok_or_else(|| "NeoForge GitHub 版本列表无效".to_owned())?
            .iter()
            .filter_map(|release| release.get("tag_name")?.as_str())
            .map(|tag| {
                tag.trim_start_matches('v')
                    .strip_prefix(&format!("{game_version}-"))
                    .unwrap_or(tag)
                    .to_owned()
            })
            .filter(|version| !version.is_empty())
            .collect::<Vec<_>>();
        if !versions.is_empty() {
            return Ok(versions);
        }
    } else {
        errors.push("NeoForge GitHub Releases");
    }
    if let Ok(body) =
        fetch_text("https://api.github.com/repos/neoforged/NeoForge/tags?per_page=100")
    {
        let value: Value = serde_json::from_str(&body)
            .map_err(|error| format!("NeoForge GitHub 标签格式无效：{error}"))?;
        let versions = value
            .as_array()
            .ok_or_else(|| "NeoForge GitHub 标签无效".to_owned())?
            .iter()
            .filter_map(|tag| tag.get("name")?.as_str())
            .map(|tag| {
                tag.trim_start_matches('v')
                    .strip_prefix(&format!("{game_version}-"))
                    .unwrap_or(tag)
                    .to_owned()
            })
            .filter(|version| !version.is_empty())
            .collect::<Vec<_>>();
        if !versions.is_empty() {
            return Ok(versions);
        }
    } else {
        errors.push("NeoForge GitHub Tags");
    }
    Err(format!("无法获取 NeoForge 版本列表：{}", errors.join("、")))
}
