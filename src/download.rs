use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;
use sha1_smol::Sha1;
use zip::ZipArchive;

use crate::brand;

const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const JAVA_RUNTIME_MANIFEST_URL: &str = "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";
const USER_AGENT: &str = brand::USER_AGENT;
const WORKERS: usize = 6;

#[derive(Clone, Debug)]
pub struct OfficialVersion {
    pub id: String,
    pub kind: String,
    pub url: String,
    pub sha1: String,
}

#[derive(Clone, Debug)]
pub struct InstallUpdate {
    pub stage: String,
    pub completed_bytes: u64,
    pub total_bytes: u64,
    pub files_done: usize,
    pub files_total: usize,
    pub bytes_per_second: u64,
    pub finished: bool,
    pub success: bool,
    pub message: String,
}

impl InstallUpdate {
    fn running(
        stage: impl Into<String>,
        completed_bytes: u64,
        total_bytes: u64,
        files_done: usize,
        files_total: usize,
    ) -> Self {
        Self {
            stage: stage.into(),
            completed_bytes,
            total_bytes,
            files_done,
            files_total,
            bytes_per_second: 0,
            finished: false,
            success: false,
            message: String::new(),
        }
    }
}

#[derive(Clone)]
struct DownloadTask {
    url: String,
    path: PathBuf,
    sha1: Option<String>,
    size: u64,
}

struct Progress {
    sender: mpsc::Sender<InstallUpdate>,
    completed_bytes: AtomicU64,
    total_bytes: u64,
    files_done: AtomicUsize,
    files_total: usize,
    stage: Mutex<String>,
    last_report: Mutex<u64>,
    started_at: Instant,
}

impl Progress {
    fn send(&self, force: bool, message: &str) {
        let completed = self.completed_bytes.load(Ordering::Relaxed);
        let mut last = self.last_report.lock().unwrap();
        if !force && completed.saturating_sub(*last) < 512 * 1024 {
            return;
        }
        *last = completed;
        let stage = self.stage.lock().unwrap().clone();
        let mut update = InstallUpdate::running(
            stage,
            completed,
            self.total_bytes,
            self.files_done.load(Ordering::Relaxed),
            self.files_total,
        );
        let elapsed = self.started_at.elapsed().as_secs_f64();
        if elapsed > 0.2 {
            update.bytes_per_second = (completed as f64 / elapsed) as u64;
        }
        let _ = self.sender.send(update);
        let _ = message;
    }

    fn set_stage(&self, stage: impl Into<String>) {
        *self.stage.lock().unwrap() = stage.into();
        self.send(true, "");
    }
}

pub fn start_manifest_fetch() -> mpsc::Receiver<Result<Vec<OfficialVersion>, String>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(fetch_official_versions());
    });
    receiver
}

pub fn fetch_official_versions() -> Result<Vec<OfficialVersion>, String> {
    let cache_path = cache_directory().join("version_manifest_v2.json");
    let etag_path = cache_directory().join("version_manifest_v2.etag");
    let body = match fetch_cached_text(MANIFEST_URL, &cache_path, &etag_path, "版本清单") {
        Ok(body) => body,
        Err(network_error) => fs::read_to_string(&cache_path)
            .ok()
            .filter(|body| !body.trim().is_empty())
            .ok_or(network_error)?,
    };
    parse_manifest(&body)
}

fn fetch_cached_text(
    url: &str,
    cache_path: &Path,
    etag_path: &Path,
    label: &str,
) -> Result<String, String> {
    ensure_allowed_url(url)?;
    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("无法创建 {label} 缓存目录：{error}"))?;
    }
    let agent = agent();
    let etag = fs::read_to_string(etag_path).ok();
    let mut last_error = String::new();
    for attempt in 0..3 {
        let mut request = agent.get(url).set("User-Agent", USER_AGENT);
        if let Some(etag) = &etag {
            request = request.set("If-None-Match", etag.trim());
        }
        match request.call() {
            Ok(response) => {
                let response_etag = response.header("etag").map(str::to_owned);
                let body = response
                    .into_string()
                    .map_err(|error| format!("读取{label}失败：{error}"))?;
                let part = part_path(cache_path);
                fs::write(&part, body.as_bytes())
                    .map_err(|error| format!("写入{label}缓存失败：{error}"))?;
                replace_file(&part, cache_path)?;
                if let Some(etag) = response_etag {
                    let _ = fs::write(etag_path, etag);
                }
                return Ok(body);
            }
            Err(ureq::Error::Status(304, _)) => {
                return fs::read_to_string(cache_path)
                    .map_err(|error| format!("读取{label}缓存失败：{error}"));
            }
            Err(ureq::Error::Status(status, _)) => {
                last_error = format!("HTTP {status}");
                if !is_retryable_status(status) {
                    return Err(format!("请求{label}失败：HTTP {status}"));
                }
            }
            Err(error) => last_error = error.to_string(),
        }
        if attempt < 2 {
            thread::sleep(backoff(attempt));
        }
    }
    Err(format!("请求{label}失败：{last_error}"))
}

