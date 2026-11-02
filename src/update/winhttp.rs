//! # `src/update/winhttp.rs`
//!
//! **功能域：** Genesis Launcher GitHub Release 检查、下载和程序替换。
//!
//! Windows 原生 HTTPS 传输层。直接调用 WinHTTP，避免自更新过程依赖浏览器或 WebView。
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `latest_release_body`
//! - `download_release_asset`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件位于外部数据或文件写入边界，不得绕过 URL、路径、哈希和大小限制。
//! - 包含平台相关分支；跨平台修改必须同时保持各 ``cfg`` 分支可编译。

use std::ffi::c_void;

use windows_sys::Win32::Networking::WinHttp::{
    WINHTTP_FLAG_SECURE, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle,
    WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders, WinHttpReadData,
    WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetTimeouts,
};

use crate::brand;

use super::model::ReleaseAsset;

/// 定义 Handle 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
struct Handle(*mut c_void);

impl Drop for Handle {
    /// 实现 drop 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                WinHttpCloseHandle(self.0);
            }
        }
    }
}

/// 实现 wide 相关的业务流程。
///
/// # 参数
/// - `value`：由调用方传入的value。
/// # 返回
/// 返回 `Vec<u16>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

/// 实现 最新发行版主体 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn latest_release_body(path: &str) -> Result<Vec<u8>, String> {
    let agent = wide(brand::USER_AGENT);
    let session =
        Handle(unsafe { WinHttpOpen(agent.as_ptr(), 0, std::ptr::null(), std::ptr::null(), 0) });
    if session.0.is_null() {
        return Err(format!(
            "无法初始化 Windows HTTPS：{}",
            std::io::Error::last_os_error()
        ));
    }
    unsafe {
        WinHttpSetTimeouts(session.0, 8_000, 8_000, 15_000, 20_000);
    }

    let host = wide("api.github.com");
    let connection = Handle(unsafe { WinHttpConnect(session.0, host.as_ptr(), 443, 0) });
    if connection.0.is_null() {
        return Err(format!(
            "无法连接 GitHub：{}",
            std::io::Error::last_os_error()
        ));
    }

    let method = wide("GET");
    let request_path = wide(path);
    let request = Handle(unsafe {
        WinHttpOpenRequest(
            connection.0,
            method.as_ptr(),
            request_path.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        )
    });
    if request.0.is_null() {
        return Err(format!(
            "无法创建 GitHub 请求：{}",
            std::io::Error::last_os_error()
        ));
    }

    let headers = wide(&format!(
        "Accept: application/vnd.github+json\r\nUser-Agent: {}\r\n",
        brand::USER_AGENT
    ));
    let sent = unsafe {
        WinHttpSendRequest(
            request.0,
            headers.as_ptr(),
            headers.len().saturating_sub(1) as u32,
            std::ptr::null(),
            0,
            0,
            0,
        )
    };
    if sent == 0 {
        return Err(format!(
            "发送 GitHub 请求失败：{}",
            std::io::Error::last_os_error()
        ));
    }
    if unsafe { WinHttpReceiveResponse(request.0, std::ptr::null_mut()) } == 0 {
        return Err(format!(
            "接收 GitHub 响应失败：{}",
            std::io::Error::last_os_error()
        ));
    }

    let status = response_status(request.0, "读取 GitHub 响应状态失败")?;
    if status == 404 {
        return Err("GitHub 仓库暂未发布正式版本".to_owned());
    }
    if status != 200 {
        return Err(format!("GitHub Releases 返回 HTTP {status}"));
    }

    let mut body = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let mut read = 0_u32;
        let succeeded = unsafe {
            WinHttpReadData(
                request.0,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut read,
            )
        };
        if succeeded == 0 {
            return Err(format!(
                "读取 GitHub 响应失败：{}",
                std::io::Error::last_os_error()
            ));
        }
        if read == 0 {
            break;
        }
        if body.len() + read as usize > 2 * 1024 * 1024 {
            return Err("GitHub 更新信息过大".to_owned());
        }
        body.extend_from_slice(&buffer[..read as usize]);
    }
    Ok(body)
}

