//! # `src/modpack/tests.rs`
//!
//! **功能域：** Modrinth 整合包读取、依赖补全、文件下载和覆盖层解压。
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

use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;

use zip::ZipArchive;

use crate::loader::LoaderKind;

use super::manifest::read_manifest;
use super::mirror::mirror_url;
use super::naming::{sanitize_name, strip_launcher_export_prefix};
use super::overrides::extract_overrides;
use super::security::safe_relative_path;

#[test]
/// 实现 rejectsescaping包paths 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn rejects_escaping_pack_paths() {
    assert!(safe_relative_path("../outside.jar").is_none());
    assert!(safe_relative_path(r"C:\Windows\bad.dll").is_none());
    assert_eq!(
        safe_relative_path("/mods/example.jar"),
        Some(PathBuf::from("mods/example.jar"))
    );
}

#[test]
/// 实现 sanitizesimported配置names 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn sanitizes_imported_profile_names() {
    assert_eq!(sanitize_name("My:Pack?"), "My_Pack_");
    assert!(!sanitize_name("").is_empty());
    assert_eq!(
        strip_launcher_export_prefix("Genesis Launcher · 1.21.1"),
        "1.21.1"
    );
    assert_eq!(strip_launcher_export_prefix("愚者整合包"), "愚者整合包");
}

#[test]
/// 实现 mapsmodrinthdownloads到mcim 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn maps_modrinth_downloads_to_mcim() {
    assert_eq!(
        mirror_url("https://cdn.modrinth.com/data/a/b.jar").as_deref(),
        Some("https://mod.mcimirror.top/data/a/b.jar")
    );
}

#[test]
/// 实现 readsmodrinthmanifestandextractsoverrides 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn reads_modrinth_manifest_and_extracts_overrides() {
    let root = std::env::temp_dir().join(format!(
        "genesis-launcher-modpack-test-{}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    let archive_path = root.join("pack.mrpack");
    let file = File::create(&archive_path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    writer.start_file("modrinth.index.json", options).unwrap();
    writer
        .write_all(
            br#"{
              "name": "Import Test",
              "dependencies": {
                "minecraft": "1.21.1",
                "fabric-loader": "0.16.9"
              },
              "files": [{
                "path": "mods/example.jar",
                "hashes": {"sha1": "0123456789abcdef0123456789abcdef01234567"},
                "downloads": ["https://cdn.modrinth.com/data/example.jar"],
                "env": {"client": "required"}
              }]
            }"#,
        )
        .unwrap();
    writer
        .start_file("overrides/config/example.toml", options)
        .unwrap();
    writer.write_all(b"enabled=true").unwrap();
    writer.finish().unwrap();

    let mut archive = ZipArchive::new(File::open(&archive_path).unwrap()).unwrap();
    let manifest = read_manifest(&mut archive, &archive_path).unwrap();
    assert_eq!(manifest.name, "Import Test");
    assert_eq!(manifest.minecraft, "1.21.1");
    assert_eq!(
        manifest.loader,
        Some((LoaderKind::Fabric, "0.16.9".to_owned()))
    );
    assert_eq!(manifest.files.len(), 1);
    let output = root.join("instance");
    extract_overrides(&mut archive, "", &output).unwrap();
    assert_eq!(
        fs::read(output.join("config/example.toml")).unwrap(),
        b"enabled=true"
    );
    let _ = fs::remove_dir_all(root);
}