fn is_retryable_status(status: u16) -> bool {
    matches!(status, 429 | 500 | 502 | 503)
}

pub fn start_install(
    game_directory: PathBuf,
    version: OfficialVersion,
) -> mpsc::Receiver<InstallUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = install_version(&game_directory, &version, &sender);
        let (success, message, total) = match result {
            Ok(total) => (true, format!("{} 安装完成", version.id), total),
            Err(error) => (false, error, 0),
        };
        let _ = sender.send(InstallUpdate {
            stage: if success {
                "安装完成".to_owned()
            } else {
                "安装失败".to_owned()
            },
            completed_bytes: if success { total } else { 0 },
            total_bytes: total,
            files_done: 0,
            files_total: 0,
            bytes_per_second: 0,
            finished: true,
            success,
            message,
        });
    });
    receiver
}

pub fn start_install_by_id(
    game_directory: PathBuf,
    version_id: String,
) -> mpsc::Receiver<InstallUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let version = fetch_official_versions().and_then(|versions| {
            versions
                .into_iter()
                .find(|version| version.id == version_id)
                .ok_or_else(|| format!("官方版本列表中未找到 {version_id}"))
        });
        let result = match version {
            Ok(version) => install_version(&game_directory, &version, &sender),
            Err(error) => Err(error),
        };
        let (success, message, total) = match result {
            Ok(total) => (true, format!("{version_id} 文件补全完成"), total),
            Err(error) => (false, error, 0),
        };
        let _ = sender.send(InstallUpdate {
            stage: if success {
                "安装完成".to_owned()
            } else {
                "安装失败".to_owned()
            },
            completed_bytes: if success { total } else { 0 },
            total_bytes: total,
            files_done: 0,
            files_total: 0,
            bytes_per_second: 0,
            finished: true,
            success,
            message,
        });
    });
    receiver
}

