//! # `src/exporter/collect.rs`
//!
//! **功能域：** 版本和整合包导出、文件收集、哈希计算与压缩包生成。
//!
//! 按选项收集需要导出的目录和文件
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `collect_modpack_files`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::path::Path;

use super::hash::hash_pack_file;
use super::model::{CollectedPackFile, VersionExportOptions};

/// 实现 收集整合包文件 相关的业务流程。
///
/// # 参数
/// - `options`：由调用方传入的options。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn collect_modpack_files(
    data_directory: &Path,
    options: &VersionExportOptions,
) -> Result<Vec<CollectedPackFile>, String> {
    let mut folders = Vec::new();
    if options.config {
        folders.extend(["config", "defaultconfigs"]);
    }
    if options.mods {
        folders.push("mods");
    }
    if options.saves {
        folders.push("saves");
    }
    if options.resourcepacks {
        folders.push("resourcepacks");
    }
    if options.shaderpacks {
        folders.push("shaderpacks");
    }
    let mut files = Vec::new();
    for folder in folders {
        collect_pack_directory(&data_directory.join(folder), folder, &mut files)?;
    }
    Ok(files)
}

/// 实现 收集包目录 相关的业务流程。
///
/// # 参数
/// - `source`：由调用方传入的source。
/// - `archive_prefix`：由调用方传入的压缩包prefix。
/// - `output`：由调用方传入的output。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn collect_pack_directory(
    source: &Path,
    archive_prefix: &str,
    output: &mut Vec<CollectedPackFile>,
) -> Result<(), String> {
    let entries = match std::fs::read_dir(source) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!("无法读取导出目录 {}：{error}", source.display()));
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let archive_path = format!("{archive_prefix}/{name}");
        if path.is_dir() {
            collect_pack_directory(&path, &archive_path, output)?;
        } else if path.is_file() {
            let queryable = is_modrinth_queryable_path(&archive_path);
            let (sha1, sha512) = if queryable {
                let (sha1, sha512) = hash_pack_file(&path)?;
                (Some(sha1), Some(sha512))
            } else {
                (None, None)
            };
            output.push(CollectedPackFile {
                source: path,
                path: archive_path,
                sha1,
                sha512,
                queryable,
            });
        }
    }
    Ok(())
}

/// 实现 modrinthqueryable路径 相关的业务流程。
///
/// # 参数
/// - `path`：由调用方传入的路径。
/// # 返回
/// 返回 `bool`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn is_modrinth_queryable_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let extension = Path::new(&lower)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    matches!(
        extension,
        "jar" | "zip" | "mrpack" | "rar" | "disabled" | "old"
    ) && [
        "mods/",
        "resourcepacks/",
        "shaderpacks/",
        "datapacks/",
        "packs/",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix))
}
