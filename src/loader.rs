use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use serde_json::Value;
use sha1_smol::Sha1;

const USER_AGENT: &str = "BlockLauncher/0.1.0 (+Rust native launcher)";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoaderKind {
    Fabric,
    Forge,
    NeoForge,
}

impl LoaderKind {
    pub const ALL: [Self; 3] = [Self::Fabric, Self::Forge, Self::NeoForge];

    pub fn label(self) -> &'static str {
        match self {
            Self::Fabric => "Fabric",
            Self::Forge => "Forge",
            Self::NeoForge => "NeoForge",
        }
    }
}

pub fn display_version(game_version: &str, version: &str) -> String {
    let prefix = format!("{game_version}-");
    version.strip_prefix(&prefix).unwrap_or(version).to_owned()
}

#[derive(Clone, Debug)]
pub struct LoaderInstallUpdate {
    pub stage: String,
    pub finished: bool,
    pub success: bool,
    pub profile_hint: Option<String>,
    pub message: String,
}

impl LoaderInstallUpdate {
    fn running(stage: impl Into<String>) -> Self {
        Self {
            stage: stage.into(),
            finished: false,
            success: false,
            profile_hint: None,
            message: String::new(),
        }
    }
}

pub fn start_fetch_versions(
    game_version: String,
    loader: LoaderKind,
) -> mpsc::Receiver<Result<Vec<String>, String>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(fetch_versions(&game_version, loader));
    });
    receiver
}

pub fn start_install(
    game_directory: PathBuf,
    game_version: String,
    loader: LoaderKind,
    loader_version: String,
) -> mpsc::Receiver<LoaderInstallUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = install_loader(
            &game_directory,
            &game_version,
            loader,
            &loader_version,
            &sender,
        );
        let (success, profile_hint, message) = match result {
            Ok(profile) => (
                true,
                Some(profile.clone()),
                format!("{} {} 安装完成", loader.label(), loader_version),
            ),
            Err(error) => (false, None, error),
        };
        let _ = sender.send(LoaderInstallUpdate {
            stage: if success {
                "加载器安装完成".to_owned()
            } else {
                "加载器安装失败".to_owned()
            },
            finished: true,
            success,
            profile_hint,
            message,
        });
    });
    receiver
}

