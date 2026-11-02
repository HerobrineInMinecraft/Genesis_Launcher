//! # `src/download/java.rs`
//!
//! **功能域：** Minecraft 官方清单、客户端、资源、Java 运行时安装与完整性校验。
//!
//! 定位、检查或安装满足 Minecraft 要求的 Java 运行时
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `install_java_runtime`
//! - `java_runtime_manifest_report`
//! - `install_java_runtime_for_report`
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
use std::sync::atomic::{AtomicU64, AtomicUsize};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Instant;

use serde_json::Value;

use super::manifest::fetch_cached_text;
use super::model::{DownloadTask, InstallUpdate, Progress};
use super::plan::{deduplicate_tasks, download_is_complete};
use super::security::{ensure_allowed_url, is_sha1, safe_join, validate_path_component};
use super::transfer::{cache_directory, download_small_file, download_tasks};

/// 提供 JavaRUNTIMEMANIFEST地址 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const JAVA_RUNTIME_MANIFEST_URL: &str = "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

/// 实现 安装Javaruntime 相关的业务流程。
///
/// # 参数
/// - `component`：由调用方传入的component。
/// - `required_major`：由调用方传入的requiredmajor。
/// - `sender`：由调用方传入的sender。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn install_java_runtime(
    game_directory: &Path,
    component: &str,
    required_major: u32,
    sender: &mpsc::Sender<InstallUpdate>,
) -> Result<(), String> {
    validate_path_component(component)?;
    let _ = sender.send(InstallUpdate::running("检查 Java 运行时", 0, 0, 0, 0));
    let all = java_runtime_catalog()?;
    let platform = java_runtime_platform();
    let candidates = all
        .get(platform)
        .and_then(|value| value.get(component))
        .and_then(Value::as_array)
        .ok_or_else(|| format!("Mojang Java 运行时清单中没有 {component}（{platform}）"))?;
    let selected = candidates
        .iter()
        .find(|entry| {
            entry
                .pointer("/version/name")
                .and_then(Value::as_str)
                .and_then(|version| version.split('.').next())
                .and_then(|major| major.parse::<u32>().ok())
                == Some(required_major)
        })
        .or_else(|| candidates.first())
        .ok_or_else(|| format!("{component} 没有可用的 Windows 运行时"))?;
    let manifest_url = selected
        .pointer("/manifest/url")
        .and_then(Value::as_str)
        .ok_or_else(|| "Java 运行时缺少清单地址".to_owned())?;
    ensure_allowed_url(manifest_url)?;
    let manifest_sha1 = selected
        .pointer("/manifest/sha1")
        .and_then(Value::as_str)
        .ok_or_else(|| "Java 运行时缺少清单 SHA1".to_owned())?;
    if !is_sha1(manifest_sha1) {
        return Err("Java 运行时清单 SHA1 无效".to_owned());
    }
    let cache = cache_directory();
    let manifest_path = cache.join(format!("java-runtime-{component}-{manifest_sha1}.json"));
    download_small_file(manifest_url, &manifest_path, Some(manifest_sha1))?;
    let manifest: Value = serde_json::from_str(
        &fs::read_to_string(&manifest_path)
            .map_err(|error| format!("读取 Java 运行时清单失败：{error}"))?,
    )
    .map_err(|error| format!("Java 运行时清单 JSON 无效：{error}"))?;
    let runtime_root = game_directory
        .join("runtime")
        .join(component)
        .join(platform);
    fs::create_dir_all(&runtime_root)
        .map_err(|error| format!("无法创建 Java 运行时目录：{error}"))?;
    let mut tasks = Vec::new();
    let files = manifest
        .get("files")
        .and_then(Value::as_object)
        .ok_or_else(|| "Java 运行时清单缺少 files".to_owned())?;
    for (relative, entry) in files {
        if entry.get("type").and_then(Value::as_str) != Some("file") {
            continue;
        }
        let Some(raw) = entry.pointer("/downloads/raw") else {
            continue;
        };
        let Some(url) = raw.get("url").and_then(Value::as_str) else {
            continue;
        };
        ensure_allowed_url(url)?;
        let sha1 = raw
            .get("sha1")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("Java 运行时文件缺少 SHA1：{relative}"))?;
        if !is_sha1(sha1) {
            return Err(format!("Java 运行时文件 SHA1 无效：{relative}"));
        }
        tasks.push(DownloadTask {
            url: url.to_owned(),
            path: safe_join(&runtime_root, relative)?,
            sha1: Some(sha1.to_owned()),
            size: raw.get("size").and_then(Value::as_u64).unwrap_or(0),
        });
    }
    deduplicate_tasks(&mut tasks);
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
        stage: Mutex::new("下载 Java 运行时".to_owned()),
        last_report: Mutex::new(0),
        started_at: Instant::now(),
    });
    progress.send(true, "");
    download_tasks(game_directory, &pending, progress)?;
    if crate::game::has_compatible_java(game_directory, required_major) {
        Ok(())
    } else {
        Err(format!(
            "Java {required_major} 运行时安装后仍不可用，请检查运行时清单"
        ))
    }
}

