//! # `src/loader/mod.rs`
//!
//! **功能域：** Fabric、Forge 和 NeoForge 的版本查询、下载与安装。
//!
//! 组织本模块声明并导出稳定公共接口
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

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;

use serde_json::Value;

use crate::brand;

/// 提供 USERAGENT 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const USER_AGENT: &str = brand::USER_AGENT;

mod install;
mod model;
mod network;
mod util;
mod versions;

pub(crate) use install::start_install;
pub(crate) use model::{LoaderInstallUpdate, LoaderKind, display_version};
pub(crate) use versions::start_fetch_versions;

#[cfg(test)]
mod tests {
    use super::util::{maven_path, neoforge_prefix, validate_component, xml_versions};
    use super::versions::{parse_bmcl_forge_versions, parse_bmcl_maven_directories};
    use super::*;

    #[test]
    /// 实现 buildsmavenpaths 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn builds_maven_paths() {
        assert_eq!(
            maven_path("org.ow2.asm:asm:9.7.1").unwrap(),
            "org/ow2/asm/asm/9.7.1/asm-9.7.1.jar"
        );
        assert_eq!(
            maven_path("net.fabricmc:sponge-mixin:0.15.4+mixin.0.8.7").unwrap(),
            "net/fabricmc/sponge-mixin/0.15.4+mixin.0.8.7/sponge-mixin-0.15.4+mixin.0.8.7.jar"
        );
    }

    #[test]
    /// 实现 parsesmavenmetadata 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn parses_maven_metadata() {
        let xml = "<metadata><versioning><versions><version>1.21.1-52.0.1</version><version>1.21.1-52.0.2</version></versions></versioning></metadata>";
        assert_eq!(xml_versions(xml), vec!["1.21.1-52.0.1", "1.21.1-52.0.2"]);
    }

    #[test]
    /// 实现 filtersneoforge版本 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn filters_neoforge_versions() {
        assert_eq!(neoforge_prefix("1.21.1").unwrap(), "21.1.");
        assert_eq!(neoforge_prefix("1.21.11").unwrap(), "21.11.");
        assert_eq!(neoforge_prefix("26.3").unwrap(), "26.3.");
        assert_eq!(neoforge_prefix("26.3-snapshot-8").unwrap(), "26.3.");
        assert_eq!(neoforge_prefix("1.20.1").unwrap(), "1.20.1-47.");
        assert!(validate_component("../bad").is_err());
    }

    #[test]
    /// 实现 displays加载器版本withoutMinecraftprefix 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn displays_loader_version_without_minecraft_prefix() {
        assert_eq!(display_version("1.21.1", "1.21.1-52.1.20"), "52.1.20");
        assert_eq!(display_version("1.20.1", "1.20.1-47.1.106"), "47.1.106");
        assert_eq!(display_version("1.21.1", "21.1.219"), "21.1.219");
    }

    #[test]
    /// 实现 keepsonlyinstallablebmclforge版本 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn keeps_only_installable_bmcl_forge_versions() {
        let body = r#"[
          {
            "mcversion": "1.21.1",
            "version": "52.1.20",
            "files": [
              {"category": "installer", "format": "jar"},
              {"category": "mdk", "format": "zip"}
            ]
          },
          {
            "mcversion": "1.21.1",
            "version": "51.0.0",
            "files": [
              {"category": "mdk", "format": "zip"}
            ]
          }
        ]"#;
        assert_eq!(
            parse_bmcl_forge_versions(body, "1.21.1").unwrap(),
            vec!["1.21.1-52.1.20"]
        );
    }

    #[test]
    /// 实现 parsesbmclneoforgedirectories 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn parses_bmcl_neoforge_directories() {
        let body = r#"{
          "files": [
            {"name": "21.1.219", "type": "DIRECTORY"},
            {"name": "21.1.220-beta", "type": "DIRECTORY"},
            {"name": "maven-metadata.xml", "type": "FILE"}
          ]
        }"#;
        assert_eq!(
            parse_bmcl_maven_directories(body).unwrap(),
            vec!["21.1.219", "21.1.220-beta"]
        );
    }
}
