use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};
use sha1_smol::Sha1;
use zip::ZipArchive;

use crate::loader::LoaderKind;

const USER_AGENT: &str = "BlockLauncher/0.1.0 (+Rust native launcher)";
const MCIM_HOST: &str = "https://mod.mcimirror.top";

#[derive(Clone, Debug)]
pub struct ModpackImportUpdate {
    pub stage: String,
    pub message: String,
    pub finished: bool,
    pub success: bool,
    pub profile: Option<String>,
}

struct PackFile {
    path: PathBuf,
    urls: Vec<String>,
    sha1: String,
}

struct PackManifest {
    name: String,
    minecraft: String,
    loader: Option<(LoaderKind, String)>,
    files: Vec<PackFile>,
    archive_prefix: String,
}

pub fn pick_modpack_file() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let script = "[Console]::OutputEncoding=[Text.Encoding]::UTF8; Add-Type -AssemblyName System.Windows.Forms; $d=New-Object System.Windows.Forms.OpenFileDialog; $d.Filter='Minecraft 整合包 (*.mrpack;*.zip)|*.mrpack;*.zip|所有文件 (*.*)|*.*'; $d.Multiselect=$false; if($d.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK){[Console]::Out.Write($d.FileName)}";
        let mut command = std::process::Command::new("powershell.exe");
        command
            .args(["-NoProfile", "-STA", "-Command", script])
            .creation_flags(0x0800_0000);
        let output = command.output().ok()?;
        let selected = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        (!selected.is_empty()).then(|| PathBuf::from(selected))
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

pub fn start_import(
    game_directory: PathBuf,
    archive_path: PathBuf,
) -> mpsc::Receiver<ModpackImportUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = import_modpack(&game_directory, &archive_path, &sender);
        let (success, profile, message) = match result {
            Ok(profile) => (true, Some(profile), "整合包导入完成".to_owned()),
            Err(error) => (false, None, error),
        };
        let _ = sender.send(ModpackImportUpdate {
            stage: if success {
                "导入完成".to_owned()
            } else {
                "导入失败".to_owned()
            },
            message,
            finished: true,
            success,
            profile,
        });
    });
    receiver
}

fn import_modpack(
    game_directory: &Path,
    archive_path: &Path,
    sender: &mpsc::Sender<ModpackImportUpdate>,
) -> Result<String, String> {
    send_stage(sender, "读取整合包", archive_path.display().to_string());
    let archive_file =
        File::open(archive_path).map_err(|error| format!("无法打开整合包：{error}"))?;
    let mut archive =
        ZipArchive::new(archive_file).map_err(|error| format!("无法读取整合包：{error}"))?;
    let manifest = read_manifest(&mut archive, archive_path)?;

    send_stage(
        sender,
        "准备 Minecraft",
        format!("Minecraft {}", manifest.minecraft),
    );
    ensure_minecraft(game_directory, &manifest.minecraft, sender)?;

    let loader_profile = if let Some((loader, loader_version)) = manifest.loader.as_ref() {
        send_stage(
            sender,
            "安装加载器",
            format!("{} {}", loader.label(), loader_version),
        );
        install_loader(
            game_directory,
            &manifest.minecraft,
            *loader,
            loader_version,
            sender,
        )?
    } else {
        manifest.minecraft.clone()
    };

    let loader_suffix = manifest
        .loader
        .as_ref()
        .map(|(loader, _)| format!("-{}", loader.label()))
        .unwrap_or_default();
    let profile = unique_profile_name(
        game_directory,
        &format!("{}{loader_suffix}", sanitize_name(&manifest.name)),
    );
    let version_directory = game_directory.join("versions").join(&profile);
    fs::create_dir_all(&version_directory)
        .map_err(|error| format!("无法创建整合包版本目录：{error}"))?;

    send_stage(sender, "解压整合包", "正在写入配置与覆盖文件".to_owned());
    extract_overrides(&mut archive, &manifest.archive_prefix, &version_directory)?;

    send_stage(
        sender,
        "下载整合包文件",
        format!("共 {} 个文件", manifest.files.len()),
    );
    download_files(&manifest.files, &version_directory, sender)?;

    send_stage(sender, "生成版本", profile.clone());
    let profile_json = json!({
        "id": profile,
        "inheritsFrom": loader_profile,
        "type": "release",
        "time": "1970-01-01T00:00:00Z",
        "releaseTime": "1970-01-01T00:00:00Z"
    });
    let profile_path = version_directory.join(format!("{profile}.json"));
    fs::write(
        profile_path,
        serde_json::to_vec_pretty(&profile_json)
            .map_err(|error| format!("生成版本配置失败：{error}"))?,
    )
    .map_err(|error| format!("写入版本配置失败：{error}"))?;
    Ok(profile)
}

