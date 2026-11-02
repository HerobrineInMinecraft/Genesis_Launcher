//! # `src/download/model.rs`
//!
//! **功能域：** Minecraft 官方清单、客户端、资源、Java 运行时安装与完整性校验。
//!
//! 定义跨文件共享的数据结构、状态和事件类型
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `OfficialVersion`
//! - `InstallUpdate`
//! - `running`
//! - `DownloadTask`
//! - `Progress`
//! - `send`
//! - `set_stage`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 实现中包含线程、通道或共享状态；同步边界必须与现有轮询和生命周期保持一致。
//!
//! ## 安全与平台
//! - 本文件位于外部数据或文件写入边界，不得绕过 URL、路径、哈希和大小限制。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, mpsc};
use std::time::Instant;

#[derive(Clone, Debug)]
/// 定义 OfficialVersion 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub struct OfficialVersion {
    pub id: String,
    pub kind: String,
    pub url: String,
    pub sha1: String,
}

#[derive(Clone, Debug)]
/// 定义 InstallUpdate 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub struct InstallUpdate {
    pub stage: String,
    pub completed_bytes: u64,
    pub total_bytes: u64,
    pub files_done: usize,
    pub files_total: usize,
    pub bytes_per_second: u64,
    pub finished: bool,
    pub success: bool,
    pub message: String,
}

impl InstallUpdate {
    /// 实现 running 相关的业务流程。
    ///
    /// # 参数
    /// - `completed_bytes`：由调用方传入的completedbytes。
    /// - `total_bytes`：由调用方传入的totalbytes。
    /// - `files_done`：由调用方传入的文件done。
    /// - `files_total`：由调用方传入的文件total。
    /// # 返回
    /// 返回 `Self`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn running(
        stage: impl Into<String>,
        completed_bytes: u64,
        total_bytes: u64,
        files_done: usize,
        files_total: usize,
    ) -> Self {
        Self {
            stage: stage.into(),
            completed_bytes,
            total_bytes,
            files_done,
            files_total,
            bytes_per_second: 0,
            finished: false,
            success: false,
            message: String::new(),
        }
    }
}

#[derive(Clone)]
/// 定义 DownloadTask 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub(super) struct DownloadTask {
    pub(super) url: String,
    pub(super) path: PathBuf,
    pub(super) sha1: Option<String>,
    pub(super) size: u64,
}

/// 定义 进度 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub(super) struct Progress {
    pub(super) sender: mpsc::Sender<InstallUpdate>,
    pub(super) completed_bytes: AtomicU64,
    pub(super) total_bytes: u64,
    pub(super) files_done: AtomicUsize,
    pub(super) files_total: usize,
    pub(super) stage: Mutex<String>,
    pub(super) last_report: Mutex<u64>,
    pub(super) started_at: Instant,
}

impl Progress {
    /// 实现 send 相关的业务流程。
    ///
    /// # 参数
    /// - `force`：由调用方传入的force。
    /// - `message`：由调用方传入的message。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn send(&self, force: bool, message: &str) {
        let completed = self.completed_bytes.load(Ordering::Relaxed);
        let mut last = self.last_report.lock().unwrap();
        if !force && completed.saturating_sub(*last) < 512 * 1024 {
            return;
        }
        *last = completed;
        let stage = self.stage.lock().unwrap().clone();
        let mut update = InstallUpdate::running(
            stage,
            completed,
            self.total_bytes,
            self.files_done.load(Ordering::Relaxed),
            self.files_total,
        );
        let elapsed = self.started_at.elapsed().as_secs_f64();
        if elapsed > 0.2 {
            update.bytes_per_second = (completed as f64 / elapsed) as u64;
        }
        let _ = self.sender.send(update);
        let _ = message;
    }

    /// 实现 设置stage 相关的业务流程。
    ///
    /// # 参数
    /// - `stage`：由调用方传入的stage。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(super) fn set_stage(&self, stage: impl Into<String>) {
        *self.stage.lock().unwrap() = stage.into();
        self.send(true, "");
    }
}
