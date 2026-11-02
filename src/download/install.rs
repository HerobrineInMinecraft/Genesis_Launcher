//! # `src/download/install.rs`
//!
//! **功能域：** Minecraft 官方清单、客户端、资源、Java 运行时安装与完整性校验。
//!
//! 编排安装流程并串联校验、下载和文件提交
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `start_install`
//! - `start_install_by_id`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 实现中包含线程、通道或共享状态；同步边界必须与现有轮询和生命周期保持一致。
//!
//! ## 安全与平台
//! - 本文件位于外部数据或文件写入边界，不得绕过 URL、路径、哈希和大小限制。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Instant;

use serde_json::Value;

use super::filesystem::{extract_native, materialize_indexed_assets};
use super::java::install_java_runtime;
use super::manifest::fetch_official_versions;
use super::model::{DownloadTask, InstallUpdate, OfficialVersion, Progress};
use super::plan::{collect_game_tasks, download_is_complete};
use super::security::{ensure_allowed_url, validate_path_component};
use super::transfer::{download_asset_index, download_small_file, download_tasks};

/// 实现 启动安装 相关的业务流程。
///
/// # 参数
/// - `game_directory`：由调用方传入的游戏目录。
/// - `version`：由调用方传入的版本。
/// # 返回
/// 返回 `mpsc::Receiver<InstallUpdate>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn start_install(
    game_directory: PathBuf,
    version: OfficialVersion,
) -> mpsc::Receiver<InstallUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = install_version(&game_directory, &version, &sender);
        let (success, message, total) = match result {
            Ok(total) => (true, format!("{} 安装完成", version.id), total),
            Err(error) => (false, error, 0),
        };
        let _ = sender.send(InstallUpdate {
            stage: if success {
                "安装完成".to_owned()
            } else {
                "安装失败".to_owned()
            },
            completed_bytes: if success { total } else { 0 },
            total_bytes: total,
            files_done: 0,
            files_total: 0,
            bytes_per_second: 0,
            finished: true,
            success,
            message,
        });
    });
    receiver
}

/// 实现 启动安装 相关的业务流程。
///
/// # 参数
/// - `game_directory`：由调用方传入的游戏目录。
/// - `version_id`：由调用方传入的版本。
/// # 返回
/// 返回 `mpsc::Receiver<InstallUpdate>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn start_install_by_id(
    game_directory: PathBuf,
    version_id: String,
) -> mpsc::Receiver<InstallUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let version = fetch_official_versions().and_then(|versions| {
            versions
                .into_iter()
                .find(|version| version.id == version_id)
                .ok_or_else(|| format!("官方版本列表中未找到 {version_id}"))
        });
        let result = match version {
            Ok(version) => install_version(&game_directory, &version, &sender),
            Err(error) => Err(error),
        };
        let (success, message, total) = match result {
            Ok(total) => (true, format!("{version_id} 文件补全完成"), total),
            Err(error) => (false, error, 0),
        };
        let _ = sender.send(InstallUpdate {
            stage: if success {
                "安装完成".to_owned()
            } else {
                "安装失败".to_owned()
            },
            completed_bytes: if success { total } else { 0 },
            total_bytes: total,
            files_done: 0,
            files_total: 0,
            bytes_per_second: 0,
            finished: true,
            success,
            message,
        });
    });
    receiver
}

/// 实现 安装版本 相关的业务流程。
///
/// # 参数
/// - `game_directory`：由调用方传入的游戏目录。
/// - `version`：由调用方传入的版本。
/// - `sender`：由调用方传入的sender。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn install_version(
    game_directory: &Path,
    version: &OfficialVersion,
    sender: &mpsc::Sender<InstallUpdate>,
) -> Result<u64, String> {
    ensure_allowed_url(&version.url)?;
    validate_path_component(&version.id)?;
    let versions_dir = game_directory.join("versions");
    let version_dir = versions_dir.join(&version.id);
    fs::create_dir_all(&version_dir).map_err(|error| format!("无法创建版本目录：{error}"))?;
    let json_path = version_dir.join(format!("{}.json", version.id));
    let _ = sender.send(InstallUpdate::running("下载版本清单", 0, 0, 0, 0));
    download_small_file(&version.url, &json_path, Some(&version.sha1))?;
    let json: Value = serde_json::from_str(
        &fs::read_to_string(&json_path).map_err(|error| format!("读取版本清单失败：{error}"))?,
    )
    .map_err(|error| format!("解析版本清单失败：{error}"))?;

    let asset_index = json
        .get("assetIndex")
        .ok_or_else(|| "版本清单缺少资源索引".to_owned())?;
    let asset_id = asset_index
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("legacy");
    validate_path_component(asset_id)?;
    let asset_index_path = game_directory
        .join("assets")
        .join("indexes")
        .join(format!("{asset_id}.json"));
    if let (Some(url), Some(sha1)) = (
        asset_index.get("url").and_then(Value::as_str),
        asset_index.get("sha1").and_then(Value::as_str),
    ) {
        ensure_allowed_url(url)?;
        download_asset_index(url, &asset_index_path, sha1)?;
    }

    let index: Value = serde_json::from_str(
        &fs::read_to_string(&asset_index_path)
            .map_err(|error| format!("读取资源索引失败：{error}"))?,
    )
    .map_err(|error| format!("解析资源索引失败：{error}"))?;
    let tasks = collect_game_tasks(&json, &index, game_directory, &version.id)?;
    let pending: Vec<DownloadTask> = tasks
        .iter()
        .filter(|task| !download_is_complete(task))
        .cloned()
        .collect();
    let total_size: u64 = pending.iter().map(|task| task.size).sum();
    let progress = Arc::new(Progress {
        sender: sender.clone(),
        completed_bytes: AtomicU64::new(0),
        total_bytes: total_size,
        files_done: AtomicUsize::new(0),
        files_total: pending.len(),
        stage: Mutex::new("下载游戏文件".to_owned()),
        last_report: Mutex::new(0),
        started_at: Instant::now(),
    });
    progress.send(true, "");
    download_tasks(game_directory, &pending, progress.clone())?;

    progress.set_stage("解压 Windows natives");
    let natives_dir = version_dir.join("natives");
    fs::create_dir_all(&natives_dir).map_err(|error| format!("无法创建 natives 目录：{error}"))?;
    for task in &tasks {
        let name = task.path.to_string_lossy().to_lowercase();
        if name.ends_with(".jar") && name.contains("natives-windows") {
            extract_native(&task.path, &natives_dir)?;
        }
    }

    if index
        .get("virtual")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        progress.set_stage("整理旧版资源");
        materialize_indexed_assets(
            game_directory,
            &index,
            &game_directory.join("assets").join("virtual").join(asset_id),
        )?;
    }
    if index
        .get("map_to_resources")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        progress.set_stage("整理旧版资源");
        materialize_indexed_assets(game_directory, &index, &game_directory.join("resources"))?;
    }

    if let (Some(component), Some(required_major)) = (
        json.pointer("/javaVersion/component")
            .and_then(Value::as_str),
        json.pointer("/javaVersion/majorVersion")
            .and_then(Value::as_u64)
            .map(|major| major as u32),
    ) && !crate::game::has_compatible_java(game_directory, required_major)
    {
        install_java_runtime(game_directory, component, required_major, sender)?;
    }

    progress.set_stage("安装完成");
    Ok(total_size)
}