fn read_manifest(
    archive: &mut ZipArchive<File>,
    archive_path: &Path,
) -> Result<PackManifest, String> {
    let mut index_path = None;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| format!("读取压缩包条目失败：{error}"))?;
        let name = entry.name().replace('\\', "/");
        if name.ends_with("modrinth.index.json") {
            index_path = Some(name);
            break;
        }
    }
    let index_path = index_path
        .ok_or_else(|| "当前仅支持 Modrinth 整合包（需要 modrinth.index.json）".to_owned())?;
    let mut contents = String::new();
    archive
        .by_name(&index_path)
        .map_err(|error| format!("读取整合包索引失败：{error}"))?
        .read_to_string(&mut contents)
        .map_err(|error| format!("解析整合包索引失败：{error}"))?;
    let index: Value =
        serde_json::from_str(&contents).map_err(|error| format!("整合包索引格式无效：{error}"))?;
    let minecraft = index
        .pointer("/dependencies/minecraft")
        .and_then(Value::as_str)
        .ok_or_else(|| "整合包未指定 Minecraft 版本".to_owned())?
        .to_owned();
    let dependencies = index
        .get("dependencies")
        .and_then(Value::as_object)
        .ok_or_else(|| "整合包依赖信息无效".to_owned())?;
    let mut loader = None;
    for (name, value) in dependencies {
        let Some(version) = value.as_str() else {
            continue;
        };
        let kind = match name.to_ascii_lowercase().as_str() {
            "fabric-loader" => Some(LoaderKind::Fabric),
            "forge" => Some(LoaderKind::Forge),
            "neoforge" | "neo-forge" => Some(LoaderKind::NeoForge),
            "quilt-loader" => return Err("暂不支持 Quilt 整合包".to_owned()),
            _ => None,
        };
        if let Some(kind) = kind {
            loader = Some((kind, version.to_owned()));
            break;
        }
    }
    let name = index
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            archive_path
                .file_stem()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "整合包".to_owned());
    let mut files = Vec::new();
    if let Some(entries) = index.get("files").and_then(Value::as_array) {
        for entry in entries {
            if entry.pointer("/env/client").and_then(Value::as_str) == Some("unsupported") {
                continue;
            }
            let Some(path) = entry
                .get("path")
                .and_then(Value::as_str)
                .and_then(safe_relative_path)
            else {
                return Err("整合包包含不安全的文件路径".to_owned());
            };
            let sha1 = entry
                .pointer("/hashes/sha1")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_ascii_lowercase();
            if sha1.len() != 40 {
                return Err(format!("{} 缺少有效 SHA1", path.display()));
            }
            let urls = entry
                .get("downloads")
                .and_then(Value::as_array)
                .map(|urls| {
                    urls.iter()
                        .filter_map(Value::as_str)
                        .filter(|url| url.starts_with("https://"))
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if urls.is_empty() {
                return Err(format!("{} 没有可用下载地址", path.display()));
            }
            files.push(PackFile { path, urls, sha1 });
        }
    }
    let archive_prefix = index_path
        .strip_suffix("modrinth.index.json")
        .unwrap_or_default()
        .to_owned();
    Ok(PackManifest {
        name,
        minecraft,
        loader,
        files,
        archive_prefix,
    })
}

fn ensure_minecraft(
    game_directory: &Path,
    minecraft: &str,
    sender: &mpsc::Sender<ModpackImportUpdate>,
) -> Result<(), String> {
    if crate::game::installation_complete(game_directory, minecraft, "原版") {
        return Ok(());
    }
    let receiver =
        crate::download::start_install_by_id(game_directory.to_owned(), minecraft.to_owned());
    let mut final_message = None;
    while let Ok(update) = receiver.recv() {
        send_stage(sender, "补全 Minecraft", update.stage.clone());
        if update.finished {
            final_message = Some((update.success, update.message));
            break;
        }
    }
    match final_message {
        Some((true, _)) => Ok(()),
        Some((false, message)) => Err(format!("Minecraft 补全失败：{message}")),
        None => Err("Minecraft 补全任务中断".to_owned()),
    }
}

fn install_loader(
    game_directory: &Path,
    minecraft: &str,
    loader: LoaderKind,
    loader_version: &str,
    sender: &mpsc::Sender<ModpackImportUpdate>,
) -> Result<String, String> {
    let receiver = crate::loader::start_install(
        game_directory.to_owned(),
        minecraft.to_owned(),
        loader,
        loader_version.to_owned(),
    );
    let mut final_update = None;
    while let Ok(update) = receiver.recv() {
        send_stage(sender, "安装加载器", update.stage.clone());
        if update.finished {
            final_update = Some(update);
            break;
        }
    }
    let update = final_update.ok_or_else(|| "加载器安装任务中断".to_owned())?;
    if !update.success {
        return Err(update.message);
    }
    update
        .profile_hint
        .ok_or_else(|| "加载器安装未返回版本配置".to_owned())
}

fn extract_overrides(
    archive: &mut ZipArchive<File>,
    archive_prefix: &str,
    version_directory: &Path,
) -> Result<(), String> {
    let prefixes = [
        format!("{archive_prefix}overrides/"),
        format!("{archive_prefix}client-overrides/"),
    ];
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("读取覆盖文件失败：{error}"))?;
        let entry_name = entry.name().replace('\\', "/");
        let Some(relative) = prefixes
            .iter()
            .find_map(|prefix| entry_name.strip_prefix(prefix))
        else {
            continue;
        };
        let Some(relative) = safe_relative_path(relative) else {
            return Err(format!("整合包含有越界覆盖路径：{}", entry.name()));
        };
        let target = version_directory.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(|error| format!("创建覆盖目录失败：{error}"))?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("创建覆盖文件目录失败：{error}"))?;
        }
        let mut file =
            File::create(&target).map_err(|error| format!("写入覆盖文件失败：{error}"))?;
        std::io::copy(&mut entry, &mut file)
            .map_err(|error| format!("解压覆盖文件失败：{error}"))?;
    }
    Ok(())
}