/// 实现 Javaruntimemanifestreport 相关的业务流程。
///
/// # 参数
/// - `component`：由调用方传入的component。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn java_runtime_manifest_report(component: &str) -> Result<String, String> {
    validate_path_component(component)?;
    let catalog = java_runtime_catalog()?;
    let platform = java_runtime_platform();
    let entries = catalog
        .get(platform)
        .and_then(|value| value.get(component))
        .and_then(Value::as_array)
        .ok_or_else(|| format!("清单中没有 {component}（{platform}）"))?;
    let versions = entries
        .iter()
        .filter_map(|entry| entry.pointer("/version/name").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join(", ");
    let selected = entries
        .first()
        .ok_or_else(|| format!("{component} 没有可用运行时"))?;
    let url = selected
        .pointer("/manifest/url")
        .and_then(Value::as_str)
        .ok_or_else(|| "运行时清单缺少地址".to_owned())?;
    let sha1 = selected
        .pointer("/manifest/sha1")
        .and_then(Value::as_str)
        .ok_or_else(|| "运行时清单缺少 SHA1".to_owned())?;
    let path = cache_directory().join(format!("java-runtime-{component}-{sha1}.json"));
    download_small_file(url, &path, Some(sha1))?;
    let manifest: Value = serde_json::from_str(
        &fs::read_to_string(path).map_err(|error| format!("读取运行时清单失败：{error}"))?,
    )
    .map_err(|error| format!("运行时清单 JSON 无效：{error}"))?;
    let file_count = manifest
        .get("files")
        .and_then(Value::as_object)
        .map(|files| {
            files
                .values()
                .filter(|entry| entry.get("type").and_then(Value::as_str) == Some("file"))
                .count()
        })
        .unwrap_or(0);
    Ok(format!(
        "{component} ({platform}): {versions}; {file_count} 个运行时文件"
    ))
}

/// 实现 安装Javaruntimeforreport 相关的业务流程。
///
/// # 参数
/// - `game_directory`：由调用方传入的游戏目录。
/// - `component`：由调用方传入的component。
/// - `required_major`：由调用方传入的requiredmajor。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn install_java_runtime_for_report(
    game_directory: &Path,
    component: &str,
    required_major: u32,
) -> Result<(), String> {
    let (sender, _receiver) = mpsc::channel();
    install_java_runtime(game_directory, component, required_major, &sender)
}

/// 实现 Javaruntimecatalog 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn java_runtime_catalog() -> Result<Value, String> {
    let cache = cache_directory();
    let path = cache.join("java-runtime-all.json");
    let etag_path = cache.join("java-runtime-all.etag");
    let body = fetch_cached_text(
        JAVA_RUNTIME_MANIFEST_URL,
        &path,
        &etag_path,
        "Java 运行时清单",
    )?;
    serde_json::from_str(&body).map_err(|error| format!("Java 运行时清单 JSON 无效：{error}"))
}

/// 实现 Javaruntimeplatform 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `&'static str`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn java_runtime_platform() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "windows-arm64",
        "x86" => "windows-x86",
        _ => "windows-x64",
    }
}
