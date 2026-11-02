//! # `src/resources/download.rs`
//!
//! **功能域：** Modrinth 资源搜索、详情解析、镜像测速和内容下载。
//!
//! 下载文件、校验内容并提交到目标位置
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `start_download`
//! - `sanitize_download_filename`
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
use std::sync::mpsc;
use std::thread;

use sha1_smol::Sha1;

use super::details::select_download_file;
use super::mirror::{request_bytes_with_mirror, request_text_with_mirror};
use super::model::{ResourceCategory, ResourceProject};
use super::network::allowed_url;
use super::util::escape_query;

/// 实现 启动下载 相关的业务流程。
///
/// # 参数
/// - `project`：由调用方传入的项目。
/// - `category`：由调用方传入的category。
/// - `game_version`：由调用方传入的游戏版本。
/// - `loader`：由调用方传入的加载器。
/// - `destination_file`：由调用方传入的destination文件。
/// # 返回
/// 返回 `mpsc::Receiver<Result<PathBuf, String>>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn start_download(
    project: ResourceProject,
    category: ResourceCategory,
    game_version: String,
    loader: Option<String>,
    destination_file: PathBuf,
) -> mpsc::Receiver<Result<PathBuf, String>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = download(
            &project,
            category,
            &game_version,
            loader.as_deref(),
            &destination_file,
        );
        let _ = sender.send(result);
    });
    receiver
}

/// 实现 下载 相关的业务流程。
///
/// # 参数
/// - `project`：由调用方传入的项目。
/// - `_category`：由调用方传入的category。
/// - `game_version`：由调用方传入的游戏版本。
/// - `loader`：由调用方传入的加载器。
/// - `destination_file`：由调用方传入的destination文件。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn download(
    project: &ResourceProject,
    _category: ResourceCategory,
    game_version: &str,
    loader: Option<&str>,
    destination_file: &Path,
) -> Result<PathBuf, String> {
    let url = format!(
        "https://api.modrinth.com/v2/project/{}/version",
        escape_query(&project.id)
    );
    let body = request_text_with_mirror(&url)?;
    let file = select_download_file(&body, &project.title, game_version, loader)?;
    if !allowed_url(&file.url) {
        return Err("拒绝非 Modrinth 官方下载地址".to_owned());
    }
    let target_dir = destination_file.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(target_dir).map_err(|error| format!("无法创建下载目录：{error}"))?;
    let target = destination_file.to_path_buf();
    let part = target.with_extension("part");
    let bytes = request_bytes_with_mirror(&file.url)?;
    if file.sha1.len() == 40
        && !Sha1::from(&bytes)
            .digest()
            .to_string()
            .eq_ignore_ascii_case(&file.sha1)
    {
        return Err("下载文件 SHA1 校验失败".to_owned());
    }
    fs::write(&part, &bytes).map_err(|error| format!("写入下载文件失败：{error}"))?;
    if target.exists() {
        fs::remove_file(&target).map_err(|error| format!("替换旧文件失败：{error}"))?;
    }
    fs::rename(&part, &target).map_err(|error| format!("提交下载文件失败：{error}"))?;
    Ok(target)
}

/// 实现 sanitize下载filename 相关的业务流程。
///
/// # 参数
/// - `value`：由调用方传入的value。
/// # 返回
/// 返回 `String`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn sanitize_download_filename(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
            {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    if sanitized.trim_matches(['.', ' ']).is_empty() {
        "download".to_owned()
    } else {
        sanitized
    }
}