fn download_files(
    files: &[PackFile],
    version_directory: &Path,
    sender: &mpsc::Sender<ModpackImportUpdate>,
) -> Result<(), String> {
    for (index, file) in files.iter().enumerate() {
        let target = version_directory.join(&file.path);
        if target
            .is_file()
            .then(|| fs::read(&target).ok())
            .flatten()
            .is_some_and(|bytes| hash_matches(&bytes, &file.sha1))
        {
            continue;
        }
        send_stage(
            sender,
            "下载整合包文件",
            format!("{}/{} · {}", index + 1, files.len(), file.path.display()),
        );
        let bytes = download_urls(&file.urls)?;
        if !hash_matches(&bytes, &file.sha1) {
            return Err(format!("{} SHA1 校验失败", file.path.display()));
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("创建下载目录失败：{error}"))?;
        }
        let part = target.with_extension("part");
        fs::write(&part, bytes).map_err(|error| format!("写入下载文件失败：{error}"))?;
        if target.exists() {
            fs::remove_file(&target).map_err(|error| format!("替换旧文件失败：{error}"))?;
        }
        fs::rename(&part, &target).map_err(|error| format!("提交下载文件失败：{error}"))?;
    }
    Ok(())
}

fn download_urls(urls: &[String]) -> Result<Vec<u8>, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(8))
        .timeout_read(Duration::from_secs(45))
        .build();
    let mut last_error = String::new();
    for url in urls {
        let mirror = mirror_url(url);
        for candidate in mirror.iter().chain(std::iter::once(url)) {
            match agent.get(candidate).set("User-Agent", USER_AGENT).call() {
                Ok(response) => {
                    let mut bytes = Vec::new();
                    if response.into_reader().read_to_end(&mut bytes).is_ok() {
                        return Ok(bytes);
                    }
                }
                Err(error) => last_error = error.to_string(),
            }
        }
    }
    Err(format!("下载整合包文件失败：{last_error}"))
}

