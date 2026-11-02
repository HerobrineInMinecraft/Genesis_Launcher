//! # `src/update/installer.rs`
//!
//! **功能域：** Genesis Launcher GitHub Release 检查、下载和程序替换。
//!
//! 自更新安装器。生成延时 PowerShell 脚本，在当前进程退出后校验、备份并替换启动器。
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `launch_update_installer`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件位于外部数据或文件写入边界，不得绕过 URL、路径、哈希和大小限制。
//! - 包含平台相关分支；跨平台修改必须同时保持各 ``cfg`` 分支可编译。

use std::path::Path;

use crate::brand;

/// 实现 启动更新installer 相关的业务流程。
///
/// # 参数
/// - `downloaded`：由调用方传入的downloaded。
/// - `expected_sha256`：由调用方传入的expectedsha256。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn launch_update_installer(
    downloaded: &Path,
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
