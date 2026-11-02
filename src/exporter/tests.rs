//! # `src/exporter/tests.rs`
//!
//! **功能域：** 版本和整合包导出、文件收集、哈希计算与压缩包生成。
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

use std::collections::HashMap;
use std::io::Read as _;
use std::time::{SystemTime, UNIX_EPOCH};

use super::archive::export_pack_with_hosted_files;
use super::collect::collect_modpack_files;
use super::export_version_archive;
use super::model::{HostedPackFile, ModpackExportFormat, VersionExportOptions};
use crate::versions::GameVersion;

/// 实现 版本 相关的业务流程。
///
/// # 参数
/// - `id`：由调用方传入的id。
/// # 返回
/// 返回 `GameVersion`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn version(id: &str) -> GameVersion {
    GameVersion {
        version: id.to_owned(),
        game_version: id.to_owned(),
        custom_name: None,
        loader: "原版".to_owned(),
        loader_family: "原版".to_owned(),
        java: "Java 21".to_owned(),
        description: String::new(),
        launchable: true,
    }
}

#[test]
/// 实现 导出zipwithselected内容 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn export_zip_with_selected_content() {
    let root = std::env::temp_dir().join(format!(
        "genesis-launcher-export-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let base = root.join(".minecraft");
    std::fs::create_dir_all(base.join("mods")).unwrap();
    std::fs::write(base.join("mods").join("sample.jar"), b"mod").unwrap();
    std::fs::create_dir_all(base.join("config")).unwrap();
    std::fs::write(base.join("config").join("sample.toml"), b"config").unwrap();
    let output = root.join("export.zip");
    let mut item = version("test-version");
    item.game_version = "1.21.1".to_owned();
    export_version_archive(
        &base,
        &base,
        &item,
        &output,
        &VersionExportOptions {
            mods: true,
            config: true,
            saves: false,
            resourcepacks: false,
            shaderpacks: false,
            format: ModpackExportFormat::Zip,
        },
    )
    .unwrap();
    let mut archive = zip::ZipArchive::new(std::fs::File::open(&output).unwrap()).unwrap();
    assert!(archive.by_name("modrinth.index.json").is_ok());
    assert!(archive.by_name("overrides/mods/sample.jar").is_ok());
    assert!(archive.by_name("overrides/config/sample.toml").is_ok());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
/// 实现 modrinthfallbackkeepslocal文件overrides 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn modrinth_fallback_keeps_local_files_in_overrides() {
    let root = std::env::temp_dir().join(format!(
        "genesis-launcher-modrinth-export-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let base = root.join(".minecraft");
    std::fs::create_dir_all(base.join("mods")).unwrap();
    std::fs::write(base.join("mods").join("sample.jar"), b"mod").unwrap();
    std::fs::create_dir_all(base.join("config")).unwrap();
    std::fs::write(base.join("config").join("sample.toml"), b"config").unwrap();
    let output = root.join("export.mrpack");
    let mut item = version("fabric-1.21.1");
    item.game_version = "1.21.1".to_owned();
    item.loader_family = "Fabric".to_owned();
    item.loader = "Fabric 0.16.14".to_owned();
    let options = VersionExportOptions {
        mods: true,
        config: true,
        saves: false,
        resourcepacks: false,
        shaderpacks: false,
        format: ModpackExportFormat::Modrinth,
    };
    let files = collect_modpack_files(&base, &options).unwrap();
    export_pack_with_hosted_files(&item, &output, &options, &files, &HashMap::new()).unwrap();

    let mut archive = zip::ZipArchive::new(std::fs::File::open(&output).unwrap()).unwrap();
    assert!(archive.by_name("overrides/mods/sample.jar").is_ok());
    assert!(archive.by_name("overrides/config/sample.toml").is_ok());
    let mut index = String::new();
    archive
        .by_name("modrinth.index.json")
        .unwrap()
        .read_to_string(&mut index)
        .unwrap();
    let index: serde_json::Value = serde_json::from_str(&index).unwrap();
    assert_eq!(index["formatVersion"], 1);
    assert_eq!(index["dependencies"]["minecraft"], "1.21.1");
    assert_eq!(index["dependencies"]["fabric-loader"], "0.16.14");
    assert_eq!(index["name"], "fabric-1.21.1");
    assert!(
        !index["summary"]
            .as_str()
            .unwrap()
            .contains("Genesis Launcher")
    );
    assert_eq!(index["files"], serde_json::json!([]));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
/// 实现 modrinth导出moveshosted文件outoverrides 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn modrinth_export_moves_hosted_files_out_of_overrides() {
    let root = std::env::temp_dir().join(format!(
        "genesis-launcher-modrinth-hosted-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let base = root.join(".minecraft");
    std::fs::create_dir_all(base.join("mods")).unwrap();
    std::fs::write(base.join("mods").join("hosted.jar"), b"hosted mod").unwrap();
    let options = VersionExportOptions {
        mods: true,
        config: false,
        saves: false,
        resourcepacks: false,
        shaderpacks: false,
        format: ModpackExportFormat::Modrinth,
    };
    let files = collect_modpack_files(&base, &options).unwrap();
    let sha1 = files[0].sha1.clone().unwrap();
    let hosted = HashMap::from([(
        sha1,
        HostedPackFile {
            downloads: vec!["https://cdn.modrinth.com/data/example/hosted.jar".to_owned()],
        },
    )]);
    let output = root.join("hosted.mrpack");
    let mut item = version("fabric-1.21.1");
    item.game_version = "1.21.1".to_owned();
    item.loader_family = "Fabric".to_owned();
    item.loader = "Fabric 0.16.14".to_owned();
    export_pack_with_hosted_files(&item, &output, &options, &files, &hosted).unwrap();

    let mut archive = zip::ZipArchive::new(std::fs::File::open(&output).unwrap()).unwrap();
    assert!(archive.by_name("overrides/mods/hosted.jar").is_err());
    let mut index = String::new();
    archive
        .by_name("modrinth.index.json")
        .unwrap()
        .read_to_string(&mut index)
        .unwrap();
    let index: serde_json::Value = serde_json::from_str(&index).unwrap();
    assert_eq!(index["files"][0]["path"], "mods/hosted.jar");
    assert_eq!(
        index["files"][0]["downloads"][0],
        "https://cdn.modrinth.com/data/example/hosted.jar"
    );
    assert_eq!(
        index["files"][0]["hashes"]["sha512"]
            .as_str()
            .unwrap()
            .len(),
        128
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
/// 验证导出自定义实例名时，清单仍写入解析后的 Mojang 官方基础版本。
fn custom_profile_exports_its_official_base_version() {
    let root = std::env::temp_dir().join(format!(
        "genesis-launcher-custom-profile-export-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let base = root.join(".minecraft");
    let versions = base.join("versions");
    for (profile, json) in [
        ("愚者", r#"{"id":"愚者","inheritsFrom":"1.21.1"}"#),
        ("1.21.1", r#"{"id":"1.21.1"}"#),
    ] {
        let directory = versions.join(profile);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join(format!("{profile}.json")), json).unwrap();
    }
    let output = root.join("愚者.mrpack");
    let mut item = version("愚者");
    item.game_version = "愚者".to_owned();
    export_version_archive(
        &base,
        &base,
        &item,
        &output,
        &VersionExportOptions {
            mods: false,
            config: false,
            saves: false,
            resourcepacks: false,
            shaderpacks: false,
            format: ModpackExportFormat::Modrinth,
        },
    )
    .unwrap();

    let mut archive = zip::ZipArchive::new(std::fs::File::open(&output).unwrap()).unwrap();
    let mut index = String::new();
    archive
        .by_name("modrinth.index.json")
        .unwrap()
        .read_to_string(&mut index)
        .unwrap();
    let index: serde_json::Value = serde_json::from_str(&index).unwrap();
    assert_eq!(index["dependencies"]["minecraft"], "1.21.1");
    let _ = std::fs::remove_dir_all(root);
}
