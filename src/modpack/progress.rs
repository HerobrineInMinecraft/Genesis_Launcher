//! # `src/modpack/progress.rs`
//!
//! **功能域：** Modrinth 整合包读取、依赖补全、文件下载和覆盖层解压。
//!
//! 发送分阶段进度事件
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `send_stage`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 实现中包含线程、通道或共享状态；同步边界必须与现有轮询和生命周期保持一致。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::sync::mpsc;

use super::model::ModpackImportUpdate;

/// 实现 sendstage 相关的业务流程。
///
/// # 参数
/// - `stage`：由调用方传入的stage。
/// - `message`：由调用方传入的message。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn send_stage(sender: &mpsc::Sender<ModpackImportUpdate>, stage: &str, message: String) {
    let _ = sender.send(ModpackImportUpdate {
        stage: stage.to_owned(),
        message,
        finished: false,
        success: false,
        profile: None,
    });
}
