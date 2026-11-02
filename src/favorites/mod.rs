//! # `src/favorites/mod.rs`
//!
//! **功能域：** 收藏站点模型、地址规范化、图标抓取和外部链接打开。
//!
//! 组织本模块声明并导出稳定公共接口
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `BUILTIN_SITES`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

/// 提供 BUILTINSITES 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub const BUILTIN_SITES: &[(&str, &str)] = &[
    ("Minecraft 官方网站", "https://www.minecraft.net/"),
    ("Minecraft Wiki", "https://zh.minecraft.wiki/"),
    ("Modrinth", "https://modrinth.com/"),
    (
        "CurseForge Minecraft",
        "https://www.curseforge.com/minecraft",
    ),
    ("MCMod", "https://www.mcmod.cn/"),
    ("Planet Minecraft", "https://www.planetminecraft.com/"),
];

mod icons;
mod model;
mod urls;

pub use icons::{open_site, start_fetch_icons};
pub use model::FavoriteIconRequest;
pub use urls::{normalize_icon_source, normalize_site_url, site_domain};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// 实现 normalizesuserwebsiteurls 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn normalizes_user_website_urls() {
        assert_eq!(
            normalize_site_url("example.com").unwrap(),
            "https://example.com"
        );
        assert!(normalize_site_url("javascript:alert(1)").is_err());
        assert!(normalize_site_url("file:///C:/test").is_err());
    }

    #[test]
    /// 实现 extractsdomains 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn extracts_domains() {
        assert_eq!(
            site_domain("https://modrinth.com/mod/example"),
            "modrinth.com"
        );
    }

    #[test]
    /// 实现 normalizescustom图标sources 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn normalizes_custom_icon_sources() {
        assert_eq!(normalize_icon_source("").unwrap(), "");
        assert_eq!(
            normalize_icon_source("example.com/icon.png").unwrap(),
            "https://example.com/icon.png"
        );
        assert!(normalize_icon_source("not-a-real-icon").is_err());
    }
}