fn install_version(
    game_directory: &Path,
    version: &OfficialVersion,
    sender: &mpsc::Sender<InstallUpdate>,
) -> Result<u64, String> {
    ensure_allowed_url(&version.url)?;
    validate_path_component(&version.id)?;
    let versions_dir = game_directory.join("versions");
    let version_dir = versions_dir.join(&version.id);
    fs::create_dir_all(&version_dir).map_err(|error| format!("无法创建版本目录：{error}"))?;
    let json_path = version_dir.join(format!("{}.json", version.id));
    let _ = sender.send(InstallUpdate::running("下载版本清单", 0, 0, 0, 0));
    download_small_file(&version.url, &json_path, Some(&version.sha1))?;
    let json: Value = serde_json::from_str(
        &fs::read_to_string(&json_path).map_err(|error| format!("读取版本清单失败：{error}"))?,
    )
    .map_err(|error| format!("解析版本清单失败：{error}"))?;

    let client = json
        .pointer("/downloads/client")
        .ok_or_else(|| "版本清单缺少客户端下载信息".to_owned())?;
    let asset_index = json
        .get("assetIndex")
        .ok_or_else(|| "版本清单缺少资源索引".to_owned())?;
    let asset_id = asset_index
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("legacy");
    validate_path_component(asset_id)?;
    let asset_index_path = game_directory
        .join("assets")
        .join("indexes")
        .join(format!("{asset_id}.json"));
    if let (Some(url), Some(sha1)) = (
        asset_index.get("url").and_then(Value::as_str),
        asset_index.get("sha1").and_then(Value::as_str),
    ) {
        ensure_allowed_url(url)?;
        download_asset_index(url, &asset_index_path, sha1)?;
    }

    let mut tasks = Vec::new();
    add_download_task(&mut tasks, client, game_directory, &version.id)?;
    if let Some(libraries) = json.get("libraries").and_then(Value::as_array) {
        for library in libraries {
            if !rules_allow(library) {
                continue;
            }
            if let Some(artifact) = library.pointer("/downloads/artifact") {
                add_library_task(&mut tasks, artifact, game_directory)?;
            }
            if let Some(classifiers) = library
                .pointer("/downloads/classifiers")
                .and_then(Value::as_object)
            {
                for (name, artifact) in classifiers {
                    if name.starts_with("natives-windows") {
                        add_library_task(&mut tasks, artifact, game_directory)?;
                    }
                }
            }
        }
    }

    let index: Value = serde_json::from_str(
        &fs::read_to_string(&asset_index_path)
            .map_err(|error| format!("读取资源索引失败：{error}"))?,
    )
    .map_err(|error| format!("解析资源索引失败：{error}"))?;
    if let Some(objects) = index.get("objects").and_then(Value::as_object) {
        for object in objects.values() {
            let Some(hash) = object.get("hash").and_then(Value::as_str) else {
                continue;
            };
            if !is_sha1(hash) {
                return Err(format!("资源索引包含无效 SHA1：{hash}"));
            }
            let path = game_directory
                .join("assets")
                .join("objects")
                .join(&hash[..2])
                .join(hash);
            tasks.push(DownloadTask {
                url: format!(
                    "https://resources.download.minecraft.net/{}/{}",
                    &hash[..2],
                    hash
                ),
                path,
                sha1: Some(hash.to_owned()),
                size: object.get("size").and_then(Value::as_u64).unwrap_or(0),
            });
        }
    }

    deduplicate_tasks(&mut tasks);
    let pending: Vec<DownloadTask> = tasks
        .iter()
        .filter(|task| !download_is_complete(task))
        .cloned()
        .collect();
    let total_size: u64 = pending.iter().map(|task| task.size).sum();
    let progress = Arc::new(Progress {
        sender: sender.clone(),
        completed_bytes: AtomicU64::new(0),
        total_bytes: total_size,
        files_done: AtomicUsize::new(0),
        files_total: pending.len(),
        stage: Mutex::new("下载游戏文件".to_owned()),
        last_report: Mutex::new(0),
        started_at: Instant::now(),
    });
    progress.send(true, "");
    download_tasks(game_directory, &pending, progress.clone())?;

    progress.set_stage("解压 Windows natives");
    let natives_dir = version_dir.join("natives");
    fs::create_dir_all(&natives_dir).map_err(|error| format!("无法创建 natives 目录：{error}"))?;
    for task in &tasks {
        let name = task.path.to_string_lossy().to_lowercase();
        if name.ends_with(".jar") && name.contains("natives-windows") {
            extract_native(&task.path, &natives_dir)?;
        }
    }

    if index
        .get("virtual")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        progress.set_stage("整理旧版资源");
        materialize_indexed_assets(
            game_directory,
            &index,
            &game_directory.join("assets").join("virtual").join(asset_id),
        )?;
    }
    if index
        .get("map_to_resources")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        progress.set_stage("整理旧版资源");
        materialize_indexed_assets(game_directory, &index, &game_directory.join("resources"))?;
    }

    if let (Some(component), Some(required_major)) = (
        json.pointer("/javaVersion/component")
            .and_then(Value::as_str),
        json.pointer("/javaVersion/majorVersion")
            .and_then(Value::as_u64)
            .map(|major| major as u32),
    ) && !crate::game::has_compatible_java(game_directory, required_major)
    {
        install_java_runtime(game_directory, component, required_major, sender)?;
    }

    progress.set_stage("安装完成");
    Ok(total_size)
}

