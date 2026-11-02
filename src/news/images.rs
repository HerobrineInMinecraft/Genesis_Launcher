//! # `src/news/images.rs`
//!
//! **功能域：** Minecraft 官方动态获取、解析、缓存和图片加载。
//!
//! 并行加载并附加新闻图片
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `attach_images`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 实现中包含线程、通道或共享状态；同步边界必须与现有轮询和生命周期保持一致。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::path::Path;
use std::thread;

use super::image_loader::load_image;
use super::model::MinecraftNews;

/// 实现 attachimages 相关的业务流程。
///
/// # 参数
/// - `cache`：由调用方传入的缓存。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn attach_images(news: &mut [MinecraftNews], cache: &Path) {
    let requests: Vec<Option<String>> = news
        .iter()
        .take(3)
        .map(|entry| entry.image_url.clone())
        .collect();
    let downloaded = thread::scope(|scope| {
        let handlers = requests
            .into_iter()
            .map(|url| {
                let cache = cache.to_path_buf();
                scope.spawn(move || url.as_deref().and_then(|url| load_image(url, &cache).ok()))
            })
            .collect::<Vec<_>>();
        handlers
            .into_iter()
            .map(|handler| handler.join().unwrap_or(None))
            .collect::<Vec<_>>()
    });
    for (entry, image_bytes) in news.iter_mut().take(3).zip(downloaded) {
        entry.image_bytes = image_bytes;
    }
}
