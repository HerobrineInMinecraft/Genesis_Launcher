//! # `src/update/github_api.rs`
//!
//! **功能域：** Genesis Launcher GitHub Release 检查、下载和程序替换。
//!
//! 在非 Windows 平台访问 GitHub API 和下载资源
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `latest_release_body`
//! - `download_release_asset`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件位于外部数据或文件写入边界，不得绕过 URL、路径、哈希和大小限制。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::io::Read;
use std::time::Duration;

use crate::brand;

use super::model::ReleaseAsset;

/// 实现 最新发行版主体 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn latest_release_body(path: &str) -> Result<Vec<u8>, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(8))
        .timeout_read(Duration::from_secs(15))
        .build();
    let response = agent
        .get(&format!("https://api.github.com{path}"))
        .set("User-Agent", brand::USER_AGENT)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|error| match error {
            ureq::Error::Status(404, _) => "GitHub 仓库暂未发布正式版本".to_owned(),
            _ => format!("无法连接 GitHub Releases：{error}"),
        })?;
    let mut body = Vec::new();
    response
        .into_reader()
        .take(2 * 1024 * 1024)
        .read_to_end(&mut body)
        .map_err(|error| format!("读取更新信息失败：{error}"))?;
    Ok(body)
}

/// 实现 下载发行版asset 相关的业务流程。
///
/// # 参数
/// - `on_progress`：由调用方传入的进度。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn download_release_asset(
    asset: &ReleaseAsset,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<Vec<u8>, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(30))
        .build();
    let mut reader = agent
        .get(&asset.url)
        .set("User-Agent", brand::USER_AGENT)
        .call()
        .map_err(|error| format!("无法下载更新文件：{error}"))?
        .into_reader();
    let mut body = Vec::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("读取更新文件失败：{error}"))?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&buffer[..read]);
        on_progress(body.len() as u64, asset.size);
    }
    Ok(body)
}
