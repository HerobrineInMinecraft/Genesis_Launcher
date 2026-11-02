//! # `src/app/utils.rs`
//!
//! **功能域：** 启动器应用外壳、页面状态、交互流程和原生 egui 绘制。
//!
//! 提供文本截断、文件选择、工程生成和统计工具
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `is_snapshot_version`
//! - `download_version_kind`
//! - `resource_category_index`
//! - `paint_list_scrollbar`
//! - `ellipsize_start`
//! - `fit_text_to_width`
//! - `ellipsize_end`
//! - `relative_play_time`
//! - `news_accent`
//! - `pick_save_file`
//! - `pick_project_directory`
//! - `pick_directory`
//! - `write_mod_project`
//! - `mod_class_name`
//! - `open_external_url`
//! - `installed_mod_files`
//! - `count_directories`
//! - `count_content_packs`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 包含平台相关分支；跨平台修改必须同时保持各 ``cfg`` 分支可编译。

// 启动器 app 模块的工具函数集合（仅对 app 模块内部可见，pub(super)）。
//
// 本文件提供大量散落在 UI 与业务逻辑中被复用的纯函数与平台相关辅助：
// - 版本字符串判断（快照/预发布）与下载类型中文名
// - 资源分类在 Tab 顺序中的索引
// - 列表滚动条绘制、文本省略（前缀/后缀/按宽度）
// - 相对游玩时间、新闻主题色、文件选择/目录选择对话框（Windows 走 PowerShell）
// - 模组工程脚手架生成（Fabric/Forge/NeoForge）
// - 模组类名安全生成、外部链接打开、已安装模组枚举
// - 版本内容条目枚举、字节/计数紧凑格式化、打开资源管理器
// - 卡片裁剪画布辅助
//
// 这些函数不直接持有应用状态，多数以参数接收所需数据，便于测试与复用。

use super::*;

/// 判断一个版本字符串是否为"快照/预发布"类版本。
///
/// 将版本转为小写后，只要包含 "snapshot"、"-rc"（候选版）、
/// "-pre"（预览版）或字符 'w'（周版本形如 24w14a）即判定为快照。
/// 参数 `version` 为原始版本字符串；返回 `true` 表示是快照/预发布。
pub(super) fn is_snapshot_version(version: &str) -> bool {
    let lower = version.to_lowercase();
    lower.contains("snapshot")
        || lower.contains("-rc")
        || lower.contains("-pre")
        || lower.contains('w')
}

/// 将 Mojang Piston 下载清单里的版本类型 `kind` 翻译为中文显示名。
///
/// 参数 `kind` 为英文类型（release/snapshot/old_beta/old_alpha）；
/// 未匹配到的未知类型原样返回。返回对应的中文标签字符串。
pub(super) fn download_version_kind(kind: &str) -> &str {
    match kind {
        "release" => "正式版",
        "snapshot" => "快照",
        "old_beta" => "旧测试版",
        "old_alpha" => "旧 Alpha",
        _ => kind,
    }
}

/// 计算某个资源分类在下载页 Tab 中的索引位置（0 表示"全部/无分类"）。
///
/// 参数 `category` 为可选的资源分类；若为 `None` 返回 0，
/// 否则返回其在 `ResourceCategory::ALL` 中的下标 + 1（即紧跟"全部"之后）。
/// 这样 0 永远代表全部，具体分类从 1 开始递增，与 Tab 顺序一致。
pub(super) fn resource_category_index(category: Option<ResourceCategory>) -> usize {
    category
        .and_then(|selected| {
            ResourceCategory::ALL
                .iter()
                .position(|category| *category == selected)
        })
        .map_or(0, |index| index + 1)
}

