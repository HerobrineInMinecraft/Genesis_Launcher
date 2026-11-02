//! # `src/download/tests.rs`
//!
//! **功能域：** Minecraft 官方清单、客户端、资源、Java 运行时安装与完整性校验。
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

use std::path::Path;

use super::security::{ensure_allowed_url, is_sha1, safe_join, validate_path_component};

#[test]
/// 实现 acceptsonlymojang下载hosts 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn accepts_only_mojang_download_hosts() {
    for url in [
        "https://piston-meta.mojang.com/v1/packages/a/manifest.json",
        "https://piston-data.mojang.com/v1/objects/a/file",
        "https://launchermeta.mojang.com/v1/products/java-runtime/a/all.json",
        "https://libraries.minecraft.net/org/example/a.jar",
        "https://resources.download.minecraft.net/ab/abcdef",
    ] {
        assert!(ensure_allowed_url(url).is_ok(), "{url}");
    }
    for url in [
        "http://piston-meta.mojang.com/file",
        "https://piston-meta.mojang.com.example.org/file",
        "https://example.org/file",
    ] {
        assert!(ensure_allowed_url(url).is_err(), "{url}");
    }
}

#[test]
/// 实现 rejectsunsafe下载paths 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn rejects_unsafe_download_paths() {
    let root = Path::new("C:\\game");
    assert!(safe_join(root, "versions/1.21/client.jar").is_ok());
    for path in [
        "C:\\Windows\\file",
        "../outside",
        "versions/../../outside",
        "versions\\..\\outside",
    ] {
        assert!(safe_join(root, path).is_err(), "{path}");
    }
    for component in ["../version", "C:", "a/b", "..", ".", ""] {
        assert!(validate_path_component(component).is_err(), "{component}");
    }
    assert!(validate_path_component("26.3").is_ok());
    assert!(validate_path_component("26.3-snapshot-8").is_ok());
}

#[test]
/// 实现 recognizesonlycompletesha1values 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn recognizes_only_complete_sha1_values() {
    assert!(is_sha1("0123456789abcdef0123456789abcdef01234567"));
    assert!(!is_sha1("012345"));
    assert!(!is_sha1("z123456789abcdef0123456789abcdef01234567"));
}
