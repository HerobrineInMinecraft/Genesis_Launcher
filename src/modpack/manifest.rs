//! # `src/modpack/manifest.rs`
//!
//! **功能域：** Modrinth 整合包读取、依赖补全、文件下载和覆盖层解压。
//!
//! 读取远端清单并维护本地缓存
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `read_manifest`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::fs::File;
use std::io::Read;
use std::path::Path;

use zip::ZipArchive;

use super::manifest_parse::parse_manifest;
use super::model::PackManifest;

/// 实现 读取manifest 相关的业务流程。
///
/// # 参数
/// - `archive_path`：由调用方传入的压缩包路径。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn read_manifest(
    archive: &mut ZipArchive<File>,
    archive_path: &Path,
) -> Result<PackManifest, String> {
    let mut index_path = None;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| format!("读取压缩包条目失败：{error}"))?;
        let name = entry.name().replace('\\', "/");
        if name.ends_with("modrinth.index.json") {
            index_path = Some(name);
            break;
        }
    }
    let index_path = index_path
        .ok_or_else(|| "当前仅支持 Modrinth 整合包（需要 modrinth.index.json）".to_owned())?;
    let mut contents = String::new();
    archive
        .by_name(&index_path)
        .map_err(|error| format!("读取整合包索引失败：{error}"))?
        .read_to_string(&mut contents)
        .map_err(|error| format!("解析整合包索引失败：{error}"))?;
    parse_manifest(&contents, &index_path, archive_path)
}
