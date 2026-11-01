use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use serde_json::Value;
use sha1_smol::Sha1;

const NEWS_URL: &str = "https://launchercontent.mojang.com/v2/news.json";
const USER_AGENT: &str = "BlockLauncher/0.1.0 (+Rust native launcher)";
const MAX_IMAGE_BYTES: usize = 12 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct MinecraftNews {
    pub id: String,
    pub title: String,
    pub category: String,
    pub date: String,
    pub summary: String,
    pub body: String,
    pub article_url: Option<String>,
    pub image_url: Option<String>,
    pub image_key: Option<String>,
    pub image_bytes: Option<Vec<u8>>,
}

pub fn start_fetch() -> mpsc::Receiver<Result<Vec<MinecraftNews>, String>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(fetch_latest_news());
    });
    receiver
}

pub fn fetch_latest_news() -> Result<Vec<MinecraftNews>, String> {
    let root = cache_directory();
    fs::create_dir_all(&root).map_err(|error| format!("无法创建动态缓存目录：{error}"))?;
    let body = fetch_json_with_etag(NEWS_URL, &root.join("news.json"), &root.join("news.etag"))?;
    parse_news(&body, &root)
}

fn parse_news(body: &str, cache: &Path) -> Result<Vec<MinecraftNews>, String> {
    parse_news_inner(body, Some(cache))
}

fn parse_news_inner(body: &str, cache: Option<&Path>) -> Result<Vec<MinecraftNews>, String> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| format!("Minecraft 动态 JSON 格式无效：{error}"))?;
    let entries = value
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(|| "Minecraft 动态缺少 entries".to_owned())?;
    let mut news = entries
        .iter()
        .filter_map(|entry| {
            let title = plain_text(entry.get("title")?.as_str()?);
            if title.is_empty() {
                return None;
            }
            let id = entry
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| Sha1::from(title.as_bytes()).digest().to_string());
            let category = entry
                .get("category")
                .and_then(Value::as_str)
                .or_else(|| {
                    entry
                        .get("newsType")
                        .and_then(Value::as_array)
                        .and_then(|types| types.first())
                        .and_then(Value::as_str)
                })
                .or_else(|| entry.get("tag").and_then(Value::as_str))
                .map(plain_text)
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "Minecraft".to_owned());
            let date = entry
                .get("date")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let summary = entry
                .get("text")
                .and_then(Value::as_str)
                .or_else(|| entry.get("description").and_then(Value::as_str))
                .map(plain_text)
                .unwrap_or_default();
            let body = entry
                .get("body")
                .and_then(Value::as_str)
                .or_else(|| entry.get("content").and_then(Value::as_str))
                .map(plain_text)
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| summary.clone());
            let article_url = entry
                .get("readMoreLink")
                .and_then(Value::as_str)
                .or_else(|| entry.get("articleUrl").and_then(Value::as_str))
                .map(str::to_owned);
            let image_url = image_url(entry);
            let image_key = image_url.as_ref().map(|url| image_cache_key(url));
            Some(MinecraftNews {
                id,
                title,
                category,
                date,
                summary,
                body,
                article_url,
                image_url,
                image_key,
                image_bytes: None,
            })
        })
        .collect::<Vec<_>>();
    news.sort_by(|left, right| right.date.cmp(&left.date));
    news.dedup_by(|left, right| left.id == right.id);
    news.truncate(12);
    if let Some(cache) = cache {
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
    Ok(news)
}

fn image_url(entry: &Value) -> Option<String> {
    for pointer in [
        "/newsPageImage/url",
        "/playPageImage/url",
        "/image/url",
        "/imageUrl",
    ] {
        if let Some(url) = entry.pointer(pointer).and_then(Value::as_str) {
            return Some(if url.starts_with('/') {
                format!("https://launchercontent.mojang.com{url}")
            } else {
                url.to_owned()
            });
        }
    }
    None
}

