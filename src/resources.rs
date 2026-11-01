use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;
use sha1_smol::Sha1;

use crate::brand;

const USER_AGENT: &str = brand::USER_AGENT;
const CACHE_TTL: Duration = Duration::from_secs(300);
const CACHE_LIMIT: usize = 48;
const MCIM_HOST: &str = "https://mod.mcimirror.top";
const ROUTE_CACHE_TTL: Duration = Duration::from_secs(120);

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum ResourceCategory {
    Mod,
    Modpack,
    Shader,
    ResourcePack,
    Datapack,
}

impl ResourceCategory {
    pub const ALL: [Self; 5] = [
        Self::Mod,
        Self::Modpack,
        Self::Shader,
        Self::ResourcePack,
        Self::Datapack,
    ];

    pub fn project_type(self) -> &'static str {
        match self {
            Self::Mod => "mod",
            Self::Modpack => "modpack",
            Self::Shader => "shader",
            Self::ResourcePack => "resourcepack",
            Self::Datapack => "datapack",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Mod => "模组",
            Self::Modpack => "整合包",
            Self::Shader => "光影",
            Self::ResourcePack => "资源包",
            Self::Datapack => "数据包",
        }
    }

    pub fn target_folder(self) -> &'static str {
        match self {
            Self::Mod => "mods",
            Self::Modpack => "downloads/modpacks",
            Self::Shader => "shaderpacks",
            Self::ResourcePack => "resourcepacks",
            Self::Datapack => "downloads/datapacks",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ResourceProject {
    pub id: String,
    pub title: String,
    pub description: String,
    pub author: String,
    pub downloads: u64,
    pub categories: Vec<String>,
    pub game_versions: Vec<String>,
}

pub fn start_search(
    category: ResourceCategory,
    query: String,
    game_version: String,
    loader: Option<String>,
) -> mpsc::Receiver<Result<Vec<ResourceProject>, String>> {
    let (sender, receiver) = mpsc::channel();
    let key = SearchKey {
        category,
        query: query.clone(),
        game_version: game_version.clone(),
        loader: loader.clone(),
    };
    if let Some(results) = cached_search(&key) {
        let _ = sender.send(Ok(results));
        return receiver;
    }
    thread::spawn(move || {
        if let Some(results) = cached_search(&key) {
            let _ = sender.send(Ok(results));
            return;
        }
        let result = search(category, &query, &game_version, loader.as_deref());
        if let Ok(results) = &result {
            store_search(key, results);
        }
        let _ = sender.send(result);
    });
    receiver
}

pub fn start_prefetch(excluded: ResourceCategory, game_version: String, loader: Option<String>) {
    thread::spawn(move || {
        for category in ResourceCategory::ALL {
            if category == excluded {
                continue;
            }
            let key = SearchKey {
                category,
                query: String::new(),
                game_version: game_version.clone(),
                loader: loader.clone(),
            };
            if cached_search(&key).is_some() {
                continue;
            }
            if let Ok(results) = search(category, "", &game_version, loader.as_deref()) {
                store_search(key, &results);
            }
        }
    });
}

pub fn start_download(
    project: ResourceProject,
    category: ResourceCategory,
    game_version: String,
    loader: Option<String>,
    game_directory: PathBuf,
) -> mpsc::Receiver<Result<PathBuf, String>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = download(
            &project,
            category,
            &game_version,
            loader.as_deref(),
            &game_directory,
        );
        let _ = sender.send(result);
    });
    receiver
}

fn search(
    category: ResourceCategory,
    query: &str,
    game_version: &str,
    loader: Option<&str>,
) -> Result<Vec<ResourceProject>, String> {
    let facets = search_facets(category, game_version, loader);
    let url = format!(
        "https://api.modrinth.com/v2/search?query={}&limit=20&index=relevance&facets={}",
        escape_query(query),
        escape_query(&facets)
    );
    let body = request_text_with_mirror(&url)?;
    let value: Value = serde_json::from_str(&body)
        .map_err(|error| format!("Modrinth 搜索结果格式无效：{error}"))?;
    let hits = value
        .get("hits")
        .and_then(Value::as_array)
        .ok_or_else(|| "Modrinth 搜索结果缺少 hits".to_owned())?;
    Ok(hits
        .iter()
        .filter_map(|hit| {
            Some(ResourceProject {
                id: hit.get("project_id")?.as_str()?.to_owned(),
                title: hit.get("title")?.as_str()?.to_owned(),
                description: hit
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                author: hit
                    .get("author")
                    .and_then(Value::as_str)
                    .unwrap_or("未知作者")
                    .to_owned(),
                downloads: hit.get("downloads").and_then(Value::as_u64).unwrap_or(0),
                categories: string_array(hit.get("categories")),
                game_versions: string_array(hit.get("versions")),
            })
        })
        .collect())
}