fn install_java_runtime(
    game_directory: &Path,
    component: &str,
    required_major: u32,
    sender: &mpsc::Sender<InstallUpdate>,
) -> Result<(), String> {
    validate_path_component(component)?;
    let _ = sender.send(InstallUpdate::running("检查 Java 运行时", 0, 0, 0, 0));
    let all = java_runtime_catalog()?;
    let platform = java_runtime_platform();
    let candidates = all
        .get(platform)
        .and_then(|value| value.get(component))
        .and_then(Value::as_array)
        .ok_or_else(|| format!("Mojang Java 运行时清单中没有 {component}（{platform}）"))?;
    let selected = candidates
        .iter()
        .find(|entry| {
            entry
                .pointer("/version/name")
                .and_then(Value::as_str)
                .and_then(|version| version.split('.').next())
                .and_then(|major| major.parse::<u32>().ok())
                == Some(required_major)
        })
        .or_else(|| candidates.first())
        .ok_or_else(|| format!("{component} 没有可用的 Windows 运行时"))?;
    let manifest_url = selected
        .pointer("/manifest/url")
        .and_then(Value::as_str)
        .ok_or_else(|| "Java 运行时缺少清单地址".to_owned())?;
    ensure_allowed_url(manifest_url)?;
    let manifest_sha1 = selected
        .pointer("/manifest/sha1")
        .and_then(Value::as_str)
        .ok_or_else(|| "Java 运行时缺少清单 SHA1".to_owned())?;
    if !is_sha1(manifest_sha1) {
        return Err("Java 运行时清单 SHA1 无效".to_owned());
    }
    let cache = cache_directory();
    let manifest_path = cache.join(format!("java-runtime-{component}-{manifest_sha1}.json"));
    download_small_file(manifest_url, &manifest_path, Some(manifest_sha1))?;
    let manifest: Value = serde_json::from_str(
        &fs::read_to_string(&manifest_path)
            .map_err(|error| format!("读取 Java 运行时清单失败：{error}"))?,
    )
    .map_err(|error| format!("Java 运行时清单 JSON 无效：{error}"))?;
    let runtime_root = game_directory
        .join("runtime")
        .join(component)
        .join(platform);
    fs::create_dir_all(&runtime_root)
        .map_err(|error| format!("无法创建 Java 运行时目录：{error}"))?;
    let mut tasks = Vec::new();
    let files = manifest
        .get("files")
        .and_then(Value::as_object)
        .ok_or_else(|| "Java 运行时清单缺少 files".to_owned())?;
    for (relative, entry) in files {
        if entry.get("type").and_then(Value::as_str) != Some("file") {
            continue;
        }
        let Some(raw) = entry.pointer("/downloads/raw") else {
            continue;
        };
        let Some(url) = raw.get("url").and_then(Value::as_str) else {
            continue;
        };
        ensure_allowed_url(url)?;
        let sha1 = raw
            .get("sha1")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("Java 运行时文件缺少 SHA1：{relative}"))?;
        if !is_sha1(sha1) {
            return Err(format!("Java 运行时文件 SHA1 无效：{relative}"));
        }
        tasks.push(DownloadTask {
            url: url.to_owned(),
            path: safe_join(&runtime_root, relative)?,
            sha1: Some(sha1.to_owned()),
            size: raw.get("size").and_then(Value::as_u64).unwrap_or(0),
        });
    }
    deduplicate_tasks(&mut tasks);
    let pending: Vec<DownloadTask> = tasks
        .iter()
        .filter(|task| !download_is_complete(task))
        .cloned()
        .collect();
    let total_size: u64 = pending.iter().map(|task| task.size).sum();
    let progress = Arc::new(Progress {
        sender: sender.clone(),
        completed_bytes: AtomicU64::new(0),
        total_bytes: total_size,
        files_done: AtomicUsize::new(0),
        files_total: pending.len(),
        stage: Mutex::new("下载 Java 运行时".to_owned()),
        last_report: Mutex::new(0),
        started_at: Instant::now(),
    });
    progress.send(true, "");
    download_tasks(game_directory, &pending, progress)?;
    if crate::game::has_compatible_java(game_directory, required_major) {
        Ok(())
    } else {
        Err(format!(
            "Java {required_major} 运行时安装后仍不可用，请检查运行时清单"
        ))
    }
}