fn mirror_url(url: &str) -> Option<String> {
    url.strip_prefix("https://cdn.modrinth.com")
        .map(|path| format!("{MCIM_HOST}{path}"))
}

fn hash_matches(bytes: &[u8], expected: &str) -> bool {
    Sha1::from(bytes)
        .digest()
        .to_string()
        .eq_ignore_ascii_case(expected)
}

fn safe_relative_path(value: &str) -> Option<PathBuf> {
    let normalized = value.trim_start_matches(['/', '\\']).replace('\\', "/");
    let path = Path::new(&normalized);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return None;
    }
    Some(path.to_owned())
}

fn sanitize_name(value: &str) -> String {
    let mut name = value
        .chars()
        .map(|character| {
            if matches!(
                character,
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
            ) || character.is_control()
            {
                '_'
            } else {
                character
            }
        })
        .collect::<String>()
        .trim_matches([' ', '.'])
        .chars()
        .take(64)
        .collect::<String>();
    if name.is_empty() {
        name = "整合包".to_owned();
    }
    name
}

fn unique_profile_name(game_directory: &Path, base: &str) -> String {
    let versions = game_directory.join("versions");
    if !versions.join(base).exists() {
        return base.to_owned();
    }
    for index in 2..1000 {
        let candidate = format!("{base} ({index})");
        if !versions.join(&candidate).exists() {
            return candidate;
        }
    }
    format!("{base}-{}", std::process::id())
}

fn send_stage(sender: &mpsc::Sender<ModpackImportUpdate>, stage: &str, message: String) {
    let _ = sender.send(ModpackImportUpdate {
        stage: stage.to_owned(),
        message,
        finished: false,
        success: false,
        profile: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn rejects_escaping_pack_paths() {
        assert!(safe_relative_path("../outside.jar").is_none());
        assert!(safe_relative_path(r"C:\Windows\bad.dll").is_none());
        assert_eq!(
            safe_relative_path("/mods/example.jar"),
            Some(PathBuf::from("mods/example.jar"))
        );
    }

    #[test]
    fn sanitizes_imported_profile_names() {
        assert_eq!(sanitize_name("My:Pack?"), "My_Pack_");
        assert!(!sanitize_name("").is_empty());
    }

    #[test]
    fn maps_modrinth_downloads_to_mcim() {
        assert_eq!(
            mirror_url("https://cdn.modrinth.com/data/a/b.jar").as_deref(),
            Some("https://mod.mcimirror.top/data/a/b.jar")
        );
    }

    #[test]
    fn reads_modrinth_manifest_and_extracts_overrides() {
        let root = std::env::temp_dir().join(format!(
            "block-launcher-modpack-test-{}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let archive_path = root.join("pack.mrpack");
        let file = File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        writer.start_file("modrinth.index.json", options).unwrap();
        writer
            .write_all(
                br#"{
                  "name": "Import Test",
                  "dependencies": {
                    "minecraft": "1.21.1",
                    "fabric-loader": "0.16.9"
                  },
                  "files": [{
                    "path": "mods/example.jar",
                    "hashes": {"sha1": "0123456789abcdef0123456789abcdef01234567"},
                    "downloads": ["https://cdn.modrinth.com/data/example.jar"],
                    "env": {"client": "required"}
                  }]
                }"#,
            )
            .unwrap();
        writer
            .start_file("overrides/config/example.toml", options)
            .unwrap();
        writer.write_all(b"enabled=true").unwrap();
        writer.finish().unwrap();

        let mut archive = ZipArchive::new(File::open(&archive_path).unwrap()).unwrap();
        let manifest = read_manifest(&mut archive, &archive_path).unwrap();
        assert_eq!(manifest.name, "Import Test");
        assert_eq!(manifest.minecraft, "1.21.1");
        assert_eq!(
            manifest.loader,
            Some((LoaderKind::Fabric, "0.16.9".to_owned()))
        );
        assert_eq!(manifest.files.len(), 1);
        let output = root.join("instance");
        extract_overrides(&mut archive, "", &output).unwrap();
        assert_eq!(
            fs::read(output.join("config/example.toml")).unwrap(),
            b"enabled=true"
        );
        let _ = fs::remove_dir_all(root);
    }
}
