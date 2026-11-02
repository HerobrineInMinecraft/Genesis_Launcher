//! # `src/game/version.rs`
//!
//! **功能域：** Minecraft 版本发现、Java 选择、启动参数组装和进程生命周期。
//!
//! 读取继承版本列表并合并依赖数据
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `resolve_profile`
//! - `load_merged_version`
//! - `load_merged_version_inner`
//! - `append_array`
//! - `rules_allow`
//! - `rule_matches`
//! - `collect_arguments`
//! - `maven_library_path`
//! - `extract_windows_natives`
//! - `extract_native_jar`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use super::launch_args::replace_placeholders;
use super::*;

/// 实现 resolve配置 相关的业务流程。
///
/// # 参数
/// - `version`：由调用方传入的版本。
/// - `loader`：由调用方传入的加载器。
/// # 返回
/// 返回 `Option<String>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn resolve_profile(
    game_directory: &Path,
    version: &str,
    loader: &str,
) -> Option<String> {
    let versions = installed_versions(game_directory);
    if versions.iter().any(|installed| installed == version) && loader == "原版" {
        return Some(version.to_owned());
    }
    let loader_key = match loader {
        "Fabric" => Some("fabric"),
        "NeoForge" => Some("neoforge"),
        "Forge" => Some("forge"),
        _ => None,
    };
    if let Some(key) = loader_key {
        versions
            .iter()
            .find(|installed| installed.contains(version) && installed.to_lowercase().contains(key))
            .cloned()
    } else {
        None
    }
}

/// 实现 加载merged版本 相关的业务流程。
///
/// # 参数
/// - `id`：由调用方传入的id。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn load_merged_version(
    game_directory: &Path,
    id: &str,
) -> Result<MergedVersion, String> {
    load_merged_version_inner(game_directory, id, &mut HashSet::new())
}

/// 实现 加载merged版本inner 相关的业务流程。
///
/// # 参数
/// - `id`：由调用方传入的id。
/// - `visited`：由调用方传入的visited。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn load_merged_version_inner(
    game_directory: &Path,
    id: &str,
    visited: &mut HashSet<String>,
) -> Result<MergedVersion, String> {
    if !visited.insert(id.to_owned()) {
        return Err(format!("版本继承出现循环：{id}"));
    }
    let path = game_directory
        .join("versions")
        .join(id)
        .join(format!("{id}.json"));
    let contents =
        fs::read_to_string(&path).map_err(|error| format!("{}：{error}", path.display()))?;
    let value: Value =
        serde_json::from_str(&contents).map_err(|error| format!("JSON 格式无效：{error}"))?;

    let parent = value
        .get("inheritsFrom")
        .and_then(Value::as_str)
        .map(|parent_id| load_merged_version_inner(game_directory, parent_id, visited))
        .transpose()?;

    let main_class = value
        .get("mainClass")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| parent.as_ref().map(|parent| parent.main_class.clone()))
        .unwrap_or_default();
    let assets = value
        .get("assets")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| parent.as_ref().map(|parent| parent.assets.clone()))
        .unwrap_or_else(|| "legacy".to_owned());
    let client_jar_version = value
        .get("jar")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            parent
                .as_ref()
                .map(|parent| parent.client_jar_version.clone())
        })
        .unwrap_or_else(|| id.to_owned());
    let java_major = value
        .pointer("/javaVersion/majorVersion")
        .and_then(Value::as_u64)
        .map(|major| major as u32)
        .or_else(|| parent.as_ref().and_then(|parent| parent.java_major));

    let mut libraries = parent
        .as_ref()
        .map(|parent| parent.libraries.clone())
        .unwrap_or_default();
    append_array(&value, "/libraries", &mut libraries);
    let mut game_args = parent
        .as_ref()
        .map(|parent| parent.game_args.clone())
        .unwrap_or_default();
    let mut jvm_args = parent
        .as_ref()
        .map(|parent| parent.jvm_args.clone())
        .unwrap_or_default();
    append_array(&value, "/arguments/game", &mut game_args);
    append_array(&value, "/arguments/jvm", &mut jvm_args);
    let legacy_args = value
        .get("minecraftArguments")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| parent.and_then(|parent| parent.legacy_args));

    Ok(MergedVersion {
        main_class,
        assets,
        client_jar_version,
        java_major,
        libraries,
        game_args,
        jvm_args,
        legacy_args,
    })
}

/// 实现 appendarray 相关的业务流程。
///
/// # 参数
/// - `pointer`：由调用方传入的指针。
/// - `target`：由调用方传入的target。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn append_array(value: &Value, pointer: &str, target: &mut Vec<Value>) {
    let source = if pointer.is_empty() {
        value
    } else {
        value.pointer(pointer).unwrap_or(&Value::Null)
    };
    if let Some(items) = source.as_array() {
        target.extend(items.iter().cloned());
    }
}

