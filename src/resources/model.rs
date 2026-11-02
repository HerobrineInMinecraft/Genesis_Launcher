//! # `src/resources/model.rs`
//!
//! **功能域：** Modrinth 资源搜索、详情解析、镜像测速和内容下载。
//!
//! 定义跨文件共享的数据结构、状态和事件类型
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `ResourceCategory`
//! - `ALL`
//! - `project_type`
//! - `label`
//! - `target_folder`
//! - `ResourceProject`
//! - `ResourceDownloadFile`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
/// 定义 ResourceCategory 可选状态或事件集合。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub enum ResourceCategory {
    Mod,
    Modpack,
    Shader,
    ResourcePack,
    Datapack,
}

impl ResourceCategory {
    /// 提供 全部 使用的只读常量。
    ///
    /// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
    pub const ALL: [Self; 5] = [
        Self::Mod,
        Self::Modpack,
        Self::Shader,
        Self::ResourcePack,
        Self::Datapack,
    ];

    /// 实现 项目type 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `&'static str`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub fn project_type(self) -> &'static str {
        match self {
            Self::Mod => "mod",
            Self::Modpack => "modpack",
            Self::Shader => "shader",
            Self::ResourcePack => "resourcepack",
            Self::Datapack => "datapack",
        }
    }

    /// 实现 标签 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `&'static str`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub fn label(self) -> &'static str {
        match self {
            Self::Mod => "模组",
            Self::Modpack => "整合包",
            Self::Shader => "光影",
            Self::ResourcePack => "资源包",
            Self::Datapack => "数据包",
        }
    }

    /// 实现 targetfolder 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `&'static str`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub fn target_folder(self) -> &'static str {
        match self {
            Self::Mod => "mods",
            Self::Modpack => "downloads/modpacks",
            Self::Shader => "shaderpacks",
            Self::ResourcePack => "resourcepacks",
            Self::Datapack => "datapacks",
        }
    }
}

#[derive(Clone, Debug)]
/// 定义 ResourceProject 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub struct ResourceProject {
    pub id: String,
    pub title: String,
    pub description: String,
    pub author: String,
    pub downloads: u64,
    pub categories: Vec<String>,
    pub game_versions: Vec<String>,
}

/// 定义 ResourceDownloadFile 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub(super) struct ResourceDownloadFile {
    pub(super) url: String,
    pub(super) sha1: String,
}
