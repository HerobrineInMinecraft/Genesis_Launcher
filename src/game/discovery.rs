//! # `src/game/discovery.rs`
//!
//! **功能域：** Minecraft 版本发现、Java 选择、启动参数组装和进程生命周期。
//!
//! 扫描本地目录并发现可用内容
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `discover_game_directory`
//! - `discover_game_directories`
//! - `paths_equal`
//! - `is_game_directory`
//! - `pick_game_directory`
//! - `installed_versions`
//! - `launchable_versions`
//! - `installation_complete`
//! - `official_base_version`
//! - `installed_loader_version`
//! - `find_argument_value`
//! - `find_library_version`
//! - `version_key`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 包含平台相关分支；跨平台修改必须同时保持各 ``cfg`` 分支可编译。

use super::java::has_compatible_java;
use super::version::{load_merged_version, maven_library_path, resolve_profile, rules_allow};
use super::*;

/// 实现 发现游戏目录 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Option<PathBuf>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn discover_game_directory() -> Option<PathBuf> {
    discover_game_directories().into_iter().next()
}

/// 实现 发现游戏directories 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Vec<PathBuf>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn discover_game_directories() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(appdata) = std::env::var_os("APPDATA") {
        candidates.push(PathBuf::from(appdata).join(".minecraft"));
    }
    if let Some(home) = std::env::var_os("USERPROFILE") {
        candidates.push(PathBuf::from(home).join(".minecraft"));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        candidates.push(
            PathBuf::from(local)
                .join("Packages")
                .join("Microsoft.4297127D64EC6_8wekyb3d8bbwe")
                .join("LocalCache")
                .join("Local")
                .join(".minecraft"),
        );
    }
    let mut directories = Vec::new();
    for candidate in candidates {
        if is_game_directory(&candidate)
            && !directories
                .iter()
                .any(|known: &PathBuf| paths_equal(known, &candidate))
        {
            directories.push(candidate);
        }
    }
    directories
}

/// 实现 pathsequal 相关的业务流程。
///
/// # 参数
/// - `right`：由调用方传入的right。
/// # 返回
/// 返回 `bool`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn paths_equal(left: &Path, right: &Path) -> bool {
    let left = left.to_string_lossy();
    let right = right.to_string_lossy();
    if cfg!(windows) {
        left.eq_ignore_ascii_case(&right)
    } else {
        left == right
    }
}

/// 实现 游戏目录 相关的业务流程。
///
/// # 参数
/// - `path`：由调用方传入的路径。
/// # 返回
/// 返回 `bool`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn is_game_directory(path: &Path) -> bool {
    path.join("versions").is_dir()
}

/// 实现 选择游戏目录 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Option<PathBuf>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn pick_game_directory() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let script = "[Console]::OutputEncoding=[Text.Encoding]::UTF8; Add-Type -AssemblyName System.Windows.Forms; $d=New-Object System.Windows.Forms.FolderBrowserDialog; $d.Description='选择 Minecraft 游戏文件夹'; $d.ShowNewFolderButton=$true; if($d.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK){[Console]::Out.Write($d.SelectedPath)}";
        let mut command = Command::new("powershell.exe");
        command
            .args(["-NoProfile", "-STA", "-Command", script])
            .creation_flags(0x0800_0000);
        let output = command.output().ok()?;
        let selected = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        (!selected.is_empty()).then(|| PathBuf::from(selected))
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

/// 实现 installed版本 相关的业务流程。
///
/// # 参数
/// - `game_directory`：由调用方传入的游戏目录。
/// # 返回
/// 返回 `Vec<String>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn installed_versions(game_directory: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(game_directory.join("versions")) else {
        return Vec::new();
    };
    let mut versions: Vec<String> = entries
        .flatten()
        .filter(|entry| {
            let path = entry.path();
            path.file_name()
                .map(|id| {
                    path.join(format!("{}.json", id.to_string_lossy()))
                        .is_file()
                })
                .unwrap_or(false)
        })
        .filter_map(|entry| {
            let path = entry.path();
            let id = path.file_name()?.to_string_lossy().to_string();
            path.join(format!("{id}.json")).is_file().then_some(id)
        })
        .collect();
    versions.sort_by_key(|version| {
        let stable = !version.contains("snapshot")
            && !version.contains("-rc")
            && !version.contains("-pre")
            && !version
                .chars()
                .any(|character| character.is_ascii_alphabetic());
        (stable, version_key(version))
    });
    versions.reverse();
    versions
}