pub fn java_runtime_manifest_report(component: &str) -> Result<String, String> {
    validate_path_component(component)?;
    let catalog = java_runtime_catalog()?;
    let platform = java_runtime_platform();
    let entries = catalog
        .get(platform)
        .and_then(|value| value.get(component))
        .and_then(Value::as_array)
        .ok_or_else(|| format!("清单中没有 {component}（{platform}）"))?;
    let versions = entries
        .iter()
        .filter_map(|entry| entry.pointer("/version/name").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join(", ");
    let selected = entries
        .first()
        .ok_or_else(|| format!("{component} 没有可用运行时"))?;
    let url = selected
        .pointer("/manifest/url")
        .and_then(Value::as_str)
        .ok_or_else(|| "运行时清单缺少地址".to_owned())?;
    let sha1 = selected
        .pointer("/manifest/sha1")
        .and_then(Value::as_str)
        .ok_or_else(|| "运行时清单缺少 SHA1".to_owned())?;
    let path = cache_directory().join(format!("java-runtime-{component}-{sha1}.json"));
    download_small_file(url, &path, Some(sha1))?;
    let manifest: Value = serde_json::from_str(
        &fs::read_to_string(path).map_err(|error| format!("读取运行时清单失败：{error}"))?,
    )
    .map_err(|error| format!("运行时清单 JSON 无效：{error}"))?;
    let file_count = manifest
        .get("files")
        .and_then(Value::as_object)
        .map(|files| {
            files
                .values()
                .filter(|entry| entry.get("type").and_then(Value::as_str) == Some("file"))
                .count()
        })
        .unwrap_or(0);
    Ok(format!(
        "{component} ({platform}): {versions}; {file_count} 个运行时文件"
    ))
}

pub fn install_java_runtime_for_report(
    game_directory: &Path,
    component: &str,
    required_major: u32,
) -> Result<(), String> {
    let (sender, _receiver) = mpsc::channel();
    install_java_runtime(game_directory, component, required_major, &sender)
}

fn java_runtime_catalog() -> Result<Value, String> {
    let cache = cache_directory();
    let path = cache.join("java-runtime-all.json");
    let etag_path = cache.join("java-runtime-all.etag");
    let body = fetch_cached_text(
        JAVA_RUNTIME_MANIFEST_URL,
        &path,
        &etag_path,
        "Java 运行时清单",
    )?;
    serde_json::from_str(&body).map_err(|error| format!("Java 运行时清单 JSON 无效：{error}"))
}

fn java_runtime_platform() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "windows-arm64",
        "x86" => "windows-x86",
        _ => "windows-x64",
    }
}

fn parse_manifest(body: &str) -> Result<Vec<OfficialVersion>, String> {
    let value: Value =
        serde_json::from_str(body).map_err(|error| format!("版本清单 JSON 无效：{error}"))?;
    let versions = value
        .get("versions")
        .and_then(Value::as_array)
        .ok_or_else(|| "版本清单缺少 versions".to_owned())?;
    Ok(versions
        .iter()
        .filter_map(|version| {
            Some(OfficialVersion {
                id: version.get("id")?.as_str()?.to_owned(),
                kind: version.get("type")?.as_str()?.to_owned(),
                url: version.get("url")?.as_str()?.to_owned(),
                sha1: version.get("sha1")?.as_str()?.to_owned(),
            })
        })
        .collect())
}

fn add_library_task(
    tasks: &mut Vec<DownloadTask>,
    artifact: &Value,
    game_directory: &Path,
) -> Result<(), String> {
    let Some(path) = artifact.get("path").and_then(Value::as_str) else {
        return Ok(());
    };
    let Some(url) = artifact.get("url").and_then(Value::as_str) else {
        return Ok(());
    };
    let sha1 = artifact
        .get("sha1")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("游戏库缺少 SHA1：{path}"))?;
    if !is_sha1(sha1) {
        return Err(format!("游戏库 SHA1 无效：{path}"));
    }
    ensure_allowed_url(url)?;
    tasks.push(DownloadTask {
        url: url.to_owned(),
        path: safe_join(&game_directory.join("libraries"), path)?,
        sha1: Some(sha1.to_owned()),
        size: artifact.get("size").and_then(Value::as_u64).unwrap_or(0),
    });
    Ok(())
}

fn add_download_task(
    tasks: &mut Vec<DownloadTask>,
    download: &Value,
    game_directory: &Path,
    version_id: &str,
) -> Result<(), String> {
    let Some(url) = download.get("url").and_then(Value::as_str) else {
        return Ok(());
    };
    let sha1 = download
        .get("sha1")
        .and_then(Value::as_str)
        .ok_or_else(|| "客户端文件缺少 SHA1".to_owned())?;
    if !is_sha1(sha1) {
        return Err("客户端文件 SHA1 无效".to_owned());
    }
    ensure_allowed_url(url)?;
    tasks.push(DownloadTask {
        url: url.to_owned(),
        path: game_directory
            .join("versions")
            .join(version_id)
            .join(format!("{version_id}.jar")),
        sha1: Some(sha1.to_owned()),
        size: download.get("size").and_then(Value::as_u64).unwrap_or(0),
    });
    Ok(())
}

