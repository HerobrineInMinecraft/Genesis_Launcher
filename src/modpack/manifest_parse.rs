//! # `src/modpack/manifest_parse.rs`
//!
//! **功能域：** Modrinth 整合包读取、依赖补全、文件下载和覆盖层解压。
//!
//! 实现 manifest_parse 相关逻辑，并向同目录模块提供必要的类型与操作。
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `parse_manifest`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::path::Path;

use serde_json::Value;

use crate::loader::LoaderKind;

use super::model::{PackFile, PackManifest};
use super::naming::strip_launcher_export_prefix;
use super::security::safe_relative_path;

/// 实现 解析manifest 相关的业务流程。
///
/// # 参数
/// - `index_path`：由调用方传入的index路径。
/// - `archive_path`：由调用方传入的压缩包路径。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn parse_manifest(
    contents: &str,
    index_path: &str,
    archive_path: &Path,
) -> Result<PackManifest, String> {
    let index: Value =
        serde_json::from_str(contents).map_err(|error| format!("整合包索引格式无效：{error}"))?;
    let minecraft = index
        .pointer("/dependencies/minecraft")
        .and_then(Value::as_str)
        .ok_or_else(|| "整合包未指定 Minecraft 版本".to_owned())?
        .to_owned();
    let dependencies = index
        .get("dependencies")
        .and_then(Value::as_object)
        .ok_or_else(|| "整合包依赖信息无效".to_owned())?;
    let mut loader = None;
    for (name, value) in dependencies {
        let Some(version) = value.as_str() else {
            continue;
        };
        let kind = match name.to_ascii_lowercase().as_str() {
            "fabric-loader" => Some(LoaderKind::Fabric),
            "forge" => Some(LoaderKind::Forge),
            "neoforge" | "neo-forge" => Some(LoaderKind::NeoForge),
            "quilt-loader" => return Err("暂不支持 Quilt 整合包".to_owned()),
            _ => None,
        };
        if let Some(kind) = kind {
            loader = Some((kind, version.to_owned()));
            break;
        }
    }
    let name = index
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            archive_path
                .file_stem()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "整合包".to_owned());
    let name = strip_launcher_export_prefix(&name);
    let mut files = Vec::new();
    if let Some(entries) = index.get("files").and_then(Value::as_array) {
        for entry in entries {
            if entry.pointer("/env/client").and_then(Value::as_str) == Some("unsupported") {
                continue;
            }
            let Some(path) = entry
                .get("path")
                .and_then(Value::as_str)
                .and_then(safe_relative_path)
            else {
                return Err("整合包包含不安全的文件路径".to_owned());
            };
            let sha1 = entry
                .pointer("/hashes/sha1")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_ascii_lowercase();
            if sha1.len() != 40 {
                return Err(format!("{} 缺少有效 SHA1", path.display()));
            }
            let urls = entry
                .get("downloads")
                .and_then(Value::as_array)
                .map(|urls| {
                    urls.iter()
                        .filter_map(Value::as_str)
                        .filter(|url| url.starts_with("https://"))
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if urls.is_empty() {
                return Err(format!("{} 没有可用下载地址", path.display()));
            }
            files.push(PackFile { path, urls, sha1 });
        }
    }
    let archive_prefix = index_path
        .strip_suffix("modrinth.index.json")
        .unwrap_or_default()
        .to_owned();
    Ok(PackManifest {
        name,
        minecraft,
        loader,
        files,
        archive_prefix,
    })
}
