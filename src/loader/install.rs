//! # `src/loader/install.rs`
//!
//! **功能域：** Fabric、Forge 和 NeoForge 的版本查询、下载与安装。
//!
//! 编排安装流程并串联校验、下载和文件提交
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `start_install`
//! - `fetch_versions`
//! - `install_loader`
//! - `install_fabric`
//! - `install_with_installer`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 实现中包含线程、通道或共享状态；同步边界必须与现有轮询和生命周期保持一致。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 包含平台相关分支；跨平台修改必须同时保持各 ``cfg`` 分支可编译。

use super::model::{LoaderInstallUpdate, LoaderKind, display_version};
use super::network::{download_verified, fetch_text};
use super::util::{
    ensure_allowed_loader_url, forge_loader_part, maven_path, neoforge_prefix, validate_component,
    version_key, write_atomic,
};
use super::versions::{fetch_forge_versions, fetch_neoforge_versions};
use super::*;
use serde_json::Value;
use std::fs;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;

/// 实现 启动安装 相关的业务流程。
///
/// # 参数
/// - `game_directory`：由调用方传入的游戏目录。
/// - `game_version`：由调用方传入的游戏版本。
/// - `loader`：由调用方传入的加载器。
/// - `loader_version`：由调用方传入的加载器版本。
/// # 返回
/// 返回 `mpsc::Receiver<LoaderInstallUpdate>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn start_install(
    game_directory: PathBuf,
    game_version: String,
    loader: LoaderKind,
    loader_version: String,
) -> mpsc::Receiver<LoaderInstallUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = install_loader(
            &game_directory,
            &game_version,
            loader,
            &loader_version,
            &sender,
        );
        let (success, profile_hint, message) = match result {
            Ok(profile) => (
                true,
                Some(profile.clone()),
                format!("{} {} 安装完成", loader.label(), loader_version),
            ),
            Err(error) => (false, None, error),
        };
        let _ = sender.send(LoaderInstallUpdate {
            stage: if success {
                "加载器安装完成".to_owned()
            } else {
                "加载器安装失败".to_owned()
            },
            finished: true,
            success,
            profile_hint,
            message,
        });
    });
    receiver
}