fn fetch_versions(game_version: &str, loader: LoaderKind) -> Result<Vec<String>, String> {
    match loader {
        LoaderKind::Fabric => {
            let url = format!("https://meta.fabricmc.net/v2/versions/loader/{game_version}");
            let body = fetch_text(&url)?;
            let value: Value = serde_json::from_str(&body)
                .map_err(|error| format!("Fabric 版本列表格式无效：{error}"))?;
            let mut versions = value
                .as_array()
                .ok_or_else(|| "Fabric 版本列表无效".to_owned())?
                .iter()
                .filter_map(|entry| {
                    entry
                        .pointer("/loader/version")
                        .or_else(|| entry.get("version"))
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .collect::<Vec<_>>();
            versions.sort_by_key(|version| std::cmp::Reverse(version_key(version)));
            versions.dedup();
            if versions.is_empty() {
                Err(format!("Fabric 不支持 Minecraft {game_version}"))
            } else {
                Ok(versions)
            }
        }
        LoaderKind::Forge => fetch_forge_versions(game_version),
        LoaderKind::NeoForge => {
            let prefix = neoforge_prefix(game_version)?;
            let mut versions = fetch_neoforge_versions(game_version)?;
            let available_hint = versions
                .iter()
                .take(6)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            versions.retain(|version| version.starts_with(&prefix));
            versions.sort_by_key(|version| std::cmp::Reverse(version_key(version)));
            if versions.is_empty() {
                Err(format!(
                    "NeoForge 暂不支持 Minecraft {game_version}（当前可用：{available_hint}）"
                ))
            } else {
                Ok(versions)
            }
        }
    }
}

fn install_loader(
    game_directory: &Path,
    game_version: &str,
    loader: LoaderKind,
    loader_version: &str,
    sender: &mpsc::Sender<LoaderInstallUpdate>,
) -> Result<String, String> {
    match loader {
        LoaderKind::Fabric => install_fabric(game_directory, game_version, loader_version, sender),
        LoaderKind::Forge => {
            install_with_installer(game_directory, game_version, loader, loader_version, sender)
        }
        LoaderKind::NeoForge => {
            install_with_installer(game_directory, game_version, loader, loader_version, sender)
        }
    }
}

fn install_fabric(
    game_directory: &Path,
    game_version: &str,
    loader_version: &str,
    sender: &mpsc::Sender<LoaderInstallUpdate>,
) -> Result<String, String> {
    let _ = sender.send(LoaderInstallUpdate::running("获取 Fabric 配置"));
    let url = format!(
        "https://meta.fabricmc.net/v2/versions/loader/{game_version}/{loader_version}/profile/json"
    );
    let body = fetch_text(&url)?;
    let profile: Value =
        serde_json::from_str(&body).map_err(|error| format!("Fabric 配置格式无效：{error}"))?;
    let profile_id = profile
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("fabric-loader-{loader_version}-{game_version}"));
    validate_component(&profile_id)?;
    let libraries = profile
        .get("libraries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut tasks = Vec::new();
    for library in &libraries {
        let Some(name) = library.get("name").and_then(Value::as_str) else {
            continue;
        };
        let base_url = library
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or("https://maven.fabricmc.net/");
        let path = maven_path(name)?;
        let url = format!("{}/{}", base_url.trim_end_matches('/'), path);
        let target = game_directory.join("libraries").join(path);
        ensure_allowed_loader_url(&url)?;
        tasks.push((url, target));
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let completed = std::sync::atomic::AtomicUsize::new(0);
    let failure = std::sync::Mutex::new(None::<String>);
    let _ = sender.send(LoaderInstallUpdate::running(format!(
        "下载 Fabric 库 0/{}",
        tasks.len()
    )));
    thread::scope(|scope| {
        for _ in 0..4.min(tasks.len()) {
            scope.spawn(|| {
                loop {
                    if failure.lock().ok().is_some_and(|value| value.is_some()) {
                        break;
                    }
                    let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some((url, target)) = tasks.get(index) else {
                        break;
                    };
                    if let Err(error) = download_verified(url, target) {
                        if let Ok(mut failure) = failure.lock() {
                            *failure = Some(error);
                        }
                        break;
                    }
                    let done = completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    let _ = sender.send(LoaderInstallUpdate::running(format!(
                        "下载 Fabric 库 {done}/{}",
                        tasks.len()
                    )));
                }
            });
        }
    });
    if let Some(error) = failure.lock().unwrap().take() {
        return Err(error);
    }
    let version_dir = game_directory.join("versions").join(&profile_id);
    fs::create_dir_all(&version_dir)
        .map_err(|error| format!("无法创建 Fabric 版本目录：{error}"))?;
    let json_path = version_dir.join(format!("{profile_id}.json"));
    write_atomic(&json_path, body.as_bytes())?;
    Ok(profile_id)
}

fn install_with_installer(
    game_directory: &Path,
    game_version: &str,
    loader: LoaderKind,
    loader_version: &str,
    sender: &mpsc::Sender<LoaderInstallUpdate>,
) -> Result<String, String> {
    let legacy_neoforge = loader == LoaderKind::NeoForge && game_version == "1.20.1";
    let neoforge_artifact = if legacy_neoforge { "forge" } else { "neoforge" };
    let neoforge_version = if legacy_neoforge && !loader_version.starts_with("1.20.1-") {
        format!("1.20.1-{loader_version}")
    } else {
        loader_version.to_owned()
    };
    let (url, profile_hint) = match loader {
        LoaderKind::Forge => (
            format!(
                "https://maven.minecraftforge.net/net/minecraftforge/forge/{loader_version}/forge-{loader_version}-installer.jar"
            ),
            format!("{game_version}-forge-{}", forge_loader_part(loader_version)),
        ),
        LoaderKind::NeoForge => {
            let display_version = display_version(game_version, loader_version);
            (
                format!(
                    "https://maven.neoforged.net/releases/net/neoforged/{neoforge_artifact}/{neoforge_version}/{neoforge_artifact}-{neoforge_version}-installer.jar"
                ),
                format!("neoforge-{display_version}"),
            )
        }
        LoaderKind::Fabric => unreachable!(),
    };
    ensure_allowed_loader_url(&url)?;
    let installer = game_directory
        .join("downloads")
        .join(format!("{}-installer.jar", loader.label().to_lowercase()));
    let _ = sender.send(LoaderInstallUpdate::running(format!(
        "下载 {} 安装器",
        loader.label()
    )));
    let fallback_urls = match loader {
        LoaderKind::Forge => vec![format!(
            "https://bmclapi2.bangbang93.com/maven/net/minecraftforge/forge/{loader_version}/forge-{loader_version}-installer.jar"
        )],
        LoaderKind::NeoForge => vec![
            format!(
                "https://bmclapi2.bangbang93.com/maven/net/neoforged/{neoforge_artifact}/{neoforge_version}/{neoforge_artifact}-{neoforge_version}-installer.jar"
            ),
            format!(
                "https://maven.aliyun.com/repository/releases/net/neoforged/{neoforge_artifact}/{neoforge_version}/{neoforge_artifact}-{neoforge_version}-installer.jar"
            ),
            format!(
                "https://maven.aliyun.com/repository/public/net/neoforged/{neoforge_artifact}/{neoforge_version}/{neoforge_artifact}-{neoforge_version}-installer.jar"
            ),
            format!(
                "https://repo.huaweicloud.com/repository/maven/net/neoforged/{neoforge_artifact}/{neoforge_version}/{neoforge_artifact}-{neoforge_version}-installer.jar"
            ),
            format!(
                "https://mirrors.cloud.tencent.com/nexus/repository/maven-public/net/neoforged/{neoforge_artifact}/{neoforge_version}/{neoforge_artifact}-{neoforge_version}-installer.jar"
            ),
        ],
        _ => Vec::new(),
    };
    let mut installed = download_verified(&url, &installer).is_ok();
    for fallback in fallback_urls {
        if installed {
            break;
        }
        installed = download_verified(&fallback, &installer).is_ok();
    }
    if !installed {
        return Err(format!("无法下载 {} 安装器", loader.label()));
    }
    let java = crate::game::java_for_loader_install(game_directory, game_version)
        .ok_or_else(|| format!("未找到可用于 {} 安装器的 Java", loader.label()))?;
    let _ = sender.send(LoaderInstallUpdate::running(format!(
        "运行 {} 安装器",
        loader.label()
    )));
    let mut command = Command::new(java);
    command
        .arg("-jar")
        .arg(&installer)
        .arg("--installClient")
        .arg(game_directory)
        .current_dir(game_directory)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let output = command
        .output()
        .map_err(|error| format!("无法运行 {} 安装器：{error}", loader.label()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = format!("{stdout}\n{stderr}");
        return Err(format!(
            "{} 安装器失败（{}）：{}",
            loader.label(),
            output.status,
            detail
                .chars()
                .rev()
                .take(600)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>()
        ));
    }
    let _ = fs::remove_file(installer);
    Ok(profile_hint)
}

fn download_verified(url: &str, target: &Path) -> Result<(), String> {
    ensure_allowed_loader_url(url)?;
    let mut last_error = String::new();
    for attempt in 0..3 {
        let result = (|| -> Result<(), String> {
            let sha_url = format!("{url}.sha1");
            let expected = fetch_text(&sha_url)?
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_owned();
            if expected.len() != 40 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(format!("SHA1 无效：{url}"));
            }
            let response = request_once(url)?;
            let mut bytes = Vec::new();
            response
                .into_reader()
                .read_to_end(&mut bytes)
                .map_err(|error| format!("读取下载文件失败：{error}"))?;
            let actual = Sha1::from(&bytes).digest().to_string();
            if !actual.eq_ignore_ascii_case(&expected) {
                return Err(format!("SHA1 校验失败：{url}"));
            }
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|error| format!("无法创建下载目录：{error}"))?;
            }
            write_atomic(target, &bytes)
        })();
        match result {
            Ok(()) => return Ok(()),
            Err(error) => last_error = error,
        }
        if attempt < 2 {
            thread::sleep(Duration::from_millis(500 * (1 << attempt)));
        }
    }
    Err(last_error)
}

fn fetch_text(url: &str) -> Result<String, String> {
    ensure_allowed_loader_url(url)?;
    request_with_retry(url)?
        .into_string()
        .map_err(|error| format!("读取网络响应失败：{error}"))
}

fn fetch_forge_versions(game_version: &str) -> Result<Vec<String>, String> {
    let bmcl_url = format!(
        "https://bmclapi2.bangbang93.com/forge/minecraft/{}",
        game_version.replace('-', "_")
    );
    if let Ok(body) = fetch_text(&bmcl_url)
        && let Ok(versions) = parse_bmcl_forge_versions(&body, game_version)
        && !versions.is_empty()
    {
        return Ok(versions);
    }
    let body =
        fetch_text("https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml")?;
    let mut versions = xml_versions(&body)
        .into_iter()
        .filter(|version| version.starts_with(&format!("{game_version}-")))
        .collect::<Vec<_>>();
    versions.sort_by_key(|version| std::cmp::Reverse(version_key(version)));
    versions.dedup();
    if versions.is_empty() {
        Err(format!("Forge 暂不支持 Minecraft {game_version}"))
    } else {
        Ok(versions)
    }
}

fn parse_bmcl_forge_versions(body: &str, game_version: &str) -> Result<Vec<String>, String> {
    let entries: Value = serde_json::from_str(body)
        .map_err(|error| format!("BMCL Forge 版本列表格式无效：{error}"))?;
    let mut versions = entries
        .as_array()
        .ok_or_else(|| "BMCL Forge 版本列表无效".to_owned())?
        .iter()
        .filter(|entry| entry.get("mcversion").and_then(Value::as_str) == Some(game_version))
        .filter(|entry| {
            entry
                .get("files")
                .and_then(Value::as_array)
                .is_some_and(|files| {
                    files.iter().any(|file| {
                        file.get("category").and_then(Value::as_str) == Some("installer")
                            && file.get("format").and_then(Value::as_str) == Some("jar")
                    })
                })
        })
        .filter_map(|entry| {
            let version = entry.get("version")?.as_str()?;
            let branch = entry
                .get("branch")
                .and_then(Value::as_str)
                .filter(|branch| !branch.is_empty());
            Some(match branch {
                Some(branch) => format!("{game_version}-{version}-{branch}"),
                None => format!("{game_version}-{version}"),
            })
        })
        .collect::<Vec<_>>();
    versions.sort_by_key(|version| std::cmp::Reverse(version_key(version)));
    versions.dedup();
    Ok(versions)
}

fn parse_bmcl_maven_directories(body: &str) -> Result<Vec<String>, String> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| format!("BMCL Maven 版本列表格式无效：{error}"))?;
    Ok(value
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(|| "BMCL Maven 版本列表缺少 files".to_owned())?
        .iter()
        .filter(|file| file.get("type").and_then(Value::as_str) == Some("DIRECTORY"))
        .filter_map(|file| file.get("name").and_then(Value::as_str))
        .map(str::to_owned)
        .collect())
}

