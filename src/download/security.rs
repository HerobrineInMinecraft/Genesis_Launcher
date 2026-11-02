//! # `src/download/security.rs`
//!
//! **功能域：** Minecraft 官方清单、客户端、资源、Java 运行时安装与完整性校验。
//!
//! 执行域名、哈希和路径边界校验
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `safe_join`
//! - `validate_path_component`
//! - `is_sha1`
//! - `ensure_allowed_url`
//! - `rules_allow`
//! - `sha1_file`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件位于外部数据或文件写入边界，不得绕过 URL、路径、哈希和大小限制。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;
use sha1_smol::Sha1;

/// 实现 safejoin 相关的业务流程。
///
/// # 参数
/// - `relative`：由调用方传入的relative。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn safe_join(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(format!("拒绝不安全的下载路径：{}", relative.display()));
    }
    Ok(root.join(relative))
}

/// 实现 校验路径component 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn validate_path_component(value: &str) -> Result<(), String> {
    let path = Path::new(value);
    let mut components = path.components();
    let valid = matches!(components.next(), Some(Component::Normal(_)))
        && components.next().is_none()
        && !value.trim().is_empty()
        && value != "."
        && value != ".."
        && !value.contains(':');
    if valid {
        Ok(())
    } else {
        Err(format!("拒绝不安全的版本或资源标识：{value}"))
    }
}

/// 实现 sha1 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `bool`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn is_sha1(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// 实现 确保allowed地址 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn ensure_allowed_url(url: &str) -> Result<(), String> {
    // Piston Data is the object CDN linked by Mojang's Java Runtime Manifest.
    /// 提供 HOSTS 使用的只读常量。
    ///
    /// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
    const HOSTS: [&str; 5] = [
        "https://piston-meta.mojang.com/",
        "https://piston-data.mojang.com/",
        "https://launchermeta.mojang.com/",
        "https://libraries.minecraft.net/",
        "https://resources.download.minecraft.net/",
    ];
    if HOSTS.iter().any(|host| url.starts_with(host)) {
        Ok(())
    } else {
        Err(format!("拒绝非 Mojang 官方下载地址：{url}"))
    }
}

/// 实现 rulesallow 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `bool`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn rules_allow(value: &Value) -> bool {
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
/// # 参数
/// - `rule`：由调用方传入的rule。
/// # 返回
/// 返回 `bool`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
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
    true
}

/// 实现 sha1文件 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn sha1_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| format!("读取校验文件失败：{error}"))?;
    let mut sha1 = Sha1::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("读取校验文件失败：{error}"))?;
        if read == 0 {
            break;
        }
        sha1.update(&buffer[..read]);
    }
    Ok(sha1.digest().to_string())
}
