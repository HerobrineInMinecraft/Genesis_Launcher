//! # `src/modpack/minecraft.rs`
//!
//! **功能域：** Modrinth 整合包读取、依赖补全、文件下载和覆盖层解压。
//!
//! 检测并补全整合包依赖的 Minecraft 原版文件
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `ensure_minecraft`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 实现中包含线程、通道或共享状态；同步边界必须与现有轮询和生命周期保持一致。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::path::Path;
use std::sync::mpsc;

use super::model::ModpackImportUpdate;
use super::progress::send_stage;

/// 实现 确保Minecraft 相关的业务流程。
///
/// # 参数
/// - `minecraft`：由调用方传入的Minecraft。
/// - `sender`：由调用方传入的sender。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn ensure_minecraft(
    game_directory: &Path,
    minecraft: &str,
    sender: &mpsc::Sender<ModpackImportUpdate>,
) -> Result<String, String> {
    // 旧版本导出的整合包可能把本地 profile 名（例如“愚者”）写进
    // dependencies.minecraft。先沿本地版本配置的 inheritsFrom 链解析基础版本，
    // 避免把中文实例名当成 Mojang 官方版本请求下载。
    let declared = minecraft;
    let minecraft = crate::game::resolve_minecraft_dependency(game_directory, declared);
    if minecraft != declared {
        send_stage(
            sender,
            "解析基础版本",
            format!("{declared} → Minecraft {minecraft}"),
        );
    }
    if !crate::game::looks_like_official_version(&minecraft) {
        return Err(format!(
            "整合包声明的 Minecraft 版本“{minecraft}”不是官方版本号，且无法从本地版本配置推断基础版本"
        ));
    }
    if crate::game::installation_complete(game_directory, &minecraft, "原版") {
        return Ok(minecraft.to_owned());
    }
    let receiver =
        crate::download::start_install_by_id(game_directory.to_owned(), minecraft.to_owned());
    let mut final_message = None;
    while let Ok(update) = receiver.recv() {
        send_stage(sender, "补全 Minecraft", update.stage.clone());
        if update.finished {
            final_message = Some((update.success, update.message));
            break;
        }
    }
    match final_message {
        Some((true, _)) => Ok(minecraft),
        Some((false, message)) => Err(format!("Minecraft 补全失败：{message}")),
        None => Err("Minecraft 补全任务中断".to_owned()),
    }
}