/// 实现 launchable版本 相关的业务流程。
///
/// # 参数
/// - `game_directory`：由调用方传入的游戏目录。
/// # 返回
/// 返回 `Vec<String>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn launchable_versions(game_directory: &Path) -> Vec<String> {
    installed_versions(game_directory)
        .into_iter()
        .filter(|profile| {
            let Ok(merged) = load_merged_version(game_directory, profile) else {
                return false;
            };
            if merged.main_class.is_empty() {
                return false;
            }
            game_directory
                .join("versions")
                .join(&merged.client_jar_version)
                .join(format!("{}.jar", merged.client_jar_version))
                .is_file()
        })
        .collect()
}

/// 实现 installationcomplete 相关的业务流程。
///
/// # 参数
/// - `game_directory`：由调用方传入的游戏目录。
/// - `requested_version`：由调用方传入的requested版本。
/// - `loader`：由调用方传入的加载器。
/// # 返回
/// 返回 `bool`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn installation_complete(game_directory: &Path, requested_version: &str, loader: &str) -> bool {
    let Some(profile) = resolve_profile(game_directory, requested_version, loader) else {
        return false;
    };
    let Ok(merged) = load_merged_version(game_directory, &profile) else {
        return false;
    };
    let client_jar = game_directory
        .join("versions")
        .join(&merged.client_jar_version)
        .join(format!("{}.jar", merged.client_jar_version));
    if !client_jar.is_file() {
        return false;
    }
    let libraries_dir = game_directory.join("libraries");
    for library in &merged.libraries {
        if !rules_allow(library) {
            continue;
        }
        let explicit_path = library
            .pointer("/downloads/artifact/path")
            .and_then(Value::as_str);
        let artifact_path = explicit_path
            .map(str::to_owned)
            .or_else(|| maven_library_path(library));
        if let Some(path) = artifact_path
            && !libraries_dir.join(path).is_file()
        {
            return false;
        }
        if let Some(classifiers) = library
            .pointer("/downloads/classifiers")
            .and_then(Value::as_object)
        {
            for (name, classifier) in classifiers {
                if name.starts_with("natives-windows")
                    && let Some(path) = classifier.get("path").and_then(Value::as_str)
                    && !libraries_dir.join(path).is_file()
                {
                    return false;
                }
            }
        }
    }
    let asset_index = game_directory
        .join("assets")
        .join("indexes")
        .join(format!("{}.json", merged.assets));
    let Ok(index) = fs::read_to_string(&asset_index)
        .ok()
        .and_then(|contents| serde_json::from_str::<Value>(&contents).ok())
        .ok_or(())
    else {
        return false;
    };
    if let Some(objects) = index.get("objects").and_then(Value::as_object) {
        for (name, object) in objects {
            let Some(hash) = object.get("hash").and_then(Value::as_str) else {
                continue;
            };
            if hash.len() < 2 {
                return false;
            }
            let path = game_directory
                .join("assets")
                .join("objects")
                .join(&hash[..2])
                .join(hash);
            if !path.is_file() {
                return false;
            }
            if index
                .get("virtual")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                && !game_directory
                    .join("assets")
                    .join("virtual")
                    .join(&merged.assets)
                    .join(name)
                    .is_file()
            {
                return false;
            }
            if index
                .get("map_to_resources")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                && !game_directory.join("resources").join(name).is_file()
            {
                return false;
            }
        }
    }
    if let Some(required_major) = merged.java_major
        && !has_compatible_java(game_directory, required_major)
    {
        return false;
    }
    true
}

/// 实现 officialbase版本 相关的业务流程。
///
/// # 参数
/// - `game_directory`：由调用方传入的游戏目录。
/// - `requested_version`：由调用方传入的requested版本。
/// - `loader`：由调用方传入的加载器。
/// # 返回
/// 返回 `Option<String>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn official_base_version(
    game_directory: &Path,
    requested_version: &str,
    loader: &str,
) -> Option<String> {
    let profile = resolve_profile(game_directory, requested_version, loader)?;
    official_base_version_for_profile(game_directory, &profile)
}