fn fetch_neoforge_versions(game_version: &str) -> Result<Vec<String>, String> {
    let mut errors = Vec::new();
    let artifact = if game_version == "1.20.1" {
        "forge"
    } else {
        "neoforge"
    };
    if let Ok(body) = fetch_text(&format!(
        "https://bmclapi2.bangbang93.com/neoforge/meta/api/maven/details/releases/net/neoforged/{artifact}"
    )) && let Ok(versions) = parse_bmcl_maven_directories(&body)
        && !versions.is_empty()
    {
        return Ok(versions);
    }
    if let Ok(body) = fetch_text(&format!(
        "https://maven.neoforged.net/releases/net/neoforged/{artifact}/maven-metadata.xml"
    )) {
        let versions = xml_versions(&body);
        if !versions.is_empty() {
            return Ok(versions);
        }
    } else {
        errors.push("NeoForge Maven metadata");
    }
    if let Ok(body) = fetch_text(&format!(
        "https://repo1.maven.org/maven2/net/neoforged/{artifact}/maven-metadata.xml"
    )) {
        let versions = xml_versions(&body);
        if !versions.is_empty() {
            return Ok(versions);
        }
    } else {
        errors.push("Maven Central metadata");
    }
    for mirror in [
        format!(
            "https://maven.aliyun.com/repository/releases/net/neoforged/{artifact}/maven-metadata.xml"
        ),
        format!(
            "https://maven.aliyun.com/repository/public/net/neoforged/{artifact}/maven-metadata.xml"
        ),
        format!(
            "https://repo.huaweicloud.com/repository/maven/net/neoforged/{artifact}/maven-metadata.xml"
        ),
        format!(
            "https://mirrors.cloud.tencent.com/nexus/repository/maven-public/net/neoforged/{artifact}/maven-metadata.xml"
        ),
    ] {
        if let Ok(body) = fetch_text(&mirror) {
            let versions = xml_versions(&body);
            if !versions.is_empty() {
                return Ok(versions);
            }
        } else {
            errors.push("NeoForge Maven 镜像");
        }
    }
    if let Ok(body) =
        fetch_text("https://api.github.com/repos/neoforged/NeoForge/releases?per_page=100")
    {
        let value: Value = serde_json::from_str(&body)
            .map_err(|error| format!("NeoForge GitHub 版本列表格式无效：{error}"))?;
        let versions = value
            .as_array()
            .ok_or_else(|| "NeoForge GitHub 版本列表无效".to_owned())?
            .iter()
            .filter_map(|release| release.get("tag_name")?.as_str())
            .map(|tag| {
                tag.trim_start_matches('v')
                    .strip_prefix(&format!("{game_version}-"))
                    .unwrap_or(tag)
                    .to_owned()
            })
            .filter(|version| !version.is_empty())
            .collect::<Vec<_>>();
        if !versions.is_empty() {
            return Ok(versions);
        }
    } else {
        errors.push("NeoForge GitHub Releases");
    }
    if let Ok(body) =
        fetch_text("https://api.github.com/repos/neoforged/NeoForge/tags?per_page=100")
    {
        let value: Value = serde_json::from_str(&body)
            .map_err(|error| format!("NeoForge GitHub 标签格式无效：{error}"))?;
        let versions = value
            .as_array()
            .ok_or_else(|| "NeoForge GitHub 标签无效".to_owned())?
            .iter()
            .filter_map(|tag| tag.get("name")?.as_str())
            .map(|tag| {
                tag.trim_start_matches('v')
                    .strip_prefix(&format!("{game_version}-"))
                    .unwrap_or(tag)
                    .to_owned()
            })
            .filter(|version| !version.is_empty())
            .collect::<Vec<_>>();
        if !versions.is_empty() {
            return Ok(versions);
        }
    } else {
        errors.push("NeoForge GitHub Tags");
    }
    Err(format!("无法获取 NeoForge 版本列表：{}", errors.join("、")))
}

