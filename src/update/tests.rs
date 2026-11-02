//! # `src/update/tests.rs`
//!
//! **功能域：** Genesis Launcher GitHub Release 检查、下载和程序替换。
//!
//! 验证本模块的关键规则、错误路径和回归行为
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - 本文件主要为同模块内部实现，不导出额外公共符号。
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件位于外部数据或文件写入边界，不得绕过 URL、路径、哈希和大小限制。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use super::release::parse_release_body;
use super::version::is_newer_version;

#[test]
/// 实现 compares发行版版本 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn compares_release_versions() {
    assert!(is_newer_version("0.1.0", "v0.1.1"));
    assert!(is_newer_version("0.1.0", "v0.2.0"));
    assert!(is_newer_version("1.9.0", "v1.10.0"));
    assert!(!is_newer_version("0.2.0", "v0.2.0"));
    assert!(is_newer_version("0.2.0-beta.1", "v0.2.0"));
}

#[test]
/// 实现 parsesgithub最新发行版 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn parses_github_latest_release() {
    let body = serde_json::json!({
        "tag_name": "v0.2.0",
        "name": "Genesis Launcher 0.2.0",
        "html_url": "https://github.com/HerobrineInMinecraft/Genesis_Launcher/releases/tag/v0.2.0",
        "assets": [{
            "name": "Genesis.Launcher.exe",
            "browser_download_url": "https://github.com/HerobrineInMinecraft/Genesis_Launcher/releases/download/v0.2.0/Genesis.Launcher.exe",
            "size": 1234,
            "digest": "sha256:ABCDEF"
        }]
    })
    .to_string();
    let release = parse_release_body(body.as_bytes()).unwrap();
    assert_eq!(release.version, "0.2.0");
    assert_eq!(release.name, "Genesis Launcher 0.2.0");
    assert!(release.page_url.contains("/releases/tag/v0.2.0"));
    let asset = release.asset.unwrap();
    assert_eq!(asset.name, "Genesis.Launcher.exe");
    assert_eq!(asset.size, 1234);
    assert_eq!(asset.sha256.as_deref(), Some("abcdef"));
}
