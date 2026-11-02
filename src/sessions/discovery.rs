//! # `src/sessions/discovery.rs`
//!
//! **功能域：** 世界、服务器与最近游玩记录的发现和 NBT 读取。
//!
//! 扫描本地目录并发现可用内容
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `discover_recent_sessions`
//! - `discover_worlds`
//! - `discover_servers`
//! - `server_log_recency`
//! - `server_addresses_match`
//! - `modified_seconds`
//! - `unix_now`
//! - `clean_name`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use super::model::{RecentSession, SessionKind};
use super::nbt::{Tag, read_nbt};
use super::*;

/// 实现 发现recentsessions 相关的业务流程。
///
/// # 参数
/// - `game_directory`：由调用方传入的游戏目录。
/// # 返回
/// 返回 `Vec<RecentSession>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn discover_recent_sessions(game_directory: &Path) -> Vec<RecentSession> {
    let mut sessions = discover_worlds(game_directory);
    sessions.extend(discover_servers(game_directory));
    sessions.sort_by(|left, right| {
        right
            .last_played
            .cmp(&left.last_played)
            .then_with(|| {
                let rank = |kind| match kind {
                    SessionKind::World => 0,
                    SessionKind::Server => 1,
                };
                rank(left.kind).cmp(&rank(right.kind))
            })
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    sessions
}

/// 实现 发现worlds 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Vec<RecentSession>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn discover_worlds(game_directory: &Path) -> Vec<RecentSession> {
    let Ok(entries) = fs::read_dir(game_directory.join("saves")) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_dir() {
                return None;
            }
            let id = path.file_name()?.to_string_lossy().to_string();
            let level_dat = path.join("level.dat");
            let nbt = read_nbt(&level_dat).ok();
            let data = nbt
                .as_ref()
                .and_then(Tag::compound)
                .and_then(|root| root.get("Data"))
                .and_then(Tag::compound);
            let name = data
                .and_then(|data| data.get("LevelName"))
                .and_then(Tag::string)
                .map(clean_name)
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| id.clone());
            let last_played = data
                .and_then(|data| data.get("LastPlayed"))
                .and_then(Tag::long)
                .filter(|value| *value > 0)
                .map(|milliseconds| (milliseconds as u64) / 1_000)
                .or_else(|| modified_seconds(&level_dat))
                .or_else(|| modified_seconds(&path));
            let version = data
                .and_then(|data| data.get("Version"))
                .and_then(Tag::compound)
                .and_then(|version| version.get("Name"))
                .and_then(Tag::string)
                .map(clean_name)
                .filter(|version| !version.is_empty());
            Some(RecentSession {
                kind: SessionKind::World,
                id,
                name,
                address: None,
                version,
                last_played,
            })
        })
        .collect()
}

/// 实现 发现servers 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Vec<RecentSession>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn discover_servers(game_directory: &Path) -> Vec<RecentSession> {
    let path = game_directory.join("servers.dat");
    let Ok(root) = read_nbt(&path) else {
        return Vec::new();
    };
    let Some(servers) = root
        .compound()
        .and_then(|root| root.get("servers"))
        .and_then(Tag::list)
    else {
        return Vec::new();
    };
    let recency = server_log_recency(game_directory);
    servers
        .iter()
        .filter_map(|server| {
            let server = server.compound()?;
            let address = server
                .get("ip")
                .and_then(Tag::string)
                .map(clean_name)
                .filter(|value| !value.is_empty())?;
            let name = server
                .get("name")
                .and_then(Tag::string)
                .map(clean_name)
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| address.clone());
            let played_at = recency
                .iter()
                .find(|(logged, _)| server_addresses_match(logged, &address))
                .map(|(_, played_at)| *played_at);
            Some(RecentSession {
                kind: SessionKind::Server,
                id: address.clone(),
                name,
                address: Some(address),
                version: None,
                last_played: played_at,
            })
        })
        .collect()
}

/// 实现 serverlogrecency 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `HashMap<String, u64>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn server_log_recency(game_directory: &Path) -> HashMap<String, u64> {
    let path = game_directory.join("logs").join("latest.log");
    let Ok(contents) = fs::read_to_string(&path) else {
        return HashMap::new();
    };
    let log_time = modified_seconds(&path).unwrap_or_else(unix_now);
    let mut recency = HashMap::new();
    for (offset, line) in contents.lines().rev().enumerate() {
        let Some(index) = line.find("Connecting to ") else {
            continue;
        };
        let address = line[index + "Connecting to ".len()..]
            .split([',', ' ', '\t'])
            .next()
            .unwrap_or("")
            .trim();
        if address.is_empty() {
            continue;
        }
        recency
            .entry(address.to_owned())
            .or_insert(log_time.saturating_sub(offset as u64));
    }
    recency
}

/// 实现 serveraddressesmatch 相关的业务流程。
///
/// # 参数
/// - `configured`：由调用方传入的configured。
/// # 返回
/// 返回 `bool`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn server_addresses_match(logged: &str, configured: &str) -> bool {
    logged == configured
        || logged
            .split_once(':')
            .map(|(host, _)| host == configured)
            .unwrap_or(false)
        || configured
            .split_once(':')
            .map(|(host, _)| host == logged)
            .unwrap_or(false)
}

/// 实现 modifiedseconds 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Option<u64>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn modified_seconds(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}

/// 实现 unixnow 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `u64`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// 实现 清理名称 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `String`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn clean_name(value: &str) -> String {
    let mut cleaned = String::new();
    let mut formatting = false;
    for character in value.chars() {
        if formatting {
            formatting = false;
            continue;
        }
        if character == '§' {
            formatting = true;
        } else if !character.is_control() {
            cleaned.push(character);
            if cleaned.chars().count() >= 80 {
                break;
            }
        }
    }
    cleaned.trim().to_owned()
}