fn request_with_retry(url: &str) -> Result<ureq::Response, String> {
    let mut last_error = String::new();
    for attempt in 0..3 {
        match request_once(url) {
            Ok(response) => return Ok(response),
            Err(error) => last_error = error,
        }
        if attempt < 2 {
            thread::sleep(Duration::from_millis(350 * (1 << attempt)));
        }
    }
    Err(format!("请求失败 {url}：{last_error}"))
}

fn request_once(url: &str) -> Result<ureq::Response, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(6))
        .timeout_read(Duration::from_secs(20))
        .build();
    agent
        .get(url)
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(|error| error.to_string())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let part = path.with_extension("part");
    fs::write(&part, bytes).map_err(|error| format!("写入临时文件失败：{error}"))?;
    if path.exists() {
        fs::remove_file(path).map_err(|error| format!("替换旧文件失败：{error}"))?;
    }
    fs::rename(&part, path).map_err(|error| format!("提交文件失败：{error}"))
}

fn maven_path(name: &str) -> Result<String, String> {
    let (coordinates, extension) = name.split_once('@').unwrap_or((name, "jar"));
    let parts: Vec<&str> = coordinates.split(':').collect();
    if parts.len() < 3 {
        return Err(format!("无效的 Maven 坐标：{name}"));
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts
        .get(3)
        .map(|value| format!("-{value}"))
        .unwrap_or_default();
    Ok(format!(
        "{group}/{artifact}/{version}/{artifact}-{version}{classifier}.{extension}"
    ))
}

fn neoforge_prefix(game_version: &str) -> Result<String, String> {
    if game_version == "1.20.1" {
        return Ok("1.20.1-47.".to_owned());
    }
    let version = game_version
        .split('-')
        .next()
        .unwrap_or(game_version)
        .strip_prefix("1.")
        .unwrap_or_else(|| game_version.split('-').next().unwrap_or(game_version));
    let mut parts = version.split('.');
    let major = parts
        .next()
        .ok_or_else(|| "无效的 Minecraft 版本".to_owned())?;
    let minor = parts
        .next()
        .ok_or_else(|| "无效的 Minecraft 版本".to_owned())?;
    Ok(format!("{major}.{minor}."))
}

fn forge_loader_part(version: &str) -> &str {
    version
        .split_once('-')
        .map(|(_, loader)| loader)
        .unwrap_or(version)
}

fn validate_component(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.contains(['/', '\\'])
        || value.contains("..")
        || value.contains(':')
    {
        Err(format!("拒绝不安全的加载器版本标识：{value}"))
    } else {
        Ok(())
    }
}

fn xml_versions(xml: &str) -> Vec<String> {
    xml.split("<version>")
        .skip(1)
        .filter_map(|entry| entry.split("</version>").next().map(str::to_owned))
        .collect()
}

fn version_key(version: &str) -> Vec<u32> {
    version
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .collect()
}

fn ensure_allowed_loader_url(url: &str) -> Result<(), String> {
    const HOSTS: [&str; 11] = [
        "https://meta.fabricmc.net/",
        "https://maven.fabricmc.net/",
        "https://bmclapi2.bangbang93.com/",
        "https://maven.minecraftforge.net/",
        "https://maven.neoforged.net/",
        "https://repo1.maven.org/",
        "https://maven.aliyun.com/",
        "https://repo.huaweicloud.com/",
        "https://mirrors.cloud.tencent.com/",
        "https://api.github.com/",
        "https://libraries.minecraft.net/",
    ];
    if HOSTS.iter().any(|host| url.starts_with(host)) {
        Ok(())
    } else {
        Err(format!("拒绝非受信任加载器地址：{url}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_maven_paths() {
        assert_eq!(
            maven_path("org.ow2.asm:asm:9.7.1").unwrap(),
            "org/ow2/asm/asm/9.7.1/asm-9.7.1.jar"
        );
        assert_eq!(
            maven_path("net.fabricmc:sponge-mixin:0.15.4+mixin.0.8.7").unwrap(),
            "net/fabricmc/sponge-mixin/0.15.4+mixin.0.8.7/sponge-mixin-0.15.4+mixin.0.8.7.jar"
        );
    }

    #[test]
    fn parses_maven_metadata() {
        let xml = "<metadata><versioning><versions><version>1.21.1-52.0.1</version><version>1.21.1-52.0.2</version></versions></versioning></metadata>";
        assert_eq!(xml_versions(xml), vec!["1.21.1-52.0.1", "1.21.1-52.0.2"]);
    }

    #[test]
    fn filters_neoforge_versions() {
        assert_eq!(neoforge_prefix("1.21.1").unwrap(), "21.1.");
        assert_eq!(neoforge_prefix("1.21.11").unwrap(), "21.11.");
        assert_eq!(neoforge_prefix("26.3").unwrap(), "26.3.");
        assert_eq!(neoforge_prefix("26.3-snapshot-8").unwrap(), "26.3.");
        assert_eq!(neoforge_prefix("1.20.1").unwrap(), "1.20.1-47.");
        assert!(validate_component("../bad").is_err());
    }

    #[test]
    fn displays_loader_version_without_minecraft_prefix() {
        assert_eq!(display_version("1.21.1", "1.21.1-52.1.20"), "52.1.20");
        assert_eq!(display_version("1.20.1", "1.20.1-47.1.106"), "47.1.106");
        assert_eq!(display_version("1.21.1", "21.1.219"), "21.1.219");
    }

    #[test]
    fn keeps_only_installable_bmcl_forge_versions() {
        let body = r#"[
          {
            "mcversion": "1.21.1",
            "version": "52.1.20",
            "files": [
              {"category": "installer", "format": "jar"},
              {"category": "mdk", "format": "zip"}
            ]
          },
          {
            "mcversion": "1.21.1",
            "version": "51.0.0",
            "files": [
              {"category": "mdk", "format": "zip"}
            ]
          }
        ]"#;
        assert_eq!(
            parse_bmcl_forge_versions(body, "1.21.1").unwrap(),
            vec!["1.21.1-52.1.20"]
        );
    }

    #[test]
    fn parses_bmcl_neoforge_directories() {
        let body = r#"{
          "files": [
            {"name": "21.1.219", "type": "DIRECTORY"},
            {"name": "21.1.220-beta", "type": "DIRECTORY"},
            {"name": "maven-metadata.xml", "type": "FILE"}
          ]
        }"#;
        assert_eq!(
            parse_bmcl_maven_directories(body).unwrap(),
            vec!["21.1.219", "21.1.220-beta"]
        );
    }
}