/// 在 egui 上手工绘制一个列表竖向滚动条（轨道 + 可拖动滑块）。
///
/// 参数：
/// - `painter`：绘制目标
/// - `right`：列表右侧 x 坐标（滚动条贴着它左侧）
/// - `list_top` / `list_height`：列表可视区域顶部与高度
/// - `content_height`：内容总高度
/// - `scroll` / `max_scroll`：当前滚动量与最大可滚动量
///
/// 当 `max_scroll<=0`（无需滚动）或内容高度为 0 时直接返回，不绘制。
/// 滑块高度按"可视比例"计算并至少 34 像素；滑块位置随滚动进度线性移动。
#[allow(clippy::too_many_arguments)]
pub(super) fn paint_list_scrollbar(
    painter: &Painter,
    right: f32,
    list_top: f32,
    list_height: f32,
    content_height: f32,
    scroll: f32,
    max_scroll: f32,
) {
    if max_scroll <= 0.0 || content_height <= 0.0 {
        return;
    }
    // 轨道：紧贴列表右侧内侧 7px、上下各缩进 3px
    let track = Rect::from_min_size(
        Pos2::new(right - 7.0, list_top + 3.0),
        Vec2::new(3.0, list_height - 6.0),
    );
    // 轨道底纹：半透明白
    theme::fill_round(
        painter,
        track,
        theme::corner_radius(),
        theme::white_alpha(16),
    );
    // 滑块高度 = 可视比例 × 轨道高，下限 34 像素保证可点击
    let thumb_height = (track.height() * list_height / content_height).max(34.0);
    // 滚动进度归一到 [0,1]，决定滑块在轨道上的偏移
    let progress = (scroll / max_scroll).clamp(0.0, 1.0);
    let thumb = Rect::from_min_size(
        Pos2::new(
            track.left(),
            track.top() + (track.height() - thumb_height) * progress,
        ),
        Vec2::new(track.width(), thumb_height),
    );
    // 滑块填充：主题绿色（半透明）
    theme::fill_round(
        painter,
        thumb,
        theme::corner_radius(),
        theme::alpha(theme::GREEN, 105),
    );
}

/// 从字符串开头省略（前缀省略），当字符数超过 `max_chars` 时用 "…" 代替开头部分。
///
/// 参数 `value` 为原字符串，`max_chars` 为允许的最大字符数（按 Unicode 字符计）。
/// 返回原样或 "…" + 尾部不超过 `max_chars` 个字符。
pub(super) fn ellipsize_start(value: &str, max_chars: usize) -> String {
    let count = value.chars().count();
    if count <= max_chars {
        return value.to_owned();
    }
    // 跳过前面多余的字符，只保留最后 max_chars 个
    let tail: String = value
        .chars()
        .skip(count.saturating_sub(max_chars))
        .collect();
    format!("…{tail}")
}

/// 按像素宽度裁剪文本：超出 `max_width` 时逐字符删减并补 "…"，保证布局不溢出。
///
/// 参数：`painter` 用于测量文本；`value` 原文本；`font` 测量所用字体；
/// `max_width` 最大像素宽度。先用 egui 布局测量整体宽度，未超宽则原样返回，
/// 否则从尾部逐个删字符直至加上 "…" 后仍能放下，极端下仅返回 "…"。
pub(super) fn fit_text_to_width(
    painter: &Painter,
    value: &str,
    font: egui::FontId,
    max_width: f32,
) -> String {
    // 先整体测量，未超宽直接返回
    if painter
        .layout_no_wrap(value.to_owned(), font.clone(), theme::ink())
        .size()
        .x
        <= max_width
    {
        return value.to_owned();
    }
    // 逐字符删减尾部，直到补上省略号后仍不超过最大宽度
    let mut chars: Vec<char> = value.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let candidate = format!("{}…", chars.iter().collect::<String>());
        if painter
            .layout_no_wrap(candidate.clone(), font.clone(), theme::ink())
            .size()
            .x
            <= max_width
        {
            return candidate;
        }
    }
    "…".to_owned()
}

