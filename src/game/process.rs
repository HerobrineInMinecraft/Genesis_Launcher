//! # `src/game/process.rs`
//!
//! **功能域：** Minecraft 版本发现、Java 选择、启动参数组装和进程生命周期。
//!
//! 检查 Java 游戏进程并执行受控关闭
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `is_game_running`
//! - `force_close_game`
//! - `process_is_java`
//! - `process_handle_is_java`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 包含平台相关分支；跨平台修改必须同时保持各 ``cfg`` 分支可编译。

use super::launch_args::append_launcher_log;

/// 实现 游戏running 相关的业务流程。
///
/// # 参数
/// - `process_id`：由调用方传入的进程。
/// # 返回
/// 返回 `bool`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn is_game_running(process_id: u32) -> bool {
    process_is_java(process_id)
}

/// 实现 force关闭游戏 相关的业务流程。
///
/// # 参数
/// - `process_id`：由调用方传入的进程。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
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

/// 实现 进程Java 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `bool`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn process_is_java(process_id: u32) -> bool {
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
/// 实现 进程handleJava 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `bool`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn process_handle_is_java(handle: windows_sys::Win32::Foundation::HANDLE) -> bool {
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
