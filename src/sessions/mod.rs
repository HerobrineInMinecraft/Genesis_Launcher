//! # `src/sessions/mod.rs`
//!
//! **功能域：** 世界、服务器与最近游玩记录的发现和 NBT 读取。
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

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use flate2::read::GzDecoder;

mod discovery;
mod model;
mod nbt;

pub(crate) use discovery::discover_recent_sessions;
pub(crate) use model::{RecentSession, SessionKind};

#[cfg(test)]
mod tests {
    use super::nbt::{Cursor, Tag};

    /// 实现 写入string 相关的业务流程。
    ///
    /// # 参数
    /// - `output`：由调用方传入的output。
    /// - `value`：由调用方传入的value。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn write_string(output: &mut Vec<u8>, value: &str) {
        output.extend_from_slice(&(value.len() as u16).to_be_bytes());
        output.extend_from_slice(value.as_bytes());
    }

    /// 实现 写入named 相关的业务流程。
    ///
    /// # 参数
    /// - `output`：由调用方传入的output。
    /// - `kind`：由调用方传入的kind。
    /// - `name`：由调用方传入的名称。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn write_named(output: &mut Vec<u8>, kind: u8, name: &str) {
        output.push(kind);
        write_string(output, name);
    }

    #[test]
    /// 实现 parses世界名称andtimestamp 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn parses_world_name_and_timestamp() {
        let mut nbt = vec![10, 0, 0, 10];
        write_string(&mut nbt, "Data");
        write_named(&mut nbt, 8, "LevelName");
        write_string(&mut nbt, "暮色生存");
        write_named(&mut nbt, 4, "LastPlayed");
        nbt.extend_from_slice(&1_777_777_777_000_i64.to_be_bytes());
        nbt.push(0);
        nbt.push(0);
        let mut cursor = Cursor::new(&nbt);
        cursor.byte().expect("root kind");
        cursor.string().expect("root name");
        let root = cursor.payload(10, 0).expect("root");
        let data = root
            .compound()
            .and_then(|root| root.get("Data"))
            .and_then(Tag::compound)
            .expect("data");
        assert_eq!(
            data.get("LevelName").and_then(Tag::string),
            Some("暮色生存")
        );
        assert_eq!(
            data.get("LastPlayed").and_then(Tag::long),
            Some(1_777_777_777_000)
        );
    }

    #[test]
    /// 实现 parsesserverlist 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn parses_server_list() {
        let mut nbt = vec![10, 0, 0, 9];
        write_string(&mut nbt, "servers");
        nbt.push(10);
        nbt.extend_from_slice(&1_i32.to_be_bytes());
        write_named(&mut nbt, 8, "name");
        write_string(&mut nbt, "NOVA SMP");
        write_named(&mut nbt, 8, "ip");
        write_string(&mut nbt, "play.example.org");
        nbt.push(0);
        nbt.push(0);
        let mut cursor = Cursor::new(&nbt);
        cursor.byte().expect("root kind");
        cursor.string().expect("root name");
        let root = cursor.payload(10, 0).expect("root");
        let server = root
            .compound()
            .and_then(|root| root.get("servers"))
            .and_then(Tag::list)
            .and_then(|servers| servers.first())
            .and_then(Tag::compound)
            .expect("server");
        assert_eq!(server.get("name").and_then(Tag::string), Some("NOVA SMP"));
        assert_eq!(
            server.get("ip").and_then(Tag::string),
            Some("play.example.org")
        );
    }
}