fn load_image(url: &str, cache: &Path) -> Result<Vec<u8>, String> {
    if !allowed_url(url) {
        return Err("拒绝非 Mojang 官方动态图片".to_owned());
    }
    let path = cache.join(format!("{}.img", image_cache_key(url)));
    if let Ok(bytes) = fs::read(&path)
        && !bytes.is_empty()
        && bytes.len() <= MAX_IMAGE_BYTES
    {
        return Ok(bytes);
    }
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(8))
        .timeout_read(Duration::from_secs(18))
        .build();
    let response = agent
        .get(url)
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(|error| format!("下载动态图片失败：{error}"))?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut bytes)
        .map_err(|error| format!("读取动态图片失败：{error}"))?;
    if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES {
        return Err("Minecraft 动态图片大小异常".to_owned());
    }
    fs::write(&path, &bytes).map_err(|error| format!("缓存动态图片失败：{error}"))?;
    Ok(bytes)
}

fn fetch_json_with_etag(url: &str, cache_path: &Path, etag_path: &Path) -> Result<String, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(30))
        .build();
    let etag = fs::read_to_string(etag_path).ok();
    let mut request = agent.get(url).set("User-Agent", USER_AGENT);
    if let Some(etag) = &etag {
        request = request.set("If-None-Match", etag.trim());
    }
    match request.call() {
        Ok(response) => {
            let new_etag = response.header("etag").map(str::to_owned);
            let body = response
                .into_string()
                .map_err(|error| format!("读取 Minecraft 动态失败：{error}"))?;
            if let Some(parent) = cache_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("无法创建动态缓存目录：{error}"))?;
            }
            fs::write(cache_path, &body)
                .map_err(|error| format!("缓存 Minecraft 动态失败：{error}"))?;
            if let Some(etag) = new_etag {
                let _ = fs::write(etag_path, etag);
            }
            Ok(body)
        }
        Err(ureq::Error::Status(304, _)) => {
            fs::read_to_string(cache_path).map_err(|error| format!("读取动态缓存失败：{error}"))
        }
        Err(error) => fs::read_to_string(cache_path)
            .ok()
            .filter(|body| !body.trim().is_empty())
            .ok_or_else(|| format!("请求 Minecraft 动态失败：{error}")),
    }
}

fn image_cache_key(url: &str) -> String {
    Sha1::from(url.as_bytes()).digest().to_string()
}

fn allowed_url(url: &str) -> bool {
    [
        "https://launchercontent.mojang.com/",
        "https://www.minecraft.net/",
        "https://minecraft.net/",
    ]
    .iter()
    .any(|prefix| url.starts_with(prefix))
}

pub fn allowed_article_url(url: &str) -> bool {
    [
        "https://www.minecraft.net/",
        "https://minecraft.net/",
        "https://www.mojang.com/",
        "https://launchercontent.mojang.com/",
    ]
    .iter()
    .any(|prefix| url.starts_with(prefix))
}

fn plain_text(value: &str) -> String {
    let mut output = String::new();
    let mut in_tag = false;
    let mut chars = value.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            '&' if !in_tag => {
                let mut entity = String::new();
                while let Some(next) = chars.peek().copied() {
                    if next == ';' || entity.len() >= 8 {
                        break;
                    }
                    entity.push(next);
                    chars.next();
                }
                if chars.peek() == Some(&';') {
                    chars.next();
                    output.push(match entity.as_str() {
                        "amp" => '&',
                        "lt" => '<',
                        "gt" => '>',
                        "quot" => '"',
                        "apos" | "#39" => '\'',
                        "nbsp" => ' ',
                        _ => ' ',
                    });
                }
            }
            _ if !in_tag => output.push(character),
            _ => {}
        }
    }
    output.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn cache_directory() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("BlockLauncher")
        .join("cache")
        .join("minecraft-news")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
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
        let news = parse_news_inner(body, None).expect("news");
        assert_eq!(news.len(), 1);
        assert_eq!(news[0].title, "Minecraft update");
        assert_eq!(news[0].summary, "A & B");
    }

    #[test]
    fn rejects_non_mojang_news_sources() {
        assert!(allowed_url(
            "https://launchercontent.mojang.com/images/a.png"
        ));
        assert!(!allowed_url("https://example.org/a.png"));
        assert!(allowed_article_url("https://www.minecraft.net/article"));
    }
}