fn deduplicate_tasks(tasks: &mut Vec<DownloadTask>) {
    let mut seen = HashSet::new();
    tasks.retain(|task| seen.insert(task.path.clone()));
}

fn download_is_complete(task: &DownloadTask) -> bool {
    task.path.is_file()
        && fs::metadata(&task.path)
            .is_ok_and(|metadata| task.size == 0 || metadata.len() == task.size)
}

fn download_tasks(
    _game_directory: &Path,
    tasks: &[DownloadTask],
    progress: Arc<Progress>,
) -> Result<(), String> {
    if tasks.is_empty() {
        return Ok(());
    }
    let cursor = Arc::new(AtomicUsize::new(0));
    let failed = Arc::new(AtomicBool::new(false));
    let error = Arc::new(Mutex::new(None));
    thread::scope(|scope| {
        for _ in 0..WORKERS.min(tasks.len()) {
            let cursor = cursor.clone();
            let failed = failed.clone();
            let error = error.clone();
            let progress = progress.clone();
            scope.spawn(move || {
                let agent = agent();
                while !failed.load(Ordering::Relaxed) {
                    let index = cursor.fetch_add(1, Ordering::Relaxed);
                    let Some(task) = tasks.get(index) else {
                        break;
                    };
                    if let Err(download_error) = download_task(&agent, task, &progress) {
                        failed.store(true, Ordering::Relaxed);
                        *error.lock().unwrap() = Some(download_error);
                        break;
                    }
                    progress.files_done.fetch_add(1, Ordering::Relaxed);
                    progress.send(true, "");
                }
            });
        }
    });
    if let Some(error) = error.lock().unwrap().clone() {
        return Err(error);
    }
    Ok(())
}

fn download_task(
    agent: &ureq::Agent,
    task: &DownloadTask,
    progress: &Progress,
) -> Result<(), String> {
    if download_is_complete(task) {
        return Ok(());
    }
    if let Some(parent) = task.path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建下载目录失败：{error}"))?;
    }
    let part = part_path(&task.path);
    let _ = fs::remove_file(&part);

    let mut last_error = String::new();
    for attempt in 0..3 {
        let mut counted_bytes = 0_u64;
        let result = (|| -> Result<(), String> {
            let response = agent
                .get(&task.url)
                .set("User-Agent", USER_AGENT)
                .call()
                .map_err(|error| error.to_string())?;
            let expected_size = response
                .header("content-length")
                .and_then(|value| value.parse::<u64>().ok());
            if task.size > 0 && expected_size.is_some_and(|size| size != task.size) {
                return Err("服务器返回的文件大小不正确".to_owned());
            }
            let mut reader = response.into_reader();
            let mut file =
                File::create(&part).map_err(|error| format!("创建临时下载文件失败：{error}"))?;
            let mut buffer = [0_u8; 64 * 1024];
            loop {
                let read = reader
                    .read(&mut buffer)
                    .map_err(|error| format!("读取下载数据失败：{error}"))?;
                if read == 0 {
                    break;
                }
                file.write_all(&buffer[..read])
                    .map_err(|error| format!("写入下载文件失败：{error}"))?;
                counted_bytes = counted_bytes.saturating_add(read as u64);
                progress
                    .completed_bytes
                    .fetch_add(read as u64, Ordering::Relaxed);
                progress.send(false, "");
            }
            file.flush()
                .map_err(|error| format!("刷新下载文件失败：{error}"))?;
            if task.size > 0
                && fs::metadata(&part)
                    .map_err(|error| format!("读取下载文件信息失败：{error}"))?
                    .len()
                    != task.size
            {
                return Err("下载文件大小校验失败".to_owned());
            }
            if let Some(expected) = &task.sha1 {
                let actual = sha1_file(&part)?;
                if !actual.eq_ignore_ascii_case(expected) {
                    return Err("SHA1 校验失败".to_owned());
                }
            }
            replace_file(&part, &task.path)
        })();
        match result {
            Ok(()) => return Ok(()),
            Err(error) => {
                progress.completed_bytes.fetch_sub(
                    counted_bytes.min(progress.completed_bytes.load(Ordering::Relaxed)),
                    Ordering::Relaxed,
                );
                last_error = error;
            }
        }
        let _ = fs::remove_file(&part);
        if attempt < 2 {
            thread::sleep(backoff(attempt));
        }
    }
    Err(format!("下载失败 {}：{last_error}", task.url))
}

