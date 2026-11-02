//! # `src/exporter/mod.rs`
//!
//! **功能域：** 版本和整合包导出、文件收集、哈希计算与压缩包生成。
//!
//! 组织本模块声明并导出稳定公共接口
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `start_modpack_export_worker`
//! - `export_version_archive`
//! - `sanitize_filename`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 实现中包含线程、通道或共享状态；同步边界必须与现有轮询和生命周期保持一致。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::versions::GameVersion;

mod archive;
mod collect;
mod hash;
mod model;
mod modrinth;
mod picker;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use archive::export_pack_with_hosted_files;
#[cfg(test)]
pub(crate) use collect::collect_modpack_files;
#[cfg(test)]
pub(crate) use model::HostedPackFile;
pub(crate) use model::{ModpackExportFormat, VersionExportOptions};
pub(crate) use picker::pick_export_file;

/// 实现 启动整合包导出worker 相关的业务流程。
///
/// # 参数
/// - `data_directory`：由调用方传入的数据目录。
/// - `version`：由调用方传入的版本。
/// - `destination`：由调用方传入的destination。
/// - `options`：由调用方传入的options。
/// # 返回
/// 返回 `std::sync::mpsc::Receiver<Result<PathBuf, String>>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn start_modpack_export_worker(
    game_directory: PathBuf,
    data_directory: PathBuf,
    version: GameVersion,
    destination: PathBuf,
    options: VersionExportOptions,
) -> std::sync::mpsc::Receiver<Result<PathBuf, String>> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = export_version_archive(
            &game_directory,
            &data_directory,
            &version,
            &destination,
            &options,
        )
        .map(|()| destination);
        let _ = sender.send(result);
    });
    receiver
}

/// 实现 导出版本压缩包 相关的业务流程。
///
/// # 参数
/// - `data_directory`：由调用方传入的数据目录。
/// - `version`：由调用方传入的版本。
/// - `destination`：由调用方传入的destination。
/// - `options`：由调用方传入的options。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn export_version_archive(
    game_directory: &Path,
    data_directory: &Path,
    version: &GameVersion,
    destination: &Path,
    options: &VersionExportOptions,
) -> Result<(), String> {
    let files = collect::collect_modpack_files(data_directory, options)?;
    let hosted = if options.format == ModpackExportFormat::Modrinth {
        modrinth::query_hosted_files(&files)
    } else {
        HashMap::new()
    };
    // Modrinth 的 `dependencies.minecraft` 必须是 Mojang 官方版本号，不能写入
    // “愚者”之类的本地 profile 名称。优先从版本 JSON 继承链解析；无法解析时，
    // 再使用 GameVersion 中已经规范化过的 game_version。
    let mut export_version = version.clone();
    if let Some(base) =
        crate::game::official_base_version_for_profile(game_directory, &version.version)
    {
        export_version.game_version = if crate::game::looks_like_official_version(&base) {
            base
        } else if crate::game::looks_like_official_version(&version.game_version) {
            version.game_version.clone()
        } else {
            base
        };
    }
    archive::export_pack_with_hosted_files(&export_version, destination, options, &files, &hosted)
}

/// 实现 sanitizefilename 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `String`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn sanitize_filename(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .collect()
}
