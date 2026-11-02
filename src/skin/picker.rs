//! # `src/skin/picker.rs`
//!
//! **功能域：** Minecraft 皮肤解码、编辑、保存和资源包安装。
//!
//! 调用 Windows 原生文件或目录选择器
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `pick_image_file`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 包含平台相关分支；跨平台修改必须同时保持各 ``cfg`` 分支可编译。

use std::path::PathBuf;

/// 实现 选择图片文件 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Option<PathBuf>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn pick_image_file(save: bool) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let dialog = if save {
            "$d=New-Object System.Windows.Forms.SaveFileDialog; $d.Filter='PNG 图片 (*.png)|*.png'; $d.FileName='skin.png'"
        } else {
            "$d=New-Object System.Windows.Forms.OpenFileDialog; $d.Filter='PNG 图片 (*.png)|*.png|所有图片 (*.*)|*.*'; $d.Multiselect=$false"
        };
        let script = format!(
            "[Console]::OutputEncoding=[Text.Encoding]::UTF8; Add-Type -AssemblyName System.Windows.Forms; {dialog}; if($d.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK){{[Console]::Out.Write($d.FileName)}}"
        );
        let mut command = std::process::Command::new("powershell.exe");
        command
            .args(["-NoProfile", "-STA", "-Command", &script])
            .creation_flags(0x0800_0000);
        let output = command.output().ok()?;
        let selected = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        (!selected.is_empty()).then(|| PathBuf::from(selected))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = save;
        None
    }
}
