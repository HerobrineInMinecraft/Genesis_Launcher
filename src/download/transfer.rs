//! # `src/download/transfer.rs`
//!
//! **功能域：** Minecraft 官方清单、客户端、资源、Java 运行时安装与完整性校验。
//!
//! 执行并发网络传输、重试、进度上报和原子写入
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `USER_AGENT`
//! - `download_tasks`
//! - `download_small_file`
//! - `download_asset_index`
//! - `backoff`
//! - `agent`
//! - `cache_directory`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 实现中包含线程、通道或共享状态；同步边界必须与现有轮询和生命周期保持一致。
//!
//! ## 安全与平台
//! - 本文件位于外部数据或文件写入边界，不得绕过 URL、路径、哈希和大小限制。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use sha1_smol::Sha1;

use crate::brand;

use super::filesystem::{part_path, replace_file};
use super::manifest::fetch_cached_text;
use super::model::{DownloadTask, Progress};
use super::plan::download_is_complete;
use super::security::sha1_file;

/// 提供 USERAGENT 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub(super) const USER_AGENT: &str = brand::USER_AGENT;
/// 提供 WORKERS 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const WORKERS: usize = 6;

/// 实现 下载tasks 相关的业务流程。
///
/// # 参数
/// - `tasks`：由调用方传入的tasks。
/// - `progress`：由调用方传入的进度。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn download_tasks(
    _game_directory: &Path,
    tasks: &[DownloadTask],
    progress: Arc<Progress>,
) -> Result<(), String> {
    if tasks.is_empty() {
        return Ok(());
    }
    let cursor = Arc::new(AtomicUsize::new(0));
    let failed = Arc::new(AtomicBool::new(false));
    let error = Arc::new(Mutex::new(None));
    thread::scope(|scope| {
        for _ in 0..WORKERS.min(tasks.len()) {
            let cursor = cursor.clone();
            let failed = failed.clone();
            let error = error.clone();
            let progress = progress.clone();
            scope.spawn(move || {
                let agent = agent();
                while !failed.load(Ordering::Relaxed) {
                    let index = cursor.fetch_add(1, Ordering::Relaxed);
                    let Some(task) = tasks.get(index) else {
                        break;
                    };
                    if let Err(download_error) = download_task(&agent, task, &progress) {
                        failed.store(true, Ordering::Relaxed);
                        *error.lock().unwrap() = Some(download_error);
                        break;
                    }
                    progress.files_done.fetch_add(1, Ordering::Relaxed);
                    progress.send(true, "");
                }
            });
        }
    });
    if let Some(error) = error.lock().unwrap().clone() {
        return Err(error);
    }
    Ok(())
}

/// 实现 下载task 相关的业务流程。
///
/// # 参数
/// - `agent`：由调用方传入的agent。
/// - `task`：由调用方传入的task。
/// - `progress`：由调用方传入的进度。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn download_task(
    agent: &ureq::Agent,
    task: &DownloadTask,
    progress: &Progress,
) -> Result<(), String> {
    if download_is_complete(task) {
        return Ok(());
    }
    if let Some(parent) = task.path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建下载目录失败：{error}"))?;
    }
    let part = part_path(&task.path);
    let _ = fs::remove_file(&part);

    let mut last_error = String::new();
    for attempt in 0..3 {
        let mut counted_bytes = 0_u64;
        let result = (|| -> Result<(), String> {
            let response = agent
                .get(&task.url)
                .set("User-Agent", USER_AGENT)
                .call()
                .map_err(|error| error.to_string())?;
            let expected_size = response
                .header("content-length")
                .and_then(|value| value.parse::<u64>().ok());
            if task.size > 0 && expected_size.is_some_and(|size| size != task.size) {
                return Err("服务器返回的文件大小不正确".to_owned());
            }
            let mut reader = response.into_reader();
            let mut file =
                File::create(&part).map_err(|error| format!("创建临时下载文件失败：{error}"))?;
            let mut buffer = [0_u8; 64 * 1024];
            loop {
                let read = reader
                    .read(&mut buffer)
                    .map_err(|error| format!("读取下载数据失败：{error}"))?;
                if read == 0 {
                    break;
                }
                file.write_all(&buffer[..read])
                    .map_err(|error| format!("写入下载文件失败：{error}"))?;
                counted_bytes = counted_bytes.saturating_add(read as u64);
                progress
                    .completed_bytes
                    .fetch_add(read as u64, Ordering::Relaxed);
                progress.send(false, "");
            }
            file.flush()
                .map_err(|error| format!("刷新下载文件失败：{error}"))?;
            if task.size > 0
                && fs::metadata(&part)
                    .map_err(|error| format!("读取下载文件信息失败：{error}"))?
                    .len()
                    != task.size
            {
                return Err("下载文件大小校验失败".to_owned());
            }
            if let Some(expected) = &task.sha1 {
                let actual = sha1_file(&part)?;
                if !actual.eq_ignore_ascii_case(expected) {
                    return Err("SHA1 校验失败".to_owned());
                }
            }
            replace_file(&part, &task.path)
        })();
        match result {
            Ok(()) => return Ok(()),
            Err(error) => {
                progress.completed_bytes.fetch_sub(
                    counted_bytes.min(progress.completed_bytes.load(Ordering::Relaxed)),
                    Ordering::Relaxed,
                );
                last_error = error;
            }
        }
        let _ = fs::remove_file(&part);
        if attempt < 2 {
            thread::sleep(backoff(attempt));
        }
    }
    Err(format!("下载失败 {}：{last_error}", task.url))
}