/// 从字符串末尾省略（后缀省略），超过 `max_chars` 时保留前面部分并补 "…"。
///
/// 参数 `value` 原字符串，`max_chars` 最大字符数（按 Unicode 字符计）。
/// 注意保留 `max_chars-1` 个字符，为省略号留出位置。
pub(super) fn ellipsize_end(value: &str, max_chars: usize) -> String {
    let count = value.chars().count();
    if count <= max_chars {
        return value.to_owned();
    }
    // 取前 max_chars-1 个字符，再加省略号
    let head: String = value.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{head}…")
}

/// 把 Unix 秒级时间戳格式化为中文相对时间（刚刚/分钟前/小时前/天前/很久以前）。
///
/// 参数 `timestamp` 为某次游玩的 Unix 秒；以当前时间减去它得到经过秒数。
/// 若系统时间异常（早于时间戳）则以时间戳本身兜底，避免下溢。
pub(super) fn relative_play_time(timestamp: u64) -> String {
    // 取当前 Unix 秒；失败时用给定的 timestamp 兜底
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(timestamp);
    let elapsed = now.saturating_sub(timestamp);
    // 分级阈值：60s / 3600s / 86400s / 2592000s(30 天)
    if elapsed < 60 {
        "刚刚".to_owned()
    } else if elapsed < 3_600 {
        format!("{} 分钟前", elapsed / 60)
    } else if elapsed < 86_400 {
        format!("{} 小时前", elapsed / 3_600)
    } else if elapsed < 2_592_000 {
        format!("{} 天前", elapsed / 86_400)
    } else {
        "很久以前".to_owned()
    }
}

/// 根据新闻条目下标返回其主题强调色（用于卡片左侧色条/标题）。
///
/// 参数 `index` 为新闻在列表中的下标：0 用青绿、1 用主题绿、其余用蓝紫。
/// 返回对应的 `Color32`。
pub(super) fn news_accent(index: usize) -> Color32 {
    match index {
        0 => Color32::from_rgb(93, 213, 205),
        1 => theme::GREEN,
        _ => Color32::from_rgb(143, 173, 247),
    }
}

/// 弹出"保存文件"系统对话框，让用户选择导出目标路径（仅 Windows 实现）。
///
/// 参数：
/// - `default_path`：默认文件名与所在目录（取自其文件名与父目录）
/// - `description`：对话框标题
/// - `extension`：默认扩展名（自动去掉前导点）
/// - `filter_label`：文件类型筛选标签
///
/// 通过 `powershell.exe` 调用 `System.Windows.Forms.SaveFileDialog`，
/// 以 UTF-8 输出选中路径。返回 `Some(PathBuf)`；取消或出错返回 `None`。
/// 非 Windows 平台直接返回 `None`。
pub(super) fn pick_save_file(
    default_path: &std::path::Path,
    description: &str,
    extension: &str,
    filter_label: &str,
) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // 取父目录作为初始目录，文件名作为默认文件名；单引号转义为双单引号（PowerShell 内插安全）
        let initial_directory = default_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_string_lossy()
            .replace('\'', "''");
        let file_name = default_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .replace('\'', "''");
        let description = description.replace('\'', "''");
        let filter_label = filter_label.replace('\'', "''");
        let extension = extension.trim_start_matches('.');
        // 构造 PowerShell 脚本：显示保存对话框，选中后把文件名写到 stdout
        let script = format!(
            "[Console]::OutputEncoding=[Text.Encoding]::UTF8; Add-Type -AssemblyName System.Windows.Forms; $d=New-Object System.Windows.Forms.SaveFileDialog; $d.Title='{description}'; $d.InitialDirectory='{initial_directory}'; $d.FileName='{file_name}'; $d.Filter='{filter_label} (*.{extension})|*.{extension}|所有文件 (*.*)|*.*'; $d.DefaultExt='{extension}'; $d.AddExtension=$true; $d.OverwritePrompt=$true; if($d.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK){{[Console]::Out.Write($d.FileName)}}"
        );
        let mut command = std::process::Command::new("powershell.exe");
        command
            .args(["-NoProfile", "-STA", "-Command", &script])
            // 0x0800_0000 = CREATE_NO_WINDOW：避免弹出多余控制台窗口
            .creation_flags(0x0800_0000);
        let output = command.output().ok()?;
        // 读取 stdout 并去除首尾空白作为选中路径
        let selected = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        (!selected.is_empty()).then(|| PathBuf::from(selected))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (default_path, description, extension, filter_label);
        None
    }
}