/// 从任意本地版本配置沿 `inheritsFrom` 链解析官方 Minecraft 基础版本。
///
/// 与 [`official_base_version`] 不同，本函数直接接收本地 profile ID，不要求
/// 调用方同时提供加载器类型；因此也适用于名称完全自定义的整合包实例（例如“愚者”）。
/// 若版本配置没有父版本，则尝试从 `jar`、`clientVersion`、Forge 参数及常见加载器
/// 依赖中推断官方版本；无法推断时保留 profile ID，交由调用方决定是否报错。
pub fn official_base_version_for_profile(
    game_directory: &Path,
    requested_profile: &str,
) -> Option<String> {
    let mut profile = requested_profile.to_owned();
    let mut visited = HashSet::new();
    loop {
        if !visited.insert(profile.clone()) {
            return None;
        }
        let path = game_directory
            .join("versions")
            .join(&profile)
            .join(format!("{profile}.json"));
        let value: Value = serde_json::from_str(&fs::read_to_string(path).ok()?).ok()?;
        if let Some(parent) = value.get("inheritsFrom").and_then(Value::as_str) {
            profile = parent.to_owned();
            continue;
        }
        return infer_minecraft_version_from_profile(&value).or(Some(profile));
    }
}

/// 将整合包清单中的 `dependencies.minecraft` 解析为可下载的官方版本 ID。
///
/// 新导出的整合包应始终写入官方 ID；该函数主要用于兼容旧整合包中被误写成本地
/// profile 名称的字段。已符合官方版本格式的值会原样返回；否则尝试沿本地 version
/// JSON 的继承链恢复基础版本。
pub fn resolve_minecraft_dependency(game_directory: &Path, declared: &str) -> String {
    if looks_like_official_version(declared) {
        return declared.to_owned();
    }
    official_base_version_for_profile(game_directory, declared)
        .filter(|resolved| resolved != declared)
        .unwrap_or_else(|| declared.to_owned())
}

/// 从本地自定义版本配置中恢复加载器类型和版本。
///
/// 旧整合包可能只记录了本地 profile 名称，没有写入 Modrinth 的
/// `fabric-loader`/`forge`/`neoforge` 依赖。导入时读取该 profile 的合并
/// 版本信息，可以恢复原加载器，避免把模组整合包错误安装成纯原版。
pub fn local_profile_loader(
    game_directory: &Path,
    profile: &str,
) -> Option<(&'static str, String)> {
    let merged = load_merged_version(game_directory, profile).ok()?;
    if let Some(version) = find_argument_value(
        &Value::Array(merged.game_args.clone()),
        "--fml.neoForgeVersion",
    ) {
        return Some(("neoforge", version));
    }
    if let Some(version) = find_argument_value(
        &Value::Array(merged.game_args.clone()),
        "--fml.forgeVersion",
    ) {
        return Some(("forge", version));
    }
    for library in &merged.libraries {
        let Some(name) = library.get("name").and_then(Value::as_str) else {
            continue;
        };
        if let Some(version) = name.strip_prefix("net.neoforged:neoforge:") {
            return Some((
                "neoforge",
                version.split(':').next().unwrap_or(version).to_owned(),
            ));
        }
        if let Some(version) = name.strip_prefix("net.neoforged:forge:") {
            return Some((
                "neoforge",
                version.split(':').next().unwrap_or(version).to_owned(),
            ));
        }
        if let Some(version) = name.strip_prefix("net.minecraftforge:forge:") {
            let loader = version
                .split_once('-')
                .map_or(version, |(_, loader)| loader);
            return Some((
                "forge",
                loader.split(':').next().unwrap_or(loader).to_owned(),
            ));
        }
        if let Some(version) = name.strip_prefix("net.fabricmc:fabric-loader:") {
            return Some((
                "fabric",
                version.split(':').next().unwrap_or(version).to_owned(),
            ));
        }
    }
    None
}

/// 判断字符串是否可能是 Mojang 官方版本 ID，而不是本地整合包实例名称。
///
/// 官方 release 以数字开头；常见 snapshot、pre-release 和 release candidate
/// 同样只包含 ASCII 字母、数字、点、下划线和连字符。中文或含空格的实例名会返回 false。
pub fn looks_like_official_version(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && value.starts_with(|character: char| character.is_ascii_digit())
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_')
        })
}

