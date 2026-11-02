//! # `src/modpack/import.rs`
//!
//! **功能域：** Modrinth 整合包读取、依赖补全、文件下载和覆盖层解压。
//!
//! 实现 import 相关逻辑，并向同目录模块提供必要的类型与操作。
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `start_import`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 实现中包含线程、通道或共享状态；同步边界必须与现有轮询和生命周期保持一致。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;

use serde_json::json;
use zip::ZipArchive;

use crate::loader::LoaderKind;

use super::download::download_files;
use super::loader::install_loader;
use super::manifest::read_manifest;
use super::minecraft::ensure_minecraft;
use super::model::ModpackImportUpdate;
use super::naming::{sanitize_name, unique_profile_name};
use super::overrides::extract_overrides;
use super::progress::send_stage;

/// 实现 启动导入 相关的业务流程。
///
/// # 参数
/// - `game_directory`：由调用方传入的游戏目录。
/// - `archive_path`：由调用方传入的压缩包路径。
/// # 返回
/// 返回 `mpsc::Receiver<ModpackImportUpdate>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn start_import(
    game_directory: PathBuf,
    archive_path: PathBuf,
) -> mpsc::Receiver<ModpackImportUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = import_modpack(&game_directory, &archive_path, &sender);
        let (success, profile, message) = match result {
            Ok(profile) => (true, Some(profile), "整合包导入完成".to_owned()),
            Err(error) => (false, None, error),
        };
        let _ = sender.send(ModpackImportUpdate {
            stage: if success {
                "导入完成".to_owned()
            } else {
                "导入失败".to_owned()
            },
            message,
            finished: true,
            success,
            profile,
        });
    });
    receiver
}

/// 实现 导入整合包 相关的业务流程。
///
/// # 参数
/// - `game_directory`：由调用方传入的游戏目录。
/// - `archive_path`：由调用方传入的压缩包路径。
/// - `sender`：由调用方传入的sender。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn import_modpack(
    game_directory: &Path,
    archive_path: &Path,
    sender: &mpsc::Sender<ModpackImportUpdate>,
) -> Result<String, String> {
    send_stage(sender, "读取整合包", archive_path.display().to_string());
    let archive_file =
        File::open(archive_path).map_err(|error| format!("无法打开整合包：{error}"))?;
    let mut archive =
        ZipArchive::new(archive_file).map_err(|error| format!("无法读取整合包：{error}"))?;
    let manifest = read_manifest(&mut archive, archive_path)?;

    send_stage(
        sender,
        "准备 Minecraft",
        format!("Minecraft {}", manifest.minecraft),
    );
    let minecraft = ensure_minecraft(game_directory, &manifest.minecraft, sender)?;

    // 兼容旧版导出的整合包：当时可能把本地 profile 名写入 minecraft 字段，
    // 同时遗漏加载器依赖。若本地原 profile 仍存在，从其中恢复加载器。
    let manifest_loader = manifest.loader.clone().or_else(|| {
        crate::game::local_profile_loader(game_directory, &manifest.minecraft).and_then(
            |(family, version)| {
                let loader = match family {
                    "fabric" => LoaderKind::Fabric,
                    "forge" => LoaderKind::Forge,
                    "neoforge" => LoaderKind::NeoForge,
                    _ => return None,
                };
                Some((loader, version))
            },
        )
    });

    let loader_profile = if let Some((loader, loader_version)) = manifest_loader.as_ref() {
        send_stage(
            sender,
            "安装加载器",
            format!("{} {}", loader.label(), loader_version),
        );
        install_loader(game_directory, &minecraft, *loader, loader_version, sender)?
    } else {
        minecraft.clone()
    };

    let loader_suffix = manifest_loader
        .as_ref()
        .map(|(loader, _)| format!("-{}", loader.label()))
        .unwrap_or_default();
    let profile = unique_profile_name(
        game_directory,
        &format!("{}{loader_suffix}", sanitize_name(&manifest.name)),
    );
    let version_directory = game_directory.join("versions").join(&profile);
    fs::create_dir_all(&version_directory)
        .map_err(|error| format!("无法创建整合包版本目录：{error}"))?;

    send_stage(sender, "解压整合包", "正在写入配置与覆盖文件".to_owned());
    extract_overrides(&mut archive, &manifest.archive_prefix, &version_directory)?;

    send_stage(
        sender,
        "下载整合包文件",
        format!("共 {} 个文件", manifest.files.len()),
    );
    download_files(&manifest.files, &version_directory, sender)?;

    send_stage(sender, "生成版本", profile.clone());
    let profile_json = json!({
        "id": profile,
        "inheritsFrom": loader_profile,
        "genesisDisplayName": manifest.name,
        "type": "release",
        "time": "1970-01-01T00:00:00Z",
        "releaseTime": "1970-01-01T00:00:00Z"
    });
    let profile_path = version_directory.join(format!("{profile}.json"));
    fs::write(
        profile_path,
        serde_json::to_vec_pretty(&profile_json)
            .map_err(|error| format!("生成版本配置失败：{error}"))?,
    )
    .map_err(|error| format!("写入版本配置失败：{error}"))?;
    Ok(profile)
}
