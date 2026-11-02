//! # `src/brand/mod.rs`
//!
//! **功能域：** 产品名称、数据目录和网络 User-Agent 等品牌常量。
//!
//! 组织本模块声明并导出稳定公共接口
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `NAME`
//! - `COMPACT_NAME`
//! - `DATA_DIRECTORY`
//! - `LEGACY_DATA_DIRECTORY`
//! - `USER_AGENT`
//! - `data_directory`
//! - `legacy_data_directory`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::path::PathBuf;

/// 提供 名称 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub const NAME: &str = "Genesis Launcher";
/// 提供 COMPACT名称 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub const COMPACT_NAME: &str = "GenesisLauncher";
/// 提供 数据目录 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub const DATA_DIRECTORY: &str = "GenesisLauncher";
/// 提供 LEGACY数据目录 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub const LEGACY_DATA_DIRECTORY: &str = "BlockLauncher";
/// 提供 USERAGENT 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub const USER_AGENT: &str = concat!(
    "GenesisLauncher/",
    env!("CARGO_PKG_VERSION"),
    " (+Rust native launcher)"
);

/// 移除旧版整合包和版本名前误加的启动器品牌前缀。
///
/// 只匹配“品牌 + 中间点 + 空格”的完整前缀，普通名称不受影响。
pub(crate) fn strip_generated_name_prefix(value: &str) -> String {
    let value = value.trim();
    for prefix in [
        format!("{NAME} · "),
        format!("{COMPACT_NAME} · "),
        "方块启动器 · ".to_owned(),
    ] {
        if let Some(name) = value.strip_prefix(&prefix)
            && !name.trim().is_empty()
        {
            return name.trim().to_owned();
        }
    }
    value.to_owned()
}

/// 实现 数据目录 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `PathBuf`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn data_directory() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(DATA_DIRECTORY)
}

/// 实现 legacy数据目录 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `PathBuf`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn legacy_data_directory() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(LEGACY_DATA_DIRECTORY)
}
