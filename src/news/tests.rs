//! # `src/news/tests.rs`
//!
//! **功能域：** Minecraft 官方动态获取、解析、缓存和图片加载。
//!
//! 验证本模块的关键规则、错误路径和回归行为
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

use super::parse::parse_news_inner;
use super::security::{allowed_article_url, allowed_url};

#[test]
/// 实现 extracts新闻内容 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn extracts_news_content() {
    let body = r#"{
      "entries": [{
        "id": "test",
        "title": "Minecraft <b>update</b>",
        "category": "Java Edition",
        "date": "2026-10-31",
        "text": "A &amp; B",
        "newsPageImage": {"url": "https://launchercontent.mojang.com/image.png"},
        "readMoreLink": "https://www.minecraft.net/article"
      }]
    }"#;
    let news = parse_news_inner(body).expect("news");
    assert_eq!(news.len(), 1);
    assert_eq!(news[0].title, "Minecraft update");
    assert_eq!(news[0].summary, "A & B");
}

#[test]
/// 实现 rejectsnonmojang新闻sources 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn rejects_non_mojang_news_sources() {
    assert!(allowed_url(
        "https://launchercontent.mojang.com/images/a.png"
    ));
    assert!(!allowed_url("https://example.org/a.png"));
    assert!(allowed_article_url("https://www.minecraft.net/article"));
}
