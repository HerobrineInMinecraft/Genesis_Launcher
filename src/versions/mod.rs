//! # `src/versions/mod.rs`
//!
//! **功能域：** 本地已安装版本聚合、排序和展示模型。
//!
//! 组织本模块声明并导出稳定公共接口
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `GameVersion`
//! - `available_versions`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::collections::HashSet;
use std::path::Path;

#[derive(Clone, Debug)]
/// 定义 GameVersion 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub(crate) struct GameVersion {
    pub(crate) version: String,
    pub(crate) game_version: String,
    pub(crate) custom_name: Option<String>,
    pub(crate) loader: String,
    pub(crate) loader_family: String,
    pub(crate) java: String,
    pub(crate) description: String,
    pub(crate) launchable: bool,
}

/// 实现 可用版本 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Vec<GameVersion>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn available_versions(game_directory: Option<&Path>) -> Vec<GameVersion> {
    let installed = game_directory
        .map(crate::game::installed_versions)
        .unwrap_or_default();
    if !installed.is_empty() {
        let launchable: HashSet<String> = game_directory
            .map(crate::game::launchable_versions)
            .unwrap_or_default()
            .into_iter()
            .collect();
        return installed
            .into_iter()
            .map(|version| {
                let loader = detect_loader_family(game_directory, &version);
                let loader_display = game_directory
                    .and_then(|directory| {
                        let loader_key = match loader {
                            "NeoForge" => "neoforge",
                            "Fabric" => "fabric",
                            "Forge" => "forge",
                            _ => return None,
                        };
                        crate::game::installed_loader_version(directory, &version, loader_key)
                            .map(|loader_version| format!("{loader} {loader_version}"))
                    })
                    .unwrap_or_else(|| loader.to_owned());
                let game_version = game_directory
                    .and_then(|directory| {
                        crate::game::official_base_version_for_profile(directory, &version)
                    })
                    .unwrap_or_else(|| version.clone());
                let custom_name = read_custom_version_name(game_directory, &version)
                    .or_else(|| infer_custom_profile_name(&version, &game_version, loader));
                let java_major = game_directory
                    .and_then(|directory| {
                        crate::game::version_java_requirement(directory, &version)
                    })
                    .unwrap_or_else(|| {
                        if version.starts_with("1.20.4")
                            || version.starts_with("1.19")
                            || version.starts_with("1.18")
                            || version.starts_with("1.17")
                        {
                            17
                        } else {
                            21
                        }
                    });
                let launchable = launchable.contains(&version);
                GameVersion {
                    description: if launchable {
                        format!("{loader_display} · 已安装 · 可启动")
                    } else {
                        format!("{loader_display} · 启动时自动补全")
                    },
                    version,
                    game_version,
                    custom_name,
                    loader: loader_display,
                    loader_family: loader.to_owned(),
                    java: format!("Java {java_major}"),
                    launchable,
                }
            })
            .collect();
    }

    [
        ("1.21.11", "Java 21"),
        ("1.21.10", "Java 21"),
        ("1.21.9", "Java 21"),
        ("1.21.8", "Java 21"),
    ]
    .into_iter()
    .map(|(version, java)| GameVersion {
        version: version.to_owned(),
        game_version: version.to_owned(),
        custom_name: None,
        loader: "原版".to_owned(),
        loader_family: "原版".to_owned(),
        java: java.to_owned(),
        description: "Mojang 官方源 · 启动时自动安装".to_owned(),
        launchable: false,
    })
    .collect()
}

