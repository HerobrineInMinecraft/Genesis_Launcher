//! # `src/resources/details.rs`
//!
//! **功能域：** Modrinth 资源搜索、详情解析、镜像测速和内容下载。
//!
//! 解析详情响应并选出满足条件的下载文件
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `select_download_file`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use serde_json::Value;

use super::model::ResourceDownloadFile;
use super::util::string_array;

/// 实现 选择下载文件 相关的业务流程。
///
/// # 参数
/// - `title`：由调用方传入的title。
/// - `game_version`：由调用方传入的游戏版本。
/// - `loader`：由调用方传入的加载器。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn select_download_file(
    body: &str,
    title: &str,
    game_version: &str,
    loader: Option<&str>,
) -> Result<ResourceDownloadFile, String> {
    let versions: Value = serde_json::from_str(body)
        .map_err(|error| format!("Modrinth 版本列表格式无效：{error}"))?;
    let versions = versions
        .as_array()
        .ok_or_else(|| "Modrinth 版本列表无效".to_owned())?;
    let loader = loader.map(str::to_lowercase);
    let selected = versions
        .iter()
        .find(|version| {
            string_array(version.get("game_versions"))
                .iter()
                .any(|v| v == game_version)
                && loader.as_ref().is_none_or(|wanted| {
                    string_array(version.get("loaders"))
                        .iter()
                        .any(|actual| actual.eq_ignore_ascii_case(wanted))
                })
        })
        .or_else(|| {
            versions.iter().find(|version| {
                string_array(version.get("game_versions"))
                    .iter()
                    .any(|value| value == game_version)
            })
        })
        .ok_or_else(|| format!("{title} 没有适用于 Minecraft {game_version} 的版本"))?;
    let file = selected
        .get("files")
        .and_then(Value::as_array)
        .and_then(|files| {
            files
                .iter()
                .find(|file| file.get("primary").and_then(Value::as_bool) == Some(true))
                .or_else(|| files.first())
        })
        .ok_or_else(|| "Modrinth 版本缺少下载文件".to_owned())?;
    let url = file
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| "Modrinth 下载地址缺失".to_owned())?;
    let sha1 = file
        .pointer("/hashes/sha1")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Ok(ResourceDownloadFile {
        url: url.to_owned(),
        sha1: sha1.to_owned(),
    })
}
