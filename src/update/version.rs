//! # `src/update/version.rs`
//!
//! **功能域：** Genesis Launcher GitHub Release 检查、下载和程序替换。
//!
//! 比较启动器发行版本号，并正确区分稳定发布与预发布版本。
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `is_newer_version`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件位于外部数据或文件写入边界，不得绕过 URL、路径、哈希和大小限制。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

/// 实现 newer版本 相关的业务流程。
///
/// # 参数
/// - `current`：由调用方传入的当前。
/// - `candidate`：由调用方传入的candidate。
/// # 返回
/// 返回 `bool`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn is_newer_version(current: &str, candidate: &str) -> bool {
    version_key(candidate) > version_key(current)
}

/// 实现 版本key 相关的业务流程。
///
/// # 参数
/// - `version`：由调用方传入的版本。
/// # 返回
/// 返回 `(Vec<u64>, bool)`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn version_key(version: &str) -> (Vec<u64>, bool) {
    let version = version.trim().trim_start_matches(['v', 'V']);
    let (core, pre_release) = version
        .split_once('-')
        .map_or((version, None), |(core, suffix)| (core, Some(suffix)));
    let mut numbers = core
        .split('.')
        .map(|part| {
            part.chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse()
                .unwrap_or(0)
        })
        .collect::<Vec<_>>();
    numbers.resize(numbers.len().max(3), 0);
    (numbers, pre_release.is_none())
}