/// 读取版本 JSON 中由整合包导入或重命名写入的显示名称。
fn read_custom_version_name(game_directory: Option<&Path>, profile: &str) -> Option<String> {
    let directory = game_directory?;
    let path = directory
        .join("versions")
        .join(profile)
        .join(format!("{profile}.json"));
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    for key in ["genesisDisplayName", "displayName"] {
        if let Some(name) = value.get(key).and_then(serde_json::Value::as_str) {
            let name = crate::brand::strip_generated_name_prefix(name);
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

/// 从非标准本地 profile 名称推断用户自定义名称。
///
/// 标准加载器生成的目录名会包含 Minecraft 版本和加载器关键字，不作为自定义名称；
/// 中文、整合包作者命名和旧版启动器前缀会保留为自定义名称。
fn infer_custom_profile_name(profile: &str, game_version: &str, loader: &str) -> Option<String> {
    let cleaned = crate::brand::strip_generated_name_prefix(profile);
    if cleaned != profile {
        return Some(cleaned);
    }
    if profile == game_version {
        return None;
    }
    let lower = profile.to_lowercase();
    let generated = match loader {
        "Fabric" => lower.contains("fabric") && lower.contains(&game_version.to_lowercase()),
        "Forge" => lower.contains("forge") && lower.contains(&game_version.to_lowercase()),
        "NeoForge" => lower.contains("neoforge") && lower.contains(&game_version.to_lowercase()),
        _ => false,
    };
    (!generated).then_some(cleaned)
}

/// 从合并后的版本配置检测加载器类型。
///
/// 本地实例名称可能完全是整合包作者自定义的中文名，因此不能只依赖 profile
/// 文件名中的 `forge`/`fabric` 关键字。这里优先检查合并后的库名和主类，
/// 无法识别时再回退到名称判断。
fn detect_loader_family(game_directory: Option<&Path>, profile: &str) -> &'static str {
    if let Some(directory) = game_directory
        && let Ok(merged) = crate::game::load_merged_version(directory, profile)
    {
        let main_class = merged.main_class.to_ascii_lowercase();
        if main_class.contains("neoforge") {
            return "NeoForge";
        }
        if main_class.contains("fabric") {
            return "Fabric";
        }
        if main_class.contains("forge") {
            return "Forge";
        }
        for library in &merged.libraries {
            let Some(name) = library.get("name").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let name = name.to_ascii_lowercase();
            if name.starts_with("net.neoforged:neoforge:")
                || name.starts_with("net.neoforged:forge:")
            {
                return "NeoForge";
            }
            if name.starts_with("net.fabricmc:fabric-loader:") {
                return "Fabric";
            }
            if name.starts_with("net.minecraftforge:forge:")
                || name.starts_with("net.minecraftforge:fmlloader:")
            {
                return "Forge";
            }
        }
    }

    let lower = profile.to_lowercase();
    if lower.contains("neo") {
        "NeoForge"
    } else if lower.contains("fabric") || lower.contains("quilt") {
        "Fabric"
    } else if lower.contains("forge") {
        "Forge"
    } else {
        "原版"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// 验证标准加载器目录不会被误认为自定义名称。
    fn ignores_generated_loader_profile_names() {
        assert_eq!(
            infer_custom_profile_name("fabric-1.21.1", "1.21.1", "Fabric"),
            None
        );
        assert_eq!(
            infer_custom_profile_name("1.21.1-forge-52.1.20", "1.21.1", "Forge"),
            None
        );
    }

    #[test]
    /// 验证中文整合包名和旧启动器前缀会被识别为显示名称。
    fn preserves_custom_and_cleans_launcher_prefix() {
        assert_eq!(
            infer_custom_profile_name("愚者整合包-Forge", "1.21.1", "Forge"),
            Some("愚者整合包-Forge".to_owned())
        );
        assert_eq!(
            infer_custom_profile_name("Genesis Launcher · 1.21.1", "1.21.1", "原版"),
            Some("1.21.1".to_owned())
        );
    }

    #[test]
    /// 验证版本 JSON 中写入的导入/重命名名称具有最高优先级。
    fn reads_custom_display_name_from_profile_json() {
        let root = std::env::temp_dir().join(format!(
            "genesis-launcher-display-name-{}",
            std::process::id()
        ));
        let directory = root.join("versions").join("pack-profile");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("pack-profile.json"),
            r#"{"id":"pack-profile","genesisDisplayName":"愚者整合包"}"#,
        )
        .unwrap();
        assert_eq!(
            read_custom_version_name(Some(&root), "pack-profile").as_deref(),
            Some("愚者整合包")
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
