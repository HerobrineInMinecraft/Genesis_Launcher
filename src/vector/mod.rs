//! # `src/vector/mod.rs`
//!
//! **功能域：** 固定设计 DPI 和矢量画布尺寸常量。
//!
//! 组织本模块声明并导出稳定公共接口
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `DESIGN_DPI`
//! - `ARTBOARD_SIZE`
//! - `DESIGN_SURFACE_SIZE`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use eframe::egui::Vec2;

/// Fixed vector-design resolution used by the launcher artboard.
pub const DESIGN_DPI: f32 = 648.0;

/// Canonical artboard size in logical vector units.
pub const ARTBOARD_SIZE: Vec2 = Vec2::new(1440.0, 900.0);

/// The on-screen window keeps the original logical artboard size.
pub const DESIGN_SURFACE_SIZE: Vec2 = ARTBOARD_SIZE;
