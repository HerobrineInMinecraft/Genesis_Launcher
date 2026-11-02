//! # `src/favorites/urls.rs`
//!
//! **功能域：** 收藏站点模型、地址规范化、图标抓取和外部链接打开。
//!
//! 规范化网站地址、图标来源和域名
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `normalize_site_url`
//! - `normalize_icon_source`
//! - `site_domain`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

/// 实现 normalizesite地址 相关的业务流程。
///
/// # 参数
/// - `value`：由调用方传入的value。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn normalize_site_url(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("请输入网站地址".to_owned());
    }
    let candidate = if trimmed.contains("://") {
        trimmed.to_owned()
    } else {
        format!("https://{trimmed}")
    };
    let lower = candidate.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return Err("只支持 HTTP 或 HTTPS 网站".to_owned());
    }
    if candidate
        .chars()
        .any(|character| character.is_whitespace() || character.is_control())
    {
        return Err("网站地址不能包含空格或控制字符".to_owned());
    }
    let (_, rest) = candidate
        .split_once("://")
        .ok_or_else(|| "网站地址格式无效".to_owned())?;
    let host = rest.split('/').next().unwrap_or_default();
    if host.is_empty() || !host.contains('.') {
        return Err("网站地址缺少有效域名".to_owned());
    }
    Ok(candidate)
}

/// 实现 normalize图标source 相关的业务流程。
///
/// # 参数
/// - `value`：由调用方传入的value。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn normalize_icon_source(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") {
        return normalize_site_url(trimmed);
    }
    let path = std::path::PathBuf::from(trimmed);
    if path.is_file() {
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "webp" | "ico") {
            return Err("本地图标只支持 PNG、JPG、WebP 或 ICO".to_owned());
        }
        if path
            .metadata()
            .map(|metadata| metadata.len() > 4 * 1024 * 1024)
            .unwrap_or(true)
        {
            return Err("本地图标不能超过 4 MB".to_owned());
        }
        return path
            .canonicalize()
            .map(|path| path.to_string_lossy().into_owned())
            .map_err(|error| format!("无法读取本地图标：{error}"));
    }
    if !trimmed.contains('\\') && trimmed.contains('.') {
        normalize_site_url(trimmed)
    } else {
        Err("请输入完整的图标 URL，或选择存在的本地图片".to_owned())
    }
}

/// 实现 sitedomain 相关的业务流程。
///
/// # 参数
/// - `url`：由调用方传入的地址。
/// # 返回
/// 返回 `String`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn site_domain(url: &str) -> String {
    url.split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or(url)
        .to_ascii_lowercase()
}
