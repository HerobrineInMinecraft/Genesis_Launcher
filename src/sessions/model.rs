//! # `src/sessions/model.rs`
//!
//! **功能域：** 世界、服务器与最近游玩记录的发现和 NBT 读取。
//!
//! 定义跨文件共享的数据结构、状态和事件类型
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `SessionKind`
//! - `RecentSession`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// 定义 SessionKind 可选状态或事件集合。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub enum SessionKind {
    World,
    Server,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// 定义 RecentSession 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub struct RecentSession {
    pub kind: SessionKind,
    pub id: String,
    pub name: String,
    pub address: Option<String>,
    pub version: Option<String>,
    pub last_played: Option<u64>,
}
