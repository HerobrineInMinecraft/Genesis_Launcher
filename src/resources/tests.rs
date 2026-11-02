//! # `src/resources/tests.rs`
//!
//! **功能域：** Modrinth 资源搜索、详情解析、镜像测速和内容下载。
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
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::time::Duration;

use super::download::sanitize_download_filename;
use super::mirror::{ModrinthRoute, mcim_mirror_url};
use super::model::ResourceCategory;
use super::network::allowed_url;
use super::route::fastest_modrinth_route;
use super::search::search_facets;

#[test]
/// 实现 onlyallowsmodrinthhosts 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn only_allows_modrinth_hosts() {
    assert!(allowed_url("https://api.modrinth.com/v2/search"));
    assert!(allowed_url("https://cdn.modrinth.com/data/file.jar"));
    assert!(allowed_url("https://mod.mcimirror.top/modrinth/v2/search"));
    assert!(allowed_url("https://mod.mcimirror.top/data/file.jar"));
    assert!(!allowed_url("https://mod.mcimirror.top.evil/file.jar"));
    assert!(!allowed_url("https://example.org/file.jar"));
}

#[test]
/// 实现 mapsmodrinthurls到mcim镜像 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn maps_modrinth_urls_to_mcim_mirror() {
    assert_eq!(
        mcim_mirror_url("https://api.modrinth.com/v2/search?query=test").as_deref(),
        Some("https://mod.mcimirror.top/modrinth/v2/search?query=test")
    );
    assert_eq!(
        mcim_mirror_url("https://cdn.modrinth.com/data/a/b.jar").as_deref(),
        Some("https://mod.mcimirror.top/data/a/b.jar")
    );
}

#[test]
/// 实现 buildsmodrinthfacetswithtwolevels 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn builds_modrinth_facets_with_two_levels() {
    assert_eq!(
        search_facets(ResourceCategory::Mod, "1.21.1", Some("fabric")),
        r#" [["project_type:mod"],["versions:1.21.1"],["categories:fabric"]]"#.trim()
    );
}

#[test]
/// 实现 choosesthelowerlatencymodrinth线路 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn chooses_the_lower_latency_modrinth_route() {
    assert_eq!(
        fastest_modrinth_route(&[
            (ModrinthRoute::Official, Duration::from_millis(80)),
            (ModrinthRoute::Mirror, Duration::from_millis(140)),
        ]),
        ModrinthRoute::Official
    );
    assert_eq!(
        fastest_modrinth_route(&[
            (ModrinthRoute::Official, Duration::from_millis(620)),
            (ModrinthRoute::Mirror, Duration::from_millis(90)),
        ]),
        ModrinthRoute::Mirror
    );
    assert_eq!(fastest_modrinth_route(&[]), ModrinthRoute::Official);
}

#[test]
/// 实现 datapacksusethe版本datapacks目录 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn datapacks_use_the_version_datapacks_directory() {
    assert_eq!(ResourceCategory::Datapack.target_folder(), "datapacks");
}

#[test]
/// 实现 sanitizesinvalid下载文件names 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn sanitizes_invalid_download_file_names() {
    assert_eq!(sanitize_download_filename("bad:name?.jar"), "bad_name_.jar");
    assert_eq!(sanitize_download_filename("..."), "download");
}
