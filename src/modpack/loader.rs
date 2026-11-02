//! # `src/modpack/loader.rs`
//!
//! **功能域：** Modrinth 整合包读取、依赖补全、文件下载和覆盖层解压。
//!
//! 转发整合包加载器安装请求，并把最终版本配置返回给导入流程。
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `install_loader`
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

use crate::loader::LoaderKind;

use super::model::ModpackImportUpdate;
use super::progress::send_stage;

/// 实现 安装加载器 相关的业务流程。
///
/// # 参数
/// - `minecraft`：由调用方传入的Minecraft。
/// - `loader`：由调用方传入的加载器。
/// - `loader_version`：由调用方传入的加载器版本。
/// - `sender`：由调用方传入的sender。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn install_loader(
    game_directory: &Path,
    minecraft: &str,
    loader: LoaderKind,
    loader_version: &str,
    sender: &mpsc::Sender<ModpackImportUpdate>,
) -> Result<String, String> {
    let receiver = crate::loader::start_install(
        game_directory.to_owned(),
        minecraft.to_owned(),
        loader,
        loader_version.to_owned(),
    );
    let mut final_update = None;
    while let Ok(update) = receiver.recv() {
        send_stage(sender, "安装加载器", update.stage.clone());
        if update.finished {
            final_update = Some(update);
            break;
        }
    }
    let update = final_update.ok_or_else(|| "加载器安装任务中断".to_owned())?;
    if !update.success {
        return Err(update.message);
    }
    update
        .profile_hint
        .ok_or_else(|| "加载器安装未返回版本配置".to_owned())
}
