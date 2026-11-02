//! # `src/skin/mod.rs`
//!
//! **功能域：** Minecraft 皮肤解码、编辑、保存和资源包安装。
//!
//! 组织本模块声明并导出稳定公共接口
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - 本文件主要为同模块内部实现，不导出额外公共符号。
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

mod default_pixels;
mod image;
mod pack;
mod picker;

pub(crate) use default_pixels::default_skin_pixels;
pub(crate) use image::{detect_slim_skin, load_player_skin, save_player_skin};
pub(crate) use pack::install_local_skin_resource_pack;
pub(crate) use picker::pick_image_file;

#[cfg(test)]
pub(crate) use image::decode_skin_image;
#[cfg(test)]
pub(crate) use pack::{DEFAULT_PLAYER_SKIN_NAMES, LOCAL_SKIN_PACK_NAME};