/// 从单个版本 JSON 推断官方 Minecraft 基础版本。
///
/// 优先级依次为显式基础字段、Forge/NeoForge 的 `--fml.mcVersion` 参数、
/// 最后是常见 Maven 依赖名称。该函数只读取元数据，不访问网络。
fn infer_minecraft_version_from_profile(value: &Value) -> Option<String> {
    for pointer in ["/jar", "/clientVersion", "/minecraftVersion"] {
        if let Some(version) = value.pointer(pointer).and_then(Value::as_str)
            && looks_like_official_version(version)
        {
            return Some(version.to_owned());
        }
    }
    if let Some(version) = find_argument_value(value, "--fml.mcVersion")
        && looks_like_official_version(&version)
    {
        return Some(version);
    }
    let libraries = value.get("libraries")?.as_array()?;
    for library in libraries {
        let Some(name) = library.get("name").and_then(Value::as_str) else {
            continue;
        };
        let candidate = if let Some(version) = name
            .strip_prefix("net.minecraftforge:forge:")
            .and_then(|version| version.split_once('-').map(|(minecraft, _)| minecraft))
        {
            Some(version)
        } else if let Some(version) = name
            .strip_prefix("net.neoforged:neoforge:")
            .and_then(|version| version.split_once('-').map(|(minecraft, _)| minecraft))
        {
            Some(version)
        } else if let Some(version) = name.strip_prefix("net.minecraft:client:") {
            Some(version.split(':').next().unwrap_or(version))
        } else if let Some(version) = name
            .strip_prefix("net.fabricmc:intermediary:")
            .or_else(|| name.strip_prefix("net.fabricmc:yarn:"))
        {
            Some(version.split('+').next().unwrap_or(version))
        } else {
            None
        };
        if let Some(candidate) = candidate
            && looks_like_official_version(candidate)
        {
            return Some(candidate.to_owned());
        }
    }
    None
}

/// 实现 installed加载器版本 相关的业务流程。
///
/// # 参数
/// - `game_directory`：由调用方传入的游戏目录。
/// - `profile`：由调用方传入的配置。
/// - `loader`：由调用方传入的加载器。
/// # 返回
/// 返回 `Option<String>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn installed_loader_version(
    game_directory: &Path,
    profile: &str,
    loader: &str,
) -> Option<String> {
    let path = game_directory
        .join("versions")
        .join(profile)
        .join(format!("{profile}.json"));
    let value: Value = serde_json::from_str(&fs::read_to_string(path).ok()?).ok()?;
    let base_version = value
        .get("inheritsFrom")
        .and_then(Value::as_str)
        .unwrap_or(profile);
    let likely_full_version = |version: String| {
        version
            .strip_prefix(&format!("{base_version}-"))
            .map(str::to_owned)
            .unwrap_or(version)
    };
    for argument in ["--fml.neoForgeVersion", "--fml.forgeVersion"] {
        if let Some(version) = find_argument_value(&value, argument) {
            return Some(likely_full_version(version));
        }
    }
    let prefixes: &[&str] = match loader {
        "neoforge" => &["net.neoforged:neoforge:", "net.neoforged:forge:"],
        "forge" => &["net.minecraftforge:forge:", "net.minecraftforge:fmlloader:"],
        "fabric" => &["net.fabricmc:fabric-loader:", "org.quiltmc:quilt-loader:"],
        _ => &[],
    };
    find_library_version(&value, prefixes).map(likely_full_version)
}

/// 实现 查找argumentvalue 相关的业务流程。
///
/// # 参数
/// - `argument`：由调用方传入的argument。
/// # 返回
/// 返回 `Option<String>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn find_argument_value(value: &Value, argument: &str) -> Option<String> {
    match value {
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                if value.as_str() == Some(argument)
                    && let Some(next) = values.get(index + 1).and_then(Value::as_str)
                {
                    return Some(next.to_owned());
                }
            }
            values
                .iter()
                .find_map(|value| find_argument_value(value, argument))
        }
        Value::String(value) => value
            .split_whitespace()
            .collect::<Vec<_>>()
            .windows(2)
            .find(|parts| parts[0] == argument)
            .map(|parts| parts[1].to_owned()),
        Value::Object(object) => object
            .values()
            .find_map(|value| find_argument_value(value, argument)),
        _ => None,
    }
}

/// 实现 查找library版本 相关的业务流程。
///
/// # 参数
/// - `prefixes`：由调用方传入的prefixes。
/// # 返回
/// 返回 `Option<String>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn find_library_version(value: &Value, prefixes: &[&str]) -> Option<String> {
    let libraries = value.get("libraries")?.as_array()?;
    for library in libraries {
        let Some(name) = library.get("name").and_then(Value::as_str) else {
            continue;
        };
        for prefix in prefixes {
            if let Some(version) = name.strip_prefix(prefix) {
                return Some(version.split(':').next().unwrap_or(version).to_owned());
            }
        }
    }
    None
}

/// 实现 版本key 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Vec<u32>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn version_key(version: &str) -> Vec<u32> {
    version
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .collect()
}
