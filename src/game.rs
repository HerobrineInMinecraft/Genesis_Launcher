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

const OFFLINE_WIDE_UUID: &str = "00000000-0000-3000-8000-000000000001";
const OFFLINE_SLIM_UUID: &str = "00000000-0000-3000-8000-00000000000a";
const OFFLINE_NAME: &str = "Steve";

#[derive(Debug)]
pub struct LaunchDetails {
    pub version: String,
    pub java_name: String,
    pub process_id: u32,
}

#[derive(Clone)]
pub struct LaunchOptions {
    pub max_memory_mb: u16,
    pub window_width: u32,
    pub window_height: u32,
    pub fullscreen: bool,
    pub extra_jvm_args: String,
    pub quick_play: Option<QuickPlayTarget>,
    pub data_directory: Option<PathBuf>,
    pub java_path: Option<PathBuf>,
    pub player_name: String,
    pub skin_slim: bool,
}

#[derive(Clone, Debug)]
pub enum QuickPlayTarget {
    World(String),
    Server(String),
}

impl Default for LaunchOptions {
    fn default() -> Self {
        Self {
            max_memory_mb: 6144,
            window_width: 1280,
            window_height: 720,
            fullscreen: false,
            extra_jvm_args: String::new(),
            quick_play: None,
            data_directory: None,
            java_path: None,
            player_name: OFFLINE_NAME.to_owned(),
            skin_slim: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JavaRuntime {
    pub path: PathBuf,
    pub major: u32,
}

#[derive(Clone)]
struct MergedVersion {
    main_class: String,
    assets: String,
    client_jar_version: String,
    java_major: Option<u32>,
    libraries: Vec<Value>,
    game_args: Vec<Value>,
    jvm_args: Vec<Value>,
    legacy_args: Option<String>,
}

struct PreparedLaunch {
    version: String,
    java: PathBuf,
    args: Vec<String>,
}

pub fn discover_game_directory() -> Option<PathBuf> {
    discover_game_directories().into_iter().next()
}

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

fn paths_equal(left: &Path, right: &Path) -> bool {
    let left = left.to_string_lossy();
    let right = right.to_string_lossy();
    if cfg!(windows) {
        left.eq_ignore_ascii_case(&right)
    } else {
        left == right
    }
}

pub fn is_game_directory(path: &Path) -> bool {
    path.join("versions").is_dir()
}

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

pub fn official_base_version(
    game_directory: &Path,
    requested_version: &str,
    loader: &str,
) -> Option<String> {
    let mut profile = resolve_profile(game_directory, requested_version, loader)?;
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
        } else {
            return Some(profile);
        }
    }
}

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

fn find_argument_value(value: &Value, argument: &str) -> Option<String> {
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

fn find_library_version(value: &Value, prefixes: &[&str]) -> Option<String> {
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

fn version_key(version: &str) -> Vec<u32> {
    version
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .collect()
}

pub fn check_launch(
    game_directory: &Path,
    requested_version: &str,
    loader: &str,
) -> Result<(String, PathBuf), String> {
    let prepared = prepare_launch(
        game_directory,
        requested_version,
        loader,
        &LaunchOptions::default(),
    )?;
    Ok((prepared.version, prepared.java))
}

fn prepare_launch(
    game_directory: &Path,
    requested_version: &str,
    loader: &str,
    options: &LaunchOptions,
) -> Result<PreparedLaunch, String> {
    if !is_game_directory(game_directory) {
        return Err("尚未找到有效的 .minecraft 游戏目录".to_owned());
    }
    let profile_id = resolve_profile(game_directory, requested_version, loader)
        .ok_or_else(|| format!("游戏目录中未找到 {requested_version} · {loader}"))?;
    let merged = load_merged_version(game_directory, &profile_id)
        .map_err(|error| format!("读取版本配置失败：{error}"))?;
    if merged.main_class.is_empty() {
        return Err("版本配置缺少 mainClass".to_owned());
    }
    let asset_index = game_directory
        .join("assets")
        .join("indexes")
        .join(format!("{}.json", merged.assets));
    if !asset_index.is_file() {
        return Err(format!("缺少资源索引：{}", asset_index.display()));
    }

    let libraries_dir = game_directory.join("libraries");
    let version_dir = game_directory.join("versions").join(&profile_id);
    let natives_dir = version_dir.join("natives");
    fs::create_dir_all(&natives_dir).map_err(|error| format!("无法创建 natives 目录：{error}"))?;
    for subdirectory in ["java", "jna", "lwjgl", "netty"] {
        fs::create_dir_all(natives_dir.join(subdirectory))
            .map_err(|error| format!("无法创建 natives 子目录：{error}"))?;
    }

    let mut classpath = Vec::new();
    let mut seen = HashSet::new();
    let mut missing_library = None;
    for library in &merged.libraries {
        if !rules_allow(library) {
            continue;
        }
        let explicit_artifact = library
            .pointer("/downloads/artifact/path")
            .and_then(Value::as_str);
        if let Some(artifact) = explicit_artifact
            .map(str::to_owned)
            .or_else(|| maven_library_path(library))
        {
            let path = libraries_dir.join(&artifact);
            if artifact.contains("natives-windows") {
                extract_native_jar(&path, &natives_dir)?;
            }
            if path.is_file() {
                if seen.insert(path.clone()) {
                    classpath.push(path);
                }
            } else if missing_library.is_none() {
                missing_library = Some(path);
            }
        }
        extract_windows_natives(library, &libraries_dir, &natives_dir)?;
    }

    if let Some(path) = missing_library {
        return Err(format!("缺少游戏库：{}", path.display()));
    }
    append_launcher_log(format!(
        "libraries={} classpath_entries={} contains_jopt={}",
        merged.libraries.len(),
        classpath.len(),
        classpath
            .iter()
            .any(|path| path.to_string_lossy().contains("jopt-simple"))
    ));

    let client_version = merged.client_jar_version.clone();
    let client_jar = game_directory
        .join("versions")
        .join(&client_version)
        .join(format!("{client_version}.jar"));
    if !client_jar.is_file() {
        return Err(format!("缺少客户端文件：{}", client_jar.display()));
    }
    classpath.insert(0, client_jar);

    let java = if let Some(java) = options.java_path.as_ref() {
        if !java.is_file() {
            return Err(format!("指定的 Java 不存在：{}", java.display()));
        }
        let actual = java_major_version(java)
            .ok_or_else(|| format!("无法识别指定 Java 的版本：{}", java.display()))?;
        if let Some(required) = merged.java_major
            && actual < required
        {
            return Err(format!(
                "指定 Java {actual} 低于所需 Java {required}：{}",
                java.display()
            ));
        }
        java.clone()
    } else {
        find_compatible_java(game_directory, merged.java_major).ok_or_else(|| {
            merged
                .java_major
                .map(|major| format!("未找到兼容的 Java {major} 或更高版本运行时"))
                .unwrap_or_else(|| "未找到 Java 运行时".to_owned())
        })?
    };
    let separator = if cfg!(windows) { ";" } else { ":" };
    let classpath_value = classpath
        .iter()
        .map(|path| path.to_string_lossy())
        .collect::<Vec<_>>()
        .join(separator);
    let data_directory = options.data_directory.as_deref().unwrap_or(game_directory);
    fs::create_dir_all(data_directory).map_err(|error| format!("无法创建版本隔离目录：{error}"))?;
    let mut placeholders = placeholders(
        game_directory,
        data_directory,
        &profile_id,
        &merged.assets,
        &natives_dir,
        &libraries_dir,
        &classpath_value,
        separator,
        &options.player_name,
        options.skin_slim,
    );
    if let Some(quick_play) = &options.quick_play {
        let (key, value) = match quick_play {
            QuickPlayTarget::World(world) => ("quickPlaySingleplayer", world),
            QuickPlayTarget::Server(server) => ("quickPlayMultiplayer", server),
        };
        placeholders.insert(key, value.clone());
    }

    let mut args = vec![
        "-Xms512M".to_owned(),
        format!("-Xmx{}M", options.max_memory_mb),
        format!("-Dminecraft.launcher.brand={}", brand::COMPACT_NAME),
        format!("-Dminecraft.launcher.version={}", env!("CARGO_PKG_VERSION")),
        "-Dlog4j2.formatMsgNoLookups=true".to_owned(),
    ];
    for value in &merged.jvm_args {
        collect_arguments(value, &placeholders, &mut args);
    }
    if !args
        .iter()
        .any(|arg| arg.starts_with("-Djava.library.path="))
    {
        args.push(format!("-Djava.library.path={}", natives_dir.display()));
    }
    if !args
        .iter()
        .any(|arg| arg == "-cp" || arg == "-classpath" || arg.starts_with("-cp="))
    {
        args.push("-cp".to_owned());
        args.push(classpath_value);
    }
    args.extend(split_command_line(&options.extra_jvm_args));

    let mut game_args = Vec::new();
    for value in &merged.game_args {
        collect_arguments(value, &placeholders, &mut game_args);
    }
    if let Some(legacy) = &merged.legacy_args {
        for token in legacy.split_whitespace() {
            game_args.push(replace_placeholders(token, &placeholders));
        }
    }
    if game_args.is_empty() {
        game_args = legacy_game_args(&placeholders);
    }
    if merged
        .game_args
        .iter()
        .any(|value| value_contains(value, "quickPlay"))
    {
        match &options.quick_play {
            Some(QuickPlayTarget::World(world))
                if !game_args
                    .iter()
                    .any(|argument| argument == "--quickPlaySingleplayer") =>
            {
                game_args.push("--quickPlaySingleplayer".to_owned());
                game_args.push(world.clone());
            }
            Some(QuickPlayTarget::Server(server))
                if !game_args
                    .iter()
                    .any(|argument| argument == "--quickPlayMultiplayer") =>
            {
                game_args.push("--quickPlayMultiplayer".to_owned());
                game_args.push(server.clone());
            }
            _ => {}
        }
    }
    if !game_args.iter().any(|argument| argument == "--width") {
        game_args.push("--width".to_owned());
        game_args.push(options.window_width.to_string());
    }
    if !game_args.iter().any(|argument| argument == "--height") {
        game_args.push("--height".to_owned());
        game_args.push(options.window_height.to_string());
    }
    if options.fullscreen && !game_args.iter().any(|argument| argument == "--fullscreen") {
        game_args.push("--fullscreen".to_owned());
    }

    args.push(replace_placeholders(&merged.main_class, &placeholders));
    args.extend(game_args);

    Ok(PreparedLaunch {
        version: profile_id,
        java,
        args,
    })
}

pub fn launch(
    game_directory: &Path,
    requested_version: &str,
    loader: &str,
    options: &LaunchOptions,
) -> Result<LaunchDetails, String> {
    let prepared = prepare_launch(game_directory, requested_version, loader, options)?;
    let logs_dir = game_directory.join("logs");
    fs::create_dir_all(&logs_dir).map_err(|error| format!("无法创建日志目录：{error}"))?;
    let output_path = logs_dir.join("launcher-java.log");
    let stdout =
        File::create(&output_path).map_err(|error| format!("无法创建启动日志：{error}"))?;
    let stderr = stdout
        .try_clone()
        .map_err(|error| format!("无法复制启动日志句柄：{error}"))?;
    let mut command = Command::new(&prepared.java);
    command
        .args(&prepared.args)
        .current_dir(game_directory)
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    append_launcher_log(format!(
        "launch version={} java={} memory={}MB",
        prepared.version,
        prepared.java.display(),
        options.max_memory_mb
    ));
    append_launcher_log(format!("arguments={:?}", prepared.args));
    let mut child = command.spawn().map_err(|error| {
        append_launcher_log(format!("spawn failed: {error}"));
        format!("无法启动 Java 进程：{error}")
    })?;
    let process_id = child.id();
    append_launcher_log(format!("java process started: pid={process_id}"));

    thread::sleep(Duration::from_millis(900));
    if let Some(status) = child
        .try_wait()
        .map_err(|error| format!("无法读取 Java 进程状态：{error}"))?
    {
        append_launcher_log(format!("java process exited early: {status}"));
        return Err(format!(
            "Java 进程提前退出（{status}），请查看 logs/launcher-java.log"
        ));
    }

    let pid = process_id;
    thread::spawn(move || {
        focus_game_window(pid);
    });
    thread::spawn(move || match child.wait() {
        Ok(status) => append_launcher_log(format!("java process exited: {status}")),
        Err(error) => append_launcher_log(format!("wait failed: {error}")),
    });

    Ok(LaunchDetails {
        version: prepared.version,
        java_name: prepared
            .java
            .file_name()
            .unwrap_or_else(|| prepared.java.as_os_str())
            .to_string_lossy()
            .to_string(),
        process_id,
    })
}

fn value_contains(value: &Value, needle: &str) -> bool {
    match value {
        Value::String(value) => value.contains(needle),
        Value::Array(values) => values.iter().any(|value| value_contains(value, needle)),
        Value::Object(object) => object.values().any(|value| value_contains(value, needle)),
        _ => false,
    }
}

fn split_command_line(value: &str) -> Vec<String> {
    let mut arguments = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for character in value.chars() {
        match character {
            '"' => quoted = !quoted,
            ' ' | '\t' if !quoted => {
                if !current.is_empty() {
                    arguments.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(character),
        }
    }
    if !current.is_empty() {
        arguments.push(current);
    }
    arguments
}

pub fn is_game_running(process_id: u32) -> bool {
    process_is_java(process_id)
}

pub fn force_close_game(process_id: u32) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE, TerminateProcess,
        };

        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
                0,
                process_id,
            )
        };
        if handle.is_null() {
            return Err("游戏进程已经结束".to_owned());
        }
        let is_java = process_handle_is_java(handle);
        if !is_java {
            unsafe {
                CloseHandle(handle);
            }
            return Err("目标进程已变化，已取消强制关闭".to_owned());
        }
        let succeeded = unsafe { TerminateProcess(handle, 1) };
        unsafe {
            CloseHandle(handle);
        }
        if succeeded != 0 {
            append_launcher_log(format!("game process force closed: pid={process_id}"));
            Ok(())
        } else {
            Err("无法强制关闭游戏进程".to_owned())
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = process_id;
        Err("当前平台暂不支持强制关闭".to_owned())
    }
}

fn process_is_java(process_id: u32) -> bool {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };

        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
        if handle.is_null() {
            return false;
        }
        let is_java = process_handle_is_java(handle);
        unsafe {
            CloseHandle(handle);
        }
        is_java
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = process_id;
        false
    }
}

