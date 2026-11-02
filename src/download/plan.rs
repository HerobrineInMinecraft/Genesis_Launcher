//! # `src/download/plan.rs`
//!
//! **功能域：** Minecraft 官方清单、客户端、资源、Java 运行时安装与完整性校验。
//!
//! 根据清单生成去重后的下载任务计划
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `collect_game_tasks`
//! - `deduplicate_tasks`
//! - `download_is_complete`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件位于外部数据或文件写入边界，不得绕过 URL、路径、哈希和大小限制。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use serde_json::Value;

use super::model::DownloadTask;
use super::security::{ensure_allowed_url, is_sha1, rules_allow, safe_join};

/// 实现 收集游戏tasks 相关的业务流程。
///
/// # 参数
/// - `asset_index`：由调用方传入的assetindex。
/// - `game_directory`：由调用方传入的游戏目录。
/// - `version_id`：由调用方传入的版本。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn collect_game_tasks(
    version_json: &Value,
    asset_index: &Value,
    game_directory: &Path,
    version_id: &str,
) -> Result<Vec<DownloadTask>, String> {
    let client = version_json
        .pointer("/downloads/client")
        .ok_or_else(|| "版本清单缺少客户端下载信息".to_owned())?;
    let mut tasks = Vec::new();
    add_download_task(&mut tasks, client, game_directory, version_id)?;
    if let Some(libraries) = version_json.get("libraries").and_then(Value::as_array) {
        for library in libraries {
            if !rules_allow(library) {
                continue;
            }
            if let Some(artifact) = library.pointer("/downloads/artifact") {
                add_library_task(&mut tasks, artifact, game_directory)?;
            }
            if let Some(classifiers) = library
                .pointer("/downloads/classifiers")
                .and_then(Value::as_object)
            {
                for (name, artifact) in classifiers {
                    if name.starts_with("natives-windows") {
                        add_library_task(&mut tasks, artifact, game_directory)?;
                    }
                }
            }
        }
    }
    if let Some(objects) = asset_index.get("objects").and_then(Value::as_object) {
        for object in objects.values() {
            let Some(hash) = object.get("hash").and_then(Value::as_str) else {
                continue;
            };
            if !is_sha1(hash) {
                return Err(format!("资源索引包含无效 SHA1：{hash}"));
            }
            let path = game_directory
                .join("assets")
                .join("objects")
                .join(&hash[..2])
                .join(hash);
            tasks.push(DownloadTask {
                url: format!(
                    "https://resources.download.minecraft.net/{}/{}",
                    &hash[..2],
                    hash
                ),
                path,
                sha1: Some(hash.to_owned()),
                size: object.get("size").and_then(Value::as_u64).unwrap_or(0),
            });
        }
    }
    deduplicate_tasks(&mut tasks);
    Ok(tasks)
}

/// 实现 添加librarytask 相关的业务流程。
///
/// # 参数
/// - `tasks`：由调用方传入的tasks。
/// - `artifact`：由调用方传入的artifact。
/// - `game_directory`：由调用方传入的游戏目录。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn add_library_task(
    tasks: &mut Vec<DownloadTask>,
    artifact: &Value,
    game_directory: &Path,
) -> Result<(), String> {
    let Some(path) = artifact.get("path").and_then(Value::as_str) else {
        return Ok(());
    };
    let Some(url) = artifact.get("url").and_then(Value::as_str) else {
        return Ok(());
    };
    let sha1 = artifact
        .get("sha1")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("游戏库缺少 SHA1：{path}"))?;
    if !is_sha1(sha1) {
        return Err(format!("游戏库 SHA1 无效：{path}"));
    }
    ensure_allowed_url(url)?;
    tasks.push(DownloadTask {
        url: url.to_owned(),
        path: safe_join(&game_directory.join("libraries"), path)?,
        sha1: Some(sha1.to_owned()),
        size: artifact.get("size").and_then(Value::as_u64).unwrap_or(0),
    });
    Ok(())
}

/// 实现 添加下载task 相关的业务流程。
///
/// # 参数
/// - `tasks`：由调用方传入的tasks。
/// - `download`：由调用方传入的下载。
/// - `game_directory`：由调用方传入的游戏目录。
/// - `version_id`：由调用方传入的版本。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn add_download_task(
    tasks: &mut Vec<DownloadTask>,
    download: &Value,
    game_directory: &Path,
    version_id: &str,
) -> Result<(), String> {
    let Some(url) = download.get("url").and_then(Value::as_str) else {
        return Ok(());
    };
    let sha1 = download
        .get("sha1")
        .and_then(Value::as_str)
        .ok_or_else(|| "客户端文件缺少 SHA1".to_owned())?;
    if !is_sha1(sha1) {
        return Err("客户端文件 SHA1 无效".to_owned());
    }
    ensure_allowed_url(url)?;
    tasks.push(DownloadTask {
        url: url.to_owned(),
        path: game_directory
            .join("versions")
            .join(version_id)
            .join(format!("{version_id}.jar")),
        sha1: Some(sha1.to_owned()),
        size: download.get("size").and_then(Value::as_u64).unwrap_or(0),
    });
    Ok(())
}

/// 实现 deduplicatetasks 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn deduplicate_tasks(tasks: &mut Vec<DownloadTask>) {
    let mut seen = HashSet::new();
    tasks.retain(|task| seen.insert(task.path.clone()));
}

/// 实现 下载complete 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `bool`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn download_is_complete(task: &DownloadTask) -> bool {
    task.path.is_file()
        && fs::metadata(&task.path)
            .is_ok_and(|metadata| task.size == 0 || metadata.len() == task.size)
}
