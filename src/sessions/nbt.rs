//! # `src/sessions/nbt.rs`
//!
//! **功能域：** 世界、服务器与最近游玩记录的发现和 NBT 读取。
//!
//! 以受限深度读取 Minecraft NBT 数据
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `read_nbt`
//! - `Tag`
//! - `compound`
//! - `list`
//! - `string`
//! - `long`
//! - `Cursor`
//! - `new`
//! - `take`
//! - `byte`
//! - `short`
//! - `int`
//! - `payload`
//! - `length`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use super::*;

/// 实现 读取nbt 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn read_nbt(path: &Path) -> Result<Tag, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let decoded;
    let bytes = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut decoder = GzDecoder::new(bytes.as_slice());
        let mut output = Vec::new();
        decoder
            .read_to_end(&mut output)
            .map_err(|error| format!("gzip: {error}"))?;
        decoded = output;
        decoded.as_slice()
    } else {
        bytes.as_slice()
    };
    let mut cursor = Cursor::new(bytes);
    if cursor.byte()? != 10 {
        return Err("root tag is not a compound".to_owned());
    }
    let _root_name = cursor.string()?;
    cursor.payload(10, 0)
}

#[derive(Debug)]
/// 定义 Tag 可选状态或事件集合。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub(crate) enum Tag {
    Long(i64),
    String(String),
    List(Vec<Tag>),
    Compound(HashMap<String, Tag>),
    Other,
}

impl Tag {
    /// 实现 compound 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `Option<&HashMap<String, Tag>>`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(crate) fn compound(&self) -> Option<&HashMap<String, Tag>> {
        match self {
            Self::Compound(value) => Some(value),
            _ => None,
        }
    }

    /// 实现 list 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `Option<&[Tag]>`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(crate) fn list(&self) -> Option<&[Tag]> {
        match self {
            Self::List(value) => Some(value),
            _ => None,
        }
    }

    /// 实现 string 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `Option<&str>`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(crate) fn string(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    /// 实现 long 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `Option<i64>`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(crate) fn long(&self) -> Option<i64> {
        match self {
            Self::Long(value) => Some(*value),
            _ => None,
        }
    }
}

/// 定义 Cursor 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub(crate) struct Cursor<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Cursor<'a> {
    /// 实现 new 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `Self`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    /// 实现 take 相关的业务流程。
    ///
    /// # 参数
    /// - `length`：由调用方传入的length。
    /// # 返回
    /// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(crate) fn take(&mut self, length: usize) -> Result<&'a [u8], String> {
        let end = self
            .position
            .checked_add(length)
            .ok_or_else(|| "NBT length overflow".to_owned())?;
        let value = self
            .bytes
            .get(self.position..end)
            .ok_or_else(|| "unexpected end of NBT data".to_owned())?;
        self.position = end;
        Ok(value)
    }

    /// 实现 byte 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(crate) fn byte(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    /// 实现 short 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(crate) fn short(&mut self) -> Result<u16, String> {
        Ok(u16::from_be_bytes(
            self.take(2)?.try_into().expect("two bytes"),
        ))
    }

    /// 实现 int 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(crate) fn int(&mut self) -> Result<i32, String> {
        Ok(i32::from_be_bytes(
            self.take(4)?.try_into().expect("four bytes"),
        ))
    }

    /// 实现 long 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(crate) fn long(&mut self) -> Result<i64, String> {
        Ok(i64::from_be_bytes(
            self.take(8)?.try_into().expect("eight bytes"),
        ))
    }

    /// 实现 string 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(crate) fn string(&mut self) -> Result<String, String> {
        let length = self.short()? as usize;
        String::from_utf8(self.take(length)?.to_vec())
            .map_err(|_| "NBT string is not UTF-8".to_owned())
    }

    /// 实现 payload 相关的业务流程。
    ///
    /// # 参数
    /// - `kind`：由调用方传入的kind。
    /// - `depth`：由调用方传入的depth。
    /// # 返回
    /// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(crate) fn payload(&mut self, kind: u8, depth: usize) -> Result<Tag, String> {
        if depth > 64 {
            return Err("NBT nesting is too deep".to_owned());
        }
        match kind {
            1 => {
                self.byte()?;
                Ok(Tag::Other)
            }
            2 => {
                self.short()?;
                Ok(Tag::Other)
            }
            3 => {
                self.int()?;
                Ok(Tag::Other)
            }
            4 => Ok(Tag::Long(self.long()?)),
            5 => {
                self.take(4)?;
                Ok(Tag::Other)
            }
            6 => {
                self.take(8)?;
                Ok(Tag::Other)
            }
            7 => {
                let length = self.length()?;
                self.take(length)?;
                Ok(Tag::Other)
            }
            8 => Ok(Tag::String(self.string()?)),
            9 => {
                let element_kind = self.byte()?;
                let length = self.length()?;
                let mut values = Vec::with_capacity(length.min(4096));
                for _ in 0..length {
                    values.push(self.payload(element_kind, depth + 1)?);
                }
                Ok(Tag::List(values))
            }
            10 => {
                let mut values = HashMap::new();
                loop {
                    let child_kind = self.byte()?;
                    if child_kind == 0 {
                        break;
                    }
                    let name = self.string()?;
                    let value = self.payload(child_kind, depth + 1)?;
                    values.insert(name, value);
                }
                Ok(Tag::Compound(values))
            }
            11 => {
                let length = self.length()?;
                self.take(
                    length
                        .checked_mul(4)
                        .ok_or_else(|| "NBT integer array overflow".to_owned())?,
                )?;
                Ok(Tag::Other)
            }
            12 => {
                let length = self.length()?;
                self.take(
                    length
                        .checked_mul(8)
                        .ok_or_else(|| "NBT long array overflow".to_owned())?,
                )?;
                Ok(Tag::Other)
            }
            other => Err(format!("unsupported NBT tag type {other}")),
        }
    }

    /// 实现 length 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(crate) fn length(&mut self) -> Result<usize, String> {
        let length = self.int()?;
        if length < 0 || length as usize > self.bytes.len() {
            return Err("invalid NBT collection length".to_owned());
        }
        Ok(length as usize)
    }
}
