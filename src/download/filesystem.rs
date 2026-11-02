//! # `src/download/filesystem.rs`
//!
//! **功能域：** Minecraft 官方清单、客户端、资源、Java 运行时安装与完整性校验。
//!
//! 提供临时文件、原子替换、解压和目录整理能力
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `part_path`
//! - `replace_file`
//! - `extract_native`
//! - `materialize_indexed_assets`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件位于外部数据或文件写入边界，不得绕过 URL、路径、哈希和大小限制。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use serde_json::Value;
use zip::ZipArchive;

use super::security::safe_join;

/// 实现 part路径 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `PathBuf`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn part_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".part");
    PathBuf::from(name)
}

/// 实现 replace文件 相关的业务流程。
///
/// # 参数
/// - `target`：由调用方传入的target。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn replace_file(part: &Path, target: &Path) -> Result<(), String> {
    if target.exists() {
        fs::remove_file(target).map_err(|error| format!("替换旧下载文件失败：{error}"))?;
    }
    fs::rename(part, target).map_err(|error| format!("提交下载文件失败：{error}"))
}

/// 实现 extractnative 相关的业务流程。
///
/// # 参数
/// - `natives_dir`：由调用方传入的nativesdir。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn extract_native(path: &Path, natives_dir: &Path) -> Result<(), String> {
    let file = File::open(path).map_err(|error| format!("无法打开 natives：{error}"))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("无法读取 natives：{error}"))?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("读取 natives 条目失败：{error}"))?;
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        if !relative
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"))
        {
            continue;
        }
        let output = natives_dir.join(relative.file_name().unwrap_or(relative.as_os_str()));
        let mut target =
            File::create(output).map_err(|error| format!("写入 natives 失败：{error}"))?;
        io::copy(&mut entry, &mut target).map_err(|error| format!("解压 natives 失败：{error}"))?;
    }
    Ok(())
}

/// 实现 materializeindexedassets 相关的业务流程。
///
/// # 参数
/// - `index`：由调用方传入的index。
/// - `target_root`：由调用方传入的targetroot。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn materialize_indexed_assets(
    game_directory: &Path,
    index: &Value,
    target_root: &Path,
) -> Result<(), String> {
    let Some(objects) = index.get("objects").and_then(Value::as_object) else {
        return Ok(());
    };
    for (name, object) in objects {
        let Some(hash) = object.get("hash").and_then(Value::as_str) else {
            continue;
        };
        if hash.len() < 2 {
            continue;
        }
        let source = game_directory
            .join("assets")
            .join("objects")
            .join(&hash[..2])
            .join(hash);
        let target = safe_join(target_root, name)?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).ok();
        }
        if source.is_file() {
            let _ = fs::copy(source, target);
        }
    }
    Ok(())
}
