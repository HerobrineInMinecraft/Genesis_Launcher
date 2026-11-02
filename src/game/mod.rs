//! # `src/game/mod.rs`
//!
//! **功能域：** Minecraft 版本发现、Java 选择、启动参数组装和进程生命周期。
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

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use serde_json::Value;
use zip::ZipArchive;

use crate::brand;

/// 提供 OFFLINEWIDEUUID 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const OFFLINE_WIDE_UUID: &str = "00000000-0000-3000-8000-000000000001";
/// 提供 OFFLINESLIMUUID 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const OFFLINE_SLIM_UUID: &str = "00000000-0000-3000-8000-00000000000a";
/// 提供 OFFLINE名称 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const OFFLINE_NAME: &str = "Steve";

mod discovery;
mod java;
mod launch_args;
mod launch_core;
mod model;
mod process;
mod version;

pub(crate) use discovery::*;
pub(crate) use java::*;
pub(crate) use launch_core::*;
pub(crate) use model::*;
pub(crate) use process::*;
pub(crate) use version::load_merged_version;

#[cfg(test)]
mod tests {
    use super::launch_args::placeholders;
    use super::version::maven_library_path;
    use super::*;

    #[test]
    /// 实现 readsneoforgeargument版本 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn reads_neoforge_argument_version() {
        let value: Value = serde_json::from_str(
            r#"{
              "inheritsFrom": "1.21.1",
              "arguments": {
                "game": ["--fml.neoForgeVersion", "21.1.219"]
              }
            }"#,
        )
        .unwrap();
        assert_eq!(
            find_argument_value(&value, "--fml.neoForgeVersion"),
            Some("21.1.219".to_owned())
        );
    }

    #[test]
    /// 实现 readsforge版本从library名称 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn reads_forge_version_from_library_name() {
        let value: Value = serde_json::from_str(
            r#"{
              "libraries": [
                {"name": "net.minecraftforge:forge:1.21.1-52.1.20"}
              ]
            }"#,
        )
        .unwrap();
        assert_eq!(
            find_library_version(&value, &["net.minecraftforge:forge:"]),
            Some("1.21.1-52.1.20".to_owned())
        );
    }

    #[test]
    /// 实现 buildspathsformavenonlylibraries 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn builds_paths_for_maven_only_libraries() {
        let fabric: Value = serde_json::from_str(
            r#"{"name":"net.fabricmc:fabric-loader:0.19.5","url":"https://maven.fabricmc.net/"}"#,
        )
        .unwrap();
        assert_eq!(
            maven_library_path(&fabric).as_deref(),
            Some("net/fabricmc/fabric-loader/0.19.5/fabric-loader-0.19.5.jar")
        );
        let classifier: Value =
            serde_json::from_str(r#"{"name":"org.example:native:1.0:natives-windows"}"#).unwrap();
        assert_eq!(
            maven_library_path(&classifier).as_deref(),
            Some("org/example/native/1.0/native-1.0-natives-windows.jar")
        );
    }

    #[test]
    /// 实现 isolated数据目录keepsassets游戏root 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn isolated_data_directory_keeps_assets_in_game_root() {
        let values = placeholders(
            Path::new(r"D:\Minecraft"),
            Path::new(r"D:\Minecraft\versions\fabric-test"),
            "test",
            "assets",
            Path::new(r"D:\Minecraft\versions\test\natives"),
            Path::new(r"D:\Minecraft\libraries"),
            "classpath",
            ";",
            "Alex_Player",
            true,
        );
        assert_eq!(
            values.get("game_directory").map(String::as_str),
            Some(r"D:\Minecraft\versions\fabric-test")
        );
        assert_eq!(
            values.get("assets_root").map(String::as_str),
            Some(r"D:\Minecraft\assets")
        );
        assert_eq!(
            values.get("auth_player_name").map(String::as_str),
            Some("Alex_Player")
        );
        assert_eq!(
            values.get("auth_uuid").map(String::as_str),
            Some(OFFLINE_SLIM_UUID)
        );
    }

    #[test]
    /// 实现 offlineuuids选择therequested玩家模型 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn offline_uuids_select_the_requested_player_model() {
        /// 实现 默认皮肤bucket 相关的业务流程。
        ///
        /// # 参数
        /// - `uuid`：由调用方传入的uuid。
        /// # 返回
        /// 返回 `i32`，供上层继续渲染、判断或持久化。
        /// # 约束
        /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
        fn default_skin_bucket(uuid: &str) -> i32 {
            let raw = uuid.replace('-', "");
            let value = u128::from_str_radix(&raw, 16).unwrap();
            let high = (value >> 64) as u64;
            let low = value as u64;
            let hash = ((high ^ low) as u32) as i32;
            hash.rem_euclid(18)
        }

        assert!(default_skin_bucket(OFFLINE_SLIM_UUID) < 9);
        assert!(default_skin_bucket(OFFLINE_WIDE_UUID) >= 9);
    }

    #[test]
    /// 验证自定义 profile 名称会沿 `inheritsFrom` 链解析为真正的官方版本。
    fn resolves_custom_profile_to_official_base() {
        let root = std::env::temp_dir().join(format!(
            "genesis-launcher-profile-chain-{}",
            std::process::id()
        ));
        let versions = root.join("versions");
        for (profile, json) in [
            ("愚者", r#"{"id":"愚者","inheritsFrom":"fabric-1.21.1"}"#),
            (
                "fabric-1.21.1",
                r#"{"id":"fabric-1.21.1","inheritsFrom":"1.21.1"}"#,
            ),
            ("1.21.1", r#"{"id":"1.21.1"}"#),
        ] {
            let directory = versions.join(profile);
            fs::create_dir_all(&directory).unwrap();
            fs::write(directory.join(format!("{profile}.json")), json).unwrap();
        }

        assert_eq!(
            official_base_version_for_profile(&root, "愚者").as_deref(),
            Some("1.21.1")
        );
        assert_eq!(resolve_minecraft_dependency(&root, "愚者"), "1.21.1");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    /// 验证没有父版本的自定义客户端仍可从 `jar` 元数据推断基础版本。
    fn infers_official_base_from_custom_client_metadata() {
        let root = std::env::temp_dir().join(format!(
            "genesis-launcher-profile-metadata-{}",
            std::process::id()
        ));
        let directory = root.join("versions").join("愚者");
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("愚者.json"),
            r#"{"id":"愚者","jar":"1.20.1"}"#,
        )
        .unwrap();
        let base = root.join("versions").join("1.20.1");
        fs::create_dir_all(&base).unwrap();
        fs::write(base.join("1.20.1.json"), r#"{"id":"1.20.1"}"#).unwrap();

        assert_eq!(
            official_base_version_for_profile(&root, "愚者").as_deref(),
            Some("1.20.1")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    /// 验证旧整合包可以从自定义 profile 的库元数据恢复加载器版本。
    fn recovers_loader_from_custom_profile_metadata() {
        let root = std::env::temp_dir().join(format!(
            "genesis-launcher-profile-loader-{}",
            std::process::id()
        ));
        let directory = root.join("versions").join("愚者");
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("愚者.json"),
            r#"{
              "id":"愚者",
              "inheritsFrom":"1.20.1",
              "libraries":[
                {"name":"net.minecraftforge:forge:1.20.1-47.3.0"}
              ]
            }"#,
        )
        .unwrap();
        let base = root.join("versions").join("1.20.1");
        fs::create_dir_all(&base).unwrap();
        fs::write(base.join("1.20.1.json"), r#"{"id":"1.20.1"}"#).unwrap();

        assert_eq!(
            local_profile_loader(&root, "愚者"),
            Some(("forge", "47.3.0".to_owned()))
        );
        let _ = fs::remove_dir_all(root);
    }
}
