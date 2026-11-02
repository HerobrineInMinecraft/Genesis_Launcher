//! # `src/modpack/naming.rs`
//!
//! **功能域：** Modrinth 整合包读取、依赖补全、文件下载和覆盖层解压。
//!
//! 清理版本名称并处理重名冲突
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `sanitize_name`
//! - `unique_profile_name`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::path::Path;

/// 移除旧版导出器自动加到整合包名称前的启动器品牌。
///
/// 仅处理明确的“品牌 · ”前缀，不修改用户自行输入的普通名称。
pub(super) fn strip_launcher_export_prefix(value: &str) -> String {
    crate::brand::strip_generated_name_prefix(value)
}

/// 实现 sanitize名称 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `String`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn sanitize_name(value: &str) -> String {
    let mut name = value
        .chars()
        .map(|character| {
            if matches!(
                character,
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
            ) || character.is_control()
            {
                '_'
            } else {
                character
            }
        })
        .collect::<String>()
        .trim_matches([' ', '.'])
        .chars()
        .take(64)
        .collect::<String>();
    if name.is_empty() {
        name = "整合包".to_owned();
    }
    name
}

/// 实现 unique配置名称 相关的业务流程。
///
/// # 参数
/// - `base`：由调用方传入的base。
/// # 返回
/// 返回 `String`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn unique_profile_name(game_directory: &Path, base: &str) -> String {
    let versions = game_directory.join("versions");
    if !versions.join(base).exists() {
        return base.to_owned();
    }
    for index in 2..1000 {
        let candidate = format!("{base} ({index})");
        if !versions.join(&candidate).exists() {
            return candidate;
        }
    }
    format!("{base}-{}", std::process::id())
}