/// 选择模组工程保存目录：以当前工作目录为默认位置，调用 `pick_directory`。
///
/// 返回用户选中的目录 `PathBuf`，取消则返回 `None`。
pub(super) fn pick_project_directory() -> Option<PathBuf> {
    pick_directory(
        &std::env::current_dir().unwrap_or_default(),
        "选择模组工程保存位置",
    )
}

/// 弹出"选择文件夹"系统对话框（仅 Windows 实现）。
///
/// 参数 `default_directory` 为初始选中目录，`description` 为对话框上方说明文字。
/// 通过 PowerShell 调用 `System.Windows.Forms.FolderBrowserDialog`，
/// 选中后把路径写到 stdout。返回 `Some(PathBuf)` 或 `None`（取消/非 Windows）。
pub(super) fn pick_directory(
    default_directory: &std::path::Path,
    description: &str,
) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // 路径与说明中的单引号转义，避免 PowerShell 字符串中断
        let default_directory = default_directory.to_string_lossy().replace('\'', "''");
        let description = description.replace('\'', "''");
        // 构造 PowerShell 脚本：FolderBrowserDialog，允许新建文件夹
        let script = format!(
            "[Console]::OutputEncoding=[Text.Encoding]::UTF8; Add-Type -AssemblyName System.Windows.Forms; $d=New-Object System.Windows.Forms.FolderBrowserDialog; $d.Description='{description}'; $d.SelectedPath='{default_directory}'; $d.ShowNewFolderButton=$true; if($d.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK){{[Console]::Out.Write($d.SelectedPath)}}"
        );
        let mut command = std::process::Command::new("powershell.exe");
        command
            .args(["-NoProfile", "-STA", "-Command", &script])
            // 0x0800_0000 = CREATE_NO_WINDOW
            .creation_flags(0x0800_0000);
        let output = command.output().ok()?;
        let selected = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        (!selected.is_empty()).then(|| PathBuf::from(selected))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = default_directory;
        let _ = description;
        None
    }
}