/// 实现 下载发行版asset 相关的业务流程。
///
/// # 参数
/// - `on_progress`：由调用方传入的进度。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn download_release_asset(
    asset: &ReleaseAsset,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<Vec<u8>, String> {
    let rest = asset
        .url
        .strip_prefix("https://")
        .ok_or_else(|| "更新下载地址必须使用 HTTPS".to_owned())?;
    let (host, path) = rest
        .split_once('/')
        .ok_or_else(|| "更新下载地址格式无效".to_owned())?;
    let agent = wide(brand::USER_AGENT);
    let session =
        Handle(unsafe { WinHttpOpen(agent.as_ptr(), 0, std::ptr::null(), std::ptr::null(), 0) });
    if session.0.is_null() {
        return Err(format!(
            "无法初始化更新下载：{}",
            std::io::Error::last_os_error()
        ));
    }
    unsafe {
        WinHttpSetTimeouts(session.0, 8_000, 15_000, 30_000, 60_000);
    }
    let host = wide(host);
    let connection = Handle(unsafe { WinHttpConnect(session.0, host.as_ptr(), 443, 0) });
    if connection.0.is_null() {
        return Err(format!(
            "无法连接更新服务器：{}",
            std::io::Error::last_os_error()
        ));
    }
    let method = wide("GET");
    let request_path = wide(&format!("/{path}"));
    let request = Handle(unsafe {
        WinHttpOpenRequest(
            connection.0,
            method.as_ptr(),
            request_path.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        )
    });
    if request.0.is_null() {
        return Err(format!(
            "无法创建更新下载请求：{}",
            std::io::Error::last_os_error()
        ));
    }
    let headers = wide(&format!("User-Agent: {}\r\n", brand::USER_AGENT));
    if unsafe {
        WinHttpSendRequest(
            request.0,
            headers.as_ptr(),
            headers.len().saturating_sub(1) as u32,
            std::ptr::null(),
            0,
            0,
            0,
        )
    } == 0
    {
        return Err(format!(
            "发送更新下载请求失败：{}",
            std::io::Error::last_os_error()
        ));
    }
    if unsafe { WinHttpReceiveResponse(request.0, std::ptr::null_mut()) } == 0 {
        return Err(format!(
            "接收更新下载响应失败：{}",
            std::io::Error::last_os_error()
        ));
    }
    let status = response_status(request.0, "读取更新下载状态失败")?;
    if status != 200 {
        return Err(format!("更新下载服务器返回 HTTP {status}"));
    }
    let mut body = Vec::with_capacity(asset.size.min(128 * 1024 * 1024) as usize);
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let mut read = 0_u32;
        if unsafe {
            WinHttpReadData(
                request.0,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut read,
            )
        } == 0
        {
            return Err(format!(
                "读取更新文件失败：{}",
                std::io::Error::last_os_error()
            ));
        }
        if read == 0 {
            break;
        }
        if body.len() + read as usize > 128 * 1024 * 1024 {
            return Err("更新文件超过 128 MB".to_owned());
        }
        body.extend_from_slice(&buffer[..read as usize]);
        on_progress(body.len() as u64, asset.size);
    }
    if body.len() < 2 || body[0] != b'M' || body[1] != b'Z' {
        return Err("下载的更新文件不是有效的 Windows 程序".to_owned());
    }
    if asset.size > 0 && body.len() as u64 != asset.size {
        return Err(format!(
            "更新文件大小不匹配：应为 {} 字节，实际 {} 字节",
            asset.size,
            body.len()
        ));
    }
    Ok(body)
}

/// 实现 responsestatus 相关的业务流程。
///
/// # 参数
/// - `request`：由调用方传入的请求。
/// - `error_prefix`：由调用方传入的errorprefix。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn response_status(request: *mut c_void, error_prefix: &str) -> Result<u32, String> {
    let mut status = 0_u32;
    let mut status_size = std::mem::size_of::<u32>() as u32;
    let mut index = 0_u32;
    let queried = unsafe {
        WinHttpQueryHeaders(
            request,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            std::ptr::null(),
            (&mut status as *mut u32).cast(),
            &mut status_size,
            &mut index,
        )
    };
    if queried == 0 {
        return Err(format!(
            "{error_prefix}：{}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(status)
}
