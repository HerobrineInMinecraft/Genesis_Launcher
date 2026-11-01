use std::sync::mpsc;
use std::thread;

use eframe::egui;
use serde_json::Value;

use crate::brand;

#[cfg(not(target_os = "windows"))]
use std::io::Read;
#[cfg(not(target_os = "windows"))]
use std::time::Duration;

pub const RELEASES_URL: &str = "https://github.com/HerobrineInMinecraft/Genesis_Launcher/releases";
#[cfg(not(target_os = "windows"))]
const LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/HerobrineInMinecraft/Genesis_Launcher/releases/latest";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LauncherRelease {
    pub version: String,
    pub name: String,
    pub page_url: String,
    pub asset: Option<ReleaseAsset>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub name: String,
    pub url: String,
    pub size: u64,
    pub sha256: Option<String>,
}

#[derive(Debug)]
pub enum ForcedUpdateEvent {
    Progress { downloaded: u64, total: u64 },
    Finished(Result<std::path::PathBuf, String>),
}

pub fn start_latest_release_check(
    ctx: &egui::Context,
) -> mpsc::Receiver<Result<LauncherRelease, String>> {
    let (sender, receiver) = mpsc::channel();
    let repaint = ctx.clone();
    thread::spawn(move || {
        let result = fetch_latest_release();
        let _ = sender.send(result);
        repaint.request_repaint();
    });
    receiver
}

pub fn start_update_download(
    asset: ReleaseAsset,
    ctx: &egui::Context,
) -> mpsc::Receiver<ForcedUpdateEvent> {
    let (sender, receiver) = mpsc::channel();
    let repaint = ctx.clone();
    thread::spawn(move || {
        let progress_sender = sender.clone();
        let progress_repaint = repaint.clone();
        let result = download_release_asset(&asset, move |downloaded, total| {
            let _ = progress_sender.send(ForcedUpdateEvent::Progress { downloaded, total });
            progress_repaint.request_repaint();
        })
        .and_then(|bytes| {
            let directory = brand::data_directory().join("updates");
            std::fs::create_dir_all(&directory)
                .map_err(|error| format!("无法创建更新目录：{error}"))?;
            let path = directory.join(format!(
                "Genesis-Launcher-{}-{}.exe",
                asset.version_suffix(),
                std::process::id()
            ));
            std::fs::write(&path, bytes).map_err(|error| format!("无法保存更新文件：{error}"))?;
            Ok(path)
        });
        let _ = sender.send(ForcedUpdateEvent::Finished(result));
        repaint.request_repaint();
    });
    receiver
}

impl ReleaseAsset {
    fn version_suffix(&self) -> String {
        self.name
            .chars()
            .filter(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_')
            })
            .collect()
    }
}

pub fn download_release_asset_for_report(
    asset: &ReleaseAsset,
) -> Result<std::path::PathBuf, String> {
    let bytes = download_release_asset(asset, |_, _| {})?;
    let path = std::env::temp_dir().join(format!(
        "genesis-launcher-update-{}.exe",
        std::process::id()
    ));
    std::fs::write(&path, bytes).map_err(|error| format!("无法保存测试更新文件：{error}"))?;
    Ok(path)
}