fn download_small_file(url: &str, path: &Path, sha1: Option<&str>) -> Result<(), String> {
    if path.is_file()
        && sha1.is_none_or(|expected| {
            sha1_file(path).is_ok_and(|actual| actual.eq_ignore_ascii_case(expected))
        })
    {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("无法创建下载目录：{error}"))?;
    }
    let part = part_path(path);
    let agent = agent();
    let mut last_error = String::new();
    for attempt in 0..3 {
        let result = (|| -> Result<(), String> {
            let response = agent
                .get(url)
                .set("User-Agent", USER_AGENT)
                .call()
                .map_err(|error| error.to_string())?;
            let mut reader = response.into_reader();
            let mut bytes = Vec::new();
            reader
                .read_to_end(&mut bytes)
                .map_err(|error| format!("读取下载数据失败：{error}"))?;
            if let Some(expected) = sha1 {
                let actual = Sha1::from(&bytes).digest().to_string();
                if !actual.eq_ignore_ascii_case(expected) {
                    return Err("SHA1 校验失败".to_owned());
                }
            }
            let mut file =
                File::create(&part).map_err(|error| format!("创建临时文件失败：{error}"))?;
            file.write_all(&bytes)
                .map_err(|error| format!("写入临时文件失败：{error}"))?;
            file.flush()
                .map_err(|error| format!("刷新临时文件失败：{error}"))?;
            replace_file(&part, path)
        })();
        match result {
            Ok(()) => return Ok(()),
            Err(error) => last_error = error,
        }
        let _ = fs::remove_file(&part);
        if attempt < 2 {
            thread::sleep(backoff(attempt));
        }
    }
    Err(format!("下载失败 {url}：{last_error}"))
}

fn download_asset_index(url: &str, path: &Path, sha1: &str) -> Result<(), String> {
    let mut etag_name = path.as_os_str().to_os_string();
    etag_name.push(".etag");
    let etag_path = PathBuf::from(etag_name);
    let body = fetch_cached_text(url, path, &etag_path, "资源索引")?;
    let actual = Sha1::from(body.as_bytes()).digest().to_string();
    if actual.eq_ignore_ascii_case(sha1) {
        return Ok(());
    }
    let _ = fs::remove_file(etag_path);
    download_small_file(url, path, Some(sha1))
}

fn backoff(attempt: usize) -> Duration {
    Duration::from_millis(400_u64.saturating_mul(1_u64 << attempt))
}

fn part_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".part");
    PathBuf::from(name)
}

fn replace_file(part: &Path, target: &Path) -> Result<(), String> {
    if target.exists() {
        fs::remove_file(target).map_err(|error| format!("替换旧下载文件失败：{error}"))?;
    }
    fs::rename(part, target).map_err(|error| format!("提交下载文件失败：{error}"))
}

fn extract_native(path: &Path, natives_dir: &Path) -> Result<(), String> {
    let file = File::open(path).map_err(|error| format!("无法打开 natives：{error}"))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("无法读取 natives：{error}"))?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("读取 natives 条目失败：{error}"))?;
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        if !relative
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"))
        {
            continue;
        }
        let output = natives_dir.join(relative.file_name().unwrap_or(relative.as_os_str()));
        let mut target =
            File::create(output).map_err(|error| format!("写入 natives 失败：{error}"))?;
        io::copy(&mut entry, &mut target).map_err(|error| format!("解压 natives 失败：{error}"))?;
    }
    Ok(())
}

fn materialize_indexed_assets(
    game_directory: &Path,
    index: &Value,
    target_root: &Path,
) -> Result<(), String> {
    let Some(objects) = index.get("objects").and_then(Value::as_object) else {
        return Ok(());
    };
    for (name, object) in objects {
        let Some(hash) = object.get("hash").and_then(Value::as_str) else {
            continue;
        };
        if hash.len() < 2 {
            continue;
        }
        let source = game_directory
            .join("assets")
            .join("objects")
            .join(&hash[..2])
            .join(hash);
        let target = safe_join(target_root, name)?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).ok();
        }
        if source.is_file() {
            let _ = fs::copy(source, target);
        }
    }
    Ok(())
}

fn safe_join(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(format!("拒绝不安全的下载路径：{}", relative.display()));
    }
    Ok(root.join(relative))
}

