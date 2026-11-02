//! # `src/exporter/archive.rs`
//!
//! **功能域：** 版本和整合包导出、文件收集、哈希计算与压缩包生成。
//!
//! 写出 ZIP 条目、清单和覆盖层
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `export_pack_with_hosted_files`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::collections::HashMap;
use std::io::Write as _;
use std::path::Path;

use crate::versions::GameVersion;

use super::model::{CollectedPackFile, HostedPackFile, VersionExportOptions};
use super::sanitize_filename;

/// 实现 导出包withhosted文件 相关的业务流程。
///
/// # 参数
/// - `destination`：由调用方传入的destination。
/// - `options`：由调用方传入的options。
/// - `files`：由调用方传入的文件。
/// - `hosted`：由调用方传入的hosted。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn export_pack_with_hosted_files(
    version: &GameVersion,
    destination: &Path,
    options: &VersionExportOptions,
    files: &[CollectedPackFile],
    hosted: &HashMap<String, HostedPackFile>,
) -> Result<(), String> {
    let file =
        std::fs::File::create(destination).map_err(|error| format!("无法创建导出文件：{error}"))?;
    let mut archive = zip::ZipWriter::new(file);
    let zip_options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);
    let mut dependencies = serde_json::Map::new();
    dependencies.insert(
        "minecraft".to_owned(),
        serde_json::Value::String(version.game_version.clone()),
    );
    if let Some((key, loader_version)) = modrinth_loader_dependency(version) {
        dependencies.insert(key.to_owned(), serde_json::Value::String(loader_version));
    }
    let index_files = files
        .iter()
        .filter_map(|file| {
            let hosted = hosted.get(file.sha1.as_ref()?)?;
            Some(serde_json::json!({
                "path": file.path,
                "hashes": {
                    "sha1": file.sha1,
                    "sha512": file.sha512,
                },
                "downloads": hosted.downloads,
                "fileSize": std::fs::metadata(&file.source).ok()?.len(),
            }))
        })
        .collect::<Vec<_>>();
    let index = serde_json::json!({
        "formatVersion": 1,
        "game": "minecraft",
        "versionId": sanitize_filename(&version.version),
        // 整合包名称属于版本内容，不应混入启动器品牌，否则导入后会把
        // “Genesis Launcher · 1.21.1”之类的字符串当成实例名。
        "name": version.version.clone(),
        "summary": format!(
            "{} hosted, {} embedded",
            index_files.len(),
            files.len().saturating_sub(index_files.len())
        ),
        "files": index_files,
        "dependencies": dependencies,
    });
    archive
        .start_file("modrinth.index.json", zip_options)
        .map_err(|error| format!("无法写入 Modrinth 清单：{error}"))?;
    archive
        .write_all(
            serde_json::to_string_pretty(&index)
                .map_err(|error| format!("无法生成 Modrinth 清单：{error}"))?
                .as_bytes(),
        )
        .map_err(|error| format!("无法写入 Modrinth 清单：{error}"))?;
    for file in files {
        let is_hosted = file
            .sha1
            .as_ref()
            .is_some_and(|sha1| hosted.contains_key(sha1));
        if !is_hosted {
            add_zip_file(
                &mut archive,
                &file.source,
                &format!("overrides/{}", file.path),
                zip_options,
            )?;
        }
    }
    archive
        .finish()
        .map_err(|error| format!("无法完成 {:?} 整合包：{error}", options.format))?;
    Ok(())
}

/// 实现 modrinth加载器dependency 相关的业务流程。
///
/// # 参数
/// - `version`：由调用方传入的版本。
/// # 返回
/// 返回 `Option<(&'static str, String)>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn modrinth_loader_dependency(version: &GameVersion) -> Option<(&'static str, String)> {
    let (key, prefix) = match version.loader_family.as_str() {
        "Fabric" => ("fabric-loader", "Fabric "),
        "Forge" => ("forge", "Forge "),
        "NeoForge" => ("neoforge", "NeoForge "),
        _ => return None,
    };
    let loader_version = version.loader.strip_prefix(prefix)?;
    (!loader_version.is_empty()).then(|| (key, loader_version.to_owned()))
}

/// 实现 添加zip文件 相关的业务流程。
///
/// # 参数
/// - `archive`：由调用方传入的压缩包。
/// - `source`：由调用方传入的source。
/// - `archive_path`：由调用方传入的压缩包路径。
/// - `options`：由调用方传入的options。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn add_zip_file(
    archive: &mut zip::ZipWriter<std::fs::File>,
    source: &Path,
    archive_path: &str,
    options: zip::write::SimpleFileOptions,
) -> Result<(), String> {
    archive
        .start_file(archive_path.replace('\\', "/"), options)
        .map_err(|error| format!("无法写入 {}：{error}", archive_path))?;
    let mut file = std::fs::File::open(source)
        .map_err(|error| format!("无法读取 {}：{error}", source.display()))?;
    std::io::copy(&mut file, archive)
        .map_err(|error| format!("无法压缩 {}：{error}", source.display()))?;
    Ok(())
}