fn search_facets(category: ResourceCategory, game_version: &str, loader: Option<&str>) -> String {
    let mut facet_groups = vec![
        format!("\"project_type:{}\"", category.project_type()),
        format!("\"versions:{}\"", escape_query(game_version)),
    ];
    if let Some(loader) = loader
        && matches!(category, ResourceCategory::Mod | ResourceCategory::Modpack)
    {
        facet_groups.push(format!(
            "\"categories:{}\"",
            escape_query(&loader.to_lowercase())
        ));
    }
    format!(
        "[{}]",
        facet_groups
            .iter()
            .map(|facet| format!("[{facet}]"))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn download(
    project: &ResourceProject,
    category: ResourceCategory,
    game_version: &str,
    loader: Option<&str>,
    game_directory: &Path,
) -> Result<PathBuf, String> {
    let url = format!(
        "https://api.modrinth.com/v2/project/{}/version",
        escape_query(&project.id)
    );
    let body = request_text_with_mirror(&url)?;
    let versions: Value = serde_json::from_str(&body)
        .map_err(|error| format!("Modrinth 版本列表格式无效：{error}"))?;
    let versions = versions
        .as_array()
        .ok_or_else(|| "Modrinth 版本列表无效".to_owned())?;
    let loader = loader.map(str::to_lowercase);
    let selected = versions
        .iter()
        .find(|version| {
            string_array(version.get("game_versions"))
                .iter()
                .any(|v| v == game_version)
                && loader.as_ref().is_none_or(|wanted| {
                    string_array(version.get("loaders"))
                        .iter()
                        .any(|actual| actual.eq_ignore_ascii_case(wanted))
                })
        })
        .or_else(|| {
            versions.iter().find(|version| {
                string_array(version.get("game_versions"))
                    .iter()
                    .any(|value| value == game_version)
            })
        })
        .ok_or_else(|| {
            format!(
                "{} 没有适用于 Minecraft {game_version} 的版本",
                project.title
            )
        })?;
    let file = selected
        .get("files")
        .and_then(Value::as_array)
        .and_then(|files| {
            files
                .iter()
                .find(|file| file.get("primary").and_then(Value::as_bool) == Some(true))
                .or_else(|| files.first())
        })
        .ok_or_else(|| "Modrinth 版本缺少下载文件".to_owned())?;
    let download_url = file
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| "Modrinth 下载地址缺失".to_owned())?;
    if !allowed_url(download_url) {
        return Err("拒绝非 Modrinth 官方下载地址".to_owned());
    }
    let filename = file
        .get("filename")
        .and_then(Value::as_str)
        .and_then(|name| Path::new(name).file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| "Modrinth 下载文件名无效".to_owned())?;
    let expected = file
        .pointer("/hashes/sha1")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let target_dir = game_directory.join(category.target_folder());
    fs::create_dir_all(&target_dir).map_err(|error| format!("无法创建下载目录：{error}"))?;
    let target = target_dir.join(filename);
    let part = target.with_extension("part");
    let bytes = request_bytes_with_mirror(download_url)?;
    if expected.len() == 40
        && !Sha1::from(&bytes)
            .digest()
            .to_string()
            .eq_ignore_ascii_case(expected)
    {
        return Err("下载文件 SHA1 校验失败".to_owned());
    }
    fs::write(&part, &bytes).map_err(|error| format!("写入下载文件失败：{error}"))?;
    if target.exists() {
        fs::remove_file(&target).map_err(|error| format!("替换旧文件失败：{error}"))?;
    }
    fs::rename(&part, &target).map_err(|error| format!("提交下载文件失败：{error}"))?;
    Ok(target)
}

fn request_text(url: &str) -> Result<String, String> {
    request(url)?
        .into_string()
        .map_err(|error| format!("读取下载源响应失败：{error}"))
}

fn request_bytes(url: &str) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    request(url)?
        .into_reader()
        .read_to_end(&mut bytes)
        .map_err(|error| format!("读取下载文件失败：{error}"))?;
    Ok(bytes)
}

