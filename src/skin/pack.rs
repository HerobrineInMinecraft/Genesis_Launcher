//! # `src/skin/pack.rs`
//!
//! **功能域：** Minecraft 皮肤解码、编辑、保存和资源包安装。
//!
//! 生成并启用本地皮肤资源包
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `LOCAL_SKIN_PACK_NAME`
//! - `DEFAULT_PLAYER_SKIN_NAMES`
//! - `install_local_skin_resource_pack`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// 提供 LOCAL皮肤包名称 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub(crate) const LOCAL_SKIN_PACK_NAME: &str = "Genesis Launcher Local Skin.zip";
/// 提供 默认玩家皮肤NAMES 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub(crate) const DEFAULT_PLAYER_SKIN_NAMES: &[&str] = &[
    "alex", "ari", "efe", "kai", "makena", "noor", "steve", "sunny", "zuri",
];

/// 实现 安装local皮肤资源包 相关的业务流程。
///
/// # 参数
/// - `pixels`：由调用方传入的pixels。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(crate) fn install_local_skin_resource_pack(
    data_directory: &Path,
    pixels: &[u8],
) -> Result<PathBuf, String> {
    let resourcepacks = data_directory.join("resourcepacks");
    fs::create_dir_all(&resourcepacks).map_err(|error| format!("无法创建资源包目录：{error}"))?;
    let pack_path = resourcepacks.join(LOCAL_SKIN_PACK_NAME);
    let image = image::RgbaImage::from_raw(64, 64, pixels.to_vec())
        .ok_or_else(|| "皮肤像素数据无效".to_owned())?;
    let mut png = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|error| format!("无法编码玩家皮肤：{error}"))?;

    let file =
        fs::File::create(&pack_path).map_err(|error| format!("无法创建本地皮肤资源包：{error}"))?;
    let mut archive = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let metadata = r#"{"pack":{"pack_format":34,"description":"Genesis Launcher Local Skin","supported_formats":{"min_inclusive":1,"max_inclusive":999},"min_format":[1,0],"max_format":999}}"#;
    for (path, bytes) in [
        ("pack.mcmeta", metadata.as_bytes()),
        ("pack.png", png.as_slice()),
        ("assets/minecraft/textures/entity/steve.png", png.as_slice()),
        ("assets/minecraft/textures/entity/alex.png", png.as_slice()),
    ] {
        archive
            .start_file(path, options)
            .map_err(|error| format!("无法写入皮肤资源包：{error}"))?;
        archive
            .write_all(bytes)
            .map_err(|error| format!("无法写入皮肤资源包：{error}"))?;
    }
    for model in ["wide", "slim"] {
        for name in DEFAULT_PLAYER_SKIN_NAMES {
            let path = format!("assets/minecraft/textures/entity/player/{model}/{name}.png");
            archive
                .start_file(path, options)
                .map_err(|error| format!("无法写入皮肤资源包：{error}"))?;
            archive
                .write_all(&png)
                .map_err(|error| format!("无法写入皮肤资源包：{error}"))?;
        }
    }
    archive
        .finish()
        .map_err(|error| format!("无法完成皮肤资源包：{error}"))?;
    enable_local_skin_resource_pack(data_directory)?;
    Ok(pack_path)
}

/// 实现 enablelocal皮肤资源包 相关的业务流程。
///
/// # 参数
/// - `data_directory`：由调用方传入的数据目录。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn enable_local_skin_resource_pack(data_directory: &Path) -> Result<(), String> {
    let options_path = data_directory.join("options.txt");
    let contents = fs::read_to_string(&options_path).unwrap_or_default();
    let pack_entry = format!("file/{LOCAL_SKIN_PACK_NAME}");
    let mut lines = Vec::new();
    let mut found_packs = false;
    let mut found_incompatible = false;
    for line in contents.lines() {
        if let Some(raw) = line.strip_prefix("resourcePacks:") {
            let mut packs = serde_json::from_str::<Vec<String>>(raw)
                .unwrap_or_else(|_| vec!["vanilla".to_owned()]);
            if !packs.iter().any(|pack| pack == &pack_entry) {
                packs.push(pack_entry.clone());
            }
            lines.push(format!(
                "resourcePacks:{}",
                serde_json::to_string(&packs).map_err(|error| error.to_string())?
            ));
            found_packs = true;
        } else if let Some(raw) = line.strip_prefix("incompatibleResourcePacks:") {
            let mut packs = serde_json::from_str::<Vec<String>>(raw).unwrap_or_else(|_| Vec::new());
            packs.retain(|pack| pack != &pack_entry);
            lines.push(format!(
                "incompatibleResourcePacks:{}",
                serde_json::to_string(&packs).map_err(|error| error.to_string())?
            ));
            found_incompatible = true;
        } else {
            lines.push(line.to_owned());
        }
    }
    if !found_packs {
        lines.push(format!(
            "resourcePacks:{}",
            serde_json::to_string(&vec!["vanilla", pack_entry.as_str()])
                .map_err(|error| error.to_string())?
        ));
    }
    if !found_incompatible {
        lines.push("incompatibleResourcePacks:[]".to_owned());
    }
    fs::write(&options_path, format!("{}\n", lines.join("\n")))
        .map_err(|error| format!("无法启用本地皮肤资源包：{error}"))
}