/// 实现 下载small文件 相关的业务流程。
///
/// # 参数
/// - `path`：由调用方传入的路径。
/// - `sha1`：由调用方传入的sha1。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn download_small_file(
    url: &str,
    path: &Path,
    sha1: Option<&str>,
) -> Result<(), String> {
    if path.is_file()
        && sha1.is_none_or(|expected| {
            sha1_file(path).is_ok_and(|actual| actual.eq_ignore_ascii_case(expected))
        })
    {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("无法创建下载目录：{error}"))?;
    }
    let part = part_path(path);
    let agent = agent();
    let mut last_error = String::new();
    for attempt in 0..3 {
        let result = (|| -> Result<(), String> {
            let response = agent
                .get(url)
                .set("User-Agent", USER_AGENT)
                .call()
                .map_err(|error| error.to_string())?;
            let mut reader = response.into_reader();
            let mut bytes = Vec::new();
            reader
                .read_to_end(&mut bytes)
                .map_err(|error| format!("读取下载数据失败：{error}"))?;
            if let Some(expected) = sha1 {
                let actual = Sha1::from(&bytes).digest().to_string();
                if !actual.eq_ignore_ascii_case(expected) {
                    return Err("SHA1 校验失败".to_owned());
                }
            }
            let mut file =
                File::create(&part).map_err(|error| format!("创建临时文件失败：{error}"))?;
            file.write_all(&bytes)
                .map_err(|error| format!("写入临时文件失败：{error}"))?;
            file.flush()
                .map_err(|error| format!("刷新临时文件失败：{error}"))?;
            replace_file(&part, path)
        })();
        match result {
            Ok(()) => return Ok(()),
            Err(error) => last_error = error,
        }
        let _ = fs::remove_file(&part);
        if attempt < 2 {
            thread::sleep(backoff(attempt));
        }
    }
    Err(format!("下载失败 {url}：{last_error}"))
}

/// 实现 下载assetindex 相关的业务流程。
///
/// # 参数
/// - `path`：由调用方传入的路径。
/// - `sha1`：由调用方传入的sha1。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn download_asset_index(url: &str, path: &Path, sha1: &str) -> Result<(), String> {
    let mut etag_name = path.as_os_str().to_os_string();
    etag_name.push(".etag");
    let etag_path = PathBuf::from(etag_name);
    let body = fetch_cached_text(url, path, &etag_path, "资源索引")?;
    let actual = Sha1::from(body.as_bytes()).digest().to_string();
    if actual.eq_ignore_ascii_case(sha1) {
        return Ok(());
    }
    let _ = fs::remove_file(etag_path);
    download_small_file(url, path, Some(sha1))
}

/// 实现 backoff 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Duration`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn backoff(attempt: usize) -> Duration {
    Duration::from_millis(400_u64.saturating_mul(1_u64 << attempt))
}

/// 实现 agent 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `ureq::Agent`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(Duration::from_secs(60))
        .build()
}

/// 实现 缓存目录 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `PathBuf`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn cache_directory() -> PathBuf {
    brand::data_directory().join("cache").join("mojang")
}