fn request_text_with_mirror(official_url: &str) -> Result<String, String> {
    let preferred = preferred_modrinth_route();
    let mut errors = Vec::new();
    for route in [preferred, preferred.other()] {
        let Some(url) = routed_url(official_url, route) else {
            continue;
        };
        match request_text(&url) {
            Ok(body) => {
                remember_modrinth_route(route);
                return Ok(body);
            }
            Err(error) => errors.push(format!("{}：{error}", route.label())),
        }
    }
    Err(format!("Modrinth 线路均不可用：{}", errors.join("；")))
}

fn request_bytes_with_mirror(official_url: &str) -> Result<Vec<u8>, String> {
    let preferred = preferred_modrinth_route();
    let mut errors = Vec::new();
    for route in [preferred, preferred.other()] {
        let Some(url) = routed_url(official_url, route) else {
            continue;
        };
        match request_bytes(&url) {
            Ok(bytes) => {
                remember_modrinth_route(route);
                return Ok(bytes);
            }
            Err(error) => errors.push(format!("{}：{error}", route.label())),
        }
    }
    Err(format!("Modrinth 下载线路均不可用：{}", errors.join("；")))
}

fn mcim_mirror_url(url: &str) -> Option<String> {
    if let Some(path) = url.strip_prefix("https://api.modrinth.com") {
        Some(format!("{MCIM_HOST}/modrinth{path}"))
    } else {
        url.strip_prefix("https://cdn.modrinth.com")
            .map(|path| format!("{MCIM_HOST}{path}"))
    }
}

fn routed_url(official_url: &str, route: ModrinthRoute) -> Option<String> {
    match route {
        ModrinthRoute::Official => Some(official_url.to_owned()),
        ModrinthRoute::Mirror => mcim_mirror_url(official_url),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModrinthRoute {
    Official,
    Mirror,
}

impl ModrinthRoute {
    fn label(self) -> &'static str {
        match self {
            Self::Official => "Modrinth 官方源",
            Self::Mirror => "MCIM 国内镜像",
        }
    }

    fn other(self) -> Self {
        match self {
            Self::Official => Self::Mirror,
            Self::Mirror => Self::Official,
        }
    }
}

#[derive(Default)]
struct RoutePreference {
    route: Option<ModrinthRoute>,
    measured_at: Option<Instant>,
}

fn route_preference() -> &'static Mutex<RoutePreference> {
    static PREFERENCE: OnceLock<Mutex<RoutePreference>> = OnceLock::new();
    PREFERENCE.get_or_init(|| Mutex::new(RoutePreference::default()))
}

fn preferred_modrinth_route() -> ModrinthRoute {
    {
        let Ok(preference) = route_preference().lock() else {
            return ModrinthRoute::Official;
        };
        if let (Some(route), Some(measured_at)) = (preference.route, preference.measured_at)
            && measured_at.elapsed() < ROUTE_CACHE_TTL
        {
            return route;
        }
    }
    let route = measure_modrinth_routes();
    remember_modrinth_route(route);
    route
}

fn remember_modrinth_route(route: ModrinthRoute) {
    if let Ok(mut preference) = route_preference().lock() {
        preference.route = Some(route);
        preference.measured_at = Some(Instant::now());
    }
}

fn measure_modrinth_routes() -> ModrinthRoute {
    let (sender, receiver) = mpsc::channel();
    for (route, url) in [
        (
            ModrinthRoute::Official,
            "https://api.modrinth.com/v2/search?query=&limit=1".to_owned(),
        ),
        (
            ModrinthRoute::Mirror,
            format!("{MCIM_HOST}/modrinth/v2/search?query=&limit=1"),
        ),
    ] {
        let sender = sender.clone();
        thread::spawn(move || {
            let started = Instant::now();
            let agent = ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(3))
                .timeout_read(Duration::from_secs(5))
                .build();
            let reachable = agent
                .get(&url)
                .set("User-Agent", USER_AGENT)
                .call()
                .ok()
                .and_then(|response| response.into_string().ok())
                .is_some();
            if reachable {
                let _ = sender.send((route, started.elapsed()));
            }
        });
    }
    drop(sender);

    let deadline = Instant::now() + Duration::from_secs(4);
    let mut samples = Vec::new();
    for _ in 0..2 {
        let comparison_deadline = if samples.is_empty() {
            deadline
        } else {
            (Instant::now() + Duration::from_millis(400)).min(deadline)
        };
        let remaining = comparison_deadline.saturating_duration_since(Instant::now());
        let Ok(result) = receiver.recv_timeout(remaining) else {
            break;
        };
        samples.push(result);
    }
    fastest_modrinth_route(&samples)
}

