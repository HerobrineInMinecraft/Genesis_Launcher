//! # `src/modpack/overrides.rs`
//!
//! **功能域：** Modrinth 整合包读取、依赖补全、文件下载和覆盖层解压。
//!
//! 安全地把整合包覆盖层写入版本目录
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `extract_overrides`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::fs::{self, File};
use std::path::Path;

use zip::ZipArchive;

use super::security::safe_relative_path;

/// 实现 extractoverrides 相关的业务流程。
///
/// # 参数
/// - `archive_prefix`：由调用方传入的压缩包prefix。
/// - `version_directory`：由调用方传入的版本目录。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn extract_overrides(
    archive: &mut ZipArchive<File>,
    archive_prefix: &str,
    version_directory: &Path,
) -> Result<(), String> {
    let prefixes = [
        format!("{archive_prefix}overrides/"),
        format!("{archive_prefix}client-overrides/"),
    ];
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("读取覆盖文件失败：{error}"))?;
        let entry_name = entry.name().replace('\\', "/");
        let Some(relative) = prefixes
            .iter()
            .find_map(|prefix| entry_name.strip_prefix(prefix))
        else {
            continue;
        };
        let Some(relative) = safe_relative_path(relative) else {
            return Err(format!("整合包含有越界覆盖路径：{}", entry.name()));
        };
        let target = version_directory.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(|error| format!("创建覆盖目录失败：{error}"))?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("创建覆盖文件目录失败：{error}"))?;
        }
        let mut file =
            File::create(&target).map_err(|error| format!("写入覆盖文件失败：{error}"))?;
        std::io::copy(&mut entry, &mut file)
            .map_err(|error| format!("解压覆盖文件失败：{error}"))?;
    }
    Ok(())
}