#[cfg(target_os = "windows")]
fn process_handle_is_java(handle: windows_sys::Win32::Foundation::HANDLE) -> bool {
    use windows_sys::Win32::System::Threading::QueryFullProcessImageNameW;

    let mut buffer = vec![0_u16; 1024];
    let mut size = buffer.len() as u32;
    let succeeded =
        unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut size) };
    if succeeded == 0 {
        return false;
    }
    let path = String::from_utf16_lossy(&buffer[..size as usize]).to_lowercase();
    path.ends_with("java.exe") || path.ends_with("javaw.exe")
}

fn resolve_profile(game_directory: &Path, version: &str, loader: &str) -> Option<String> {
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

fn load_merged_version(game_directory: &Path, id: &str) -> Result<MergedVersion, String> {
    load_merged_version_inner(game_directory, id, &mut HashSet::new())
}

fn load_merged_version_inner(
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

fn append_array(value: &Value, pointer: &str, target: &mut Vec<Value>) {
    let source = if pointer.is_empty() {
        value
    } else {
        value.pointer(pointer).unwrap_or(&Value::Null)
    };
    if let Some(items) = source.as_array() {
        target.extend(items.iter().cloned());
    }
}

fn rules_allow(value: &Value) -> bool {
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

fn rule_matches(rule: &Value) -> bool {
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

fn collect_arguments(
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

fn maven_library_path(library: &Value) -> Option<String> {
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

fn extract_windows_natives(
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

fn extract_native_jar(archive_path: &Path, natives_dir: &Path) -> Result<(), String> {
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

pub fn has_compatible_java(game_directory: &Path, required_major: u32) -> bool {
    find_compatible_java(game_directory, Some(required_major)).is_some()
}

pub fn java_for_loader_install(game_directory: &Path, version: &str) -> Option<PathBuf> {
    let required = load_merged_version(game_directory, version)
        .ok()
        .and_then(|merged| merged.java_major);
    find_compatible_java(game_directory, required)
}

pub fn version_java_requirement(game_directory: &Path, profile: &str) -> Option<u32> {
    load_merged_version(game_directory, profile)
        .ok()
        .and_then(|version| version.java_major)
}

pub fn discover_java_runtimes(game_directories: &[PathBuf]) -> Vec<JavaRuntime> {
    let executable = if cfg!(windows) { "java.exe" } else { "java" };
    let mut candidates = Vec::new();
    for game_directory in game_directories {
        let runtime = game_directory.join("runtime");
        if runtime.is_dir() {
            collect_java_candidates(&runtime, executable, 7, &mut candidates);
        }
    }
    collect_common_java_candidates(executable, &mut candidates);
    let mut seen = HashSet::new();
    let mut runtimes = candidates
        .into_iter()
        .filter(|path| path.is_file())
        .filter_map(|path| {
            let key = path.to_string_lossy().to_lowercase();
            if !seen.insert(key) {
                return None;
            }
            let major = java_major_version(&path)?;
            Some(JavaRuntime { path, major })
        })
        .collect::<Vec<_>>();
    runtimes.sort_by(|left, right| {
        right
            .major
            .cmp(&left.major)
            .then_with(|| left.path.cmp(&right.path))
    });
    runtimes
}

fn find_compatible_java(game_directory: &Path, required_major: Option<u32>) -> Option<PathBuf> {
    let executable = if cfg!(windows) { "java.exe" } else { "java" };
    let mut candidates = Vec::new();
    let runtime = game_directory.join("runtime");
    if runtime.is_dir() {
        collect_java_candidates(&runtime, executable, 7, &mut candidates);
    }
    collect_common_java_candidates(executable, &mut candidates);
    candidates.into_iter().find(|path| {
        path.is_file()
            && required_major.is_none_or(|required| {
                java_major_version(path).is_some_and(|actual| actual >= required)
            })
    })
}

fn collect_common_java_candidates(executable: &str, candidates: &mut Vec<PathBuf>) {
    if let Some(java_home) = std::env::var_os("JAVA_HOME") {
        candidates.push(PathBuf::from(java_home).join("bin").join(executable));
    }
    for root in [
        r"C:\Program Files\Java",
        r"C:\Program Files\Eclipse Adoptium",
        r"C:\Program Files\Microsoft",
        r"C:\Program Files\Zulu",
        r"C:\Program Files\Amazon Corretto",
        r"C:\Program Files\BellSoft",
    ] {
        collect_java_candidates(Path::new(root), executable, 5, &mut *candidates);
    }
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            let candidate = directory.join(executable);
            if candidate.is_file() {
                candidates.push(candidate);
            }
        }
    }
}

fn java_major_version(java: &Path) -> Option<u32> {
    let mut command = Command::new(java);
    command.arg("-version");
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let output = command.output().ok()?;
    let mut version_text = String::from_utf8_lossy(&output.stderr).to_string();
    version_text.push_str(&String::from_utf8_lossy(&output.stdout));
    let marker = version_text.find("version \"")? + "version \"".len();
    let version = version_text[marker..].split('"').next()?;
    let mut parts = version.split('.');
    let first = parts.next()?.parse::<u32>().ok()?;
    if first == 1 {
        parts.next()?.parse().ok()
    } else {
        Some(first)
    }
}

fn collect_java_candidates(root: &Path, executable: &str, depth: usize, output: &mut Vec<PathBuf>) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_java_candidates(&path, executable, depth - 1, output);
        } else if path
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case(executable))
        {
            output.push(path);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn placeholders(
    game_directory: &Path,
    data_directory: &Path,
    version: &str,
    assets: &str,
    natives_dir: &Path,
    libraries_dir: &Path,
    classpath: &str,
    separator: &str,
    player_name: &str,
    skin_slim: bool,
) -> HashMap<&'static str, String> {
    HashMap::from([
        ("auth_player_name", player_name.to_owned()),
        ("version_name", version.to_owned()),
        (
            "game_directory",
            data_directory.to_string_lossy().to_string(),
        ),
        (
            "assets_root",
            game_directory.join("assets").to_string_lossy().to_string(),
        ),
        ("assets_index_name", assets.to_owned()),
        (
            "auth_uuid",
            if skin_slim {
                OFFLINE_SLIM_UUID
            } else {
                OFFLINE_WIDE_UUID
            }
            .to_owned(),
        ),
        ("auth_access_token", "0".to_owned()),
        ("auth_xuid", "0".to_owned()),
        ("clientid", "0".to_owned()),
        ("auth_session", "0".to_owned()),
        ("user_type", "legacy".to_owned()),
        ("version_type", "release".to_owned()),
        (
            "natives_directory",
            natives_dir.to_string_lossy().to_string(),
        ),
        (
            "library_directory",
            libraries_dir.to_string_lossy().to_string(),
        ),
        ("classpath", classpath.to_owned()),
        ("classpath_separator", separator.to_owned()),
        ("launcher_name", brand::NAME.to_owned()),
        ("launcher_version", env!("CARGO_PKG_VERSION").to_owned()),
        ("resolution_width", "1280".to_owned()),
        ("resolution_height", "720".to_owned()),
        ("quickPlayPath", String::new()),
        ("quickPlaySingleplayer", String::new()),
        ("quickPlayMultiplayer", String::new()),
        ("quickPlayRealms", String::new()),
        ("path", String::new()),
    ])
}

fn replace_placeholders(value: &str, placeholders: &HashMap<&str, String>) -> String {
    let mut result = value.to_owned();
    for (key, replacement) in placeholders {
        result = result.replace(&format!("${{{key}}}"), replacement);
    }
    result
}

fn legacy_game_args(placeholders: &HashMap<&str, String>) -> Vec<String> {
    let mut args = Vec::new();
    for key in [
        "auth_player_name",
        "version_name",
        "game_directory",
        "assets_root",
        "assets_index_name",
        "auth_uuid",
        "auth_access_token",
        "user_type",
        "version_type",
    ] {
        args.push(format!("--{key}"));
        args.push(placeholders.get(key).cloned().unwrap_or_default());
    }
    args.push("--userProperties".to_owned());
    args.push("{}".to_owned());
    args
}

fn append_launcher_log(message: String) {
    let directory = brand::data_directory();
    let path = directory.join("launcher.log");
    let _ = fs::create_dir_all(directory);
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{message}");
    }
}

#[cfg(target_os = "windows")]
fn focus_game_window(process_id: u32) {
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowThreadProcessId, IsWindowVisible, SW_RESTORE, SetForegroundWindow,
        ShowWindow,
    };

    struct Search {
        process_id: u32,
        window: HWND,
    }

    unsafe extern "system" fn find_window(window: HWND, parameter: LPARAM) -> i32 {
        let search = unsafe { &mut *(parameter as *mut Search) };
        let mut process_id = 0_u32;
        unsafe {
            GetWindowThreadProcessId(window, &mut process_id);
        }
        if process_id == search.process_id && unsafe { IsWindowVisible(window) } != 0 {
            search.window = window;
            return 0;
        }
        1
    }

    for _ in 0..60 {
        let mut search = Search {
            process_id,
            window: std::ptr::null_mut(),
        };
        unsafe {
            EnumWindows(Some(find_window), (&mut search as *mut Search) as LPARAM);
        }
        if !search.window.is_null() {
            unsafe {
                ShowWindow(search.window, SW_RESTORE);
                SetForegroundWindow(search.window);
            }
            append_launcher_log(format!("focused game window for pid={process_id}"));
            return;
        }
        thread::sleep(Duration::from_millis(200));
    }
    append_launcher_log(format!("game window not found for pid={process_id}"));
}

#[cfg(not(target_os = "windows"))]
fn focus_game_window(_process_id: u32) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
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
    fn offline_uuids_select_the_requested_player_model() {
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
}
