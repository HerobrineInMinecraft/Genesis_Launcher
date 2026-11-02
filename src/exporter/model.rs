//! # `src/exporter/model.rs`
//!
//! **功能域：** 版本和整合包导出、文件收集、哈希计算与压缩包生成。
//!
//! 定义跨文件共享的数据结构、状态和事件类型
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `VersionExportOptions`
//! - `ModpackExportFormat`
//! - `extension`
//! - `CollectedPackFile`
//! - `HostedPackFile`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

#[derive(Clone, Copy)]
/// 定义 VersionExportOptions 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub(crate) struct VersionExportOptions {
    pub(crate) mods: bool,
    pub(crate) config: bool,
    pub(crate) saves: bool,
    pub(crate) resourcepacks: bool,
    pub(crate) shaderpacks: bool,
    pub(crate) format: ModpackExportFormat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// 定义 ModpackExportFormat 可选状态或事件集合。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub(crate) enum ModpackExportFormat {
    Modrinth,
    Zip,
}

impl ModpackExportFormat {
    /// 实现 extension 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `&'static str`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(crate) fn extension(self) -> &'static str {
        match self {
            Self::Modrinth => "mrpack",
            Self::Zip => "zip",
        }
    }
}

/// 定义 CollectedPackFile 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub(crate) struct CollectedPackFile {
    pub(crate) source: std::path::PathBuf,
    pub(crate) path: String,
    pub(crate) sha1: Option<String>,
    pub(crate) sha512: Option<String>,
    pub(crate) queryable: bool,
}

#[derive(Clone)]
/// 定义 HostedPackFile 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub(crate) struct HostedPackFile {
    pub(crate) downloads: Vec<String>,
}
