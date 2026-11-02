//! # `src/modpack/download.rs`
//!
//! **功能域：** Modrinth 整合包读取、依赖补全、文件下载和覆盖层解压。
//!
//! 下载整合包声明的文件，校验 SHA1 并原子提交到版本目录。
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `download_files`
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
use std::sync::mpsc;

use sha1_smol::Sha1;

use super::model::{ModpackImportUpdate, PackFile};
use super::network::download_urls;
use super::progress::send_stage;

/// 实现 下载文件 相关的业务流程。
///
/// # 参数
/// - `version_directory`：由调用方传入的版本目录。
/// - `sender`：由调用方传入的sender。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn download_files(
    files: &[PackFile],
    version_directory: &Path,
    sender: &mpsc::Sender<ModpackImportUpdate>,
) -> Result<(), String> {
    for (index, file) in files.iter().enumerate() {
        let target = version_directory.join(&file.path);
        if target
            .is_file()
            .then(|| fs::read(&target).ok())
            .flatten()
            .is_some_and(|bytes| hash_matches(&bytes, &file.sha1))
        {
            continue;
        }
        send_stage(
            sender,
            "下载整合包文件",
            format!("{}/{} · {}", index + 1, files.len(), file.path.display()),
        );
        let bytes = download_urls(&file.urls)?;
        if !hash_matches(&bytes, &file.sha1) {
            return Err(format!("{} SHA1 校验失败", file.path.display()));
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("创建下载目录失败：{error}"))?;
        }
        let part = target.with_extension("part");
        fs::write(&part, bytes).map_err(|error| format!("写入下载文件失败：{error}"))?;
        if target.exists() {
            fs::remove_file(&target).map_err(|error| format!("替换旧文件失败：{error}"))?;
        }
        fs::rename(&part, &target).map_err(|error| format!("提交下载文件失败：{error}"))?;
    }
    Ok(())
}

/// 实现 hashmatches 相关的业务流程。
///
/// # 参数
/// - `bytes`：由调用方传入的bytes。
/// - `expected`：由调用方传入的expected。
/// # 返回
/// 返回 `bool`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn hash_matches(bytes: &[u8], expected: &str) -> bool {
    Sha1::from(bytes)
        .digest()
        .to_string()
        .eq_ignore_ascii_case(expected)
}
