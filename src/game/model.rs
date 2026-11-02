//! # `src/game/model.rs`
//!
//! **功能域：** Minecraft 版本发现、Java 选择、启动参数组装和进程生命周期。
//!
//! 定义跨文件共享的数据结构、状态和事件类型
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `LaunchDetails`
//! - `LaunchOptions`
//! - `QuickPlayTarget`
//! - `JavaRuntime`
//! - `MergedVersion`
//! - `PreparedLaunch`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use super::*;

/// 定义 LaunchDetails 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub struct LaunchDetails {
    pub version: String,
    pub java_name: String,
    pub process_id: u32,
}

#[derive(Clone)]
/// 定义 LaunchOptions 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub struct LaunchOptions {
    pub max_memory_mb: u16,
    pub window_width: u32,
    pub window_height: u32,
    pub fullscreen: bool,
    pub extra_jvm_args: String,
    pub quick_play: Option<QuickPlayTarget>,
    pub data_directory: Option<PathBuf>,
    pub java_path: Option<PathBuf>,
    pub player_name: String,
    pub skin_slim: bool,
}

#[derive(Clone, Debug)]
/// 定义 QuickPlayTarget 可选状态或事件集合。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub enum QuickPlayTarget {
    World(String),
    Server(String),
}

impl Default for LaunchOptions {
    /// 实现 默认 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `Self`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn default() -> Self {
        Self {
            max_memory_mb: 6144,
            window_width: 1280,
            window_height: 720,
            fullscreen: false,
            extra_jvm_args: String::new(),
            quick_play: None,
            data_directory: None,
            java_path: None,
            player_name: OFFLINE_NAME.to_owned(),
            skin_slim: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// 定义 JavaRuntime 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub struct JavaRuntime {
    pub path: PathBuf,
    pub major: u32,
}

#[derive(Clone)]
/// 定义 MergedVersion 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub(crate) struct MergedVersion {
    pub(crate) main_class: String,
    pub(crate) assets: String,
    pub(crate) client_jar_version: String,
    pub(crate) java_major: Option<u32>,
    pub(crate) libraries: Vec<Value>,
    pub(crate) game_args: Vec<Value>,
    pub(crate) jvm_args: Vec<Value>,
    pub(crate) legacy_args: Option<String>,
}

/// 定义 PreparedLaunch 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub(crate) struct PreparedLaunch {
    pub(crate) version: String,
    pub(crate) java: PathBuf,
    pub(crate) args: Vec<String>,
}
