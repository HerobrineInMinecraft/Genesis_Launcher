//! # `src/app/settings/model.rs`
//!
//! **功能域：** 启动器应用外壳、页面状态、交互流程和原生 egui 绘制。
//!
//! 定义跨文件共享的数据结构、状态和事件类型
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `SettingsSection`
//! - `ALL`
//! - `label`
//! - `description`
//! - `index`
//! - `AnimationLevel`
//! - `strength`
//! - `stored`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

#[derive(Clone, Copy, PartialEq, Eq)]
/// 定义 SettingsSection 可选状态或事件集合。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub(crate) enum SettingsSection {
    General,
    Game,
    Versions,
    Launch,
    Home,
    Appearance,
    About,
}

impl SettingsSection {
    /// 提供 全部 使用的只读常量。
    ///
    /// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
    pub(crate) const ALL: [Self; 7] = [
        Self::General,
        Self::Game,
        Self::Versions,
        Self::Launch,
        Self::Home,
        Self::Appearance,
        Self::About,
    ];

    /// 实现 标签 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `&'static str`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::General => "通用",
            Self::Game => "游戏",
            Self::Versions => "版本",
            Self::Launch => "启动",
            Self::Home => "主页",
            Self::Appearance => "外观",
            Self::About => "关于",
        }
    }

    /// 实现 description 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `&'static str`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(crate) fn description(self) -> &'static str {
        match self {
            Self::General => "启动器行为与启动提醒",
            Self::Game => "Java、内存和启动参数",
            Self::Versions => "Minecraft 文件夹与版本隔离",
            Self::Launch => "窗口尺寸、全屏与启动行为",
            Self::Home => "首页主视觉的文字、颜色与字号",
            Self::Appearance => "动画、提示和玻璃效果",
            Self::About => "版本信息与支持的功能",
        }
    }

    /// 实现 index 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `usize`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(crate) fn index(self) -> usize {
        match self {
            Self::General => 0,
            Self::Game => 1,
            Self::Versions => 2,
            Self::Launch => 3,
            Self::Home => 4,
            Self::Appearance => 5,
            Self::About => 6,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// 定义 AnimationLevel 可选状态或事件集合。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub(crate) enum AnimationLevel {
    Off,
    Weak,
    Medium,
    Strong,
}

impl AnimationLevel {
    /// 提供 全部 使用的只读常量。
    ///
    /// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
    pub(crate) const ALL: [Self; 4] = [Self::Off, Self::Weak, Self::Medium, Self::Strong];

    /// 实现 标签 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `&'static str`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Off => "关",
            Self::Weak => "弱",
            Self::Medium => "中",
            Self::Strong => "强",
        }
    }

    /// 实现 strength 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `f32`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(crate) fn strength(self) -> f32 {
        match self {
            Self::Off => 0.0,
            Self::Weak => 0.58,
            Self::Medium => 1.0,
            Self::Strong => 1.42,
        }
    }

    /// 实现 stored 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `&'static str`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub(crate) fn stored(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Weak => "weak",
            Self::Medium => "medium",
            Self::Strong => "strong",
        }
    }
}
