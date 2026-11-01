use std::io::Read;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::brand;

const USER_AGENT: &str = brand::USER_AGENT;

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

#[derive(Clone, Debug)]
pub struct FavoriteIconRequest {
    pub key: String,
    pub site_url: String,
    pub icon_source: String,
}

pub fn normalize_site_url(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("请输入网站地址".to_owned());
    }
    let candidate = if trimmed.contains("://") {
        trimmed.to_owned()
    } else {
        format!("https://{trimmed}")
    };
    let lower = candidate.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return Err("只支持 HTTP 或 HTTPS 网站".to_owned());
    }
    if candidate
        .chars()
        .any(|character| character.is_whitespace() || character.is_control())
    {
        return Err("网站地址不能包含空格或控制字符".to_owned());
    }
    let (_, rest) = candidate
        .split_once("://")
        .ok_or_else(|| "网站地址格式无效".to_owned())?;
    let host = rest.split('/').next().unwrap_or_default();
    if host.is_empty() || !host.contains('.') {
        return Err("网站地址缺少有效域名".to_owned());
    }
    Ok(candidate)
}

pub fn normalize_icon_source(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") {
        return normalize_site_url(trimmed);
    }
    let path = std::path::PathBuf::from(trimmed);
    if path.is_file() {
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "webp" | "ico") {
            return Err("本地图标只支持 PNG、JPG、WebP 或 ICO".to_owned());
        }
        if path
            .metadata()
            .map(|metadata| metadata.len() > 4 * 1024 * 1024)
            .unwrap_or(true)
        {
            return Err("本地图标不能超过 4 MB".to_owned());
        }
        return path
            .canonicalize()
            .map(|path| path.to_string_lossy().into_owned())
            .map_err(|error| format!("无法读取本地图标：{error}"));
    }
    if !trimmed.contains('\\') && trimmed.contains('.') {
        normalize_site_url(trimmed)
    } else {
        Err("请输入完整的图标 URL，或选择存在的本地图片".to_owned())
    }
}

pub fn site_domain(url: &str) -> String {
    url.split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or(url)
        .to_ascii_lowercase()
}

pub fn start_fetch_icons(
    requests: Vec<FavoriteIconRequest>,
) -> mpsc::Receiver<Vec<(String, Vec<u8>)>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut icons = Vec::new();
        for request in requests {
            if !request.icon_source.trim().is_empty() {
                let custom = if request.icon_source.starts_with("http://")
                    || request.icon_source.starts_with("https://")
                {
                    fetch_bytes(&request.icon_source)
                } else {
                    fetch_local_icon(&request.icon_source)
                };
                if let Ok(bytes) = custom
                    && image::load_from_memory(&bytes).is_ok()
                {
                    icons.push((request.key, bytes));
                    continue;
                }
            }
            let domain = site_domain(&request.site_url);
            if domain.is_empty() {
                continue;
            }
            let candidates = [
                format!("https://unavatar.io/{domain}"),
                format!("https://{domain}/favicon.ico"),
                format!("https://{domain}/apple-touch-icon.png"),
                format!("https://www.google.com/s2/favicons?domain={domain}&sz=128"),
                format!("https://icons.duckduckgo.com/ip3/{domain}.ico"),
            ];
            for candidate in candidates {
                if let Ok(bytes) = fetch_bytes(&candidate)
                    && image::load_from_memory(&bytes).is_ok()
                {
                    icons.push((request.key.clone(), bytes));
                    break;
                }
            }
        }
        let _ = sender.send(icons);
    });
    receiver
}

fn fetch_local_icon(path: &str) -> Result<Vec<u8>, String> {
    let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
    if metadata.len() > 4 * 1024 * 1024 {
        return Err("本地图标不能超过 4 MB".to_owned());
    }
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    if bytes.is_empty() {
        Err("图标文件为空".to_owned())
    } else {
        Ok(bytes)
    }
}

pub fn open_site(url: &str) -> Result<(), String> {
    let url = normalize_site_url(url)?;
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("rundll32.exe")
            .args(["url.dll,FileProtocolHandler", &url])
            .creation_flags(0x0800_0000)
            .spawn()
            .map_err(|error| error.to_string())?;
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}

fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout_read(Duration::from_secs(10))
        .build();
    let mut bytes = Vec::new();
    agent
        .get(url)
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(|error| error.to_string())?
        .into_reader()
        .take(4 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.is_empty() {
        Err("图标响应为空".to_owned())
    } else {
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_user_website_urls() {
        assert_eq!(
            normalize_site_url("example.com").unwrap(),
            "https://example.com"
        );
        assert!(normalize_site_url("javascript:alert(1)").is_err());
        assert!(normalize_site_url("file:///C:/test").is_err());
    }

    #[test]
    fn extracts_domains() {
        assert_eq!(
            site_domain("https://modrinth.com/mod/example"),
            "modrinth.com"
        );
    }

    #[test]
    fn normalizes_custom_icon_sources() {
        assert_eq!(normalize_icon_source("").unwrap(), "");
        assert_eq!(
            normalize_icon_source("example.com/icon.png").unwrap(),
            "https://example.com/icon.png"
        );
        assert!(normalize_icon_source("not-a-real-icon").is_err());
    }
}