pub fn launch_update_installer(
    downloaded: &std::path::Path,
    expected_sha256: Option<&str>,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;

        let current =
            std::env::current_exe().map_err(|error| format!("无法定位当前启动器：{error}"))?;
        let script =
            brand::data_directory().join(format!("updates/install-{}.ps1", std::process::id()));
        if let Some(parent) = script.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("无法创建更新目录：{error}"))?;
        }
        let quote = |value: &str| format!("'{}'", value.replace('\'', "''"));
        let source = quote(&downloaded.to_string_lossy());
        let target = quote(&current.to_string_lossy());
        let expected = quote(expected_sha256.unwrap_or_default());
        let script_path = quote(&script.to_string_lossy());
        let body = format!(
            r#"$ErrorActionPreference = 'Stop'
Wait-Process -Id {pid} -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 600
$source = {source}
$target = {target}
$expected = {expected}
$staged = "$target.new"
$backup = "$target.previous"
try {{
    if ($expected -ne '') {{
        $actual = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actual -ne $expected) {{ throw "SHA-256 mismatch: $actual" }}
    }}
    Copy-Item -LiteralPath $source -Destination $staged -Force
    if (Test-Path -LiteralPath $target) {{
        Copy-Item -LiteralPath $target -Destination $backup -Force
    }}
    for ($i = 0; $i -lt 80; $i++) {{
        try {{
            Copy-Item -LiteralPath $staged -Destination $target -Force
            Remove-Item -LiteralPath $staged -Force
            break
        }} catch {{
            if ($i -eq 79) {{ throw }}
            Start-Sleep -Milliseconds 250
        }}
    }}
    Start-Process -FilePath $target -WorkingDirectory (Split-Path -Parent $target)
}} catch {{
    $log = Join-Path (Split-Path -Parent $target) 'genesis-update-error.log'
    $_ | Out-String | Set-Content -LiteralPath $log
    if (Test-Path -LiteralPath $staged) {{ Remove-Item -LiteralPath $staged -Force }}
    try {{ Start-Process -FilePath $target -WorkingDirectory (Split-Path -Parent $target) }} catch {{}}
}} finally {{
    if (Test-Path -LiteralPath {script_path}) {{ Remove-Item -LiteralPath {script_path} -Force }}
}}
"#,
            pid = std::process::id(),
            source = source,
            target = target,
            expected = expected,
            script_path = script_path,
        );
        std::fs::write(&script, body).map_err(|error| format!("无法写入更新脚本：{error}"))?;
        std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-WindowStyle",
                "Hidden",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(&script)
            .creation_flags(0x0800_0000)
            .spawn()
            .map_err(|error| format!("无法启动更新安装器：{error}"))?;
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = downloaded;
        let _ = expected_sha256;
        Err("自动更新目前只支持 Windows".to_owned())
    }
}

pub fn is_newer_version(current: &str, candidate: &str) -> bool {
    version_key(candidate) > version_key(current)
}

pub fn fetch_latest_release() -> Result<LauncherRelease, String> {
    #[cfg(target_os = "windows")]
    {
        let body = windows_latest_release_body()?;
        parse_release_body(&body)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let body = ureq_latest_release_body()?;
        parse_release_body(&body)
    }
}

#[cfg(not(target_os = "windows"))]
fn ureq_latest_release_body() -> Result<Vec<u8>, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(8))
        .timeout_read(Duration::from_secs(15))
        .build();
    let response = agent
        .get(LATEST_RELEASE_API)
        .set("User-Agent", brand::USER_AGENT)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|error| match error {
            ureq::Error::Status(404, _) => "GitHub 仓库暂未发布正式版本".to_owned(),
            _ => format!("无法连接 GitHub Releases：{error}"),
        })?;
    let mut body = Vec::new();
    response
        .into_reader()
        .take(2 * 1024 * 1024)
        .read_to_end(&mut body)
        .map_err(|error| format!("读取更新信息失败：{error}"))?;
    Ok(body)
}

fn parse_release_body(body: &[u8]) -> Result<LauncherRelease, String> {
    let value: Value =
        serde_json::from_slice(body).map_err(|error| format!("更新信息格式无效：{error}"))?;
    parse_release(&value)
}

#[cfg(target_os = "windows")]
fn windows_latest_release_body() -> Result<Vec<u8>, String> {
    use std::ffi::c_void;
    use windows_sys::Win32::Networking::WinHttp::{
        WINHTTP_FLAG_SECURE, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
        WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders,
        WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetTimeouts,
    };

    struct Handle(*mut c_void);
    impl Drop for Handle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    WinHttpCloseHandle(self.0);
                }
            }
        }
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }

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
    let path = wide("/repos/HerobrineInMinecraft/Genesis_Launcher/releases/latest");
    let request = Handle(unsafe {
        WinHttpOpenRequest(
            connection.0,
            method.as_ptr(),
            path.as_ptr(),
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

    let mut status = 0_u32;
    let mut status_size = std::mem::size_of::<u32>() as u32;
    let mut index = 0_u32;
    let queried = unsafe {
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            std::ptr::null(),
            (&mut status as *mut u32).cast(),
            &mut status_size,
            &mut index,
        )
    };
    if queried == 0 {
        return Err(format!(
            "读取 GitHub 响应状态失败：{}",
            std::io::Error::last_os_error()
        ));
    }
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

#[cfg(target_os = "windows")]
fn download_release_asset(
    asset: &ReleaseAsset,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<Vec<u8>, String> {
    use std::ffi::c_void;
    use windows_sys::Win32::Networking::WinHttp::{
        WINHTTP_FLAG_SECURE, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
        WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders,
        WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetTimeouts,
    };

    struct Handle(*mut c_void);
    impl Drop for Handle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    WinHttpCloseHandle(self.0);
                }
            }
        }
    }
    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }

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
    let path = wide(&format!("/{path}"));
    let request = Handle(unsafe {
        WinHttpOpenRequest(
            connection.0,
            method.as_ptr(),
            path.as_ptr(),
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
    let mut status = 0_u32;
    let mut status_size = std::mem::size_of::<u32>() as u32;
    let mut index = 0_u32;
    if unsafe {
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            std::ptr::null(),
            (&mut status as *mut u32).cast(),
            &mut status_size,
            &mut index,
        )
    } == 0
    {
        return Err(format!(
            "读取更新下载状态失败：{}",
            std::io::Error::last_os_error()
        ));
    }
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

#[cfg(not(target_os = "windows"))]
fn download_release_asset(
    asset: &ReleaseAsset,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<Vec<u8>, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(30))
        .build();
    let mut reader = agent
        .get(&asset.url)
        .set("User-Agent", brand::USER_AGENT)
        .call()
        .map_err(|error| format!("无法下载更新文件：{error}"))?
        .into_reader();
    let mut body = Vec::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("读取更新文件失败：{error}"))?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&buffer[..read]);
        on_progress(body.len() as u64, asset.size);
    }
    Ok(body)
}