fn validate_path_component(value: &str) -> Result<(), String> {
    let path = Path::new(value);
    let mut components = path.components();
    let valid = matches!(components.next(), Some(Component::Normal(_)))
        && components.next().is_none()
        && !value.trim().is_empty()
        && value != "."
        && value != ".."
        && !value.contains(':');
    if valid {
        Ok(())
    } else {
        Err(format!("拒绝不安全的版本或资源标识：{value}"))
    }
}

fn is_sha1(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn ensure_allowed_url(url: &str) -> Result<(), String> {
    // Piston Data is the object CDN linked by Mojang's Java Runtime Manifest.
    const HOSTS: [&str; 5] = [
        "https://piston-meta.mojang.com/",
        "https://piston-data.mojang.com/",
        "https://launchermeta.mojang.com/",
        "https://libraries.minecraft.net/",
        "https://resources.download.minecraft.net/",
    ];
    if HOSTS.iter().any(|host| url.starts_with(host)) {
        Ok(())
    } else {
        Err(format!("拒绝非 Mojang 官方下载地址：{url}"))
    }
}

fn rules_allow(value: &Value) -> bool {
    let Some(rules) = value.get("rules").and_then(Value::as_array) else {
        return true;
    };
    let mut allowed = false;
    for rule in rules {
        if rule_matches(rule) {
            allowed = rule.get("action").and_then(Value::as_str) == Some("allow");
        }
    }
    allowed
}

fn rule_matches(rule: &Value) -> bool {
    if let Some(os) = rule.get("os") {
        if let Some(name) = os.get("name").and_then(Value::as_str)
            && name != "windows"
        {
            return false;
        }
        if let Some(arch) = os.get("arch").and_then(Value::as_str)
            && arch != std::env::consts::ARCH
        {
            return false;
        }
    }
    true
}

fn sha1_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| format!("读取校验文件失败：{error}"))?;
    let mut sha1 = Sha1::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("读取校验文件失败：{error}"))?;
        if read == 0 {
            break;
        }
        sha1.update(&buffer[..read]);
    }
    Ok(sha1.digest().to_string())
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(Duration::from_secs(60))
        .build()
}

fn cache_directory() -> PathBuf {
    brand::data_directory().join("cache").join("mojang")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_mojang_download_hosts() {
        for url in [
            "https://piston-meta.mojang.com/v1/packages/a/manifest.json",
            "https://piston-data.mojang.com/v1/objects/a/file",
            "https://launchermeta.mojang.com/v1/products/java-runtime/a/all.json",
            "https://libraries.minecraft.net/org/example/a.jar",
            "https://resources.download.minecraft.net/ab/abcdef",
        ] {
            assert!(ensure_allowed_url(url).is_ok(), "{url}");
        }
        for url in [
            "http://piston-meta.mojang.com/file",
            "https://piston-meta.mojang.com.example.org/file",
            "https://example.org/file",
        ] {
            assert!(ensure_allowed_url(url).is_err(), "{url}");
        }
    }

    #[test]
    fn rejects_unsafe_download_paths() {
        let root = Path::new("C:\\game");
        assert!(safe_join(root, "versions/1.21/client.jar").is_ok());
        for path in [
            "C:\\Windows\\file",
            "../outside",
            "versions/../../outside",
            "versions\\..\\outside",
        ] {
            assert!(safe_join(root, path).is_err(), "{path}");
        }
        for component in ["../version", "C:", "a/b", "..", ".", ""] {
            assert!(validate_path_component(component).is_err(), "{component}");
        }
        assert!(validate_path_component("26.3").is_ok());
        assert!(validate_path_component("26.3-snapshot-8").is_ok());
    }

    #[test]
    fn parses_piston_meta_versions() {
        let body = r#"{
            "latest": {"release": "1.21.11", "snapshot": "26.3-snapshot-8"},
            "versions": [{
                "id": "1.21.11",
                "type": "release",
                "url": "https://piston-meta.mojang.com/v1/packages/a/1.21.11.json",
                "sha1": "0123456789abcdef0123456789abcdef01234567"
            }]
        }"#;
        let versions = parse_manifest(body).expect("manifest");
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].id, "1.21.11");
        assert_eq!(versions[0].kind, "release");
    }

    #[test]
    fn recognizes_only_complete_sha1_values() {
        assert!(is_sha1("0123456789abcdef0123456789abcdef01234567"));
        assert!(!is_sha1("012345"));
        assert!(!is_sha1("z123456789abcdef0123456789abcdef01234567"));
    }
}