/// 实现 rulesallow 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `bool`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn rules_allow(value: &Value) -> bool {
    let Some(rules) = value.get("rules").and_then(Value::as_array) else {
        return true;
    };
    let mut allowed = false;
    for rule in rules {
        if rule_matches(rule) {
            allowed = rule.get("action").and_then(Value::as_str) == Some("allow");
        }
    }
    allowed
}

/// 实现 rulematches 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `bool`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn rule_matches(rule: &Value) -> bool {
    if let Some(os) = rule.get("os") {
        if let Some(name) = os.get("name").and_then(Value::as_str)
            && name != "windows"
        {
            return false;
        }
        if let Some(arch) = os.get("arch").and_then(Value::as_str)
            && arch != std::env::consts::ARCH
        {
            return false;
        }
    }
    if let Some(features) = rule.get("features").and_then(Value::as_object) {
        for (_feature, expected) in features {
            let actual = false;
            if expected.as_bool().unwrap_or(false) != actual {
                return false;
            }
        }
    }
    true
}

/// 实现 收集arguments 相关的业务流程。
///
/// # 参数
/// - `placeholders`：由调用方传入的placeholders。
/// - `target`：由调用方传入的target。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn collect_arguments(
    value: &Value,
    placeholders: &HashMap<&str, String>,
    target: &mut Vec<String>,
) {
    match value {
        Value::String(argument) => target.push(replace_placeholders(argument, placeholders)),
        Value::Array(values) => {
            for value in values {
                collect_arguments(value, placeholders, target);
            }
        }
        Value::Object(object) => {
            if !rules_allow(value) {
                return;
            }
            if let Some(value) = object.get("value") {
                collect_arguments(value, placeholders, target);
            }
        }
        _ => {}
    }
}

/// 实现 mavenlibrary路径 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Option<String>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn maven_library_path(library: &Value) -> Option<String> {
    let name = library.get("name").and_then(Value::as_str)?;
    let (coordinates, extension) = name
        .split_once('@')
        .map_or((name, "jar"), |(coordinates, extension)| {
            (coordinates, extension)
        });
    let parts = coordinates.split(':').collect::<Vec<_>>();
    if parts.len() < 3 || parts[..3].iter().any(|part| part.is_empty()) {
        return None;
    }
    let group = parts[0];
    let artifact = parts[1];
    let version = parts[2];
    let file_name = if let Some(classifier) = parts.get(3) {
        format!("{artifact}-{version}-{classifier}.{extension}")
    } else {
        format!("{artifact}-{version}.{extension}")
    };
    Some(format!(
        "{}/{artifact}/{version}/{file_name}",
        group.replace('.', "/")
    ))
}

/// 实现 extractwindowsnatives 相关的业务流程。
///
/// # 参数
/// - `libraries_dir`：由调用方传入的librariesdir。
/// - `natives_dir`：由调用方传入的nativesdir。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn extract_windows_natives(
    library: &Value,
    libraries_dir: &Path,
    natives_dir: &Path,
) -> Result<(), String> {
    let Some(classifiers) = library
        .pointer("/downloads/classifiers")
        .and_then(Value::as_object)
    else {
        return Ok(());
    };
    let arch = if std::env::consts::ARCH == "aarch64" {
        "arm64"
    } else {
        "64"
    };
    let classifier_key = format!("natives-windows-{arch}");
    let path = classifiers
        .get(&classifier_key)
        .or_else(|| classifiers.get("natives-windows"))
        .and_then(|download| download.get("path"))
        .and_then(Value::as_str);
    let Some(path) = path else {
        return Ok(());
    };
    let archive_path = libraries_dir.join(path);
    if !archive_path.is_file() {
        return Ok(());
    }

    extract_native_jar(&archive_path, natives_dir)
}

/// 实现 extractnativejar 相关的业务流程。
///
/// # 参数
/// - `natives_dir`：由调用方传入的nativesdir。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn extract_native_jar(archive_path: &Path, natives_dir: &Path) -> Result<(), String> {
    if !archive_path.is_file() {
        return Ok(());
    }
    let file = File::open(archive_path).map_err(|error| format!("无法打开 natives：{error}"))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("无法读取 natives：{error}"))?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("无法读取 natives 条目：{error}"))?;
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        if !relative
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"))
        {
            continue;
        }
        let output = natives_dir.join(relative.file_name().unwrap_or(relative.as_os_str()));
        let mut target =
            File::create(&output).map_err(|error| format!("无法写入 natives：{error}"))?;
        io::copy(&mut entry, &mut target).map_err(|error| format!("无法解压 natives：{error}"))?;
    }
    Ok(())
}