fn parse_release(value: &Value) -> Result<LauncherRelease, String> {
    let version = value
        .get("tag_name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .ok_or_else(|| "GitHub Release 缺少版本标签".to_owned())?;
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(version);
    let page_url = value
        .get("html_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| url.starts_with(RELEASES_URL))
        .unwrap_or(RELEASES_URL);
    let asset = value
        .get("assets")
        .and_then(Value::as_array)
        .and_then(|assets| {
            assets
                .iter()
                .filter_map(parse_asset)
                .max_by_key(|asset| asset.size)
        });
    Ok(LauncherRelease {
        version: version.trim_start_matches(['v', 'V']).to_owned(),
        name: name.to_owned(),
        page_url: page_url.to_owned(),
        asset,
    })
}

fn parse_asset(value: &Value) -> Option<ReleaseAsset> {
    let name = value.get("name")?.as_str()?.trim();
    if !name.to_ascii_lowercase().ends_with(".exe") {
        return None;
    }
    let url = value.get("browser_download_url")?.as_str()?.trim();
    if !url
        .starts_with("https://github.com/HerobrineInMinecraft/Genesis_Launcher/releases/download/")
    {
        return None;
    }
    let sha256 = value
        .get("digest")
        .and_then(Value::as_str)
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .map(str::to_ascii_lowercase);
    Some(ReleaseAsset {
        name: name.to_owned(),
        url: url.to_owned(),
        size: value.get("size").and_then(Value::as_u64).unwrap_or(0),
        sha256,
    })
}

fn version_key(version: &str) -> (Vec<u64>, bool) {
    let version = version.trim().trim_start_matches(['v', 'V']);
    let (core, pre_release) = version
        .split_once('-')
        .map_or((version, None), |(core, suffix)| (core, Some(suffix)));
    let mut numbers = core
        .split('.')
        .map(|part| {
            part.chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse()
                .unwrap_or(0)
        })
        .collect::<Vec<_>>();
    numbers.resize(numbers.len().max(3), 0);
    (numbers, pre_release.is_none())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_release_versions() {
        assert!(is_newer_version("0.1.0", "v0.1.1"));
        assert!(is_newer_version("0.1.0", "v0.2.0"));
        assert!(is_newer_version("1.9.0", "v1.10.0"));
        assert!(!is_newer_version("0.2.0", "v0.2.0"));
        assert!(is_newer_version("0.2.0-beta.1", "v0.2.0"));
    }

    #[test]
    fn parses_github_latest_release() {
        let release = parse_release(&serde_json::json!({
            "tag_name": "v0.2.0",
            "name": "Genesis Launcher 0.2.0",
            "html_url": "https://github.com/HerobrineInMinecraft/Genesis_Launcher/releases/tag/v0.2.0",
            "assets": [{
                "name": "Genesis.Launcher.exe",
                "browser_download_url": "https://github.com/HerobrineInMinecraft/Genesis_Launcher/releases/download/v0.2.0/Genesis.Launcher.exe",
                "size": 1234,
                "digest": "sha256:ABCDEF"
            }]
        }))
        .unwrap();
        assert_eq!(release.version, "0.2.0");
        assert_eq!(release.name, "Genesis Launcher 0.2.0");
        assert!(release.page_url.contains("/releases/tag/v0.2.0"));
        let asset = release.asset.unwrap();
        assert_eq!(asset.name, "Genesis.Launcher.exe");
        assert_eq!(asset.size, 1234);
        assert_eq!(asset.sha256.as_deref(), Some("abcdef"));
    }
}
