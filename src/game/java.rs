//! # `src/game/java.rs`
//!
//! **功能域：** Minecraft 版本发现、Java 选择、启动参数组装和进程生命周期。
//!
//! 检查 Java 主版本兼容性，并从环境变量、PATH 和 Minecraft 运行时目录发现可用 Java。
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `has_compatible_java`
//! - `java_for_loader_install`
//! - `version_java_requirement`
//! - `discover_java_runtimes`
//! - `find_compatible_java`
//! - `collect_common_java_candidates`
//! - `java_major_version`
//! - `collect_java_candidates`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 包含平台相关分支；跨平台修改必须同时保持各 ``cfg`` 分支可编译。

use super::version::load_merged_version;
use super::*;

/// 实现 hascompatibleJava 相关的业务流程。
///
/// # 参数
/// - `game_directory`：由调用方传入的游戏目录。
/// - `required_major`：由调用方传入的requiredmajor。
/// # 返回
/// 返回 `bool`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn has_compatible_java(game_directory: &Path, required_major: u32) -> bool {
    find_compatible_java(game_directory, Some(required_major)).is_some()
}

/// 实现 Javafor加载器安装 相关的业务流程。
///
/// # 参数
/// - `game_directory`：由调用方传入的游戏目录。
/// - `version`：由调用方传入的版本。
/// # 返回
/// 返回 `Option<PathBuf>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn java_for_loader_install(game_directory: &Path, version: &str) -> Option<PathBuf> {
    let required = load_merged_version(game_directory, version)
        .ok()
        .and_then(|merged| merged.java_major);
    find_compatible_java(game_directory, required)
}

/// 实现 版本Javarequirement 相关的业务流程。
///
/// # 参数
/// - `game_directory`：由调用方传入的游戏目录。
/// - `profile`：由调用方传入的配置。
/// # 返回
/// 返回 `Option<u32>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn version_java_requirement(game_directory: &Path, profile: &str) -> Option<u32> {
    load_merged_version(game_directory, profile)
        .ok()
        .and_then(|version| version.java_major)
}

/// 实现 发现Javaruntimes 相关的业务流程。
///
/// # 参数
/// - `game_directories`：由调用方传入的游戏directories。
/// # 返回
/// 返回 `Vec<JavaRuntime>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn discover_java_runtimes(game_directories: &[PathBuf]) -> Vec<JavaRuntime> {
    let executable = if cfg!(windows) { "java.exe" } else { "java" };
    let mut candidates = Vec::new();
    for game_directory in game_directories {
        let runtime = game_directory.join("runtime");
        if runtime.is_dir() {
            collect_java_candidates(&runtime, executable, 7, &mut candidates);
        }
    }
    collect_common_java_candidates(executable, &mut candidates);
    let mut seen = HashSet::new();
    let mut runtimes = candidates
        .into_iter()
        .filter(|path| path.is_file())
        .filter_map(|path| {
            let key = path.to_string_lossy().to_lowercase();
            if !seen.insert(key) {
                return None;
            }
            let major = java_major_version(&path)?;
            Some(JavaRuntime { path, major })
        })
        .collect::<Vec<_>>();
    runtimes.sort_by(|left, right| {
        right
            .major
            .cmp(&left.major)
            .then_with(|| left.path.cmp(&right.path))
    });
    runtimes
}

/// 实现 查找compatibleJava 相关的业务流程。
///
/// # 参数
/// - `required_major`：由调用方传入的requiredmajor。
/// # 返回
/// 返回 `Option<PathBuf>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn find_compatible_java(
    game_directory: &Path,
    required_major: Option<u32>,
) -> Option<PathBuf> {
    let executable = if cfg!(windows) { "java.exe" } else { "java" };
    let mut candidates = Vec::new();
    let runtime = game_directory.join("runtime");
    if runtime.is_dir() {
        collect_java_candidates(&runtime, executable, 7, &mut candidates);
    }
    collect_common_java_candidates(executable, &mut candidates);
    candidates.into_iter().find(|path| {
        path.is_file()
            && required_major.is_none_or(|required| {
                java_major_version(path).is_some_and(|actual| actual >= required)
            })
    })
}

/// 实现 收集commonJavacandidates 相关的业务流程。
///
/// # 参数
/// - `candidates`：由调用方传入的candidates。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn collect_common_java_candidates(executable: &str, candidates: &mut Vec<PathBuf>) {
    if let Some(java_home) = std::env::var_os("JAVA_HOME") {
        candidates.push(PathBuf::from(java_home).join("bin").join(executable));
    }
    for root in [
        r"C:\Program Files\Java",
        r"C:\Program Files\Eclipse Adoptium",
        r"C:\Program Files\Microsoft",
        r"C:\Program Files\Zulu",
        r"C:\Program Files\Amazon Corretto",
        r"C:\Program Files\BellSoft",
    ] {
        collect_java_candidates(Path::new(root), executable, 5, &mut *candidates);
    }
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            let candidate = directory.join(executable);
            if candidate.is_file() {
                candidates.push(candidate);
            }
        }
    }
}

/// 实现 Javamajor版本 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Option<u32>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn java_major_version(java: &Path) -> Option<u32> {
    let mut command = Command::new(java);
    command.arg("-version");
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let output = command.output().ok()?;
    let mut version_text = String::from_utf8_lossy(&output.stderr).to_string();
    version_text.push_str(&String::from_utf8_lossy(&output.stdout));
    let marker = version_text.find("version \"")? + "version \"".len();
    let version = version_text[marker..].split('"').next()?;
    let mut parts = version.split('.');
    let first = parts.next()?.parse::<u32>().ok()?;
    if first == 1 {
        parts.next()?.parse().ok()
    } else {
        Some(first)
    }
}

/// 实现 收集Javacandidates 相关的业务流程。
///
/// # 参数
/// - `executable`：由调用方传入的executable。
/// - `depth`：由调用方传入的depth。
/// - `output`：由调用方传入的output。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn collect_java_candidates(
    root: &Path,
    executable: &str,
    depth: usize,
    output: &mut Vec<PathBuf>,
) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_java_candidates(&path, executable, depth - 1, output);
        } else if path
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case(executable))
        {
            output.push(path);
        }
    }
}
