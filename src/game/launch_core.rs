//! # `src/game/launch_core.rs`
//!
//! **功能域：** Minecraft 版本发现、Java 选择、启动参数组装和进程生命周期。
//!
//! Minecraft 启动核心。负责启动前检查、JVM 参数、占位符替换、工作目录设置和进程创建。
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `check_launch`
//! - `prepare_launch`
//! - `launch`
//! - `value_contains`
//! - `split_command_line`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 实现中包含线程、通道或共享状态；同步边界必须与现有轮询和生命周期保持一致。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 包含平台相关分支；跨平台修改必须同时保持各 ``cfg`` 分支可编译。

use super::java::{find_compatible_java, java_major_version};
use super::launch_args::{
    append_launcher_log, focus_game_window, legacy_game_args, placeholders, replace_placeholders,
};
use super::version::{
    collect_arguments, extract_native_jar, extract_windows_natives, load_merged_version,
    maven_library_path, resolve_profile, rules_allow,
};
use super::*;

/// 实现 检查启动 相关的业务流程。
///
/// # 参数
/// - `game_directory`：由调用方传入的游戏目录。
/// - `requested_version`：由调用方传入的requested版本。
/// - `loader`：由调用方传入的加载器。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
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

/// 实现 prepare启动 相关的业务流程。
///
/// # 参数
/// - `requested_version`：由调用方传入的requested版本。
/// - `loader`：由调用方传入的加载器。
/// - `options`：由调用方传入的options。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn prepare_launch(
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

/// 实现 启动 相关的业务流程。
///
/// # 参数
/// - `game_directory`：由调用方传入的游戏目录。
/// - `requested_version`：由调用方传入的requested版本。
/// - `loader`：由调用方传入的加载器。
/// - `options`：由调用方传入的options。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
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

/// 实现 valuecontains 相关的业务流程。
///
/// # 参数
/// - `needle`：由调用方传入的needle。
/// # 返回
/// 返回 `bool`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn value_contains(value: &Value, needle: &str) -> bool {
    match value {
        Value::String(value) => value.contains(needle),
        Value::Array(values) => values.iter().any(|value| value_contains(value, needle)),
        Value::Object(object) => object.values().any(|value| value_contains(value, needle)),
        _ => false,
    }
}

/// 实现 splitcommandline 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Vec<String>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn split_command_line(value: &str) -> Vec<String> {
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