/// 根据模组工程设置，在 `root` 下生成一个可编译的模组工程骨架（Fabric/Forge/NeoForge）。
///
/// 参数 `root` 为工程根目录，`settings` 包含名称/ID/包名/版本/MC 版本/加载器类型(0=Fabric,1=Forge,2=NeoForge)。
/// 先校验 ID（仅字母数字及 `_-`）、Java 包名（点分且每段仅字母数字/下划线）、MC 版本非空、
/// 且目标目录 `{id}-mod` 尚未存在；随后写入 Java 主类、元数据、Gradle 配置、README 等。
///
/// 返回 `Ok(PathBuf)`（工程目录）；任何校验失败或写入错误返回 `Err(String)`（中文错误信息）。
/// 副作用：在磁盘上创建目录与文件。
pub(super) fn write_mod_project(
    root: &std::path::Path,
    settings: &ModProjectSettings,
) -> Result<PathBuf, String> {
    let id = settings.id.trim();
    // 校验模组 ID：非空且只含 ASCII 字母数字、下划线、连字符
    if id.is_empty()
        || !id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
    {
        return Err("模组 ID 只能包含字母、数字、下划线和连字符".to_owned());
    }
    let package = settings.package.trim();
    // 校验 Java 包名：点分，每段非空且只含字母数字/下划线
    if package.is_empty()
        || !package.split('.').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_')
        })
    {
        return Err("Java 包名格式无效".to_owned());
    }
    // MC 版本不可为空
    if settings.minecraft.trim().is_empty() {
        return Err("Minecraft 版本不能为空".to_owned());
    }
    // 工程目录名 = {id}-mod，已存在则报错
    let project = root.join(format!("{id}-mod"));
    if project.exists() {
        return Err(format!("目标目录已经存在：{}", project.display()));
    }
    // 由名称生成 Java 主类名（必要时回退到 id），并把包名转为目录路径
    let class_name = mod_class_name(&settings.name, id);
    let package_path = package.replace('.', "/");
    let java_dir = project.join("src/main/java").join(&package_path);
    let resources = project.join("src/main/resources");
    fs::create_dir_all(&java_dir).map_err(|error| format!("创建源码目录失败：{error}"))?;
    fs::create_dir_all(&resources).map_err(|error| format!("创建资源目录失败：{error}"))?;
    // 按加载器类型拼接 Java 主类源码；Fabric 实现 ModInitializer，Forge/NeoForge 用 @Mod
    let java = match settings.loader {
        0 => format!(
            "package {package};\n\nimport net.fabricmc.api.ModInitializer;\nimport org.slf4j.Logger;\nimport org.slf4j.LoggerFactory;\n\npublic final class {class_name} implements ModInitializer {{\n    public static final String MOD_ID = \"{id}\";\n    public static final Logger LOGGER = LoggerFactory.getLogger(MOD_ID);\n\n    @Override\n    public void onInitialize() {{\n        LOGGER.info(\"Initializing {}\", MOD_ID);\n    }}\n}}\n",
            settings.name.replace(['"', '\\'], "")
        ),
        _ => format!(
            "package {package};\n\nimport net.minecraftforge.fml.common.Mod;\n\n@Mod({class_name}.MOD_ID)\npublic final class {class_name} {{\n    public static final String MOD_ID = \"{id}\";\n\n    public {class_name}() {{\n    }}\n}}\n"
        ),
    };
    // NeoForge 仅替换导入包名（fml.common.Mod → neoforged.fml.common.Mod）
    let java = if settings.loader == 2 {
        java.replace(
            "net.minecraftforge.fml.common.Mod",
            "net.neoforged.fml.common.Mod",
        )
    } else {
        java
    };
    fs::write(java_dir.join(format!("{class_name}.java")), java)
        .map_err(|error| format!("写入 Java 源码失败：{error}"))?;
    match settings.loader {
        0 => {
            // Fabric：写入 fabric.mod.json 元数据
            let metadata = serde_json::json!({
                "schemaVersion": 1,
                "id": id,
                "version": settings.version,
                "name": settings.name,
                "environment": "*",
                "entrypoints": {"main": [format!("{package}.{class_name}")]},
                "depends": {
                    "fabricloader": ">=0.15.0",
                    "minecraft": format!("~{}", settings.minecraft)
                }
            });
            fs::write(
                resources.join("fabric.mod.json"),
                serde_json::to_vec_pretty(&metadata)
                    .map_err(|error| format!("生成模组元数据失败：{error}"))?,
            )
            .map_err(|error| format!("写入模组元数据失败：{error}"))?;
            // Fabric 的 build.gradle（使用 fabric-loom 插件，Java 21 工具链）
            fs::write(
                project.join("build.gradle"),
                "plugins {\n    id 'fabric-loom' version '1.10-SNAPSHOT'\n}\n\nversion = project.mod_version\ngroup = project.maven_group\n\nbase {\n    archivesName = project.archives_base_name\n}\n\ndependencies {\n    minecraft \"com.mojang:minecraft:${project.minecraft_version}\"\n    mappings loom.officialMojangMappings()\n    modImplementation \"net.fabricmc:fabric-loader:${project.loader_version}\"\n}\n\njava {\n    toolchain.languageVersion = JavaLanguageVersion.of(21)\n}\n",
            )
            .map_err(|error| format!("写入 Gradle 配置失败：{error}"))?;
            // gradle.properties 写入各版本占位符
            fs::write(
                project.join("gradle.properties"),
                format!(
                    "mod_version={}\nmaven_group={}\narchives_base_name={}\nminecraft_version={}\nloader_version=0.16.0\n",
                    settings.version, package, id, settings.minecraft
                ),
            )
            .map_err(|error| format!("写入 Gradle 属性失败：{error}"))?;
        }
        loader => {
            // Forge(1) 或 NeoForge(2)：写入 META-INF 下的元数据（mods.toml / neoforge.mods.toml）
            let loader_name = if loader == 1 { "Forge" } else { "NeoForge" };
            fs::create_dir_all(resources.join("META-INF"))
                .map_err(|error| format!("创建元数据目录失败：{error}"))?;
            let metadata_name = if loader == 1 {
                "mods.toml"
            } else {
                "neoforge.mods.toml"
            };
            fs::write(
                resources.join("META-INF").join(metadata_name),
                format!(
                    "modLoader=\"{loader_name}\"\nloaderVersion=\"[1,)\"\nlicense=\"All Rights Reserved\"\n[[mods]]\nmodId=\"{id}\"\nversion=\"{}\"\ndisplayName=\"{}\"\n",
                    settings.version,
                    settings.name.replace('"', "")
                ),
            )
            .map_err(|error| format!("写入模组元数据失败：{error}"))?;
            if loader == 1 {
                // Forge 的 build.gradle（使用 ForgeGradle）
                fs::write(
                    project.join("build.gradle"),
                    "plugins {\n    id 'eclipse'\n    id 'net.minecraftforge.gradle' version '[6.0,6.2)'\n}\n\nversion = project.mod_version\ngroup = project.maven_group\n\nminecraft {\n    mappings channel: 'official', version: project.minecraft_version\n}\n\ndependencies {\n    minecraft \"net.minecraftforge:forge:${project.minecraft_version}-${project.loader_version}\"\n}\n",
                )
                .map_err(|error| format!("写入 Forge Gradle 配置失败：{error}"))?;
            } else {
                // NeoForge 的 build.gradle（使用 moddev 插件）
                fs::write(
                    project.join("build.gradle"),
                    "plugins {\n    id 'net.neoforged.moddev' version '2.0.78'\n}\n\nversion = project.mod_version\ngroup = project.maven_group\n\nneoForge {\n    version = \"${project.minecraft_version}-${project.loader_version}\"\n}\n",
                )
                .map_err(|error| format!("写入 NeoForge Gradle 配置失败：{error}"))?;
            }
            // gradle.properties（loader_version 留待用户补充）
            fs::write(
                project.join("gradle.properties"),
                format!(
                    "mod_version={}\nmaven_group={}\nminecraft_version={}\nloader_version=REPLACE_ME\n",
                    settings.version, package, settings.minecraft
                ),
            )
            .map_err(|error| format!("写入 Gradle 属性失败：{error}"))?;
        }
    }
    // settings.gradle 与 README 对所有加载器通用
    fs::write(
        project.join("settings.gradle"),
        format!("rootProject.name = '{}'\n", settings.name.replace('\'', "")),
    )
    .map_err(|error| format!("写入 settings.gradle 失败：{error}"))?;
    fs::write(
        project.join("README.md"),
        format!(
            "# {}\n\n- Mod ID: `{id}`\n- Package: `{package}`\n- Minecraft: `{}`\n- Loader: {}\n\n请根据所选加载器官网补充对应的 Gradle 插件和加载器版本，然后构建模组。\n",
            settings.name,
            settings.minecraft,
            if settings.loader == 0 {
                "Fabric"
            } else if settings.loader == 1 {
                "Forge"
            } else {
                "NeoForge"
            }
        ),
    )
    .map_err(|error| format!("写入 README 失败：{error}"))?;
    Ok(project)
}

