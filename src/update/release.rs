//! # `src/update/release.rs`
//!
//! **功能域：** Genesis Launcher GitHub Release 检查、下载和程序替换。
//!
//! 解析 GitHub Release 响应和 Windows 资源
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `RELEASES_URL`
//! - `LATEST_RELEASE_PATH`
//! - `parse_release_body`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件位于外部数据或文件写入边界，不得绕过 URL、路径、哈希和大小限制。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use serde_json::Value;

use super::model::{LauncherRelease, ReleaseAsset};

/// 提供 RELEASES地址 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub const RELEASES_URL: &str = "https://github.com/HerobrineInMinecraft/Genesis_Launcher/releases";
/// 提供 最新发行版路径 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub(super) const LATEST_RELEASE_PATH: &str =
    "/repos/HerobrineInMinecraft/Genesis_Launcher/releases/latest";

/// 实现 解析发行版主体 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn parse_release_body(body: &[u8]) -> Result<LauncherRelease, String> {
    let value: Value =
        serde_json::from_slice(body).map_err(|error| format!("更新信息格式无效：{error}"))?;
    parse_release(&value)
}

/// 实现 解析发行版 相关的业务流程。
///
/// # 参数
/// - `value`：由调用方传入的value。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn parse_release(value: &Value) -> Result<LauncherRelease, String> {
    let version = value
        .get("tag_name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .ok_or_else(|| "GitHub Release 缺少版本标签".to_owned())?;
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(version);
    let page_url = value
        .get("html_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| url.starts_with(RELEASES_URL))
        .unwrap_or(RELEASES_URL);
    let asset = value
        .get("assets")
        .and_then(Value::as_array)
        .and_then(|assets| {
            assets
                .iter()
                .filter_map(parse_asset)
                .max_by_key(|asset| asset.size)
        });
    Ok(LauncherRelease {
        version: version.trim_start_matches(['v', 'V']).to_owned(),
        name: name.to_owned(),
        page_url: page_url.to_owned(),
        asset,
    })
}

/// 实现 解析asset 相关的业务流程。
///
/// # 参数
/// - `value`：由调用方传入的value。
/// # 返回
/// 返回 `Option<ReleaseAsset>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn parse_asset(value: &Value) -> Option<ReleaseAsset> {
    let name = value.get("name")?.as_str()?.trim();
    if !name.to_ascii_lowercase().ends_with(".exe") {
        return None;
    }
    let url = value.get("browser_download_url")?.as_str()?.trim();
    if !url
        .starts_with("https://github.com/HerobrineInMinecraft/Genesis_Launcher/releases/download/")
    {
        return None;
    }
    let sha256 = value
        .get("digest")
        .and_then(Value::as_str)
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .map(str::to_ascii_lowercase);
    Some(ReleaseAsset {
        name: name.to_owned(),
        url: url.to_owned(),
        size: value.get("size").and_then(Value::as_u64).unwrap_or(0),
        sha256,
    })
}