/// 实现 获取版本 相关的业务流程。
///
/// # 参数
/// - `loader`：由调用方传入的加载器。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn fetch_versions(
    game_version: &str,
    loader: LoaderKind,
) -> Result<Vec<String>, String> {
    match loader {
        LoaderKind::Fabric => {
            let url = format!("https://meta.fabricmc.net/v2/versions/loader/{game_version}");
            let body = fetch_text(&url)?;
            let value: Value = serde_json::from_str(&body)
                .map_err(|error| format!("Fabric 版本列表格式无效：{error}"))?;
            let mut versions = value
                .as_array()
                .ok_or_else(|| "Fabric 版本列表无效".to_owned())?
                .iter()
                .filter_map(|entry| {
                    entry
                        .pointer("/loader/version")
                        .or_else(|| entry.get("version"))
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .collect::<Vec<_>>();
            versions.sort_by_key(|version| std::cmp::Reverse(version_key(version)));
            versions.dedup();
            if versions.is_empty() {
                Err(format!("Fabric 不支持 Minecraft {game_version}"))
            } else {
                Ok(versions)
            }
        }
        LoaderKind::Forge => fetch_forge_versions(game_version),
        LoaderKind::NeoForge => {
            let prefix = neoforge_prefix(game_version)?;
            let mut versions = fetch_neoforge_versions(game_version)?;
            let available_hint = versions
                .iter()
                .take(6)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            versions.retain(|version| version.starts_with(&prefix));
            versions.sort_by_key(|version| std::cmp::Reverse(version_key(version)));
            if versions.is_empty() {
                Err(format!(
                    "NeoForge 暂不支持 Minecraft {game_version}（当前可用：{available_hint}）"
                ))
            } else {
                Ok(versions)
            }
        }
    }
}

/// 实现 安装加载器 相关的业务流程。
///
/// # 参数
/// - `game_version`：由调用方传入的游戏版本。
/// - `loader`：由调用方传入的加载器。
/// - `loader_version`：由调用方传入的加载器版本。
/// - `sender`：由调用方传入的sender。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn install_loader(
    game_directory: &Path,
    game_version: &str,
    loader: LoaderKind,
    loader_version: &str,
    sender: &mpsc::Sender<LoaderInstallUpdate>,
) -> Result<String, String> {
    match loader {
        LoaderKind::Fabric => install_fabric(game_directory, game_version, loader_version, sender),
        LoaderKind::Forge => {
            install_with_installer(game_directory, game_version, loader, loader_version, sender)
        }
        LoaderKind::NeoForge => {
            install_with_installer(game_directory, game_version, loader, loader_version, sender)
        }
    }
}

/// 实现 安装fabric 相关的业务流程。
///
/// # 参数
/// - `game_version`：由调用方传入的游戏版本。
/// - `loader_version`：由调用方传入的加载器版本。
/// - `sender`：由调用方传入的sender。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn install_fabric(
    game_directory: &Path,
    game_version: &str,
    loader_version: &str,
    sender: &mpsc::Sender<LoaderInstallUpdate>,
) -> Result<String, String> {
    let _ = sender.send(LoaderInstallUpdate::running("获取 Fabric 配置"));
    let url = format!(
        "https://meta.fabricmc.net/v2/versions/loader/{game_version}/{loader_version}/profile/json"
    );
    let body = fetch_text(&url)?;
    let profile: Value =
        serde_json::from_str(&body).map_err(|error| format!("Fabric 配置格式无效：{error}"))?;
    let profile_id = profile
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("fabric-loader-{loader_version}-{game_version}"));
    validate_component(&profile_id)?;
    let libraries = profile
        .get("libraries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut tasks = Vec::new();
    for library in &libraries {
        let Some(name) = library.get("name").and_then(Value::as_str) else {
            continue;
        };
        let base_url = library
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or("https://maven.fabricmc.net/");
        let path = maven_path(name)?;
        let url = format!("{}/{}", base_url.trim_end_matches('/'), path);
        let target = game_directory.join("libraries").join(path);
        ensure_allowed_loader_url(&url)?;
        tasks.push((url, target));
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let completed = std::sync::atomic::AtomicUsize::new(0);
    let failure = std::sync::Mutex::new(None::<String>);
    let _ = sender.send(LoaderInstallUpdate::running(format!(
        "下载 Fabric 库 0/{}",
        tasks.len()
    )));
    thread::scope(|scope| {
        for _ in 0..4.min(tasks.len()) {
            scope.spawn(|| {
                loop {
                    if failure.lock().ok().is_some_and(|value| value.is_some()) {
                        break;
                    }
                    let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some((url, target)) = tasks.get(index) else {
                        break;
                    };
                    if let Err(error) = download_verified(url, target) {
                        if let Ok(mut failure) = failure.lock() {
                            *failure = Some(error);
                        }
                        break;
                    }
                    let done = completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    let _ = sender.send(LoaderInstallUpdate::running(format!(
                        "下载 Fabric 库 {done}/{}",
                        tasks.len()
                    )));
                }
            });
        }
    });
    if let Some(error) = failure.lock().unwrap().take() {
        return Err(error);
    }
    let version_dir = game_directory.join("versions").join(&profile_id);
    fs::create_dir_all(&version_dir)
        .map_err(|error| format!("无法创建 Fabric 版本目录：{error}"))?;
    let json_path = version_dir.join(format!("{profile_id}.json"));
    write_atomic(&json_path, body.as_bytes())?;
    Ok(profile_id)
}

/// 实现 安装withinstaller 相关的业务流程。
///
/// # 参数
/// - `game_version`：由调用方传入的游戏版本。
/// - `loader`：由调用方传入的加载器。
/// - `loader_version`：由调用方传入的加载器版本。
/// - `sender`：由调用方传入的sender。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn install_with_installer(
    game_directory: &Path,
    game_version: &str,
    loader: LoaderKind,
    loader_version: &str,
    sender: &mpsc::Sender<LoaderInstallUpdate>,
) -> Result<String, String> {
    let legacy_neoforge = loader == LoaderKind::NeoForge && game_version == "1.20.1";
    let neoforge_artifact = if legacy_neoforge { "forge" } else { "neoforge" };
    let neoforge_version = if legacy_neoforge && !loader_version.starts_with("1.20.1-") {
        format!("1.20.1-{loader_version}")
    } else {
        loader_version.to_owned()
    };
    let (url, profile_hint) = match loader {
        LoaderKind::Forge => (
            format!(
                "https://maven.minecraftforge.net/net/minecraftforge/forge/{loader_version}/forge-{loader_version}-installer.jar"
            ),
            format!("{game_version}-forge-{}", forge_loader_part(loader_version)),
        ),
        LoaderKind::NeoForge => {
            let display_version = display_version(game_version, loader_version);
            (
                format!(
                    "https://maven.neoforged.net/releases/net/neoforged/{neoforge_artifact}/{neoforge_version}/{neoforge_artifact}-{neoforge_version}-installer.jar"
                ),
                format!("neoforge-{display_version}"),
            )
        }
        LoaderKind::Fabric => unreachable!(),
    };
    ensure_allowed_loader_url(&url)?;
    let installer = game_directory
        .join("downloads")
        .join(format!("{}-installer.jar", loader.label().to_lowercase()));
    let _ = sender.send(LoaderInstallUpdate::running(format!(
        "下载 {} 安装器",
        loader.label()
    )));
    let fallback_urls = match loader {
        LoaderKind::Forge => vec![format!(
            "https://bmclapi2.bangbang93.com/maven/net/minecraftforge/forge/{loader_version}/forge-{loader_version}-installer.jar"
        )],
        LoaderKind::NeoForge => vec![
            format!(
                "https://bmclapi2.bangbang93.com/maven/net/neoforged/{neoforge_artifact}/{neoforge_version}/{neoforge_artifact}-{neoforge_version}-installer.jar"
            ),
            format!(
                "https://maven.aliyun.com/repository/releases/net/neoforged/{neoforge_artifact}/{neoforge_version}/{neoforge_artifact}-{neoforge_version}-installer.jar"
            ),
            format!(
                "https://maven.aliyun.com/repository/public/net/neoforged/{neoforge_artifact}/{neoforge_version}/{neoforge_artifact}-{neoforge_version}-installer.jar"
            ),
            format!(
                "https://repo.huaweicloud.com/repository/maven/net/neoforged/{neoforge_artifact}/{neoforge_version}/{neoforge_artifact}-{neoforge_version}-installer.jar"
            ),
            format!(
                "https://mirrors.cloud.tencent.com/nexus/repository/maven-public/net/neoforged/{neoforge_artifact}/{neoforge_version}/{neoforge_artifact}-{neoforge_version}-installer.jar"
            ),
        ],
        _ => Vec::new(),
    };
    let mut installed = download_verified(&url, &installer).is_ok();
    for fallback in fallback_urls {
        if installed {
            break;
        }
        installed = download_verified(&fallback, &installer).is_ok();
    }
    if !installed {
        return Err(format!("无法下载 {} 安装器", loader.label()));
    }
    let java = crate::game::java_for_loader_install(game_directory, game_version)
        .ok_or_else(|| format!("未找到可用于 {} 安装器的 Java", loader.label()))?;
    let _ = sender.send(LoaderInstallUpdate::running(format!(
        "运行 {} 安装器",
        loader.label()
    )));
    let mut command = Command::new(java);
    command
        .arg("-jar")
        .arg(&installer)
        .arg("--installClient")
        .arg(game_directory)
        .current_dir(game_directory)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let output = command
        .output()
        .map_err(|error| format!("无法运行 {} 安装器：{error}", loader.label()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = format!("{stdout}\n{stderr}");
        return Err(format!(
            "{} 安装器失败（{}）：{}",
            loader.label(),
            output.status,
            detail
                .chars()
                .rev()
                .take(600)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>()
        ));
    }
    let _ = fs::remove_file(installer);
    Ok(profile_hint)
}