/// 由模组名称（必要时回退到 id）生成安全的 Java 类名。
///
/// 规则：按非字母数字切分单词，每段首字母大写并拼接；若名称产出为空则用 `fallback`
/// 相同规则生成；若仍为空则固定 "ExampleMod"；最后若不以 "Mod" 结尾则补上 "Mod"。
/// 参数 `name` 为显示名，`fallback` 为回退串。返回合法的 Java 类名。
pub(super) fn mod_class_name(name: &str, fallback: &str) -> String {
    // 主路径：用名称生成
    let mut class_name = name
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|first| first.to_ascii_uppercase().to_string() + chars.as_str())
                .unwrap_or_default()
        })
        .collect::<String>();
    // 名称为空时用 fallback 生成
    if class_name.is_empty() {
        class_name = fallback
            .split(|character: char| !character.is_ascii_alphanumeric())
            .map(|part| {
                let mut chars = part.chars();
                chars
                    .next()
                    .map(|first| first.to_ascii_uppercase().to_string() + chars.as_str())
                    .unwrap_or_default()
            })
            .collect();
    }
    // 仍为空则使用固定占位名
    if class_name.is_empty() {
        class_name = "ExampleMod".to_owned();
    }
    // 保证类名以 "Mod" 结尾，便于统一识别
    if !class_name.ends_with("Mod") {
        class_name.push_str("Mod");
    }
    class_name
}

