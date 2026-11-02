//! # `src/game/launch_args.rs`
//!
//! **功能域：** Minecraft 版本发现、Java 选择、启动参数组装和进程生命周期。
//!
//! 构造占位符、JVM 参数和游戏参数
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `placeholders`
//! - `replace_placeholders`
//! - `legacy_game_args`
//! - `append_launcher_log`
//! - `focus_game_window`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 实现中包含线程、通道或共享状态；同步边界必须与现有轮询和生命周期保持一致。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 包含平台相关分支；跨平台修改必须同时保持各 ``cfg`` 分支可编译。

use super::*;

#[allow(clippy::too_many_arguments)]
/// 实现 placeholders 相关的业务流程。
///
/// # 参数
/// - `data_directory`：由调用方传入的数据目录。
/// - `version`：由调用方传入的版本。
/// - `assets`：由调用方传入的assets。
/// - `natives_dir`：由调用方传入的nativesdir。
/// - `libraries_dir`：由调用方传入的librariesdir。
/// - `classpath`：由调用方传入的classpath。
/// - `separator`：由调用方传入的separator。
/// - `player_name`：由调用方传入的玩家名称。
/// - `skin_slim`：由调用方传入的皮肤slim。
/// # 返回
/// 返回 `HashMap<&'static str, String>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn placeholders(
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

/// 实现 replaceplaceholders 相关的业务流程。
///
/// # 参数
/// - `placeholders`：由调用方传入的placeholders。
/// # 返回
/// 返回 `String`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn replace_placeholders(value: &str, placeholders: &HashMap<&str, String>) -> String {
    let mut result = value.to_owned();
    for (key, replacement) in placeholders {
        result = result.replace(&format!("${{{key}}}"), replacement);
    }
    result
}

/// 实现 legacy游戏参数 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Vec<String>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn legacy_game_args(placeholders: &HashMap<&str, String>) -> Vec<String> {
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

/// 实现 appendlauncherlog 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn append_launcher_log(message: String) {
    let directory = brand::data_directory();
    let path = directory.join("launcher.log");
    let _ = fs::create_dir_all(directory);
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{message}");
    }
}

#[cfg(target_os = "windows")]
/// 实现 focus游戏窗口 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn focus_game_window(process_id: u32) {
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowThreadProcessId, IsWindowVisible, SW_RESTORE, SetForegroundWindow,
        ShowWindow,
    };

    /// 定义 搜索 数据结构及其跨模块传递的状态。
    ///
    /// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
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
/// 实现 focus游戏窗口 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn focus_game_window(_process_id: u32) {}
