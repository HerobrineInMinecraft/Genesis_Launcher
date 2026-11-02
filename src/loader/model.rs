//! # `src/loader/model.rs`
//!
//! **功能域：** Fabric、Forge 和 NeoForge 的版本查询、下载与安装。
//!
//! 定义跨文件共享的数据结构、状态和事件类型
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `LoaderKind`
//! - `ALL`
//! - `label`
//! - `display_version`
//! - `LoaderInstallUpdate`
//! - `running`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// 定义 LoaderKind 可选状态或事件集合。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub enum LoaderKind {
    Fabric,
    Forge,
    NeoForge,
}

impl LoaderKind {
    /// 提供 全部 使用的只读常量。
    ///
    /// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
    pub const ALL: [Self; 3] = [Self::Fabric, Self::Forge, Self::NeoForge];

    /// 实现 标签 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `&'static str`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub fn label(self) -> &'static str {
        match self {
            Self::Fabric => "Fabric",
            Self::Forge => "Forge",
            Self::NeoForge => "NeoForge",
        }
    }
}

/// 实现 display版本 相关的业务流程。
///
/// # 参数
/// - `game_version`：由调用方传入的游戏版本。
/// - `version`：由调用方传入的版本。
/// # 返回
/// 返回 `String`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn display_version(game_version: &str, version: &str) -> String {
    let prefix = format!("{game_version}-");
    version.strip_prefix(&prefix).unwrap_or(version).to_owned()
}

#[derive(Clone, Debug)]
/// 定义 LoaderInstallUpdate 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub struct LoaderInstallUpdate {
    pub stage: String,
    pub finished: bool,
    pub success: bool,
    pub profile_hint: Option<String>,
    pub message: String,
}

impl LoaderInstallUpdate {
    /// 实现 running 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `Self`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(crate) fn running(stage: impl Into<String>) -> Self {
        Self {
            stage: stage.into(),
            finished: false,
            success: false,
            profile_hint: None,
            message: String::new(),
        }
    }
}