/// 在系统默认浏览器中打开外部 URL（仅允许 Minecraft 官方页面）。
///
/// 参数 `url` 为待打开链接。先经 `crate::news::allowed_article_url` 校验是否为官方页面，
/// 否则返回 `Err`。Windows 用 `rundll32 url.dll,FileProtocolHandler` 打开；
/// 其他平台用 `xdg-open`。以 spawn 异步启动，不阻塞。返回 `Ok(())` 或 `Err(String)`。
pub(super) fn open_external_url(url: &str) -> Result<(), String> {
    if !crate::news::allowed_article_url(url) {
        return Err("该链接不是 Minecraft 官方页面".to_owned());
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("rundll32.exe")
            .args(["url.dll,FileProtocolHandler", url])
            // 0x0800_0000 = CREATE_NO_WINDOW
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

/// 枚举某个游戏目录下 `mods` 文件夹中的已安装模组文件（仅 .jar，含文件大小）。
///
/// 参数 `game_directory` 为游戏目录。读取其下 `mods` 子目录，
/// 过滤出扩展名 .jar（忽略大小写）的文件，返回 `(文件名, 文件字节数)` 列表，
/// 并按文件名小写排序。目录不存在/无权限时返回空向量（不报错）。
pub(super) fn installed_mod_files(game_directory: &std::path::Path) -> Vec<(String, u64)> {
    // 读取 mods 目录；失败直接返回空
    let Ok(entries) = std::fs::read_dir(game_directory.join("mods")) else {
        return Vec::new();
    };
    let mut files = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            // 仅保留 .jar 文件
            if !path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("jar"))
            {
                return None;
            }
            Some((
                path.file_name()?.to_string_lossy().to_string(),
                entry.metadata().ok()?.len(),
            ))
        })
        .collect::<Vec<_>>();
    files.sort_by_key(|entry| entry.0.to_lowercase());
    files
}

/// 统计某路径下子目录的数量。
///
/// 参数 `path` 为目标目录；读取其直接子项并统计 `is_dir()` 为真的数量。
/// 目录不存在/无权限时返回 0。
pub(super) fn count_directories(path: &std::path::Path) -> usize {
    std::fs::read_dir(path)
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.path().is_dir())
                .count()
        })
        .unwrap_or(0)
}

/// 统计某路径下的"内容包"数量：子目录，或扩展名为 .zip/.jar 的文件。
///
/// 参数 `path` 为目标目录；常用于统计材质包/光影包/数据包等。
/// 目录不可访问时返回 0。
pub(super) fn count_content_packs(path: &std::path::Path) -> usize {
    std::fs::read_dir(path)
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| {
                    let path = entry.path();
                    path.is_dir()
                        || path.extension().is_some_and(|extension| {
                            extension.eq_ignore_ascii_case("zip")
                                || extension.eq_ignore_ascii_case("jar")
                        })
                })
                .count()
        })
        .unwrap_or(0)
}