fn fastest_modrinth_route(samples: &[(ModrinthRoute, Duration)]) -> ModrinthRoute {
    samples
        .iter()
        .min_by_key(|(_, latency)| *latency)
        .map_or(ModrinthRoute::Official, |(route, _)| *route)
}

fn request(url: &str) -> Result<ureq::Response, String> {
    if !allowed_url(url) {
        return Err("拒绝非受支持的资源下载源地址".to_owned());
    }
    let mut last_error = String::new();
    for attempt in 0..3 {
        match http_agent().get(url).set("User-Agent", USER_AGENT).call() {
            Ok(response) => return Ok(response),
            Err(error) => last_error = error.to_string(),
        }
        if attempt < 2 {
            thread::sleep(Duration::from_millis(350 * (1 << attempt)));
        }
    }
    Err(format!("资源下载源请求失败：{last_error}"))
}

fn http_agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(6))
            .timeout_read(Duration::from_secs(20))
            .build()
    })
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct SearchKey {
    category: ResourceCategory,
    query: String,
    game_version: String,
    loader: Option<String>,
}

type SearchCache = HashMap<SearchKey, (Instant, Vec<ResourceProject>)>;

fn cached_search(key: &SearchKey) -> Option<Vec<ResourceProject>> {
    let mut cache = search_cache().lock().ok()?;
    cache.retain(|_, (stored_at, _)| stored_at.elapsed() < CACHE_TTL);
    cache.get(key).map(|(_, results)| results.clone())
}

fn store_search(key: SearchKey, results: &[ResourceProject]) {
    let Ok(mut cache) = search_cache().lock() else {
        return;
    };
    cache.retain(|_, (stored_at, _)| stored_at.elapsed() < CACHE_TTL);
    if cache.len() >= CACHE_LIMIT
        && let Some(oldest_key) = cache
            .iter()
            .min_by_key(|(_, (stored_at, _))| *stored_at)
            .map(|(key, _)| key.clone())
    {
        cache.remove(&oldest_key);
    }
    cache.insert(key, (Instant::now(), results.to_vec()));
}

fn search_cache() -> &'static Mutex<SearchCache> {
    static CACHE: OnceLock<Mutex<SearchCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn allowed_url(url: &str) -> bool {
    url.starts_with("https://api.modrinth.com/")
        || url.starts_with("https://cdn.modrinth.com/")
        || url.starts_with("https://mod.mcimirror.top/modrinth/")
        || url.starts_with("https://mod.mcimirror.top/data/")
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn escape_query(value: &str) -> String {
    let mut output = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            output.push(byte as char);
        } else {
            output.push_str(&format!("%{byte:02X}"));
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_allows_modrinth_hosts() {
        assert!(allowed_url("https://api.modrinth.com/v2/search"));
        assert!(allowed_url("https://cdn.modrinth.com/data/file.jar"));
        assert!(allowed_url("https://mod.mcimirror.top/modrinth/v2/search"));
        assert!(allowed_url("https://mod.mcimirror.top/data/file.jar"));
        assert!(!allowed_url("https://mod.mcimirror.top.evil/file.jar"));
        assert!(!allowed_url("https://example.org/file.jar"));
    }

    #[test]
    fn maps_modrinth_urls_to_mcim_mirror() {
        assert_eq!(
            mcim_mirror_url("https://api.modrinth.com/v2/search?query=test").as_deref(),
            Some("https://mod.mcimirror.top/modrinth/v2/search?query=test")
        );
        assert_eq!(
            mcim_mirror_url("https://cdn.modrinth.com/data/a/b.jar").as_deref(),
            Some("https://mod.mcimirror.top/data/a/b.jar")
        );
    }

    #[test]
    fn builds_modrinth_facets_with_two_levels() {
        assert_eq!(
            search_facets(ResourceCategory::Mod, "1.21.1", Some("fabric")),
            r#" [["project_type:mod"],["versions:1.21.1"],["categories:fabric"]]"#.trim()
        );
    }

    #[test]
    fn chooses_the_lower_latency_modrinth_route() {
        assert_eq!(
            fastest_modrinth_route(&[
                (ModrinthRoute::Official, Duration::from_millis(80)),
                (ModrinthRoute::Mirror, Duration::from_millis(140)),
            ]),
            ModrinthRoute::Official
        );
        assert_eq!(
            fastest_modrinth_route(&[
                (ModrinthRoute::Official, Duration::from_millis(620)),
                (ModrinthRoute::Mirror, Duration::from_millis(90)),
            ]),
            ModrinthRoute::Mirror
        );
        assert_eq!(fastest_modrinth_route(&[]), ModrinthRoute::Official);
    }
}
