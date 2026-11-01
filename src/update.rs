use std::io::Read;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use eframe::egui;
use serde_json::Value;

use crate::brand;

pub const RELEASES_URL: &str = "https://github.com/HerobrineInMinecraft/Genesis_Launcher/releases";
const LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/HerobrineInMinecraft/Genesis_Launcher/releases/latest";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LauncherRelease {
    pub version: String,
    pub name: String,
    pub page_url: String,
}

pub fn start_latest_release_check(
    ctx: &egui::Context,
) -> mpsc::Receiver<Result<LauncherRelease, String>> {
    let (sender, receiver) = mpsc::channel();
    let repaint = ctx.clone();
    thread::spawn(move || {
        let result = fetch_latest_release();
        let _ = sender.send(result);
        repaint.request_repaint();
    });
    receiver
}

pub fn is_newer_version(current: &str, candidate: &str) -> bool {
    version_key(candidate) > version_key(current)
}

fn fetch_latest_release() -> Result<LauncherRelease, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(8))
        .timeout_read(Duration::from_secs(15))
        .build();
    let response = agent
        .get(LATEST_RELEASE_API)
        .set("User-Agent", brand::USER_AGENT)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|error| match error {
            ureq::Error::Status(404, _) => "GitHub 仓库暂未发布正式版本".to_owned(),
            _ => format!("无法连接 GitHub Releases：{error}"),
        })?;
    let mut body = Vec::new();
    response
        .into_reader()
        .take(2 * 1024 * 1024)
        .read_to_end(&mut body)
        .map_err(|error| format!("读取更新信息失败：{error}"))?;
    let value: Value =
        serde_json::from_slice(&body).map_err(|error| format!("更新信息格式无效：{error}"))?;
    parse_release(&value)
}

fn parse_release(value: &Value) -> Result<LauncherRelease, String> {
    let version = value
        .get("tag_name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .ok_or_else(|| "GitHub Release 缺少版本标签".to_owned())?;
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(version);
    let page_url = value
        .get("html_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| url.starts_with(RELEASES_URL))
        .unwrap_or(RELEASES_URL);
    Ok(LauncherRelease {
        version: version.trim_start_matches(['v', 'V']).to_owned(),
        name: name.to_owned(),
        page_url: page_url.to_owned(),
    })
}

fn version_key(version: &str) -> (Vec<u64>, bool) {
    let version = version.trim().trim_start_matches(['v', 'V']);
    let (core, pre_release) = version
        .split_once('-')
        .map_or((version, None), |(core, suffix)| (core, Some(suffix)));
    let mut numbers = core
        .split('.')
        .map(|part| {
            part.chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse()
                .unwrap_or(0)
        })
        .collect::<Vec<_>>();
    numbers.resize(numbers.len().max(3), 0);
    (numbers, pre_release.is_none())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_release_versions() {
        assert!(is_newer_version("0.1.0", "v0.1.1"));
        assert!(is_newer_version("0.1.0", "v0.2.0"));
        assert!(is_newer_version("1.9.0", "v1.10.0"));
        assert!(!is_newer_version("0.2.0", "v0.2.0"));
        assert!(is_newer_version("0.2.0-beta.1", "v0.2.0"));
    }

    #[test]
    fn parses_github_latest_release() {
        let release = parse_release(&serde_json::json!({
            "tag_name": "v0.2.0",
            "name": "Genesis Launcher 0.2.0",
            "html_url": "https://github.com/HerobrineInMinecraft/Genesis_Launcher/releases/tag/v0.2.0"
        }))
        .unwrap();
        assert_eq!(release.version, "0.2.0");
        assert_eq!(release.name, "Genesis Launcher 0.2.0");
        assert!(release.page_url.contains("/releases/tag/v0.2.0"));
    }
}