/// 枚举某版本内容目录下的条目（模组/存档/光影/材质/数据包/导出）。
///
/// 参数 `path` 为目标目录，`kind` 决定筛选与展示规则：
/// - Mods：仅 .jar 文件，detail 显示"模组 · 大小"
/// - Saves：仅子目录，detail 显示"存档文件夹"
/// - ShaderPacks/ResourcePacks/Datapacks：子目录或 .zip/.jar 压缩包
/// - Export：返回 None（不枚举）
///
/// 返回 `VersionContentEntry` 列表，按名称小写排序。目录不可访问返回空。
pub(super) fn version_content_entries(
    path: &std::path::Path,
    kind: VersionContentKind,
) -> Vec<VersionContentEntry> {
    let Ok(entries) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut result = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let metadata = entry.metadata().ok()?;
            let name = entry.file_name().to_string_lossy().to_string();
            match kind {
                VersionContentKind::Mods => {
                    // 模组：必须是 .jar 文件
                    if !path.is_file()
                        || !path
                            .extension()
                            .is_some_and(|extension| extension.eq_ignore_ascii_case("jar"))
                    {
                        return None;
                    }
                    Some(VersionContentEntry {
                        name,
                        detail: format!("模组 · {}", format_size(metadata.len())),
                    })
                }
                VersionContentKind::Saves => {
                    // 存档：必须是子目录
                    if !path.is_dir() {
                        return None;
                    }
                    Some(VersionContentEntry {
                        name,
                        detail: "存档文件夹".to_owned(),
                    })
                }
                VersionContentKind::ShaderPacks
                | VersionContentKind::ResourcePacks
                | VersionContentKind::Datapacks => {
                    // 内容包：子目录直接计入；压缩包(.zip/.jar)显示大小
                    if path.is_dir() {
                        Some(VersionContentEntry {
                            name,
                            detail: format!(
                                "{} · 文件夹",
                                match kind {
                                    VersionContentKind::ShaderPacks => "光影包",
                                    VersionContentKind::ResourcePacks => "材质包",
                                    _ => "数据包",
                                }
                            ),
                        })
                    } else if path.extension().is_some_and(|extension| {
                        extension.eq_ignore_ascii_case("zip")
                            || extension.eq_ignore_ascii_case("jar")
                    }) {
                        Some(VersionContentEntry {
                            name,
                            detail: format!("压缩包 · {}", format_size(metadata.len())),
                        })
                    } else {
                        None
                    }
                }
                VersionContentKind::Export => None,
            }
        })
        .collect::<Vec<_>>();
    result.sort_by_key(|entry| entry.name.to_lowercase());
    result
}

/// 把字节数格式化为带单位的易读字符串（B / KB / MB）。
///
/// 参数 `bytes` 为字节数；>=1MiB 显示 1 位小数的 MB，>=1KiB 显示 KB，否则显示 B。
pub(super) fn format_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

/// 把大计数压缩为带后缀字符串（K / M / B），用于下载量等展示。
///
/// 参数 `value` 为原始计数；>=1e9 用 B，>=1e6 用 M，>=1e3 用 K，否则原样。均保留 1 位小数。
pub(super) fn compact_count(value: u64) -> String {
    if value >= 1_000_000_000 {
        format!("{:.1}B", value as f64 / 1_000_000_000.0)
    } else if value >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1}K", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}

/// 在系统文件管理器中打开指定路径（资源管理器 / xdg-open），不等待、忽略错误。
///
/// 参数 `path` 为要打开的目录或文件；Windows 用 `explorer`，其他平台用 `xdg-open`。
pub(super) fn open_path(path: &std::path::Path) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer").arg(path).spawn();
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    }
}

/// 为某张卡片创建一个裁剪画布（clip rect），使后续绘制不超出卡片矩形。
///
/// 参数 `painter` 原画布，`card` 卡片矩形；返回裁剪后的子 `Painter`。
/// 用于在卡片圆角/边界内绘制文字与控件，避免溢出到相邻卡片。
pub(super) fn card_clip(painter: &Painter, card: Rect) -> Painter {
    painter.with_clip_rect(card)
}
