#![allow(clippy::needless_borrow)]

use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io::Write as _;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use eframe::egui::{
    self, Color32, CursorIcon, Id, Painter, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Vec2,
};
use eframe::{App, CreationContext, Frame};

use crate::brand;
use crate::game::JavaRuntime;
use crate::loader::{LoaderInstallUpdate, LoaderKind, display_version as loader_display_version};
use crate::news::MinecraftNews;
use crate::resources::{ResourceCategory, ResourceProject};
use crate::scene;
use crate::sessions::{RecentSession, SessionKind};
use crate::theme;
use crate::update::{self, LauncherRelease};
use crate::vector;

const TITLEBAR_HEIGHT: f32 = 42.0;
const RAIL_WIDTH: f32 = 86.0;
const CONTENT_PAD: f32 = 23.0;
const VERSION_ROW_HEIGHT: f32 = 51.5;
const VERSION_MENU_MAX_HEIGHT: f32 = 430.0;
const RECENT_CARD_SIZE: Vec2 = Vec2::new(246.0, 187.0);
const SETTINGS_ITEM_SPACING: f32 = 49.0;
const SETTING_CARD_PADDING_X: f32 = 14.0;
const CONTENT_CLOSE_BOUNCE_SECONDS: f32 = 0.18;
const SKIN_HISTORY_LIMIT: usize = 64;
const SETTINGS_REVISION: u32 = 2;

#[derive(Clone)]
struct GameVersion {
    version: String,
    game_version: String,
    loader: String,
    loader_family: String,
    java: String,
    description: String,
    launchable: bool,
}

#[derive(Clone, PartialEq, Eq)]
struct RecentPlay {
    version: String,
    played_at: u64,
}

#[derive(Clone, PartialEq, Eq)]
struct FavoriteSite {
    title: String,
    url: String,
    icon_source: String,
}

type FavoriteIconReceiver = std::sync::mpsc::Receiver<Vec<(String, Vec<u8>)>>;

fn available_versions(game_directory: Option<&std::path::Path>) -> Vec<GameVersion> {
    let installed = game_directory
        .map(crate::game::installed_versions)
        .unwrap_or_default();
    if !installed.is_empty() {
        let launchable: std::collections::HashSet<String> = game_directory
            .map(crate::game::launchable_versions)
            .unwrap_or_default()
            .into_iter()
            .collect();
        return installed
            .into_iter()
            .map(|version| {
                let lower = version.to_lowercase();
                let loader = if lower.contains("neo") {
                    "NeoForge"
                } else if lower.contains("fabric") {
                    "Fabric"
                } else if lower.contains("forge") {
                    "Forge"
                } else {
                    "原版"
                };
                let loader_display = game_directory
                    .and_then(|directory| {
                        let loader_key = match loader {
                            "NeoForge" => "neoforge",
                            "Fabric" => "fabric",
                            "Forge" => "forge",
                            _ => return None,
                        };
                        crate::game::installed_loader_version(directory, &version, loader_key)
                            .map(|loader_version| format!("{loader} {loader_version}"))
                    })
                    .unwrap_or_else(|| loader.to_owned());
                let game_version = game_directory
                    .and_then(|directory| {
                        crate::game::official_base_version(directory, &version, loader)
                    })
                    .unwrap_or_else(|| version.clone());
                let java_major = game_directory
                    .and_then(|directory| {
                        crate::game::version_java_requirement(directory, &version)
                    })
                    .unwrap_or_else(|| {
                        if version.starts_with("1.20.4")
                            || version.starts_with("1.19")
                            || version.starts_with("1.18")
                            || version.starts_with("1.17")
                        {
                            17
                        } else {
                            21
                        }
                    });
                let java = format!("Java {java_major}");
                let is_launchable = launchable.contains(&version);
                GameVersion {
                    description: if is_launchable {
                        format!("{loader_display} · 已安装 · 可启动")
                    } else {
                        format!("{loader_display} · 启动时自动补全")
                    },
                    version,
                    game_version,
                    loader: loader_display,
                    loader_family: loader.to_owned(),
                    java,
                    launchable: is_launchable,
                }
            })
            .collect();
    }

    vec![
        GameVersion {
            version: "1.21.11".to_owned(),
            game_version: "1.21.11".to_owned(),
            loader: "原版".to_owned(),
            loader_family: "原版".to_owned(),
            java: "Java 21".to_owned(),
            description: "Mojang 官方源 · 启动时自动安装".to_owned(),
            launchable: false,
        },
        GameVersion {
            version: "1.21.10".to_owned(),
            game_version: "1.21.10".to_owned(),
            loader: "原版".to_owned(),
            loader_family: "原版".to_owned(),
            java: "Java 21".to_owned(),
            description: "Mojang 官方源 · 启动时自动安装".to_owned(),
            launchable: false,
        },
        GameVersion {
            version: "1.21.9".to_owned(),
            game_version: "1.21.9".to_owned(),
            loader: "原版".to_owned(),
            loader_family: "原版".to_owned(),
            java: "Java 21".to_owned(),
            description: "Mojang 官方源 · 启动时自动安装".to_owned(),
            launchable: false,
        },
        GameVersion {
            version: "1.21.8".to_owned(),
            game_version: "1.21.8".to_owned(),
            loader: "原版".to_owned(),
            loader_family: "原版".to_owned(),
            java: "Java 21".to_owned(),
            description: "Mojang 官方源 · 启动时自动安装".to_owned(),
            launchable: false,
        },
    ]
}

fn version_recency_map(
    settings: &LauncherSettings,
    game_directory: Option<&std::path::Path>,
) -> HashMap<String, u64> {
    let mut recency: HashMap<String, u64> = settings
        .recent_versions
        .iter()
        .map(|recent| (recent.version.clone(), recent.played_at))
        .collect();
    if let Some(game_directory) = game_directory {
        for (version, played_at) in official_profile_recency(game_directory) {
            recency
                .entry(version)
                .and_modify(|known| *known = (*known).max(played_at))
                .or_insert(played_at);
        }
    }
    recency
}

fn version_menu_order(versions: &[GameVersion], recency: &HashMap<String, u64>) -> Vec<usize> {
    let mut order: Vec<usize> = (0..versions.len()).collect();
    order.sort_by(|left, right| {
        recency
            .get(&versions[*right].version)
            .copied()
            .unwrap_or(0)
            .cmp(&recency.get(&versions[*left].version).copied().unwrap_or(0))
            .then_with(|| left.cmp(right))
    });
    order
}

fn official_profile_recency(game_directory: &std::path::Path) -> HashMap<String, u64> {
    let Ok(contents) = std::fs::read_to_string(game_directory.join("launcher_profiles.json"))
    else {
        return HashMap::new();
    };
    let Ok(profiles) = serde_json::from_str::<serde_json::Value>(&contents) else {
        return HashMap::new();
    };
    profiles
        .get("profiles")
        .and_then(serde_json::Value::as_object)
        .map(|profiles| {
            profiles
                .values()
                .filter_map(|profile| {
                    let version = profile.get("lastVersionId")?.as_str()?.to_owned();
                    let played_at = parse_iso_timestamp(profile.get("lastUsed")?.as_str()?)?;
                    Some((version, played_at))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_iso_timestamp(value: &str) -> Option<u64> {
    if value.len() < 19 {
        return None;
    }
    let year = value.get(0..4)?.parse::<i64>().ok()?;
    let month = value.get(5..7)?.parse::<i64>().ok()?;
    let day = value.get(8..10)?.parse::<i64>().ok()?;
    let hour = value.get(11..13)?.parse::<i64>().ok()?;
    let minute = value.get(14..16)?.parse::<i64>().ok()?;
    let second = value.get(17..19)?.parse::<i64>().ok()?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let adjusted_year = year - i64::from(month <= 2);
    let era = if adjusted_year >= 0 {
        adjusted_year
    } else {
        adjusted_year - 399
    } / 400;
    let year_of_era = adjusted_year - era * 400;
    let shifted_month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let mut timestamp =
        (era * 146_097 + day_of_era - 719_468) * 86_400 + hour * 3_600 + minute * 60 + second;
    let timezone = value.get(19..).unwrap_or("");
    if timezone.starts_with('+') || timezone.starts_with('-') {
        let offset_hour = timezone.get(1..3)?.parse::<i64>().ok()?;
        let offset_minute = timezone.get(4..6)?.parse::<i64>().ok()?;
        let offset = offset_hour * 3_600 + offset_minute * 60;
        timestamp += if timezone.starts_with('+') {
            -offset
        } else {
            offset
        };
    }
    u64::try_from(timestamp).ok()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Menu {
    None,
    Version,
    Profile,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Home,
    Versions,
    Download,
    Community,
    Create,
    Settings,
}

#[derive(Clone, Copy)]
struct CardMotion {
    screen: Rect,
    direction: f32,
    progress: f32,
    outgoing: bool,
}

#[derive(Clone, Copy)]
struct SettingsView {
    section: SettingsSection,
    chrome_motion: CardMotion,
    content_motion: CardMotion,
    draw_chrome: bool,
    draw_footer: bool,
}

#[derive(Clone)]
struct DownloadDetailView {
    version: crate::download::OfficialVersion,
    motion: CardMotion,
    active: bool,
}

#[derive(Clone)]
struct ResourceDetailView {
    project: Option<ResourceProject>,
    motion: CardMotion,
    active: bool,
}

#[derive(Clone)]
struct VersionDetailView {
    index: usize,
    version: GameVersion,
    motion: CardMotion,
    active: bool,
}

#[derive(Clone, Copy)]
struct NavTooltip {
    id: u8,
    label: &'static str,
    button: Rect,
}

impl CardMotion {
    fn rect(self, rect: Rect) -> Rect {
        rect.translate(Vec2::new(0.0, self.offset(rect)))
    }

    fn local_progress(self, rect: Rect) -> f32 {
        let distance = if self.direction > 0.0 {
            (self.screen.bottom() - rect.center().y) / self.screen.height().max(1.0)
        } else {
            (rect.center().y - self.screen.top()) / self.screen.height().max(1.0)
        };
        let delay = distance.clamp(0.0, 1.0) * 0.36;
        ((self.progress - delay) / (1.0 - delay)).clamp(0.0, 1.12)
    }

    fn offset(self, rect: Rect) -> f32 {
        let local = self.local_progress(rect);
        if self.outgoing {
            -self.direction * self.screen.height() * local
        } else {
            self.direction * self.screen.height() * (1.0 - local)
        }
    }
}

fn outgoing_page_opacity(progress: f32) -> f32 {
    ((1.0 - progress.clamp(0.0, 1.0)) / 0.45).clamp(0.0, 1.0)
}

fn version_content_text_opacity(progress: f32) -> f32 {
    card_text_opacity(progress)
}

fn card_text_opacity(progress: f32) -> f32 {
    let fade = ((progress - 0.16) / 0.54).clamp(0.0, 1.0);
    fade * fade * (3.0 - 2.0 * fade)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VersionFilter {
    All,
    Release,
    Snapshot,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VersionContentKind {
    Mods,
    Saves,
    ShaderPacks,
    ResourcePacks,
    Export,
}

impl VersionContentKind {
    fn folder(self) -> &'static str {
        match self {
            Self::Mods => "mods",
            Self::Saves => "saves",
            Self::ShaderPacks => "shaderpacks",
            Self::ResourcePacks => "resourcepacks",
            Self::Export => "",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Mods => "模组",
            Self::Saves => "存档",
            Self::ShaderPacks => "光影",
            Self::ResourcePacks => "材质包",
            Self::Export => "导出",
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Mods => 0,
            Self::Saves => 1,
            Self::ShaderPacks => 2,
            Self::ResourcePacks => 3,
            Self::Export => 4,
        }
    }
}

struct VersionContentEntry {
    name: String,
    detail: String,
}

struct VersionExportOptions {
    mods: bool,
    config: bool,
    saves: bool,
    resourcepacks: bool,
    shaderpacks: bool,
}

struct ModProjectSettings {
    name: String,
    id: String,
    package: String,
    version: String,
    minecraft: String,
    loader: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SkinPart {
    Head,
    Body,
    RightArm,
    LeftArm,
    RightLeg,
    LeftLeg,
}

#[derive(Clone, Copy, Debug)]
struct SkinPoint3 {
    x: f32,
    y: f32,
    z: f32,
}

impl SkinPoint3 {
    const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
}

#[derive(Clone, Copy, Debug)]
struct SkinCuboid {
    part: SkinPart,
    min: SkinPoint3,
    max: SkinPoint3,
    uv_x: f32,
    uv_y: f32,
}

impl SkinCuboid {
    fn size(self) -> SkinPoint3 {
        SkinPoint3::new(
            self.max.x - self.min.x,
            self.max.y - self.min.y,
            self.max.z - self.min.z,
        )
    }
}

#[derive(Clone, Copy, Debug)]
struct SkinCuboidFace {
    face: usize,
    points: [SkinPoint3; 4],
    uv: Rect,
    normal: SkinPoint3,
}

#[derive(Default)]
struct SkinEditHistory {
    undo: VecDeque<Vec<u8>>,
    redo: Vec<Vec<u8>>,
}

impl SkinEditHistory {
    fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    fn record(&mut self, before: Vec<u8>, current: &[u8]) {
        if before.as_slice() == current {
            return;
        }
        if self.undo.len() >= SKIN_HISTORY_LIMIT {
            self.undo.pop_front();
        }
        self.undo.push_back(before);
        self.redo.clear();
    }

    fn undo(&mut self, current: &[u8]) -> Option<Vec<u8>> {
        let previous = self.undo.pop_back()?;
        if self.redo.len() >= SKIN_HISTORY_LIMIT {
            self.redo.remove(0);
        }
        self.redo.push(current.to_vec());
        Some(previous)
    }

    fn redo(&mut self, current: &[u8]) -> Option<Vec<u8>> {
        let next = self.redo.pop()?;
        if self.undo.len() >= SKIN_HISTORY_LIMIT {
            self.undo.pop_front();
        }
        self.undo.push_back(current.to_vec());
        Some(next)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SkinFaceKey {
    part: SkinPart,
    face: usize,
}

#[derive(Clone, Debug)]
struct SkinPreviewFace {
    key: SkinFaceKey,
    points: [Pos2; 4],
    uv: Rect,
    depth: f32,
    tint: Color32,
}

impl SkinPreviewFace {
    fn texture_position(&self, point: Pos2) -> Option<Pos2> {
        let horizontal = self.points[1] - self.points[0];
        let vertical = self.points[3] - self.points[0];
        let determinant = horizontal.x * vertical.y - horizontal.y * vertical.x;
        if determinant.abs() < 0.0001 {
            return None;
        }
        let offset = point - self.points[0];
        let u = (offset.x * vertical.y - offset.y * vertical.x) / determinant;
        let v = (horizontal.x * offset.y - horizontal.y * offset.x) / determinant;
        if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
            return None;
        }
        Some(Pos2::new(
            self.uv.left() + self.uv.width() * u,
            self.uv.top() + self.uv.height() * v,
        ))
    }
}

#[derive(Clone, PartialEq, Eq)]
enum JavaPickerTarget {
    Global,
    Version { directory: PathBuf, profile: String },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsSection {
    General,
    Game,
    Versions,
    Launch,
    Appearance,
    About,
}

impl SettingsSection {
    const ALL: [Self; 6] = [
        Self::General,
        Self::Game,
        Self::Versions,
        Self::Launch,
        Self::Appearance,
        Self::About,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::General => "通用",
            Self::Game => "游戏",
            Self::Versions => "版本",
            Self::Launch => "启动",
            Self::Appearance => "外观",
            Self::About => "关于",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::General => "启动器行为与启动提醒",
            Self::Game => "Java、内存和启动参数",
            Self::Versions => "Minecraft 文件夹与版本隔离",
            Self::Launch => "窗口尺寸、全屏与启动行为",
            Self::Appearance => "动画、提示和玻璃效果",
            Self::About => "版本信息与支持的功能",
        }
    }

    fn index(self) -> usize {
        match self {
            Self::General => 0,
            Self::Game => 1,
            Self::Versions => 2,
            Self::Launch => 3,
            Self::Appearance => 4,
            Self::About => 5,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
struct LauncherSettings {
    memory_mb: u16,
    game_directory: Option<PathBuf>,
    game_directories: Vec<PathBuf>,
    version_isolation: bool,
    version_memory: Vec<String>,
    java_path: Option<PathBuf>,
    version_java: Vec<String>,
    show_loader_versions: bool,
    window_width: u32,
    window_height: u32,
    fullscreen: bool,
    extra_jvm_args: String,
    check_updates: bool,
    notify_after_launch: bool,
    hide_after_launch: bool,
    animations: bool,
    tooltips: bool,
    liquid_glass: bool,
    liquid_glass_mode: LiquidGlassMode,
    glass_transparency: u16,
    pointer_brightness: u16,
    pointer_radius: u16,
    pointer_refresh_rate: u16,
    favorite_sites: Vec<FavoriteSite>,
    recent_versions: Vec<RecentPlay>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LiquidGlassMode {
    Regular,
    Clear,
}

impl LiquidGlassMode {
    fn label(self) -> &'static str {
        match self {
            Self::Regular => "常规型",
            Self::Clear => "透明型",
        }
    }
}

impl Default for LauncherSettings {
    fn default() -> Self {
        Self {
            memory_mb: 6144,
            game_directory: None,
            game_directories: Vec::new(),
            version_isolation: true,
            version_memory: Vec::new(),
            java_path: None,
            version_java: Vec::new(),
            show_loader_versions: false,
            window_width: 1280,
            window_height: 720,
            fullscreen: false,
            extra_jvm_args: String::new(),
            check_updates: true,
            notify_after_launch: true,
            hide_after_launch: false,
            animations: true,
            tooltips: true,
            liquid_glass: false,
            liquid_glass_mode: LiquidGlassMode::Regular,
            glass_transparency: 82,
            pointer_brightness: 100,
            pointer_radius: 155,
            pointer_refresh_rate: 60,
            favorite_sites: Vec::new(),
            recent_versions: Vec::new(),
        }
    }
}

impl LauncherSettings {
    fn load() -> Self {
        let mut settings = Self::default();
        let current_path = settings_path();
        let legacy_path = legacy_settings_path();
        let (load_path, migrate_to_current) = if current_path.is_file() {
            (current_path, false)
        } else if legacy_path.is_file() {
            (legacy_path, true)
        } else {
            settings.game_directories = crate::game::discover_game_directories();
            settings.game_directory = settings.game_directories.first().cloned();
            return settings;
        };
        let Ok(contents) = std::fs::read_to_string(load_path) else {
            settings.game_directories = crate::game::discover_game_directories();
            settings.game_directory = settings.game_directories.first().cloned();
            return settings;
        };
        let mut settings_revision = 0;
        let mut stored_liquid_glass = None;
        for line in contents.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key {
                "memory_mb" => settings.memory_mb = value.parse().unwrap_or(settings.memory_mb),
                "game_directory" => {
                    if !value.is_empty() {
                        settings.game_directory = Some(PathBuf::from(value));
                    }
                }
                "game_directories" => {
                    settings.game_directories = value
                        .split('|')
                        .filter(|path| !path.is_empty())
                        .map(PathBuf::from)
                        .collect();
                }
                "version_isolation" => settings.version_isolation = value == "true",
                "java_path" => {
                    settings.java_path = (!value.is_empty()).then(|| PathBuf::from(value))
                }
                "show_loader_versions" => settings.show_loader_versions = value == "true",
                "isolated_version" => settings.version_isolation = true,
                "version_memory" => {
                    if let Some((profile, memory)) = value.rsplit_once('\u{1f}')
                        && let Ok(memory) = memory.parse::<u16>()
                    {
                        settings
                            .version_memory
                            .push(format!("{profile}\u{1f}{}", memory.clamp(2048, 16384)));
                    }
                }
                "version_java" => {
                    if let Some((profile, java)) = value.rsplit_once('\u{1f}')
                        && !profile.is_empty()
                        && !java.is_empty()
                    {
                        settings.version_java.push(format!("{profile}\u{1f}{java}"));
                    }
                }
                "window_width" => {
                    settings.window_width = value.parse().unwrap_or(settings.window_width)
                }
                "window_height" => {
                    settings.window_height = value.parse().unwrap_or(settings.window_height)
                }
                "fullscreen" => settings.fullscreen = value == "true",
                "jvm_args" => settings.extra_jvm_args = value.to_owned(),
                "check_updates" => settings.check_updates = value == "true",
                "notify_after_launch" => settings.notify_after_launch = value == "true",
                "hide_after_launch" => settings.hide_after_launch = value == "true",
                "animations" => settings.animations = value == "true",
                "tooltips" => settings.tooltips = value == "true",
                "settings_revision" => {
                    settings_revision = value.parse().unwrap_or_default();
                }
                "liquid_glass" => stored_liquid_glass = Some(value == "true"),
                "liquid_glass_mode" => {
                    settings.liquid_glass_mode = if value == "clear" {
                        LiquidGlassMode::Clear
                    } else {
                        LiquidGlassMode::Regular
                    }
                }
                "glass_transparency" => {
                    settings.glass_transparency =
                        value.parse().unwrap_or(settings.glass_transparency)
                }
                "pointer_brightness" => {
                    settings.pointer_brightness =
                        value.parse().unwrap_or(settings.pointer_brightness)
                }
                "pointer_radius" => {
                    settings.pointer_radius = value.parse().unwrap_or(settings.pointer_radius)
                }
                "pointer_refresh_rate" => {
                    settings.pointer_refresh_rate =
                        value.parse().unwrap_or(settings.pointer_refresh_rate)
                }
                "favorite_site" => {
                    if let Some(site) = parse_favorite_site(value) {
                        settings.favorite_sites.push(site);
                    }
                }
                "recent_version" => {
                    if let Some((played_at, version)) = value.split_once('\t')
                        && let Ok(played_at) = played_at.parse::<u64>()
                        && !version.is_empty()
                    {
                        settings.recent_versions.push(RecentPlay {
                            version: version.to_owned(),
                            played_at,
                        });
                    }
                }
                _ => {}
            }
        }
        settings.liquid_glass =
            resolve_liquid_glass_setting(settings_revision, stored_liquid_glass);
        settings.memory_mb = settings.memory_mb.clamp(2048, 16384);
        settings.window_width = settings.window_width.clamp(640, 7680);
        settings.window_height = settings.window_height.clamp(360, 4320);
        settings.glass_transparency = settings.glass_transparency.clamp(20, 100);
        settings.pointer_brightness = settings.pointer_brightness.clamp(0, 200);
        settings.pointer_radius = settings.pointer_radius.clamp(40, 320);
        settings.pointer_refresh_rate = settings.pointer_refresh_rate.clamp(20, 144);
        settings.extra_jvm_args = settings.extra_jvm_args.replace(['\r', '\n'], " ");
        settings
            .recent_versions
            .sort_by_key(|recent| std::cmp::Reverse(recent.played_at));
        settings
            .recent_versions
            .dedup_by(|left, right| left.version == right.version);
        settings.recent_versions.truncate(32);
        for directory in crate::game::discover_game_directories() {
            settings.add_game_directory(directory);
        }
        if let Some(directory) = settings.game_directory.clone() {
            settings.add_game_directory(directory);
        }
        settings
            .game_directories
            .retain(|directory| crate::game::is_game_directory(directory));
        if !settings
            .game_directory
            .as_deref()
            .is_some_and(crate::game::is_game_directory)
        {
            settings.game_directory = settings.game_directories.first().cloned();
        }
        if migrate_to_current {
            let _ = settings.save();
        }
        settings
    }

    fn add_game_directory(&mut self, directory: PathBuf) {
        if !crate::game::is_game_directory(&directory)
            || self
                .game_directories
                .iter()
                .any(|known| same_path(known, &directory))
        {
            return;
        }
        self.game_directories.push(directory);
        self.game_directories
            .sort_by(|left, right| left.to_string_lossy().cmp(&right.to_string_lossy()));
    }

    fn version_memory_mb(&self, directory: &std::path::Path, profile: &str) -> Option<u16> {
        let key = version_profile_key(directory, profile);
        self.version_memory.iter().find_map(|entry| {
            let (known_key, memory) = entry.rsplit_once('\u{1f}')?;
            (known_key.eq_ignore_ascii_case(&key))
                .then(|| memory.parse::<u16>().ok())
                .flatten()
        })
    }

    fn set_version_memory(&mut self, directory: &std::path::Path, profile: &str, memory_mb: u16) {
        let key = version_profile_key(directory, profile);
        self.version_memory
            .retain(|entry| !entry.starts_with(&format!("{key}\u{1f}")));
        self.version_memory
            .push(format!("{key}\u{1f}{}", memory_mb.clamp(2048, 16384)));
    }

    fn clear_version_memory(&mut self, directory: &std::path::Path, profile: &str) {
        let key = version_profile_key(directory, profile);
        self.version_memory
            .retain(|entry| !entry.starts_with(&format!("{key}\u{1f}")));
    }

    fn version_java_path(&self, directory: &std::path::Path, profile: &str) -> Option<PathBuf> {
        let key = version_profile_key(directory, profile);
        self.version_java.iter().find_map(|entry| {
            let (known_key, java) = entry.rsplit_once('\u{1f}')?;
            known_key
                .eq_ignore_ascii_case(&key)
                .then(|| PathBuf::from(java))
        })
    }

    fn set_version_java_path(
        &mut self,
        directory: &std::path::Path,
        profile: &str,
        java: Option<&std::path::Path>,
    ) {
        let key = version_profile_key(directory, profile);
        self.version_java
            .retain(|entry| !entry.starts_with(&format!("{key}\u{1f}")));
        if let Some(java) = java {
            self.version_java
                .push(format!("{key}\u{1f}{}", java.to_string_lossy()));
        }
    }

    fn save(&self) -> std::io::Result<()> {
        let path = settings_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut contents = format!(
            "memory_mb={}\ngame_directory={}\ngame_directories={}\nversion_isolation={}\njava_path={}\nshow_loader_versions={}\nwindow_width={}\nwindow_height={}\nfullscreen={}\njvm_args={}\ncheck_updates={}\nnotify_after_launch={}\nhide_after_launch={}\nanimations={}\ntooltips={}\nsettings_revision={}\nliquid_glass={}\nliquid_glass_mode={}\nglass_transparency={}\npointer_brightness={}\npointer_radius={}\npointer_refresh_rate={}\n",
            self.memory_mb,
            self.game_directory
                .as_ref()
                .map(|path| path.to_string_lossy())
                .unwrap_or_default(),
            self.game_directories
                .iter()
                .map(|path| path.to_string_lossy())
                .collect::<Vec<_>>()
                .join("|"),
            self.version_isolation,
            self.java_path
                .as_ref()
                .map(|path| path.to_string_lossy())
                .unwrap_or_default(),
            self.show_loader_versions,
            self.window_width,
            self.window_height,
            self.fullscreen,
            self.extra_jvm_args.replace(['\r', '\n'], " "),
            self.check_updates,
            self.notify_after_launch,
            self.hide_after_launch,
            self.animations,
            self.tooltips,
            SETTINGS_REVISION,
            self.liquid_glass,
            match self.liquid_glass_mode {
                LiquidGlassMode::Regular => "regular",
                LiquidGlassMode::Clear => "clear",
            },
            self.glass_transparency,
            self.pointer_brightness,
            self.pointer_radius,
            self.pointer_refresh_rate
        );
        for recent in &self.recent_versions {
            contents.push_str(&format!(
                "recent_version={}\t{}\n",
                recent.played_at,
                recent.version.replace(['\r', '\n', '\t'], " ")
            ));
        }
        for site in &self.favorite_sites {
            contents.push_str(&format!(
                "favorite_site={}\u{1f}{}\u{1f}{}\n",
                site.title.replace(['\r', '\n', '\u{1f}'], " "),
                site.url.replace(['\r', '\n', '\u{1f}'], ""),
                site.icon_source.replace(['\r', '\n', '\u{1f}'], "")
            ));
        }
        for memory in &self.version_memory {
            contents.push_str(&format!("version_memory={memory}\n"));
        }
        for java in &self.version_java {
            contents.push_str(&format!("version_java={java}\n"));
        }
        std::fs::write(path, contents)
    }
}

fn resolve_liquid_glass_setting(revision: u32, stored: Option<bool>) -> bool {
    if revision >= SETTINGS_REVISION {
        stored.unwrap_or(false)
    } else {
        false
    }
}

fn settings_path() -> PathBuf {
    brand::data_directory().join("settings.ini")
}

fn legacy_settings_path() -> PathBuf {
    brand::legacy_data_directory().join("settings.ini")
}

fn parse_favorite_site(value: &str) -> Option<FavoriteSite> {
    let mut fields = value.split('\u{1f}');
    let title = fields.next()?.trim();
    let url = fields.next()?.trim();
    let icon_source = fields.next().unwrap_or_default().trim();
    (!title.is_empty() && !url.is_empty()).then(|| FavoriteSite {
        title: title.to_owned(),
        url: url.to_owned(),
        icon_source: icon_source.to_owned(),
    })
}

fn same_path(left: &std::path::Path, right: &std::path::Path) -> bool {
    let left = left.to_string_lossy();
    let right = right.to_string_lossy();
    if cfg!(windows) {
        left.eq_ignore_ascii_case(&right)
    } else {
        left == right
    }
}

fn version_profile_key(directory: &std::path::Path, profile: &str) -> String {
    let directory = directory.to_string_lossy();
    let directory = if cfg!(windows) {
        directory.to_lowercase()
    } else {
        directory.into_owned()
    };
    format!("{directory}\u{1f}{profile}")
}

fn hero_background_path() -> Option<PathBuf> {
    let requested = PathBuf::from(r"D:\wdsjqdq\beijing.png");
    if requested.is_file() {
        return Some(requested);
    }
    let adjacent = std::env::current_exe().ok()?.parent()?.join("beijing.png");
    adjacent.is_file().then_some(adjacent)
}

fn load_texture_file(
    ctx: &egui::Context,
    path: &std::path::Path,
    name: &str,
) -> Option<egui::TextureHandle> {
    let metadata = std::fs::metadata(path).ok()?;
    if metadata.len() > 32 * 1024 * 1024 {
        return None;
    }
    let image = image::load_from_memory(&std::fs::read(path).ok()?).ok()?;
    let color_image = color_image_for_context(ctx, image);
    Some(ctx.load_texture(name, color_image, egui::TextureOptions::LINEAR))
}

fn load_embedded_texture(
    ctx: &egui::Context,
    bytes: &'static [u8],
    name: &str,
) -> Option<egui::TextureHandle> {
    let image = image::load_from_memory(bytes).ok()?;
    let color_image = color_image_for_context(ctx, image);
    Some(ctx.load_texture(name, color_image, egui::TextureOptions::LINEAR))
}

fn builtin_favorite_icon_assets() -> [(&'static str, &'static [u8], &'static str); 6] {
    [
        (
            "https://www.minecraft.net/",
            include_bytes!("../assets/favorite-icons/minecraft.png"),
            "favorite-icon:minecraft",
        ),
        (
            "https://zh.minecraft.wiki/",
            include_bytes!("../assets/favorite-icons/wiki.png"),
            "favorite-icon:wiki",
        ),
        (
            "https://modrinth.com/",
            include_bytes!("../assets/favorite-icons/modrinth.png"),
            "favorite-icon:modrinth",
        ),
        (
            "https://www.curseforge.com/minecraft",
            include_bytes!("../assets/favorite-icons/curseforge.png"),
            "favorite-icon:curseforge",
        ),
        (
            "https://www.mcmod.cn/",
            include_bytes!("../assets/favorite-icons/mcmod.png"),
            "favorite-icon:mcmod",
        ),
        (
            "https://www.planetminecraft.com/",
            include_bytes!("../assets/favorite-icons/planetminecraft.png"),
            "favorite-icon:planetminecraft",
        ),
    ]
}

fn color_image_for_context(ctx: &egui::Context, image: image::DynamicImage) -> egui::ColorImage {
    let max_side = ctx.input(|input| input.max_texture_side).max(1) as u32;
    let image = if image.width() > max_side || image.height() > max_side {
        let scale = max_side as f64 / image.width().max(image.height()) as f64;
        image.resize_exact(
            ((image.width() as f64 * scale).round() as u32).max(1),
            ((image.height() as f64 * scale).round() as u32).max(1),
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        image
    };
    let rgba = image.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw())
}

struct Toast {
    message: String,
    shown_at: Instant,
}

pub struct LauncherApp {
    selected_version: usize,
    menu: Menu,
    version_popup_value: f32,
    version_popup_velocity: f32,
    news: Option<usize>,
    launch_started: Option<Instant>,
    launch_complete: bool,
    toast: Option<Toast>,
    started_at: Instant,
    screenshot_path: Option<PathBuf>,
    screenshot_requested: bool,
    window_rounded: bool,
    game_process_id: Option<u32>,
    launch_without_recent: bool,
    preview_tooltip: bool,
    pointer_glow_preview: bool,
    pointer_glow_position: Pos2,
    pointer_glow_strength: f32,
    pointer_glow_initialized: bool,
    report_dpi: bool,
    dpi_reported: bool,
    page: Page,
    nav_selected: usize,
    nav_highlight_y: f32,
    nav_highlight_velocity: f32,
    nav_highlight_initialized: bool,
    nav_tooltip_id: u8,
    nav_tooltip_label: &'static str,
    nav_tooltip_button: Rect,
    nav_tooltip_value: f32,
    nav_tooltip_velocity: f32,
    nav_tooltip_target: bool,
    previous_page: Option<Page>,
    page_transition_progress: f32,
    page_transition_velocity: f32,
    page_transition_direction: f32,
    community_tab: usize,
    community_scroll: f32,
    favorite_editor_open: bool,
    favorite_title_input: String,
    favorite_url_input: String,
    favorite_icon_input: String,
    favorite_editor_error: Option<String>,
    favorite_editor_value: f32,
    favorite_editor_velocity: f32,
    favorite_editor_source: Rect,
    favorite_editor_target: Rect,
    favorite_editor_card_target: Rect,
    favorite_editor_target_scroll: f32,
    favorite_editor_pending_site: Option<FavoriteSite>,
    favorite_icon_receiver: Option<FavoriteIconReceiver>,
    favorite_icon_attempted: Vec<String>,
    favorite_icon_textures: HashMap<String, egui::TextureHandle>,
    previous_resource_detail: Option<usize>,
    resource_detail_progress: f32,
    resource_detail_velocity: f32,
    resource_detail_direction: f32,
    create_tab: usize,
    skin_pixels: Vec<u8>,
    skin_model_alex: bool,
    skin_color: [u8; 4],
    skin_erasing: bool,
    skin_preview_yaw: f32,
    skin_preview_pitch: f32,
    skin_texture: Option<egui::TextureHandle>,
    skin_dirty: bool,
    skin_last_pixel: Option<(usize, usize)>,
    skin_preview_last_face: Option<SkinFaceKey>,
    skin_history: SkinEditHistory,
    skin_stroke_before: Option<Vec<u8>>,
    skin_status: Option<String>,
    mod_name_input: String,
    mod_id_input: String,
    mod_package_input: String,
    mod_version_input: String,
    mod_minecraft_input: String,
    mod_loader_index: usize,
    mod_output_directory: Option<PathBuf>,
    mod_status: Option<String>,
    previous_create_tab: Option<usize>,
    create_transition_progress: f32,
    create_transition_velocity: f32,
    create_transition_direction: f32,
    create_tab_position: f32,
    create_tab_velocity: f32,
    settings_section: SettingsSection,
    settings_highlight_y: f32,
    settings_highlight_velocity: f32,
    settings_highlight_initialized: bool,
    previous_settings_section: Option<SettingsSection>,
    settings_transition_progress: f32,
    settings_transition_velocity: f32,
    settings_transition_direction: f32,
    settings: LauncherSettings,
    saved_settings: LauncherSettings,
    launch_settings_open: bool,
    launch_popup_value: f32,
    launch_popup_velocity: f32,
    hero_background_texture: Option<egui::TextureHandle>,
    release_icon_texture: Option<egui::TextureHandle>,
    snapshot_icon_texture: Option<egui::TextureHandle>,
    loader_icon_texture: Option<egui::TextureHandle>,
    java_runtimes: Vec<JavaRuntime>,
    java_picker_target: Option<JavaPickerTarget>,
    java_picker_anchor: Rect,
    java_picker_scroll: f32,
    #[allow(dead_code)]
    version_export_open: bool,
    export_include_mods: bool,
    export_include_config: bool,
    export_include_saves: bool,
    export_include_resourcepacks: bool,
    export_include_shaderpacks: bool,
    versions: Vec<GameVersion>,
    recent_sessions: Vec<RecentSession>,
    news_receiver: Option<std::sync::mpsc::Receiver<Result<Vec<MinecraftNews>, String>>>,
    news_items: Vec<MinecraftNews>,
    news_error: Option<String>,
    news_textures: Vec<Option<egui::TextureHandle>>,
    news_texture_keys: Vec<Option<String>>,
    launcher_update_receiver: Option<std::sync::mpsc::Receiver<Result<LauncherRelease, String>>>,
    launcher_update_result: Option<LauncherRelease>,
    launcher_update_error: Option<String>,
    launcher_update_manual: bool,
    version_scroll: f32,
    version_page_selected: usize,
    version_page_scroll: f32,
    version_folder_menu_open: bool,
    version_folder_scroll: f32,
    version_content_dialog: Option<VersionContentKind>,
    version_content_dialog_open: bool,
    version_content_popup_value: f32,
    version_content_popup_velocity: f32,
    version_content_popup_source: Rect,
    version_content_close_elapsed: f32,
    version_content_scroll: f32,
    previous_version_detail: Option<usize>,
    version_detail_progress: f32,
    version_detail_velocity: f32,
    version_detail_direction: f32,
    version_filter: VersionFilter,
    official_manifest:
        Option<std::sync::mpsc::Receiver<Result<Vec<crate::download::OfficialVersion>, String>>>,
    official_versions: Vec<crate::download::OfficialVersion>,
    official_error: Option<String>,
    official_browser_open: bool,
    official_selected: usize,
    official_scroll: f32,
    previous_download_detail: Option<usize>,
    download_detail_progress: f32,
    download_detail_velocity: f32,
    download_detail_direction: f32,
    download_loader_open: bool,
    download_loader_value: f32,
    download_loader_velocity: f32,
    download_search: String,
    download_resource_category: Option<ResourceCategory>,
    download_category_position: f32,
    download_category_velocity: f32,
    download_page_progress: f32,
    download_page_velocity: f32,
    download_page_direction: f32,
    download_switch_pending: Option<Option<ResourceCategory>>,
    download_switch_receiver: Option<std::sync::mpsc::Receiver<std::sync::Arc<egui::ColorImage>>>,
    download_switch_requested_at: Option<Instant>,
    download_switch_texture: Option<egui::TextureHandle>,
    download_switch_texture_size: [usize; 2],
    download_switch_queued: Option<Option<ResourceCategory>>,
    resource_results: Vec<ResourceProject>,
    resource_search_receiver:
        Option<std::sync::mpsc::Receiver<Result<Vec<ResourceProject>, String>>>,
    resource_search_error: Option<String>,
    resource_prefetched_key: Option<String>,
    resource_version_filter: String,
    resource_version_menu_open: bool,
    resource_version_anchor: Rect,
    resource_version_scroll: f32,
    resource_selected: usize,
    resource_scroll: f32,
    resource_download_receiver: Option<std::sync::mpsc::Receiver<Result<PathBuf, String>>>,
    resource_download_name: Option<String>,
    official_filter: VersionFilter,
    install_receiver: Option<std::sync::mpsc::Receiver<crate::download::InstallUpdate>>,
    install_progress: Option<crate::download::InstallUpdate>,
    install_version_id: Option<String>,
    install_launch_after: bool,
    modpack_import_receiver: Option<std::sync::mpsc::Receiver<crate::modpack::ModpackImportUpdate>>,
    modpack_import_update: Option<crate::modpack::ModpackImportUpdate>,
    loader_dialog_open: bool,
    loader_base_version: String,
    loader_preinstall: Option<crate::download::OfficialVersion>,
    pending_loader_after_base: Option<(LoaderKind, String)>,
    loader_kind: Option<LoaderKind>,
    loader_versions_receiver: Option<std::sync::mpsc::Receiver<Result<Vec<String>, String>>>,
    loader_versions: Vec<String>,
    loader_versions_error: Option<String>,
    loader_version_selected: usize,
    loader_version_scroll: f32,
    loader_install_receiver: Option<std::sync::mpsc::Receiver<LoaderInstallUpdate>>,
    loader_install_update: Option<LoaderInstallUpdate>,
}

impl LauncherApp {
    pub fn new(cc: &CreationContext<'_>) -> Self {
        theme::setup(&cc.egui_ctx);
        if let Some(gl) = cc.gl.clone() {
            theme::setup_glass(gl);
        }

        let args: Vec<String> = std::env::args().collect();
        let screenshot_path = args
            .iter()
            .position(|arg| arg == "--screenshot")
            .and_then(|index| args.get(index + 1))
            .map(PathBuf::from);
        let initial_menu = if args.iter().any(|arg| arg == "--profile-menu") {
            Menu::Profile
        } else if args.iter().any(|arg| arg == "--version-menu") {
            Menu::Version
        } else {
            Menu::None
        };
        let preview_tooltip = args.iter().any(|arg| arg == "--preview-tooltip");
        let pointer_glow_preview = args.iter().any(|arg| arg == "--pointer-glow");
        let report_dpi = args.iter().any(|arg| arg == "--dpi-report");
        let official_browser_open = args.iter().any(|arg| arg == "--official-browser");
        let launch_settings_open = args.iter().any(|arg| arg == "--launch-settings");
        let loader_dialog_open = args.iter().any(|arg| arg == "--loader-dialog");
        let download_loader_open = args.iter().any(|arg| arg == "--download-loader");
        let resource_version_menu_open = args.iter().any(|arg| arg == "--resource-version-menu");
        let favorite_editor_open = args.iter().any(|arg| arg == "--favorite-editor");
        let initial_create_tab = if args.iter().any(|arg| arg == "--create-mod") {
            1
        } else {
            0
        };
        let version_folder_menu_open = args.iter().any(|arg| arg == "--version-folders");
        let download_resource_category = args
            .iter()
            .position(|arg| arg == "--download-category")
            .and_then(|index| args.get(index + 1))
            .and_then(|category| match category.as_str() {
                "mod" => Some(ResourceCategory::Mod),
                "modpack" => Some(ResourceCategory::Modpack),
                "shader" => Some(ResourceCategory::Shader),
                "resourcepack" => Some(ResourceCategory::ResourcePack),
                "datapack" => Some(ResourceCategory::Datapack),
                _ => None,
            });
        let version_content_dialog = if args.iter().any(|arg| arg == "--version-export") {
            Some(VersionContentKind::Export)
        } else {
            args.iter()
                .position(|arg| arg == "--version-content")
                .and_then(|index| args.get(index + 1))
                .and_then(|kind| match kind.as_str() {
                    "mods" => Some(VersionContentKind::Mods),
                    "saves" => Some(VersionContentKind::Saves),
                    "shaders" => Some(VersionContentKind::ShaderPacks),
                    "resourcepacks" => Some(VersionContentKind::ResourcePacks),
                    "export" => Some(VersionContentKind::Export),
                    _ => None,
                })
        };
        let loader_base_version = args
            .iter()
            .position(|arg| arg == "--loader-base")
            .and_then(|index| args.get(index + 1))
            .cloned()
            .unwrap_or_else(|| "1.21.1".to_owned());
        let loader_kind = args
            .iter()
            .position(|arg| arg == "--loader-kind")
            .and_then(|index| args.get(index + 1))
            .and_then(|value| match value.as_str() {
                "fabric" => Some(LoaderKind::Fabric),
                "forge" => Some(LoaderKind::Forge),
                "neoforge" => Some(LoaderKind::NeoForge),
                _ => None,
            });
        let loader_versions_receiver = loader_kind
            .map(|loader| crate::loader::start_fetch_versions(loader_base_version.clone(), loader));
        let page = if args.iter().any(|arg| {
            matches!(
                arg.as_str(),
                "--settings"
                    | "--appearance"
                    | "--about"
                    | "--version-settings"
                    | "--game-settings"
            )
        }) {
            Page::Settings
        } else if args.iter().any(|arg| arg == "--download") {
            Page::Download
        } else if args.iter().any(|arg| arg == "--versions") || official_browser_open {
            Page::Versions
        } else if args.iter().any(|arg| arg == "--community") {
            Page::Community
        } else if args.iter().any(|arg| arg == "--create") {
            Page::Create
        } else {
            Page::Home
        };
        let settings_section = if args.iter().any(|arg| arg == "--about") {
            SettingsSection::About
        } else if args.iter().any(|arg| arg == "--version-settings") {
            SettingsSection::Versions
        } else if args.iter().any(|arg| arg == "--game-settings") {
            SettingsSection::Game
        } else if args.iter().any(|arg| arg == "--appearance") {
            SettingsSection::Appearance
        } else if args.iter().any(|arg| arg == "--launch-settings") {
            SettingsSection::Launch
        } else {
            SettingsSection::General
        };
        let mut settings = LauncherSettings::load();
        if args.iter().any(|arg| arg == "--no-liquid-glass") {
            settings.liquid_glass = false;
        }
        if args.iter().any(|arg| arg == "--clear-liquid-glass") {
            settings.liquid_glass_mode = LiquidGlassMode::Clear;
        }
        let initial_nav = match page {
            Page::Versions => 1,
            Page::Download => 2,
            Page::Community => 3,
            Page::Create => 4,
            Page::Settings => 5,
            Page::Home => 0,
        };
        let saved_settings = settings.clone();
        let hero_background_texture = hero_background_path()
            .and_then(|path| load_texture_file(&cc.egui_ctx, &path, "hero-background"));
        let release_icon_texture =
            load_embedded_texture(&cc.egui_ctx, include_bytes!("../Grass.png"), "release-icon");
        let snapshot_icon_texture =
            load_embedded_texture(&cc.egui_ctx, include_bytes!("../img.png"), "snapshot-icon");
        let loader_icon_texture =
            load_embedded_texture(&cc.egui_ctx, include_bytes!("../img_1.png"), "loader-icon");
        let mut favorite_icon_textures = HashMap::new();
        for (url, bytes, name) in builtin_favorite_icon_assets() {
            if let Some(texture) = load_embedded_texture(&cc.egui_ctx, bytes, name) {
                favorite_icon_textures.insert(url.to_owned(), texture);
            }
        }
        let versions = available_versions(settings.game_directory.as_deref());
        let java_runtimes = crate::game::discover_java_runtimes(&settings.game_directories);
        let recency = version_recency_map(&settings, settings.game_directory.as_deref());
        let selected_version = version_menu_order(&versions, &recency)
            .into_iter()
            .find(|index| versions[*index].launchable)
            .unwrap_or(0);
        let initial_resource_version = args
            .iter()
            .position(|arg| arg == "--resource-version")
            .and_then(|index| args.get(index + 1))
            .cloned()
            .unwrap_or_default();
        let initial_resource_category = download_resource_category;
        let resource_search_receiver = initial_resource_category.and_then(|category| {
            if initial_resource_version.is_empty() {
                return None;
            }
            let selected = versions.get(selected_version);
            let loader = selected
                .filter(|version| version.loader_family != "原版")
                .map(|version| version.loader_family.to_lowercase());
            Some(crate::resources::start_search(
                category,
                String::new(),
                initial_resource_version.clone(),
                loader,
            ))
        });
        let initial_profile = versions
            .get(selected_version)
            .map(|version| version.version.as_str())
            .unwrap_or_default();
        let initial_data_directory = settings.game_directory.as_ref().map(|base| {
            if settings.version_isolation {
                base.join("versions").join(initial_profile)
            } else {
                base.clone()
            }
        });
        let recent_sessions = initial_data_directory
            .as_deref()
            .map(crate::sessions::discover_recent_sessions)
            .unwrap_or_default();
        let official_manifest = Some(crate::download::start_manifest_fetch());
        let news_receiver = Some(crate::news::start_fetch());
        let launcher_update_receiver = settings
            .check_updates
            .then(|| update::start_latest_release_check(&cc.egui_ctx));

        Self {
            selected_version,
            menu: initial_menu,
            version_popup_value: 0.0,
            version_popup_velocity: 0.0,
            news: None,
            launch_started: None,
            launch_complete: false,
            toast: None,
            started_at: Instant::now(),
            screenshot_path,
            screenshot_requested: false,
            window_rounded: false,
            game_process_id: None,
            launch_without_recent: false,
            preview_tooltip,
            pointer_glow_preview,
            pointer_glow_position: Pos2::ZERO,
            pointer_glow_strength: 0.0,
            pointer_glow_initialized: false,
            report_dpi,
            dpi_reported: false,
            page,
            nav_selected: initial_nav,
            nav_highlight_y: 0.0,
            nav_highlight_velocity: 0.0,
            nav_highlight_initialized: false,
            nav_tooltip_id: u8::MAX,
            nav_tooltip_label: "",
            nav_tooltip_button: Rect::NOTHING,
            nav_tooltip_value: 0.0,
            nav_tooltip_velocity: 0.0,
            nav_tooltip_target: false,
            previous_page: None,
            page_transition_progress: 1.0,
            page_transition_velocity: 0.0,
            page_transition_direction: 1.0,
            community_tab: 0,
            community_scroll: 0.0,
            favorite_editor_open,
            favorite_title_input: String::new(),
            favorite_url_input: String::new(),
            favorite_icon_input: String::new(),
            favorite_editor_error: None,
            favorite_editor_value: if favorite_editor_open { 1.0 } else { 0.0 },
            favorite_editor_velocity: 0.0,
            favorite_editor_source: Rect::NOTHING,
            favorite_editor_target: Rect::NOTHING,
            favorite_editor_card_target: Rect::NOTHING,
            favorite_editor_target_scroll: 0.0,
            favorite_editor_pending_site: None,
            favorite_icon_receiver: None,
            favorite_icon_attempted: Vec::new(),
            favorite_icon_textures,
            previous_resource_detail: None,
            resource_detail_progress: 1.0,
            resource_detail_velocity: 0.0,
            resource_detail_direction: 1.0,
            create_tab: initial_create_tab,
            skin_pixels: default_skin_pixels(),
            skin_model_alex: false,
            skin_color: [76, 175, 80, 255],
            skin_erasing: false,
            skin_preview_yaw: -0.42,
            skin_preview_pitch: -0.12,
            skin_texture: None,
            skin_dirty: true,
            skin_last_pixel: None,
            skin_preview_last_face: None,
            skin_history: SkinEditHistory::default(),
            skin_stroke_before: None,
            skin_status: None,
            mod_name_input: "My Mod".to_owned(),
            mod_id_input: "mymod".to_owned(),
            mod_package_input: "com.example.mymod".to_owned(),
            mod_version_input: "1.0.0".to_owned(),
            mod_minecraft_input: versions
                .get(selected_version)
                .map(|version| version.game_version.clone())
                .unwrap_or_else(|| "1.21.1".to_owned()),
            mod_loader_index: 0,
            mod_output_directory: None,
            mod_status: None,
            previous_create_tab: None,
            create_transition_progress: 1.0,
            create_transition_velocity: 0.0,
            create_transition_direction: 1.0,
            create_tab_position: initial_create_tab as f32,
            create_tab_velocity: 0.0,
            settings_section,
            settings_highlight_y: 0.0,
            settings_highlight_velocity: 0.0,
            settings_highlight_initialized: false,
            previous_settings_section: None,
            settings_transition_progress: 1.0,
            settings_transition_velocity: 0.0,
            settings_transition_direction: 1.0,
            settings,
            saved_settings,
            launch_settings_open,
            launch_popup_value: 0.0,
            launch_popup_velocity: 0.0,
            hero_background_texture,
            release_icon_texture,
            snapshot_icon_texture,
            loader_icon_texture,
            java_runtimes,
            java_picker_target: None,
            java_picker_anchor: Rect::NOTHING,
            java_picker_scroll: 0.0,
            version_export_open: false,
            export_include_mods: true,
            export_include_config: true,
            export_include_saves: false,
            export_include_resourcepacks: true,
            export_include_shaderpacks: true,
            versions,
            recent_sessions,
            news_receiver,
            news_items: Vec::new(),
            news_error: None,
            news_textures: Vec::new(),
            news_texture_keys: Vec::new(),
            launcher_update_receiver,
            launcher_update_result: None,
            launcher_update_error: None,
            launcher_update_manual: false,
            version_scroll: 0.0,
            version_page_selected: selected_version,
            version_page_scroll: (selected_version as f32 * 64.0 - 180.0).max(0.0),
            version_folder_menu_open,
            version_folder_scroll: 0.0,
            version_content_dialog,
            version_content_dialog_open: version_content_dialog.is_some(),
            version_content_popup_value: if version_content_dialog.is_some() {
                1.0
            } else {
                0.0
            },
            version_content_popup_velocity: 0.0,
            version_content_popup_source: Rect::NOTHING,
            version_content_close_elapsed: 0.0,
            version_content_scroll: 0.0,
            previous_version_detail: None,
            version_detail_progress: 1.0,
            version_detail_velocity: 0.0,
            version_detail_direction: 1.0,
            version_filter: VersionFilter::All,
            official_manifest,
            official_versions: Vec::new(),
            official_error: None,
            official_browser_open,
            official_selected: 0,
            official_scroll: 0.0,
            previous_download_detail: None,
            download_detail_progress: 1.0,
            download_detail_velocity: 0.0,
            download_detail_direction: 1.0,
            download_loader_open,
            download_loader_value: if download_loader_open { 1.0 } else { 0.0 },
            download_loader_velocity: 0.0,
            download_search: String::new(),
            download_resource_category: initial_resource_category,
            download_category_position: resource_category_index(initial_resource_category) as f32,
            download_category_velocity: 0.0,
            download_page_progress: 1.0,
            download_page_velocity: 0.0,
            download_page_direction: 1.0,
            download_switch_pending: None,
            download_switch_receiver: None,
            download_switch_requested_at: None,
            download_switch_texture: None,
            download_switch_texture_size: [0, 0],
            download_switch_queued: None,
            resource_results: Vec::new(),
            resource_search_receiver,
            resource_search_error: None,
            resource_prefetched_key: None,
            resource_version_filter: initial_resource_version,
            resource_version_menu_open,
            resource_version_anchor: Rect::NOTHING,
            resource_version_scroll: 0.0,
            resource_selected: 0,
            resource_scroll: 0.0,
            resource_download_receiver: None,
            resource_download_name: None,
            official_filter: VersionFilter::All,
            install_receiver: None,
            install_progress: None,
            install_version_id: None,
            install_launch_after: false,
            modpack_import_receiver: None,
            modpack_import_update: None,
            loader_dialog_open,
            loader_base_version,
            loader_preinstall: None,
            pending_loader_after_base: None,
            loader_kind,
            loader_versions_receiver,
            loader_versions: Vec::new(),
            loader_versions_error: None,
            loader_version_selected: 0,
            loader_version_scroll: 0.0,
            loader_install_receiver: None,
            loader_install_update: None,
        }
    }

    fn version(&self) -> &GameVersion {
        &self.versions[self.selected_version]
    }

    fn version_icon(&self, version: &GameVersion) -> Option<&egui::TextureHandle> {
        if version.loader_family != "原版" {
            self.loader_icon_texture.as_ref()
        } else if is_snapshot_version(&version.version) {
            self.snapshot_icon_texture.as_ref()
        } else {
            self.release_icon_texture.as_ref()
        }
    }

    fn visible_loader_label(&self, version: &GameVersion) -> String {
        if version.loader_family == "原版" {
            String::new()
        } else if self.settings.show_loader_versions {
            version.loader.clone()
        } else {
            version.loader_family.clone()
        }
    }

    fn version_selector_label(&self, version: &GameVersion) -> String {
        let loader = self.visible_loader_label(version);
        if loader.is_empty() {
            version.game_version.clone()
        } else {
            format!("{} · {loader}", version.game_version)
        }
    }

    fn version_selector_detail(&self, version: &GameVersion) -> String {
        if self.settings.show_loader_versions {
            version.description.clone()
        } else if version.loader_family == "原版" {
            "原版 · 已安装 · 可启动".to_owned()
        } else {
            "已安装 · 可启动".to_owned()
        }
    }

    fn version_data_directory(&self) -> Option<PathBuf> {
        let profile = self.versions.get(self.selected_version)?.version.as_str();
        self.version_data_directory_for(profile)
    }

    fn version_data_directory_for(&self, profile: &str) -> Option<PathBuf> {
        let base = self.settings.game_directory.as_ref()?;
        if self.settings.version_isolation {
            Some(base.join("versions").join(profile))
        } else {
            Some(base.clone())
        }
    }

    fn java_path_for_version(&self, directory: &std::path::Path, profile: &str) -> Option<PathBuf> {
        self.settings
            .version_java_path(directory, profile)
            .or_else(|| self.settings.java_path.clone())
    }

    fn java_runtime_label(&self, path: &std::path::Path) -> String {
        if let Some(runtime) = self
            .java_runtimes
            .iter()
            .find(|runtime| same_path(&runtime.path, path))
        {
            format!(
                "Java {} · {}",
                runtime.major,
                runtime
                    .path
                    .parent()
                    .and_then(std::path::Path::parent)
                    .and_then(std::path::Path::file_name)
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| runtime.path.display().to_string())
            )
        } else {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string())
        }
    }

    fn refresh_java_runtimes(&mut self) {
        self.java_runtimes = crate::game::discover_java_runtimes(&self.settings.game_directories);
    }

    fn select_game_directory(&mut self, directory: PathBuf) {
        if !crate::game::is_game_directory(&directory) {
            self.show_toast(format!(
                "不是有效的 Minecraft 文件夹：{}",
                directory.display()
            ));
            return;
        }
        self.settings.add_game_directory(directory.clone());
        self.settings.game_directory = Some(directory);
        self.versions = available_versions(self.settings.game_directory.as_deref());
        self.selected_version = self
            .version_menu_order()
            .into_iter()
            .find(|index| {
                self.versions
                    .get(*index)
                    .is_some_and(|version| version.launchable)
            })
            .unwrap_or(0);
        self.version_page_selected = self.selected_version;
        self.version_page_scroll = 0.0;
        self.version_content_dialog = None;
        self.previous_version_detail = None;
        self.version_detail_progress = 1.0;
        self.refresh_recent_sessions();
        self.show_toast(format!(
            "已切换到 {}",
            self.settings
                .game_directory
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default()
        ));
    }

    fn set_version_isolation(&mut self, enabled: bool) {
        self.settings.version_isolation = enabled;
        if enabled && let Some(base) = self.settings.game_directory.as_ref() {
            for version in &self.versions {
                let directory = base.join("versions").join(&version.version);
                let _ = std::fs::create_dir_all(directory.join("mods"));
                let _ = std::fs::create_dir_all(directory.join("saves"));
            }
        }
        self.refresh_recent_sessions();
        self.show_toast(if enabled {
            "已为所有版本开启隔离"
        } else {
            "已关闭全部版本隔离"
        });
    }

    fn refresh_recent_sessions(&mut self) {
        self.recent_sessions = self
            .version_data_directory()
            .map(|directory| crate::sessions::discover_recent_sessions(&directory))
            .unwrap_or_default();
    }

    fn ensure_news_textures(&mut self, ctx: &egui::Context) {
        let keys: Vec<Option<String>> = self
            .news_items
            .iter()
            .take(3)
            .map(|news| news.image_key.clone())
            .collect();
        if keys == self.news_texture_keys {
            return;
        }
        self.news_texture_keys = keys;
        self.news_textures = Vec::with_capacity(3);
        for news in self.news_items.iter().take(3) {
            let Some(bytes) = news.image_bytes.as_deref() else {
                self.news_textures.push(None);
                continue;
            };
            let texture = image::load_from_memory(bytes).ok().map(|image| {
                let color_image = color_image_for_context(ctx, image);
                ctx.load_texture(
                    format!(
                        "minecraft-news-{}",
                        news.image_key.as_deref().unwrap_or(&news.id)
                    ),
                    color_image,
                    egui::TextureOptions::LINEAR,
                )
            });
            self.news_textures.push(texture);
        }
    }

    fn version_recency(&self) -> HashMap<String, u64> {
        version_recency_map(&self.settings, self.settings.game_directory.as_deref())
    }

    fn version_menu_order(&self) -> Vec<usize> {
        version_menu_order(&self.versions, &self.version_recency())
    }

    fn selectable_version_order(&self) -> Vec<usize> {
        self.version_menu_order()
            .into_iter()
            .filter(|index| self.versions[*index].launchable)
            .collect()
    }

    fn update_popup_animation(&mut self, ctx: &egui::Context) {
        let version_target = if self.menu == Menu::Version { 1.0 } else { 0.0 };
        let launch_target = if self.launch_settings_open { 1.0 } else { 0.0 };
        if !self.settings.animations {
            self.version_popup_value = version_target;
            self.version_popup_velocity = 0.0;
            self.launch_popup_value = launch_target;
            self.launch_popup_velocity = 0.0;
            return;
        }
        let delta = ctx.input(|input| input.stable_dt).clamp(0.001, 0.05);
        let version_active = spring_popup(
            &mut self.version_popup_value,
            &mut self.version_popup_velocity,
            version_target,
            delta,
        );
        let launch_active = spring_popup(
            &mut self.launch_popup_value,
            &mut self.launch_popup_velocity,
            launch_target,
            delta,
        );
        if version_active || launch_active {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn update_navigation_animation(&mut self, ctx: &egui::Context) {
        if !self.settings.animations {
            self.nav_highlight_velocity = 0.0;
            self.previous_page = None;
            self.page_transition_progress = 1.0;
            self.page_transition_velocity = 0.0;
            return;
        }
        let delta = ctx.input(|input| input.stable_dt).clamp(0.001, 0.05);
        let page_active = if self.previous_page.is_some() {
            let active = spring_smooth(
                &mut self.page_transition_progress,
                &mut self.page_transition_velocity,
                1.0,
                delta,
            );
            if !active {
                self.previous_page = None;
                self.page_transition_progress = 1.0;
            }
            active
        } else {
            false
        };
        if page_active {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn update_download_animations(&mut self, ctx: &egui::Context) {
        let category_target = resource_category_index(self.download_resource_category) as f32;
        if !self.settings.animations {
            self.download_category_position = category_target;
            self.download_category_velocity = 0.0;
            self.download_page_progress = 1.0;
            self.download_page_velocity = 0.0;
            self.download_switch_texture = None;
            self.download_switch_texture_size = [0, 0];
            self.download_switch_queued = None;
            return;
        }
        let delta = ctx.input(|input| input.stable_dt).clamp(0.001, 0.05);
        let category_active = spring_smooth(
            &mut self.download_category_position,
            &mut self.download_category_velocity,
            category_target,
            delta,
        );
        let page_active =
            if self.download_page_progress < 1.0 || self.download_page_velocity.abs() > 0.01 {
                spring_smooth(
                    &mut self.download_page_progress,
                    &mut self.download_page_velocity,
                    1.0,
                    delta,
                )
            } else {
                false
            };
        if !page_active {
            self.download_page_progress = 1.0;
            self.download_page_velocity = 0.0;
            self.download_switch_texture = None;
            self.download_switch_texture_size = [0, 0];
        }
        let queued = if page_active {
            None
        } else {
            self.download_switch_queued.take()
        };
        if let Some(category) = queued {
            self.begin_download_category_switch(ctx, category);
        } else if category_active || page_active {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn update_nav_tooltip(&mut self, ctx: &egui::Context, hovered: Option<NavTooltip>) {
        if let Some(tooltip) = hovered {
            if self.nav_tooltip_id != tooltip.id {
                self.nav_tooltip_id = tooltip.id;
                self.nav_tooltip_value = 0.0;
                self.nav_tooltip_velocity = 0.0;
            }
            self.nav_tooltip_label = tooltip.label;
            self.nav_tooltip_button = tooltip.button;
            self.nav_tooltip_target = true;
        } else {
            self.nav_tooltip_target = false;
        }
        let target = if self.nav_tooltip_target { 1.0 } else { 0.0 };
        if !self.settings.animations {
            self.nav_tooltip_value = target;
            self.nav_tooltip_velocity = 0.0;
        } else {
            let delta = ctx.input(|input| input.stable_dt).clamp(0.001, 0.05);
            if spring_smooth(
                &mut self.nav_tooltip_value,
                &mut self.nav_tooltip_velocity,
                target,
                delta,
            ) {
                ctx.request_repaint_after(Duration::from_millis(16));
            }
        }
    }

    fn update_settings_animation(&mut self, ctx: &egui::Context) {
        if !self.settings.animations {
            self.previous_settings_section = None;
            self.settings_transition_progress = 1.0;
            self.settings_transition_velocity = 0.0;
            return;
        }
        if self.previous_settings_section.is_none() {
            return;
        }
        let delta = ctx.input(|input| input.stable_dt).clamp(0.001, 0.05);
        let active = spring_smooth(
            &mut self.settings_transition_progress,
            &mut self.settings_transition_velocity,
            1.0,
            delta,
        );
        if !active {
            self.previous_settings_section = None;
            self.settings_transition_progress = 1.0;
        } else {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn update_detail_animations(&mut self, ctx: &egui::Context) {
        if !self.settings.animations {
            self.previous_version_detail = None;
            self.version_detail_progress = 1.0;
            self.version_detail_velocity = 0.0;
            self.previous_download_detail = None;
            self.download_detail_progress = 1.0;
            self.download_detail_velocity = 0.0;
            self.previous_resource_detail = None;
            self.resource_detail_progress = 1.0;
            self.resource_detail_velocity = 0.0;
            self.download_loader_value = if self.download_loader_open { 1.0 } else { 0.0 };
            self.download_loader_velocity = 0.0;
            return;
        }
        let delta = ctx.input(|input| input.stable_dt).clamp(0.001, 0.05);
        let mut repaint = false;
        if self.previous_version_detail.is_some() {
            let active = spring_smooth(
                &mut self.version_detail_progress,
                &mut self.version_detail_velocity,
                1.0,
                delta,
            );
            if active {
                repaint = true;
            } else {
                self.previous_version_detail = None;
                self.version_detail_progress = 1.0;
            }
        }
        if self.previous_download_detail.is_some() {
            let active = spring_smooth(
                &mut self.download_detail_progress,
                &mut self.download_detail_velocity,
                1.0,
                delta,
            );
            if active {
                repaint = true;
            } else {
                self.previous_download_detail = None;
                self.download_detail_progress = 1.0;
            }
        }
        if self.previous_resource_detail.is_some() {
            let active = spring_smooth(
                &mut self.resource_detail_progress,
                &mut self.resource_detail_velocity,
                1.0,
                delta,
            );
            if active {
                repaint = true;
            } else {
                self.previous_resource_detail = None;
                self.resource_detail_progress = 1.0;
            }
        }
        let loader_target = if self.download_loader_open { 1.0 } else { 0.0 };
        if spring_smooth(
            &mut self.download_loader_value,
            &mut self.download_loader_velocity,
            loader_target,
            delta,
        ) {
            repaint = true;
        }
        if repaint {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn update_version_content_animation(&mut self, ctx: &egui::Context) {
        if self.version_content_dialog.is_none() {
            return;
        }
        let target = if self.version_content_dialog_open {
            1.0
        } else {
            0.0
        };
        if !self.settings.animations {
            self.version_content_popup_value = target;
            self.version_content_popup_velocity = 0.0;
            self.version_content_close_elapsed = 0.0;
            if !self.version_content_dialog_open {
                self.version_content_dialog = None;
            }
            return;
        }
        let delta = ctx.input(|input| input.stable_dt).clamp(0.001, 0.05);
        if self.version_content_dialog_open {
            self.version_content_close_elapsed = 0.0;
            if spring_popup(
                &mut self.version_content_popup_value,
                &mut self.version_content_popup_velocity,
                target,
                delta,
            ) {
                ctx.request_repaint_after(Duration::from_millis(16));
            }
        } else if self.version_content_popup_value <= 0.002 {
            self.version_content_popup_value = 0.0;
            self.version_content_popup_velocity = 0.0;
            self.version_content_close_elapsed += delta;
            if self.version_content_close_elapsed >= CONTENT_CLOSE_BOUNCE_SECONDS {
                self.version_content_dialog = None;
                self.version_content_close_elapsed = 0.0;
            } else {
                ctx.request_repaint_after(Duration::from_millis(16));
            }
        } else {
            self.version_content_close_elapsed = 0.0;
            if spring_popup(
                &mut self.version_content_popup_value,
                &mut self.version_content_popup_velocity,
                target,
                delta,
            ) {
                ctx.request_repaint_after(Duration::from_millis(16));
            }
        }
    }

    fn start_resource_search(&mut self) {
        let Some(category) = self.download_resource_category else {
            return;
        };
        if self.resource_version_filter.trim().is_empty() {
            if let Some(latest) = self
                .official_versions
                .iter()
                .find(|version| version.kind == "release")
                .map(|version| version.id.clone())
            {
                self.resource_version_filter = latest;
            } else if self.official_manifest.is_some() {
                self.resource_search_error = Some("正在获取 Minecraft 版本列表，请稍候".to_owned());
                return;
            } else {
                self.resource_version_filter = self.version().game_version.clone();
            }
        }
        let loader = self.resource_loader();
        self.resource_search_receiver = Some(crate::resources::start_search(
            category,
            self.download_search.trim().to_owned(),
            self.resource_version_filter.clone(),
            loader,
        ));
        self.resource_results.clear();
        self.resource_search_error = None;
        self.resource_selected = 0;
        self.resource_scroll = 0.0;
        self.previous_resource_detail = None;
        self.resource_detail_progress = 1.0;
        self.resource_detail_velocity = 0.0;
    }

    fn resource_loader(&self) -> Option<String> {
        let version = self.version();
        matches!(
            self.download_resource_category,
            Some(ResourceCategory::Mod | ResourceCategory::Modpack)
        )
        .then(|| version.loader_family.to_lowercase())
        .filter(|loader| loader != "原版")
    }

    fn begin_download_category_switch(
        &mut self,
        ctx: &egui::Context,
        category: Option<ResourceCategory>,
    ) {
        if self.download_switch_pending.is_some() {
            if self.download_resource_category == category {
                self.download_switch_pending = None;
                self.download_switch_receiver = None;
                self.download_switch_requested_at = None;
            } else {
                self.download_switch_pending = Some(category);
            }
            ctx.request_repaint();
            return;
        }
        if self.download_resource_category == category {
            self.download_switch_queued = None;
            return;
        }
        if !self.settings.animations {
            self.download_switch_queued = None;
            self.apply_download_category_switch(category);
            return;
        }
        if self.download_page_progress < 0.999
            || self.download_page_velocity.abs() > 0.01
            || self.download_switch_texture.is_some()
        {
            self.download_switch_queued = Some(category);
            ctx.request_repaint();
            return;
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        let repaint_ctx = ctx.clone();
        ctx.request_screenshot(move |image| {
            let _ = sender.send(image);
            repaint_ctx.request_repaint();
        });
        self.download_switch_pending = Some(category);
        self.download_switch_receiver = Some(receiver);
        self.download_switch_requested_at = Some(Instant::now());
        ctx.request_repaint();
    }

    fn apply_download_category_switch(&mut self, category: Option<ResourceCategory>) {
        let previous_index = resource_category_index(self.download_resource_category);
        let next_index = resource_category_index(category);
        self.download_page_direction = if next_index > previous_index {
            1.0
        } else {
            -1.0
        };
        self.download_page_progress = 0.0;
        self.download_page_velocity = 0.0;
        self.download_resource_category = category;
        self.resource_version_menu_open = false;
        self.download_search.clear();
        if category.is_some() {
            self.start_resource_search();
        } else {
            self.resource_search_receiver = None;
            self.resource_results.clear();
            self.resource_search_error = None;
            self.resource_selected = 0;
            self.resource_scroll = 0.0;
        }
        self.download_switch_pending = None;
        self.download_switch_receiver = None;
        self.download_switch_requested_at = None;
    }

    fn poll_download_category_switch(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.download_switch_pending else {
            return;
        };
        let Some(receiver) = self.download_switch_receiver.take() else {
            self.apply_download_category_switch(pending);
            return;
        };
        match receiver.try_recv() {
            Ok(image) => {
                self.download_switch_texture_size = image.size;
                self.download_switch_texture = Some(ctx.load_texture(
                    "download-category-outgoing",
                    image.as_ref().clone(),
                    egui::TextureOptions::LINEAR,
                ));
                self.apply_download_category_switch(pending);
                ctx.request_repaint();
            }
            Err(std::sync::mpsc::TryRecvError::Empty)
                if self
                    .download_switch_requested_at
                    .is_some_and(|started| started.elapsed() < Duration::from_millis(550)) =>
            {
                self.download_switch_receiver = Some(receiver);
            }
            Err(std::sync::mpsc::TryRecvError::Empty)
            | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.apply_download_category_switch(pending);
                ctx.request_repaint();
            }
        }
    }

    fn prefetch_resource_categories(&mut self) {
        let Some(category) = self.download_resource_category else {
            return;
        };
        let game_version = self.resource_version_filter.clone();
        let loader = self.resource_loader();
        let prefetch_key = format!("{game_version}:{}", loader.as_deref().unwrap_or_default());
        if self.resource_prefetched_key.as_deref() == Some(&prefetch_key) {
            return;
        }
        self.resource_prefetched_key = Some(prefetch_key);
        crate::resources::start_prefetch(category, game_version, loader);
    }

    fn poll_resource_tasks(&mut self) {
        if let Some(receiver) = self.resource_search_receiver.take() {
            match receiver.try_recv() {
                Ok(Ok(results)) => {
                    self.resource_results = results;
                    self.resource_search_error = None;
                    self.resource_selected = 0;
                    self.resource_scroll = 0.0;
                    self.previous_resource_detail = None;
                    self.resource_detail_progress = 1.0;
                    self.resource_detail_velocity = 0.0;
                    self.prefetch_resource_categories();
                }
                Ok(Err(error)) => self.resource_search_error = Some(error),
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.resource_search_receiver = Some(receiver);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.resource_search_error = Some("资源下载源搜索连接已断开".to_owned());
                }
            }
        }
        if let Some(receiver) = self.resource_download_receiver.take() {
            match receiver.try_recv() {
                Ok(Ok(path)) => {
                    self.resource_download_name = None;
                    self.show_toast(format!("已下载到 {}", path.display()));
                }
                Ok(Err(error)) => {
                    self.resource_download_name = None;
                    self.show_toast(format!("资源下载失败：{error}"));
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.resource_download_receiver = Some(receiver);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.resource_download_name = None;
                    self.show_toast("资源下载连接已断开");
                }
            }
        }
    }

    fn ensure_favorite_icons(&mut self, ctx: &egui::Context) {
        if self.favorite_icon_receiver.is_some() {
            return;
        }
        let requests = self
            .all_favorite_sites()
            .into_iter()
            .filter(|site| {
                !self.favorite_icon_textures.contains_key(&site.url)
                    && !self
                        .favorite_icon_attempted
                        .iter()
                        .any(|known| known == &site.url)
            })
            .map(|site| crate::favorites::FavoriteIconRequest {
                key: site.url.clone(),
                site_url: site.url,
                icon_source: site.icon_source,
            })
            .collect::<Vec<_>>();
        if requests.is_empty() {
            return;
        }
        self.favorite_icon_attempted
            .extend(requests.iter().map(|request| request.key.clone()));
        self.favorite_icon_receiver = Some(crate::favorites::start_fetch_icons(requests));
        ctx.request_repaint();
    }

    fn poll_favorite_icons(&mut self, ctx: &egui::Context) {
        let Some(receiver) = self.favorite_icon_receiver.take() else {
            return;
        };
        match receiver.try_recv() {
            Ok(icons) => {
                for (url, bytes) in icons {
                    let Ok(image) = image::load_from_memory(&bytes) else {
                        continue;
                    };
                    let color_image = color_image_for_context(ctx, image);
                    let texture = ctx.load_texture(
                        format!("favorite-icon:{url}"),
                        color_image,
                        egui::TextureOptions::LINEAR,
                    );
                    self.favorite_icon_textures.insert(url, texture);
                }
                ctx.request_repaint();
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.favorite_icon_receiver = Some(receiver);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {}
        }
    }

    fn all_favorite_sites(&self) -> Vec<FavoriteSite> {
        let mut sites = crate::favorites::BUILTIN_SITES
            .iter()
            .map(|(title, url)| FavoriteSite {
                title: (*title).to_owned(),
                url: (*url).to_owned(),
                icon_source: String::new(),
            })
            .collect::<Vec<_>>();
        sites.extend(self.settings.favorite_sites.iter().cloned());
        sites
    }

    fn open_favorite_editor(&mut self, source: Rect) {
        self.favorite_editor_open = true;
        self.favorite_editor_value = 0.0;
        self.favorite_editor_velocity = 0.0;
        self.favorite_editor_source = source;
        self.favorite_editor_target = source;
        self.favorite_editor_pending_site = None;
        self.favorite_editor_error = None;
        self.favorite_title_input.clear();
        self.favorite_url_input.clear();
        self.favorite_icon_input.clear();
    }

    fn cancel_favorite_editor(&mut self) {
        if !self.favorite_editor_open {
            return;
        }
        self.favorite_editor_open = false;
        self.favorite_editor_target = self.favorite_editor_source;
        self.favorite_editor_pending_site = None;
        self.favorite_editor_error = None;
        if !self.settings.animations {
            self.finish_favorite_editor();
        }
    }

    fn add_favorite_site(&mut self, target_card: Rect) {
        let url = match crate::favorites::normalize_site_url(&self.favorite_url_input) {
            Ok(url) => url,
            Err(error) => {
                self.favorite_editor_error = Some(error);
                return;
            }
        };
        if self
            .all_favorite_sites()
            .iter()
            .any(|site| site.url.eq_ignore_ascii_case(&url))
        {
            self.favorite_editor_error = Some("这个网站已经收藏过了".to_owned());
            return;
        }
        let title = self.favorite_title_input.trim();
        let title = if title.is_empty() {
            crate::favorites::site_domain(&url)
        } else {
            title.to_owned()
        };
        let icon_source = match crate::favorites::normalize_icon_source(&self.favorite_icon_input) {
            Ok(source) => source,
            Err(error) => {
                self.favorite_editor_error = Some(error);
                return;
            }
        };
        self.favorite_editor_open = false;
        self.favorite_editor_target = target_card;
        self.community_scroll = self.favorite_editor_target_scroll;
        self.favorite_editor_pending_site = Some(FavoriteSite {
            title,
            url,
            icon_source,
        });
        self.favorite_editor_error = None;
        if !self.settings.animations {
            self.finish_favorite_editor();
        }
    }

    fn finish_favorite_editor(&mut self) {
        if let Some(site) = self.favorite_editor_pending_site.take() {
            let custom_icon = !site.icon_source.is_empty();
            self.settings.favorite_sites.push(site);
            self.show_toast(if custom_icon {
                "收藏网站已添加，正在加载自定义图标"
            } else {
                "收藏网站已添加，正在获取网站图标"
            });
        }
        self.favorite_title_input.clear();
        self.favorite_url_input.clear();
        self.favorite_icon_input.clear();
        self.favorite_editor_error = None;
        self.favorite_editor_value = 0.0;
        self.favorite_editor_velocity = 0.0;
        self.favorite_editor_source = Rect::NOTHING;
        self.favorite_editor_target = Rect::NOTHING;
    }

    fn update_favorite_editor_animation(&mut self, ctx: &egui::Context) {
        let target = if self.favorite_editor_open { 1.0 } else { 0.0 };
        if !self.settings.animations {
            self.favorite_editor_value = target;
            self.favorite_editor_velocity = 0.0;
            if !self.favorite_editor_open {
                self.finish_favorite_editor();
            }
            return;
        }
        if self.favorite_editor_value <= 0.002 && !self.favorite_editor_open {
            if self.favorite_editor_source != Rect::NOTHING {
                self.finish_favorite_editor();
            }
            return;
        }
        let delta = ctx.input(|input| input.stable_dt).clamp(0.001, 0.05);
        if spring_popup(
            &mut self.favorite_editor_value,
            &mut self.favorite_editor_velocity,
            target,
            delta,
        ) {
            ctx.request_repaint_after(Duration::from_millis(16));
        } else if !self.favorite_editor_open {
            self.finish_favorite_editor();
        }
    }

    fn switch_settings_section(&mut self, section: SettingsSection) {
        if self.settings_section == section {
            return;
        }
        if self.settings.animations {
            self.previous_settings_section = Some(self.settings_section);
            self.settings_transition_progress = 0.0;
            self.settings_transition_velocity = 0.0;
            self.settings_transition_direction = if section.index() > self.settings_section.index()
            {
                1.0
            } else {
                -1.0
            };
        } else {
            self.previous_settings_section = None;
            self.settings_transition_progress = 1.0;
            self.settings_transition_velocity = 0.0;
        }
        self.settings_section = section;
    }

    fn switch_create_tab(&mut self, index: usize) {
        if self.create_tab == index {
            return;
        }
        if self.settings.animations {
            self.previous_create_tab = Some(self.create_tab);
            self.create_transition_progress = 0.0;
            self.create_transition_velocity = 0.0;
            self.create_transition_direction = if index > self.create_tab { 1.0 } else { -1.0 };
        } else {
            self.previous_create_tab = None;
            self.create_transition_progress = 1.0;
            self.create_transition_velocity = 0.0;
            self.create_tab_position = index as f32;
            self.create_tab_velocity = 0.0;
        }
        self.create_tab = index;
    }

    fn update_create_animation(&mut self, ctx: &egui::Context) {
        if !self.settings.animations {
            self.previous_create_tab = None;
            self.create_transition_progress = 1.0;
            self.create_transition_velocity = 0.0;
            self.create_tab_position = self.create_tab as f32;
            self.create_tab_velocity = 0.0;
            return;
        }
        let delta = ctx.input(|input| input.stable_dt).clamp(0.001, 0.05);
        let tab_active = spring_smooth(
            &mut self.create_tab_position,
            &mut self.create_tab_velocity,
            self.create_tab as f32,
            delta,
        );
        let content_active = if self.previous_create_tab.is_some() {
            let active = spring_smooth(
                &mut self.create_transition_progress,
                &mut self.create_transition_velocity,
                1.0,
                delta,
            );
            if !active {
                self.previous_create_tab = None;
                self.create_transition_progress = 1.0;
            }
            active
        } else {
            false
        };
        if tab_active || content_active {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn switch_version_detail(&mut self, index: usize) {
        if self.version_page_selected == index {
            return;
        }
        if self.settings.animations {
            self.previous_version_detail = Some(self.version_page_selected);
            self.version_detail_progress = 0.0;
            self.version_detail_velocity = 0.0;
            self.version_detail_direction = if index > self.version_page_selected {
                1.0
            } else {
                -1.0
            };
        }
        self.version_page_selected = index;
        self.version_content_dialog = None;
        self.version_content_scroll = 0.0;
        self.version_export_open = false;
        self.refresh_recent_sessions();
    }

    fn open_version_content_dialog(&mut self, kind: VersionContentKind, source: Rect) {
        self.version_content_dialog = Some(kind);
        self.version_content_dialog_open = true;
        self.version_content_popup_value = 0.0;
        self.version_content_popup_velocity = 0.0;
        self.version_content_popup_source = source;
        self.version_content_close_elapsed = 0.0;
        self.version_content_scroll = 0.0;
    }

    fn close_version_content_dialog(&mut self) {
        self.version_content_close_elapsed = 0.0;
        self.version_content_dialog_open = false;
    }

    fn switch_download_detail(&mut self, index: usize) {
        if self.official_selected == index {
            return;
        }
        if let Some(version) = self.official_versions.get(index) {
            self.loader_base_version = version.id.clone();
        }
        if self.settings.animations {
            self.previous_download_detail = Some(self.official_selected);
            self.download_detail_progress = 0.0;
            self.download_detail_velocity = 0.0;
            self.download_detail_direction = if index > self.official_selected {
                1.0
            } else {
                -1.0
            };
        }
        self.reset_download_loader();
        self.official_selected = index;
    }

    fn switch_resource_detail(&mut self, index: usize) {
        if self.resource_selected == index {
            return;
        }
        if self.settings.animations {
            self.previous_resource_detail = Some(self.resource_selected);
            self.resource_detail_progress = 0.0;
            self.resource_detail_velocity = 0.0;
            self.resource_detail_direction = if index > self.resource_selected {
                1.0
            } else {
                -1.0
            };
        } else {
            self.previous_resource_detail = None;
            self.resource_detail_progress = 1.0;
            self.resource_detail_velocity = 0.0;
        }
        self.resource_selected = index;
    }

    fn reset_download_loader(&mut self) {
        self.download_loader_open = false;
        self.loader_kind = None;
        self.loader_versions_receiver = None;
        self.loader_versions.clear();
        self.loader_versions_error = None;
        self.loader_version_selected = 0;
        self.loader_version_scroll = 0.0;
    }

    fn switch_nav_page(&mut self, index: usize, page: Page) {
        if self.settings.animations && self.nav_selected != index {
            self.previous_page = Some(self.page);
            self.page_transition_progress = 0.0;
            self.page_transition_velocity = 0.0;
            self.page_transition_direction = if index > self.nav_selected { 1.0 } else { -1.0 };
        } else {
            self.previous_page = None;
            self.page_transition_progress = 1.0;
            self.page_transition_velocity = 0.0;
        }
        if page != Page::Settings {
            self.previous_settings_section = None;
            self.settings_transition_progress = 1.0;
            self.settings_transition_velocity = 0.0;
        }
        if page != Page::Versions {
            self.version_folder_menu_open = false;
            self.version_content_dialog = None;
            self.version_export_open = false;
        }
        if page != Page::Home && self.menu == Menu::Version {
            self.menu = Menu::None;
        }
        if page != Page::Download {
            self.resource_version_menu_open = false;
        }
        if page != Page::Community {
            self.favorite_editor_open = false;
            self.favorite_editor_pending_site = None;
            self.favorite_editor_value = 0.0;
            self.favorite_editor_velocity = 0.0;
            self.favorite_editor_source = Rect::NOTHING;
            self.favorite_editor_target = Rect::NOTHING;
            self.favorite_editor_error = None;
        }
        self.java_picker_target = None;
        self.nav_selected = index;
        self.page = page;
    }

    fn record_version_play(&mut self, version: &str) {
        let played_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        self.settings
            .recent_versions
            .retain(|recent| recent.version != version);
        self.settings.recent_versions.push(RecentPlay {
            version: version.to_owned(),
            played_at,
        });
        self.settings
            .recent_versions
            .sort_by_key(|recent| std::cmp::Reverse(recent.played_at));
        self.settings.recent_versions.truncate(32);
    }

    fn show_toast(&mut self, message: impl Into<String>) {
        self.toast = Some(Toast {
            message: message.into(),
            shown_at: Instant::now(),
        });
    }

    fn persist_settings(&mut self) {
        if self.settings == self.saved_settings {
            return;
        }
        match self.settings.save() {
            Ok(()) => {
                self.saved_settings = self.settings.clone();
            }
            Err(error) => {
                self.saved_settings = self.settings.clone();
                self.show_toast(format!("设置自动保存失败：{error}"));
            }
        }
    }

    fn refresh_official_manifest(&mut self) {
        if self.official_manifest.is_none() {
            self.official_error = None;
            self.official_manifest = Some(crate::download::start_manifest_fetch());
        }
    }

    fn start_launcher_update_check(&mut self, ctx: &egui::Context, manual: bool) {
        if self.launcher_update_receiver.is_some() {
            if manual {
                self.show_toast("正在检查 Genesis Launcher 更新");
            }
            return;
        }
        self.launcher_update_manual = manual;
        self.launcher_update_error = None;
        self.launcher_update_result = None;
        self.launcher_update_receiver = Some(update::start_latest_release_check(ctx));
        ctx.request_repaint();
    }

    fn poll_launcher_update(&mut self, ctx: &egui::Context) {
        let Some(receiver) = self.launcher_update_receiver.take() else {
            return;
        };
        match receiver.try_recv() {
            Ok(Ok(release)) => {
                let newer = update::is_newer_version(env!("CARGO_PKG_VERSION"), &release.version);
                self.launcher_update_result = Some(release.clone());
                self.launcher_update_error = None;
                if newer {
                    self.show_toast(format!("发现 Genesis Launcher 新版本 {}", release.version));
                } else if self.launcher_update_manual {
                    self.show_toast("Genesis Launcher 已是最新版本");
                }
                self.launcher_update_manual = false;
                ctx.request_repaint();
            }
            Ok(Err(error)) => {
                self.launcher_update_error = Some(error.clone());
                self.launcher_update_result = None;
                if self.launcher_update_manual {
                    self.show_toast(format!("检查更新失败：{error}"));
                }
                self.launcher_update_manual = false;
                ctx.request_repaint();
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.launcher_update_receiver = Some(receiver);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.launcher_update_error = Some("更新检查连接已断开".to_owned());
                self.launcher_update_manual = false;
            }
        }
    }

    fn start_version_install(&mut self, version_id: String, loader: &str, launch_after: bool) {
        if self.install_receiver.is_some() {
            self.show_toast("已有安装任务正在运行");
            return;
        }
        let Some(game_directory) = self.settings.game_directory.clone() else {
            self.show_toast("未找到游戏目录，无法下载版本");
            return;
        };
        self.install_progress = Some(crate::download::InstallUpdate {
            stage: "连接 Mojang 官方源".to_owned(),
            completed_bytes: 0,
            total_bytes: 0,
            files_done: 0,
            files_total: 0,
            bytes_per_second: 0,
            finished: false,
            success: false,
            message: String::new(),
        });
        let official_id = crate::game::official_base_version(&game_directory, &version_id, loader)
            .unwrap_or(version_id);
        self.install_version_id = Some(official_id.clone());
        self.install_launch_after = launch_after;
        self.install_receiver = Some(crate::download::start_install_by_id(
            game_directory,
            official_id,
        ));
    }

    fn start_official_install(
        &mut self,
        version: crate::download::OfficialVersion,
        launch_after: bool,
    ) {
        if self.install_receiver.is_some() {
            self.show_toast("已有安装任务正在运行");
            return;
        }
        let Some(game_directory) = self.settings.game_directory.clone() else {
            self.show_toast("未找到游戏目录，无法下载版本");
            return;
        };
        self.install_progress = Some(crate::download::InstallUpdate {
            stage: format!("准备安装 {}", version.id),
            completed_bytes: 0,
            total_bytes: 0,
            files_done: 0,
            files_total: 0,
            bytes_per_second: 0,
            finished: false,
            success: false,
            message: String::new(),
        });
        self.install_version_id = Some(version.id.clone());
        self.install_launch_after = launch_after;
        self.install_receiver = Some(crate::download::start_install(game_directory, version));
    }

    fn open_loader_dialog(&mut self, base_version: String) {
        self.loader_dialog_open = true;
        self.loader_base_version = base_version;
        self.loader_preinstall = None;
        self.pending_loader_after_base = None;
        self.loader_kind = None;
        self.loader_versions_receiver = None;
        self.loader_versions.clear();
        self.loader_versions_error = None;
        self.loader_version_selected = 0;
        self.loader_version_scroll = 0.0;
        self.loader_install_receiver = None;
        self.loader_install_update = None;
    }

    fn open_preinstall_loader_dialog(&mut self, version: crate::download::OfficialVersion) {
        self.open_loader_dialog(version.id.clone());
        self.loader_preinstall = Some(version);
    }

    fn choose_loader(&mut self, loader: LoaderKind) {
        self.loader_kind = Some(loader);
        self.loader_versions.clear();
        self.loader_versions_error = None;
        self.loader_version_selected = 0;
        self.loader_version_scroll = 0.0;
        self.loader_versions_receiver = Some(crate::loader::start_fetch_versions(
            self.loader_base_version.clone(),
            loader,
        ));
    }

    fn start_loader_install(&mut self) {
        let (Some(game_directory), Some(loader), Some(loader_version)) = (
            self.settings.game_directory.clone(),
            self.loader_kind,
            self.loader_versions
                .get(self.loader_version_selected)
                .cloned(),
        ) else {
            return;
        };
        if self.loader_install_receiver.is_some() {
            return;
        }
        self.loader_install_update = Some(LoaderInstallUpdate {
            stage: format!("准备安装 {}", loader.label()),
            finished: false,
            success: false,
            profile_hint: None,
            message: String::new(),
        });
        self.loader_install_receiver = Some(crate::loader::start_install(
            game_directory,
            self.loader_base_version.clone(),
            loader,
            loader_version,
        ));
    }

    fn start_modpack_import(&mut self) {
        if self.modpack_import_receiver.is_some() {
            return;
        }
        let Some(game_directory) = self.settings.game_directory.clone() else {
            self.show_toast("请先选择 Minecraft 游戏目录");
            return;
        };
        let Some(archive_path) = crate::modpack::pick_modpack_file() else {
            return;
        };
        if !self.settings.version_isolation {
            self.set_version_isolation(true);
        }
        self.modpack_import_update = Some(crate::modpack::ModpackImportUpdate {
            stage: "准备导入".to_owned(),
            message: archive_path.display().to_string(),
            finished: false,
            success: false,
            profile: None,
        });
        self.modpack_import_receiver =
            Some(crate::modpack::start_import(game_directory, archive_path));
    }

    fn begin_preinstall(&mut self, include_loader: bool) {
        let Some(version) = self.loader_preinstall.clone() else {
            return;
        };
        let choice = if include_loader {
            self.loader_kind.and_then(|loader| {
                self.loader_versions
                    .get(self.loader_version_selected)
                    .cloned()
                    .map(|loader_version| (loader, loader_version))
            })
        } else {
            None
        };
        self.pending_loader_after_base = choice;
        self.loader_preinstall = None;
        self.loader_dialog_open = false;
        self.loader_versions_receiver = None;
        self.start_official_install(version, false);
    }

    fn poll_loader_tasks(&mut self, ctx: &egui::Context) {
        if let Some(receiver) = self.loader_versions_receiver.take() {
            loop {
                match receiver.try_recv() {
                    Ok(Ok(versions)) => {
                        self.loader_versions = versions;
                        self.loader_versions_error = None;
                        self.loader_version_selected = 0;
                        self.loader_version_scroll = 0.0;
                        ctx.request_repaint();
                    }
                    Ok(Err(error)) => self.loader_versions_error = Some(error),
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        self.loader_versions_receiver = Some(receiver);
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
                }
            }
        }

        let mut finished = None;
        if let Some(receiver) = self.loader_install_receiver.take() {
            loop {
                match receiver.try_recv() {
                    Ok(update) => {
                        finished = update.finished.then_some(update.clone());
                        self.loader_install_update = Some(update);
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        self.loader_install_receiver = Some(receiver);
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        if finished.is_none() {
                            finished = Some(LoaderInstallUpdate {
                                stage: "加载器安装中断".to_owned(),
                                finished: true,
                                success: false,
                                profile_hint: None,
                                message: "安装器连接已断开".to_owned(),
                            });
                        }
                        break;
                    }
                }
            }
        }
        let Some(update) = finished else {
            return;
        };
        self.loader_install_receiver = None;
        if update.success {
            self.loader_dialog_open = false;
            self.loader_install_update = None;
            self.versions = available_versions(self.settings.game_directory.as_deref());
            let loader_label = self.loader_kind.map(LoaderKind::label).unwrap_or("");
            let selected = update
                .profile_hint
                .as_deref()
                .and_then(|profile| {
                    self.versions
                        .iter()
                        .position(|version| version.version == profile)
                })
                .or_else(|| {
                    self.versions.iter().position(|version| {
                        version.version.contains(&self.loader_base_version)
                            && version.version.contains(loader_label)
                    })
                })
                .or_else(|| self.versions.iter().position(|version| version.launchable))
                .unwrap_or(0);
            self.selected_version = selected;
            self.version_page_selected = selected;
            self.show_toast(update.message);
        } else {
            self.loader_install_update = Some(update);
        }
        ctx.request_repaint();
    }

    fn poll_modpack_import(&mut self, ctx: &egui::Context) {
        let Some(receiver) = self.modpack_import_receiver.take() else {
            return;
        };
        let mut finished = None;
        loop {
            match receiver.try_recv() {
                Ok(update) => {
                    finished = update.finished.then_some(update.clone());
                    self.modpack_import_update = Some(update);
                    ctx.request_repaint();
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.modpack_import_receiver = Some(receiver);
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    if finished.is_none() {
                        finished = Some(crate::modpack::ModpackImportUpdate {
                            stage: "导入中断".to_owned(),
                            message: "整合包导入连接已断开".to_owned(),
                            finished: true,
                            success: false,
                            profile: None,
                        });
                    }
                    break;
                }
            }
        }
        let Some(update) = finished else {
            return;
        };
        self.modpack_import_receiver = None;
        self.modpack_import_update = Some(update.clone());
        if update.success {
            self.versions = available_versions(self.settings.game_directory.as_deref());
            if let Some(profile) = update.profile.as_ref() {
                self.selected_version = self
                    .versions
                    .iter()
                    .position(|version| version.version == *profile)
                    .unwrap_or(self.selected_version);
                self.version_page_selected = self.selected_version;
            }
            self.refresh_recent_sessions();
            self.show_toast(update.message);
        } else {
            self.show_toast(format!("整合包导入失败：{}", update.message));
        }
        ctx.request_repaint();
    }

    fn poll_background_tasks(&mut self, ctx: &egui::Context) {
        self.poll_loader_tasks(ctx);
        self.poll_modpack_import(ctx);
        self.poll_launcher_update(ctx);
        if let Some(receiver) = self.news_receiver.take() {
            match receiver.try_recv() {
                Ok(Ok(news)) => {
                    self.news_items = news;
                    self.news_error = None;
                    self.news_textures.clear();
                    self.news_texture_keys.clear();
                    ctx.request_repaint();
                }
                Ok(Err(error)) => {
                    self.news_error = Some(error);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.news_receiver = Some(receiver);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.news_error = Some("Minecraft 动态连接已断开".to_owned());
                }
            }
        }
        if let Some(receiver) = self.official_manifest.take() {
            match receiver.try_recv() {
                Ok(Ok(versions)) => {
                    self.official_versions = versions;
                    self.official_selected = self
                        .official_versions
                        .iter()
                        .position(|version| version.kind == "release")
                        .unwrap_or(0);
                    self.official_error = None;
                    if self.resource_version_filter.is_empty()
                        && let Some(latest) = self
                            .official_versions
                            .iter()
                            .find(|version| version.kind == "release")
                            .map(|version| version.id.clone())
                    {
                        self.resource_version_filter = latest;
                        if self.download_resource_category.is_some()
                            && self.resource_search_receiver.is_none()
                        {
                            self.start_resource_search();
                        }
                    }
                    ctx.request_repaint();
                }
                Ok(Err(error)) => {
                    self.official_error = Some(error);
                    if self.resource_version_filter.is_empty()
                        && self.download_resource_category.is_some()
                    {
                        self.resource_version_filter = self.version().game_version.clone();
                        self.start_resource_search();
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.official_manifest = Some(receiver);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.official_error = Some("官方版本清单连接已断开".to_owned());
                }
            }
        }

        let mut finished = None;
        if let Some(receiver) = self.install_receiver.take() {
            loop {
                match receiver.try_recv() {
                    Ok(update) => {
                        finished = update.finished.then_some(update.clone());
                        self.install_progress = Some(update);
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        self.install_receiver = Some(receiver);
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        if finished.is_none() {
                            finished = Some(crate::download::InstallUpdate {
                                stage: "安装中断".to_owned(),
                                completed_bytes: 0,
                                total_bytes: 0,
                                files_done: 0,
                                files_total: 0,
                                bytes_per_second: 0,
                                finished: true,
                                success: false,
                                message: "安装任务意外结束".to_owned(),
                            });
                        }
                        break;
                    }
                }
            }
        }
        let Some(update) = finished else {
            return;
        };
        self.install_progress = None;
        self.install_receiver = None;
        let launch_after = self.install_launch_after;
        self.install_launch_after = false;
        let version_id = self.install_version_id.take();
        if update.success {
            self.versions = available_versions(self.settings.game_directory.as_deref());
            if !launch_after
                && let Some(version_id) = version_id.as_ref()
                && let Some(index) = self
                    .versions
                    .iter()
                    .position(|version| version.version == *version_id)
            {
                self.selected_version = index;
                self.version_page_selected = index;
            }
            self.show_toast(update.message);
            if launch_after {
                self.resume_launch_after_repair(ctx);
            } else if let Some((loader, loader_version)) = self.pending_loader_after_base.take() {
                self.loader_kind = Some(loader);
                self.loader_versions = vec![loader_version];
                self.loader_version_selected = 0;
                self.loader_dialog_open = false;
                self.loader_install_update = None;
                self.start_loader_install();
            }
        } else {
            self.pending_loader_after_base = None;
            self.show_toast(format!("文件补全失败：{}", update.message));
        }
    }

    fn update_timers(&mut self, ctx: &egui::Context) {
        self.poll_background_tasks(ctx);
        self.poll_resource_tasks();
        self.poll_favorite_icons(ctx);
        self.update_popup_animation(ctx);
        self.update_favorite_editor_animation(ctx);
        self.update_navigation_animation(ctx);
        self.poll_download_category_switch(ctx);
        self.update_download_animations(ctx);
        self.update_settings_animation(ctx);
        self.update_create_animation(ctx);
        self.update_detail_animations(ctx);
        self.update_version_content_animation(ctx);
        if self
            .game_process_id
            .is_some_and(|process_id| !crate::game::is_game_running(process_id))
        {
            self.game_process_id = None;
            self.launch_started = None;
            self.launch_complete = false;
            self.refresh_recent_sessions();
            if self.settings.notify_after_launch {
                self.show_toast("游戏已退出");
            }
        }
        if let Some(started) = self.launch_started
            && started.elapsed() >= Duration::from_millis(1550)
        {
            self.launch_started = None;
            self.launch_complete = true;
            if self.settings.notify_after_launch {
                self.show_toast("Minecraft 游戏进程已启动");
            }
            if self.settings.hide_after_launch {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
        }

        if self
            .toast
            .as_ref()
            .is_some_and(|toast| toast.shown_at.elapsed() >= Duration::from_secs(3))
        {
            self.toast = None;
        }

        let animate = self.settings.animations
            && (self.launch_started.is_some()
                || self.toast.is_some()
                || self.news.is_some()
                || self.menu != Menu::None
                || self.game_process_id.is_some()
                || self.news_receiver.is_some()
                || self.official_manifest.is_some()
                || self.install_receiver.is_some()
                || self.loader_versions_receiver.is_some()
                || self.loader_install_receiver.is_some()
                || self.modpack_import_receiver.is_some()
                || self.favorite_icon_receiver.is_some()
                || self.favorite_editor_value > 0.002
                || self.resource_search_receiver.is_some()
                || self.resource_download_receiver.is_some());
        ctx.request_repaint_after(if animate {
            Duration::from_millis(33)
        } else {
            Duration::from_millis(80)
        });
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            if self.java_picker_target.is_some() {
                self.java_picker_target = None;
            } else if self.version_export_open {
                self.version_export_open = false;
            } else if self.version_content_dialog.is_some() {
                self.close_version_content_dialog();
            } else if self.official_browser_open {
                self.official_browser_open = false;
            } else if self.news.is_some() {
                self.news = None;
            } else if self.launch_settings_open {
                self.launch_settings_open = false;
            } else if self.version_folder_menu_open {
                self.version_folder_menu_open = false;
            } else if self.loader_dialog_open && self.loader_install_receiver.is_none() {
                self.loader_dialog_open = false;
                self.loader_versions_receiver = None;
            } else if self.favorite_editor_open {
                self.cancel_favorite_editor();
            } else if self.page != Page::Home {
                self.switch_nav_page(0, Page::Home);
            } else {
                self.menu = Menu::None;
            }
        }

        if self.page == Page::Create && self.create_tab == 0 {
            let (redo, undo) = ctx.input(|input| {
                let command = input.modifiers.command;
                (
                    command && input.modifiers.shift && input.key_pressed(egui::Key::Z)
                        || command && input.key_pressed(egui::Key::Y),
                    command && !input.modifiers.shift && input.key_pressed(egui::Key::Z),
                )
            });
            if undo {
                self.undo_skin_edit();
                ctx.request_repaint();
            } else if redo {
                self.redo_skin_edit();
                ctx.request_repaint();
            }
        }

        if self.page == Page::Home
            && self.news.is_none()
            && ctx.input(|input| input.key_pressed(egui::Key::Enter))
            && self.launch_started.is_none()
        {
            self.start_launch(ctx);
        }
    }

    fn start_launch(&mut self, _ctx: &egui::Context) {
        let Some(game_directory) = self.settings.game_directory.clone() else {
            self.show_toast("未找到游戏目录，请先在设置中重新发现");
            return;
        };
        let version = self.version().clone();
        if self.install_receiver.is_some() {
            return;
        }
        if !crate::game::installation_complete(
            &game_directory,
            &version.version,
            &version.loader_family,
        ) {
            self.start_version_install(version.version, &version.loader_family, true);
            return;
        }
        let resume_recent = !self.launch_without_recent;
        let data_directory = self.version_data_directory();
        let memory_mb = self
            .settings
            .version_memory_mb(&game_directory, &version.version)
            .unwrap_or(self.settings.memory_mb);
        let java_path = self.java_path_for_version(&game_directory, &version.version);
        let launch_options = crate::game::LaunchOptions {
            max_memory_mb: memory_mb,
            window_width: self.settings.window_width,
            window_height: self.settings.window_height,
            fullscreen: self.settings.fullscreen,
            extra_jvm_args: self.settings.extra_jvm_args.clone(),
            quick_play: if resume_recent {
                self.recent_sessions
                    .first()
                    .map(|session| match session.kind {
                        SessionKind::World => {
                            crate::game::QuickPlayTarget::World(session.id.clone())
                        }
                        SessionKind::Server => {
                            crate::game::QuickPlayTarget::Server(session.id.clone())
                        }
                    })
            } else {
                None
            },
            data_directory,
            java_path,
        };
        match crate::game::launch(
            &game_directory,
            &version.version,
            &version.loader_family,
            &launch_options,
        ) {
            Ok(details) => {
                self.launch_without_recent = false;
                self.record_version_play(&version.version);
                self.game_process_id = Some(details.process_id);
                self.launch_started = Some(Instant::now());
                self.launch_complete = false;
                self.menu = Menu::None;
                self.show_toast(format!(
                    "正在启动 {} · {} · PID {}",
                    details.version, details.java_name, details.process_id
                ));
            }
            Err(error) => {
                self.launch_without_recent = false;
                self.game_process_id = None;
                self.launch_started = None;
                self.launch_complete = false;
                if ["缺少", "客户端文件", "游戏库", "资源索引", "读取版本配置"]
                    .iter()
                    .any(|pattern| error.contains(pattern))
                {
                    self.start_version_install(version.version, &version.loader_family, true);
                } else {
                    self.show_toast(format!("启动失败：{error}"));
                }
            }
        }
    }

    fn resume_launch_after_repair(&mut self, ctx: &egui::Context) {
        let Some(game_directory) = self.settings.game_directory.as_ref() else {
            return;
        };
        let version = self.version().clone();
        if crate::game::installation_complete(
            game_directory,
            &version.version,
            &version.loader_family,
        ) {
            self.start_launch(ctx);
        } else {
            self.show_toast(format!(
                "{} 的官方基础文件已补齐，但模组加载器文件仍不完整",
                version.version
            ));
        }
    }

    fn handle_screenshot(&mut self, ctx: &egui::Context) {
        if self.screenshot_requested {
            return;
        }
        let Some(path) = self.screenshot_path.clone() else {
            return;
        };
        if self.page == Page::Download
            && self.download_loader_open
            && self.loader_versions_receiver.is_some()
            && self.started_at.elapsed() < Duration::from_secs(8)
        {
            ctx.request_repaint_after(Duration::from_millis(50));
            return;
        }
        if self.page == Page::Download
            && self.resource_search_receiver.is_some()
            && self.started_at.elapsed() < Duration::from_secs(8)
        {
            ctx.request_repaint_after(Duration::from_millis(50));
            return;
        }
        if (matches!(self.page, Page::Home | Page::Download) || self.official_browser_open)
            && self.official_manifest.is_some()
            && self.started_at.elapsed() < Duration::from_secs(5)
        {
            ctx.request_repaint_after(Duration::from_millis(50));
            return;
        }
        if self.page == Page::Home
            && self.news_receiver.is_some()
            && self.started_at.elapsed() < Duration::from_secs(6)
        {
            ctx.request_repaint_after(Duration::from_millis(50));
            return;
        }
        if self.page == Page::Community
            && self.favorite_icon_receiver.is_some()
            && self.started_at.elapsed() < Duration::from_secs(8)
        {
            ctx.request_repaint_after(Duration::from_millis(50));
            return;
        }
        self.screenshot_requested = true;
        let close_ctx = ctx.clone();
        ctx.request_screenshot(move |image| {
            let [width, height] = image.size;
            let mut rgba = Vec::with_capacity(width * height * 4);
            for pixel in &image.pixels {
                rgba.extend_from_slice(&pixel.to_array());
            }
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = image::save_buffer(
                &path,
                &rgba,
                width as u32,
                height as u32,
                image::ColorType::Rgba8,
            );
            close_ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        });
        ctx.request_repaint();
    }

    fn paint_pointer_light(&mut self, ctx: &egui::Context, painter: &Painter, screen: Rect) {
        let target = if self.pointer_glow_preview {
            Some(Pos2::new(screen.right() - 120.0, screen.top() + 92.0))
        } else {
            ctx.input(|input| {
                input
                    .pointer
                    .hover_pos()
                    .filter(|position| screen.contains(*position))
            })
        };
        if let Some(target) = target {
            self.pointer_glow_position = target;
            self.pointer_glow_strength = 1.0;
            self.pointer_glow_initialized = true;
            if self.settings.pointer_brightness > 0 && ctx.input(|input| input.pointer.is_moving())
            {
                let refresh_rate = self.settings.pointer_refresh_rate.clamp(20, 144) as f32;
                ctx.request_repaint_after(Duration::from_secs_f32(1.0 / refresh_rate));
            }
        } else {
            self.pointer_glow_strength = 0.0;
        }
        if self.pointer_glow_strength > 0.01 && self.pointer_glow_initialized {
            theme::pointer_glow(
                painter,
                self.pointer_glow_position,
                self.pointer_glow_strength,
                screen,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_page_body(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        body_painter: &Painter,
        layout: &Layout,
        time: f64,
        page: Page,
        motion: CardMotion,
    ) {
        let painter = body_painter.clone();
        match page {
            Page::Home => {
                self.paint_hero(ctx, ui, &painter, layout, time, motion);
                self.paint_latest(ctx, ui, &painter, layout, time, motion);
            }
            Page::Versions => self.paint_versions(ctx, ui, &painter, layout, time, motion),
            Page::Download => self.paint_download(ctx, ui, &painter, layout, time, motion),
            Page::Community => self.paint_favorites(ctx, ui, &painter, layout, time, motion),
            Page::Create => self.paint_create_workspace(ctx, ui, &painter, layout, time, motion),
            Page::Settings => self.paint_settings(ctx, ui, &painter, layout, time, motion),
        }
    }

    fn render(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, screen: Rect) {
        theme::configure_effects(
            self.settings.glass_transparency,
            self.settings.pointer_brightness,
            self.settings.pointer_radius,
            self.settings.liquid_glass,
            self.settings.liquid_glass_mode == LiquidGlassMode::Clear,
        );
        let time = if self.screenshot_requested || !self.settings.animations {
            2.2
        } else {
            self.started_at.elapsed().as_secs_f64()
        };
        let painter = ui.painter().clone();
        let mut layout = Layout::new(screen);
        let version_label = self.version_selector_label(self.version());
        let max_trigger_width = (screen.width() * 0.34).clamp(220.0, 380.0);
        let version_label = fit_text_to_width(
            &painter,
            &version_label,
            theme::bold_font(10.5),
            (max_trigger_width - 86.0).max(100.0),
        );
        let version_label_width = painter
            .layout_no_wrap(
                version_label.clone(),
                theme::bold_font(10.5),
                theme::INK_SOFT,
            )
            .size()
            .x;
        let trigger_width = (50.0 + version_label_width + 36.0)
            .ceil()
            .clamp(170.0, max_trigger_width);
        layout.version_trigger = Rect::from_min_size(
            Pos2::new(
                layout.version_trigger.right() - trigger_width,
                layout.version_trigger.top(),
            ),
            Vec2::new(trigger_width, layout.version_trigger.height()),
        );
        self.handle_outside_click(ctx, &layout);

        painter.rect_filled(screen, 0, theme::BG);
        paint_ambient_background(&painter, screen, time);
        self.paint_pointer_light(ctx, &painter, screen);
        self.handle_window_frame(ctx, ui, &layout);
        if let Some(previous_page) = self.previous_page {
            let outgoing = CardMotion {
                screen: layout.screen,
                direction: self.page_transition_direction,
                progress: self.page_transition_progress,
                outgoing: true,
            };
            let mut outgoing_painter = painter.clone();
            outgoing_painter.set_opacity(outgoing_page_opacity(self.page_transition_progress));
            self.paint_page_body(
                ctx,
                ui,
                &outgoing_painter,
                &layout,
                time,
                previous_page,
                outgoing,
            );
            let incoming = CardMotion {
                screen: layout.screen,
                direction: self.page_transition_direction,
                progress: self.page_transition_progress,
                outgoing: false,
            };
            self.paint_page_body(ctx, ui, &painter, &layout, time, self.page, incoming);
        } else {
            self.paint_page_body(
                ctx,
                ui,
                &painter,
                &layout,
                time,
                self.page,
                CardMotion {
                    screen: layout.screen,
                    direction: 1.0,
                    progress: 1.0,
                    outgoing: false,
                },
            );
        }
        self.paint_titlebar(ctx, ui, &painter, &layout, time);
        self.paint_rail(ctx, ui, &painter, &layout, time);
        self.paint_header(ctx, ui, &painter, &layout, time);
        if self.version_folder_menu_open && self.page == Page::Versions {
            self.paint_version_folder_menu(ctx, &layout, time);
        }
        if self.launch_settings_open || self.launch_popup_value > 0.002 {
            let card = launch_settings_card_rect(&layout);
            let layer = egui::LayerId::new(
                egui::Order::Foreground,
                Id::unique("launch-settings-popup-layer"),
            );
            let scale = 0.82 + 0.18 * self.launch_popup_value.clamp(-0.05, 1.12);
            let anchor = card.right_bottom().to_vec2();
            let transform = egui::emath::TSTransform::new(anchor - anchor * scale, scale);
            let mut popup_painter = painter.clone().with_layer_id(layer);
            popup_painter.set_opacity(popup_opacity(
                self.launch_popup_value,
                self.launch_settings_open,
            ));
            self.paint_launch_settings_card(ui, &popup_painter, &layout, time);
            ctx.transform_layer_shapes(layer, transform);
        }
        if self.news.is_none() {
            self.paint_menus(ctx, &layout, time);
        } else {
            self.paint_news_dialog(ctx, ui, &painter, screen, time);
        }
        if let Some(kind) = self.version_content_dialog {
            self.paint_version_content_dialog(ctx, screen, time, kind);
        }
        if self.java_picker_target.is_some() {
            self.paint_java_picker(ctx, screen, time);
        }

        if self.official_browser_open {
            self.paint_official_browser(ui, &painter, screen, time);
        }
        if self.loader_dialog_open {
            self.paint_loader_dialog(ui, &painter, screen, time);
        }
        self.paint_install_progress(&painter, screen, time);
        self.paint_modpack_import_progress(&painter, screen, time);
        self.paint_toast(&painter, screen);
        if self.pointer_glow_strength > 0.01 {
            theme::pointer_glow_overlay(
                &painter,
                self.pointer_glow_position,
                self.pointer_glow_strength,
                screen,
            );
        }
    }

    fn handle_window_frame(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, layout: &Layout) {
        let drag_rect = Rect::from_min_max(
            Pos2::ZERO,
            Pos2::new(layout.screen.right() - 137.0, TITLEBAR_HEIGHT),
        );
        let drag = ui.interact(drag_rect, Id::unique("native-window-drag"), Sense::drag());
        if drag.drag_started() {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }

        let Some(pointer) = ctx.input(|input| input.pointer.interact_pos()) else {
            return;
        };
        if !layout.screen.contains(pointer) {
            return;
        }

        let edge = 6.0;
        let left = pointer.x <= edge;
        let right = pointer.x >= layout.screen.right() - edge;
        let top = pointer.y <= edge;
        let bottom = pointer.y >= layout.screen.bottom() - edge;
        let direction = match (left, right, top, bottom) {
            (true, _, true, _) => Some(egui::ResizeDirection::NorthWest),
            (_, true, true, _) => Some(egui::ResizeDirection::NorthEast),
            (true, _, _, true) => Some(egui::ResizeDirection::SouthWest),
            (_, true, _, true) => Some(egui::ResizeDirection::SouthEast),
            (true, _, _, _) => Some(egui::ResizeDirection::West),
            (_, true, _, _) => Some(egui::ResizeDirection::East),
            (_, _, true, _) => Some(egui::ResizeDirection::North),
            (_, _, _, true) => Some(egui::ResizeDirection::South),
            _ => None,
        };

        if let Some(direction) = direction {
            ctx.set_cursor_icon(match direction {
                egui::ResizeDirection::North | egui::ResizeDirection::South => {
                    CursorIcon::ResizeVertical
                }
                egui::ResizeDirection::East | egui::ResizeDirection::West => {
                    CursorIcon::ResizeHorizontal
                }
                egui::ResizeDirection::NorthEast | egui::ResizeDirection::SouthWest => {
                    CursorIcon::ResizeNeSw
                }
                egui::ResizeDirection::NorthWest | egui::ResizeDirection::SouthEast => {
                    CursorIcon::ResizeNwSe
                }
            });
            if ctx.input(|input| input.pointer.primary_pressed()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
            }
        }
    }

    fn paint_titlebar(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
    ) {
        theme::vertical_gradient(
            painter,
            layout.titlebar,
            Color32::from_rgba_unmultiplied(12, 18, 14, 248),
            Color32::from_rgba_unmultiplied(9, 14, 11, 226),
        );
        theme::soft_orb(
            painter,
            Pos2::new(
                layout.screen.center().x + (time * 0.08).sin() as f32 * 42.0,
                layout.titlebar.bottom(),
            ),
            170.0,
            theme::GREEN,
            7,
        );
        theme::horizontal_fade(
            painter,
            Rect::from_min_max(
                Pos2::new(
                    layout.titlebar.left() + 86.0,
                    layout.titlebar.bottom() - 1.0,
                ),
                Pos2::new(layout.titlebar.right() - 137.0, layout.titlebar.bottom()),
            ),
            theme::alpha(theme::GREEN, 10),
            theme::white_alpha(38),
            theme::alpha(theme::GREEN, 14),
        );
        painter.line_segment(
            [
                layout.titlebar.left_bottom(),
                layout.titlebar.right_bottom(),
            ],
            Stroke::new(1.0, theme::white_alpha(12)),
        );

        let title_icon = Rect::from_center_size(Pos2::new(23.0, 21.0), Vec2::splat(22.0));
        if let Some(texture) = self.release_icon_texture.as_ref() {
            paint_rounded_image(painter, title_icon, 6.0, texture, texture.size());
        } else {
            paint_block(
                painter,
                title_icon.center(),
                13.0,
                Color32::from_rgb(121, 210, 105),
                Color32::from_rgb(61, 142, 67),
                Color32::from_rgb(46, 104, 55),
            );
        }
        theme::bold_text(
            painter,
            Pos2::new(36.0, 13.0),
            brand::NAME,
            11.5,
            Color32::from_rgb(217, 226, 216),
        );
        let tag_rect = Rect::from_min_size(Pos2::new(145.0, 14.5), Vec2::new(55.0, 15.0));
        theme::fill_round(
            painter,
            tag_rect,
            theme::RADIUS,
            theme::alpha(theme::GREEN, 17),
        );
        theme::stroke_round(
            painter,
            tag_rect,
            theme::RADIUS,
            1.0,
            theme::alpha(theme::GREEN, 48),
        );
        theme::mono_text(
            painter,
            Pos2::new(153.0, 16.0),
            "PREVIEW",
            7.5,
            Color32::from_rgb(144, 198, 137),
        );

        painter.circle_filled(
            Pos2::new(layout.screen.center().x - 45.0, 21.0),
            2.6,
            Color32::from_rgb(105, 211, 98),
        );
        theme::text(
            painter,
            Pos2::new(layout.screen.center().x - 37.0, 14.0),
            if self.install_receiver.is_some() {
                "正在补全游戏文件"
            } else if self.game_process_id.is_some() {
                "游戏运行中"
            } else if self.launch_started.is_some() {
                "正在准备游戏"
            } else if self.launch_complete {
                "启动准备完成"
            } else {
                "准备就绪"
            },
            9.2,
            theme::MUTED,
        );

        let button_size = Vec2::new(45.0, TITLEBAR_HEIGHT);
        for (index, action) in WindowAction::ALL.into_iter().enumerate() {
            let rect = Rect::from_min_size(
                Pos2::new(
                    layout.screen.right() - button_size.x * (3 - index) as f32,
                    0.0,
                ),
                button_size,
            );
            let response = ui
                .interact(rect, Id::unique(("window", index)), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand);
            let hovered = response.hovered();
            if hovered {
                theme::fill_round(
                    painter,
                    Rect::from_center_size(
                        rect.center(),
                        Vec2::new(rect.width() - 8.0, rect.height() - 8.0),
                    ),
                    theme::RADIUS,
                    if matches!(action, WindowAction::Close) {
                        Color32::from_rgb(160, 57, 55)
                    } else {
                        theme::white_alpha(14)
                    },
                );
            }
            paint_window_icon(
                painter,
                rect.center(),
                action,
                if hovered { theme::INK } else { theme::MUTED },
            );
            if response.clicked() {
                match action {
                    WindowAction::Minimize => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true))
                    }
                    WindowAction::Maximize => {
                        let maximized =
                            ctx.input(|input| input.viewport().maximized.unwrap_or(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                    }
                    WindowAction::Close => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                }
            }
        }
    }

    fn paint_rail(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
    ) {
        let mut hovered_tooltip: Option<NavTooltip> = None;
        theme::vertical_gradient(
            painter,
            layout.rail,
            Color32::from_rgba_unmultiplied(16, 25, 19, 238),
            Color32::from_rgba_unmultiplied(10, 16, 12, 232),
        );
        theme::soft_orb(
            painter,
            Pos2::new(layout.rail.center().x, TITLEBAR_HEIGHT + 200.0),
            105.0,
            theme::GREEN,
            12,
        );
        painter.line_segment(
            [layout.rail.right_top(), layout.rail.right_bottom()],
            Stroke::new(1.0, theme::white_alpha(18)),
        );

        let logo = Rect::from_center_size(
            Pos2::new(layout.rail.center().x, TITLEBAR_HEIGHT + 40.0),
            Vec2::splat(50.0),
        );
        theme::glass_panel(
            painter,
            logo,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(28, 42, 31), theme::GREEN, time, 0.3),
        );
        let logo_content = card_clip(painter, logo);
        if let Some(texture) = self.release_icon_texture.as_ref() {
            paint_rounded_image(
                &logo_content,
                logo.shrink(8.0),
                12.0,
                texture,
                texture.size(),
            );
        } else {
            paint_block(
                &logo_content,
                logo.center() + Vec2::new(0.0, 1.0),
                28.0,
                Color32::from_rgb(133, 226, 114),
                Color32::from_rgb(85, 68, 50),
                Color32::from_rgb(68, 145, 70),
            );
        }

        let nav_top = logo.bottom() + 25.0;
        let nav_kinds = [
            NavKind::Home,
            NavKind::Versions,
            NavKind::Download,
            NavKind::Community,
            NavKind::Create,
        ];
        let settings_rect = Rect::from_center_size(
            Pos2::new(layout.rail.center().x, layout.rail_profile.top() - 30.5),
            Vec2::new(48.0, 47.0),
        );
        let target_y = if self.nav_selected < 5 {
            nav_top + 23.0 + self.nav_selected as f32 * 57.0
        } else {
            settings_rect.center().y
        };
        if !self.nav_highlight_initialized || !self.settings.animations {
            self.nav_highlight_y = target_y;
            self.nav_highlight_velocity = 0.0;
            self.nav_highlight_initialized = true;
        } else {
            let delta = ctx.input(|input| input.stable_dt).clamp(0.001, 0.05);
            let active = spring_smooth(
                &mut self.nav_highlight_y,
                &mut self.nav_highlight_velocity,
                target_y,
                delta,
            );
            if active {
                ctx.request_repaint_after(Duration::from_millis(16));
            }
        }
        let highlight = Rect::from_center_size(
            Pos2::new(layout.rail.center().x, self.nav_highlight_y),
            Vec2::new(48.0, 47.0),
        );
        theme::glass_panel(
            painter,
            highlight,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(27, 47, 31), theme::GREEN, time, 0.8),
        );
        painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(layout.rail.left() - 1.0, highlight.center().y - 9.5),
                Vec2::new(3.0, 19.0),
            ),
            theme::RADIUS,
            Color32::from_rgb(126, 227, 110),
        );
        for (index, kind) in nav_kinds.into_iter().enumerate() {
            let rect = Rect::from_center_size(
                Pos2::new(layout.rail.center().x, nav_top + 23.0 + index as f32 * 57.0),
                Vec2::new(48.0, 47.0),
            );
            let response = ui
                .interact(rect, Id::unique(("nav", index)), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand);
            let active = index == self.nav_selected;
            draw_icon(
                painter,
                NavIcon::from(kind),
                Rect::from_center_size(rect.center(), Vec2::splat(20.0)),
                if active {
                    Color32::from_rgb(215, 255, 203)
                } else if response.hovered() {
                    theme::INK_SOFT
                } else {
                    Color32::from_rgb(113, 128, 119)
                },
            );

            if self.settings.tooltips
                && (response.hovered() || (self.preview_tooltip && index == 1))
            {
                hovered_tooltip = Some(NavTooltip {
                    id: index as u8,
                    label: kind.label(),
                    button: rect,
                });
            }
            if response.clicked() {
                ctx.request_repaint();
                self.menu = Menu::None;
                self.news = None;
                match index {
                    0 => self.switch_nav_page(0, Page::Home),
                    1 => {
                        self.switch_nav_page(1, Page::Versions);
                        self.version_page_selected = self.selected_version;
                        self.version_page_scroll =
                            (self.selected_version as f32 * 64.0 - 180.0).max(0.0);
                    }
                    2 => {
                        self.switch_nav_page(2, Page::Download);
                        self.refresh_official_manifest();
                    }
                    3 => self.switch_nav_page(3, Page::Community),
                    _ => self.switch_nav_page(4, Page::Create),
                }
            }
        }

        let settings = ui
            .interact(settings_rect, Id::unique("settings"), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        let settings_active = self.page == Page::Settings;
        if settings.hovered() && self.settings.tooltips {
            hovered_tooltip = Some(NavTooltip {
                id: 5,
                label: "设置",
                button: settings_rect,
            });
        }
        draw_icon(
            painter,
            NavIcon::Settings,
            Rect::from_center_size(settings_rect.center(), Vec2::splat(20.0)),
            if settings_active {
                Color32::from_rgb(215, 255, 203)
            } else if settings.hovered() {
                theme::INK_SOFT
            } else {
                theme::MUTED
            },
        );
        if settings.clicked() {
            self.menu = Menu::None;
            self.news = None;
            self.switch_nav_page(5, Page::Settings);
            ctx.request_repaint();
        }

        let avatar = ui
            .interact(
                layout.rail_profile,
                Id::unique("rail-profile"),
                Sense::click(),
            )
            .on_hover_cursor(CursorIcon::PointingHand);
        theme::fill_round(
            painter,
            layout.rail_profile,
            theme::RADIUS,
            Color32::from_rgb(26, 36, 29),
        );
        theme::stroke_round(
            painter,
            layout.rail_profile,
            theme::RADIUS,
            1.0,
            if avatar.hovered() {
                theme::alpha(theme::GREEN, 80)
            } else {
                theme::white_alpha(26)
            },
        );
        paint_avatar(
            painter,
            Rect::from_center_size(layout.rail_profile.center(), Vec2::splat(29.0)),
            theme::RADIUS,
        );
        painter.circle_filled(
            layout.rail_profile.right_bottom() + Vec2::new(-4.0, -4.0),
            4.2,
            Color32::from_rgb(101, 218, 96),
        );
        if avatar.hovered() {
            hovered_tooltip = Some(NavTooltip {
                id: 6,
                label: "账户",
                button: layout.rail_profile,
            });
        }
        if avatar.clicked() {
            if self.menu == Menu::Profile {
                self.menu = Menu::None;
            } else {
                self.menu = Menu::Profile;
            }
        }
        self.update_nav_tooltip(ctx, hovered_tooltip);
        self.paint_nav_tooltip(ctx, layout, time);
    }

    fn paint_nav_tooltip(&self, ctx: &egui::Context, layout: &Layout, time: f64) {
        if self.nav_tooltip_value <= 0.002 {
            return;
        }
        let progress = self.nav_tooltip_value.clamp(0.0, 1.08);
        let eased = progress.clamp(0.0, 1.0);
        let eased = eased * eased * (3.0 - 2.0 * eased);
        let full_width = self.nav_tooltip_label.chars().count() as f32 * 8.2 + 38.0;
        let width = 22.0 + (full_width - 22.0) * eased;
        let height = 22.0 + 8.0 * eased;
        let origin = Pos2::new(
            self.nav_tooltip_button.right() + 4.0,
            self.nav_tooltip_button.center().y,
        );
        let target = Pos2::new(
            layout.rail.right() + 13.0 + full_width * 0.5,
            self.nav_tooltip_button.center().y,
        );
        let center = origin + (target - origin) * eased;
        let rect = Rect::from_center_size(center, Vec2::new(width, height));
        let opacity = (progress / 0.38).clamp(0.0, 1.0);
        egui::Area::new(Id::unique("animated-nav-tooltip-layer"))
            .order(egui::Order::Tooltip)
            .fixed_pos(rect.min)
            .interactable(false)
            .show(ctx, |ui| {
                let mut tooltip_painter = ui.painter().clone();
                tooltip_painter.set_opacity(opacity);
                theme::glass_panel(
                    &tooltip_painter,
                    rect,
                    (height * 0.5).round() as u8,
                    theme::GlassSpec::new(Color32::from_rgb(25, 39, 29), theme::GREEN, time, 10.8)
                        .backdrop_blur(12.0),
                );
                if eased > 0.42 {
                    tooltip_painter.set_opacity(opacity * ((eased - 0.42) / 0.58).clamp(0.0, 1.0));
                    let content = card_clip(&tooltip_painter, rect);
                    theme::center_text(
                        &content,
                        rect.center(),
                        self.nav_tooltip_label,
                        8.0,
                        theme::INK_SOFT,
                        true,
                    );
                }
            });
    }

    fn paint_header(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
    ) {
        let heading_painter = painter.clone();
        let game_directory_ready = self.settings.game_directory.is_some();
        if let Some(previous_page) = self.previous_page {
            let outgoing = CardMotion {
                screen: layout.screen,
                direction: self.page_transition_direction,
                progress: self.page_transition_progress,
                outgoing: true,
            };
            let incoming = CardMotion {
                screen: layout.screen,
                direction: self.page_transition_direction,
                progress: self.page_transition_progress,
                outgoing: false,
            };
            let mut outgoing_heading_painter = heading_painter.clone();
            outgoing_heading_painter
                .set_opacity(outgoing_page_opacity(self.page_transition_progress));
            paint_page_heading(
                &outgoing_heading_painter,
                layout,
                previous_page,
                game_directory_ready,
                outgoing,
            );
            paint_page_heading(
                &heading_painter,
                layout,
                self.page,
                game_directory_ready,
                incoming,
            );
        } else {
            paint_page_heading(
                &heading_painter,
                layout,
                self.page,
                game_directory_ready,
                CardMotion {
                    screen: layout.screen,
                    direction: 1.0,
                    progress: 1.0,
                    outgoing: false,
                },
            );
        }

        if let Some(previous_page) = self.previous_page {
            if previous_page == Page::Home {
                let outgoing = CardMotion {
                    screen: layout.screen,
                    direction: self.page_transition_direction,
                    progress: self.page_transition_progress,
                    outgoing: true,
                };
                let mut trigger_painter = painter.clone();
                trigger_painter.set_opacity(outgoing_page_opacity(self.page_transition_progress));
                self.paint_version_trigger(
                    ctx,
                    ui,
                    &trigger_painter,
                    layout,
                    outgoing.rect(layout.version_trigger),
                    time,
                    false,
                );
            }
            if self.page == Page::Home {
                let incoming = CardMotion {
                    screen: layout.screen,
                    direction: self.page_transition_direction,
                    progress: self.page_transition_progress,
                    outgoing: false,
                };
                self.paint_version_trigger(
                    ctx,
                    ui,
                    painter,
                    layout,
                    incoming.rect(layout.version_trigger),
                    time,
                    false,
                );
            }
        } else if self.page == Page::Home {
            self.paint_version_trigger(
                ctx,
                ui,
                painter,
                layout,
                layout.version_trigger,
                time,
                true,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_version_trigger(
        &mut self,
        _ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        trigger: Rect,
        time: f64,
        active: bool,
    ) {
        let response = active.then(|| {
            ui.interact(trigger, Id::unique("version-trigger"), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand)
        });
        let hovered = response.as_ref().is_some_and(|response| response.hovered());
        theme::glass_panel(
            painter,
            trigger,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(27, 39, 31), theme::GREEN, time, 1.2)
                .hovered(hovered),
        );
        let content = card_clip(painter, trigger);
        let painter = &content;

        let badge = Rect::from_center_size(
            Pos2::new(trigger.left() + 23.0, trigger.center().y),
            Vec2::new(34.0, 34.0),
        );
        theme::fill_round(painter, badge, theme::RADIUS, Color32::from_rgb(28, 45, 30));
        let version = self.version();
        if let Some(texture) = self.version_icon(version) {
            paint_rounded_image(painter, badge.shrink(3.0), 8.0, texture, texture.size());
        } else {
            paint_block(
                painter,
                badge.center(),
                22.0,
                Color32::from_rgb(135, 222, 111),
                Color32::from_rgb(91, 67, 47),
                Color32::from_rgb(78, 155, 76),
            );
        }
        theme::text(
            painter,
            Pos2::new(badge.right() + 10.0, trigger.top() + 7.0),
            "当前版本",
            7.2,
            theme::MUTED,
        );
        let version_label = self.version_selector_label(version);
        let version_label = fit_text_to_width(
            painter,
            &version_label,
            theme::bold_font(10.5),
            (trigger.width() - 94.0).max(70.0),
        );
        theme::bold_text(
            painter,
            Pos2::new(badge.right() + 10.0, trigger.top() + 20.0),
            version_label,
            10.5,
            theme::INK_SOFT,
        );
        draw_icon(
            painter,
            NavIcon::Chevron,
            Rect::from_center_size(
                Pos2::new(trigger.right() - 17.0, trigger.center().y),
                Vec2::splat(13.0),
            ),
            theme::MUTED,
        );
        if response.as_ref().is_some_and(|response| response.clicked()) {
            let opening = self.menu != Menu::Version;
            let selectable = self.selectable_version_order();
            if opening {
                let visible_height =
                    (version_menu_rect(layout, selectable.len()).height() - 39.0).max(1.0);
                let content_height = selectable.len() as f32 * VERSION_ROW_HEIGHT;
                let max_scroll = (content_height - visible_height).max(0.0);
                let selected_menu_index = selectable
                    .iter()
                    .position(|index| *index == self.selected_version)
                    .unwrap_or(0);
                self.version_scroll = (selected_menu_index as f32 * VERSION_ROW_HEIGHT
                    - (visible_height - VERSION_ROW_HEIGHT) * 0.5)
                    .clamp(0.0, max_scroll);
            }
            self.menu = if opening { Menu::Version } else { Menu::None };
        }
    }

    fn paint_hero(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
        motion: CardMotion,
    ) {
        let source_hero = layout.hero;
        let hero = motion.rect(source_hero);
        theme::fill_round(painter, hero, theme::RADIUS, Color32::from_rgb(14, 24, 19));
        let world_rect = hero.shrink(8.0);
        if let Some(background) = self.hero_background_texture.clone() {
            paint_rounded_image(
                painter,
                world_rect,
                theme::RADIUS as f32,
                &background,
                background.size(),
            );
            let image_layer = painter.clone();
            theme::horizontal_fade(
                &image_layer,
                world_rect,
                Color32::from_rgba_unmultiplied(6, 13, 9, 205),
                Color32::from_rgba_unmultiplied(7, 15, 10, 76),
                Color32::TRANSPARENT,
            );
            theme::vertical_gradient(
                &image_layer,
                Rect::from_min_max(
                    Pos2::new(
                        world_rect.left(),
                        world_rect.bottom() - world_rect.height() * 0.38,
                    ),
                    world_rect.right_bottom(),
                ),
                Color32::TRANSPARENT,
                Color32::from_rgba_unmultiplied(5, 11, 7, 176),
            );
        } else {
            scene::paint_world(painter, world_rect, time);
        }
        painter.rect_stroke(
            hero,
            theme::RADIUS,
            Stroke::new(8.0, Color32::from_rgb(14, 24, 19)),
            StrokeKind::Inside,
        );
        theme::stroke_round(painter, hero, theme::RADIUS, 1.0, theme::LINE_STRONG);
        theme::horizontal_fade(
            painter,
            Rect::from_min_max(
                Pos2::new(hero.left() + 30.0, hero.top() + 0.7),
                Pos2::new(hero.right() - 30.0, hero.top() + 1.6),
            ),
            Color32::TRANSPARENT,
            theme::white_alpha(76),
            theme::alpha(theme::GREEN, 20),
        );
        theme::soft_orb(
            painter,
            Pos2::new(hero.left() + 235.0, hero.top() + hero.height() * 0.42),
            210.0,
            theme::GREEN,
            10,
        );
        let hero_content = card_clip(painter, hero);
        let painter = &hero_content;

        let title_size = if hero.height() < 470.0 { 45.0 } else { 58.0 };
        let pad = 42.0;
        let badge = Rect::from_min_size(
            Pos2::new(hero.left() + pad, hero.top() + 34.0),
            Vec2::new(218.0, 28.0),
        );
        theme::glass_panel(
            painter,
            badge,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(15, 30, 21), theme::GREEN, time, 3.1),
        );
        painter.circle_filled(
            Pos2::new(badge.left() + 15.0, badge.center().y),
            9.5,
            theme::GREEN,
        );
        draw_icon(
            painter,
            NavIcon::Sparkles,
            Rect::from_center_size(
                Pos2::new(badge.left() + 15.0, badge.center().y),
                Vec2::splat(10.0),
            ),
            Color32::from_rgb(235, 255, 226),
        );
        let latest_status = if let Some(latest) = self
            .official_versions
            .iter()
            .find(|version| version.kind == "release")
        {
            format!("{} 现已正式可玩", latest.id)
        } else if self.official_manifest.is_some() {
            "正在获取最新版本".to_owned()
        } else {
            "最新版本暂不可用".to_owned()
        };
        theme::bold_text(
            painter,
            Pos2::new(badge.left() + 30.0, badge.top() + 7.0),
            fit_text_to_width(
                painter,
                &latest_status,
                theme::bold_font(8.4),
                (badge.width() - 93.0).max(70.0),
            ),
            8.4,
            theme::INK,
        );
        let update_button = Rect::from_min_size(
            Pos2::new(badge.right() - 57.0, badge.top() + 3.0),
            Vec2::new(52.0, 22.0),
        );
        let update_response = ui
            .interact(
                update_button,
                Id::unique("hero-view-update"),
                Sense::click(),
            )
            .on_hover_cursor(CursorIcon::PointingHand);
        if update_response.hovered() {
            theme::fill_round(
                painter,
                update_button,
                theme::RADIUS,
                theme::white_alpha(12),
            );
        }
        painter.line_segment(
            [
                Pos2::new(badge.right() - 54.0, badge.top() + 8.0),
                Pos2::new(badge.right() - 54.0, badge.bottom() - 8.0),
            ],
            Stroke::new(1.0, theme::white_alpha(32)),
        );
        theme::text(
            painter,
            Pos2::new(badge.right() - 45.0, badge.top() + 7.0),
            "查看更新",
            8.0,
            if update_response.hovered() {
                theme::GREEN_BRIGHT
            } else {
                Color32::from_rgb(168, 220, 162)
            },
        );
        if update_response.clicked() {
            self.official_filter = VersionFilter::Release;
            self.official_scroll = 0.0;
            if let Some(index) = self
                .official_versions
                .iter()
                .position(|version| version.kind == "release")
            {
                self.official_selected = index;
            }
            self.previous_download_detail = None;
            self.download_detail_progress = 1.0;
            self.switch_nav_page(2, Page::Download);
            self.refresh_official_manifest();
        }

        let title_y = hero.top() + 76.0;
        theme::bold_text(
            painter,
            Pos2::new(hero.left() + pad, title_y),
            "世界，",
            title_size,
            Color32::from_rgb(245, 250, 238),
        );
        theme::bold_text(
            painter,
            Pos2::new(hero.left() + pad, title_y + title_size * 1.02),
            "等你回来。",
            title_size,
            Color32::from_rgb(217, 245, 191),
        );
        let description_y = title_y + title_size * 2.22;
        theme::wrapped_text(
            painter,
            Pos2::new(hero.left() + pad + 1.0, description_y),
            "暮色穿过树影，熟悉的方块世界已经准备好。\n整理行囊，继续你的下一段冒险。",
            10.2,
            theme::alpha(Color32::from_rgb(235, 244, 230), 184),
            (hero.width() * 0.43).max(320.0),
        );

        let meta_y = description_y + 48.0;
        let version = self.version().clone();
        let data_directory = self.version_data_directory().unwrap_or_default();
        let mod_count = installed_mod_files(&data_directory).len();
        let shader_count = count_content_packs(&data_directory.join("shaderpacks"));
        let java_label = self
            .settings
            .game_directory
            .as_ref()
            .and_then(|directory| self.java_path_for_version(directory, &version.version))
            .and_then(|path| {
                self.java_runtimes
                    .iter()
                    .find(|runtime| same_path(&runtime.path, &path))
                    .map(|runtime| format!("Java {}", runtime.major))
            })
            .unwrap_or_else(|| format!("自动 {}", version.java));
        let meta = [
            (NavIcon::Box, format!("{mod_count} 个模组")),
            (
                NavIcon::Sparkles,
                if shader_count == 0 {
                    "无光影".to_owned()
                } else {
                    format!("{shader_count} 个光影")
                },
            ),
            (NavIcon::Monitor, java_label),
        ];
        let mut x = hero.left() + pad;
        for (index, (icon, label)) in meta.into_iter().enumerate() {
            let width = 37.0 + label.chars().count() as f32 * 8.2;
            let chip = Rect::from_min_size(Pos2::new(x, meta_y), Vec2::new(width, 23.0));
            theme::glass_panel(
                painter,
                chip,
                theme::RADIUS,
                theme::GlassSpec::new(
                    Color32::from_rgb(14, 29, 19),
                    theme::GREEN,
                    time,
                    10.2 + index as f32 * 0.3,
                ),
            );
            draw_icon(
                painter,
                icon,
                Rect::from_center_size(
                    Pos2::new(chip.left() + 12.0, chip.center().y),
                    Vec2::splat(11.0),
                ),
                Color32::from_rgb(158, 200, 153),
            );
            theme::text(
                painter,
                Pos2::new(chip.left() + 21.0, chip.top() + 5.3),
                label,
                7.7,
                Color32::from_rgb(207, 222, 202),
            );
            x = chip.right() + 8.0;
        }

        if hero.width() > 1010.0 {
            self.paint_server_card(ctx, ui, painter, source_hero, time, motion);
        }

        self.paint_launch_dock(ctx, ui, painter, source_hero, time, motion);
    }

    fn paint_server_card(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        hero: Rect,
        time: f64,
        motion: CardMotion,
    ) {
        let recent = self.recent_sessions.first().cloned();
        let is_server = recent
            .as_ref()
            .is_some_and(|session| session.kind == SessionKind::Server);
        let accent = if is_server {
            Color32::from_rgb(89, 190, 168)
        } else {
            Color32::from_rgb(137, 211, 111)
        };
        let rect = motion.rect(Rect::from_min_size(
            Pos2::new(hero.right() - 274.0, hero.top() + 28.0),
            RECENT_CARD_SIZE,
        ));
        theme::glass_panel(
            painter,
            rect,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(15, 28, 21), accent, time, 4.5),
        );
        let content = card_clip(painter, rect);
        let painter = &content;
        theme::soft_orb(
            &content,
            Pos2::new(rect.right() - 35.0, rect.top() + 53.0),
            76.0,
            accent,
            11,
        );
        let recent_icon = Rect::from_center_size(
            Pos2::new(rect.left() + 27.0, rect.top() + 31.0),
            Vec2::splat(36.0),
        );
        let recent_version = recent
            .as_ref()
            .and_then(|session| session.version.as_deref());
        let icon_version = recent_version
            .and_then(|session_version| {
                self.versions
                    .iter()
                    .find(|version| version.version == session_version)
                    .or_else(|| {
                        self.versions.iter().find(|version| {
                            version.game_version == session_version && version.launchable
                        })
                    })
            })
            .cloned()
            .unwrap_or_else(|| self.version().clone());
        if let Some(texture) = self.version_icon(&icon_version) {
            paint_rounded_image(painter, recent_icon, 10.0, texture, texture.size());
        } else {
            theme::fill_round(
                painter,
                recent_icon,
                theme::RADIUS,
                theme::alpha(accent, 32),
            );
            draw_icon(
                painter,
                NavIcon::Box,
                Rect::from_center_size(recent_icon.center(), Vec2::splat(21.0)),
                accent,
            );
        }
        theme::bold_text(
            painter,
            Pos2::new(rect.left() + 51.0, rect.top() + 16.0),
            "最近游玩",
            8.0,
            Color32::from_rgb(201, 219, 204),
        );
        painter.text(
            Pos2::new(rect.right() - 14.0, rect.top() + 17.0),
            egui::Align2::RIGHT_TOP,
            recent
                .as_ref()
                .and_then(|session| session.last_played)
                .map(relative_play_time)
                .unwrap_or_else(|| "尚无记录".to_owned()),
            theme::font(7.2),
            Color32::from_rgb(185, 224, 188),
        );
        let title = recent
            .as_ref()
            .map(|session| ellipsize_end(&session.name, 19))
            .unwrap_or_else(|| "还没有游玩记录".to_owned());
        let subtitle = match &recent {
            Some(session) => match session.kind {
                SessionKind::World => "单人存档".to_owned(),
                SessionKind::Server => session
                    .address
                    .as_deref()
                    .map(|address| ellipsize_end(address, 28))
                    .unwrap_or_else(|| "最近连接的服务器".to_owned()),
            },
            None => "创建世界或添加服务器后会显示在这里".to_owned(),
        };
        theme::bold_text(
            painter,
            Pos2::new(rect.left() + 16.0, rect.top() + 57.0),
            title,
            17.0,
            Color32::WHITE,
        );
        theme::text(
            painter,
            Pos2::new(rect.left() + 16.0, rect.top() + 82.0),
            subtitle,
            8.0,
            Color32::from_rgb(186, 207, 190),
        );

        let recent_version = recent
            .as_ref()
            .and_then(|session| session.version.as_deref())
            .unwrap_or_else(|| self.version().version.as_str());
        let version_label = if is_server {
            format!("服务器 · {recent_version}")
        } else {
            format!("存档 · {recent_version}")
        };
        let version_width = version_label.chars().count() as f32 * 7.0 + 18.0;
        let version_chip = Rect::from_min_size(
            Pos2::new(rect.left() + 16.0, rect.top() + 105.0),
            Vec2::new(version_width.min(190.0), 21.0),
        );
        theme::fill_round(
            painter,
            version_chip,
            theme::RADIUS,
            theme::alpha(accent, 22),
        );
        theme::stroke_round(
            painter,
            version_chip,
            theme::RADIUS,
            0.8,
            theme::alpha(accent, 42),
        );
        theme::center_text(
            painter,
            version_chip.center(),
            ellipsize_end(&version_label, 24),
            7.2,
            Color32::from_rgb(197, 224, 195),
            false,
        );
        painter.line_segment(
            [
                Pos2::new(rect.left() + 14.0, rect.top() + 136.0),
                Pos2::new(rect.right() - 14.0, rect.top() + 136.0),
            ],
            Stroke::new(1.0, theme::white_alpha(19)),
        );
        theme::text(
            painter,
            Pos2::new(rect.left() + 16.0, rect.top() + 151.0),
            if recent.is_some() {
                if is_server {
                    "● 最近连接"
                } else {
                    "● 最近存档"
                }
            } else {
                "● 等待第一次冒险"
            },
            7.6,
            Color32::from_rgb(174, 194, 177),
        );
        let join = Rect::from_min_size(
            Pos2::new(rect.right() - 94.0, rect.top() + 145.0),
            Vec2::new(78.0, 27.0),
        );
        let response = ui
            .interact(join, Id::unique("server-join"), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        if response.hovered() {
            theme::fill_round(painter, join, theme::RADIUS, theme::white_alpha(13));
        }
        theme::text(
            painter,
            Pos2::new(join.left() + 7.0, join.top() + 5.0),
            if recent.is_some() {
                "继续游戏 →"
            } else {
                "开始游戏 →"
            },
            7.6,
            if response.hovered() {
                theme::GREEN_BRIGHT
            } else {
                Color32::from_rgb(160, 215, 154)
            },
        );
        if response.clicked() {
            let starting = self.launch_started.is_some()
                || self.install_receiver.is_some()
                || self.game_process_id.is_some();
            if !starting {
                self.start_launch(ctx);
            }
        }
    }

    fn paint_launch_dock(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        hero: Rect,
        time: f64,
        motion: CardMotion,
    ) {
        let dock = motion.rect(Rect::from_min_max(
            Pos2::new(hero.left() + 19.0, hero.bottom() - 96.0),
            Pos2::new(hero.right() - 19.0, hero.bottom() - 18.0),
        ));
        theme::glass_panel(
            painter,
            dock,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(19, 31, 23), theme::GREEN, time, 5.7),
        );
        let dock_content = card_clip(painter, dock);
        let painter = &dock_content;

        let version = self.version().clone();
        let version_icon = Rect::from_center_size(
            Pos2::new(dock.left() + 43.0, dock.center().y + 2.0),
            Vec2::splat(44.0),
        );
        if let Some(texture) = self.version_icon(&version) {
            paint_rounded_image(painter, version_icon, 11.0, texture, texture.size());
        } else {
            paint_block(
                painter,
                version_icon.center(),
                43.0,
                Color32::from_rgb(128, 205, 105),
                Color32::from_rgb(91, 67, 49),
                Color32::from_rgb(111, 81, 54),
            );
        }
        theme::stroke_round(painter, version_icon, 11, 1.0, theme::white_alpha(28));
        let text_left = dock.left() + 76.0;
        let text_right = dock.right() - 282.0;
        let text_width = (text_right - text_left).max(100.0);
        let loader_font = theme::font(7.5);
        let mut loader_text = self.visible_loader_label(&version);
        let has_loader = !loader_text.is_empty();
        let mut loader_text_width = if has_loader {
            painter
                .layout_no_wrap(loader_text.clone(), loader_font.clone(), theme::INK)
                .size()
                .x
        } else {
            0.0
        };
        let max_loader_text = (text_width * 0.48 - 14.0).max(34.0);
        if has_loader && loader_text_width > max_loader_text {
            loader_text =
                fit_text_to_width(painter, &loader_text, loader_font.clone(), max_loader_text);
            loader_text_width = painter
                .layout_no_wrap(loader_text.clone(), loader_font, theme::INK)
                .size()
                .x;
        }
        let loader_width = if has_loader {
            loader_text_width + 14.0
        } else {
            0.0
        };
        let version_max_width = (text_width - loader_width - 10.0).max(80.0);
        let version_text = fit_text_to_width(
            painter,
            &version.game_version,
            theme::bold_font(14.0),
            version_max_width,
        );
        let version_width = painter
            .layout_no_wrap(version_text.clone(), theme::bold_font(14.0), theme::INK)
            .size()
            .x;
        theme::bold_text(
            painter,
            Pos2::new(text_left, dock.top() + 17.0),
            version_text,
            14.0,
            theme::INK,
        );
        if has_loader {
            let loader_rect = Rect::from_min_size(
                Pos2::new(text_left + version_width + 8.0, dock.top() + 19.0),
                Vec2::new(loader_width, 17.0),
            );
            theme::fill_round(
                painter,
                loader_rect,
                theme::RADIUS,
                theme::alpha(theme::GREEN, 20),
            );
            theme::center_text(
                painter,
                loader_rect.center(),
                loader_text,
                7.5,
                Color32::from_rgb(168, 221, 160),
                false,
            );
            if loader_rect.right() + 62.0 < text_right {
                theme::text(
                    painter,
                    Pos2::new(loader_rect.right() + 9.0, dock.top() + 20.0),
                    "✓ 已校验",
                    7.2,
                    theme::MUTED,
                );
            }
        }
        theme::text(
            painter,
            Pos2::new(text_left, dock.top() + 42.0),
            ellipsize_end(&version.description, 58),
            8.3,
            Color32::from_rgb(128, 142, 133),
        );

        let more = Rect::from_center_size(
            Pos2::new(dock.right() - 295.0, dock.center().y),
            Vec2::new(34.0, 34.0),
        );
        let more_response = ui
            .interact(more, Id::unique("launch-more"), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        if more_response.hovered() || self.launch_settings_open {
            theme::fill_round(painter, more, theme::RADIUS, theme::white_alpha(13));
        }
        for offset in [-4.0, 0.0, 4.0] {
            painter.circle_filled(
                Pos2::new(more.center().x + offset, more.center().y),
                1.5,
                theme::MUTED,
            );
        }
        if more_response.clicked() {
            self.menu = Menu::None;
            self.news = None;
            self.launch_settings_open = !self.launch_settings_open;
        }

        painter.line_segment(
            [
                Pos2::new(dock.right() - 267.0, dock.top() + 18.0),
                Pos2::new(dock.right() - 267.0, dock.bottom() - 18.0),
            ],
            Stroke::new(1.0, theme::white_alpha(22)),
        );

        let launch = Rect::from_min_max(
            Pos2::new(dock.right() - 250.0, dock.top() + 9.0),
            Pos2::new(dock.right() - 15.0, dock.bottom() - 9.0),
        );
        let response = ui
            .interact(launch, Id::unique("launch-game"), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        let game_running = self.game_process_id.is_some() && self.launch_started.is_none();
        let installing = self.install_receiver.is_some();
        let starting = self.launch_started.is_some() || installing;
        let brighten = response.hovered() && !starting;
        if game_running {
            theme::fill_round(
                painter,
                launch,
                theme::RADIUS,
                if response.hovered() {
                    Color32::from_rgb(164, 77, 66)
                } else {
                    Color32::from_rgb(67, 129, 74)
                },
            );
            theme::stroke_round(painter, launch, theme::RADIUS, 1.0, theme::white_alpha(72));
        } else {
            theme::liquid_button(painter, launch, theme::RADIUS, time, brighten);
        }
        if starting {
            theme::fill_round(painter, launch, theme::RADIUS, theme::black_alpha(54));
        }
        let play_rect = Rect::from_center_size(
            Pos2::new(launch.left() + 29.0, launch.center().y),
            Vec2::new(44.0, 44.0),
        );
        theme::fill_round(
            painter,
            play_rect,
            theme::RADIUS,
            theme::alpha(Color32::from_rgb(13, 61, 26), 62),
        );
        if game_running {
            painter.rect_filled(
                Rect::from_center_size(play_rect.center(), Vec2::splat(15.0)),
                theme::RADIUS,
                Color32::WHITE,
            );
        } else if starting {
            painter.circle_stroke(
                play_rect.center(),
                9.0,
                Stroke::new(2.0, theme::white_alpha(195)),
            );
            painter.circle_stroke(
                play_rect.center(),
                5.5,
                Stroke::new(1.0, theme::white_alpha(45)),
            );
        } else {
            painter.add(Shape::convex_polygon(
                vec![
                    play_rect.center() + Vec2::new(-4.0, -8.0),
                    play_rect.center() + Vec2::new(8.0, 0.0),
                    play_rect.center() + Vec2::new(-4.0, 8.0),
                ],
                Color32::WHITE,
                Stroke::NONE,
            ));
        }
        theme::bold_text(
            painter,
            Pos2::new(launch.left() + 59.0, launch.top() + 13.0),
            if game_running {
                "游戏运行中"
            } else if installing {
                "正在补全文件…"
            } else if starting {
                "正在启动…"
            } else {
                "开始游戏"
            },
            13.0,
            Color32::WHITE,
        );
        theme::text(
            painter,
            Pos2::new(launch.left() + 59.0, launch.top() + 36.0),
            if game_running {
                "再次点击强制关闭"
            } else if installing {
                self.install_progress
                    .as_ref()
                    .map(|progress| progress.stage.as_str())
                    .unwrap_or("连接 Mojang 官方下载源")
            } else if starting {
                "检查运行环境与模组"
            } else {
                &version.java
            },
            7.3,
            theme::alpha(Color32::WHITE, 174),
        );
        if response.clicked() {
            if game_running {
                if let Some(process_id) = self.game_process_id {
                    match crate::game::force_close_game(process_id) {
                        Ok(()) => {
                            self.game_process_id = None;
                            self.launch_started = None;
                            self.launch_complete = false;
                            self.show_toast("已强制关闭游戏");
                        }
                        Err(error) => self.show_toast(format!("关闭游戏失败：{error}")),
                    }
                }
            } else if !starting {
                self.start_launch(ctx);
                ctx.request_repaint();
            }
        }
    }

    fn paint_launch_settings_card(
        &mut self,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
    ) {
        let card = launch_settings_card_rect(layout);
        theme::glass_panel(
            painter,
            card,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(18, 29, 22), theme::GREEN, time, 23.0)
                .backdrop_blur(16.0),
        );
        let card_content = card_clip(painter, card);
        let painter = &card_content;
        theme::bold_text(
            painter,
            Pos2::new(card.left() + 20.0, card.top() + 17.0),
            "启动设置",
            16.0,
            Color32::from_rgb(174, 185, 177),
        );
        theme::text(
            painter,
            Pos2::new(card.left() + 20.0, card.top() + 42.0),
            "修改后自动保存",
            7.8,
            Color32::from_rgb(125, 139, 129),
        );
        let close = Rect::from_min_size(
            Pos2::new(card.right() - 44.0, card.top() + 14.0),
            Vec2::splat(30.0),
        );
        let close_response = ui
            .interact(close, Id::unique("launch-settings-close"), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        theme::fill_round(
            painter,
            close,
            theme::RADIUS,
            if close_response.hovered() {
                theme::white_alpha(22)
            } else {
                theme::black_alpha(56)
            },
        );
        draw_icon(
            painter,
            NavIcon::Close,
            Rect::from_center_size(close.center(), Vec2::splat(13.0)),
            Color32::from_rgb(148, 160, 151),
        );
        if close_response.clicked() {
            self.launch_settings_open = false;
        }

        let row_left = card.left() + 20.0;
        let row_width = card.width() - 40.0;
        let mut row_y = card.top() + 67.0;
        let row_height = 55.0;

        let row = Rect::from_min_size(Pos2::new(row_left, row_y), Vec2::new(row_width, row_height));
        paint_setting_label(painter, row, "游戏内存", "Java 虚拟机最大可用内存");
        let minus = Rect::from_center_size(
            Pos2::new(row.right() - 145.0, row.center().y),
            Vec2::splat(28.0),
        );
        let plus = Rect::from_center_size(
            Pos2::new(row.right() - 27.0, row.center().y),
            Vec2::splat(28.0),
        );
        if compact_icon_button(ui, painter, minus, "popup-memory-minus", false) {
            self.settings.memory_mb = self.settings.memory_mb.saturating_sub(512).max(2048);
        }
        if compact_icon_button(ui, painter, plus, "popup-memory-plus", true) {
            self.settings.memory_mb = (self.settings.memory_mb + 512).min(16384);
        }
        theme::bold_text(
            painter,
            Pos2::new(row.right() - 111.0, row.center().y - 8.0),
            format!("{:.1} GB", self.settings.memory_mb as f32 / 1024.0),
            11.0,
            Color32::from_rgb(183, 194, 186),
        );
        row_y += row_height;

        let row = Rect::from_min_size(Pos2::new(row_left, row_y), Vec2::new(row_width, row_height));
        paint_setting_label(painter, row, "窗口尺寸", "点击切换常用启动分辨率");
        let resolution = Rect::from_min_size(
            Pos2::new(row.right() - 143.0, row.top() + 12.0),
            Vec2::new(143.0, 32.0),
        );
        let label = format!(
            "{} × {}",
            self.settings.window_width, self.settings.window_height
        );
        if settings_button(ui, painter, resolution, "popup-resolution", &label, false) {
            const PRESETS: [(u32, u32); 4] = [(1280, 720), (1600, 900), (1920, 1080), (2560, 1440)];
            let next = PRESETS
                .iter()
                .position(|size| *size == (self.settings.window_width, self.settings.window_height))
                .map(|index| (index + 1) % PRESETS.len())
                .unwrap_or(0);
            (self.settings.window_width, self.settings.window_height) = PRESETS[next];
        }
        row_y += row_height;

        let row = Rect::from_min_size(Pos2::new(row_left, row_y), Vec2::new(row_width, row_height));
        paint_toggle_row(
            ui,
            painter,
            row,
            "popup-fullscreen",
            "全屏启动",
            "游戏启动后直接进入全屏",
            &mut self.settings.fullscreen,
        );
        row_y += row_height;

        let row = Rect::from_min_size(Pos2::new(row_left, row_y), Vec2::new(row_width, row_height));
        paint_toggle_row(
            ui,
            painter,
            row,
            "popup-hide-launcher",
            "启动后隐藏启动器",
            "游戏进程启动完成后自动最小化",
            &mut self.settings.hide_after_launch,
        );
    }

    fn paint_loader_dialog(
        &mut self,
        ui: &mut egui::Ui,
        painter: &Painter,
        screen: Rect,
        time: f64,
    ) {
        let size = Vec2::new(
            590.0_f32.min(screen.width() - 34.0),
            480.0_f32.min(screen.height() - 38.0),
        );
        let dialog = Rect::from_center_size(screen.center(), size);
        theme::glass_panel(
            painter,
            dialog,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(20, 31, 24), theme::GREEN, time, 31.0)
                .backdrop_blur(18.0),
        );
        let dialog_content = card_clip(painter, dialog);
        let painter = &dialog_content;
        theme::bold_text(
            painter,
            Pos2::new(dialog.left() + 24.0, dialog.top() + 20.0),
            "安装加载器",
            21.0,
            theme::INK,
        );
        theme::text(
            painter,
            Pos2::new(dialog.left() + 24.0, dialog.top() + 48.0),
            if self.loader_preinstall.is_some() {
                format!(
                    "Minecraft {} · 先选择加载器，再开始下载安装",
                    self.loader_base_version
                )
            } else {
                format!(
                    "Minecraft {} · 选择加载器或仅保留原版",
                    self.loader_base_version
                )
            },
            8.2,
            theme::MUTED,
        );

        let installing = self.loader_install_receiver.is_some();
        for (index, loader) in LoaderKind::ALL.into_iter().enumerate() {
            let button = Rect::from_min_size(
                Pos2::new(
                    dialog.left() + 24.0 + index as f32 * 131.0,
                    dialog.top() + 78.0,
                ),
                Vec2::new(121.0, 34.0),
            );
            let response = ui
                .interact(button, Id::unique(("loader-type", index)), Sense::click())
                .on_hover_cursor(if installing {
                    CursorIcon::NotAllowed
                } else {
                    CursorIcon::PointingHand
                });
            let selected = self.loader_kind == Some(loader);
            theme::fill_round(
                painter,
                button,
                theme::RADIUS,
                if selected {
                    theme::alpha(theme::GREEN, 34)
                } else if response.hovered() && !installing {
                    theme::white_alpha(13)
                } else {
                    theme::black_alpha(42)
                },
            );
            theme::stroke_round(
                painter,
                button,
                theme::RADIUS,
                1.0,
                if selected {
                    theme::alpha(theme::GREEN, 75)
                } else {
                    theme::white_alpha(24)
                },
            );
            theme::center_text(
                painter,
                button.center(),
                loader.label(),
                9.0,
                if selected {
                    theme::GREEN_BRIGHT
                } else {
                    theme::INK_SOFT
                },
                selected,
            );
            if response.clicked() && !installing {
                self.choose_loader(loader);
            }
        }

        let list_top = dialog.top() + 128.0;
        let list_bottom = dialog.bottom() - 78.0;
        let list_clip = Rect::from_min_max(
            Pos2::new(dialog.left() + 20.0, list_top),
            Pos2::new(dialog.right() - 20.0, list_bottom),
        );
        let list_painter = painter.clone();
        if let Some(update) = &self.loader_install_update {
            theme::fill_round(
                &list_painter,
                list_clip,
                theme::RADIUS,
                theme::black_alpha(45),
            );
            theme::center_text(
                &list_painter,
                Pos2::new(list_clip.center().x, list_clip.top() + 70.0),
                &update.stage,
                13.0,
                theme::INK,
                true,
            );
            if update.finished && !update.success {
                theme::wrapped_text(
                    &list_painter,
                    Pos2::new(list_clip.left() + 24.0, list_clip.top() + 102.0),
                    ellipsize_end(&update.message, 240),
                    8.5,
                    Color32::from_rgb(229, 148, 132),
                    list_clip.width() - 48.0,
                );
            } else if !update.finished {
                let track = Rect::from_min_size(
                    Pos2::new(list_clip.left() + 80.0, list_clip.top() + 112.0),
                    Vec2::new((list_clip.width() - 160.0).max(80.0), 7.0),
                );
                theme::fill_round(&list_painter, track, theme::RADIUS, theme::white_alpha(18));
                let phase = ((time * 0.35) % 1.0) as f32;
                let knob = Rect::from_min_size(
                    Pos2::new(track.left() + (track.width() - 72.0) * phase, track.top()),
                    Vec2::new(72.0, track.height()),
                );
                theme::fill_round(&list_painter, knob, theme::RADIUS, theme::GREEN);
            }
        } else if self.loader_kind.is_none() {
            theme::center_text(
                &list_painter,
                Pos2::new(list_clip.center().x, list_clip.top() + 74.0),
                "选择上方加载器以查看可用版本",
                9.3,
                theme::MUTED,
                false,
            );
        } else if self.loader_versions_receiver.is_some() {
            theme::center_text(
                &list_painter,
                Pos2::new(list_clip.center().x, list_clip.top() + 74.0),
                "正在获取加载器版本…",
                9.3,
                theme::MUTED,
                false,
            );
        } else if let Some(error) = &self.loader_versions_error {
            theme::wrapped_text(
                &list_painter,
                Pos2::new(list_clip.left() + 28.0, list_clip.top() + 48.0),
                error,
                8.5,
                Color32::from_rgb(229, 148, 132),
                list_clip.width() - 56.0,
            );
        } else {
            let row_height = 38.0;
            let content_height = self.loader_versions.len() as f32 * row_height;
            let max_scroll = (content_height - list_clip.height()).max(0.0);
            if ui.ctx().input(|input| {
                input
                    .pointer
                    .hover_pos()
                    .is_some_and(|pointer| dialog.contains(pointer))
            }) {
                self.loader_version_scroll -=
                    ui.ctx().input(|input| input.smooth_scroll_delta.y) * 1.2;
            }
            self.loader_version_scroll = self.loader_version_scroll.clamp(0.0, max_scroll);
            for (index, version) in self.loader_versions.iter().enumerate() {
                let y = list_top + index as f32 * row_height - self.loader_version_scroll;
                if y + 34.0 < list_top || y > list_bottom {
                    continue;
                }
                let row = Rect::from_min_size(
                    Pos2::new(dialog.left() + 24.0, y),
                    Vec2::new(dialog.width() - 48.0, 34.0),
                );
                let response = ui
                    .interact(row, Id::unique(("loader-version", index)), Sense::click())
                    .on_hover_cursor(CursorIcon::PointingHand);
                let selected = self.loader_version_selected == index;
                if selected || response.hovered() {
                    theme::fill_round(
                        &list_painter,
                        row,
                        theme::RADIUS,
                        if selected {
                            theme::alpha(theme::GREEN, 24)
                        } else {
                            theme::white_alpha(8)
                        },
                    );
                }
                theme::text(
                    &list_painter,
                    Pos2::new(row.left() + 13.0, row.top() + 8.0),
                    version,
                    9.0,
                    if selected {
                        theme::GREEN_BRIGHT
                    } else {
                        theme::INK_SOFT
                    },
                );
                if response.clicked() {
                    self.loader_version_selected = index;
                }
            }
        }

        let skip = Rect::from_min_size(
            Pos2::new(dialog.left() + 24.0, dialog.bottom() - 54.0),
            Vec2::new(118.0, 34.0),
        );
        let preinstall = self.loader_preinstall.is_some();
        if settings_button(
            ui,
            painter,
            skip,
            "loader-skip",
            if preinstall {
                "仅安装原版"
            } else {
                "仅保留原版"
            },
            false,
        ) && !installing
        {
            if preinstall {
                self.begin_preinstall(false);
            } else {
                self.loader_dialog_open = false;
                self.loader_versions_receiver = None;
                self.loader_install_update = None;
            }
        }
        let install = Rect::from_min_size(
            Pos2::new(dialog.right() - 150.0, dialog.bottom() - 54.0),
            Vec2::new(126.0, 34.0),
        );
        let can_install = !installing
            && self.loader_kind.is_some()
            && !self.loader_versions.is_empty()
            && self
                .loader_install_update
                .as_ref()
                .is_none_or(|update| update.success || update.finished);
        if settings_button(
            ui,
            painter,
            install,
            "loader-install",
            if preinstall {
                "开始安装"
            } else {
                "安装加载器"
            },
            can_install,
        ) && can_install
        {
            if preinstall {
                self.begin_preinstall(true);
            } else {
                self.start_loader_install();
            }
        }
    }

    fn paint_latest(
        &mut self,
        _ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
        motion: CardMotion,
    ) {
        let heading_shift = motion.offset(layout.news_cards[0]);
        theme::mono_text(
            painter,
            Pos2::new(layout.content_left, layout.latest.top() + heading_shift),
            "DISCOVER",
            7.2,
            theme::MUTED,
        );
        theme::bold_text(
            painter,
            Pos2::new(
                layout.content_left,
                layout.latest.top() + 11.0 + heading_shift,
            ),
            "最新动态",
            15.0,
            theme::INK_SOFT,
        );
        let all_news = Rect::from_min_size(
            Pos2::new(
                layout.content_right - 78.0,
                layout.latest.top() + 10.0 + heading_shift,
            ),
            Vec2::new(78.0, 24.0),
        );
        let all_response = ui
            .interact(all_news, Id::unique("all-news"), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        theme::text(
            painter,
            Pos2::new(all_news.left(), all_news.top() + 2.0),
            "查看全部  →",
            8.3,
            if all_response.hovered() {
                Color32::from_rgb(188, 224, 183)
            } else {
                theme::MUTED
            },
        );
        if all_response.clicked() {
            match open_external_url("https://www.minecraft.net/article") {
                Ok(()) => {}
                Err(error) => self.show_toast(format!("无法打开 Minecraft 动态：{error}")),
            }
        }

        self.ensure_news_textures(_ctx);
        let entries: Vec<(usize, MinecraftNews)> = self
            .news_items
            .iter()
            .take(3)
            .cloned()
            .enumerate()
            .collect();
        let textures = self.news_textures.clone();

        for index in 0..3 {
            let rect = motion.rect(layout.news_cards[index]);
            let response = ui
                .interact(rect, Id::unique(("news", index)), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand);
            let accent = news_accent(index);
            theme::glass_panel(
                painter,
                rect,
                theme::RADIUS,
                theme::GlassSpec::new(
                    Color32::from_rgb(20, 29, 23),
                    accent,
                    time,
                    index as f32 * 1.37,
                )
                .hovered(response.hovered()),
            );
            let news_content = card_clip(painter, rect);
            let painter = &news_content;
            let art = Rect::from_min_size(
                rect.left_top() + Vec2::new(7.0, 7.0),
                Vec2::new(92.0, rect.height() - 14.0),
            );
            if let Some(Some(texture)) = textures.get(index) {
                paint_rounded_image(painter, art, 12.0, texture, texture.size());
            } else {
                scene::paint_news_art(
                    painter,
                    art,
                    match index {
                        0 => scene::NewsArt::Version,
                        1 => scene::NewsArt::Mod,
                        _ => scene::NewsArt::Community,
                    },
                    time,
                );
            }
            theme::stroke_round(painter, art, 12, 1.0, theme::white_alpha(28));
            let copy_x = art.right() + 13.0;
            let entry = entries.get(index).map(|(_, entry)| entry);
            let kicker = entry
                .map(|entry| {
                    if entry.date.is_empty() {
                        entry.category.clone()
                    } else {
                        format!("{} · {}", entry.category, entry.date)
                    }
                })
                .unwrap_or_else(|| {
                    if let Some(error) = &self.news_error {
                        error.clone()
                    } else if self.news_receiver.is_some() {
                        "正在连接 Minecraft 官方动态".to_owned()
                    } else {
                        "Minecraft 官方动态".to_owned()
                    }
                });
            let title = entry
                .map(|entry| ellipsize_end(&entry.title, 24))
                .unwrap_or_else(|| "正在获取最新内容".to_owned());
            let description = entry
                .map(|entry| ellipsize_end(&entry.summary, 52))
                .filter(|description| !description.is_empty())
                .unwrap_or_else(|| "稍后刷新即可查看 Mojang 发布的最新消息。".to_owned());
            theme::text(
                painter,
                Pos2::new(copy_x, rect.top() + 14.0),
                ellipsize_end(&kicker, 36),
                7.3,
                Color32::from_rgb(119, 169, 114),
            );
            theme::bold_text(
                painter,
                Pos2::new(copy_x, rect.top() + 31.0),
                &title,
                11.3,
                theme::INK_SOFT,
            );
            theme::wrapped_text(
                painter,
                Pos2::new(copy_x, rect.top() + 50.0),
                &description,
                7.6,
                Color32::from_rgb(124, 138, 128),
                rect.right() - copy_x - 12.0,
            );
            if entry.is_some() {
                theme::text(
                    painter,
                    Pos2::new(copy_x, rect.bottom() - 20.0),
                    "阅读官方详情  →",
                    7.5,
                    Color32::from_rgb(159, 210, 152),
                );
            }
            if response.clicked()
                && let Some((global_index, _)) = entries.get(index)
            {
                self.news = Some(*global_index);
                self.menu = Menu::None;
            }
        }
    }

    fn paint_versions(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
        motion: CardMotion,
    ) {
        let page = Rect::from_min_max(
            Pos2::new(layout.content_left, layout.header.bottom() + 14.0),
            Pos2::new(layout.content_right, layout.screen.bottom() - 22.0),
        );
        let list_width = (page.width() * 0.49).clamp(430.0, 590.0);
        let source_list_panel = Rect::from_min_max(
            page.left_top(),
            Pos2::new(page.left() + list_width, page.bottom()),
        );
        let list_panel = motion.rect(source_list_panel);
        let list_heading =
            Rect::from_min_size(list_panel.left_top(), Vec2::new(list_panel.width(), 150.0));
        let base_painter = painter;
        theme::glass_panel(
            painter,
            list_heading,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(20, 32, 24), theme::GREEN, time, 16.0),
        );
        let heading_content = card_clip(painter, list_heading);
        let painter = &heading_content;

        let installed_count = self
            .settings
            .game_directory
            .as_deref()
            .map(crate::game::installed_versions)
            .map(|versions| versions.len())
            .unwrap_or(0);
        let launchable_count = self
            .versions
            .iter()
            .filter(|version| version.launchable)
            .count();
        theme::bold_text(
            painter,
            Pos2::new(list_heading.left() + 22.0, list_heading.top() + 15.0),
            "已安装版本",
            19.0,
            theme::INK,
        );
        theme::text(
            painter,
            Pos2::new(list_heading.left() + 22.0, list_heading.top() + 48.0),
            ellipsize_end(
                &format!("{installed_count} 个版本 · {launchable_count} 个可启动"),
                42,
            ),
            8.5,
            theme::MUTED,
        );
        let refresh = Rect::from_min_size(
            Pos2::new(list_heading.right() - 91.0, list_heading.top() + 14.0),
            Vec2::new(69.0, 30.0),
        );
        let import = Rect::from_min_size(
            Pos2::new(list_heading.right() - 197.0, list_heading.top() + 14.0),
            Vec2::new(98.0, 30.0),
        );
        if settings_button(
            ui,
            painter,
            import,
            "version-import-modpack",
            "导入整合包",
            false,
        ) {
            self.start_modpack_import();
            ctx.request_repaint();
        }
        if settings_button(ui, painter, refresh, "version-refresh", "刷新", false) {
            let active_name = self.version().version.clone();
            self.versions = available_versions(self.settings.game_directory.as_deref());
            self.selected_version = self
                .versions
                .iter()
                .position(|version| version.version == active_name)
                .or_else(|| self.versions.iter().position(|version| version.launchable))
                .unwrap_or(0);
            self.version_page_selected = self.selected_version;
            self.version_page_scroll = 0.0;
            self.refresh_official_manifest();
            self.show_toast(format!("版本列表已刷新 · {} 个版本", installed_count));
        }

        let folder_selector = Rect::from_min_size(
            Pos2::new(list_heading.left() + 22.0, list_heading.top() + 68.0),
            Vec2::new(list_heading.width() - 44.0, 34.0),
        );
        let folder_response = ui
            .interact(
                folder_selector,
                Id::unique("version-folder-selector"),
                Sense::click(),
            )
            .on_hover_cursor(CursorIcon::PointingHand);
        theme::fill_round(
            painter,
            folder_selector,
            theme::RADIUS,
            if folder_response.hovered() || self.version_folder_menu_open {
                Color32::from_rgb(31, 48, 36)
            } else {
                Color32::from_rgb(25, 38, 29)
            },
        );
        theme::stroke_round(
            painter,
            folder_selector,
            theme::RADIUS,
            1.0,
            if self.version_folder_menu_open {
                theme::alpha(theme::GREEN, 66)
            } else {
                theme::white_alpha(28)
            },
        );
        draw_icon(
            painter,
            NavIcon::Folder,
            Rect::from_center_size(
                Pos2::new(folder_selector.left() + 16.0, folder_selector.center().y),
                Vec2::splat(13.0),
            ),
            theme::GREEN_BRIGHT,
        );
        let folder_text = self
            .settings
            .game_directory
            .as_ref()
            .map(|path| ellipsize_start(&path.display().to_string(), 42))
            .unwrap_or_else(|| "选择 Minecraft 游戏文件夹".to_owned());
        theme::text(
            painter,
            Pos2::new(folder_selector.left() + 29.0, folder_selector.top() + 9.0),
            folder_text,
            8.2,
            theme::INK_SOFT,
        );
        draw_icon(
            painter,
            NavIcon::Chevron,
            Rect::from_center_size(
                Pos2::new(folder_selector.right() - 17.0, folder_selector.center().y),
                Vec2::splat(12.0),
            ),
            theme::MUTED,
        );
        if folder_response.clicked() {
            self.menu = Menu::None;
            self.news = None;
            self.version_folder_menu_open = !self.version_folder_menu_open;
        }

        let filters = [
            (VersionFilter::All, "全部"),
            (VersionFilter::Release, "正式版"),
            (VersionFilter::Snapshot, "快照"),
        ];
        let mut filter_x = list_heading.left() + 22.0;
        for (index, (filter, label)) in filters.into_iter().enumerate() {
            let button = Rect::from_min_size(
                Pos2::new(filter_x, list_heading.top() + 114.0),
                Vec2::new(66.0, 27.0),
            );
            let response = ui
                .interact(
                    button,
                    Id::unique(("version-filter", index)),
                    Sense::click(),
                )
                .on_hover_cursor(CursorIcon::PointingHand);
            let selected = self.version_filter == filter;
            theme::fill_round(
                painter,
                button,
                theme::RADIUS,
                if selected {
                    theme::alpha(theme::GREEN, 27)
                } else if response.hovered() {
                    theme::white_alpha(14)
                } else {
                    theme::white_alpha(7)
                },
            );
            theme::center_text(
                painter,
                button.center(),
                label,
                8.4,
                if selected { theme::INK } else { theme::MUTED },
                selected,
            );
            if response.clicked() {
                self.version_filter = filter;
                self.version_page_scroll = 0.0;
                self.previous_version_detail = None;
                self.version_detail_progress = 1.0;
            }
            filter_x = button.right() + 7.0;
        }

        let filtered: Vec<(usize, GameVersion)> = self
            .versions
            .iter()
            .enumerate()
            .filter(|(_, version)| match self.version_filter {
                VersionFilter::All => version.launchable,
                VersionFilter::Release => {
                    version.launchable && !is_snapshot_version(&version.version)
                }
                VersionFilter::Snapshot => {
                    version.launchable && is_snapshot_version(&version.version)
                }
            })
            .map(|(index, version)| (index, version.clone()))
            .collect();
        if !filtered.is_empty()
            && !filtered
                .iter()
                .any(|(index, _)| *index == self.version_page_selected)
        {
            self.version_page_selected = filtered[0].0;
            self.previous_version_detail = None;
            self.version_detail_progress = 1.0;
        }

        let source_list_top = source_list_panel.top() + 158.0;
        let list_top = list_panel.top() + 158.0;
        let list_bottom = list_panel.bottom() - 13.0;
        let list_height = (list_bottom - list_top).max(1.0);
        let row_height = 64.0;
        let content_height = filtered.len() as f32 * row_height;
        let max_scroll = (content_height - list_height).max(0.0);
        if self.version_content_dialog.is_none()
            && ui.ctx().input(|input| {
                input
                    .pointer
                    .hover_pos()
                    .is_some_and(|pointer| list_panel.contains(pointer))
                    && input.smooth_scroll_delta.y.abs() > 0.0
            })
        {
            self.version_page_scroll -= ui.ctx().input(|input| input.smooth_scroll_delta.y) * 1.05;
        }
        self.version_page_scroll = self.version_page_scroll.clamp(0.0, max_scroll);
        let painter = base_painter;
        let list_viewport = Rect::from_min_max(
            Pos2::new(list_panel.left() + 4.0, list_top),
            Pos2::new(list_panel.right() - 10.0, list_bottom),
        );
        let list_painter = painter.with_clip_rect(list_viewport);

        for (visible_index, (global_index, version)) in filtered.iter().enumerate() {
            let source_y =
                source_list_top + visible_index as f32 * row_height - self.version_page_scroll;
            if source_y + row_height < source_list_top
                || source_y > source_list_panel.bottom() - 13.0
            {
                continue;
            }
            let row = motion.rect(Rect::from_min_size(
                Pos2::new(source_list_panel.left() + 4.0, source_y),
                Vec2::new(source_list_panel.width() - 14.0, row_height - 5.0),
            ));
            let response = ui
                .interact(
                    row,
                    Id::unique(("page-version", global_index)),
                    Sense::click(),
                )
                .on_hover_cursor(CursorIcon::PointingHand);
            let selected = self.version_page_selected == *global_index;
            let card = row.shrink2(Vec2::new(2.0, 2.5));
            theme::glass_panel(
                &list_painter,
                card,
                theme::RADIUS,
                theme::GlassSpec::new(
                    if selected || response.hovered() {
                        Color32::from_rgb(28, 47, 32)
                    } else {
                        Color32::from_rgb(20, 32, 24)
                    },
                    theme::GREEN,
                    time,
                    row.top() * 0.01,
                )
                .hovered(response.hovered()),
            );
            let row_painter = card_clip(&list_painter, card);
            if selected || response.hovered() {
                theme::stroke_round(
                    &list_painter,
                    card,
                    theme::RADIUS,
                    1.0,
                    if selected {
                        theme::alpha(theme::GREEN, 82)
                    } else {
                        theme::white_alpha(32)
                    },
                );
            }
            let mark = Rect::from_center_size(
                Pos2::new(row.left() + 24.0, row.center().y),
                Vec2::splat(30.0),
            );
            theme::fill_round(
                &row_painter,
                mark,
                theme::RADIUS,
                if version.launchable {
                    Color32::from_rgb(48, 104, 57)
                } else {
                    Color32::from_rgb(66, 70, 61)
                },
            );
            if let Some(texture) = self.version_icon(version) {
                paint_rounded_image(&row_painter, mark.shrink(2.0), 9.0, texture, texture.size());
            } else {
                draw_icon(
                    &row_painter,
                    NavIcon::Box,
                    Rect::from_center_size(mark.center(), Vec2::splat(16.0)),
                    if version.launchable {
                        theme::GREEN_BRIGHT
                    } else {
                        Color32::from_rgb(150, 150, 132)
                    },
                );
            }
            let row_label = self.version_selector_label(version);
            let row_label = fit_text_to_width(
                &row_painter,
                &row_label,
                theme::font(11.0),
                (row.width() - 118.0).max(100.0),
            );
            let row_detail = self.version_selector_detail(version);
            theme::bold_text(
                &row_painter,
                Pos2::new(row.left() + 48.0, row.top() + 12.0),
                row_label,
                11.0,
                if version.launchable {
                    theme::INK
                } else {
                    theme::INK_SOFT
                },
            );
            theme::text(
                &row_painter,
                Pos2::new(row.left() + 48.0, row.top() + 33.0),
                ellipsize_end(&row_detail, 42),
                8.0,
                theme::MUTED,
            );
            let status = if version.launchable {
                "可启动"
            } else {
                "需补全"
            };
            let status_width = if version.launchable { 47.0 } else { 50.0 };
            let status_rect = Rect::from_center_size(
                Pos2::new(row.right() - status_width * 0.5 - 12.0, row.center().y),
                Vec2::new(status_width, 23.0),
            );
            theme::fill_round(
                &row_painter,
                status_rect,
                theme::RADIUS,
                if version.launchable {
                    theme::alpha(theme::GREEN, 22)
                } else {
                    theme::alpha(Color32::from_rgb(225, 178, 84), 20)
                },
            );
            theme::center_text(
                &row_painter,
                status_rect.center(),
                status,
                7.5,
                if version.launchable {
                    theme::GREEN_BRIGHT
                } else {
                    Color32::from_rgb(208, 172, 102)
                },
                false,
            );
            if response.clicked() {
                self.switch_version_detail(*global_index);
            }
        }
        if filtered.is_empty() {
            theme::center_text(
                &list_painter,
                Pos2::new(list_panel.center().x, list_top + 60.0),
                "当前分类没有版本",
                9.0,
                theme::MUTED,
                false,
            );
        }
        if max_scroll > 0.0 {
            paint_list_scrollbar(
                painter,
                list_panel.right(),
                list_top,
                list_height,
                content_height,
                self.version_page_scroll,
                max_scroll,
            );
        }

        let Some((selected_index, selected)) = filtered
            .iter()
            .find(|(index, _)| *index == self.version_page_selected)
        else {
            return;
        };
        let current = selected.clone();
        let detail_motion = if let Some(previous_index) = self.previous_version_detail {
            let outgoing = CardMotion {
                screen: layout.screen,
                direction: self.version_detail_direction,
                progress: self.version_detail_progress,
                outgoing: true,
            };
            if let Some(previous) = self.versions.get(previous_index).cloned() {
                self.paint_version_detail(
                    ui,
                    painter,
                    layout,
                    time,
                    VersionDetailView {
                        index: previous_index,
                        version: previous,
                        motion: outgoing,
                        active: false,
                    },
                );
            }
            CardMotion {
                screen: layout.screen,
                direction: self.version_detail_direction,
                progress: self.version_detail_progress,
                outgoing: false,
            }
        } else {
            motion
        };
        self.paint_version_detail(
            ui,
            painter,
            layout,
            time,
            VersionDetailView {
                index: *selected_index,
                version: current,
                motion: detail_motion,
                active: true,
            },
        );

        let _ = ctx;
    }

    fn paint_version_folder_menu(&mut self, ctx: &egui::Context, layout: &Layout, time: f64) {
        let rect = version_folder_menu_rect(layout, self.settings.game_directories.len());
        egui::Area::new(Id::unique("version-folder-menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .movable(false)
            .show(ctx, |ui| {
                ui.set_min_size(rect.size());
                let painter = ui.painter().clone();
                theme::glass_panel(
                    &painter,
                    rect,
                    theme::RADIUS,
                    theme::GlassSpec::new(Color32::from_rgb(20, 32, 24), theme::GREEN, time, 18.4)
                        .backdrop_blur(14.0),
                );
                let content = card_clip(&painter, rect);
                let painter = &content;
                theme::bold_text(
                    &painter,
                    Pos2::new(rect.left() + 16.0, rect.top() + 11.0),
                    "选择 Minecraft 文件夹",
                    9.2,
                    theme::INK,
                );
                let directories = self.settings.game_directories.clone();
                let visible = directories.len().min(5);
                let list_height = visible as f32 * 42.0;
                let max_scroll = (directories.len() as f32 * 42.0 - list_height).max(0.0);
                if ui.ctx().input(|input| {
                    input
                        .pointer
                        .hover_pos()
                        .is_some_and(|pointer| rect.contains(pointer))
                        && input.smooth_scroll_delta.y.abs() > 0.0
                }) {
                    self.version_folder_scroll -=
                        ui.ctx().input(|input| input.smooth_scroll_delta.y);
                }
                self.version_folder_scroll = self.version_folder_scroll.clamp(0.0, max_scroll);
                let list_top = rect.top() + 34.0;
                for (index, directory) in directories.iter().enumerate() {
                    let row_top = list_top + index as f32 * 42.0 - self.version_folder_scroll;
                    if row_top + 37.0 < list_top || row_top > list_top + list_height {
                        continue;
                    }
                    let row = Rect::from_min_size(
                        Pos2::new(rect.left() + 8.0, row_top),
                        Vec2::new(rect.width() - 16.0, 37.0),
                    );
                    let row_painter = card_clip(painter, row);
                    let response = ui
                        .interact(
                            row,
                            Id::unique(("version-folder-option", index)),
                            Sense::click(),
                        )
                        .on_hover_cursor(CursorIcon::PointingHand);
                    let selected = self
                        .settings
                        .game_directory
                        .as_ref()
                        .is_some_and(|current| same_path(current, directory));
                    theme::fill_round(
                        &row_painter,
                        row,
                        theme::RADIUS,
                        if selected {
                            theme::alpha(theme::GREEN, 24)
                        } else if response.hovered() {
                            theme::white_alpha(10)
                        } else {
                            Color32::TRANSPARENT
                        },
                    );
                    draw_icon(
                        &row_painter,
                        NavIcon::Folder,
                        Rect::from_center_size(
                            Pos2::new(row.left() + 17.0, row.center().y),
                            Vec2::splat(14.0),
                        ),
                        if selected {
                            theme::GREEN_BRIGHT
                        } else {
                            theme::MUTED
                        },
                    );
                    theme::text(
                        &row_painter,
                        Pos2::new(row.left() + 32.0, row.top() + 6.0),
                        ellipsize_start(&directory.display().to_string(), 48),
                        7.9,
                        if selected {
                            theme::INK
                        } else {
                            theme::INK_SOFT
                        },
                    );
                    if selected {
                        theme::text(
                            &row_painter,
                            Pos2::new(row.right() - 17.0, row.top() + 7.0),
                            "✓",
                            9.0,
                            theme::GREEN_BRIGHT,
                        );
                    }
                    if response.clicked() {
                        self.select_game_directory(directory.clone());
                        self.version_folder_menu_open = false;
                    }
                }
                let action_y = list_top + list_height + 6.0;
                let browse = Rect::from_min_size(
                    Pos2::new(rect.left() + 8.0, action_y),
                    Vec2::new(rect.width() - 16.0, 34.0),
                );
                if settings_button(
                    ui,
                    &painter,
                    browse,
                    "version-folder-browse",
                    "浏览并添加文件夹…",
                    false,
                ) && let Some(directory) = crate::game::pick_game_directory()
                {
                    self.select_game_directory(directory);
                    self.version_folder_menu_open = false;
                }
                let scan = Rect::from_min_size(
                    Pos2::new(rect.left() + 8.0, action_y + 42.0),
                    Vec2::new(rect.width() - 16.0, 34.0),
                );
                if settings_button(
                    ui,
                    &painter,
                    scan,
                    "version-folder-scan",
                    "使用官方游戏目录",
                    false,
                ) {
                    for directory in crate::game::discover_game_directories() {
                        self.settings.add_game_directory(directory);
                    }
                    self.show_toast(format!(
                        "官方 Minecraft 文件夹已载入 · 共保存 {} 个",
                        self.settings.game_directories.len()
                    ));
                }
            });
    }

    fn paint_version_content_dialog(
        &mut self,
        ctx: &egui::Context,
        screen: Rect,
        time: f64,
        kind: VersionContentKind,
    ) {
        let Some(version) = self.versions.get(self.version_page_selected).cloned() else {
            self.version_content_dialog = None;
            return;
        };
        let entries = if kind == VersionContentKind::Export {
            Vec::new()
        } else {
            self.version_data_directory_for(&version.version)
                .map(|directory| version_content_entries(&directory.join(kind.folder()), kind))
                .unwrap_or_default()
        };
        let dialog = version_content_dialog_rect(screen);
        let progress = self.version_content_popup_value;
        let source = if self.version_content_popup_source.width() > 0.0
            && self.version_content_popup_source.height() > 0.0
        {
            self.version_content_popup_source
        } else {
            dialog
        };
        let animated = progress.clamp(-0.05, 1.08);
        let travel = progress.clamp(0.0, 1.0);
        let travel = travel * travel * (3.0 - 2.0 * travel);
        let mut animated_center = source.center() + (screen.center() - source.center()) * travel;
        let close_bounce = if !self.version_content_dialog_open && progress <= 0.002 {
            let t =
                (self.version_content_close_elapsed / CONTENT_CLOSE_BOUNCE_SECONDS).clamp(0.0, 1.0);
            (t * std::f32::consts::PI).sin() * (1.0 - t) * (1.0 - t)
        } else {
            0.0
        };
        let return_direction = {
            let direction = source.center() - dialog.center();
            if direction.length_sq() > 0.001 {
                direction.normalized()
            } else {
                Vec2::Y
            }
        };
        animated_center += return_direction * 5.0 * close_bounce;
        let animated_size = (source.size() + (dialog.size() - source.size()) * animated)
            * (1.0 + close_bounce * 0.012);
        let morph_rect = Rect::from_center_size(animated_center, animated_size);
        let opacity = 1.0;
        let content_opacity = version_content_text_opacity(progress);
        let content_start_scale = ((source.width() / dialog.width())
            * (source.height() / dialog.height()))
        .sqrt()
        .clamp(0.08, 0.48);
        let content_scale = content_start_scale + (1.0 - content_start_scale) * animated;
        let content_center = animated_center;
        let content_transform = egui::emath::TSTransform::new(
            content_center - dialog.center() * content_scale,
            content_scale,
        );
        egui::Area::new(Id::unique("version-content-backdrop"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .movable(false)
            .show(ctx, |ui| {
                ui.set_min_size(screen.size());
                let mut shell_painter = ui.painter().clone();
                shell_painter.set_opacity(opacity);
                theme::glass_panel(
                    &shell_painter,
                    morph_rect,
                    theme::RADIUS,
                    theme::GlassSpec::new(Color32::from_rgb(21, 34, 25), theme::GREEN, time, 31.0)
                        .backdrop_blur(22.0),
                );
                let response = ui.interact(
                    screen,
                    Id::unique("version-content-backdrop-interact"),
                    Sense::click(),
                );
                if response.clicked()
                    && ui
                        .ctx()
                        .input(|input| input.pointer.interact_pos())
                        .is_some_and(|pointer| !dialog.contains(pointer))
                {
                    self.close_version_content_dialog();
                }
            });
        let mut dialog_layer = egui::LayerId::new(
            egui::Order::Foreground,
            Id::unique("version-content-dialog"),
        );
        egui::Area::new(Id::unique("version-content-dialog"))
            .order(egui::Order::Foreground)
            .fixed_pos(dialog.min)
            .movable(false)
            .show(ctx, |ui| {
                ui.set_min_size(dialog.size());
                dialog_layer = ui.layer_id();
                let mut painter = ui.painter().clone();
                painter.set_opacity(content_opacity);
                let content = card_clip(&painter, dialog);
                let painter = &content;
                let icon = match kind {
                    VersionContentKind::Mods => NavIcon::Box,
                    VersionContentKind::Saves => NavIcon::Folder,
                    VersionContentKind::ShaderPacks | VersionContentKind::ResourcePacks => {
                        NavIcon::Sparkles
                    }
                    VersionContentKind::Export => NavIcon::Layers,
                };
                draw_icon(
                    &painter,
                    icon,
                    Rect::from_center_size(
                        Pos2::new(dialog.left() + 31.0, dialog.top() + 30.0),
                        Vec2::splat(22.0),
                    ),
                    theme::GREEN_BRIGHT,
                );
                theme::bold_text(
                    &painter,
                    Pos2::new(dialog.left() + 52.0, dialog.top() + 17.0),
                    kind.label(),
                    15.0,
                    theme::INK,
                );
                theme::text(
                    &painter,
                    Pos2::new(dialog.left() + 52.0, dialog.top() + 39.0),
                    if kind == VersionContentKind::Export {
                        format!("{} · 选择要包含的内容", version.game_version)
                    } else {
                        format!(
                            "{} · {} 项 · {}",
                            version.game_version,
                            entries.len(),
                            if self.settings.version_isolation {
                                "版本独立目录"
                            } else {
                                "公共数据目录"
                            }
                        )
                    },
                    7.8,
                    theme::MUTED,
                );
                let close = version_content_close_rect(dialog);
                let close_response = ui
                    .interact(close, Id::unique("version-content-close"), Sense::click())
                    .on_hover_cursor(CursorIcon::PointingHand);
                theme::fill_round(
                    &painter,
                    close,
                    theme::RADIUS,
                    if close_response.hovered() {
                        theme::white_alpha(18)
                    } else {
                        theme::black_alpha(60)
                    },
                );
                draw_icon(
                    &painter,
                    NavIcon::Close,
                    close.shrink(10.0),
                    theme::INK_SOFT,
                );
                if close_response.clicked() {
                    self.close_version_content_dialog();
                }
                if kind == VersionContentKind::Export {
                    self.paint_export_contents(ui, &painter, dialog, time, &version);
                    return;
                }

                let list = Rect::from_min_max(
                    Pos2::new(dialog.left() + 18.0, dialog.top() + 68.0),
                    Pos2::new(dialog.right() - 18.0, dialog.bottom() - 16.0),
                );
                let gap = 10.0;
                let card_width = (list.width() - gap) * 0.5;
                let card_height = 72.0;
                let row_height = card_height + gap;
                let content_height = entries.len().div_ceil(2) as f32 * row_height;
                let max_scroll = (content_height - list.height()).max(0.0);
                if ui.ctx().input(|input| {
                    input
                        .pointer
                        .hover_pos()
                        .is_some_and(|pointer| list.contains(pointer))
                        && input.smooth_scroll_delta.y.abs() > 0.0
                }) {
                    self.version_content_scroll -=
                        ui.ctx().input(|input| input.smooth_scroll_delta.y);
                }
                self.version_content_scroll = self.version_content_scroll.clamp(0.0, max_scroll);
                let list_painter = painter.with_clip_rect(list);
                if entries.is_empty() {
                    theme::center_text(
                        &list_painter,
                        list.center(),
                        format!("这里还没有{}", kind.label()),
                        9.0,
                        theme::MUTED,
                        false,
                    );
                }
                for (index, entry) in entries.iter().enumerate() {
                    let column = index % 2;
                    let row = index / 2;
                    let card = Rect::from_min_size(
                        Pos2::new(
                            list.left() + column as f32 * (card_width + gap),
                            list.top() + row as f32 * row_height - self.version_content_scroll,
                        ),
                        Vec2::new(card_width, card_height),
                    );
                    if card.bottom() < list.top() || card.top() > list.bottom() {
                        continue;
                    }
                    let response = ui.interact(
                        card,
                        Id::unique(("version-content-entry", kind.index(), index)),
                        Sense::hover(),
                    );
                    theme::glass_panel(
                        &list_painter,
                        card,
                        theme::RADIUS,
                        theme::GlassSpec::new(
                            if response.hovered() {
                                Color32::from_rgb(29, 48, 33)
                            } else {
                                Color32::from_rgb(21, 35, 26)
                            },
                            theme::GREEN,
                            time,
                            31.5 + index as f32 * 0.15,
                        )
                        .hovered(response.hovered()),
                    );
                    draw_icon(
                        &list_painter,
                        icon,
                        Rect::from_center_size(
                            Pos2::new(card.left() + 25.0, card.center().y),
                            Vec2::splat(18.0),
                        ),
                        theme::GREEN_BRIGHT,
                    );
                    theme::bold_text(
                        &list_painter,
                        Pos2::new(card.left() + 44.0, card.top() + 15.0),
                        ellipsize_end(&entry.name, 38),
                        9.2,
                        theme::INK_SOFT,
                    );
                    theme::text(
                        &list_painter,
                        Pos2::new(card.left() + 44.0, card.top() + 39.0),
                        ellipsize_end(&entry.detail, 42),
                        7.6,
                        theme::MUTED,
                    );
                }
            });
        ctx.transform_layer_shapes(dialog_layer, content_transform);
    }

    fn paint_java_picker(&mut self, ctx: &egui::Context, screen: Rect, time: f64) {
        let Some(target) = self.java_picker_target.clone() else {
            return;
        };
        let option_count = self.java_runtimes.len() + 1;
        let visible = option_count.min(6);
        let rect = java_picker_rect(self.java_picker_anchor, screen, option_count);
        egui::Area::new(Id::unique("java-picker"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .movable(false)
            .show(ctx, |ui| {
                ui.set_min_size(rect.size());
                let painter = ui.painter().clone();
                theme::glass_panel(
                    &painter,
                    rect,
                    theme::RADIUS,
                    theme::GlassSpec::new(Color32::from_rgb(21, 34, 25), theme::GREEN, time, 34.0)
                        .backdrop_blur(16.0),
                );
                let content = card_clip(&painter, rect);
                let painter = &content;
                theme::bold_text(
                    &painter,
                    Pos2::new(rect.left() + 16.0, rect.top() + 10.0),
                    "选择 Java 运行时",
                    9.2,
                    theme::INK,
                );
                let list_top = rect.top() + 34.0;
                let list_height = visible as f32 * 42.0;
                let list_rect = Rect::from_min_size(
                    Pos2::new(rect.left() + 6.0, list_top),
                    Vec2::new(rect.width() - 12.0, list_height),
                );
                let max_scroll = (option_count as f32 * 42.0 - list_height).max(0.0);
                if ui.ctx().input(|input| {
                    input
                        .pointer
                        .hover_pos()
                        .is_some_and(|pointer| list_rect.contains(pointer))
                        && input.smooth_scroll_delta.y.abs() > 0.0
                }) {
                    self.java_picker_scroll -= ui.ctx().input(|input| input.smooth_scroll_delta.y);
                }
                self.java_picker_scroll = self.java_picker_scroll.clamp(0.0, max_scroll);
                let list_painter = painter.clone();
                let automatic = Rect::from_min_size(
                    Pos2::new(rect.left() + 9.0, list_top - self.java_picker_scroll),
                    Vec2::new(rect.width() - 18.0, 38.0),
                );
                let automatic_response = ui
                    .interact(automatic, Id::unique("java-picker-auto"), Sense::click())
                    .on_hover_cursor(CursorIcon::PointingHand);
                let automatic_selected = match &target {
                    JavaPickerTarget::Global => self.settings.java_path.is_none(),
                    JavaPickerTarget::Version { directory, profile } => self
                        .settings
                        .version_java_path(directory, profile)
                        .is_none(),
                };
                theme::fill_round(
                    &list_painter,
                    automatic,
                    theme::RADIUS,
                    if automatic_selected {
                        theme::alpha(theme::GREEN, 23)
                    } else if automatic_response.hovered() {
                        theme::white_alpha(10)
                    } else {
                        Color32::TRANSPARENT
                    },
                );
                theme::bold_text(
                    &list_painter,
                    Pos2::new(automatic.left() + 12.0, automatic.top() + 5.0),
                    match target {
                        JavaPickerTarget::Global => "自动选择兼容 Java",
                        JavaPickerTarget::Version { .. } => "跟随全局 Java 设置",
                    },
                    8.6,
                    theme::INK_SOFT,
                );
                theme::text(
                    &list_painter,
                    Pos2::new(automatic.left() + 12.0, automatic.top() + 21.0),
                    if self.settings.java_path.is_some() {
                        "当前全局已指定 Java"
                    } else {
                        "由启动器根据版本要求选择"
                    },
                    7.2,
                    theme::MUTED,
                );
                if automatic_response.clicked() {
                    match &target {
                        JavaPickerTarget::Global => self.settings.java_path = None,
                        JavaPickerTarget::Version { directory, profile } => self
                            .settings
                            .set_version_java_path(directory, profile, None),
                    }
                    self.java_picker_target = None;
                }
                for (index, runtime) in self.java_runtimes.iter().enumerate() {
                    let row = Rect::from_min_size(
                        Pos2::new(
                            rect.left() + 9.0,
                            list_top + (index + 1) as f32 * 42.0 - self.java_picker_scroll,
                        ),
                        Vec2::new(rect.width() - 18.0, 38.0),
                    );
                    if row.bottom() < list_top || row.top() > list_rect.bottom() {
                        continue;
                    }
                    let response = ui
                        .interact(row, Id::unique(("java-runtime", index)), Sense::click())
                        .on_hover_cursor(CursorIcon::PointingHand);
                    let selected = match &target {
                        JavaPickerTarget::Global => self
                            .settings
                            .java_path
                            .as_ref()
                            .is_some_and(|path| same_path(path, &runtime.path)),
                        JavaPickerTarget::Version { directory, profile } => self
                            .settings
                            .version_java_path(directory, profile)
                            .as_ref()
                            .is_some_and(|path| same_path(path, &runtime.path)),
                    };
                    theme::fill_round(
                        &list_painter,
                        row,
                        theme::RADIUS,
                        if selected {
                            theme::alpha(theme::GREEN, 23)
                        } else if response.hovered() {
                            theme::white_alpha(10)
                        } else {
                            Color32::TRANSPARENT
                        },
                    );
                    theme::bold_text(
                        &list_painter,
                        Pos2::new(row.left() + 12.0, row.top() + 5.0),
                        format!("Java {}", runtime.major),
                        8.6,
                        theme::INK_SOFT,
                    );
                    theme::text(
                        &list_painter,
                        Pos2::new(row.left() + 12.0, row.top() + 21.0),
                        ellipsize_start(&runtime.path.display().to_string(), 48),
                        7.1,
                        theme::MUTED,
                    );
                    if response.clicked() {
                        match &target {
                            JavaPickerTarget::Global => {
                                self.settings.java_path = Some(runtime.path.clone())
                            }
                            JavaPickerTarget::Version { directory, profile } => self
                                .settings
                                .set_version_java_path(directory, profile, Some(&runtime.path)),
                        }
                        self.java_picker_target = None;
                    }
                }
                let refresh = Rect::from_min_size(
                    Pos2::new(rect.left() + 9.0, list_top + list_height + 4.0),
                    Vec2::new(rect.width() - 18.0, 32.0),
                );
                if settings_button(
                    ui,
                    &painter,
                    refresh,
                    "java-picker-refresh",
                    "重新扫描 Java",
                    false,
                ) {
                    self.refresh_java_runtimes();
                }
            });
    }

    fn paint_export_contents(
        &mut self,
        ui: &mut egui::Ui,
        painter: &Painter,
        dialog: Rect,
        time: f64,
        version: &GameVersion,
    ) {
        let options = [
            (
                "export-mods",
                "模组",
                "包含 mods 目录中的已启用模组",
                &mut self.export_include_mods,
            ),
            (
                "export-config",
                "模组配置",
                "包含 config 与默认配置",
                &mut self.export_include_config,
            ),
            (
                "export-saves",
                "存档",
                "包含 saves 目录中的世界（可能较大）",
                &mut self.export_include_saves,
            ),
            (
                "export-resourcepacks",
                "材质包",
                "包含 resourcepacks 目录",
                &mut self.export_include_resourcepacks,
            ),
            (
                "export-shaderpacks",
                "光影包",
                "包含 shaderpacks 目录",
                &mut self.export_include_shaderpacks,
            ),
        ];
        for (index, (id, title, description, value)) in options.into_iter().enumerate() {
            let row = Rect::from_min_size(
                Pos2::new(
                    dialog.left() + 20.0,
                    dialog.top() + 68.0 + index as f32 * 52.0,
                ),
                Vec2::new(dialog.width() - 40.0, 46.0),
            );
            paint_setting_card(painter, row, time);
            paint_toggle_row(ui, painter, row, id, title, description, value);
        }
        let hint = Rect::from_min_size(
            Pos2::new(dialog.left() + 20.0, dialog.bottom() - 92.0),
            Vec2::new(dialog.width() - 40.0, 30.0),
        );
        theme::fill_round(
            painter,
            hint,
            theme::RADIUS,
            theme::alpha(Color32::from_rgb(220, 178, 89), 15),
        );
        theme::text(
            painter,
            Pos2::new(hint.left() + 12.0, hint.top() + 8.0),
            "请确认整合包中的资源分发许可，避免公开上传受限内容。",
            7.8,
            Color32::from_rgb(208, 176, 112),
        );
        let cancel = Rect::from_min_size(
            Pos2::new(dialog.left() + 20.0, dialog.bottom() - 50.0),
            Vec2::new(96.0, 36.0),
        );
        if settings_button(ui, painter, cancel, "export-cancel", "取消", false) {
            self.close_version_content_dialog();
        }
        let export = Rect::from_min_size(
            Pos2::new(dialog.right() - 158.0, dialog.bottom() - 50.0),
            Vec2::new(138.0, 36.0),
        );
        if settings_button(ui, painter, export, "export-start", "选择位置并导出", true) {
            let default_name = format!("{}.zip", sanitize_filename(&version.version));
            if let Some(destination) = pick_export_file(&default_name)
                && let Some(base) = self.settings.game_directory.clone()
            {
                let data_directory = self
                    .version_data_directory_for(&version.version)
                    .unwrap_or_else(|| base.clone());
                let result = export_version_archive(
                    &base,
                    &data_directory,
                    version,
                    &destination,
                    &VersionExportOptions {
                        mods: self.export_include_mods,
                        config: self.export_include_config,
                        saves: self.export_include_saves,
                        resourcepacks: self.export_include_resourcepacks,
                        shaderpacks: self.export_include_shaderpacks,
                    },
                );
                match result {
                    Ok(()) => {
                        self.close_version_content_dialog();
                        self.show_toast(format!("已导出到 {}", destination.display()));
                        open_path(destination.parent().unwrap_or(&base));
                    }
                    Err(error) => self.show_toast(format!("导出失败：{error}")),
                }
            }
        }
    }

    #[allow(dead_code)]
    fn paint_version_export_dialog(&mut self, ctx: &egui::Context, screen: Rect, time: f64) {
        let Some(version) = self.versions.get(self.version_page_selected).cloned() else {
            self.version_export_open = false;
            return;
        };
        let dialog = version_export_dialog_rect(screen);
        egui::Area::new(Id::unique("version-export-dialog"))
            .order(egui::Order::Foreground)
            .fixed_pos(dialog.min)
            .movable(false)
            .show(ctx, |ui| {
                ui.set_min_size(dialog.size());
                let painter = ui.painter().clone();
                theme::glass_panel(
                    &painter,
                    dialog,
                    theme::RADIUS,
                    theme::GlassSpec::new(Color32::from_rgb(21, 33, 25), theme::GREEN, time, 35.0)
                        .backdrop_blur(18.0),
                );
                let content = card_clip(&painter, dialog);
                let painter = &content;
                theme::bold_text(
                    &painter,
                    Pos2::new(dialog.left() + 22.0, dialog.top() + 17.0),
                    "导出整合包",
                    17.0,
                    theme::INK,
                );
                theme::text(
                    &painter,
                    Pos2::new(dialog.left() + 22.0, dialog.top() + 44.0),
                    format!(
                        "{} · 版本配置和加载器信息始终包含",
                        self.version_selector_label(&version)
                    ),
                    8.1,
                    theme::MUTED,
                );
                let close = Rect::from_min_size(
                    Pos2::new(dialog.right() - 44.0, dialog.top() + 16.0),
                    Vec2::splat(28.0),
                );
                let close_response = ui
                    .interact(close, Id::unique("export-close"), Sense::click())
                    .on_hover_cursor(CursorIcon::PointingHand);
                theme::fill_round(
                    &painter,
                    close,
                    theme::RADIUS,
                    if close_response.hovered() {
                        theme::white_alpha(18)
                    } else {
                        theme::black_alpha(60)
                    },
                );
                draw_icon(&painter, NavIcon::Close, close.shrink(8.0), theme::INK_SOFT);
                if close_response.clicked() {
                    self.version_export_open = false;
                }
                let options = [
                    (
                        "export-mods",
                        "模组",
                        "包含 mods 目录中的已启用模组",
                        &mut self.export_include_mods,
                    ),
                    (
                        "export-config",
                        "模组配置",
                        "包含 config 与默认配置",
                        &mut self.export_include_config,
                    ),
                    (
                        "export-saves",
                        "存档",
                        "包含 saves 目录中的世界（可能较大）",
                        &mut self.export_include_saves,
                    ),
                    (
                        "export-resourcepacks",
                        "材质包",
                        "包含 resourcepacks 目录",
                        &mut self.export_include_resourcepacks,
                    ),
                    (
                        "export-shaderpacks",
                        "光影包",
                        "包含 shaderpacks 目录",
                        &mut self.export_include_shaderpacks,
                    ),
                ];
                for (index, (id, title, description, value)) in options.into_iter().enumerate() {
                    let row = Rect::from_min_size(
                        Pos2::new(
                            dialog.left() + 20.0,
                            dialog.top() + 75.0 + index as f32 * 52.0,
                        ),
                        Vec2::new(dialog.width() - 40.0, 46.0),
                    );
                    paint_setting_card(&painter, row, time);
                    paint_toggle_row(ui, &painter, row, id, title, description, value);
                }
                let hint = Rect::from_min_size(
                    Pos2::new(dialog.left() + 20.0, dialog.bottom() - 92.0),
                    Vec2::new(dialog.width() - 40.0, 30.0),
                );
                theme::fill_round(
                    &painter,
                    hint,
                    theme::RADIUS,
                    theme::alpha(Color32::from_rgb(220, 178, 89), 15),
                );
                theme::text(
                    &painter,
                    Pos2::new(hint.left() + 12.0, hint.top() + 8.0),
                    "请确认整合包中的资源分发许可，避免公开上传受限内容。",
                    7.8,
                    Color32::from_rgb(208, 176, 112),
                );
                let cancel = Rect::from_min_size(
                    Pos2::new(dialog.left() + 20.0, dialog.bottom() - 50.0),
                    Vec2::new(96.0, 36.0),
                );
                if settings_button(ui, &painter, cancel, "export-cancel", "取消", false) {
                    self.version_export_open = false;
                }
                let export = Rect::from_min_size(
                    Pos2::new(dialog.right() - 158.0, dialog.bottom() - 50.0),
                    Vec2::new(138.0, 36.0),
                );
                if settings_button(ui, &painter, export, "export-start", "选择位置并导出", true)
                {
                    let default_name = format!("{}.zip", sanitize_filename(&version.version));
                    if let Some(destination) = pick_export_file(&default_name)
                        && let Some(base) = self.settings.game_directory.clone()
                    {
                        let data_directory = self
                            .version_data_directory_for(&version.version)
                            .unwrap_or_else(|| base.clone());
                        let result = export_version_archive(
                            &base,
                            &data_directory,
                            &version,
                            &destination,
                            &VersionExportOptions {
                                mods: self.export_include_mods,
                                config: self.export_include_config,
                                saves: self.export_include_saves,
                                resourcepacks: self.export_include_resourcepacks,
                                shaderpacks: self.export_include_shaderpacks,
                            },
                        );
                        match result {
                            Ok(()) => {
                                self.version_export_open = false;
                                self.show_toast(format!("已导出到 {}", destination.display()));
                                open_path(destination.parent().unwrap_or(&base));
                            }
                            Err(error) => self.show_toast(format!("导出失败：{error}")),
                        }
                    }
                }
            });
    }

    fn paint_version_detail(
        &mut self,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
        view: VersionDetailView,
    ) {
        let selected = view.version;
        let motion = view.motion;
        let page = Rect::from_min_max(
            Pos2::new(layout.content_left, layout.header.bottom() + 14.0),
            Pos2::new(layout.content_right, layout.screen.bottom() - 22.0),
        );
        let list_width = (page.width() * 0.49).clamp(430.0, 590.0);
        let source_detail_panel = Rect::from_min_max(
            Pos2::new(page.left() + list_width + 14.0, page.top()),
            page.right_bottom(),
        );
        let painter = painter.clone();
        let painter = &painter;
        let detail_heading = motion.rect(Rect::from_min_size(
            source_detail_panel.left_top(),
            Vec2::new(source_detail_panel.width(), 76.0),
        ));
        let painter_base = painter;
        theme::glass_panel(
            painter,
            detail_heading,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(20, 32, 24), theme::GREEN, time, 17.0),
        );
        let detail_heading_content = card_clip(painter, detail_heading);
        let painter = &detail_heading_content;
        let heading = Pos2::new(detail_heading.left() + 22.0, detail_heading.top() + 12.0);
        theme::text(painter, heading, "版本详情", 8.0, theme::MUTED);
        let detail_title = self.version_selector_label(&selected);
        let detail_title = fit_text_to_width(
            painter,
            &detail_title,
            theme::bold_font(20.0),
            detail_heading.width() - 44.0,
        );
        theme::bold_text(
            painter,
            heading + Vec2::new(0.0, 18.0),
            detail_title,
            20.0,
            theme::INK,
        );
        let content_width = (source_detail_panel.width() - 52.0).max(1.0);
        let summary = motion.rect(Rect::from_min_size(
            Pos2::new(
                source_detail_panel.left() + 26.0,
                source_detail_panel.top() + 94.0,
            ),
            Vec2::new(content_width, 116.0),
        ));
        let painter = painter_base;
        theme::glass_panel(
            painter,
            summary,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(22, 35, 26), theme::GREEN, time, 17.8),
        );
        let summary_content = card_clip(painter, summary);
        let painter = &summary_content;
        let detail_icon = Rect::from_center_size(
            Pos2::new(summary.left() + 54.0, summary.center().y),
            Vec2::splat(64.0),
        );
        if let Some(texture) = self.version_icon(&selected) {
            paint_rounded_image(painter, detail_icon, 11.0, texture, texture.size());
        } else {
            paint_block(
                painter,
                detail_icon.center(),
                64.0,
                Color32::from_rgb(122, 210, 103),
                Color32::from_rgb(86, 64, 44),
                Color32::from_rgb(68, 143, 69),
            );
        }
        let info_x = summary.left() + 100.0;
        theme::text(
            painter,
            Pos2::new(info_x, summary.top() + 20.0),
            "加载器",
            7.9,
            theme::MUTED,
        );
        theme::bold_text(
            painter,
            Pos2::new(info_x, summary.top() + 35.0),
            ellipsize_end(&selected.loader, 30),
            11.0,
            theme::INK_SOFT,
        );
        theme::text(
            painter,
            Pos2::new(info_x, summary.top() + 64.0),
            "Java 要求",
            7.9,
            theme::MUTED,
        );
        theme::text(
            painter,
            Pos2::new(info_x, summary.top() + 79.0),
            &selected.java,
            9.5,
            theme::INK_SOFT,
        );
        theme::text(
            painter,
            Pos2::new(summary.right() - 150.0, summary.top() + 20.0),
            if self.settings.version_isolation {
                "独立数据目录"
            } else {
                "公共数据目录"
            },
            7.9,
            theme::GREEN_BRIGHT,
        );

        let base_directory = self.settings.game_directory.clone();
        let data_directory = self
            .version_data_directory_for(&selected.version)
            .unwrap_or_else(|| base_directory.clone().unwrap_or_default());
        let default_memory = self.settings.memory_mb;
        let override_memory = base_directory.as_ref().and_then(|directory| {
            self.settings
                .version_memory_mb(directory, &selected.version)
        });
        let memory = override_memory.unwrap_or(default_memory);
        let memory_card = motion.rect(Rect::from_min_size(
            Pos2::new(summary.left(), summary.bottom() + 10.0),
            Vec2::new(content_width, 78.0),
        ));
        let painter = painter_base;
        theme::glass_panel(
            painter,
            memory_card,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(21, 34, 25), theme::GREEN, time, 18.1),
        );
        let memory_content = card_clip(painter, memory_card);
        let painter = &memory_content;
        theme::bold_text(
            painter,
            Pos2::new(memory_card.left() + 18.0, memory_card.top() + 13.0),
            "内存分配",
            10.0,
            theme::INK,
        );
        theme::text(
            painter,
            Pos2::new(memory_card.left() + 18.0, memory_card.top() + 36.0),
            if override_memory.is_some() {
                "当前版本独立设置"
            } else {
                "当前跟随全局设置"
            },
            7.8,
            theme::MUTED,
        );
        let memory_control_center = Pos2::new(memory_card.right() - 135.0, memory_card.center().y);
        theme::center_text(
            painter,
            memory_control_center,
            format!("{:.1} GB", memory as f32 / 1024.0),
            13.0,
            theme::INK,
            true,
        );
        if view.active {
            let minus = Rect::from_center_size(
                Pos2::new(memory_card.right() - 220.0, memory_card.center().y),
                Vec2::splat(30.0),
            );
            let plus = Rect::from_center_size(
                Pos2::new(memory_card.right() - 50.0, memory_card.center().y),
                Vec2::splat(30.0),
            );
            if compact_icon_button(ui, painter, minus, "version-memory-minus", false)
                && let Some(directory) = base_directory.as_ref()
            {
                self.settings.set_version_memory(
                    directory,
                    &selected.version,
                    memory.saturating_sub(512).max(2048),
                );
            }
            if compact_icon_button(ui, painter, plus, "version-memory-plus", true)
                && let Some(directory) = base_directory.as_ref()
            {
                self.settings.set_version_memory(
                    directory,
                    &selected.version,
                    (memory + 512).min(16384),
                );
            }
            if override_memory.is_some() {
                let reset = Rect::from_min_size(
                    Pos2::new(minus.left() - 96.0, memory_card.center().y - 12.0),
                    Vec2::new(88.0, 24.0),
                );
                if settings_button(
                    ui,
                    painter,
                    reset,
                    "version-memory-reset",
                    "跟随全局",
                    false,
                ) && let Some(directory) = base_directory.as_ref()
                {
                    self.settings
                        .clear_version_memory(directory, &selected.version);
                }
            }
        }

        let version_java = base_directory.as_ref().and_then(|directory| {
            self.settings
                .version_java_path(directory, &selected.version)
        });
        let effective_java = version_java
            .clone()
            .or_else(|| self.settings.java_path.clone());
        let java_card = motion.rect(Rect::from_min_size(
            Pos2::new(memory_card.left(), memory_card.bottom() + 10.0),
            Vec2::new(content_width, 78.0),
        ));
        let painter = painter_base;
        theme::glass_panel(
            painter,
            java_card,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(21, 34, 25), theme::GREEN, time, 18.2),
        );
        let java_content = card_clip(painter, java_card);
        let painter = &java_content;
        theme::bold_text(
            painter,
            Pos2::new(java_card.left() + 18.0, java_card.top() + 13.0),
            "Java 运行时",
            10.0,
            theme::INK,
        );
        let java_description = match effective_java.as_ref() {
            Some(path) => {
                let prefix = if version_java.is_some() {
                    "版本指定"
                } else {
                    "跟随全局"
                };
                format!("{prefix} · {}", self.java_runtime_label(path))
            }
            None => "自动选择兼容版本".to_owned(),
        };
        theme::text(
            painter,
            Pos2::new(java_card.left() + 18.0, java_card.top() + 37.0),
            ellipsize_end(&java_description, 58),
            7.8,
            theme::MUTED,
        );
        if view.active {
            let select = Rect::from_min_size(
                Pos2::new(java_card.right() - 96.0, java_card.center().y - 16.0),
                Vec2::new(78.0, 32.0),
            );
            if settings_button(ui, painter, select, "version-java-select", "选择", false)
                && let Some(directory) = base_directory.clone()
            {
                self.java_picker_anchor = select;
                self.java_picker_target = Some(JavaPickerTarget::Version {
                    directory,
                    profile: selected.version.clone(),
                });
                self.java_picker_scroll = 0.0;
            }
        }

        let control_top = java_card.bottom() + 10.0;
        let gap = 10.0;
        let control_width = (content_width - gap) * 0.5;
        let controls = [
            (
                "mods",
                "模组",
                installed_mod_files(&data_directory).len(),
                NavIcon::Box,
            ),
            (
                "saves",
                "存档",
                count_directories(&data_directory.join("saves")),
                NavIcon::Folder,
            ),
            (
                "shaderpacks",
                "光影",
                count_content_packs(&data_directory.join("shaderpacks")),
                NavIcon::Sparkles,
            ),
            (
                "resourcepacks",
                "材质包",
                count_content_packs(&data_directory.join("resourcepacks")),
                NavIcon::Sparkles,
            ),
            ("export", "导出", 0, NavIcon::Layers),
        ];
        for (index, (folder, label, count, icon)) in controls.into_iter().enumerate() {
            let content_kind = match folder {
                "mods" => Some(VersionContentKind::Mods),
                "saves" => Some(VersionContentKind::Saves),
                "shaderpacks" => Some(VersionContentKind::ShaderPacks),
                "resourcepacks" => Some(VersionContentKind::ResourcePacks),
                "export" => Some(VersionContentKind::Export),
                _ => None,
            };
            let column = index % 2;
            let row = index / 2;
            let card = motion.rect(Rect::from_min_size(
                Pos2::new(
                    summary.left() + column as f32 * (control_width + gap),
                    control_top + row as f32 * 92.0,
                ),
                Vec2::new(control_width, 82.0),
            ));
            let source_card_hidden = self.version_content_dialog == content_kind;
            if source_card_hidden && self.version_content_dialog == content_kind {
                continue;
            }
            let response = view.active.then(|| {
                ui.interact(
                    card,
                    Id::unique(("version-control", view.index, index)),
                    Sense::click(),
                )
                .on_hover_cursor(CursorIcon::PointingHand)
            });
            let hovered = response.as_ref().is_some_and(|response| response.hovered());
            let painter = painter_base;
            theme::glass_panel(
                painter,
                card,
                theme::RADIUS,
                theme::GlassSpec::new(
                    if hovered {
                        Color32::from_rgb(28, 46, 32)
                    } else {
                        Color32::from_rgb(21, 34, 25)
                    },
                    theme::GREEN,
                    time,
                    18.4 + index as f32 * 0.3,
                )
                .hovered(hovered),
            );
            let card_content = card_clip(painter, card);
            let painter = &card_content;
            draw_icon(
                painter,
                icon,
                Rect::from_center_size(
                    Pos2::new(card.left() + 27.0, card.center().y),
                    Vec2::splat(20.0),
                ),
                theme::GREEN_BRIGHT,
            );
            theme::bold_text(
                painter,
                Pos2::new(card.left() + 48.0, card.top() + 17.0),
                label,
                10.0,
                theme::INK_SOFT,
            );
            theme::text(
                painter,
                Pos2::new(card.left() + 48.0, card.top() + 39.0),
                match folder {
                    "mods" => format!("{count} 个模组"),
                    "saves" => format!("{count} 个存档"),
                    "shaderpacks" => format!("{count} 个光影包"),
                    "resourcepacks" => format!("{count} 个材质包"),
                    _ => "导出整合包".to_owned(),
                },
                8.0,
                theme::MUTED,
            );
            theme::text(
                painter,
                Pos2::new(card.right() - 17.0, card.top() + 17.0),
                "→",
                9.0,
                theme::MUTED,
            );
            if response.as_ref().is_some_and(|response| response.clicked())
                && view.active
                && let Some(kind) = content_kind
            {
                self.open_version_content_dialog(kind, card);
            }
        }

        let painter = painter_base;
        if view.active {
            let detail_panel = motion.rect(source_detail_panel);
            let actions_y = detail_panel.bottom() - 57.0;
            let open = Rect::from_min_size(
                Pos2::new(detail_panel.left() + 26.0, actions_y),
                Vec2::new(104.0, 38.0),
            );
            let apply = Rect::from_min_size(
                Pos2::new(detail_panel.right() - 142.0, actions_y),
                Vec2::new(116.0, 38.0),
            );
            if settings_button(ui, painter, open, "version-open-folder", "打开目录", false)
                && let Some(game_directory) = self.settings.game_directory.as_ref()
            {
                let path = game_directory.join("versions").join(&selected.version);
                #[cfg(target_os = "windows")]
                let _ = std::process::Command::new("explorer").arg(path).spawn();
                #[cfg(not(target_os = "windows"))]
                let _ = std::process::Command::new("xdg-open").arg(path).spawn();
            }
            let already_active = self.selected_version == view.index;
            let apply_label = if already_active {
                "当前版本"
            } else {
                "设为当前版本"
            };
            if settings_button(
                ui,
                painter,
                apply,
                "version-apply",
                apply_label,
                !already_active,
            ) && !already_active
            {
                self.selected_version = view.index;
                self.refresh_recent_sessions();
                self.show_toast(format!("已将 {} 设为当前版本", selected.version));
            }
        }
    }

    fn paint_download(
        &mut self,
        _ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
        motion: CardMotion,
    ) {
        let outer_page = Rect::from_min_max(
            Pos2::new(layout.content_left, layout.header.bottom() + 14.0),
            Pos2::new(layout.content_right, layout.screen.bottom() - 22.0),
        );
        let options: Vec<(Option<ResourceCategory>, &str)> = std::iter::once((None, "原版"))
            .chain(
                ResourceCategory::ALL
                    .into_iter()
                    .map(|category| (Some(category), category.label())),
            )
            .collect();
        let category_bar = motion.rect(Rect::from_min_size(
            outer_page.left_top(),
            Vec2::new(outer_page.width(), 44.0),
        ));
        let page_base = painter;
        theme::glass_panel(
            painter,
            category_bar,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(19, 31, 23), theme::GREEN, time, 27.2),
        );
        let category_content = card_clip(painter, category_bar);
        let painter = &category_content;
        let tab_gap = 7.0;
        let tab_width = (category_bar.width() - 20.0 - tab_gap * 5.0) / 6.0;
        let selected_index = resource_category_index(self.download_resource_category);
        let indicator_center = category_bar.left()
            + 10.0
            + self.download_category_position.clamp(0.0, 5.0) * (tab_width + tab_gap)
            + tab_width * 0.5;
        let indicator_stretch = (self.download_category_velocity.abs() * 4.0).clamp(0.0, 16.0);
        let indicator = Rect::from_center_size(
            Pos2::new(indicator_center, category_bar.top() + 22.0),
            Vec2::new(tab_width + indicator_stretch, 32.0),
        );
        theme::fill_round(
            painter,
            indicator,
            theme::RADIUS,
            theme::alpha(theme::GREEN, 28),
        );
        theme::stroke_round(
            painter,
            indicator,
            theme::RADIUS,
            1.0,
            theme::alpha(theme::GREEN_BRIGHT, 56),
        );
        for (index, (category, label)) in options.into_iter().enumerate() {
            let tab = Rect::from_min_size(
                Pos2::new(
                    category_bar.left() + 10.0 + index as f32 * (tab_width + tab_gap),
                    category_bar.top() + 6.0,
                ),
                Vec2::new(tab_width, 32.0),
            );
            let response = ui
                .interact(
                    tab,
                    Id::unique(("download-category", index)),
                    Sense::click(),
                )
                .on_hover_cursor(CursorIcon::PointingHand);
            let selected = index == selected_index;
            if !selected {
                theme::fill_round(
                    painter,
                    tab,
                    theme::RADIUS,
                    if response.hovered() {
                        theme::white_alpha(11)
                    } else {
                        theme::white_alpha(5)
                    },
                );
            }
            theme::center_text(
                painter,
                tab.center(),
                label,
                8.3,
                if selected { theme::INK } else { theme::MUTED },
                selected,
            );
            if response.clicked() && !selected {
                self.begin_download_category_switch(ui.ctx(), category);
            }
        }
        let painter = page_base;
        let content_top = category_bar
            .bottom()
            .clamp(outer_page.top(), outer_page.bottom());
        let content_clip = Rect::from_min_max(
            Pos2::new(outer_page.left(), content_top),
            outer_page.right_bottom(),
        );
        let body_base = Rect::from_min_max(
            Pos2::new(outer_page.left(), outer_page.top() + 54.0),
            outer_page.right_bottom(),
        );
        let transition_layer = painter.with_clip_rect(content_clip);
        let painter = &transition_layer;
        if let Some(snapshot) = self.download_switch_texture.as_ref()
            && self.download_switch_texture_size[0] > 0
            && self.download_switch_texture_size[1] > 0
        {
            let outgoing_offset = -self.download_page_direction
                * content_clip.width()
                * self.download_page_progress.clamp(0.0, 1.08);
            let outgoing_rect = content_clip.translate(Vec2::new(outgoing_offset, 0.0));
            let uv = Rect::from_min_max(
                Pos2::new(
                    ((content_clip.left() - layout.screen.left()) / layout.screen.width())
                        .clamp(0.0, 1.0),
                    ((content_clip.top() - layout.screen.top()) / layout.screen.height())
                        .clamp(0.0, 1.0),
                ),
                Pos2::new(
                    ((content_clip.right() - layout.screen.left()) / layout.screen.width())
                        .clamp(0.0, 1.0),
                    ((content_clip.bottom() - layout.screen.top()) / layout.screen.height())
                        .clamp(0.0, 1.0),
                ),
            );
            painter.image(snapshot.id(), outgoing_rect, uv, Color32::WHITE);
        }
        let slide_offset = self.download_page_direction
            * content_clip.width()
            * (1.0 - self.download_page_progress.clamp(0.0, 1.08));
        let page = body_base.translate(Vec2::new(slide_offset, 0.0));
        if self.download_resource_category.is_some() {
            self.paint_resource_download(ui, painter, page, time, motion);
            return;
        }
        let list_width = (page.width() * 0.44).clamp(390.0, 540.0);
        let source_list = Rect::from_min_max(
            page.left_top(),
            Pos2::new(page.left() + list_width, page.bottom()),
        );
        let list_panel = motion.rect(source_list);
        let list_heading =
            Rect::from_min_size(list_panel.left_top(), Vec2::new(list_panel.width(), 148.0));
        let page_painter = painter;
        theme::glass_panel(
            painter,
            list_heading,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(20, 32, 24), theme::GREEN, time, 28.0),
        );
        let list_heading_content = card_clip(painter, list_heading);
        let painter = &list_heading_content;
        theme::bold_text(
            painter,
            Pos2::new(list_heading.left() + 22.0, list_heading.top() + 15.0),
            "下载游戏版本",
            19.0,
            theme::INK,
        );
        theme::text(
            painter,
            Pos2::new(list_heading.left() + 22.0, list_heading.top() + 47.0),
            if let Some(error) = &self.official_error {
                ellipsize_end(error, 42)
            } else if self.official_manifest.is_some() {
                "正在连接 Mojang Piston Meta…".to_owned()
            } else {
                format!(
                    "Mojang 官方源 · {} 个可下载版本",
                    self.official_versions.len()
                )
            },
            8.4,
            theme::MUTED,
        );
        let refresh = Rect::from_min_size(
            Pos2::new(list_heading.right() - 91.0, list_heading.top() + 14.0),
            Vec2::new(69.0, 30.0),
        );
        if settings_button(ui, painter, refresh, "download-refresh", "刷新", false)
            && self.official_manifest.is_none()
        {
            self.refresh_official_manifest();
        }

        let search = Rect::from_min_size(
            Pos2::new(list_heading.left() + 22.0, list_heading.top() + 64.0),
            Vec2::new(list_heading.width() - 44.0, 34.0),
        );
        theme::fill_round(
            painter,
            search,
            theme::RADIUS,
            Color32::from_rgb(25, 38, 29),
        );
        theme::stroke_round(painter, search, theme::RADIUS, 1.0, theme::white_alpha(30));
        draw_icon(
            painter,
            NavIcon::Search,
            Rect::from_center_size(
                Pos2::new(search.left() + 17.0, search.center().y),
                Vec2::splat(13.0),
            ),
            theme::MUTED,
        );
        let _ = ui.put(
            Rect::from_min_max(
                Pos2::new(search.left() + 31.0, search.top() + 5.0),
                Pos2::new(search.right() - 10.0, search.bottom() - 5.0),
            ),
            egui::TextEdit::singleline(&mut self.download_search)
                .frame(egui::Frame::NONE)
                .font(theme::font(8.8))
                .text_color(theme::INK)
                .hint_text("搜索 Minecraft 版本号"),
        );

        let filters = [
            (VersionFilter::All, "全部"),
            (VersionFilter::Release, "正式版"),
            (VersionFilter::Snapshot, "快照"),
        ];
        let mut filter_x = list_heading.left() + 22.0;
        for (index, (filter, label)) in filters.into_iter().enumerate() {
            let button = Rect::from_min_size(
                Pos2::new(filter_x, list_heading.top() + 108.0),
                Vec2::new(66.0, 27.0),
            );
            let response = ui
                .interact(
                    button,
                    Id::unique(("download-filter", index)),
                    Sense::click(),
                )
                .on_hover_cursor(CursorIcon::PointingHand);
            let selected = self.official_filter == filter;
            theme::fill_round(
                painter,
                button,
                theme::RADIUS,
                if selected {
                    theme::alpha(theme::GREEN, 27)
                } else if response.hovered() {
                    theme::white_alpha(14)
                } else {
                    theme::white_alpha(7)
                },
            );
            theme::center_text(
                painter,
                button.center(),
                label,
                8.4,
                if selected { theme::INK } else { theme::MUTED },
                selected,
            );
            if response.clicked() {
                self.official_filter = filter;
                self.official_scroll = 0.0;
                self.previous_download_detail = None;
                self.download_detail_progress = 1.0;
            }
            filter_x = button.right() + 7.0;
        }

        let search_query = self.download_search.trim().to_lowercase();
        let filtered: Vec<(usize, crate::download::OfficialVersion)> = self
            .official_versions
            .iter()
            .enumerate()
            .filter(|(_, version)| {
                version.id.to_lowercase().contains(&search_query)
                    && match self.official_filter {
                        VersionFilter::All => true,
                        VersionFilter::Release => version.kind == "release",
                        VersionFilter::Snapshot => version.kind == "snapshot",
                    }
            })
            .map(|(index, version)| (index, version.clone()))
            .collect();
        if !filtered.is_empty()
            && !filtered
                .iter()
                .any(|(index, _)| *index == self.official_selected)
        {
            self.official_selected = filtered[0].0;
            self.reset_download_loader();
            self.previous_download_detail = None;
            self.download_detail_progress = 1.0;
        }

        let painter = page_painter;
        let source_list_top = source_list.top() + 156.0;
        let list_top = list_panel.top() + 156.0;
        let list_bottom = list_panel.bottom();
        let list_height = (list_bottom - list_top).max(1.0);
        let row_height = 58.0;
        let content_height = filtered.len() as f32 * row_height;
        let max_scroll = (content_height - list_height).max(0.0);
        if ui.ctx().input(|input| {
            input
                .pointer
                .hover_pos()
                .is_some_and(|pointer| list_panel.contains(pointer))
                && input.smooth_scroll_delta.y.abs() > 0.0
        }) {
            self.official_scroll -= ui.ctx().input(|input| input.smooth_scroll_delta.y) * 1.05;
        }
        self.official_scroll = self.official_scroll.clamp(0.0, max_scroll);
        let list_viewport = Rect::from_min_max(
            Pos2::new(list_panel.left() + 2.0, list_top),
            Pos2::new(list_panel.right() - 9.0, list_bottom),
        );
        let list_painter = painter.with_clip_rect(list_viewport);
        for (visible_index, (global_index, version)) in filtered.iter().enumerate() {
            let source_y =
                source_list_top + visible_index as f32 * row_height - self.official_scroll;
            if source_y + row_height < source_list_top || source_y > source_list.bottom() {
                continue;
            }
            let row = motion.rect(Rect::from_min_size(
                Pos2::new(source_list.left() + 4.0, source_y),
                Vec2::new(source_list.width() - 14.0, row_height - 4.0),
            ));
            let response = ui
                .interact(
                    row,
                    Id::unique(("download-version", global_index)),
                    Sense::click(),
                )
                .on_hover_cursor(CursorIcon::PointingHand);
            let selected = self.official_selected == *global_index;
            let card = row.shrink2(Vec2::new(2.0, 2.0));
            theme::glass_panel(
                &list_painter,
                card,
                theme::RADIUS,
                theme::GlassSpec::new(
                    if selected || response.hovered() {
                        Color32::from_rgb(29, 48, 33)
                    } else {
                        Color32::from_rgb(20, 33, 24)
                    },
                    theme::GREEN,
                    time,
                    row.top() * 0.01,
                )
                .hovered(response.hovered()),
            );
            let row_painter = card_clip(&list_painter, card);
            if selected {
                theme::stroke_round(
                    &list_painter,
                    card,
                    theme::RADIUS,
                    1.0,
                    theme::alpha(theme::GREEN, 74),
                );
            }
            let icon = Rect::from_center_size(
                Pos2::new(row.left() + 21.0, row.center().y),
                Vec2::splat(28.0),
            );
            let texture = if version.kind == "snapshot" {
                self.snapshot_icon_texture.as_ref()
            } else {
                self.release_icon_texture.as_ref()
            };
            if let Some(texture) = texture {
                paint_rounded_image(&row_painter, icon, 8.0, texture, texture.size());
            }
            theme::bold_text(
                &row_painter,
                Pos2::new(row.left() + 42.0, row.top() + 9.0),
                &version.id,
                10.3,
                theme::INK_SOFT,
            );
            theme::text(
                &row_painter,
                Pos2::new(row.left() + 42.0, row.top() + 29.0),
                download_version_kind(&version.kind),
                7.6,
                theme::MUTED,
            );
            if response.clicked() {
                self.switch_download_detail(*global_index);
            }
        }
        if filtered.is_empty() {
            theme::center_text(
                &list_painter,
                Pos2::new(source_list.center().x, list_top + 64.0),
                if self.official_manifest.is_some() {
                    "正在获取官方版本…"
                } else if let Some(error) = &self.official_error {
                    error
                } else if !self.download_search.trim().is_empty() {
                    "没有匹配的 Minecraft 版本"
                } else {
                    "当前分类没有可下载版本"
                },
                9.0,
                theme::MUTED,
                false,
            );
        }
        let Some((_, current)) = filtered
            .iter()
            .find(|(index, _)| *index == self.official_selected)
            .cloned()
        else {
            return;
        };
        if self.loader_base_version != current.id {
            self.loader_base_version = current.id.clone();
            self.reset_download_loader();
        }
        let current_motion = if let Some(previous_index) = self.previous_download_detail {
            let outgoing = CardMotion {
                screen: layout.screen,
                direction: self.download_detail_direction,
                progress: self.download_detail_progress,
                outgoing: true,
            };
            if let Some(previous) = self.official_versions.get(previous_index).cloned() {
                self.paint_download_detail(
                    ui,
                    painter,
                    page,
                    time,
                    DownloadDetailView {
                        version: previous,
                        motion: outgoing,
                        active: false,
                    },
                );
            }
            CardMotion {
                screen: layout.screen,
                direction: self.download_detail_direction,
                progress: self.download_detail_progress,
                outgoing: false,
            }
        } else {
            motion
        };
        self.paint_download_detail(
            ui,
            painter,
            page,
            time,
            DownloadDetailView {
                version: current,
                motion: current_motion,
                active: true,
            },
        );
    }

    fn paint_resource_download(
        &mut self,
        ui: &mut egui::Ui,
        painter: &Painter,
        page: Rect,
        time: f64,
        motion: CardMotion,
    ) {
        let Some(category) = self.download_resource_category else {
            return;
        };
        let list_width = (page.width() * 0.44).clamp(390.0, 540.0);
        let source_list = Rect::from_min_max(
            page.left_top(),
            Pos2::new(page.left() + list_width, page.bottom()),
        );
        let source_detail = Rect::from_min_max(
            Pos2::new(source_list.right() + 14.0, page.top()),
            page.right_bottom(),
        );
        let page_painter = painter;
        let list_panel = motion.rect(source_list);
        let heading =
            Rect::from_min_size(list_panel.left_top(), Vec2::new(list_panel.width(), 130.0));
        theme::glass_panel(
            painter,
            heading,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(20, 32, 24), theme::GREEN, time, 32.0),
        );
        let heading_content = card_clip(painter, heading);
        let painter = &heading_content;
        let version_filter_button = Rect::from_min_size(
            Pos2::new(heading.right() - 170.0, heading.top() + 10.0),
            Vec2::new(150.0, 28.0),
        );
        self.resource_version_anchor = version_filter_button;
        let version_filter_response = ui
            .interact(
                version_filter_button,
                Id::unique("resource-version-filter"),
                Sense::click(),
            )
            .on_hover_cursor(CursorIcon::PointingHand);
        theme::fill_round(
            painter,
            version_filter_button,
            theme::RADIUS,
            if version_filter_response.hovered() || self.resource_version_menu_open {
                theme::alpha(theme::GREEN, 25)
            } else {
                theme::white_alpha(7)
            },
        );
        theme::stroke_round(
            painter,
            version_filter_button,
            theme::RADIUS,
            1.0,
            if self.resource_version_menu_open {
                theme::alpha(theme::GREEN_BRIGHT, 70)
            } else {
                theme::white_alpha(26)
            },
        );
        theme::text(
            painter,
            Pos2::new(
                version_filter_button.left() + 11.0,
                version_filter_button.top() + 8.0,
            ),
            fit_text_to_width(
                painter,
                &format!("版本 {}", self.resource_version_filter),
                theme::font(8.0),
                version_filter_button.width() - 34.0,
            ),
            8.0,
            theme::INK_SOFT,
        );
        draw_icon(
            painter,
            NavIcon::Chevron,
            Rect::from_center_size(
                Pos2::new(
                    version_filter_button.right() - 14.0,
                    version_filter_button.center().y,
                ),
                Vec2::splat(12.0),
            ),
            theme::MUTED,
        );
        if version_filter_response.clicked() {
            self.resource_version_menu_open = !self.resource_version_menu_open;
            self.resource_version_scroll = 0.0;
            ui.ctx().request_repaint();
        }
        theme::bold_text(
            painter,
            Pos2::new(heading.left() + 20.0, heading.top() + 13.0),
            fit_text_to_width(
                painter,
                &format!("下载{}", category.label()),
                theme::bold_font(18.0),
                heading.width() - 200.0,
            ),
            18.0,
            theme::INK,
        );
        theme::text(
            painter,
            Pos2::new(heading.left() + 20.0, heading.top() + 40.0),
            format!(
                "Minecraft {} · Modrinth · 自动选路{}{}",
                self.resource_version_filter,
                if matches!(category, ResourceCategory::Mod | ResourceCategory::Modpack)
                    && self.version().loader_family != "原版"
                {
                    " · "
                } else {
                    ""
                },
                if matches!(category, ResourceCategory::Mod | ResourceCategory::Modpack)
                    && self.version().loader_family != "原版"
                {
                    self.version().loader_family.as_str()
                } else {
                    ""
                }
            ),
            8.1,
            theme::MUTED,
        );
        let search = Rect::from_min_size(
            Pos2::new(heading.left() + 20.0, heading.top() + 61.0),
            Vec2::new(heading.width() - 110.0, 34.0),
        );
        theme::fill_round(
            painter,
            search,
            theme::RADIUS,
            Color32::from_rgb(25, 38, 29),
        );
        theme::stroke_round(painter, search, theme::RADIUS, 1.0, theme::white_alpha(28));
        draw_icon(
            painter,
            NavIcon::Search,
            Rect::from_center_size(
                Pos2::new(search.left() + 16.0, search.center().y),
                Vec2::splat(13.0),
            ),
            theme::MUTED,
        );
        let search_response = ui.put(
            Rect::from_min_max(
                Pos2::new(search.left() + 30.0, search.top() + 5.0),
                Pos2::new(search.right() - 8.0, search.bottom() - 5.0),
            ),
            egui::TextEdit::singleline(&mut self.download_search)
                .frame(egui::Frame::NONE)
                .font(theme::font(8.8))
                .text_color(theme::INK)
                .hint_text(format!("搜索{}", category.label())),
        );
        let search_button = Rect::from_min_size(
            Pos2::new(heading.right() - 82.0, heading.top() + 61.0),
            Vec2::new(62.0, 34.0),
        );
        let search_clicked =
            settings_button(ui, painter, search_button, "resource-search", "搜索", true)
                || (search_response.lost_focus()
                    && ui.ctx().input(|input| input.key_pressed(egui::Key::Enter)));
        if search_clicked {
            self.start_resource_search();
        }

        let painter = page_painter;
        let source_list_top = source_list.top() + 140.0;
        let list_top = list_panel.top() + 140.0;
        let list_bottom = list_panel.bottom();
        let list_height = (list_bottom - list_top).max(1.0);
        let row_height = 64.0;
        let content_height = self.resource_results.len() as f32 * row_height;
        let max_scroll = (content_height - list_height).max(0.0);
        if ui.ctx().input(|input| {
            input
                .pointer
                .hover_pos()
                .is_some_and(|pointer| list_panel.contains(pointer))
                && input.smooth_scroll_delta.y.abs() > 0.0
        }) {
            self.resource_scroll -= ui.ctx().input(|input| input.smooth_scroll_delta.y) * 1.05;
        }
        self.resource_scroll = self.resource_scroll.clamp(0.0, max_scroll);
        let list_viewport = Rect::from_min_max(
            Pos2::new(list_panel.left() + 4.0, list_top),
            Pos2::new(list_panel.right() - 10.0, list_bottom),
        );
        let list_painter = painter.with_clip_rect(list_viewport);
        let mut clicked_resource = None;
        for (index, project) in self.resource_results.iter().enumerate() {
            let source_y = source_list_top + index as f32 * row_height - self.resource_scroll;
            if source_y + row_height < source_list_top || source_y > source_list.bottom() {
                continue;
            }
            let row = motion.rect(Rect::from_min_size(
                Pos2::new(source_list.left() + 5.0, source_y),
                Vec2::new(source_list.width() - 16.0, row_height - 5.0),
            ));
            let response = ui
                .interact(row, Id::unique(("resource-project", index)), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand);
            let selected = self.resource_selected == index;
            theme::glass_panel(
                &list_painter,
                row.shrink2(Vec2::new(2.0, 2.0)),
                theme::RADIUS,
                theme::GlassSpec::new(
                    if selected || response.hovered() {
                        Color32::from_rgb(29, 48, 33)
                    } else {
                        Color32::from_rgb(20, 33, 24)
                    },
                    theme::GREEN,
                    time,
                    row.top() * 0.01,
                )
                .hovered(response.hovered()),
            );
            let row_painter = card_clip(&list_painter, row.shrink2(Vec2::new(2.0, 2.0)));
            theme::bold_text(
                &row_painter,
                Pos2::new(row.left() + 16.0, row.top() + 10.0),
                fit_text_to_width(
                    &row_painter,
                    &project.title,
                    theme::bold_font(10.0),
                    row.width() - 126.0,
                ),
                10.0,
                theme::INK_SOFT,
            );
            theme::text(
                &row_painter,
                Pos2::new(row.left() + 16.0, row.top() + 30.0),
                fit_text_to_width(
                    &list_painter,
                    &project.description,
                    theme::font(7.5),
                    row.width() - 32.0,
                ),
                7.5,
                theme::MUTED,
            );
            theme::text(
                &row_painter,
                Pos2::new(row.right() - 84.0, row.top() + 10.0),
                format!("↓ {}", compact_count(project.downloads)),
                7.4,
                theme::GREEN_BRIGHT,
            );
            if response.clicked() {
                clicked_resource = Some(index);
            }
        }
        if let Some(index) = clicked_resource {
            self.switch_resource_detail(index);
        }
        if self.resource_search_receiver.is_some() {
            theme::center_text(
                &list_painter,
                Pos2::new(list_panel.center().x, list_top + 70.0),
                "正在自动选择 Modrinth 线路并搜索…",
                9.0,
                theme::MUTED,
                false,
            );
        } else if let Some(error) = self.resource_search_error.as_ref() {
            theme::center_text(
                &list_painter,
                Pos2::new(list_panel.center().x, list_top + 70.0),
                fit_text_to_width(
                    &list_painter,
                    error,
                    theme::font(8.0),
                    (list_panel.width() - 40.0).max(120.0),
                ),
                8.0,
                Color32::from_rgb(208, 172, 102),
                false,
            );
        } else if self.resource_results.is_empty() {
            theme::center_text(
                &list_painter,
                Pos2::new(list_panel.center().x, list_top + 70.0),
                format!("没有找到符合条件的{}", category.label()),
                9.0,
                theme::MUTED,
                false,
            );
        }

        let detail_motion = if let Some(previous_index) = self.previous_resource_detail {
            let outgoing = CardMotion {
                screen: motion.screen,
                direction: self.resource_detail_direction,
                progress: self.resource_detail_progress,
                outgoing: true,
            };
            if let Some(previous) = self.resource_results.get(previous_index).cloned() {
                self.paint_resource_detail(
                    ui,
                    painter,
                    source_detail,
                    time,
                    ResourceDetailView {
                        project: Some(previous),
                        motion: outgoing,
                        active: false,
                    },
                );
            }
            CardMotion {
                screen: motion.screen,
                direction: self.resource_detail_direction,
                progress: self.resource_detail_progress,
                outgoing: false,
            }
        } else {
            motion
        };
        self.paint_resource_detail(
            ui,
            painter,
            source_detail,
            time,
            ResourceDetailView {
                project: self.resource_results.get(self.resource_selected).cloned(),
                motion: detail_motion,
                active: true,
            },
        );
        self.paint_resource_version_menu(ui.ctx(), time);
    }

    fn paint_resource_detail(
        &mut self,
        ui: &mut egui::Ui,
        painter: &Painter,
        source_detail: Rect,
        time: f64,
        view: ResourceDetailView,
    ) {
        let Some(category) = self.download_resource_category else {
            return;
        };
        let motion = view.motion;
        let detail_base = painter;
        let detail_heading = motion.rect(Rect::from_min_size(
            source_detail.left_top(),
            Vec2::new(source_detail.width(), 76.0),
        ));
        theme::glass_panel(
            painter,
            detail_heading,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(20, 32, 24), theme::GREEN, time, 32.5),
        );
        let detail_heading_content = card_clip(painter, detail_heading);
        theme::text(
            &detail_heading_content,
            Pos2::new(detail_heading.left() + 22.0, detail_heading.top() + 12.0),
            format!("{}详情", category.label()),
            8.0,
            theme::MUTED,
        );
        theme::bold_text(
            &detail_heading_content,
            Pos2::new(detail_heading.left() + 22.0, detail_heading.top() + 31.0),
            view.project
                .as_ref()
                .map(|project| {
                    fit_text_to_width(
                        &detail_heading_content,
                        &project.title,
                        theme::bold_font(19.0),
                        detail_heading.width() - 44.0,
                    )
                })
                .unwrap_or_else(|| "等待选择项目".to_owned()),
            19.0,
            theme::INK,
        );
        let detail_card = motion.rect(Rect::from_min_size(
            Pos2::new(source_detail.left(), source_detail.top() + 90.0),
            Vec2::new(source_detail.width(), 220.0),
        ));
        theme::glass_panel(
            painter,
            detail_card,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(21, 34, 25), theme::GREEN, time, 33.0),
        );
        let detail_content = card_clip(painter, detail_card);
        if let Some(project) = view.project.as_ref() {
            let info_x = detail_card.left() + 24.0;
            theme::text(
                &detail_content,
                Pos2::new(info_x, detail_card.top() + 24.0),
                format!("作者 {}", project.author),
                8.2,
                theme::GREEN_BRIGHT,
            );
            theme::wrapped_text(
                &detail_content,
                Pos2::new(info_x, detail_card.top() + 50.0),
                &project.description,
                9.0,
                theme::INK_SOFT,
                detail_card.width() - 48.0,
            );
            theme::text(
                &detail_content,
                Pos2::new(info_x, detail_card.top() + 145.0),
                fit_text_to_width(
                    &detail_content,
                    &format!(
                        "{} 次下载 · {} 个适用版本 · {}",
                        compact_count(project.downloads),
                        project.game_versions.len(),
                        project
                            .categories
                            .iter()
                            .take(4)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(" / ")
                    ),
                    theme::font(7.8),
                    detail_card.width() - 48.0,
                ),
                7.8,
                theme::MUTED,
            );
        }
        let download = motion.rect(Rect::from_min_size(
            Pos2::new(source_detail.right() - 150.0, source_detail.bottom() - 46.0),
            Vec2::new(132.0, 38.0),
        ));
        let downloading = self.resource_download_receiver.is_some();
        let label = if downloading {
            self.resource_download_name
                .as_deref()
                .map(|name| format!("正在下载 {}", ellipsize_end(name, 14)))
                .unwrap_or_else(|| "正在下载…".to_owned())
        } else {
            format!("下载{}", category.label())
        };
        let label = fit_text_to_width(
            detail_base,
            &label,
            theme::bold_font(9.0),
            download.width() - 22.0,
        );
        if view.active
            && settings_button(ui, detail_base, download, "resource-download", &label, true)
            && !downloading
            && let Some(project) = view.project
            && let Some(game_directory) = self.version_data_directory()
        {
            let loader = if matches!(category, ResourceCategory::Mod | ResourceCategory::Modpack)
                && self.version().loader_family != "原版"
            {
                Some(self.version().loader_family.to_lowercase())
            } else {
                None
            };
            self.resource_download_name = Some(project.title.clone());
            self.resource_download_receiver = Some(crate::resources::start_download(
                project,
                category,
                self.resource_version_filter.clone(),
                loader,
                game_directory,
            ));
        }
        let footer = motion.rect(Rect::from_min_size(
            Pos2::new(source_detail.left() + 4.0, source_detail.bottom() - 36.0),
            Vec2::new(source_detail.width() - 160.0, 20.0),
        ));
        theme::text(
            detail_base,
            footer.left_top(),
            format!("将保存到 {}", category.target_folder()),
            8.0,
            theme::MUTED,
        );
    }

    fn paint_resource_version_menu(&mut self, ctx: &egui::Context, time: f64) {
        if !self.resource_version_menu_open || self.resource_version_anchor == Rect::NOTHING {
            return;
        }
        let screen = ctx.content_rect();
        let menu = resource_version_menu_rect(self.resource_version_anchor, screen);
        let mut versions = self
            .official_versions
            .iter()
            .map(|version| (version.id.clone(), version.kind.clone()))
            .collect::<Vec<_>>();
        if versions.is_empty()
            || !versions
                .iter()
                .any(|(id, _)| id == &self.resource_version_filter)
        {
            versions.insert(
                0,
                (self.resource_version_filter.clone(), "release".to_owned()),
            );
        }
        versions.dedup_by(|left, right| left.0 == right.0);

        egui::Area::new(Id::unique("resource-version-menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(menu.min)
            .movable(false)
            .show(ctx, |ui| {
                ui.set_min_size(menu.size());
                let painter = ui.painter().clone();
                theme::glass_panel(
                    &painter,
                    menu,
                    theme::RADIUS,
                    theme::GlassSpec::new(Color32::from_rgb(20, 32, 24), theme::GREEN, time, 34.2)
                        .backdrop_blur(16.0),
                );
                let content = card_clip(&painter, menu);
                theme::bold_text(
                    &content,
                    Pos2::new(menu.left() + 16.0, menu.top() + 13.0),
                    "选择 Minecraft 版本",
                    9.2,
                    theme::INK,
                );
                theme::text(
                    &content,
                    Pos2::new(menu.left() + 16.0, menu.top() + 34.0),
                    "筛选适用于指定游戏版本的资源",
                    7.4,
                    theme::MUTED,
                );

                let list = Rect::from_min_max(
                    Pos2::new(menu.left() + 8.0, menu.top() + 57.0),
                    Pos2::new(menu.right() - 8.0, menu.bottom() - 8.0),
                );
                let row_height = 38.0;
                let max_scroll = (versions.len() as f32 * row_height - list.height()).max(0.0);
                if ui.ctx().input(|input| {
                    input
                        .pointer
                        .hover_pos()
                        .is_some_and(|pointer| list.contains(pointer))
                        && input.smooth_scroll_delta.y.abs() > 0.0
                }) {
                    self.resource_version_scroll -=
                        ui.ctx().input(|input| input.smooth_scroll_delta.y);
                }
                self.resource_version_scroll = self.resource_version_scroll.clamp(0.0, max_scroll);
                let list_painter = content.with_clip_rect(list);
                let mut selected_version = None;
                for (index, (id, kind)) in versions.iter().enumerate() {
                    let y = list.top() + index as f32 * row_height - self.resource_version_scroll;
                    if y + row_height < list.top() || y > list.bottom() {
                        continue;
                    }
                    let row = Rect::from_min_size(
                        Pos2::new(list.left(), y),
                        Vec2::new(list.width(), row_height - 4.0),
                    );
                    let response = ui
                        .interact(
                            row,
                            Id::unique(("resource-version-option", id)),
                            Sense::click(),
                        )
                        .on_hover_cursor(CursorIcon::PointingHand);
                    let selected = id == &self.resource_version_filter;
                    let row_content = card_clip(&list_painter, row);
                    theme::fill_round(
                        &row_content,
                        row,
                        theme::RADIUS,
                        if selected {
                            theme::alpha(theme::GREEN, 26)
                        } else if response.hovered() {
                            theme::white_alpha(11)
                        } else {
                            theme::white_alpha(4)
                        },
                    );
                    theme::bold_text(
                        &row_content,
                        Pos2::new(row.left() + 13.0, row.top() + 5.0),
                        id,
                        9.0,
                        if selected {
                            theme::INK
                        } else {
                            theme::INK_SOFT
                        },
                    );
                    theme::text(
                        &row_content,
                        Pos2::new(row.right() - 54.0, row.top() + 8.0),
                        download_version_kind(kind),
                        7.2,
                        if selected {
                            theme::GREEN_BRIGHT
                        } else {
                            theme::MUTED
                        },
                    );
                    if response.clicked() {
                        selected_version = Some(id.clone());
                    }
                }
                if let Some(version) = selected_version {
                    self.resource_version_filter = version;
                    self.resource_version_menu_open = false;
                    self.resource_version_scroll = 0.0;
                    self.start_resource_search();
                    ctx.request_repaint();
                }
            });
    }

    fn paint_download_detail(
        &mut self,
        ui: &mut egui::Ui,
        painter: &Painter,
        page: Rect,
        time: f64,
        view: DownloadDetailView,
    ) {
        let list_width = (page.width() * 0.44).clamp(390.0, 540.0);
        let source_detail = Rect::from_min_max(
            Pos2::new(page.left() + list_width + 14.0, page.top()),
            page.right_bottom(),
        );
        let painter = painter.clone();
        let painter = &painter;
        let motion = view.motion;
        let detail_heading = motion.rect(Rect::from_min_size(
            source_detail.left_top(),
            Vec2::new(source_detail.width(), 76.0),
        ));
        let detail_base = painter;
        theme::glass_panel(
            painter,
            detail_heading,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(20, 32, 24), theme::GREEN, time, 29.0),
        );
        let detail_heading_content = card_clip(painter, detail_heading);
        let painter = &detail_heading_content;
        theme::text(
            painter,
            Pos2::new(detail_heading.left() + 22.0, detail_heading.top() + 12.0),
            "下载详情",
            8.0,
            theme::MUTED,
        );
        theme::bold_text(
            painter,
            Pos2::new(detail_heading.left() + 22.0, detail_heading.top() + 30.0),
            &view.version.id,
            20.0,
            theme::INK,
        );

        let detail_card = motion.rect(Rect::from_min_size(
            Pos2::new(source_detail.left(), source_detail.top() + 90.0),
            Vec2::new(source_detail.width(), 198.0),
        ));
        let painter = detail_base;
        theme::glass_panel(
            painter,
            detail_card,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(21, 34, 25), theme::GREEN, time, 29.8),
        );
        let detail_card_content = card_clip(painter, detail_card);
        let painter = &detail_card_content;
        let icon_center = Pos2::new(detail_card.left() + 68.0, detail_card.center().y);
        theme::soft_orb(
            painter,
            icon_center + Vec2::new(0.0, 12.0),
            64.0,
            theme::GREEN,
            13,
        );
        let icon = Rect::from_center_size(icon_center, Vec2::splat(72.0));
        let icon_texture = if view.version.kind == "snapshot" {
            self.snapshot_icon_texture.as_ref()
        } else {
            self.release_icon_texture.as_ref()
        };
        if let Some(texture) = icon_texture {
            paint_rounded_image(painter, icon, 12.0, texture, texture.size());
        } else {
            paint_block(
                painter,
                icon.center(),
                72.0,
                Color32::from_rgb(125, 216, 106),
                Color32::from_rgb(87, 65, 46),
                Color32::from_rgb(69, 148, 70),
            );
        }
        let info_x = detail_card.left() + 128.0;
        theme::text(
            painter,
            Pos2::new(info_x, detail_card.top() + 24.0),
            "版本类型",
            8.0,
            theme::MUTED,
        );
        theme::bold_text(
            painter,
            Pos2::new(info_x, detail_card.top() + 40.0),
            download_version_kind(&view.version.kind),
            12.0,
            theme::INK_SOFT,
        );
        theme::text(
            painter,
            Pos2::new(info_x, detail_card.top() + 76.0),
            "下载来源",
            8.0,
            theme::MUTED,
        );
        theme::bold_text(
            painter,
            Pos2::new(info_x, detail_card.top() + 92.0),
            "Mojang Piston Meta",
            11.0,
            theme::INK_SOFT,
        );
        theme::text(
            painter,
            Pos2::new(info_x, detail_card.top() + 128.0),
            "安装流程",
            8.0,
            theme::MUTED,
        );
        theme::text(
            painter,
            Pos2::new(info_x, detail_card.top() + 145.0),
            "下载原版文件后，可选择 Fabric、Forge 或 NeoForge",
            8.5,
            Color32::from_rgb(176, 194, 178),
        );

        if !view.active {
            return;
        }

        let loader_progress = self.download_loader_value.clamp(0.0, 1.08);
        let loader_eased = loader_progress.clamp(0.0, 1.0);
        let loader_eased = loader_eased * loader_eased * (3.0 - 2.0 * loader_eased);
        let loader_height = 94.0 + (342.0 - 94.0) * loader_eased;
        let loader_card = motion.rect(Rect::from_min_size(
            Pos2::new(source_detail.left(), source_detail.top() + 298.0),
            Vec2::new(source_detail.width(), loader_height),
        ));
        let painter = detail_base;
        theme::glass_panel(
            painter,
            loader_card,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(21, 34, 25), theme::GREEN, time, 30.1),
        );
        let loader_content = card_clip(painter, loader_card);
        let painter = &loader_content;
        let loader_header =
            Rect::from_min_size(loader_card.left_top(), Vec2::new(loader_card.width(), 58.0));
        let loader_response = ui
            .interact(
                loader_header,
                Id::unique("download-loader-toggle"),
                Sense::click(),
            )
            .on_hover_cursor(CursorIcon::PointingHand);
        let loader_icon = Rect::from_center_size(
            Pos2::new(loader_card.left() + 22.0, loader_card.top() + 29.0),
            Vec2::splat(28.0),
        );
        if let Some(texture) = self.loader_icon_texture.as_ref() {
            paint_rounded_image(painter, loader_icon, 8.0, texture, texture.size());
        }
        theme::text(
            painter,
            Pos2::new(loader_card.left() + 42.0, loader_card.top() + 11.0),
            "模组加载器",
            8.0,
            theme::MUTED,
        );
        theme::bold_text(
            painter,
            Pos2::new(loader_card.left() + 42.0, loader_card.top() + 26.0),
            self.loader_kind
                .map(|kind| format!("{} · Minecraft {}", kind.label(), self.loader_base_version))
                .unwrap_or_else(|| "原版 / 不使用加载器".to_owned()),
            12.0,
            theme::INK_SOFT,
        );
        let arrow_center = Pos2::new(loader_card.right() - 28.0, loader_card.top() + 29.0);
        let arrow_y = 4.0 - 8.0 * loader_eased;
        painter.line_segment(
            [arrow_center + Vec2::new(-5.0, arrow_y), arrow_center],
            Stroke::new(1.6, theme::INK_SOFT),
        );
        painter.line_segment(
            [arrow_center, arrow_center + Vec2::new(5.0, arrow_y)],
            Stroke::new(1.6, theme::INK_SOFT),
        );
        if loader_response.clicked() {
            self.download_loader_open = !self.download_loader_open;
        }

        if loader_progress > 0.02 {
            let painter = painter.clone();
            let painter = &painter;
            let options = [
                (None, "原版"),
                (Some(LoaderKind::Fabric), "Fabric"),
                (Some(LoaderKind::Forge), "Forge"),
                (Some(LoaderKind::NeoForge), "NeoForge"),
            ];
            let mut option_x = loader_card.left() + 20.0;
            for (index, (kind, label)) in options.into_iter().enumerate() {
                let width = if index == 0 { 64.0 } else { 78.0 };
                let option = Rect::from_min_size(
                    Pos2::new(option_x, loader_card.top() + 62.0),
                    Vec2::new(width, 30.0),
                );
                let response = ui
                    .interact(
                        option,
                        Id::unique(("download-loader-kind", index)),
                        Sense::click(),
                    )
                    .on_hover_cursor(CursorIcon::PointingHand);
                let selected = self.loader_kind == kind;
                theme::fill_round(
                    painter,
                    option,
                    theme::RADIUS,
                    if selected {
                        theme::alpha(theme::GREEN, 28)
                    } else if response.hovered() {
                        theme::white_alpha(13)
                    } else {
                        theme::white_alpha(7)
                    },
                );
                theme::center_text(
                    painter,
                    option.center(),
                    label,
                    8.2,
                    if selected { theme::INK } else { theme::MUTED },
                    selected,
                );
                if response.clicked() && !selected {
                    if let Some(kind) = kind {
                        self.choose_loader(kind);
                        self.download_loader_open = true;
                    } else {
                        self.loader_kind = None;
                        self.loader_versions_receiver = None;
                        self.loader_versions.clear();
                        self.loader_versions_error = None;
                        self.loader_version_selected = 0;
                        self.loader_version_scroll = 0.0;
                    }
                }
                option_x = option.right() + 8.0;
            }

            if let Some(kind) = self.loader_kind {
                let list = Rect::from_min_max(
                    Pos2::new(loader_card.left() + 20.0, loader_card.top() + 102.0),
                    Pos2::new(loader_card.right() - 20.0, loader_card.bottom() - 12.0),
                );
                let row_height = 34.0;
                let content_height = self.loader_versions.len() as f32 * row_height;
                let max_scroll = (content_height - list.height()).max(0.0);
                if ui.ctx().input(|input| {
                    input
                        .pointer
                        .hover_pos()
                        .is_some_and(|pointer| list.contains(pointer))
                        && input.smooth_scroll_delta.y.abs() > 0.0
                }) {
                    self.loader_version_scroll -=
                        ui.ctx().input(|input| input.smooth_scroll_delta.y) * 1.0;
                }
                self.loader_version_scroll = self.loader_version_scroll.clamp(0.0, max_scroll);
                let list_painter = painter.clone();
                if self.loader_versions_receiver.is_some() {
                    theme::center_text(
                        &list_painter,
                        list.center(),
                        format!("正在获取 {} 版本…", kind.label()),
                        8.5,
                        theme::MUTED,
                        false,
                    );
                } else if let Some(error) = &self.loader_versions_error {
                    theme::center_text(
                        &list_painter,
                        list.center(),
                        ellipsize_end(error, 64),
                        8.0,
                        Color32::from_rgb(207, 170, 103),
                        false,
                    );
                } else if self.loader_versions.is_empty() {
                    theme::center_text(
                        &list_painter,
                        list.center(),
                        format!(
                            "{} 没有适用于 Minecraft {} 的版本",
                            kind.label(),
                            self.loader_base_version
                        ),
                        8.2,
                        Color32::from_rgb(207, 170, 103),
                        false,
                    );
                } else {
                    for (index, version) in self.loader_versions.iter().enumerate() {
                        let y = list.top() + index as f32 * row_height - self.loader_version_scroll;
                        if y + row_height < list.top() || y > list.bottom() {
                            continue;
                        }
                        let row = Rect::from_min_size(
                            Pos2::new(list.left(), y),
                            Vec2::new(list.width(), row_height - 4.0),
                        );
                        let response = ui
                            .interact(
                                row,
                                Id::unique(("download-loader-version", index)),
                                Sense::click(),
                            )
                            .on_hover_cursor(CursorIcon::PointingHand);
                        let selected = self.loader_version_selected == index;
                        theme::fill_round(
                            &list_painter,
                            row,
                            theme::RADIUS,
                            if selected {
                                theme::alpha(theme::GREEN, 24)
                            } else if response.hovered() {
                                theme::white_alpha(10)
                            } else {
                                Color32::from_rgb(18, 29, 21)
                            },
                        );
                        theme::text(
                            &list_painter,
                            Pos2::new(row.left() + 14.0, row.top() + 7.0),
                            loader_display_version(&self.loader_base_version, version),
                            8.5,
                            if selected {
                                theme::INK
                            } else {
                                theme::INK_SOFT
                            },
                        );
                        if selected {
                            theme::text(
                                &list_painter,
                                Pos2::new(row.right() - 18.0, row.top() + 7.0),
                                "✓",
                                9.0,
                                theme::GREEN_BRIGHT,
                            );
                        }
                        if response.clicked() {
                            self.loader_version_selected = index;
                        }
                    }
                }
            } else {
                theme::text(
                    painter,
                    Pos2::new(loader_card.left() + 20.0, loader_card.top() + 104.0),
                    "保持原版安装，不添加模组加载器。",
                    8.5,
                    theme::MUTED,
                );
            }
        }

        let painter = detail_base;
        let selected_loader_version = self.loader_kind.and_then(|loader| {
            self.loader_versions
                .get(self.loader_version_selected)
                .cloned()
                .map(|version| (loader, version))
        });
        let can_install = self.install_receiver.is_none()
            && (self.loader_kind.is_none() || selected_loader_version.is_some());
        let install_label = if self.install_receiver.is_some() {
            "正在安装…"
        } else if self.loader_kind.is_some() {
            "下载并安装加载器"
        } else {
            "下载原版"
        };
        let install = motion.rect(Rect::from_min_size(
            Pos2::new(source_detail.right() - 150.0, source_detail.bottom() - 46.0),
            Vec2::new(132.0, 38.0),
        ));
        if settings_button(
            ui,
            painter,
            install,
            "download-install",
            install_label,
            true,
        ) && can_install
        {
            self.pending_loader_after_base = selected_loader_version;
            self.start_official_install(view.version, false);
        }
        theme::text(
            painter,
            Pos2::new(source_detail.left() + 4.0, source_detail.bottom() - 36.0),
            if self.loader_kind.is_some() && self.loader_versions.is_empty() {
                "请先选择一个加载器版本"
            } else {
                "只使用 Mojang 官方下载域名，并在安装前校验文件"
            },
            8.0,
            theme::MUTED,
        );
    }

    fn paint_favorites(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
        motion: CardMotion,
    ) {
        let page = Rect::from_min_max(
            Pos2::new(layout.content_left, layout.header.bottom() + 14.0),
            Pos2::new(layout.content_right, layout.screen.bottom() - 22.0),
        );
        let base_painter = painter;
        let header = motion.rect(Rect::from_min_size(
            page.left_top(),
            Vec2::new(page.width(), 92.0),
        ));
        theme::glass_panel(
            painter,
            header,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(20, 32, 24), theme::GREEN, time, 24.5),
        );
        let header_content = card_clip(painter, header);
        theme::bold_text(
            &header_content,
            Pos2::new(header.left() + 24.0, header.top() + 16.0),
            "收藏",
            21.0,
            theme::INK,
        );
        theme::text(
            &header_content,
            Pos2::new(header.left() + 24.0, header.top() + 47.0),
            "收藏 Minecraft 社区网站，或创建自己的快捷入口",
            8.5,
            theme::MUTED,
        );
        let add = Rect::from_min_size(
            Pos2::new(header.right() - 142.0, header.top() + 27.0),
            Vec2::new(116.0, 38.0),
        );
        if settings_button(ui, &header_content, add, "favorite-add", "添加收藏", true) {
            self.open_favorite_editor(add);
        }

        self.ensure_favorite_icons(ctx);
        let sites = self.all_favorite_sites();
        let builtin_count = crate::favorites::BUILTIN_SITES.len();
        let page_base = motion.rect(Rect::from_min_max(
            Pos2::new(page.left(), page.top() + 104.0),
            page.right_bottom(),
        ));
        let columns = 3;
        let gap = 12.0;
        let card_height = 112.0;
        let card_width = (page_base.width() - gap * (columns - 1) as f32) / columns as f32;
        let row_height = card_height + gap;
        let grid_top = page_base.top() + 16.0;
        let grid_height = (page_base.bottom() - grid_top).max(1.0);
        let content_height = sites.len().div_ceil(columns) as f32 * row_height;
        let max_scroll = (content_height - grid_height).max(0.0);
        let next_row = sites.len() / columns;
        let next_content_height = (sites.len() + 1).div_ceil(columns) as f32 * row_height;
        let next_scroll = (next_content_height - grid_height).max(0.0);
        self.favorite_editor_target_scroll = next_scroll;
        self.favorite_editor_card_target = motion.rect(Rect::from_min_size(
            Pos2::new(
                page_base.left() + (sites.len() % columns) as f32 * (card_width + gap),
                grid_top + next_row as f32 * row_height - next_scroll,
            ),
            Vec2::new(card_width, card_height),
        ));
        if self.favorite_editor_open {
            self.community_scroll = self.community_scroll.clamp(0.0, max_scroll);
        } else if ui.ctx().input(|input| {
            input
                .pointer
                .hover_pos()
                .is_some_and(|pointer| page_base.contains(pointer))
        }) {
            self.community_scroll -= ui.ctx().input(|input| input.smooth_scroll_delta.y) * 1.25;
            self.community_scroll = self.community_scroll.clamp(0.0, max_scroll);
        }
        let viewport = page_base.shrink2(Vec2::new(0.0, 2.0));
        let list_painter = base_painter.with_clip_rect(viewport);
        for (index, site) in sites.iter().enumerate() {
            let column = index % columns;
            let row = index / columns;
            let card = motion.rect(Rect::from_min_size(
                Pos2::new(
                    page_base.left() + column as f32 * (card_width + gap),
                    grid_top + row as f32 * row_height - self.community_scroll,
                ),
                Vec2::new(card_width, card_height),
            ));
            if card.bottom() < viewport.top() || card.top() > viewport.bottom() {
                continue;
            }
            let response = ui
                .interact(
                    card,
                    Id::unique(("favorite-site", &site.url)),
                    Sense::click(),
                )
                .on_hover_cursor(CursorIcon::PointingHand);
            let card_painter = card_clip(&list_painter, card);
            theme::glass_panel(
                &card_painter,
                card,
                theme::RADIUS,
                theme::GlassSpec::new(
                    if response.hovered() {
                        Color32::from_rgb(29, 48, 34)
                    } else {
                        Color32::from_rgb(20, 33, 24)
                    },
                    theme::GREEN,
                    time,
                    25.0 + index as f32 * 0.2,
                )
                .hovered(response.hovered()),
            );
            let icon = Rect::from_center_size(
                Pos2::new(card.left() + 42.0, card.top() + 44.0),
                Vec2::splat(46.0),
            );
            if let Some(texture) = self.favorite_icon_textures.get(&site.url) {
                paint_rounded_image(
                    &card_painter,
                    icon,
                    theme::RADIUS as f32,
                    texture,
                    texture.size(),
                );
            } else {
                theme::fill_round(
                    &card_painter,
                    icon,
                    theme::RADIUS,
                    theme::alpha(theme::GREEN, 24),
                );
                theme::center_text(
                    &card_painter,
                    icon.center(),
                    site.title
                        .chars()
                        .next()
                        .unwrap_or('W')
                        .to_uppercase()
                        .to_string(),
                    17.0,
                    theme::GREEN_BRIGHT,
                    true,
                );
            }
            theme::bold_text(
                &card_painter,
                Pos2::new(card.left() + 75.0, card.top() + 24.0),
                fit_text_to_width(
                    &card_painter,
                    &site.title,
                    theme::bold_font(12.0),
                    card.width() - 100.0,
                ),
                12.0,
                theme::INK,
            );
            theme::text(
                &card_painter,
                Pos2::new(card.left() + 75.0, card.top() + 48.0),
                crate::favorites::site_domain(&site.url),
                8.0,
                theme::MUTED,
            );
            theme::text(
                &card_painter,
                Pos2::new(card.left() + 18.0, card.bottom() - 28.0),
                "打开网站",
                8.2,
                if response.hovered() {
                    theme::GREEN_BRIGHT
                } else {
                    Color32::from_rgb(157, 204, 151)
                },
            );
            theme::text(
                &card_painter,
                Pos2::new(card.right() - 20.0, card.bottom() - 30.0),
                "→",
                9.0,
                theme::MUTED,
            );
            if index >= builtin_count {
                let remove = Rect::from_center_size(
                    Pos2::new(card.right() - 18.0, card.top() + 18.0),
                    Vec2::splat(24.0),
                );
                let remove_response = ui
                    .interact(
                        remove,
                        Id::unique(("favorite-remove", &site.url)),
                        Sense::click(),
                    )
                    .on_hover_cursor(CursorIcon::PointingHand);
                theme::fill_round(
                    &card_painter,
                    remove,
                    theme::RADIUS,
                    if remove_response.hovered() {
                        theme::white_alpha(18)
                    } else {
                        theme::black_alpha(48)
                    },
                );
                draw_icon(
                    &card_painter,
                    NavIcon::Close,
                    remove.shrink(6.0),
                    theme::MUTED,
                );
                if remove_response.clicked() {
                    self.settings
                        .favorite_sites
                        .retain(|favorite| favorite.url != site.url);
                    self.favorite_icon_textures.remove(&site.url);
                    self.favorite_icon_attempted
                        .retain(|known| known != &site.url);
                    self.community_scroll = self.community_scroll.clamp(
                        0.0,
                        (sites.len().saturating_sub(1).div_ceil(columns) as f32 * row_height
                            - grid_height)
                            .max(0.0),
                    );
                    ui.ctx().request_repaint();
                    continue;
                }
            }
            if response.clicked()
                && let Err(error) = crate::favorites::open_site(&site.url)
            {
                self.show_toast(format!("无法打开网站：{error}"));
            }
        }
        self.paint_favorite_editor(ctx, layout.screen, time);
    }

    fn paint_favorite_editor(&mut self, ctx: &egui::Context, screen: Rect, time: f64) {
        if !self.favorite_editor_open && self.favorite_editor_value <= 0.002 {
            return;
        }
        let dialog = favorite_editor_rect(screen);
        let source = if self.favorite_editor_source == Rect::NOTHING {
            dialog
        } else {
            self.favorite_editor_source
        };
        let target = if self.favorite_editor_target == Rect::NOTHING {
            if self.favorite_editor_open {
                dialog
            } else {
                source
            }
        } else {
            self.favorite_editor_target
        };
        let progress = self.favorite_editor_value.clamp(-0.05, 1.08);
        let raw_travel = if self.favorite_editor_open {
            progress.clamp(0.0, 1.0)
        } else {
            1.0 - progress.clamp(0.0, 1.0)
        };
        let travel = raw_travel * raw_travel * (3.0 - 2.0 * raw_travel);
        let from = if self.favorite_editor_open {
            source
        } else {
            dialog
        };
        let to = if self.favorite_editor_open {
            dialog
        } else {
            target
        };
        let morph_rect = Rect::from_min_max(
            from.min + (to.min - from.min) * travel,
            from.max + (to.max - from.max) * travel,
        );
        let content_opacity = card_text_opacity(progress);
        let start_scale = ((from.width() / dialog.width()) * (from.height() / dialog.height()))
            .sqrt()
            .clamp(0.08, 0.55);
        let scale = start_scale + (1.0 - start_scale) * travel;
        let transform =
            egui::emath::TSTransform::new(morph_rect.center() - dialog.center() * scale, scale);

        egui::Area::new(Id::unique("favorite-editor-backdrop"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .movable(false)
            .show(ctx, |ui| {
                ui.set_min_size(screen.size());
                let painter = ui.painter();
                painter.rect_filled(
                    screen,
                    0,
                    Color32::from_black_alpha((22.0 * progress.clamp(0.0, 1.0)) as u8),
                );
                if self.favorite_editor_open || progress >= 0.16 {
                    theme::glass_panel(
                        painter,
                        morph_rect,
                        theme::RADIUS,
                        theme::GlassSpec::new(
                            Color32::from_rgb(21, 34, 25),
                            theme::GREEN,
                            time,
                            26.0,
                        )
                        .backdrop_blur(18.0),
                    );
                } else {
                    let fade = (progress / 0.16).clamp(0.0, 1.0);
                    theme::fill_round(
                        painter,
                        morph_rect,
                        theme::RADIUS,
                        Color32::from_black_alpha((28.0 * fade) as u8),
                    );
                }
            });

        let mut editor_layer =
            egui::LayerId::new(egui::Order::Foreground, Id::unique("favorite-editor-layer"));
        egui::Area::new(Id::unique("favorite-editor"))
            .order(egui::Order::Foreground)
            .fixed_pos(dialog.min)
            .movable(false)
            .show(ctx, |ui| {
                ui.set_min_size(dialog.size());
                editor_layer = ui.layer_id();
                let mut painter = ui.painter().clone();
                painter.set_opacity(content_opacity);
                let content = card_clip(&painter, dialog);
                theme::bold_text(
                    &content,
                    Pos2::new(dialog.left() + 22.0, dialog.top() + 18.0),
                    "添加收藏网站",
                    18.0,
                    theme::INK,
                );
                theme::text(
                    &content,
                    Pos2::new(dialog.left() + 22.0, dialog.top() + 46.0),
                    "自动读取网站图标，也可指定图片 URL 或本地图片",
                    8.0,
                    theme::MUTED,
                );
                let title = Rect::from_min_size(
                    Pos2::new(dialog.left() + 22.0, dialog.top() + 78.0),
                    Vec2::new(dialog.width() - 44.0, 34.0),
                );
                let url = Rect::from_min_size(
                    Pos2::new(dialog.left() + 22.0, dialog.top() + 124.0),
                    Vec2::new(dialog.width() - 44.0, 34.0),
                );
                for field in [title, url] {
                    theme::fill_round(
                        &content,
                        field,
                        theme::RADIUS,
                        Color32::from_rgb(24, 38, 28),
                    );
                    theme::stroke_round(
                        &content,
                        field,
                        theme::RADIUS,
                        1.0,
                        theme::white_alpha(28),
                    );
                }
                let icon = Rect::from_min_size(
                    Pos2::new(dialog.left() + 22.0, dialog.top() + 170.0),
                    Vec2::new(dialog.width() - 220.0, 34.0),
                );
                let choose_icon = Rect::from_min_size(
                    Pos2::new(icon.right() + 8.0, dialog.top() + 170.0),
                    Vec2::new(82.0, 34.0),
                );
                let auto_icon = Rect::from_min_size(
                    Pos2::new(choose_icon.right() + 8.0, dialog.top() + 170.0),
                    Vec2::new(78.0, 34.0),
                );
                theme::fill_round(&content, icon, theme::RADIUS, Color32::from_rgb(24, 38, 28));
                theme::stroke_round(&content, icon, theme::RADIUS, 1.0, theme::white_alpha(28));
                ui.put(
                    title.shrink2(Vec2::new(10.0, 5.0)),
                    egui::TextEdit::singleline(&mut self.favorite_title_input)
                        .frame(egui::Frame::NONE)
                        .font(theme::font(8.8))
                        .text_color(theme::INK)
                        .hint_text("网站名称（可留空）"),
                );
                ui.put(
                    url.shrink2(Vec2::new(10.0, 5.0)),
                    egui::TextEdit::singleline(&mut self.favorite_url_input)
                        .frame(egui::Frame::NONE)
                        .font(theme::font(8.8))
                        .text_color(theme::INK)
                        .hint_text("https://example.com"),
                );
                ui.put(
                    icon.shrink2(Vec2::new(10.0, 5.0)),
                    egui::TextEdit::singleline(&mut self.favorite_icon_input)
                        .frame(egui::Frame::NONE)
                        .font(theme::font(8.0))
                        .text_color(theme::INK)
                        .hint_text("图标 URL（留空自动）"),
                );
                if self.favorite_editor_open
                    && settings_button(
                        ui,
                        &content,
                        choose_icon,
                        "favorite-editor-icon-file",
                        "选择图片",
                        false,
                    )
                    && let Some(path) = pick_image_file(false)
                {
                    self.favorite_icon_input = path.display().to_string();
                    self.favorite_editor_error = None;
                }
                if self.favorite_editor_open
                    && settings_button(
                        ui,
                        &content,
                        auto_icon,
                        "favorite-editor-icon-auto",
                        "自动图标",
                        false,
                    )
                {
                    self.favorite_icon_input.clear();
                    self.favorite_editor_error = None;
                }
                theme::text(
                    &content,
                    Pos2::new(dialog.left() + 22.0, dialog.top() + 211.0),
                    "图标留空时优先读取对应网站 favicon，支持 PNG、JPG、WebP 和 ICO",
                    7.3,
                    theme::MUTED,
                );
                if let Some(error) = self.favorite_editor_error.as_ref() {
                    theme::text(
                        &content,
                        Pos2::new(dialog.left() + 22.0, dialog.top() + 231.0),
                        fit_text_to_width(&content, error, theme::font(7.8), dialog.width() - 44.0),
                        7.8,
                        Color32::from_rgb(208, 172, 102),
                    );
                }
                let cancel = Rect::from_min_size(
                    Pos2::new(dialog.right() - 178.0, dialog.bottom() - 54.0),
                    Vec2::new(70.0, 36.0),
                );
                let add = Rect::from_min_size(
                    Pos2::new(dialog.right() - 98.0, dialog.bottom() - 54.0),
                    Vec2::new(76.0, 36.0),
                );
                if self.favorite_editor_open
                    && settings_button(
                        ui,
                        &content,
                        cancel,
                        "favorite-editor-cancel",
                        "取消",
                        false,
                    )
                {
                    self.cancel_favorite_editor();
                }
                if self.favorite_editor_open
                    && settings_button(ui, &content, add, "favorite-editor-add", "添加", true)
                {
                    self.add_favorite_site(self.favorite_editor_card_target);
                    ctx.request_repaint();
                }
            });
        ctx.transform_layer_shapes(editor_layer, transform);
    }

    #[allow(dead_code)]
    fn paint_community(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
        motion: CardMotion,
    ) {
        let page = Rect::from_min_max(
            Pos2::new(layout.content_left, layout.header.bottom() + 14.0),
            Pos2::new(layout.content_right, layout.screen.bottom() - 22.0),
        );
        let data_directory = self.version_data_directory();
        let side_width = (page.width() * 0.28).clamp(260.0, 330.0);
        let source_main = Rect::from_min_max(
            page.left_top(),
            Pos2::new(page.right() - side_width - 14.0, page.bottom()),
        );
        let source_side = Rect::from_min_max(
            Pos2::new(source_main.right() + 14.0, page.top()),
            page.right_bottom(),
        );
        let main = motion.rect(source_main);
        let side = motion.rect(source_side);
        let community_base = painter;
        theme::glass_panel(
            painter,
            main,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(18, 28, 22), theme::GREEN, time, 24.0),
        );
        theme::glass_panel(
            painter,
            side,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(19, 30, 23), theme::GREEN, time, 25.0),
        );
        let main_content = card_clip(painter, main);
        let painter = &main_content;
        theme::bold_text(
            painter,
            Pos2::new(main.left() + 24.0, main.top() + 20.0),
            "收藏与模组",
            21.0,
            theme::INK,
        );
        theme::text(
            painter,
            Pos2::new(main.left() + 24.0, main.top() + 48.0),
            "管理收藏内容与当前版本的模组",
            8.5,
            theme::MUTED,
        );
        for (index, label) in ["收藏模组", "收藏动态"].into_iter().enumerate() {
            let tab = Rect::from_min_size(
                Pos2::new(main.left() + 24.0 + index as f32 * 105.0, main.top() + 73.0),
                Vec2::new(98.0, 31.0),
            );
            let response = ui
                .interact(tab, Id::unique(("community-tab", index)), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand);
            let selected = self.community_tab == index;
            theme::fill_round(
                painter,
                tab,
                theme::RADIUS,
                if selected {
                    theme::alpha(theme::GREEN, 26)
                } else if response.hovered() {
                    theme::white_alpha(10)
                } else {
                    theme::black_alpha(32)
                },
            );
            theme::center_text(
                painter,
                tab.center(),
                label,
                8.5,
                if selected { theme::INK } else { theme::MUTED },
                selected,
            );
            if response.clicked() {
                self.community_tab = index;
                self.community_scroll = 0.0;
            }
        }

        let source_list = Rect::from_min_max(
            Pos2::new(source_main.left() + 18.0, source_main.top() + 118.0),
            Pos2::new(source_main.right() - 18.0, source_main.bottom() - 18.0),
        );
        let list = Rect::from_min_max(
            Pos2::new(main.left() + 18.0, main.top() + 118.0),
            Pos2::new(main.right() - 18.0, main.bottom() - 18.0),
        );
        let rows = if self.community_tab == 0 {
            data_directory
                .as_ref()
                .map(|directory| installed_mod_files(directory))
                .unwrap_or_default()
                .into_iter()
                .map(|(name, size)| (name, format_size(size), None))
                .collect::<Vec<_>>()
        } else {
            self.news_items
                .iter()
                .take(8)
                .enumerate()
                .map(|(index, news)| {
                    (
                        news.title.clone(),
                        format!("{} · {}", news.category, news.date),
                        Some(index),
                    )
                })
                .collect::<Vec<_>>()
        };
        let row_height = 48.0;
        let content_height = rows.len() as f32 * row_height;
        let max_scroll = (content_height - list.height()).max(0.0);
        if ui.ctx().input(|input| {
            input
                .pointer
                .hover_pos()
                .is_some_and(|pointer| list.contains(pointer))
        }) {
            self.community_scroll -= ui.ctx().input(|input| input.smooth_scroll_delta.y) * 1.25;
        }
        self.community_scroll = self.community_scroll.clamp(0.0, max_scroll);
        let list_painter = painter.clone();
        if rows.is_empty() {
            theme::center_text(
                &list_painter,
                Pos2::new(list.center().x, list.top() + 80.0),
                if self.community_tab == 0 {
                    "收藏中还没有模组"
                } else {
                    "正在获取收藏动态"
                },
                10.0,
                theme::MUTED,
                false,
            );
        }
        for (index, (title, detail, news_index)) in rows.iter().enumerate() {
            let source_y = source_list.top() + index as f32 * row_height - self.community_scroll;
            if source_y + 42.0 < source_list.top() || source_y > source_list.bottom() {
                continue;
            }
            let row = motion.rect(Rect::from_min_size(
                Pos2::new(source_list.left(), source_y),
                Vec2::new(source_list.width(), 42.0),
            ));
            let response = ui
                .interact(
                    row,
                    Id::unique(("community-row", self.community_tab, index)),
                    Sense::click(),
                )
                .on_hover_cursor(CursorIcon::PointingHand);
            let row_content = card_clip(&list_painter, row);
            if response.hovered() {
                theme::fill_round(&row_content, row, theme::RADIUS, theme::white_alpha(9));
            }
            let icon = Rect::from_center_size(
                Pos2::new(row.left() + 22.0, row.center().y),
                Vec2::splat(28.0),
            );
            theme::fill_round(
                &row_content,
                icon,
                theme::RADIUS,
                theme::alpha(theme::GREEN, 22),
            );
            if self.community_tab == 1 {
                draw_icon(
                    &row_content,
                    NavIcon::Sparkles,
                    icon.shrink(7.0),
                    theme::GREEN_BRIGHT,
                );
            } else {
                paint_block(
                    &row_content,
                    icon.center(),
                    18.0,
                    Color32::from_rgb(125, 205, 101),
                    Color32::from_rgb(90, 66, 47),
                    Color32::from_rgb(73, 145, 69),
                );
            }
            theme::bold_text(
                &row_content,
                Pos2::new(row.left() + 42.0, row.top() + 7.0),
                ellipsize_end(title, 42),
                9.4,
                theme::INK_SOFT,
            );
            theme::text(
                &row_content,
                Pos2::new(row.left() + 42.0, row.top() + 25.0),
                ellipsize_end(detail, 50),
                7.4,
                theme::MUTED,
            );
            if response.clicked()
                && let Some(news_index) = news_index
            {
                self.news = Some(*news_index);
            }
        }

        let painter = community_base;
        let side_content = card_clip(painter, side);
        let painter = &side_content;
        theme::bold_text(
            painter,
            Pos2::new(side.left() + 20.0, side.top() + 20.0),
            "模组环境",
            16.0,
            theme::INK,
        );
        let mod_count = self
            .version_data_directory()
            .as_ref()
            .map(|directory| installed_mod_files(directory).len())
            .unwrap_or(0);
        painter.circle_filled(
            Pos2::new(side.left() + 26.0, side.top() + 72.0),
            19.0,
            theme::alpha(theme::GREEN, 28),
        );
        theme::center_text(
            painter,
            Pos2::new(side.left() + 26.0, side.top() + 72.0),
            mod_count.to_string(),
            13.0,
            theme::GREEN_BRIGHT,
            true,
        );
        theme::text(
            painter,
            Pos2::new(side.left() + 52.0, side.top() + 56.0),
            "已安装模组",
            8.2,
            theme::MUTED,
        );
        theme::bold_text(
            painter,
            Pos2::new(side.left() + 52.0, side.top() + 71.0),
            format!("{} · {}", self.version().version, self.version().loader),
            10.5,
            theme::INK_SOFT,
        );
        painter.line_segment(
            [
                Pos2::new(side.left() + 20.0, side.top() + 108.0),
                Pos2::new(side.right() - 20.0, side.top() + 108.0),
            ],
            Stroke::new(1.0, theme::LINE),
        );
        let mods_dir = self
            .version_data_directory()
            .as_ref()
            .map(|base| base.join("mods"));
        let open_mods = Rect::from_min_size(
            Pos2::new(side.left() + 20.0, side.top() + 126.0),
            Vec2::new(side.width() - 40.0, 36.0),
        );
        if settings_button(
            ui,
            painter,
            open_mods,
            "community-open-mods",
            "打开模组目录",
            false,
        ) && let Some(path) = mods_dir
        {
            let _ = std::fs::create_dir_all(&path);
            open_path(&path);
        }
        let refresh = Rect::from_min_size(
            Pos2::new(side.left() + 20.0, side.top() + 170.0),
            Vec2::new(side.width() - 40.0, 36.0),
        );
        if settings_button(
            ui,
            painter,
            refresh,
            "community-refresh",
            "刷新模组列表",
            false,
        ) {
            self.show_toast("模组列表已刷新");
        }
        theme::wrapped_text(
            painter,
            Pos2::new(side.left() + 20.0, side.top() + 225.0),
            "收藏内容与模组管理已在同一页面整合。安装模组后，刷新即可在上方列表中查看。",
            8.2,
            theme::MUTED,
            side.width() - 40.0,
        );
        let _ = ctx;
    }

    fn ensure_skin_texture(&mut self, ctx: &egui::Context) {
        if !self.skin_dirty && self.skin_texture.is_some() {
            return;
        }
        let image = egui::ColorImage::from_rgba_unmultiplied([64, 64], &self.skin_pixels);
        self.skin_texture =
            Some(ctx.load_texture("skin-editor-texture", image, egui::TextureOptions::NEAREST));
        self.skin_dirty = false;
    }

    fn begin_skin_stroke(&mut self) {
        if self.skin_last_pixel.is_none() && self.skin_stroke_before.is_none() {
            self.skin_stroke_before = Some(self.skin_pixels.clone());
        }
    }

    fn finish_skin_stroke(&mut self) {
        if let Some(before) = self.skin_stroke_before.take() {
            self.skin_history.record(before, &self.skin_pixels);
        }
    }

    fn undo_skin_edit(&mut self) {
        self.skin_last_pixel = None;
        self.skin_preview_last_face = None;
        self.skin_stroke_before = None;
        if let Some(previous) = self.skin_history.undo(&self.skin_pixels) {
            self.skin_pixels = previous;
            self.skin_dirty = true;
            self.skin_status = Some("已撤销上一步皮肤编辑".to_owned());
        }
    }

    fn redo_skin_edit(&mut self) {
        self.skin_last_pixel = None;
        self.skin_preview_last_face = None;
        self.skin_stroke_before = None;
        if let Some(next) = self.skin_history.redo(&self.skin_pixels) {
            self.skin_pixels = next;
            self.skin_dirty = true;
            self.skin_status = Some("已重做皮肤编辑".to_owned());
        }
    }

    fn set_skin_pixel(&mut self, x: usize, y: usize) {
        if x >= 64 || y >= 64 {
            return;
        }
        let offset = (y * 64 + x) * 4;
        let color = if self.skin_erasing {
            [0, 0, 0, 0]
        } else {
            self.skin_color
        };
        if self.skin_pixels[offset..offset + 4] != color {
            self.skin_pixels[offset..offset + 4].copy_from_slice(&color);
            self.skin_dirty = true;
        }
    }

    fn paint_skin_line(&mut self, from: (usize, usize), to: (usize, usize)) {
        let (mut x0, mut y0) = (from.0 as i32, from.1 as i32);
        let (x1, y1) = (to.0 as i32, to.1 as i32);
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut error = dx + dy;
        loop {
            if (0..64).contains(&x0) && (0..64).contains(&y0) {
                self.set_skin_pixel(x0 as usize, y0 as usize);
            }
            if x0 == x1 && y0 == y1 {
                break;
            }
            let doubled = 2 * error;
            if doubled >= dy {
                error += dy;
                x0 += sx;
            }
            if doubled <= dx {
                error += dx;
                y0 += sy;
            }
        }
    }

    fn import_skin_file(&mut self) {
        let Some(path) = pick_image_file(false) else {
            return;
        };
        match image::open(&path) {
            Ok(image) => {
                let resized = image::imageops::resize(
                    &image.to_rgba8(),
                    64,
                    64,
                    image::imageops::FilterType::Nearest,
                )
                .into_raw();
                let before = self.skin_pixels.clone();
                self.skin_pixels = resized;
                self.skin_history.record(before, &self.skin_pixels);
                self.skin_last_pixel = None;
                self.skin_preview_last_face = None;
                self.skin_stroke_before = None;
                self.skin_dirty = true;
                self.skin_status = Some(format!("已导入 {}", path.display()));
            }
            Err(error) => self.skin_status = Some(format!("导入皮肤失败：{error}")),
        }
    }

    fn export_skin_file(&mut self) {
        let Some(path) = pick_image_file(true) else {
            return;
        };
        let Some(image) = image::RgbaImage::from_raw(64, 64, self.skin_pixels.clone()) else {
            self.skin_status = Some("皮肤数据无效".to_owned());
            return;
        };
        match image.save(&path) {
            Ok(()) => self.skin_status = Some(format!("已导出 {}", path.display())),
            Err(error) => self.skin_status = Some(format!("导出皮肤失败：{error}")),
        }
    }

    fn create_mod_project(&mut self) {
        let Some(output_root) = self.mod_output_directory.clone() else {
            self.mod_status = Some("请先选择工程保存位置".to_owned());
            return;
        };
        let settings = ModProjectSettings {
            name: self.mod_name_input.clone(),
            id: self.mod_id_input.to_ascii_lowercase(),
            package: self.mod_package_input.clone(),
            version: self.mod_version_input.clone(),
            minecraft: self.mod_minecraft_input.clone(),
            loader: self.mod_loader_index,
        };
        match write_mod_project(&output_root, &settings) {
            Ok(path) => {
                self.mod_status = Some(format!("工程已创建：{}", path.display()));
                open_path(&path);
            }
            Err(error) => self.mod_status = Some(format!("创建模组工程失败：{error}")),
        }
    }

    fn paint_create_workspace(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
        motion: CardMotion,
    ) {
        let page = Rect::from_min_max(
            Pos2::new(layout.content_left, layout.header.bottom() + 14.0),
            Pos2::new(layout.content_right, layout.screen.bottom() - 22.0),
        );
        let header = motion.rect(Rect::from_min_size(
            page.left_top(),
            Vec2::new(page.width(), 92.0),
        ));
        theme::glass_panel(
            painter,
            header,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(20, 32, 24), theme::GREEN, time, 26.2),
        );
        let header_content = card_clip(painter, header);
        theme::bold_text(
            &header_content,
            Pos2::new(header.left() + 24.0, header.top() + 16.0),
            "创造",
            21.0,
            theme::INK,
        );
        theme::text(
            &header_content,
            Pos2::new(header.left() + 24.0, header.top() + 47.0),
            "制作 Minecraft 皮肤与模组工程",
            8.5,
            theme::MUTED,
        );
        let tab_center =
            header.right() - 232.0 + self.create_tab_position.clamp(0.0, 1.0) * 108.0 + 50.0;
        let tab_highlight = Rect::from_center_size(
            Pos2::new(tab_center, header.top() + 46.0),
            Vec2::new(
                100.0 + (self.create_tab_velocity.abs() * 5.0).clamp(0.0, 12.0),
                38.0,
            ),
        );
        theme::fill_round(
            &header_content,
            tab_highlight,
            theme::RADIUS,
            theme::alpha(theme::GREEN, 28),
        );
        for (index, label) in ["皮肤制作", "模组制作"].into_iter().enumerate() {
            let tab = Rect::from_min_size(
                Pos2::new(
                    header.right() - 232.0 + index as f32 * 108.0,
                    header.top() + 27.0,
                ),
                Vec2::new(100.0, 38.0),
            );
            let response = ui
                .interact(tab, Id::unique(("create-tab", index)), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand);
            let selected = self.create_tab == index;
            if !selected {
                theme::fill_round(
                    &header_content,
                    tab,
                    theme::RADIUS,
                    if response.hovered() {
                        theme::white_alpha(12)
                    } else {
                        theme::white_alpha(5)
                    },
                );
            }
            theme::center_text(
                &header_content,
                tab.center(),
                label,
                8.8,
                if selected { theme::INK } else { theme::MUTED },
                selected,
            );
            if response.clicked() {
                self.switch_create_tab(index);
                ui.ctx().request_repaint();
            }
        }

        let body_base = motion.rect(Rect::from_min_max(
            Pos2::new(page.left(), page.top() + 104.0),
            page.right_bottom(),
        ));
        theme::glass_panel(
            painter,
            body_base,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(18, 29, 22), theme::GREEN, time, 26.8),
        );
        let body_clip = card_clip(painter, body_base);
        let progress = self.create_transition_progress.clamp(0.0, 1.08);
        if let Some(previous) = self.previous_create_tab {
            let mut outgoing_painter = body_clip.clone();
            outgoing_painter.set_opacity((1.0 - progress).clamp(0.0, 1.0));
            let outgoing_body = body_base.translate(Vec2::new(
                -self.create_transition_direction * body_base.width() * progress,
                0.0,
            ));
            self.paint_create_tab_content(
                previous,
                ctx,
                ui,
                &outgoing_painter,
                outgoing_body,
                time,
                false,
            );
        }
        let mut incoming_painter = body_clip;
        incoming_painter.set_opacity(progress.clamp(0.0, 1.0));
        let incoming_body = body_base.translate(Vec2::new(
            self.create_transition_direction * body_base.width() * (1.0 - progress),
            0.0,
        ));
        self.paint_create_tab_content(
            self.create_tab,
            ctx,
            ui,
            &incoming_painter,
            incoming_body,
            time,
            self.previous_create_tab.is_none(),
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_create_tab_content(
        &mut self,
        tab: usize,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        body: Rect,
        time: f64,
        active: bool,
    ) {
        let layer = painter.with_clip_rect(body.shrink(1.0));
        if tab == 0 {
            self.paint_skin_workspace(ctx, ui, &layer, body, time, active);
        } else {
            self.paint_mod_workspace(ui, &layer, body, time, active);
        }
    }

    fn paint_skin_workspace(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        body: Rect,
        _time: f64,
        active: bool,
    ) {
        self.ensure_skin_texture(ctx);
        let canvas = Rect::from_min_size(
            Pos2::new(body.left() + 24.0, body.top() + 24.0),
            Vec2::splat((body.height() - 48.0).min(360.0)),
        );
        theme::fill_round(
            painter,
            canvas,
            theme::RADIUS,
            Color32::from_rgb(12, 20, 15),
        );
        let cell = canvas.width() / 16.0;
        for y in 0..16 {
            for x in 0..16 {
                if (x + y) % 2 == 0 {
                    painter.rect_filled(
                        Rect::from_min_size(
                            Pos2::new(
                                canvas.left() + x as f32 * cell,
                                canvas.top() + y as f32 * cell,
                            ),
                            Vec2::splat(cell),
                        ),
                        0,
                        Color32::from_rgb(25, 35, 28),
                    );
                }
            }
        }
        if let Some(texture) = self.skin_texture.as_ref() {
            painter.image(
                texture.id(),
                canvas,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        for index in 1..8 {
            let x = canvas.left() + canvas.width() * index as f32 / 8.0;
            let y = canvas.top() + canvas.height() * index as f32 / 8.0;
            painter.line_segment(
                [Pos2::new(x, canvas.top()), Pos2::new(x, canvas.bottom())],
                Stroke::new(0.5, theme::white_alpha(18)),
            );
            painter.line_segment(
                [Pos2::new(canvas.left(), y), Pos2::new(canvas.right(), y)],
                Stroke::new(0.5, theme::white_alpha(18)),
            );
        }
        let response = ui
            .interact(canvas, Id::unique("skin-canvas"), Sense::click_and_drag())
            .on_hover_cursor(CursorIcon::Crosshair);
        if active
            && (response.clicked() || response.dragged())
            && let Some(pointer) = response.interact_pointer_pos()
            && canvas.contains(pointer)
        {
            let x = (((pointer.x - canvas.left()) / canvas.width()) * 64.0)
                .floor()
                .clamp(0.0, 63.0) as usize;
            let y = (((pointer.y - canvas.top()) / canvas.height()) * 64.0)
                .floor()
                .clamp(0.0, 63.0) as usize;
            self.begin_skin_stroke();
            if let Some(previous) = self.skin_last_pixel {
                self.paint_skin_line(previous, (x, y));
            } else {
                self.set_skin_pixel(x, y);
            }
            self.finish_skin_stroke();
            self.skin_last_pixel = Some((x, y));
            self.skin_preview_last_face = None;
            ui.ctx().request_repaint();
        }
        if ui.ctx().input(|input| input.pointer.any_released()) {
            self.skin_last_pixel = None;
            self.skin_preview_last_face = None;
            self.skin_stroke_before = None;
        }
        theme::stroke_round(painter, canvas, theme::RADIUS, 1.0, theme::white_alpha(28));

        let preview_card = Rect::from_min_size(
            Pos2::new(body.right() - 372.0, body.top() + 24.0),
            Vec2::new(348.0, body.height() - 48.0),
        );
        theme::glass_panel(
            painter,
            preview_card,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(20, 32, 24), theme::GREEN, 2.2, 32.8),
        );
        let preview_content = card_clip(painter, preview_card);
        let tools_x = canvas.right() + 28.0;
        let tools_width = (preview_card.left() - tools_x - 24.0).max(260.0);
        theme::bold_text(
            painter,
            Pos2::new(tools_x, body.top() + 24.0),
            "64 × 64 皮肤画布",
            16.0,
            theme::INK,
        );
        theme::text(
            painter,
            Pos2::new(tools_x, body.top() + 51.0),
            "在画布上点击或拖动绘制像素 · Ctrl+Z 撤销",
            8.2,
            theme::MUTED,
        );
        let color_swatch =
            Rect::from_min_size(Pos2::new(tools_x, body.top() + 92.0), Vec2::splat(36.0));
        theme::fill_round(
            painter,
            color_swatch,
            theme::RADIUS,
            Color32::from_rgba_unmultiplied(
                self.skin_color[0],
                self.skin_color[1],
                self.skin_color[2],
                self.skin_color[3],
            ),
        );
        theme::stroke_round(
            painter,
            color_swatch,
            theme::RADIUS,
            1.0,
            theme::white_alpha(42),
        );
        theme::bold_text(
            painter,
            Pos2::new(tools_x + 48.0, body.top() + 91.0),
            skin_rgb_hex(self.skin_color),
            11.0,
            theme::INK,
        );
        theme::text(
            painter,
            Pos2::new(tools_x + 48.0, body.top() + 113.0),
            "完整 RGB · 每个通道 0–255",
            7.6,
            theme::MUTED,
        );
        let slider_width = tools_width.clamp(250.0, 366.0);
        let channels = [
            ("R", Color32::from_rgb(235, 91, 91)),
            ("G", Color32::from_rgb(83, 199, 121)),
            ("B", Color32::from_rgb(88, 142, 244)),
        ];
        for (index, (label, accent)) in channels.into_iter().enumerate() {
            let mut channel = self.skin_color[index];
            let before = channel;
            paint_skin_rgb_slider(
                ui,
                painter,
                Rect::from_min_size(
                    Pos2::new(tools_x, body.top() + 145.0 + index as f32 * 38.0),
                    Vec2::new(slider_width, 30.0),
                ),
                match index {
                    0 => "skin-rgb-red",
                    1 => "skin-rgb-green",
                    _ => "skin-rgb-blue",
                },
                label,
                &mut channel,
                accent,
                active,
            );
            if channel != before {
                self.skin_color[index] = channel;
                self.skin_color[3] = 255;
                self.skin_erasing = false;
            }
        }
        let eraser = Rect::from_min_size(
            Pos2::new(tools_x, body.top() + 267.0),
            Vec2::new(72.0, 34.0),
        );
        if active
            && settings_button(
                ui,
                painter,
                eraser,
                "skin-eraser",
                "橡皮擦",
                self.skin_erasing,
            )
        {
            self.skin_erasing = !self.skin_erasing;
        }
        let model = Rect::from_min_size(
            Pos2::new(tools_x + 82.0, body.top() + 267.0),
            Vec2::new(110.0, 34.0),
        );
        if active
            && settings_button(
                ui,
                painter,
                model,
                "skin-model",
                if self.skin_model_alex {
                    "Alex 模型"
                } else {
                    "Steve 模型"
                },
                false,
            )
        {
            self.skin_model_alex = !self.skin_model_alex;
        }
        let undo = Rect::from_min_size(
            Pos2::new(tools_x + 202.0, body.top() + 267.0),
            Vec2::new(74.0, 34.0),
        );
        if active
            && skin_history_button(
                ui,
                painter,
                undo,
                "skin-undo",
                "撤销",
                self.skin_history.can_undo(),
            )
        {
            self.undo_skin_edit();
            ui.ctx().request_repaint();
        }
        let redo = Rect::from_min_size(
            Pos2::new(tools_x + 286.0, body.top() + 267.0),
            Vec2::new(74.0, 34.0),
        );
        if active
            && skin_history_button(
                ui,
                painter,
                redo,
                "skin-redo",
                "重做",
                self.skin_history.can_redo(),
            )
        {
            self.redo_skin_edit();
            ui.ctx().request_repaint();
        }
        let buttons = [
            ("导入 PNG", "skin-import"),
            ("导出 PNG", "skin-export"),
            ("清空画布", "skin-clear"),
        ];
        for (index, (label, id)) in buttons.into_iter().enumerate() {
            let button = Rect::from_min_size(
                Pos2::new(tools_x + index as f32 * 118.0, body.top() + 317.0),
                Vec2::new(108.0, 36.0),
            );
            if active && settings_button(ui, painter, button, id, label, false) {
                match index {
                    0 => self.import_skin_file(),
                    1 => self.export_skin_file(),
                    _ => {
                        let before = self.skin_pixels.clone();
                        self.skin_pixels.fill(0);
                        self.skin_history.record(before, &self.skin_pixels);
                        self.skin_last_pixel = None;
                        self.skin_preview_last_face = None;
                        self.skin_stroke_before = None;
                        self.skin_dirty = true;
                        self.skin_status = Some("画布已清空".to_owned());
                    }
                }
            }
        }
        if let Some(status) = self.skin_status.as_ref() {
            theme::text(
                painter,
                Pos2::new(tools_x, body.top() + 373.0),
                fit_text_to_width(painter, status, theme::font(8.0), tools_width),
                8.0,
                theme::GREEN_BRIGHT,
            );
        }
        let preview_inner = preview_content;
        theme::bold_text(
            &preview_inner,
            Pos2::new(preview_card.left() + 18.0, preview_card.top() + 17.0),
            "3D 角色预览",
            10.0,
            theme::INK,
        );
        theme::text(
            &preview_inner,
            Pos2::new(preview_card.left() + 18.0, preview_card.top() + 39.0),
            "右键拖动旋转 · 左键在模型上绘制",
            7.5,
            theme::MUTED,
        );
        let preview_stage = Rect::from_min_max(
            Pos2::new(preview_card.left() + 14.0, preview_card.top() + 58.0),
            Pos2::new(preview_card.right() - 14.0, preview_card.bottom() - 24.0),
        );
        let preview_response = ui
            .interact(
                preview_stage,
                Id::unique("skin-preview-3d"),
                Sense::click_and_drag(),
            )
            .on_hover_cursor(CursorIcon::Crosshair);
        if active && preview_response.dragged_by(egui::PointerButton::Secondary) {
            let delta = ui.ctx().input(|input| input.pointer.delta());
            self.skin_preview_yaw =
                (self.skin_preview_yaw + delta.x * 0.012).rem_euclid(std::f32::consts::TAU);
            self.skin_preview_pitch =
                (self.skin_preview_pitch + delta.y * 0.009).clamp(-1.15, 1.15);
        }
        let model_scale = ((preview_stage.width() - 44.0).max(1.0) / 18.2)
            .min((preview_stage.height() - 30.0).max(1.0) / 33.5);
        let shadow_center = Pos2::new(
            preview_stage.center().x,
            preview_stage.center().y + model_scale * 15.8,
        );
        preview_inner.add(Shape::ellipse_filled(
            shadow_center + Vec2::new(0.0, 4.0),
            Vec2::new(model_scale * 6.4, model_scale * 1.45),
            Color32::from_black_alpha(52),
        ));
        preview_inner.add(Shape::ellipse_filled(
            shadow_center,
            Vec2::new(model_scale * 4.8, model_scale * 0.82),
            Color32::from_black_alpha(30),
        ));
        let preview_faces = skin_preview_faces(
            preview_stage,
            model_scale,
            self.skin_preview_yaw,
            self.skin_preview_pitch,
            self.skin_model_alex,
        );
        if let Some(texture) = self.skin_texture.as_ref() {
            paint_skin_model(&preview_inner, texture, &preview_faces);
        }
        let hovered_face = if active {
            preview_response
                .hover_pos()
                .and_then(|pointer| skin_face_at(preview_faces.iter().rev(), pointer))
        } else {
            None
        };
        if let Some(face) = hovered_face {
            preview_inner.add(Shape::convex_polygon(
                face.points.to_vec(),
                theme::white_alpha(10),
                Stroke::new(1.0, theme::white_alpha(72)),
            ));
        }
        if active
            && (preview_response.clicked_by(egui::PointerButton::Primary)
                || preview_response.dragged_by(egui::PointerButton::Primary))
            && let Some(pointer) = preview_response.interact_pointer_pos()
            && let Some(face) = skin_face_at(preview_faces.iter().rev(), pointer)
            && let Some(uv) = face.texture_position(pointer)
        {
            let pixel = (
                (uv.x * 64.0).floor().clamp(0.0, 63.0) as usize,
                (uv.y * 64.0).floor().clamp(0.0, 63.0) as usize,
            );
            self.begin_skin_stroke();
            if self.skin_preview_last_face == Some(face.key)
                && let Some(previous) = self.skin_last_pixel
            {
                self.paint_skin_line(previous, pixel);
            } else {
                self.set_skin_pixel(pixel.0, pixel.1);
            }
            self.finish_skin_stroke();
            self.skin_last_pixel = Some(pixel);
            self.skin_preview_last_face = Some(face.key);
            ui.ctx().request_repaint();
        }
    }

    fn paint_mod_workspace(
        &mut self,
        ui: &mut egui::Ui,
        painter: &Painter,
        body: Rect,
        _time: f64,
        active: bool,
    ) {
        let left = body.left() + 28.0;
        let width = (body.width() - 56.0).min(760.0);
        theme::bold_text(
            painter,
            Pos2::new(left, body.top() + 24.0),
            "模组工程",
            17.0,
            theme::INK,
        );
        theme::text(
            painter,
            Pos2::new(left, body.top() + 52.0),
            "生成 Java 源码与 Gradle 工程 · Minecraft 目标版本独立设置",
            8.3,
            theme::MUTED,
        );
        let fields = [
            ("模组名称", 0),
            ("模组 ID", 1),
            ("Java 包名", 2),
            ("模组版本", 3),
            ("目标 MC 版本", 4),
        ];
        for (index, (label, kind)) in fields.into_iter().enumerate() {
            let y = body.top() + 82.0 + index as f32 * 48.0;
            theme::text(painter, Pos2::new(left, y + 7.0), label, 8.0, theme::MUTED);
            let field =
                Rect::from_min_size(Pos2::new(left + 128.0, y), Vec2::new(width - 128.0, 34.0));
            theme::fill_round(painter, field, theme::RADIUS, Color32::from_rgb(24, 38, 28));
            theme::stroke_round(painter, field, theme::RADIUS, 1.0, theme::white_alpha(26));
            let target = match kind {
                0 => &mut self.mod_name_input,
                1 => &mut self.mod_id_input,
                2 => &mut self.mod_package_input,
                3 => &mut self.mod_version_input,
                _ => &mut self.mod_minecraft_input,
            };
            if active {
                ui.put(
                    field.shrink2(Vec2::new(10.0, 5.0)),
                    egui::TextEdit::singleline(target)
                        .frame(egui::Frame::NONE)
                        .font(theme::font(8.8))
                        .text_color(theme::INK),
                );
            }
        }
        let loader_y = body.top() + 82.0 + fields.len() as f32 * 48.0;
        theme::text(
            painter,
            Pos2::new(left, loader_y + 7.0),
            "加载器",
            8.0,
            theme::MUTED,
        );
        for (index, label) in ["Fabric", "Forge", "NeoForge"].into_iter().enumerate() {
            let button = Rect::from_min_size(
                Pos2::new(left + 128.0 + index as f32 * 94.0, loader_y),
                Vec2::new(84.0, 34.0),
            );
            let selected = self.mod_loader_index == index;
            if active
                && settings_button(
                    ui,
                    painter,
                    button,
                    match index {
                        0 => "mod-loader-fabric",
                        1 => "mod-loader-forge",
                        _ => "mod-loader-neoforge",
                    },
                    label,
                    selected,
                )
            {
                self.mod_loader_index = index;
            }
        }
        let output_y = loader_y + 52.0;
        let output = Rect::from_min_size(
            Pos2::new(left + 128.0, output_y),
            Vec2::new(width - 238.0, 34.0),
        );
        theme::fill_round(
            painter,
            output,
            theme::RADIUS,
            Color32::from_rgb(22, 34, 26),
        );
        theme::text(
            painter,
            Pos2::new(output.left() + 10.0, output.top() + 8.0),
            self.mod_output_directory
                .as_ref()
                .map(|path| ellipsize_start(&path.display().to_string(), 60))
                .unwrap_or_else(|| "尚未选择工程保存位置".to_owned()),
            8.0,
            if self.mod_output_directory.is_some() {
                theme::INK_SOFT
            } else {
                theme::MUTED
            },
        );
        let choose = Rect::from_min_size(
            Pos2::new(output.right() + 8.0, output_y),
            Vec2::new(92.0, 34.0),
        );
        if active
            && settings_button(ui, painter, choose, "mod-choose-output", "选择位置", false)
            && let Some(directory) = pick_project_directory()
        {
            self.mod_output_directory = Some(directory);
        }
        let create = Rect::from_min_size(
            Pos2::new(left + 128.0, output_y + 48.0),
            Vec2::new(124.0, 38.0),
        );
        if active && settings_button(ui, painter, create, "mod-create-project", "创建工程", true)
        {
            self.create_mod_project();
        }
        if let Some(status) = self.mod_status.as_ref() {
            theme::text(
                painter,
                Pos2::new(left + 264.0, output_y + 57.0),
                fit_text_to_width(painter, status, theme::font(8.0), width - 280.0),
                8.0,
                Color32::from_rgb(169, 205, 165),
            );
        }
    }

    #[allow(dead_code)]
    fn paint_create(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
        motion: CardMotion,
    ) {
        let page = Rect::from_min_max(
            Pos2::new(layout.content_left, layout.header.bottom() + 14.0),
            Pos2::new(layout.content_right, layout.screen.bottom() - 22.0),
        );
        let hero = motion.rect(Rect::from_min_max(
            page.left_top(),
            Pos2::new(page.right(), page.top() + 218.0),
        ));
        let create_base = painter;
        theme::glass_panel(
            painter,
            hero,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(19, 31, 23), theme::GREEN, time, 26.0),
        );
        let hero_content = card_clip(painter, hero);
        let painter = &hero_content;
        theme::soft_orb(
            painter,
            Pos2::new(hero.right() - 170.0, hero.center().y),
            120.0,
            theme::GREEN,
            15,
        );
        paint_block(
            painter,
            Pos2::new(hero.right() - 170.0, hero.center().y + 4.0),
            82.0,
            Color32::from_rgb(132, 218, 108),
            Color32::from_rgb(91, 66, 46),
            Color32::from_rgb(73, 151, 70),
        );
        theme::mono_text(
            painter,
            Pos2::new(hero.left() + 30.0, hero.top() + 30.0),
            "CREATE",
            7.5,
            Color32::from_rgb(139, 196, 133),
        );
        theme::bold_text(
            painter,
            Pos2::new(hero.left() + 30.0, hero.top() + 48.0),
            "创造，从一个新世界开始。",
            27.0,
            theme::INK,
        );
        theme::wrapped_text(
            painter,
            Pos2::new(hero.left() + 30.0, hero.top() + 92.0),
            "创建新的生存或创造世界，导入已有存档，也可以从资源包和截图开始整理你的创作。",
            9.5,
            theme::INK_SOFT,
            (hero.width() * 0.58).max(330.0),
        );
        let create_world = Rect::from_min_size(
            Pos2::new(hero.left() + 30.0, hero.bottom() - 58.0),
            Vec2::new(147.0, 38.0),
        );
        if settings_button(
            ui,
            painter,
            create_world,
            "create-new-world",
            "启动并创建世界",
            true,
        ) {
            self.show_toast("正在启动 Minecraft，请在游戏中选择创建新世界");
            self.launch_without_recent = true;
            self.start_launch(ctx);
        }

        let painter = create_base;
        let gap = 12.0;
        let card_width = (page.width() - gap * 2.0) / 3.0;
        let cards = [
            ("导入存档", "将存档放入 saves 目录", "saves"),
            ("资源包", "打开 resourcepacks 目录", "resourcepacks"),
            ("截图画廊", "浏览游戏内截图", "screenshots"),
        ];
        for (index, (title, description, folder)) in cards.into_iter().enumerate() {
            let rect = motion.rect(Rect::from_min_size(
                Pos2::new(
                    page.left() + index as f32 * (card_width + gap),
                    page.top() + 218.0 + 14.0,
                ),
                Vec2::new(card_width, 132.0),
            ));
            let response = ui
                .interact(rect, Id::unique(("create-card", index)), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand);
            theme::glass_panel(
                painter,
                rect,
                theme::RADIUS,
                theme::GlassSpec::new(
                    Color32::from_rgb(18, 29, 22),
                    theme::GREEN,
                    time,
                    27.0 + index as f32,
                )
                .hovered(response.hovered()),
            );
            let card_content = card_clip(painter, rect);
            let painter = &card_content;
            draw_icon(
                painter,
                match index {
                    0 => NavIcon::Folder,
                    1 => NavIcon::Box,
                    _ => NavIcon::Sparkles,
                },
                Rect::from_center_size(
                    Pos2::new(rect.left() + 32.0, rect.top() + 34.0),
                    Vec2::splat(22.0),
                ),
                theme::GREEN_BRIGHT,
            );
            theme::bold_text(
                painter,
                Pos2::new(rect.left() + 55.0, rect.top() + 22.0),
                title,
                12.5,
                theme::INK,
            );
            theme::wrapped_text(
                painter,
                Pos2::new(rect.left() + 20.0, rect.top() + 62.0),
                description,
                8.2,
                theme::MUTED,
                rect.width() - 40.0,
            );
            theme::text(
                painter,
                Pos2::new(rect.left() + 20.0, rect.bottom() - 27.0),
                "打开目录  →",
                8.0,
                if response.hovered() {
                    theme::GREEN_BRIGHT
                } else {
                    Color32::from_rgb(157, 204, 151)
                },
            );
            if response.clicked()
                && let Some(base) = self.version_data_directory()
            {
                let path = base.join(folder);
                let _ = std::fs::create_dir_all(&path);
                open_path(&path);
            }
        }
    }

    fn paint_settings(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
        motion: CardMotion,
    ) {
        let (outgoing, incoming) = if let Some(previous_section) = self.previous_settings_section {
            let outgoing = CardMotion {
                screen: layout.screen,
                direction: self.settings_transition_direction,
                progress: self.settings_transition_progress,
                outgoing: true,
            };
            let incoming = CardMotion {
                screen: layout.screen,
                direction: self.settings_transition_direction,
                progress: self.settings_transition_progress,
                outgoing: false,
            };
            (Some((previous_section, outgoing)), Some(incoming))
        } else {
            (None, None)
        };
        let content_motion = incoming.unwrap_or(motion);
        if let Some((previous_section, outgoing_motion)) = outgoing {
            self.paint_settings_view(
                ctx,
                ui,
                painter,
                layout,
                time,
                SettingsView {
                    section: previous_section,
                    chrome_motion: motion,
                    content_motion: outgoing_motion,
                    draw_chrome: false,
                    draw_footer: false,
                },
            );
        }
        self.paint_settings_view(
            ctx,
            ui,
            painter,
            layout,
            time,
            SettingsView {
                section: self.settings_section,
                chrome_motion: motion,
                content_motion,
                draw_chrome: true,
                draw_footer: true,
            },
        );
    }

    fn paint_settings_view(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
        view: SettingsView,
    ) {
        let page = Rect::from_min_max(
            Pos2::new(layout.content_left, layout.header.bottom() + 14.0),
            Pos2::new(layout.content_right, layout.screen.bottom() - 22.0),
        );
        let category_width = 210.0;
        let source_categories = Rect::from_min_max(
            page.left_top(),
            Pos2::new(page.left() + category_width, page.bottom()),
        );
        let source_panel = Rect::from_min_max(
            Pos2::new(source_categories.right() + 14.0, page.top()),
            page.right_bottom(),
        );
        let categories = view.chrome_motion.rect(source_categories);
        let panel = view.chrome_motion.rect(source_panel);

        if view.draw_chrome {
            let chrome_base = painter;
            let title_card = Rect::from_min_size(
                Pos2::new(categories.left() + 8.0, categories.top() + 8.0),
                Vec2::new(categories.width() - 16.0, 66.0),
            );
            theme::glass_panel(
                painter,
                title_card,
                theme::RADIUS,
                theme::GlassSpec::new(Color32::from_rgb(20, 32, 24), theme::GREEN, time, 12.2),
            );
            let title_content = card_clip(painter, title_card);
            let painter = &title_content;
            theme::mono_text(
                painter,
                Pos2::new(title_card.left() + 16.0, title_card.top() + 13.0),
                "SETTINGS",
                7.4,
                theme::MUTED,
            );
            theme::bold_text(
                painter,
                Pos2::new(title_card.left() + 16.0, title_card.top() + 30.0),
                "设置中心",
                18.0,
                theme::INK,
            );

            let painter = chrome_base;
            let item_top = categories.top() + 86.0;
            let target_y = item_top + 21.0 + view.section.index() as f32 * SETTINGS_ITEM_SPACING;
            if !self.settings_highlight_initialized || !self.settings.animations {
                self.settings_highlight_y = target_y;
                self.settings_highlight_velocity = 0.0;
                self.settings_highlight_initialized = true;
            } else {
                let delta = ctx.input(|input| input.stable_dt).clamp(0.001, 0.05);
                if spring_smooth(
                    &mut self.settings_highlight_y,
                    &mut self.settings_highlight_velocity,
                    target_y,
                    delta,
                ) {
                    ctx.request_repaint_after(Duration::from_millis(16));
                }
            }
            let highlight = Rect::from_center_size(
                Pos2::new(categories.center().x, self.settings_highlight_y),
                Vec2::new(categories.width() - 20.0, 42.0),
            );
            theme::glass_panel(
                painter,
                highlight,
                theme::RADIUS,
                theme::GlassSpec::new(Color32::from_rgb(29, 49, 33), theme::GREEN, time, 13.8),
            );
            painter.rect_filled(
                Rect::from_min_size(
                    Pos2::new(categories.left() + 8.0, highlight.center().y - 9.5),
                    Vec2::new(3.0, 19.0),
                ),
                theme::RADIUS,
                theme::GREEN_BRIGHT,
            );

            for (index, section) in SettingsSection::ALL.into_iter().enumerate() {
                let item = Rect::from_min_size(
                    Pos2::new(
                        categories.left() + 10.0,
                        item_top + index as f32 * SETTINGS_ITEM_SPACING,
                    ),
                    Vec2::new(categories.width() - 20.0, 42.0),
                );
                let response = ui
                    .interact(
                        item,
                        Id::unique(("settings-category", index)),
                        Sense::click(),
                    )
                    .on_hover_cursor(CursorIcon::PointingHand);
                let item_content = card_clip(painter, item);
                let selected = self.settings_section == section;
                draw_icon(
                    &item_content,
                    match section {
                        SettingsSection::General => NavIcon::Settings,
                        SettingsSection::Game => NavIcon::Box,
                        SettingsSection::Versions => NavIcon::Layers,
                        SettingsSection::Launch => NavIcon::Monitor,
                        SettingsSection::Appearance => NavIcon::Sparkles,
                        SettingsSection::About => NavIcon::Monitor,
                    },
                    Rect::from_center_size(
                        Pos2::new(item.left() + 23.0, item.center().y),
                        Vec2::splat(17.0),
                    ),
                    if selected {
                        theme::GREEN_BRIGHT
                    } else if response.hovered() {
                        theme::INK_SOFT
                    } else {
                        theme::MUTED
                    },
                );
                theme::bold_text(
                    &item_content,
                    Pos2::new(item.left() + 43.0, item.top() + 11.0),
                    section.label(),
                    9.4,
                    if selected {
                        theme::INK
                    } else if response.hovered() {
                        theme::INK_SOFT
                    } else {
                        Color32::from_rgb(163, 176, 166)
                    },
                );
                if response.clicked() {
                    self.switch_settings_section(section);
                    ctx.request_repaint();
                }
            }

            painter.line_segment(
                [
                    Pos2::new(categories.left() + 16.0, categories.bottom() - 58.0),
                    Pos2::new(categories.right() - 16.0, categories.bottom() - 58.0),
                ],
                Stroke::new(1.0, theme::LINE),
            );
            theme::text(
                painter,
                Pos2::new(categories.left() + 18.0, categories.bottom() - 45.0),
                "配置保存在本机",
                7.4,
                theme::MUTED,
            );
            theme::mono_text(
                painter,
                Pos2::new(categories.left() + 18.0, categories.bottom() - 29.0),
                "settings.ini",
                8.0,
                Color32::from_rgb(139, 177, 135),
            );
        }

        let painter = painter.clone();
        let painter = &painter;
        let heading_card = view.content_motion.rect(Rect::from_min_size(
            Pos2::new(source_panel.left(), source_panel.top() + 8.0),
            Vec2::new(source_panel.width(), 72.0),
        ));
        let section_base = painter;
        theme::glass_panel(
            painter,
            heading_card,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(20, 32, 24), theme::GREEN, time, 13.5),
        );
        let heading_content = card_clip(painter, heading_card);
        let painter = &heading_content;
        let heading_y = heading_card.top() + 13.0;
        theme::bold_text(
            painter,
            Pos2::new(heading_card.left() + 22.0, heading_y),
            view.section.label(),
            20.0,
            theme::INK,
        );
        theme::text(
            painter,
            Pos2::new(heading_card.left() + 22.0, heading_y + 30.0),
            view.section.description(),
            8.6,
            theme::MUTED,
        );
        let painter = section_base;

        let source_row_left = source_panel.left() + 26.0;
        let source_row_right = source_panel.right() - 26.0;
        let source_row_width = source_row_right - source_row_left;
        let setting_row = |y: f32, height: f32| {
            view.content_motion.rect(Rect::from_min_size(
                Pos2::new(source_row_left, y),
                Vec2::new(source_row_width, height),
            ))
        };
        let mut row_y = source_panel.top() + 96.0;
        let row_gap = 74.0;
        match view.section {
            SettingsSection::General => {
                let row = setting_row(row_y, row_gap);
                paint_setting_card(painter, row, time);
                paint_toggle_row(
                    ui,
                    painter,
                    row,
                    "setting-check-updates",
                    "启动时检查更新",
                    "启动器打开时检查版本与组件更新",
                    &mut self.settings.check_updates,
                );
                row_y += row_gap;
                let row = setting_row(row_y, row_gap);
                paint_setting_card(painter, row, time);
                paint_toggle_row(
                    ui,
                    painter,
                    row,
                    "setting-notify-launch",
                    "启动完成后通知",
                    "游戏进程接管后显示桌面提示",
                    &mut self.settings.notify_after_launch,
                );
                row_y += row_gap;
                let row = setting_row(row_y, row_gap);
                paint_setting_card(painter, row, time);
                let current_version = env!("CARGO_PKG_VERSION");
                let available = self.launcher_update_result.as_ref().is_some_and(|release| {
                    update::is_newer_version(current_version, &release.version)
                });
                let description = if self.launcher_update_receiver.is_some() {
                    "正在从 GitHub Releases 检查最新版本".to_owned()
                } else if let Some(release) = self.launcher_update_result.as_ref() {
                    if available {
                        format!("发现新版本 v{} · 当前 v{current_version}", release.version)
                    } else {
                        format!("已是最新版本 · 当前 v{current_version}")
                    }
                } else if let Some(error) = self.launcher_update_error.as_ref() {
                    ellipsize_end(&format!("检查失败 · {error}"), 56)
                } else {
                    format!("检查 Genesis Launcher 的 GitHub Releases · 当前 v{current_version}")
                };
                paint_setting_label(painter, row, "启动器更新", &description);
                let release_button = Rect::from_min_size(
                    Pos2::new(
                        row.right() - SETTING_CARD_PADDING_X - 88.0,
                        row.top() + 16.0,
                    ),
                    Vec2::new(88.0, 34.0),
                );
                let check_button = Rect::from_min_size(
                    Pos2::new(release_button.left() - 90.0, row.top() + 16.0),
                    Vec2::new(82.0, 34.0),
                );
                if settings_button(
                    ui,
                    painter,
                    check_button,
                    "launcher-update-check",
                    "检查更新",
                    false,
                ) {
                    self.start_launcher_update_check(ui.ctx(), true);
                }
                if settings_button(
                    ui,
                    painter,
                    release_button,
                    "launcher-update-page",
                    if available {
                        "打开更新"
                    } else {
                        "发布页"
                    },
                    available,
                ) {
                    let page_url = self
                        .launcher_update_result
                        .as_ref()
                        .map(|release| release.page_url.as_str())
                        .unwrap_or(update::RELEASES_URL);
                    if let Err(error) = crate::favorites::open_site(page_url) {
                        self.show_toast(format!("无法打开更新页面：{error}"));
                    }
                }
            }
            SettingsSection::Game => {
                let row = setting_row(row_y, row_gap);
                paint_setting_card(painter, row, time);
                paint_setting_label(painter, row, "游戏内存", "为 Java 虚拟机预留的最大内存");
                let minus = Rect::from_center_size(
                    Pos2::new(row.right() - 147.0, row.center().y),
                    Vec2::splat(34.0),
                );
                let plus = Rect::from_center_size(
                    Pos2::new(row.right() - 33.0, row.center().y),
                    Vec2::splat(34.0),
                );
                let minus_hit = compact_icon_button(ui, painter, minus, "memory-minus", false);
                let plus_hit = compact_icon_button(ui, painter, plus, "memory-plus", true);
                if minus_hit {
                    self.settings.memory_mb = self.settings.memory_mb.saturating_sub(512).max(2048);
                }
                if plus_hit {
                    self.settings.memory_mb = (self.settings.memory_mb + 512).min(16384);
                }
                theme::bold_text(
                    painter,
                    Pos2::new(row.right() - 104.0, row.center().y - 10.0),
                    format!("{:.1} GB", self.settings.memory_mb as f32 / 1024.0),
                    14.0,
                    theme::INK,
                );
                row_y += row_gap;

                let row = setting_row(row_y, row_gap);
                paint_setting_card(painter, row, time);
                let installed = self
                    .settings
                    .game_directory
                    .as_deref()
                    .map(crate::game::installed_versions)
                    .unwrap_or_default();
                let directory_description = self
                    .settings
                    .game_directory
                    .as_ref()
                    .map(|path| {
                        let path = path.display().to_string();
                        let suffix = path
                            .char_indices()
                            .rev()
                            .nth(44)
                            .map(|(index, _)| format!("…{}", &path[index..]))
                            .unwrap_or(path);
                        format!("已发现 {} 个版本 · {suffix}", installed.len())
                    })
                    .unwrap_or_else(|| "尚未发现游戏目录".to_owned());
                paint_setting_label(painter, row, "游戏目录", &directory_description);
                let discover = Rect::from_min_size(
                    Pos2::new(
                        row.right() - SETTING_CARD_PADDING_X - 86.0,
                        row.top() + 18.0,
                    ),
                    Vec2::new(86.0, 32.0),
                );
                if settings_button(
                    ui,
                    painter,
                    discover,
                    "discover-game-dir",
                    "官方目录",
                    false,
                ) {
                    if let Some(current) = self.settings.game_directory.clone() {
                        self.settings.add_game_directory(current);
                    }
                    let discovered = crate::game::discover_game_directories();
                    if let Some(directory) = discovered.first().cloned() {
                        let count = crate::game::installed_versions(&directory).len();
                        for candidate in discovered {
                            self.settings.add_game_directory(candidate);
                        }
                        self.select_game_directory(directory);
                        self.show_toast(format!("已选择官方游戏目录 · {} 个版本", count));
                    } else {
                        self.show_toast("未找到官方 Minecraft 游戏目录");
                    }
                }
                if self.settings.game_directory.is_some() {
                    let open = Rect::from_min_size(
                        Pos2::new(
                            row.right() - SETTING_CARD_PADDING_X - 166.0,
                            row.top() + 18.0,
                        ),
                        Vec2::new(72.0, 32.0),
                    );
                    if settings_button(ui, painter, open, "open-game-dir", "打开", false) {
                        self.open_game_folder(ctx);
                    }
                }
                row_y += row_gap;

                let row = setting_row(row_y, row_gap);
                paint_setting_card(painter, row, time);
                let java_description = self
                    .settings
                    .java_path
                    .as_ref()
                    .map(|path| self.java_runtime_label(path))
                    .unwrap_or_else(|| "自动选择兼容版本".to_owned());
                paint_setting_label(
                    painter,
                    row,
                    "Java 运行时",
                    &ellipsize_end(&java_description, 48),
                );
                let select_java = Rect::from_min_size(
                    Pos2::new(
                        row.right() - SETTING_CARD_PADDING_X - 82.0,
                        row.top() + 18.0,
                    ),
                    Vec2::new(82.0, 32.0),
                );
                if settings_button(
                    ui,
                    painter,
                    select_java,
                    "settings-java-select",
                    "选择",
                    false,
                ) {
                    self.java_picker_anchor = select_java;
                    self.java_picker_target = Some(JavaPickerTarget::Global);
                    self.java_picker_scroll = 0.0;
                }
                row_y += row_gap;

                let row = setting_row(row_y, row_gap);
                paint_setting_card(painter, row, time);
                paint_setting_label(painter, row, "JVM 启动参数", "-Xms512m · 由启动器自动管理");
                paint_status_badge(painter, row, "自动");
            }
            SettingsSection::Versions => {
                let row = setting_row(row_y, row_gap);
                paint_setting_card(painter, row, time);
                let current_directory = self
                    .settings
                    .game_directory
                    .as_ref()
                    .map(|path| ellipsize_start(&path.display().to_string(), 34))
                    .unwrap_or_else(|| "尚未选择 Minecraft 文件夹".to_owned());
                let directory_description = format!(
                    "{} · 已保存 {} 个文件夹",
                    current_directory,
                    self.settings.game_directories.len()
                );
                paint_setting_label(painter, row, "Minecraft 文件夹", &directory_description);
                let browse = Rect::from_min_size(
                    Pos2::new(
                        row.right() - SETTING_CARD_PADDING_X - 72.0,
                        row.top() + 18.0,
                    ),
                    Vec2::new(72.0, 32.0),
                );
                if settings_button(
                    ui,
                    painter,
                    browse,
                    "settings-version-folder-browse",
                    "浏览",
                    false,
                ) && let Some(directory) = crate::game::pick_game_directory()
                {
                    self.select_game_directory(directory);
                }
                let scan = Rect::from_min_size(
                    Pos2::new(browse.left() - 78.0, row.top() + 18.0),
                    Vec2::new(72.0, 32.0),
                );
                if settings_button(
                    ui,
                    painter,
                    scan,
                    "settings-version-folder-scan",
                    "官方",
                    false,
                ) {
                    for directory in crate::game::discover_game_directories() {
                        self.settings.add_game_directory(directory);
                    }
                    self.show_toast(format!(
                        "官方 Minecraft 文件夹已载入 · 共保存 {} 个",
                        self.settings.game_directories.len()
                    ));
                }
                row_y += row_gap;

                let row = setting_row(row_y, row_gap);
                paint_setting_card(painter, row, time);
                paint_toggle_row(
                    ui,
                    painter,
                    row,
                    "setting-show-loader-version",
                    "显示加载器版本号",
                    "在版本选择名称后附加 Fabric、Forge 或 NeoForge 的实际版本号",
                    &mut self.settings.show_loader_versions,
                );
                row_y += row_gap;

                let row = setting_row(row_y, row_gap);
                paint_setting_card(painter, row, time);
                let previous_isolation = self.settings.version_isolation;
                paint_toggle_row(
                    ui,
                    painter,
                    row,
                    "setting-version-isolation",
                    "版本隔离（全局）",
                    "所有版本分别使用独立的 mods、saves 和配置目录",
                    &mut self.settings.version_isolation,
                );
                if self.settings.version_isolation != previous_isolation {
                    self.set_version_isolation(self.settings.version_isolation);
                }
                row_y += row_gap;

                let row = setting_row(row_y, row_gap);
                paint_setting_card(painter, row, time);
                let installed_count = self
                    .settings
                    .game_directory
                    .as_deref()
                    .map(crate::game::installed_versions)
                    .map(|versions| versions.len())
                    .unwrap_or(0);
                let launchable_count = self
                    .versions
                    .iter()
                    .filter(|version| version.launchable)
                    .count();
                paint_setting_label(
                    painter,
                    row,
                    "版本概览",
                    &format!("{installed_count} 个已安装 · {launchable_count} 个完整可启动"),
                );
                let open = Rect::from_min_size(
                    Pos2::new(
                        row.right() - SETTING_CARD_PADDING_X - 106.0,
                        row.top() + 18.0,
                    ),
                    Vec2::new(106.0, 32.0),
                );
                if settings_button(
                    ui,
                    painter,
                    open,
                    "settings-open-versions",
                    "打开版本目录",
                    false,
                ) && let Some(directory) = self.settings.game_directory.as_ref()
                {
                    let path = directory.join("versions");
                    let _ = std::fs::create_dir_all(&path);
                    open_path(&path);
                }
            }
            SettingsSection::Launch => {
                let row = setting_row(row_y, row_gap);
                paint_setting_card(painter, row, time);
                paint_setting_label(
                    painter,
                    row,
                    "窗口分辨率",
                    "Minecraft 启动时使用的初始窗口大小",
                );
                let resolution = Rect::from_min_size(
                    Pos2::new(
                        row.right() - SETTING_CARD_PADDING_X - 132.0,
                        row.top() + 18.0,
                    ),
                    Vec2::new(132.0, 32.0),
                );
                let label = format!(
                    "{} × {}",
                    self.settings.window_width, self.settings.window_height
                );
                if settings_button(ui, painter, resolution, "launch-resolution", &label, false) {
                    const PRESETS: [(u32, u32); 4] =
                        [(1280, 720), (1600, 900), (1920, 1080), (2560, 1440)];
                    let next = PRESETS
                        .iter()
                        .position(|size| {
                            *size == (self.settings.window_width, self.settings.window_height)
                        })
                        .map(|index| (index + 1) % PRESETS.len())
                        .unwrap_or(0);
                    (self.settings.window_width, self.settings.window_height) = PRESETS[next];
                }
                row_y += row_gap;

                let row = setting_row(row_y, row_gap);
                paint_setting_card(painter, row, time);
                paint_toggle_row(
                    ui,
                    painter,
                    row,
                    "setting-fullscreen",
                    "全屏启动",
                    "游戏启动后直接进入全屏模式",
                    &mut self.settings.fullscreen,
                );
                row_y += row_gap;

                let row = setting_row(row_y, row_gap);
                paint_setting_card(painter, row, time);
                paint_setting_label(
                    painter,
                    row,
                    "额外 JVM 参数",
                    "空格分隔，例如 -Xss2M -Dfile.encoding=UTF-8",
                );
                let input = Rect::from_min_size(
                    Pos2::new(
                        row.right() - SETTING_CARD_PADDING_X - 390.0,
                        row.top() + 17.0,
                    ),
                    Vec2::new(390.0, 36.0),
                );
                theme::fill_round(painter, input, theme::RADIUS, Color32::from_rgb(25, 38, 29));
                theme::stroke_round(painter, input, theme::RADIUS, 1.0, theme::white_alpha(34));
                let _ = ui.put(
                    input.shrink2(Vec2::new(11.0, 5.0)),
                    egui::TextEdit::singleline(&mut self.settings.extra_jvm_args)
                        .frame(egui::Frame::NONE)
                        .font(theme::font(9.0))
                        .text_color(theme::INK)
                        .hint_text("可选启动参数"),
                );
                row_y += row_gap;

                let row = setting_row(row_y, row_gap);
                paint_setting_card(painter, row, time);
                paint_toggle_row(
                    ui,
                    painter,
                    row,
                    "setting-hide-launch-settings",
                    "启动后隐藏启动器",
                    "游戏进程启动完成后自动最小化启动器",
                    &mut self.settings.hide_after_launch,
                );
            }
            SettingsSection::Appearance => {
                let card_height = 82.0;
                let column_gap = 12.0;
                let row_gap = 12.0;
                let card_width = ((source_row_width - column_gap) * 0.5).max(260.0);
                let appearance_row = |row_index: usize, column: usize| {
                    view.content_motion.rect(Rect::from_min_size(
                        Pos2::new(
                            source_row_left + column as f32 * (card_width + column_gap),
                            source_panel.top() + 96.0 + row_index as f32 * (card_height + row_gap),
                        ),
                        Vec2::new(card_width, card_height),
                    ))
                };

                let row = appearance_row(0, 0);
                paint_setting_card(painter, row, time);
                paint_toggle_row(
                    ui,
                    painter,
                    row,
                    "setting-liquid-glass",
                    "液态玻璃",
                    "开启 GPU 背景模糊、折射与鼠标受光；关闭后使用普通圆角卡片",
                    &mut self.settings.liquid_glass,
                );

                let row = appearance_row(0, 1);
                paint_setting_card(painter, row, time);
                paint_setting_label(
                    painter,
                    row,
                    "玻璃风格",
                    "透明型提高背景透出，并使用轻微暗化层保证文字可读",
                );
                let regular = Rect::from_min_size(
                    Pos2::new(
                        row.right() - SETTING_CARD_PADDING_X - 142.0,
                        row.top() + 15.0,
                    ),
                    Vec2::new(66.0, 30.0),
                );
                let clear = Rect::from_min_size(
                    Pos2::new(
                        row.right() - SETTING_CARD_PADDING_X - 70.0,
                        row.top() + 15.0,
                    ),
                    Vec2::new(66.0, 30.0),
                );
                if settings_button(
                    ui,
                    painter,
                    regular,
                    "liquid-glass-regular",
                    LiquidGlassMode::Regular.label(),
                    self.settings.liquid_glass_mode == LiquidGlassMode::Regular,
                ) {
                    self.settings.liquid_glass_mode = LiquidGlassMode::Regular;
                }
                if settings_button(
                    ui,
                    painter,
                    clear,
                    "liquid-glass-clear",
                    LiquidGlassMode::Clear.label(),
                    self.settings.liquid_glass_mode == LiquidGlassMode::Clear,
                ) {
                    self.settings.liquid_glass_mode = LiquidGlassMode::Clear;
                }

                let row = appearance_row(1, 0);
                paint_setting_card(painter, row, time);
                paint_toggle_row(
                    ui,
                    painter,
                    row,
                    "setting-animations",
                    "动态界面",
                    "启用环境光斑、玻璃高光和场景动画",
                    &mut self.settings.animations,
                );

                let row = appearance_row(1, 1);
                paint_setting_card(painter, row, time);
                paint_toggle_row(
                    ui,
                    painter,
                    row,
                    "setting-tooltips",
                    "导航提示",
                    "悬停左侧导航时显示功能名称",
                    &mut self.settings.tooltips,
                );

                let row = appearance_row(2, 0);
                paint_setting_card(painter, row, time);
                paint_effect_slider(
                    ui,
                    painter,
                    row,
                    "setting-glass-transparency",
                    "卡片透明度",
                    "数值越高，背景越清晰",
                    &mut self.settings.glass_transparency,
                    20,
                    100,
                    5,
                    "%",
                );

                let row = appearance_row(2, 1);
                paint_setting_card(painter, row, time);
                paint_effect_slider(
                    ui,
                    painter,
                    row,
                    "setting-pointer-brightness",
                    "鼠标光亮度",
                    "0% 可完全关闭鼠标光晕",
                    &mut self.settings.pointer_brightness,
                    0,
                    200,
                    10,
                    "%",
                );

                let row = appearance_row(3, 0);
                paint_setting_card(painter, row, time);
                paint_effect_slider(
                    ui,
                    painter,
                    row,
                    "setting-pointer-radius",
                    "光效范围",
                    "同时影响光晕和玻璃受光范围",
                    &mut self.settings.pointer_radius,
                    40,
                    320,
                    10,
                    " px",
                );

                let row = appearance_row(3, 1);
                paint_setting_card(painter, row, time);
                paint_effect_slider(
                    ui,
                    painter,
                    row,
                    "setting-pointer-refresh",
                    "光效刷新率",
                    "快速移动鼠标时的最高重绘频率",
                    &mut self.settings.pointer_refresh_rate,
                    20,
                    144,
                    4,
                    " FPS",
                );
            }
            SettingsSection::About => {
                let card = setting_row(row_y, 258.0);
                paint_setting_card(painter, card, time);
                let icon_center = Pos2::new(card.left() + 70.0, card.top() + 76.0);
                theme::soft_orb(
                    painter,
                    icon_center + Vec2::new(0.0, 8.0),
                    64.0,
                    theme::GREEN,
                    13,
                );
                let about_icon = Rect::from_center_size(icon_center, Vec2::splat(92.0));
                if let Some(texture) = self.release_icon_texture.as_ref() {
                    paint_rounded_image(
                        painter,
                        about_icon,
                        theme::RADIUS as f32,
                        texture,
                        texture.size(),
                    );
                } else {
                    paint_block(
                        painter,
                        icon_center,
                        72.0,
                        Color32::from_rgb(128, 216, 110),
                        Color32::from_rgb(89, 66, 48),
                        Color32::from_rgb(71, 150, 72),
                    );
                }
                let info_left = card.left() + 140.0;
                theme::bold_text(
                    painter,
                    Pos2::new(info_left, card.top() + 34.0),
                    brand::NAME,
                    25.0,
                    theme::INK,
                );
                theme::text(
                    painter,
                    Pos2::new(info_left + 1.0, card.top() + 68.0),
                    "方块启动器 · Version 0.1.0 · 原生桌面",
                    9.4,
                    theme::MUTED,
                );
                theme::text(
                    painter,
                    Pos2::new(info_left + 1.0, card.top() + 91.0),
                    "原生桌面启动器 · 离线游戏启动与本地目录管理",
                    9.0,
                    theme::INK_SOFT,
                );
                painter.line_segment(
                    [
                        Pos2::new(info_left, card.top() + 119.0),
                        Pos2::new(card.right() - 24.0, card.top() + 119.0),
                    ],
                    Stroke::new(1.0, theme::LINE),
                );
                theme::text(
                    painter,
                    Pos2::new(info_left, card.top() + 129.0),
                    "支持的功能",
                    8.2,
                    theme::MUTED,
                );
                let detail = [
                    "官方版本下载与文件补全",
                    "Fabric / Forge / NeoForge",
                    "模组、光影、材质与数据包",
                    "整合包导入与版本隔离",
                    "3D 皮肤编辑与 RGB 调色",
                    "模组工程创建与 Java 选择",
                ];
                let chip_gap = 12.0;
                let chip_width = (card.right() - info_left - 24.0 - chip_gap).max(160.0) * 0.5;
                for (index, label) in detail.into_iter().enumerate() {
                    let column = index % 2;
                    let row = index / 2;
                    let chip = Rect::from_min_size(
                        Pos2::new(
                            info_left + column as f32 * (chip_width + chip_gap),
                            card.top() + 151.0 + row as f32 * 31.0,
                        ),
                        Vec2::new(chip_width, 25.0),
                    );
                    theme::fill_round(painter, chip, theme::RADIUS, theme::alpha(theme::GREEN, 18));
                    theme::stroke_round(
                        painter,
                        chip,
                        theme::RADIUS,
                        1.0,
                        theme::alpha(theme::GREEN, 34),
                    );
                    theme::center_text(
                        painter,
                        chip.center(),
                        fit_text_to_width(painter, label, theme::font(7.4), chip.width() - 18.0),
                        7.4,
                        theme::INK_SOFT,
                        false,
                    );
                }
            }
        }

        if view.draw_footer {
            let footer_y = panel.bottom() - 56.0;
            theme::text(
                painter,
                Pos2::new(panel.left() + 26.0, footer_y + 9.0),
                if self.settings == self.saved_settings {
                    "修改会自动保存到本机"
                } else {
                    "正在自动保存…"
                },
                8.0,
                theme::MUTED,
            );
            let reset = Rect::from_min_size(
                Pos2::new(panel.right() - 114.0, footer_y),
                Vec2::new(96.0, 38.0),
            );
            if settings_button(ui, painter, reset, "settings-reset", "恢复默认", false) {
                self.settings = LauncherSettings::default();
                self.settings.game_directories = crate::game::discover_game_directories();
                self.settings.game_directory = self.settings.game_directories.first().cloned();
                self.versions = available_versions(self.settings.game_directory.as_deref());
                self.refresh_recent_sessions();
                self.selected_version = self
                    .versions
                    .iter()
                    .position(|version| version.launchable)
                    .unwrap_or(0);
                self.version_scroll = 0.0;
                self.show_toast("设置已恢复为默认值");
            }
        }
    }

    fn paint_menus(&mut self, ctx: &egui::Context, layout: &Layout, time: f64) {
        let show_version = self.menu == Menu::Version || self.version_popup_value > 0.002;
        if !show_version && self.menu != Menu::Profile {
            return;
        }
        let (menu_rect, is_version) = if show_version {
            (
                version_menu_rect(layout, self.selectable_version_order().len()),
                true,
            )
        } else {
            (profile_menu_rect(layout), false)
        };
        let spring_value = if is_version {
            self.version_popup_value
        } else {
            1.0
        };
        let opacity = popup_opacity(spring_value, self.menu == Menu::Version);
        egui::Area::new(Id::unique("menu-foreground-layer"))
            .order(egui::Order::Foreground)
            .fixed_pos(menu_rect.min)
            .movable(false)
            .interactable(true)
            .show(ctx, |ui| {
                ui.set_min_size(menu_rect.size());
                ui.set_opacity(opacity);
                let painter = ui.painter().clone();
                if is_version {
                    self.paint_version_menu(ui, &painter, layout, time);
                    let scale = 0.82 + 0.18 * spring_value.clamp(-0.05, 1.12);
                    let anchor = menu_rect.right_top().to_vec2();
                    let transform = egui::emath::TSTransform::new(anchor - anchor * scale, scale);
                    ctx.transform_layer_shapes(ui.layer_id(), transform);
                } else {
                    self.paint_profile_menu(ctx, ui, &painter, layout, time);
                }
            });
        if is_version && (self.menu == Menu::Version || self.version_popup_value > 0.002) {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn paint_version_menu(
        &mut self,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
    ) {
        let versions: Vec<(usize, GameVersion)> = self
            .selectable_version_order()
            .into_iter()
            .map(|index| (index, self.versions[index].clone()))
            .collect();
        let rect = version_menu_rect(layout, versions.len());
        theme::glass_panel(
            painter,
            rect,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(22, 31, 25), theme::GREEN, time, 6.4)
                .backdrop_blur(14.0),
        );
        let content = card_clip(painter, rect);
        let painter = &content;
        theme::text(
            painter,
            Pos2::new(rect.left() + 14.0, rect.top() + 11.0),
            "选择游戏版本 · 最近游玩优先",
            9.4,
            theme::INK_SOFT,
        );
        theme::text(
            painter,
            Pos2::new(rect.right() - 104.0, rect.top() + 11.0),
            format!("{} 个可启动版本", versions.len()),
            7.3,
            theme::MUTED,
        );
        let list_top = rect.top() + 38.0;
        let list_bottom = rect.bottom() - 7.0;
        let visible_height = (list_bottom - list_top).max(1.0);
        let content_height = versions.len() as f32 * VERSION_ROW_HEIGHT;
        let max_scroll = (content_height - visible_height).max(0.0);
        if ui.ctx().input(|input| {
            input
                .pointer
                .hover_pos()
                .is_some_and(|pointer| rect.contains(pointer))
                && input.smooth_scroll_delta.y.abs() > 0.0
        }) {
            self.version_scroll -= ui.ctx().input(|input| input.smooth_scroll_delta.y) * 1.05;
        }
        self.version_scroll = self.version_scroll.clamp(0.0, max_scroll);
        let list_painter = painter.clone();
        for (index, (version_index, version)) in versions.iter().enumerate() {
            let item_y = list_top + index as f32 * VERSION_ROW_HEIGHT - self.version_scroll;
            if item_y + 47.0 < list_top || item_y > list_bottom {
                continue;
            }
            let item = Rect::from_min_size(
                Pos2::new(rect.left() + 7.0, item_y),
                Vec2::new(rect.width() - 22.0, 47.0),
            );
            let response = ui
                .interact(item, Id::unique(("version-option", index)), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand);
            let selected = self.selected_version == *version_index;
            let item_card = item.shrink2(Vec2::new(2.0, 1.5));
            theme::glass_panel(
                &list_painter,
                item_card,
                theme::RADIUS,
                theme::GlassSpec::new(
                    if selected || response.hovered() {
                        Color32::from_rgb(29, 47, 33)
                    } else {
                        Color32::from_rgb(21, 34, 25)
                    },
                    theme::GREEN,
                    time,
                    index as f32 * 0.8,
                )
                .hovered(response.hovered()),
            );
            if selected {
                theme::stroke_round(
                    &list_painter,
                    item_card,
                    theme::RADIUS,
                    1.0,
                    theme::alpha(theme::GREEN, 68),
                );
            }
            let mark = Rect::from_center_size(
                Pos2::new(item.left() + 22.0, item.center().y),
                Vec2::splat(28.0),
            );
            if let Some(texture) = self.version_icon(version) {
                paint_rounded_image(&list_painter, mark, 8.0, texture, texture.size());
            } else {
                theme::fill_round(
                    &list_painter,
                    mark,
                    theme::RADIUS,
                    Color32::from_rgb(53, 66, 57),
                );
            }
            let selector_label = self.version_selector_label(version);
            let selector_label = fit_text_to_width(
                &list_painter,
                &selector_label,
                theme::font(10.2),
                (item.width() - 70.0).max(80.0),
            );
            let selector_detail = self.version_selector_detail(version);
            theme::text(
                &list_painter,
                Pos2::new(item.left() + 42.0, item.top() + 8.0),
                selector_label,
                10.2,
                if version.launchable {
                    theme::INK_SOFT
                } else {
                    theme::MUTED
                },
            );
            theme::text(
                &list_painter,
                Pos2::new(item.left() + 42.0, item.top() + 25.0),
                ellipsize_end(&selector_detail, 38),
                7.4,
                theme::MUTED,
            );
            if selected {
                theme::text(
                    &list_painter,
                    Pos2::new(item.right() - 20.0, item.top() + 16.0),
                    "✓",
                    10.0,
                    theme::GREEN,
                );
            } else if !version.launchable {
                theme::text(
                    &list_painter,
                    Pos2::new(item.right() - 57.0, item.top() + 16.0),
                    "需补全",
                    7.4,
                    Color32::from_rgb(164, 148, 118),
                );
            }
            if response.clicked() {
                self.selected_version = *version_index;
                self.switch_version_detail(*version_index);
                self.menu = Menu::None;
                self.launch_complete = false;
                self.show_toast(if version.launchable {
                    format!("已切换至 {} · {}", version.version, version.loader)
                } else {
                    format!("已选择 {}，启动时将自动补全文件", version.version)
                });
            }
        }
        if versions.is_empty() {
            theme::center_text(
                &list_painter,
                Pos2::new(rect.center().x, list_top + 58.0),
                "没有可启动的完整版本",
                8.8,
                theme::MUTED,
                false,
            );
        }
        if max_scroll > 0.0 {
            let track = Rect::from_min_size(
                Pos2::new(rect.right() - 6.0, list_top + 3.0),
                Vec2::new(3.0, visible_height - 6.0),
            );
            theme::fill_round(painter, track, theme::RADIUS, theme::white_alpha(16));
            let thumb_height = (visible_height * visible_height / content_height).max(32.0);
            let progress = self.version_scroll / max_scroll;
            let thumb = Rect::from_min_size(
                Pos2::new(
                    track.left(),
                    track.top() + (track.height() - thumb_height) * progress,
                ),
                Vec2::new(track.width(), thumb_height),
            );
            theme::fill_round(
                painter,
                thumb,
                theme::RADIUS,
                theme::alpha(theme::GREEN, 105),
            );
        }
    }

    fn paint_profile_menu(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        layout: &Layout,
        time: f64,
    ) {
        let rect = profile_menu_rect(layout);
        theme::glass_panel(
            painter,
            rect,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(22, 31, 25), theme::GREEN, time, 7.2)
                .backdrop_blur(14.0),
        );
        let content = card_clip(painter, rect);
        let painter = &content;
        paint_avatar(
            painter,
            Rect::from_min_size(
                Pos2::new(rect.left() + 14.0, rect.top() + 13.0),
                Vec2::splat(48.0),
            ),
            theme::RADIUS,
        );
        theme::text(
            painter,
            Pos2::new(rect.left() + 74.0, rect.top() + 16.0),
            "当前玩家",
            7.3,
            Color32::from_rgb(119, 171, 116),
        );
        theme::bold_text(
            painter,
            Pos2::new(rect.left() + 74.0, rect.top() + 27.0),
            "Steve",
            12.3,
            theme::INK_SOFT,
        );
        theme::text(
            painter,
            Pos2::new(rect.left() + 74.0, rect.top() + 44.0),
            "player@blockworld.cn",
            7.1,
            theme::MUTED,
        );
        painter.line_segment(
            [
                Pos2::new(rect.left() + 12.0, rect.top() + 72.0),
                Pos2::new(rect.right() - 12.0, rect.top() + 72.0),
            ],
            Stroke::new(1.0, theme::LINE),
        );

        let rows = [
            (NavIcon::User, "管理账户", "已登录"),
            (NavIcon::Folder, "打开游戏目录", ""),
            (NavIcon::Logout, "切换账户", ""),
        ];
        for (index, (icon, label, suffix)) in rows.into_iter().enumerate() {
            let row = Rect::from_min_size(
                Pos2::new(rect.left() + 7.0, rect.top() + 80.0 + index as f32 * 42.0),
                Vec2::new(rect.width() - 14.0, 38.0),
            );
            let response = ui
                .interact(row, Id::unique(("profile-row", index)), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand);
            if response.hovered() {
                theme::fill_round(painter, row, theme::RADIUS, theme::white_alpha(10));
            }
            draw_icon(
                painter,
                icon,
                Rect::from_center_size(
                    Pos2::new(row.left() + 14.0, row.center().y),
                    Vec2::splat(15.0),
                ),
                theme::MUTED,
            );
            theme::text(
                painter,
                Pos2::new(row.left() + 28.0, row.top() + 10.0),
                label,
                8.8,
                theme::INK_SOFT,
            );
            if !suffix.is_empty() {
                theme::text(
                    painter,
                    Pos2::new(row.right() - 43.0, row.top() + 10.0),
                    suffix,
                    7.2,
                    theme::MUTED,
                );
            }
            if response.clicked() {
                self.menu = Menu::None;
                match index {
                    0 => self.show_toast("Microsoft 账户管理将在下一阶段接入"),
                    1 => self.open_game_folder(ctx),
                    _ => self.show_toast("账户切换将在下一阶段接入"),
                }
            }
        }
    }

    fn open_game_folder(&mut self, _ctx: &egui::Context) {
        let folder = self
            .settings
            .game_directory
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        #[cfg(target_os = "windows")]
        {
            let _ = std::process::Command::new("explorer").arg(folder).spawn();
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = std::process::Command::new("xdg-open").arg(folder).spawn();
        }
        self.show_toast("已打开启动器所在目录");
    }

    fn paint_news_dialog(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        painter: &Painter,
        screen: Rect,
        time: f64,
    ) {
        let Some(news_index) = self.news else {
            return;
        };
        let Some(story) = self.news_items.get(news_index).cloned() else {
            self.news = None;
            return;
        };
        self.ensure_news_textures(ctx);
        let image = self.news_textures.get(news_index).cloned().flatten();
        let dialog_size = Vec2::new(
            540.0_f32.min(screen.width() - 28.0),
            390.0_f32.min(screen.height() - 28.0),
        );
        let dialog = Rect::from_center_size(screen.center(), dialog_size);
        theme::glass_panel(
            painter,
            dialog,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(22, 32, 25), theme::GREEN, time, 8.6)
                .backdrop_blur(18.0),
        );
        let content = card_clip(painter, dialog);
        let painter = &content;
        let art = Rect::from_min_size(
            dialog.left_top() + Vec2::new(7.0, 7.0),
            Vec2::new(dialog.width() - 14.0, 116.0),
        );
        if let Some(image) = image {
            paint_rounded_image(painter, art, 12.0, &image, image.size());
        } else {
            scene::paint_news_art(
                painter,
                art,
                match news_index {
                    0 => scene::NewsArt::Version,
                    1 => scene::NewsArt::Mod,
                    _ => scene::NewsArt::Community,
                },
                time,
            );
        }
        theme::stroke_round(painter, art, 12, 1.0, theme::white_alpha(30));

        let close = Rect::from_min_size(
            Pos2::new(dialog.right() - 44.0, dialog.top() + 14.0),
            Vec2::splat(30.0),
        );
        let close_response = ui
            .interact(close, Id::unique("dialog-close"), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        theme::fill_round(
            painter,
            close,
            theme::RADIUS,
            if close_response.hovered() {
                theme::white_alpha(24)
            } else {
                theme::black_alpha(76)
            },
        );
        draw_icon(
            painter,
            NavIcon::Close,
            Rect::from_center_size(close.center(), Vec2::splat(14.0)),
            theme::INK_SOFT,
        );
        if close_response.clicked() {
            self.news = None;
        }

        let body_y = art.bottom() + 22.0;
        theme::text(
            painter,
            Pos2::new(dialog.left() + 25.0, body_y),
            ellipsize_end(
                &if story.date.is_empty() {
                    story.category.clone()
                } else {
                    format!("{} · {}", story.category, story.date)
                },
                46,
            ),
            7.8,
            Color32::from_rgb(126, 176, 121),
        );
        theme::bold_text(
            painter,
            Pos2::new(dialog.left() + 25.0, body_y + 18.0),
            ellipsize_end(&story.title, 34),
            21.0,
            theme::INK,
        );
        theme::wrapped_text(
            painter,
            Pos2::new(dialog.left() + 25.0, body_y + 54.0),
            ellipsize_end(
                if story.body.is_empty() {
                    &story.summary
                } else {
                    &story.body
                },
                320,
            ),
            9.4,
            theme::INK_SOFT,
            dialog.width() - 50.0,
        );

        let action = Rect::from_min_size(
            Pos2::new(dialog.left() + 25.0, dialog.bottom() - 57.0),
            Vec2::new(112.0, 36.0),
        );
        let action_response = ui
            .interact(action, Id::unique("dialog-action"), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        theme::fill_round(
            painter,
            action,
            theme::RADIUS,
            if action_response.hovered() {
                theme::GREEN_BRIGHT
            } else {
                theme::GREEN
            },
        );
        theme::center_text(
            painter,
            action.center(),
            "前往查看  →",
            8.8,
            Color32::from_rgb(15, 48, 19),
            true,
        );
        if action_response.clicked() {
            let url = story
                .article_url
                .as_deref()
                .unwrap_or("https://www.minecraft.net/article");
            match open_external_url(url) {
                Ok(()) => self.news = None,
                Err(error) => self.show_toast(format!("无法打开官方详情：{error}")),
            }
        }

        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.news = None;
        }
    }

    fn paint_toast(&self, painter: &Painter, screen: Rect) {
        let Some(toast) = &self.toast else {
            return;
        };
        let width = (toast.message.chars().count() as f32 * 8.1 + 70.0)
            .clamp(220.0, 390.0)
            .min(screen.width() - 24.0);
        let rect = Rect::from_min_size(
            Pos2::new(screen.right() - width - 24.0, screen.bottom() - 70.0),
            Vec2::new(width, 47.0),
        );
        theme::glass_panel(
            painter,
            rect,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(27, 39, 30), theme::GREEN, 0.0, 9.3),
        );
        let content = card_clip(painter, rect);
        let painter = &content;
        painter.circle_filled(
            Pos2::new(rect.left() + 23.0, rect.center().y),
            13.0,
            theme::alpha(theme::GREEN, 34),
        );
        theme::center_text(
            painter,
            Pos2::new(rect.left() + 23.0, rect.center().y),
            "✓",
            10.5,
            theme::GREEN,
            true,
        );
        theme::text(
            painter,
            Pos2::new(rect.left() + 44.0, rect.top() + 14.0),
            &toast.message,
            8.5,
            theme::INK_SOFT,
        );
    }

    fn paint_official_browser(
        &mut self,
        ui: &mut egui::Ui,
        painter: &Painter,
        screen: Rect,
        time: f64,
    ) {
        let dialog_size = Vec2::new(
            650.0_f32.min(screen.width() - 30.0),
            600.0_f32.min(screen.height() - 36.0),
        );
        let dialog = Rect::from_center_size(screen.center(), dialog_size);
        theme::glass_panel(
            painter,
            dialog,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(22, 33, 26), theme::GREEN, time, 20.0)
                .backdrop_blur(18.0),
        );
        let content = card_clip(painter, dialog);
        let painter = &content;
        theme::bold_text(
            painter,
            Pos2::new(dialog.left() + 24.0, dialog.top() + 20.0),
            "官方版本",
            21.0,
            theme::INK,
        );
        theme::text(
            painter,
            Pos2::new(dialog.left() + 24.0, dialog.top() + 48.0),
            if let Some(error) = &self.official_error {
                error.clone()
            } else if self.official_manifest.is_some() {
                "正在连接 Mojang Piston Meta…".to_owned()
            } else {
                format!(
                    "Piston Meta · {} 个可下载版本",
                    self.official_versions.len()
                )
            },
            8.4,
            theme::MUTED,
        );
        let close = Rect::from_min_size(
            Pos2::new(dialog.right() - 46.0, dialog.top() + 16.0),
            Vec2::splat(30.0),
        );
        let close_response = ui
            .interact(close, Id::unique("official-close"), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        theme::fill_round(
            painter,
            close,
            theme::RADIUS,
            if close_response.hovered() {
                theme::white_alpha(22)
            } else {
                theme::black_alpha(72)
            },
        );
        draw_icon(
            painter,
            NavIcon::Close,
            Rect::from_center_size(close.center(), Vec2::splat(14.0)),
            theme::INK_SOFT,
        );
        if close_response.clicked() {
            self.official_browser_open = false;
            return;
        }

        let filters = [
            (VersionFilter::All, "全部"),
            (VersionFilter::Release, "正式版"),
            (VersionFilter::Snapshot, "快照"),
        ];
        let mut filter_x = dialog.left() + 24.0;
        for (index, (filter, label)) in filters.into_iter().enumerate() {
            let button = Rect::from_min_size(
                Pos2::new(filter_x, dialog.top() + 76.0),
                Vec2::new(66.0, 27.0),
            );
            let response = ui
                .interact(
                    button,
                    Id::unique(("official-filter", index)),
                    Sense::click(),
                )
                .on_hover_cursor(CursorIcon::PointingHand);
            let selected = self.official_filter == filter;
            theme::fill_round(
                painter,
                button,
                theme::RADIUS,
                if selected {
                    theme::alpha(theme::GREEN, 28)
                } else if response.hovered() {
                    theme::white_alpha(14)
                } else {
                    theme::white_alpha(7)
                },
            );
            theme::center_text(
                painter,
                button.center(),
                label,
                8.4,
                if selected { theme::INK } else { theme::MUTED },
                selected,
            );
            if response.clicked() {
                self.official_filter = filter;
                self.official_scroll = 0.0;
            }
            filter_x = button.right() + 7.0;
        }

        let filtered: Vec<(usize, crate::download::OfficialVersion)> = self
            .official_versions
            .iter()
            .enumerate()
            .filter(|(_, version)| match self.official_filter {
                VersionFilter::All => true,
                VersionFilter::Release => version.kind == "release",
                VersionFilter::Snapshot => version.kind == "snapshot",
            })
            .map(|(index, version)| (index, version.clone()))
            .collect();
        if !filtered.is_empty()
            && !filtered
                .iter()
                .any(|(index, _)| *index == self.official_selected)
        {
            self.official_selected = filtered[0].0;
        }
        let list_top = dialog.top() + 116.0;
        let list_bottom = dialog.bottom() - 70.0;
        let list_height = (list_bottom - list_top).max(1.0);
        let row_height = 49.0;
        let content_height = filtered.len() as f32 * row_height;
        let max_scroll = (content_height - list_height).max(0.0);
        if ui.ctx().input(|input| {
            input
                .pointer
                .hover_pos()
                .is_some_and(|pointer| dialog.contains(pointer))
                && input.smooth_scroll_delta.y.abs() > 0.0
        }) {
            self.official_scroll -= ui.ctx().input(|input| input.smooth_scroll_delta.y) * 1.05;
        }
        self.official_scroll = self.official_scroll.clamp(0.0, max_scroll);
        let list_painter = painter.clone();
        for (visible_index, (global_index, version)) in filtered.iter().enumerate() {
            let y = list_top + visible_index as f32 * row_height - self.official_scroll;
            if y + row_height < list_top || y > list_bottom {
                continue;
            }
            let row = Rect::from_min_size(
                Pos2::new(dialog.left() + 22.0, y),
                Vec2::new(dialog.width() - 44.0, 43.0),
            );
            let response = ui
                .interact(
                    row,
                    Id::unique(("official-version", global_index)),
                    Sense::click(),
                )
                .on_hover_cursor(CursorIcon::PointingHand);
            let selected = self.official_selected == *global_index;
            let card = row.shrink2(Vec2::new(0.0, 1.5));
            theme::glass_panel(
                &list_painter,
                card,
                theme::RADIUS,
                theme::GlassSpec::new(
                    if selected || response.hovered() {
                        Color32::from_rgb(29, 48, 33)
                    } else {
                        Color32::from_rgb(21, 34, 25)
                    },
                    theme::GREEN,
                    time,
                    row.top() * 0.01,
                )
                .hovered(response.hovered()),
            );
            if selected {
                theme::stroke_round(
                    &list_painter,
                    card,
                    theme::RADIUS,
                    1.0,
                    theme::alpha(theme::GREEN, 72),
                );
            }
            let installed = self
                .versions
                .iter()
                .find(|installed| installed.version == version.id);
            let status = match installed {
                Some(installed) if installed.launchable => "已安装",
                Some(_) => "需补全",
                None => "可安装",
            };
            theme::bold_text(
                &list_painter,
                Pos2::new(row.left() + 14.0, row.top() + 7.0),
                &version.id,
                10.4,
                theme::INK_SOFT,
            );
            theme::text(
                &list_painter,
                Pos2::new(row.left() + 14.0, row.top() + 25.0),
                match version.kind.as_str() {
                    "release" => "正式版",
                    "snapshot" => "快照",
                    "old_beta" => "旧测试版",
                    "old_alpha" => "旧 Alpha",
                    _ => &version.kind,
                },
                7.7,
                theme::MUTED,
            );
            theme::text(
                &list_painter,
                Pos2::new(row.right() - 67.0, row.top() + 14.0),
                status,
                7.8,
                if status == "已安装" {
                    theme::GREEN_BRIGHT
                } else {
                    Color32::from_rgb(201, 171, 105)
                },
            );
            if response.clicked() {
                self.official_selected = *global_index;
            }
        }
        if filtered.is_empty() {
            theme::center_text(
                &list_painter,
                Pos2::new(dialog.center().x, list_top + 56.0),
                if self.official_manifest.is_some() {
                    "正在获取官方版本…"
                } else if let Some(error) = &self.official_error {
                    error
                } else {
                    "当前分类没有官方版本"
                },
                9.0,
                theme::MUTED,
                false,
            );
        }
        if max_scroll > 0.0 {
            let track = Rect::from_min_size(
                Pos2::new(dialog.right() - 11.0, list_top + 3.0),
                Vec2::new(3.0, list_height - 6.0),
            );
            theme::fill_round(painter, track, theme::RADIUS, theme::white_alpha(16));
            let thumb_height = (list_height * list_height / content_height).max(32.0);
            let progress = self.official_scroll / max_scroll;
            let thumb = Rect::from_min_size(
                Pos2::new(
                    track.left(),
                    track.top() + (track.height() - thumb_height) * progress,
                ),
                Vec2::new(track.width(), thumb_height),
            );
            theme::fill_round(
                painter,
                thumb,
                theme::RADIUS,
                theme::alpha(theme::GREEN, 105),
            );
        }

        let selected_version = self.official_versions.get(self.official_selected).cloned();
        theme::text(
            painter,
            Pos2::new(dialog.left() + 24.0, dialog.bottom() - 48.0),
            selected_version
                .as_ref()
                .map(|version| format!("将安装 Minecraft {}", version.id))
                .unwrap_or_else(|| "请选择要安装的版本".to_owned()),
            8.5,
            theme::MUTED,
        );
        let install = Rect::from_min_size(
            Pos2::new(dialog.right() - 140.0, dialog.bottom() - 52.0),
            Vec2::new(116.0, 36.0),
        );
        if let Some(version) = selected_version
            && settings_button(
                ui,
                painter,
                install,
                "official-install",
                "下载并安装",
                false,
            )
        {
            self.official_browser_open = false;
            self.open_preinstall_loader_dialog(version);
        }
    }

    fn paint_install_progress(&self, painter: &Painter, screen: Rect, time: f64) {
        let Some(progress) = &self.install_progress else {
            return;
        };
        let width = 390.0_f32.min(screen.width() - 36.0);
        let rect = Rect::from_min_size(
            Pos2::new(screen.right() - width - 24.0, screen.bottom() - 178.0),
            Vec2::new(width, 108.0),
        );
        theme::glass_panel(
            painter,
            rect,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(22, 34, 26), theme::GREEN, time, 19.0),
        );
        let content = card_clip(painter, rect);
        let painter = &content;
        theme::soft_orb(
            painter,
            Pos2::new(
                rect.left() + 24.0 + ((time * 0.08) % 1.0) as f32 * (rect.width() - 48.0),
                rect.top() + 30.0,
            ),
            70.0,
            theme::GREEN,
            13,
        );
        theme::bold_text(
            painter,
            Pos2::new(rect.left() + 18.0, rect.top() + 15.0),
            &progress.stage,
            10.5,
            theme::INK,
        );
        let version = self.install_version_id.as_deref().unwrap_or("Minecraft");
        let file_status = if progress.files_total == 0 {
            "正在统计文件".to_owned()
        } else {
            format!(
                "剩余 {} 个文件 · {:.1} MB/s",
                progress.files_total.saturating_sub(progress.files_done),
                progress.bytes_per_second as f64 / 1_048_576.0
            )
        };
        theme::text(
            painter,
            Pos2::new(rect.left() + 18.0, rect.top() + 37.0),
            format!("{version} · {file_status}"),
            8.2,
            theme::MUTED,
        );
        let progress_ratio = if progress.total_bytes > 0 {
            (progress.completed_bytes as f32 / progress.total_bytes as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };
        theme::text(
            painter,
            Pos2::new(rect.right() - 58.0, rect.top() + 15.0),
            format!("{}%", (progress_ratio * 100.0).round() as u32),
            9.0,
            theme::GREEN_BRIGHT,
        );
        let track = Rect::from_min_size(
            Pos2::new(rect.left() + 18.0, rect.top() + 64.0),
            Vec2::new(rect.width() - 36.0, 9.0),
        );
        theme::fill_round(painter, track, theme::RADIUS, theme::white_alpha(17));
        if progress_ratio > 0.0 {
            theme::fill_round(
                painter,
                Rect::from_min_size(
                    track.left_top(),
                    Vec2::new(track.width() * progress_ratio, track.height()),
                ),
                theme::RADIUS,
                theme::GREEN,
            );
        }
        theme::text(
            painter,
            Pos2::new(rect.left() + 18.0, rect.bottom() - 22.0),
            format!(
                "{:.1} MB / {:.1} MB",
                progress.completed_bytes as f64 / 1_048_576.0,
                progress.total_bytes as f64 / 1_048_576.0
            ),
            8.0,
            theme::MUTED,
        );
    }

    fn paint_modpack_import_progress(&self, painter: &Painter, screen: Rect, time: f64) {
        if self.modpack_import_receiver.is_none() {
            return;
        }
        let Some(update) = self.modpack_import_update.as_ref() else {
            return;
        };
        let width = 390.0_f32.min(screen.width() - 36.0);
        let rect = Rect::from_min_size(
            Pos2::new(screen.right() - width - 24.0, screen.bottom() - 178.0),
            Vec2::new(width, 108.0),
        );
        theme::glass_panel(
            painter,
            rect,
            theme::RADIUS,
            theme::GlassSpec::new(Color32::from_rgb(22, 34, 26), theme::GREEN, time, 19.5)
                .backdrop_blur(18.0),
        );
        let content = card_clip(painter, rect);
        let painter = &content;
        theme::bold_text(
            painter,
            Pos2::new(rect.left() + 18.0, rect.top() + 15.0),
            &update.stage,
            10.5,
            theme::INK,
        );
        theme::text(
            painter,
            Pos2::new(rect.left() + 18.0, rect.top() + 39.0),
            fit_text_to_width(
                painter,
                &update.message,
                theme::font(8.0),
                rect.width() - 36.0,
            ),
            8.0,
            theme::MUTED,
        );
        let track = Rect::from_min_size(
            Pos2::new(rect.left() + 18.0, rect.top() + 72.0),
            Vec2::new(rect.width() - 36.0, 9.0),
        );
        theme::fill_round(painter, track, theme::RADIUS, theme::white_alpha(17));
        let pulse = (0.18 + (time * 0.28).sin().abs() as f32 * 0.42).clamp(0.0, 0.72);
        let run_width = track.width() * pulse;
        let run_left =
            track.left() + (track.width() - run_width) * (0.5 + (time * 0.37).sin() as f32 * 0.5);
        theme::fill_round(
            painter,
            Rect::from_min_size(
                Pos2::new(run_left, track.top()),
                Vec2::new(run_width, track.height()),
            ),
            theme::RADIUS,
            theme::GREEN,
        );
    }

    fn handle_outside_click(&mut self, ctx: &egui::Context, layout: &Layout) {
        if !ctx.input(|input| input.pointer.primary_pressed()) {
            return;
        }
        if self.news.is_some() {
            return;
        }
        let Some(pointer) = ctx.input(|input| input.pointer.interact_pos()) else {
            return;
        };
        if self.favorite_editor_open && !favorite_editor_rect(layout.screen).contains(pointer) {
            self.cancel_favorite_editor();
        }
        if self.java_picker_target.is_some() {
            let menu = java_picker_rect(
                self.java_picker_anchor,
                layout.screen,
                self.java_runtimes.len() + 2,
            );
            if !menu.contains(pointer) && !self.java_picker_anchor.contains(pointer) {
                self.java_picker_target = None;
            }
        }
        if self.version_export_open && !version_export_dialog_rect(layout.screen).contains(pointer)
        {
            self.version_export_open = false;
        }
        if self.version_content_dialog.is_some() && self.version_content_dialog_open {
            let dialog = version_content_dialog_rect(layout.screen);
            if version_content_close_rect(dialog).contains(pointer) {
                self.close_version_content_dialog();
                return;
            }
            if !dialog.contains(pointer) {
                self.close_version_content_dialog();
            }
        }
        if self.resource_version_menu_open {
            let menu = resource_version_menu_rect(self.resource_version_anchor, layout.screen);
            if !menu.contains(pointer) && !self.resource_version_anchor.contains(pointer) {
                self.resource_version_menu_open = false;
            }
        }
        if self.launch_settings_open
            && !launch_settings_card_rect(layout).contains(pointer)
            && !launch_more_rect(layout).contains(pointer)
        {
            self.launch_settings_open = false;
        }
        if self.version_folder_menu_open {
            let menu = version_folder_menu_rect(layout, self.settings.game_directories.len());
            let selector = version_folder_selector_rect(layout);
            if !menu.contains(pointer) && !selector.contains(pointer) {
                self.version_folder_menu_open = false;
            }
        }
        let version_menu = version_menu_rect(layout, self.selectable_version_order().len());
        let profile_menu = profile_menu_rect(layout);
        if self.menu == Menu::Version
            && !version_menu.contains(pointer)
            && !layout.version_trigger.contains(pointer)
        {
            self.menu = Menu::None;
        }
        if self.menu == Menu::Profile
            && !profile_menu.contains(pointer)
            && !layout.rail_profile.contains(pointer)
        {
            self.menu = Menu::None;
        }
    }
}

impl App for LauncherApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // 圆角窗口的四个角靠 DWM 抠出来，底色不能是 egui 默认的深黑，
        // 否则标题栏压不到的那一圈边就算画了底色，圆角外仍会透出一圈黑边。
        let [r, g, b, a] = theme::BG.to_normalized_gamma_f32();
        [r, g, b, a]
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut Frame) {
        let ctx = ui.ctx().clone();
        if self.report_dpi && !self.dpi_reported {
            self.dpi_reported = true;
            eprintln!(
                "DPI report: vector_dpi={}, vector_surface={:?}, ppp={}, native_ppp={:?}, zoom={}, screen_rect={:?}",
                vector::DESIGN_DPI,
                vector::DESIGN_SURFACE_SIZE,
                ctx.pixels_per_point(),
                ctx.native_pixels_per_point(),
                ctx.zoom_factor(),
                ui.max_rect()
            );
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        self.update_timers(&ctx);
        self.handle_shortcuts(&ctx);
        let screen = ui.max_rect();
        if !self.window_rounded {
            self.window_rounded = crate::apply_rounded_window();
        }
        ui.set_min_size(screen.size());
        self.render(&ctx, ui, screen);
        self.persist_settings();
        self.handle_screenshot(&ctx);
    }
}

fn paint_ambient_background(painter: &Painter, screen: Rect, time: f64) {
    theme::vertical_gradient(
        painter,
        screen,
        Color32::from_rgb(11, 17, 13),
        Color32::from_rgb(7, 12, 9),
    );
    let drift = ((time * 0.025) as f32).sin();
    theme::soft_orb(
        painter,
        Pos2::new(
            screen.right() - screen.width() * 0.10 + drift * 18.0,
            screen.top() + screen.height() * 0.12,
        ),
        screen.width() * 0.31,
        Color32::from_rgb(67, 158, 91),
        17,
    );
    theme::soft_orb(
        painter,
        Pos2::new(
            screen.left() + screen.width() * 0.10 - drift * 14.0,
            screen.bottom() - screen.height() * 0.16,
        ),
        screen.width() * 0.24,
        Color32::from_rgb(81, 118, 152),
        10,
    );
    theme::soft_orb(
        painter,
        Pos2::new(screen.center().x + drift * 45.0, screen.bottom() + 40.0),
        screen.width() * 0.22,
        theme::GREEN,
        7,
    );
}

fn paint_setting_card(painter: &Painter, row: Rect, time: f64) {
    let card = row.shrink2(Vec2::new(0.0, 4.5));
    if card.height() <= 1.0 {
        return;
    }
    theme::glass_panel(
        painter,
        card,
        theme::RADIUS,
        theme::GlassSpec::new(
            Color32::from_rgb(20, 32, 24),
            theme::GREEN,
            time,
            row.top() * 0.01,
        ),
    );
}

fn paint_setting_label(painter: &Painter, row: Rect, title: &str, description: &str) {
    let content = card_clip(painter, row);
    let painter = &content;
    theme::bold_text(
        painter,
        Pos2::new(row.left() + SETTING_CARD_PADDING_X, row.top() + 18.0),
        title,
        11.0,
        theme::INK,
    );
    theme::text(
        painter,
        Pos2::new(row.left() + SETTING_CARD_PADDING_X, row.top() + 40.0),
        description,
        8.2,
        theme::MUTED,
    );
}

fn paint_toggle_row(
    ui: &mut egui::Ui,
    painter: &Painter,
    row: Rect,
    id: &'static str,
    title: &str,
    description: &str,
    value: &mut bool,
) {
    let content = card_clip(painter, row);
    let painter = &content;
    paint_setting_label(painter, row, title, description);
    let toggle = Rect::from_center_size(
        Pos2::new(
            row.right() - SETTING_CARD_PADDING_X - 24.0,
            row.top() + 32.0,
        ),
        Vec2::new(48.0, 27.0),
    );
    let response = ui
        .interact(toggle, Id::unique(id), Sense::click())
        .on_hover_cursor(CursorIcon::PointingHand);
    theme::fill_round(
        painter,
        toggle,
        theme::RADIUS,
        if *value {
            theme::GREEN
        } else {
            Color32::from_rgb(45, 58, 49)
        },
    );
    theme::stroke_round(painter, toggle, theme::RADIUS, 1.0, theme::white_alpha(30));
    let knob_x = if *value {
        toggle.right() - 13.5
    } else {
        toggle.left() + 13.5
    };
    painter.circle_filled(
        Pos2::new(knob_x, toggle.center().y),
        9.5,
        if *value {
            Color32::from_rgb(236, 255, 230)
        } else {
            Color32::from_rgb(140, 153, 144)
        },
    );
    if response.clicked() {
        *value = !*value;
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_effect_slider(
    ui: &mut egui::Ui,
    painter: &Painter,
    row: Rect,
    id: &'static str,
    title: &str,
    description: &str,
    value: &mut u16,
    minimum: u16,
    maximum: u16,
    step: u16,
    suffix: &str,
) {
    let content = card_clip(painter, row);
    let painter = &content;
    let content = row.shrink2(Vec2::new(SETTING_CARD_PADDING_X, 0.0));
    theme::bold_text(
        painter,
        Pos2::new(content.left(), row.top() + 9.0),
        title,
        9.0,
        theme::INK_SOFT,
    );
    painter.text(
        Pos2::new(content.right(), row.top() + 9.0),
        egui::Align2::RIGHT_TOP,
        format!("{value}{suffix}"),
        theme::bold_font(9.0),
        Color32::from_rgb(180, 221, 173),
    );
    if !description.is_empty() {
        theme::text(
            painter,
            Pos2::new(content.left(), row.top() + 27.0),
            description,
            7.3,
            theme::MUTED,
        );
    }
    let track = Rect::from_min_size(
        Pos2::new(content.left(), row.top() + 49.0),
        Vec2::new(content.width(), 5.0),
    );
    let hit = Rect::from_min_max(
        Pos2::new(track.left(), track.top() - 7.0),
        Pos2::new(track.right(), track.bottom() + 7.0),
    );
    let response = ui
        .interact(hit, Id::unique(id), Sense::click_and_drag())
        .on_hover_cursor(CursorIcon::PointingHand);
    if (response.clicked() || response.dragged())
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let ratio = ((pointer.x - track.left()) / track.width()).clamp(0.0, 1.0);
        let raw = minimum as f32 + ratio * (maximum - minimum) as f32;
        *value = (((raw / step as f32).round() as u16) * step).clamp(minimum, maximum);
        ui.ctx().request_repaint();
    }
    let ratio = (*value - minimum) as f32 / (maximum - minimum).max(1) as f32;
    theme::fill_round(painter, track, 4, theme::white_alpha(18));
    theme::fill_round(
        painter,
        Rect::from_min_size(
            track.left_top(),
            Vec2::new(track.width() * ratio, track.height()),
        ),
        4,
        if response.hovered() {
            theme::GREEN_BRIGHT
        } else {
            theme::GREEN
        },
    );
    painter.circle_filled(
        Pos2::new(track.left() + track.width() * ratio, track.center().y),
        6.0,
        Color32::from_rgb(238, 255, 232),
    );
}

fn skin_rgb_hex(color: [u8; 4]) -> String {
    format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2])
}

#[allow(clippy::too_many_arguments)]
fn paint_skin_rgb_slider(
    ui: &mut egui::Ui,
    painter: &Painter,
    row: Rect,
    id: &'static str,
    label: &str,
    value: &mut u8,
    accent: Color32,
    active: bool,
) {
    let content = card_clip(painter, row);
    let painter = &content;
    theme::bold_text(
        painter,
        Pos2::new(row.left(), row.top() + 7.0),
        label,
        8.8,
        accent,
    );
    let track = Rect::from_min_size(
        Pos2::new(row.left() + 27.0, row.center().y - 3.0),
        Vec2::new((row.width() - 75.0).max(80.0), 6.0),
    );
    let hit = Rect::from_min_max(
        Pos2::new(track.left(), track.top() - 10.0),
        Pos2::new(track.right(), track.bottom() + 10.0),
    );
    let mut hovered = false;
    if active {
        let response = ui
            .interact(hit, Id::unique(id), Sense::click_and_drag())
            .on_hover_cursor(CursorIcon::PointingHand);
        hovered = response.hovered();
        if (response.clicked() || response.dragged())
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let ratio = ((pointer.x - track.left()) / track.width()).clamp(0.0, 1.0);
            *value = (ratio * 255.0).round() as u8;
            ui.ctx().request_repaint();
        }
    }
    let ratio = *value as f32 / 255.0;
    theme::fill_round(painter, track, theme::RADIUS, theme::white_alpha(18));
    if ratio > 0.0 {
        theme::fill_round(
            painter,
            Rect::from_min_size(
                track.left_top(),
                Vec2::new(track.width() * ratio, track.height()),
            ),
            theme::RADIUS,
            if hovered {
                accent.gamma_multiply(1.18)
            } else {
                accent
            },
        );
    }
    painter.circle_filled(
        Pos2::new(track.left() + track.width() * ratio, track.center().y),
        6.0,
        Color32::from_rgb(239, 246, 240),
    );
    painter.text(
        Pos2::new(row.right(), row.top() + 7.0),
        egui::Align2::RIGHT_TOP,
        value.to_string(),
        theme::bold_font(8.8),
        theme::INK_SOFT,
    );
}

fn paint_status_badge(painter: &Painter, row: Rect, label: &str) {
    let content = card_clip(painter, row);
    let painter = &content;
    let content = row.shrink2(Vec2::new(SETTING_CARD_PADDING_X, 0.0));
    let width = label.chars().count() as f32 * 7.2 + 18.0;
    let badge = Rect::from_min_size(
        Pos2::new(content.right() - width, row.top() + 18.0),
        Vec2::new(width, 26.0),
    );
    theme::fill_round(
        painter,
        badge,
        theme::RADIUS,
        theme::alpha(theme::GREEN, 20),
    );
    theme::stroke_round(
        painter,
        badge,
        theme::RADIUS,
        1.0,
        theme::alpha(theme::GREEN, 44),
    );
    theme::center_text(
        painter,
        badge.center(),
        label,
        7.6,
        theme::GREEN_BRIGHT,
        false,
    );
}

fn paint_page_heading(
    painter: &Painter,
    layout: &Layout,
    page: Page,
    game_directory_ready: bool,
    motion: CardMotion,
) {
    let header = layout.header;
    let anchor = page_card_anchor(page, layout);
    let shift = motion.offset(anchor);
    let title_card = Rect::from_min_size(
        Pos2::new(header.left(), header.top() + shift),
        Vec2::new(258.0, header.height()),
    );
    theme::glass_panel(
        painter,
        title_card,
        theme::RADIUS,
        theme::GlassSpec::new(Color32::from_rgb(20, 31, 23), theme::GREEN, 0.0, 8.4),
    );
    let content = card_clip(painter, title_card);
    let painter = &content;
    theme::mono_text(
        painter,
        Pos2::new(title_card.left() + 14.0, title_card.top() + 3.0),
        page_kicker(page),
        7.5,
        theme::MUTED,
    );
    theme::bold_text(
        painter,
        Pos2::new(title_card.left() + 14.0, title_card.top() + 17.0),
        page_title(page),
        23.0,
        Color32::from_rgb(238, 245, 233),
    );
    let pill = Rect::from_min_size(
        Pos2::new(title_card.left() + 82.0, title_card.top() + 25.0),
        Vec2::new(69.0, 21.0),
    );
    let status_color = if game_directory_ready {
        theme::GREEN
    } else {
        Color32::from_rgb(226, 181, 86)
    };
    theme::fill_round(painter, pill, theme::RADIUS, theme::alpha(status_color, 15));
    painter.circle_filled(
        Pos2::new(pill.left() + 11.0, pill.center().y),
        2.2,
        status_color,
    );
    theme::text(
        painter,
        Pos2::new(pill.left() + 18.0, pill.top() + 4.5),
        page_status_label(page, game_directory_ready),
        8.0,
        Color32::from_rgb(145, 183, 141),
    );
}

fn page_card_anchor(page: Page, layout: &Layout) -> Rect {
    let top = layout.header.bottom() + 14.0;
    let left = layout.content_left;
    let width = (layout.content_right - layout.content_left).max(1.0);
    match page {
        Page::Home => layout.hero,
        Page::Versions | Page::Download => {
            Rect::from_min_size(Pos2::new(left, top), Vec2::new(width * 0.45, 104.0))
        }
        Page::Community => {
            Rect::from_min_size(Pos2::new(left, top), Vec2::new(width * 0.72, 220.0))
        }
        Page::Create => Rect::from_min_size(Pos2::new(left, top), Vec2::new(width, 218.0)),
        Page::Settings => {
            Rect::from_min_size(Pos2::new(left + 8.0, top + 8.0), Vec2::new(194.0, 66.0))
        }
    }
}

fn page_kicker(page: Page) -> &'static str {
    match page {
        Page::Home => "GENESIS LAUNCHER",
        Page::Versions => "VERSION LIBRARY",
        Page::Download => "MINECRAFT DOWNLOADS",
        Page::Community => "FAVORITES & MODS",
        Page::Create => "CREATIVE WORKSPACE",
        Page::Settings => "LAUNCHER PREFERENCES",
    }
}

fn page_title(page: Page) -> &'static str {
    match page {
        Page::Home => "游戏",
        Page::Versions => "版本",
        Page::Download => "下载",
        Page::Community => "收藏",
        Page::Create => "创造",
        Page::Settings => "设置",
    }
}

fn page_status_label(page: Page, game_directory_ready: bool) -> &'static str {
    match page {
        Page::Settings => "本机配置",
        Page::Versions => "已安装",
        Page::Download => "官方源",
        Page::Community => "收藏内容",
        Page::Create => "创作空间",
        Page::Home if game_directory_ready => "目录就绪",
        Page::Home => "未发现",
    }
}

fn compact_icon_button(
    ui: &mut egui::Ui,
    painter: &Painter,
    rect: Rect,
    id: &'static str,
    plus: bool,
) -> bool {
    let content = card_clip(painter, rect);
    let painter = &content;
    let response = ui
        .interact(rect, Id::unique(id), Sense::click())
        .on_hover_cursor(CursorIcon::PointingHand);
    theme::fill_round(
        painter,
        rect,
        theme::RADIUS,
        if response.hovered() {
            Color32::from_rgb(42, 64, 47)
        } else {
            Color32::from_rgb(32, 47, 37)
        },
    );
    theme::stroke_round(painter, rect, theme::RADIUS, 1.0, theme::white_alpha(34));
    painter.line_segment(
        [
            Pos2::new(rect.center().x - 6.0, rect.center().y),
            Pos2::new(rect.center().x + 6.0, rect.center().y),
        ],
        Stroke::new(1.5, theme::INK_SOFT),
    );
    if plus {
        painter.line_segment(
            [
                Pos2::new(rect.center().x, rect.center().y - 6.0),
                Pos2::new(rect.center().x, rect.center().y + 6.0),
            ],
            Stroke::new(1.5, theme::INK_SOFT),
        );
    }
    response.clicked()
}

fn settings_button(
    ui: &mut egui::Ui,
    painter: &Painter,
    rect: Rect,
    id: &'static str,
    label: &str,
    primary: bool,
) -> bool {
    let content = card_clip(painter, rect);
    let painter = &content;
    let response = ui
        .interact(rect, Id::unique(id), Sense::click())
        .on_hover_cursor(CursorIcon::PointingHand);
    if primary {
        theme::liquid_button(painter, rect, theme::RADIUS, 2.2, response.hovered());
        theme::center_text(painter, rect.center(), label, 9.0, Color32::WHITE, true);
    } else {
        theme::fill_round(
            painter,
            rect,
            theme::RADIUS,
            if response.hovered() {
                Color32::from_rgb(37, 53, 42)
            } else {
                Color32::from_rgb(28, 42, 33)
            },
        );
        theme::stroke_round(painter, rect, theme::RADIUS, 1.0, theme::white_alpha(34));
        theme::center_text(painter, rect.center(), label, 8.8, theme::INK_SOFT, false);
    }
    response.clicked()
}

fn skin_history_button(
    ui: &mut egui::Ui,
    painter: &Painter,
    rect: Rect,
    id: &'static str,
    label: &str,
    enabled: bool,
) -> bool {
    if !enabled {
        theme::fill_round(
            painter,
            rect,
            theme::RADIUS,
            Color32::from_rgba_unmultiplied(28, 40, 32, 120),
        );
        theme::stroke_round(painter, rect, theme::RADIUS, 1.0, theme::white_alpha(16));
        theme::center_text(
            painter,
            rect.center(),
            label,
            8.2,
            theme::white_alpha(58),
            false,
        );
        return false;
    }
    settings_button(ui, painter, rect, id, label, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(id: &str) -> GameVersion {
        GameVersion {
            version: id.to_owned(),
            game_version: id.to_owned(),
            loader: "原版".to_owned(),
            loader_family: "原版".to_owned(),
            java: "Java 21".to_owned(),
            description: String::new(),
            launchable: true,
        }
    }

    #[test]
    fn version_menu_orders_recently_played_first() {
        let versions = vec![version("1.21.8"), version("1.21.11"), version("1.21.10")];
        let recency = HashMap::from([("1.21.8".to_owned(), 100), ("1.21.11".to_owned(), 300)]);
        assert_eq!(version_menu_order(&versions, &recency), vec![1, 0, 2]);
    }

    #[test]
    fn builtin_favorites_have_six_valid_embedded_site_icons() {
        let icons = builtin_favorite_icon_assets();
        assert_eq!(icons.len(), crate::favorites::BUILTIN_SITES.len());
        for (index, (url, bytes, texture_name)) in icons.into_iter().enumerate() {
            assert_eq!(url, crate::favorites::BUILTIN_SITES[index].1);
            assert!(!texture_name.is_empty());
            let image = image::load_from_memory(bytes).unwrap();
            assert!(image.width() >= 32 && image.height() >= 32);
        }
    }

    #[test]
    fn favorite_site_settings_keep_icon_sources_and_read_legacy_entries() {
        let legacy = parse_favorite_site("Example\u{1f}https://example.com").unwrap();
        assert_eq!(legacy.title, "Example");
        assert_eq!(legacy.url, "https://example.com");
        assert!(legacy.icon_source.is_empty());

        let configured =
            parse_favorite_site("Example\u{1f}https://example.com\u{1f}C:\\icons\\example.png")
                .unwrap();
        assert_eq!(configured.icon_source, "C:\\icons\\example.png");
    }

    #[test]
    fn skin_model_uses_minecraft_body_proportions() {
        let steve = skin_cuboids(false);
        let head = steve
            .iter()
            .find(|part| part.part == SkinPart::Head)
            .unwrap();
        let body = steve
            .iter()
            .find(|part| part.part == SkinPart::Body)
            .unwrap();
        let right_arm = steve
            .iter()
            .find(|part| part.part == SkinPart::RightArm)
            .unwrap();
        let right_leg = steve
            .iter()
            .find(|part| part.part == SkinPart::RightLeg)
            .unwrap();
        assert_eq!(
            (head.size().x, head.size().y, head.size().z),
            (8.0, 8.0, 8.0)
        );
        assert_eq!(
            (body.size().x, body.size().y, body.size().z),
            (8.0, 12.0, 4.0)
        );
        assert_eq!(
            (right_arm.size().x, right_arm.size().y, right_arm.size().z),
            (4.0, 12.0, 4.0)
        );
        assert_eq!(
            (right_leg.size().x, right_leg.size().y, right_leg.size().z),
            (4.0, 12.0, 4.0)
        );
        assert_eq!(head.min.y, body.max.y);
        assert_eq!(body.min.y, right_leg.max.y);
        assert_eq!(right_arm.min.y, body.min.y);
        assert_eq!(right_arm.max.y, body.max.y);

        let alex = skin_cuboids(true);
        let alex_arm = alex
            .iter()
            .find(|part| part.part == SkinPart::LeftArm)
            .unwrap();
        assert_eq!(alex_arm.size().x, 3.0);
    }

    #[test]
    fn skin_uvs_stay_inside_the_texture_and_front_faces_match_the_layout() {
        for cuboid in skin_cuboids(false).into_iter().chain(skin_cuboids(true)) {
            for face in skin_cuboid_faces(cuboid) {
                assert!(face.uv.left() >= 0.0);
                assert!(face.uv.top() >= 0.0);
                assert!(face.uv.right() <= 1.0);
                assert!(face.uv.bottom() <= 1.0);
                assert!(face.uv.width() > 0.0);
                assert!(face.uv.height() > 0.0);
            }
        }
        let body = skin_cuboids(false)
            .into_iter()
            .find(|part| part.part == SkinPart::Body)
            .unwrap();
        let front = skin_cuboid_faces(body)
            .into_iter()
            .find(|face| face.face == 0)
            .unwrap();
        assert!((front.uv.left() - 20.0 / 64.0).abs() < 0.0001);
        assert!((front.uv.top() - 20.0 / 64.0).abs() < 0.0001);
        assert!((front.uv.right() - 28.0 / 64.0).abs() < 0.0001);
        assert!((front.uv.bottom() - 32.0 / 64.0).abs() < 0.0001);
    }

    #[test]
    fn skin_projection_keeps_the_character_upright_and_finite() {
        let stage = Rect::from_min_size(Pos2::new(100.0, 80.0), Vec2::new(320.0, 620.0));
        let top = project_skin_point(SkinPoint3::new(0.0, 32.0, 0.0), stage, 15.0, -0.42, -0.12);
        let bottom = project_skin_point(SkinPoint3::new(0.0, 0.0, 0.0), stage, 15.0, -0.42, -0.12);
        assert!(top.0.x.is_finite() && top.0.y.is_finite() && top.1.is_finite());
        assert!(bottom.0.x.is_finite() && bottom.0.y.is_finite() && bottom.1.is_finite());
        assert!(top.0.y < bottom.0.y);

        let faces = skin_preview_faces(stage, 15.0, -0.42, -0.12, false);
        assert!(!faces.is_empty());
        assert!(faces.iter().all(|face| {
            face.points
                .iter()
                .all(|point| point.x.is_finite() && point.y.is_finite())
                && face.depth.is_finite()
        }));
        assert!(faces.windows(2).all(|pair| pair[0].depth <= pair[1].depth));
    }

    #[test]
    fn hit_testing_maps_projected_faces_back_to_texture_pixels() {
        let face = SkinPreviewFace {
            key: SkinFaceKey {
                part: SkinPart::Head,
                face: 0,
            },
            points: [
                Pos2::new(10.0, 20.0),
                Pos2::new(50.0, 20.0),
                Pos2::new(50.0, 60.0),
                Pos2::new(10.0, 60.0),
            ],
            uv: Rect::from_min_max(
                Pos2::new(8.0 / 64.0, 8.0 / 64.0),
                Pos2::new(16.0 / 64.0, 16.0 / 64.0),
            ),
            depth: 1.0,
            tint: Color32::WHITE,
        };
        let top_left = face.texture_position(Pos2::new(10.0, 20.0)).unwrap();
        let center = face.texture_position(Pos2::new(30.0, 40.0)).unwrap();
        let bottom_right = face.texture_position(Pos2::new(50.0, 60.0)).unwrap();
        assert!((top_left.x - 8.0 / 64.0).abs() < 0.0001);
        assert!((top_left.y - 8.0 / 64.0).abs() < 0.0001);
        assert!((center.x - 12.0 / 64.0).abs() < 0.0001);
        assert!((center.y - 12.0 / 64.0).abs() < 0.0001);
        assert!((bottom_right.x - 16.0 / 64.0).abs() < 0.0001);
        assert!((bottom_right.y - 16.0 / 64.0).abs() < 0.0001);
        assert!(face.texture_position(Pos2::new(5.0, 10.0)).is_none());
    }

    #[test]
    fn default_skin_has_a_visible_face_and_clothing() {
        let pixels = default_skin_pixels();
        let face_offset = (12 * 64 + 12) * 4;
        let shirt_offset = (24 * 64 + 24) * 4;
        assert_eq!(pixels[face_offset + 3], 255);
        assert_eq!(pixels[shirt_offset + 3], 255);
        assert_ne!(&pixels[face_offset..face_offset + 3], &[0, 0, 0]);
    }

    #[test]
    fn rgb_color_readout_keeps_all_channels_exact() {
        assert_eq!(skin_rgb_hex([0, 0, 0, 255]), "#000000");
        assert_eq!(skin_rgb_hex([255, 255, 255, 255]), "#FFFFFF");
        assert_eq!(skin_rgb_hex([18, 171, 244, 255]), "#12ABF4");
    }

    #[test]
    fn skin_history_undoes_redo_and_invalidates_branch_after_new_edit() {
        let mut history = SkinEditHistory::default();
        let first = vec![0, 0, 0, 255];
        let second = vec![20, 40, 60, 255];
        let third = vec![90, 80, 70, 255];
        history.record(first.clone(), &second);
        assert!(history.can_undo());
        assert!(!history.can_redo());
        assert_eq!(history.undo(&second).as_deref(), Some(first.as_slice()));
        assert!(history.can_redo());
        assert_eq!(history.redo(&first).as_deref(), Some(second.as_slice()));
        assert_eq!(history.undo(&second).as_deref(), Some(first.as_slice()));
        history.record(first.clone(), &third);
        assert!(!history.can_redo());
        assert_eq!(history.undo(&third).as_deref(), Some(first.as_slice()));
    }

    #[test]
    fn skin_history_is_bounded_and_ignores_noop_edits() {
        let mut history = SkinEditHistory::default();
        let original = vec![0; 16 * 16 * 4];
        history.record(original.clone(), &original);
        assert!(!history.can_undo());
        for index in 0..(SKIN_HISTORY_LIMIT + 12) {
            let before = vec![index as u8; 16 * 16 * 4];
            let current = vec![(index + 1) as u8; 16 * 16 * 4];
            history.record(before, &current);
        }
        assert_eq!(history.undo.len(), SKIN_HISTORY_LIMIT);
    }

    #[test]
    fn create_tab_transition_spring_overshoots_softly_and_settles() {
        let mut progress = 0.0_f32;
        let mut velocity = 0.0_f32;
        let mut peak = 0.0_f32;
        for _ in 0..120 {
            spring_smooth(&mut progress, &mut velocity, 1.0, 1.0 / 60.0);
            peak = peak.max(progress);
        }
        assert!(peak > 1.0);
        assert!(peak < 1.12);
        assert_eq!(progress, 1.0);
        assert_eq!(velocity, 0.0);
    }

    #[test]
    fn download_category_indices_match_the_tab_order() {
        assert_eq!(resource_category_index(None), 0);
        assert_eq!(resource_category_index(Some(ResourceCategory::Mod)), 1);
        assert_eq!(resource_category_index(Some(ResourceCategory::Modpack)), 2);
        assert_eq!(resource_category_index(Some(ResourceCategory::Shader)), 3);
        assert_eq!(
            resource_category_index(Some(ResourceCategory::ResourcePack)),
            4
        );
        assert_eq!(resource_category_index(Some(ResourceCategory::Datapack)), 5);
    }

    #[test]
    fn liquid_glass_is_disabled_by_default() {
        let settings = LauncherSettings::default();
        assert!(!settings.liquid_glass);
        assert_eq!(settings.liquid_glass_mode, LiquidGlassMode::Regular);
    }

    #[test]
    fn legacy_glass_setting_migrates_to_off_and_new_choices_persist() {
        assert!(!resolve_liquid_glass_setting(0, Some(true)));
        assert!(resolve_liquid_glass_setting(SETTINGS_REVISION, Some(true)));
        assert!(!resolve_liquid_glass_setting(
            SETTINGS_REVISION,
            Some(false)
        ));
    }

    #[test]
    fn version_isolation_is_enabled_by_default() {
        assert!(LauncherSettings::default().version_isolation);
    }

    #[test]
    fn version_content_close_target_is_large_and_inside_dialog() {
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0));
        let dialog = version_content_dialog_rect(screen);
        let close = version_content_close_rect(dialog);
        assert!(dialog.contains(close.center()));
        assert!(close.width() >= 40.0);
        assert!(close.height() >= 40.0);
    }

    #[test]
    fn version_content_text_fades_without_opacity_jump() {
        assert_eq!(version_content_text_opacity(0.0), 0.0);
        assert_eq!(version_content_text_opacity(0.16), 0.0);
        assert_eq!(version_content_text_opacity(1.0), 1.0);
        let mut previous = 0.0;
        for step in 0..=100 {
            let opacity = version_content_text_opacity(step as f32 / 100.0);
            assert!(opacity + 0.0001 >= previous);
            previous = opacity;
        }
    }

    #[test]
    fn parses_official_launcher_timestamps() {
        assert_eq!(parse_iso_timestamp("1970-01-01T00:00:00Z"), Some(0));
        assert!(
            parse_iso_timestamp("2026-10-31T17:48:22Z")
                > parse_iso_timestamp("2026-10-31T17:48:21Z")
        );
        assert!(parse_iso_timestamp("not-a-timestamp").is_none());
    }

    #[test]
    fn closing_popup_never_fades_back_in() {
        let mut value = 1.0_f32;
        let mut velocity = 0.0_f32;
        let mut fully_hidden = false;
        for _ in 0..120 {
            spring_popup(&mut value, &mut velocity, 0.0, 1.0 / 60.0);
            let opacity = popup_opacity(value, false);
            if opacity == 0.0 {
                fully_hidden = true;
            } else {
                assert!(!fully_hidden);
            }
        }
        assert!(fully_hidden);
        assert_eq!(value, 0.0);
    }

    #[test]
    fn cards_at_the_leading_edge_start_first() {
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0));
        let upper = Rect::from_min_size(Pos2::new(0.0, 80.0), Vec2::new(100.0, 80.0));
        let lower = Rect::from_min_size(Pos2::new(0.0, 760.0), Vec2::new(100.0, 80.0));
        let moving_down = CardMotion {
            screen,
            direction: 1.0,
            progress: 0.2,
            outgoing: false,
        };
        assert!(moving_down.offset(lower) < moving_down.offset(upper));

        let moving_up = CardMotion {
            screen,
            direction: -1.0,
            progress: 0.2,
            outgoing: false,
        };
        assert!(moving_up.offset(upper) > moving_up.offset(lower));
    }

    #[test]
    fn writes_fabric_mod_project_skeleton() {
        let root = std::env::temp_dir().join(format!(
            "genesis-launcher-mod-project-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let project = write_mod_project(
            &root,
            &ModProjectSettings {
                name: "Example Tool".to_owned(),
                id: "example_tool".to_owned(),
                package: "com.example.exampletool".to_owned(),
                version: "1.0.0".to_owned(),
                minecraft: "1.21.1".to_owned(),
                loader: 0,
            },
        )
        .unwrap();
        assert!(project.join("build.gradle").is_file());
        assert!(project.join("src/main/resources/fabric.mod.json").is_file());
        assert!(
            project
                .join("src/main/java/com/example/exampletool/ExampleToolMod.java")
                .is_file()
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn creates_safe_mod_class_names() {
        assert_eq!(mod_class_name("My Cool Mod", "fallback"), "MyCoolMod");
        assert_eq!(mod_class_name("", "fallback-id"), "FallbackIdMod");
    }

    #[test]
    fn exports_version_archive_with_selected_content() {
        let root = std::env::temp_dir().join(format!(
            "genesis-launcher-export-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let base = root.join(".minecraft");
        let profile = "test-version";
        let version_dir = base.join("versions").join(profile);
        std::fs::create_dir_all(&version_dir).unwrap();
        std::fs::write(version_dir.join(format!("{profile}.json")), "{}").unwrap();
        std::fs::create_dir_all(base.join("mods")).unwrap();
        std::fs::write(base.join("mods").join("sample.jar"), b"mod").unwrap();
        std::fs::create_dir_all(base.join("config")).unwrap();
        std::fs::write(base.join("config").join("sample.toml"), b"config").unwrap();
        let output = root.join("export.zip");
        let mut item = version(profile);
        item.game_version = "1.21.1".to_owned();
        export_version_archive(
            &base,
            &base,
            &item,
            &output,
            &VersionExportOptions {
                mods: true,
                config: true,
                saves: false,
                resourcepacks: false,
                shaderpacks: false,
            },
        )
        .unwrap();
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&output).unwrap()).unwrap();
        assert!(archive.by_name("manifest.json").is_ok());
        assert!(archive.by_name("mods/sample.jar").is_ok());
        assert!(archive.by_name("config/sample.toml").is_ok());
        let _ = std::fs::remove_dir_all(root);
    }
}

fn is_snapshot_version(version: &str) -> bool {
    let lower = version.to_lowercase();
    lower.contains("snapshot")
        || lower.contains("-rc")
        || lower.contains("-pre")
        || lower.contains('w')
}

fn download_version_kind(kind: &str) -> &str {
    match kind {
        "release" => "正式版",
        "snapshot" => "快照",
        "old_beta" => "旧测试版",
        "old_alpha" => "旧 Alpha",
        _ => kind,
    }
}

fn resource_category_index(category: Option<ResourceCategory>) -> usize {
    category
        .and_then(|selected| {
            ResourceCategory::ALL
                .iter()
                .position(|category| *category == selected)
        })
        .map_or(0, |index| index + 1)
}

#[allow(clippy::too_many_arguments)]
fn paint_list_scrollbar(
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
    let track = Rect::from_min_size(
        Pos2::new(right - 7.0, list_top + 3.0),
        Vec2::new(3.0, list_height - 6.0),
    );
    theme::fill_round(painter, track, theme::RADIUS, theme::white_alpha(16));
    let thumb_height = (track.height() * list_height / content_height).max(34.0);
    let progress = (scroll / max_scroll).clamp(0.0, 1.0);
    let thumb = Rect::from_min_size(
        Pos2::new(
            track.left(),
            track.top() + (track.height() - thumb_height) * progress,
        ),
        Vec2::new(track.width(), thumb_height),
    );
    theme::fill_round(
        painter,
        thumb,
        theme::RADIUS,
        theme::alpha(theme::GREEN, 105),
    );
}

fn ellipsize_start(value: &str, max_chars: usize) -> String {
    let count = value.chars().count();
    if count <= max_chars {
        return value.to_owned();
    }
    let tail: String = value
        .chars()
        .skip(count.saturating_sub(max_chars))
        .collect();
    format!("…{tail}")
}

fn fit_text_to_width(painter: &Painter, value: &str, font: egui::FontId, max_width: f32) -> String {
    if painter
        .layout_no_wrap(value.to_owned(), font.clone(), theme::INK)
        .size()
        .x
        <= max_width
    {
        return value.to_owned();
    }
    let mut chars: Vec<char> = value.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let candidate = format!("{}…", chars.iter().collect::<String>());
        if painter
            .layout_no_wrap(candidate.clone(), font.clone(), theme::INK)
            .size()
            .x
            <= max_width
        {
            return candidate;
        }
    }
    "…".to_owned()
}

fn ellipsize_end(value: &str, max_chars: usize) -> String {
    let count = value.chars().count();
    if count <= max_chars {
        return value.to_owned();
    }
    let head: String = value.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{head}…")
}

fn relative_play_time(timestamp: u64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(timestamp);
    let elapsed = now.saturating_sub(timestamp);
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

fn spring_popup(value: &mut f32, velocity: &mut f32, target: f32, delta: f32) -> bool {
    const STIFFNESS: f32 = 310.0;
    const DAMPING: f32 = 21.0;
    let substeps = (delta / 0.008).ceil().clamp(1.0, 8.0) as usize;
    let step = delta / substeps as f32;
    for _ in 0..substeps {
        let acceleration = (target - *value) * STIFFNESS - *velocity * DAMPING;
        *velocity += acceleration * step;
        *value += *velocity * step;
    }
    if target == 0.0 && *value <= 0.0 {
        *value = 0.0;
        *velocity = 0.0;
        return false;
    }
    if (*value - target).abs() < 0.001 && velocity.abs() < 0.01 {
        *value = target;
        *velocity = 0.0;
        false
    } else {
        true
    }
}

fn spring_smooth(value: &mut f32, velocity: &mut f32, target: f32, delta: f32) -> bool {
    const STIFFNESS: f32 = 270.0;
    const DAMPING: f32 = 20.0;
    let substeps = (delta / 0.008).ceil().clamp(1.0, 8.0) as usize;
    let step = delta / substeps as f32;
    for _ in 0..substeps {
        let acceleration = (target - *value) * STIFFNESS - *velocity * DAMPING;
        *velocity += acceleration * step;
        *value += *velocity * step;
    }
    if (*value - target).abs() < 0.001 && velocity.abs() < 0.01 {
        *value = target;
        *velocity = 0.0;
        false
    } else {
        true
    }
}

fn popup_opacity(value: f32, open: bool) -> f32 {
    let fade = if open {
        value.clamp(0.0, 1.0)
    } else {
        ((value - 0.10) / 0.90).clamp(0.0, 1.0)
    };
    fade * fade * (3.0 - 2.0 * fade)
}

fn news_accent(index: usize) -> Color32 {
    match index {
        0 => Color32::from_rgb(93, 213, 205),
        1 => theme::GREEN,
        _ => Color32::from_rgb(143, 173, 247),
    }
}

fn sanitize_filename(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .collect()
}

fn export_version_archive(
    game_directory: &std::path::Path,
    data_directory: &std::path::Path,
    version: &GameVersion,
    destination: &std::path::Path,
    options: &VersionExportOptions,
) -> Result<(), String> {
    let file =
        std::fs::File::create(destination).map_err(|error| format!("无法创建导出文件：{error}"))?;
    let mut archive = zip::ZipWriter::new(file);
    let zip_options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);
    let manifest = serde_json::json!({
        "format": "GenesisLauncherVersionExport",
        "version": version.version,
        "minecraftVersion": version.game_version,
        "loader": version.loader,
        "loaderFamily": version.loader_family,
        "versionIsolation": version.version != version.game_version,
    })
    .to_string();
    archive
        .start_file("manifest.json", zip_options)
        .map_err(|error| format!("无法写入导出清单：{error}"))?;
    archive
        .write_all(manifest.as_bytes())
        .map_err(|error| format!("无法写入导出清单：{error}"))?;

    let version_directory = game_directory.join("versions").join(&version.version);
    for file_name in [
        format!("{}.json", version.version),
        format!("{}.jar", version.version),
    ] {
        let source = version_directory.join(&file_name);
        if source.is_file() {
            add_zip_file(
                &mut archive,
                &source,
                &format!("versions/{}/{}", version.version, file_name),
                zip_options,
            )?;
        }
    }

    let folders = [
        (options.mods, "mods"),
        (options.config, "config"),
        (options.saves, "saves"),
        (options.resourcepacks, "resourcepacks"),
        (options.shaderpacks, "shaderpacks"),
    ];
    for (include, folder) in folders {
        if include {
            add_zip_directory(
                &mut archive,
                &data_directory.join(folder),
                folder,
                zip_options,
            )?;
        }
    }
    archive
        .finish()
        .map_err(|error| format!("无法完成导出：{error}"))?;
    Ok(())
}

fn add_zip_directory(
    archive: &mut zip::ZipWriter<std::fs::File>,
    source: &std::path::Path,
    prefix: &str,
    options: zip::write::SimpleFileOptions,
) -> Result<(), String> {
    let entries = match std::fs::read_dir(source) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!("无法读取导出目录 {}：{error}", source.display()));
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let archive_path = format!("{prefix}/{name}");
        if path.is_dir() {
            add_zip_directory(archive, &path, &archive_path, options)?;
        } else if path.is_file() {
            add_zip_file(archive, &path, &archive_path, options)?;
        }
    }
    Ok(())
}

fn add_zip_file(
    archive: &mut zip::ZipWriter<std::fs::File>,
    source: &std::path::Path,
    archive_path: &str,
    options: zip::write::SimpleFileOptions,
) -> Result<(), String> {
    archive
        .start_file(archive_path.replace('\\', "/"), options)
        .map_err(|error| format!("无法写入 {}：{error}", archive_path))?;
    let mut file = std::fs::File::open(source)
        .map_err(|error| format!("无法读取 {}：{error}", source.display()))?;
    std::io::copy(&mut file, archive)
        .map_err(|error| format!("无法压缩 {}：{error}", source.display()))?;
    Ok(())
}

fn pick_export_file(default_name: &str) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let default_name = default_name.replace('\'', "''");
        let script = format!(
            "[Console]::OutputEncoding=[Text.Encoding]::UTF8; Add-Type -AssemblyName System.Windows.Forms; $d=New-Object System.Windows.Forms.SaveFileDialog; $d.Filter='ZIP 压缩包 (*.zip)|*.zip'; $d.FileName='{default_name}'; if($d.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK){{[Console]::Out.Write($d.FileName)}}"
        );
        let mut command = std::process::Command::new("powershell.exe");
        command
            .args(["-NoProfile", "-STA", "-Command", &script])
            .creation_flags(0x0800_0000);
        let output = command.output().ok()?;
        let selected = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        (!selected.is_empty()).then(|| PathBuf::from(selected))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = default_name;
        None
    }
}

fn pick_image_file(save: bool) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let dialog = if save {
            "$d=New-Object System.Windows.Forms.SaveFileDialog; $d.Filter='PNG 图片 (*.png)|*.png'; $d.FileName='skin.png'"
        } else {
            "$d=New-Object System.Windows.Forms.OpenFileDialog; $d.Filter='PNG 图片 (*.png)|*.png|所有图片 (*.*)|*.*'; $d.Multiselect=$false"
        };
        let script = format!(
            "[Console]::OutputEncoding=[Text.Encoding]::UTF8; Add-Type -AssemblyName System.Windows.Forms; {dialog}; if($d.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK){{[Console]::Out.Write($d.FileName)}}"
        );
        let mut command = std::process::Command::new("powershell.exe");
        command
            .args(["-NoProfile", "-STA", "-Command", &script])
            .creation_flags(0x0800_0000);
        let output = command.output().ok()?;
        let selected = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        (!selected.is_empty()).then(|| PathBuf::from(selected))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = save;
        None
    }
}

fn pick_project_directory() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let script = "[Console]::OutputEncoding=[Text.Encoding]::UTF8; Add-Type -AssemblyName System.Windows.Forms; $d=New-Object System.Windows.Forms.FolderBrowserDialog; $d.Description='选择模组工程保存位置'; $d.ShowNewFolderButton=$true; if($d.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK){[Console]::Out.Write($d.SelectedPath)}";
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

fn write_mod_project(
    root: &std::path::Path,
    settings: &ModProjectSettings,
) -> Result<PathBuf, String> {
    let id = settings.id.trim();
    if id.is_empty()
        || !id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
    {
        return Err("模组 ID 只能包含字母、数字、下划线和连字符".to_owned());
    }
    let package = settings.package.trim();
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
    if settings.minecraft.trim().is_empty() {
        return Err("Minecraft 版本不能为空".to_owned());
    }
    let project = root.join(format!("{id}-mod"));
    if project.exists() {
        return Err(format!("目标目录已经存在：{}", project.display()));
    }
    let class_name = mod_class_name(&settings.name, id);
    let package_path = package.replace('.', "/");
    let java_dir = project.join("src/main/java").join(&package_path);
    let resources = project.join("src/main/resources");
    fs::create_dir_all(&java_dir).map_err(|error| format!("创建源码目录失败：{error}"))?;
    fs::create_dir_all(&resources).map_err(|error| format!("创建资源目录失败：{error}"))?;
    let java = match settings.loader {
        0 => format!(
            "package {package};\n\nimport net.fabricmc.api.ModInitializer;\nimport org.slf4j.Logger;\nimport org.slf4j.LoggerFactory;\n\npublic final class {class_name} implements ModInitializer {{\n    public static final String MOD_ID = \"{id}\";\n    public static final Logger LOGGER = LoggerFactory.getLogger(MOD_ID);\n\n    @Override\n    public void onInitialize() {{\n        LOGGER.info(\"Initializing {}\", MOD_ID);\n    }}\n}}\n",
            settings.name.replace(['"', '\\'], "")
        ),
        _ => format!(
            "package {package};\n\nimport net.minecraftforge.fml.common.Mod;\n\n@Mod({class_name}.MOD_ID)\npublic final class {class_name} {{\n    public static final String MOD_ID = \"{id}\";\n\n    public {class_name}() {{\n    }}\n}}\n"
        ),
    };
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
            fs::write(
                project.join("build.gradle"),
                "plugins {\n    id 'fabric-loom' version '1.10-SNAPSHOT'\n}\n\nversion = project.mod_version\ngroup = project.maven_group\n\nbase {\n    archivesName = project.archives_base_name\n}\n\ndependencies {\n    minecraft \"com.mojang:minecraft:${project.minecraft_version}\"\n    mappings loom.officialMojangMappings()\n    modImplementation \"net.fabricmc:fabric-loader:${project.loader_version}\"\n}\n\njava {\n    toolchain.languageVersion = JavaLanguageVersion.of(21)\n}\n",
            )
            .map_err(|error| format!("写入 Gradle 配置失败：{error}"))?;
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
                fs::write(
                    project.join("build.gradle"),
                    "plugins {\n    id 'eclipse'\n    id 'net.minecraftforge.gradle' version '[6.0,6.2)'\n}\n\nversion = project.mod_version\ngroup = project.maven_group\n\nminecraft {\n    mappings channel: 'official', version: project.minecraft_version\n}\n\ndependencies {\n    minecraft \"net.minecraftforge:forge:${project.minecraft_version}-${project.loader_version}\"\n}\n",
                )
                .map_err(|error| format!("写入 Forge Gradle 配置失败：{error}"))?;
            } else {
                fs::write(
                    project.join("build.gradle"),
                    "plugins {\n    id 'net.neoforged.moddev' version '2.0.78'\n}\n\nversion = project.mod_version\ngroup = project.maven_group\n\nneoForge {\n    version = \"${project.minecraft_version}-${project.loader_version}\"\n}\n",
                )
                .map_err(|error| format!("写入 NeoForge Gradle 配置失败：{error}"))?;
            }
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

fn mod_class_name(name: &str, fallback: &str) -> String {
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
    if class_name.is_empty() {
        class_name = "ExampleMod".to_owned();
    }
    if !class_name.ends_with("Mod") {
        class_name.push_str("Mod");
    }
    class_name
}

fn open_external_url(url: &str) -> Result<(), String> {
    if !crate::news::allowed_article_url(url) {
        return Err("该链接不是 Minecraft 官方页面".to_owned());
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("rundll32.exe")
            .args(["url.dll,FileProtocolHandler", url])
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

fn installed_mod_files(game_directory: &std::path::Path) -> Vec<(String, u64)> {
    let Ok(entries) = std::fs::read_dir(game_directory.join("mods")) else {
        return Vec::new();
    };
    let mut files = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
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

fn count_directories(path: &std::path::Path) -> usize {
    std::fs::read_dir(path)
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.path().is_dir())
                .count()
        })
        .unwrap_or(0)
}

fn count_content_packs(path: &std::path::Path) -> usize {
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

fn version_content_entries(
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
                    if !path.is_dir() {
                        return None;
                    }
                    Some(VersionContentEntry {
                        name,
                        detail: "存档文件夹".to_owned(),
                    })
                }
                VersionContentKind::ShaderPacks | VersionContentKind::ResourcePacks => {
                    if path.is_dir() {
                        Some(VersionContentEntry {
                            name,
                            detail: format!(
                                "{} · 文件夹",
                                if kind == VersionContentKind::ShaderPacks {
                                    "光影包"
                                } else {
                                    "材质包"
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

fn format_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn compact_count(value: u64) -> String {
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

fn open_path(path: &std::path::Path) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer").arg(path).spawn();
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    }
}

fn card_clip(painter: &Painter, card: Rect) -> Painter {
    painter.with_clip_rect(card)
}

fn default_skin_pixels() -> Vec<u8> {
    let mut pixels = vec![0; 64 * 64 * 4];
    let skin = [204, 153, 119, 255];
    let skin_light = [231, 184, 147, 255];
    let hair = [61, 45, 36, 255];
    let hair_light = [82, 60, 44, 255];
    let shirt = [54, 137, 185, 255];
    let shirt_dark = [39, 106, 150, 255];
    let pants = [52, 77, 139, 255];
    let pants_light = [66, 94, 158, 255];
    let shoe = [66, 61, 56, 255];
    let eye_white = [239, 239, 231, 255];
    let eye = [39, 72, 94, 255];

    fill_skin_rect(&mut pixels, 8, 0, 8, 8, hair);
    fill_skin_rect(&mut pixels, 16, 0, 8, 8, skin);
    fill_skin_rect(&mut pixels, 0, 8, 8, 8, hair_light);
    fill_skin_rect(&mut pixels, 8, 8, 8, 8, skin);
    fill_skin_rect(&mut pixels, 16, 8, 8, 8, hair_light);
    fill_skin_rect(&mut pixels, 24, 8, 8, 8, hair);
    fill_skin_rect(&mut pixels, 8, 8, 8, 2, hair_light);
    fill_skin_rect(&mut pixels, 12, 11, 1, 1, skin_light);
    fill_skin_rect(&mut pixels, 10, 12, 1, 1, eye_white);
    fill_skin_rect(&mut pixels, 13, 12, 1, 1, eye_white);
    fill_skin_rect(&mut pixels, 10, 13, 1, 1, eye);
    fill_skin_rect(&mut pixels, 13, 13, 1, 1, eye);
    fill_skin_rect(&mut pixels, 11, 14, 2, 1, [125, 76, 61, 255]);

    for (x, y, width, height) in [
        (20, 16, 8, 4),
        (28, 16, 8, 4),
        (16, 20, 4, 12),
        (20, 20, 8, 12),
        (28, 20, 4, 12),
        (32, 20, 8, 12),
    ] {
        fill_skin_rect(&mut pixels, x, y, width, height, shirt);
    }
    fill_skin_rect(&mut pixels, 20, 20, 8, 2, shirt_dark);
    fill_skin_rect(&mut pixels, 16, 20, 4, 2, shirt_dark);
    fill_skin_rect(&mut pixels, 28, 20, 4, 2, shirt_dark);

    for (x, y, width, height) in [
        (44, 16, 4, 4),
        (48, 16, 4, 4),
        (40, 20, 4, 12),
        (44, 20, 4, 12),
        (48, 20, 4, 12),
        (52, 20, 4, 12),
    ] {
        fill_skin_rect(&mut pixels, x, y, width, height, skin);
    }
    fill_skin_rect(&mut pixels, 40, 20, 4, 3, shirt);
    fill_skin_rect(&mut pixels, 44, 20, 4, 3, shirt);
    fill_skin_rect(&mut pixels, 48, 20, 4, 3, shirt_dark);
    fill_skin_rect(&mut pixels, 52, 20, 4, 3, shirt);

    for (x, y, width, height) in [
        (36, 48, 4, 4),
        (40, 48, 4, 4),
        (32, 52, 4, 12),
        (36, 52, 4, 12),
        (40, 52, 4, 12),
        (44, 52, 4, 12),
    ] {
        fill_skin_rect(&mut pixels, x, y, width, height, skin);
    }
    fill_skin_rect(&mut pixels, 32, 52, 4, 3, shirt);
    fill_skin_rect(&mut pixels, 36, 52, 4, 3, shirt_dark);
    fill_skin_rect(&mut pixels, 40, 52, 4, 3, shirt);
    fill_skin_rect(&mut pixels, 44, 52, 4, 3, shirt_dark);

    for (x, y, width, height) in [
        (4, 16, 4, 4),
        (8, 16, 4, 4),
        (0, 20, 4, 12),
        (4, 20, 4, 12),
        (8, 20, 4, 12),
        (12, 20, 4, 12),
        (20, 48, 4, 4),
        (24, 48, 4, 4),
        (16, 52, 4, 12),
        (20, 52, 4, 12),
        (24, 52, 4, 12),
        (28, 52, 4, 12),
    ] {
        fill_skin_rect(&mut pixels, x, y, width, height, pants);
    }
    for (x, y) in [(0, 20), (8, 20), (16, 52), (24, 52)] {
        fill_skin_rect(&mut pixels, x, y + 1, 4, 11, pants_light);
    }
    for (x, y) in [
        (0, 29),
        (4, 29),
        (8, 29),
        (12, 29),
        (16, 61),
        (20, 61),
        (24, 61),
        (28, 61),
    ] {
        fill_skin_rect(&mut pixels, x, y, 4, 3, shoe);
    }
    pixels
}

fn fill_skin_rect(
    pixels: &mut [u8],
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    color: [u8; 4],
) {
    for py in y..(y + height).min(64) {
        for px in x..(x + width).min(64) {
            let offset = (py * 64 + px) * 4;
            pixels[offset..offset + 4].copy_from_slice(&color);
        }
    }
}

fn skin_cuboids(alex: bool) -> [SkinCuboid; 6] {
    let arm_width = if alex { 3.0 } else { 4.0 };
    [
        SkinCuboid {
            part: SkinPart::Head,
            min: SkinPoint3::new(-4.0, 24.0, -4.0),
            max: SkinPoint3::new(4.0, 32.0, 4.0),
            uv_x: 0.0,
            uv_y: 0.0,
        },
        SkinCuboid {
            part: SkinPart::Body,
            min: SkinPoint3::new(-4.0, 12.0, -2.0),
            max: SkinPoint3::new(4.0, 24.0, 2.0),
            uv_x: 16.0,
            uv_y: 16.0,
        },
        SkinCuboid {
            part: SkinPart::RightArm,
            min: SkinPoint3::new(-4.0 - arm_width, 12.0, -2.0),
            max: SkinPoint3::new(-4.0, 24.0, 2.0),
            uv_x: 40.0,
            uv_y: 16.0,
        },
        SkinCuboid {
            part: SkinPart::LeftArm,
            min: SkinPoint3::new(4.0, 12.0, -2.0),
            max: SkinPoint3::new(4.0 + arm_width, 24.0, 2.0),
            uv_x: 32.0,
            uv_y: 48.0,
        },
        SkinCuboid {
            part: SkinPart::RightLeg,
            min: SkinPoint3::new(-4.0, 0.0, -2.0),
            max: SkinPoint3::new(0.0, 12.0, 2.0),
            uv_x: 0.0,
            uv_y: 16.0,
        },
        SkinCuboid {
            part: SkinPart::LeftLeg,
            min: SkinPoint3::new(0.0, 0.0, -2.0),
            max: SkinPoint3::new(4.0, 12.0, 2.0),
            uv_x: 16.0,
            uv_y: 48.0,
        },
    ]
}

fn skin_cuboid_faces(cuboid: SkinCuboid) -> [SkinCuboidFace; 6] {
    let size = cuboid.size();
    let uv = |x: f32, y: f32, width: f32, height: f32| {
        Rect::from_min_max(
            Pos2::new(x / 64.0, y / 64.0),
            Pos2::new((x + width) / 64.0, (y + height) / 64.0),
        )
    };
    let min = cuboid.min;
    let max = cuboid.max;
    [
        SkinCuboidFace {
            face: 0,
            points: [
                SkinPoint3::new(min.x, max.y, max.z),
                SkinPoint3::new(max.x, max.y, max.z),
                SkinPoint3::new(max.x, min.y, max.z),
                SkinPoint3::new(min.x, min.y, max.z),
            ],
            uv: uv(cuboid.uv_x + size.z, cuboid.uv_y + size.z, size.x, size.y),
            normal: SkinPoint3::new(0.0, 0.0, 1.0),
        },
        SkinCuboidFace {
            face: 1,
            points: [
                SkinPoint3::new(max.x, max.y, min.z),
                SkinPoint3::new(min.x, max.y, min.z),
                SkinPoint3::new(min.x, min.y, min.z),
                SkinPoint3::new(max.x, min.y, min.z),
            ],
            uv: uv(
                cuboid.uv_x + size.z * 3.0,
                cuboid.uv_y + size.z,
                size.x,
                size.y,
            ),
            normal: SkinPoint3::new(0.0, 0.0, -1.0),
        },
        SkinCuboidFace {
            face: 2,
            points: [
                SkinPoint3::new(min.x, max.y, min.z),
                SkinPoint3::new(min.x, max.y, max.z),
                SkinPoint3::new(min.x, min.y, max.z),
                SkinPoint3::new(min.x, min.y, min.z),
            ],
            uv: uv(cuboid.uv_x, cuboid.uv_y + size.z, size.z, size.y),
            normal: SkinPoint3::new(-1.0, 0.0, 0.0),
        },
        SkinCuboidFace {
            face: 3,
            points: [
                SkinPoint3::new(max.x, max.y, max.z),
                SkinPoint3::new(max.x, max.y, min.z),
                SkinPoint3::new(max.x, min.y, min.z),
                SkinPoint3::new(max.x, min.y, max.z),
            ],
            uv: uv(
                cuboid.uv_x + size.z * 2.0,
                cuboid.uv_y + size.z,
                size.z,
                size.y,
            ),
            normal: SkinPoint3::new(1.0, 0.0, 0.0),
        },
        SkinCuboidFace {
            face: 4,
            points: [
                SkinPoint3::new(min.x, max.y, min.z),
                SkinPoint3::new(max.x, max.y, min.z),
                SkinPoint3::new(max.x, max.y, max.z),
                SkinPoint3::new(min.x, max.y, max.z),
            ],
            uv: uv(cuboid.uv_x + size.z, cuboid.uv_y, size.x, size.z),
            normal: SkinPoint3::new(0.0, 1.0, 0.0),
        },
        SkinCuboidFace {
            face: 5,
            points: [
                SkinPoint3::new(min.x, min.y, max.z),
                SkinPoint3::new(max.x, min.y, max.z),
                SkinPoint3::new(max.x, min.y, min.z),
                SkinPoint3::new(min.x, min.y, min.z),
            ],
            uv: uv(cuboid.uv_x + size.z + size.x, cuboid.uv_y, size.x, size.z),
            normal: SkinPoint3::new(0.0, -1.0, 0.0),
        },
    ]
}

fn rotate_skin_point(point: SkinPoint3, yaw: f32, pitch: f32) -> SkinPoint3 {
    let (sin_yaw, cos_yaw) = yaw.sin_cos();
    let yawed_x = point.x * cos_yaw + point.z * sin_yaw;
    let yawed_z = -point.x * sin_yaw + point.z * cos_yaw;
    let (sin_pitch, cos_pitch) = pitch.sin_cos();
    SkinPoint3::new(
        yawed_x,
        point.y * cos_pitch - yawed_z * sin_pitch,
        point.y * sin_pitch + yawed_z * cos_pitch,
    )
}

fn project_skin_point(
    point: SkinPoint3,
    stage: Rect,
    scale: f32,
    yaw: f32,
    pitch: f32,
) -> (Pos2, f32) {
    let rotated = rotate_skin_point(
        SkinPoint3::new(point.x, point.y - 16.0, point.z),
        yaw,
        pitch,
    );
    (
        Pos2::new(
            stage.center().x + rotated.x * scale,
            stage.center().y - rotated.y * scale,
        ),
        rotated.z,
    )
}

fn skin_preview_faces(
    stage: Rect,
    scale: f32,
    yaw: f32,
    pitch: f32,
    alex: bool,
) -> Vec<SkinPreviewFace> {
    let light = SkinPoint3::new(-0.32, 0.62, 0.72);
    let mut faces = Vec::with_capacity(36);
    for cuboid in skin_cuboids(alex) {
        for face in skin_cuboid_faces(cuboid) {
            let normal = rotate_skin_point(face.normal, yaw, pitch);
            if normal.z <= 0.001 {
                continue;
            }
            let projected = face
                .points
                .map(|point| project_skin_point(point, stage, scale, yaw, pitch));
            let depth = projected.iter().map(|(_, depth)| *depth).sum::<f32>() / 4.0;
            let lighting = (normal.x * light.x + normal.y * light.y + normal.z * light.z).max(0.0);
            let brightness = (214.0 + lighting * 40.0).clamp(0.0, 255.0) as u8;
            faces.push(SkinPreviewFace {
                key: SkinFaceKey {
                    part: cuboid.part,
                    face: face.face,
                },
                points: projected.map(|(point, _)| point),
                uv: face.uv,
                depth,
                tint: Color32::from_gray(brightness),
            });
        }
    }
    faces.sort_by(|left, right| left.depth.total_cmp(&right.depth));
    faces
}

fn paint_skin_model(painter: &Painter, texture: &egui::TextureHandle, faces: &[SkinPreviewFace]) {
    let mut mesh = egui::epaint::Mesh::with_texture(texture.id());
    for face in faces {
        let start = mesh.vertices.len() as u32;
        let uv = [
            face.uv.left_top(),
            face.uv.right_top(),
            face.uv.right_bottom(),
            face.uv.left_bottom(),
        ];
        for (point, uv) in face.points.into_iter().zip(uv) {
            mesh.vertices.push(egui::epaint::Vertex {
                pos: point,
                uv,
                color: face.tint,
            });
        }
        mesh.indices
            .extend_from_slice(&[start, start + 1, start + 2, start, start + 2, start + 3]);
    }
    painter.add(Shape::mesh(mesh));
}

fn skin_face_at<'a>(
    mut faces: impl Iterator<Item = &'a SkinPreviewFace>,
    pointer: Pos2,
) -> Option<&'a SkinPreviewFace> {
    faces.find(|face| face.texture_position(pointer).is_some())
}

fn paint_rounded_image(
    painter: &Painter,
    rect: Rect,
    radius: f32,
    texture: &egui::TextureHandle,
    source_size: [usize; 2],
) {
    const CORNER_SEGMENTS: usize = 7;
    let radius = radius.min(rect.width() * 0.5).min(rect.height() * 0.5);
    let source_aspect = source_size[0] as f32 / source_size[1].max(1) as f32;
    let target_aspect = rect.width() / rect.height().max(1.0);
    let (u0, u1, v0, v1) = if source_aspect > target_aspect {
        let visible = target_aspect / source_aspect;
        ((1.0 - visible) * 0.5, (1.0 + visible) * 0.5, 0.0, 1.0)
    } else {
        let visible = source_aspect / target_aspect;
        (0.0, 1.0, (1.0 - visible) * 0.5, (1.0 + visible) * 0.5)
    };
    let corners = [
        (
            Pos2::new(rect.left() + radius, rect.top() + radius),
            std::f32::consts::PI,
            std::f32::consts::PI * 1.5,
        ),
        (
            Pos2::new(rect.right() - radius, rect.top() + radius),
            std::f32::consts::PI * 1.5,
            std::f32::consts::TAU,
        ),
        (
            Pos2::new(rect.right() - radius, rect.bottom() - radius),
            0.0,
            std::f32::consts::FRAC_PI_2,
        ),
        (
            Pos2::new(rect.left() + radius, rect.bottom() - radius),
            std::f32::consts::FRAC_PI_2,
            std::f32::consts::PI,
        ),
    ];
    let mut perimeter = Vec::with_capacity((CORNER_SEGMENTS + 1) * 4);
    for (center, start, end) in corners {
        for segment in 0..=CORNER_SEGMENTS {
            let angle = start + (end - start) * segment as f32 / CORNER_SEGMENTS as f32;
            perimeter.push(center + Vec2::angled(angle) * radius);
        }
    }
    let uv = |point: Pos2| {
        Pos2::new(
            u0 + (point.x - rect.left()) / rect.width() * (u1 - u0),
            v0 + (point.y - rect.top()) / rect.height() * (v1 - v0),
        )
    };
    let mut mesh = egui::Mesh {
        texture_id: texture.id(),
        ..Default::default()
    };
    mesh.vertices.push(egui::epaint::Vertex {
        pos: rect.center(),
        uv: uv(rect.center()),
        color: Color32::WHITE,
    });
    for point in &perimeter {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: *point,
            uv: uv(*point),
            color: Color32::WHITE,
        });
    }
    for index in 0..perimeter.len() {
        mesh.add_triangle(
            0,
            (index + 1) as u32,
            ((index + 1) % perimeter.len() + 1) as u32,
        );
    }
    painter.add(Shape::mesh(mesh));
}

struct Layout {
    screen: Rect,
    titlebar: Rect,
    rail: Rect,
    content_left: f32,
    content_right: f32,
    header: Rect,
    version_trigger: Rect,
    rail_profile: Rect,
    hero: Rect,
    latest: Rect,
    news_cards: [Rect; 3],
}

impl Layout {
    fn new(screen: Rect) -> Self {
        let rail_width = if screen.width() < 1120.0 {
            78.0
        } else {
            RAIL_WIDTH
        };
        let content_left = rail_width + CONTENT_PAD;
        let content_right = screen.right() - CONTENT_PAD;
        let rail_profile_size = 45.0;
        let rail_profile_margin = (rail_width - rail_profile_size) * 0.5;
        let header_top = TITLEBAR_HEIGHT + 19.0;
        let header = Rect::from_min_max(
            Pos2::new(content_left, header_top),
            Pos2::new(content_right, header_top + 54.0),
        );
        let version_trigger = Rect::from_min_size(
            Pos2::new(content_right - 170.0, header.top() + 4.0),
            Vec2::new(170.0, 46.0),
        );
        let bottom = screen.bottom() - 22.0;
        let latest = Rect::from_min_max(
            Pos2::new(content_left, bottom - 162.0),
            Pos2::new(content_right, bottom),
        );
        let hero = Rect::from_min_max(
            Pos2::new(content_left, header.bottom() + 14.0),
            Pos2::new(content_right, latest.top() - 14.0),
        );
        let cards_y = latest.top() + 43.0;
        let cards_height = bottom - cards_y;
        let gap = 10.0;
        let card_width = (content_right - content_left - gap * 2.0) / 3.0;
        let mut news_cards = [Rect::NOTHING; 3];
        for (index, card) in news_cards.iter_mut().enumerate() {
            let left = content_left + index as f32 * (card_width + gap);
            *card = Rect::from_min_size(
                Pos2::new(left, cards_y),
                Vec2::new(card_width, cards_height),
            );
        }

        Self {
            screen,
            titlebar: Rect::from_min_max(
                screen.left_top(),
                Pos2::new(screen.right(), TITLEBAR_HEIGHT),
            ),
            rail: Rect::from_min_max(
                Pos2::new(screen.left(), TITLEBAR_HEIGHT),
                Pos2::new(rail_width, screen.bottom()),
            ),
            rail_profile: Rect::from_center_size(
                Pos2::new(
                    rail_width * 0.5,
                    screen.bottom() - rail_profile_margin - rail_profile_size * 0.5,
                ),
                Vec2::splat(rail_profile_size),
            ),
            content_left,
            content_right,
            header,
            version_trigger,
            hero,
            latest,
            news_cards,
        }
    }
}

fn profile_menu_rect(layout: &Layout) -> Rect {
    let size = Vec2::new(264.0, 218.0);
    let top = (layout.rail_profile.top() - size.y - 10.0).max(layout.titlebar.bottom() + 10.0);
    Rect::from_min_size(Pos2::new(layout.rail.right() + 12.0, top), size)
}

fn version_folder_selector_rect(layout: &Layout) -> Rect {
    let page_top = layout.header.bottom() + 14.0;
    let list_width = ((layout.content_right - layout.content_left) * 0.49).clamp(430.0, 590.0);
    Rect::from_min_size(
        Pos2::new(layout.content_left + 22.0, page_top + 68.0),
        Vec2::new(list_width - 44.0, 34.0),
    )
}

fn version_folder_menu_rect(layout: &Layout, directory_count: usize) -> Rect {
    let selector = version_folder_selector_rect(layout);
    let visible = directory_count.min(5);
    Rect::from_min_size(
        Pos2::new(selector.left(), selector.bottom() + 6.0),
        Vec2::new(selector.width(), 126.0 + visible as f32 * 42.0),
    )
}

fn version_content_dialog_rect(screen: Rect) -> Rect {
    Rect::from_center_size(
        screen.center(),
        Vec2::new(
            760.0_f32.min(screen.width() - 34.0),
            560.0_f32.min(screen.height() - 42.0),
        ),
    )
}

fn version_content_close_rect(dialog: Rect) -> Rect {
    Rect::from_center_size(
        Pos2::new(dialog.right() - 30.0, dialog.top() + 30.0),
        Vec2::splat(40.0),
    )
}

fn java_picker_rect(anchor: Rect, screen: Rect, option_count: usize) -> Rect {
    let visible = option_count.min(6);
    let width = 360.0_f32.min(screen.width() - 24.0);
    let height = 78.0 + visible as f32 * 42.0;
    let x = anchor.right() - width;
    let y = (anchor.bottom() + 6.0).min(screen.bottom() - height - 12.0);
    Rect::from_min_size(
        Pos2::new(
            x.clamp(screen.left() + 12.0, screen.right() - width - 12.0),
            y,
        ),
        Vec2::new(width, height),
    )
}

fn version_export_dialog_rect(screen: Rect) -> Rect {
    Rect::from_center_size(
        screen.center(),
        Vec2::new(
            560.0_f32.min(screen.width() - 30.0),
            470.0_f32.min(screen.height() - 34.0),
        ),
    )
}

fn version_menu_rect(layout: &Layout, version_count: usize) -> Rect {
    let width = layout.version_trigger.width().max(286.0);
    let height = (35.0 + version_count as f32 * VERSION_ROW_HEIGHT).min(VERSION_MENU_MAX_HEIGHT);
    Rect::from_min_size(
        Pos2::new(
            layout.version_trigger.right() - width,
            layout.version_trigger.bottom() + 9.0,
        ),
        Vec2::new(width, height),
    )
}

fn resource_version_menu_rect(anchor: Rect, screen: Rect) -> Rect {
    let width = 310.0_f32.min(screen.width() - 24.0);
    let height = 356.0_f32.min(screen.height() - 24.0);
    let x = anchor
        .right()
        .min(screen.right() - width - 12.0)
        .max(screen.left() + 12.0);
    let below = anchor.bottom() + 8.0;
    let y = if below + height <= screen.bottom() - 12.0 {
        below
    } else {
        (anchor.top() - height - 8.0).max(screen.top() + 12.0)
    };
    Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, height))
}

fn favorite_editor_rect(screen: Rect) -> Rect {
    Rect::from_center_size(
        screen.center(),
        Vec2::new(440.0_f32.min(screen.width() - 30.0), 338.0),
    )
}

fn launch_more_rect(layout: &Layout) -> Rect {
    let dock = Rect::from_min_max(
        Pos2::new(layout.hero.left() + 19.0, layout.hero.bottom() - 96.0),
        Pos2::new(layout.hero.right() - 19.0, layout.hero.bottom() - 18.0),
    );
    Rect::from_center_size(
        Pos2::new(dock.right() - 295.0, dock.center().y),
        Vec2::new(34.0, 34.0),
    )
}

fn launch_settings_card_rect(layout: &Layout) -> Rect {
    let size = Vec2::new(380.0, 310.0);
    let more = launch_more_rect(layout);
    let top = (more.top() - size.y - 20.0).max(layout.header.bottom() + 10.0);
    let right = layout.hero.right() - 34.0;
    Rect::from_min_size(Pos2::new(right - size.x, top), size)
}

#[derive(Clone, Copy)]
enum WindowAction {
    Minimize,
    Maximize,
    Close,
}

impl WindowAction {
    const ALL: [Self; 3] = [Self::Minimize, Self::Maximize, Self::Close];
}

#[derive(Clone, Copy)]
enum NavKind {
    Home,
    Versions,
    Download,
    Community,
    Create,
}

impl NavKind {
    fn label(self) -> &'static str {
        match self {
            Self::Home => "游戏",
            Self::Versions => "版本",
            Self::Download => "下载",
            Self::Community => "收藏",
            Self::Create => "创造",
        }
    }
}

#[derive(Clone, Copy)]
enum NavIcon {
    Home,
    Layers,
    Download,
    Search,
    Box,
    Settings,
    Monitor,
    Chevron,
    Sparkles,
    User,
    Folder,
    Logout,
    Close,
}

impl From<NavKind> for NavIcon {
    fn from(value: NavKind) -> Self {
        match value {
            NavKind::Home => Self::Home,
            NavKind::Versions => Self::Layers,
            NavKind::Download => Self::Download,
            NavKind::Community => Self::Sparkles,
            NavKind::Create => Self::Box,
        }
    }
}

fn draw_icon(painter: &Painter, icon: NavIcon, rect: Rect, color: Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height());
    let stroke = Stroke::new(1.65, color);
    match icon {
        NavIcon::Home => {
            painter.line(
                vec![
                    c + Vec2::new(-s * 0.38, -s * 0.04),
                    c + Vec2::new(0.0, -s * 0.38),
                    c + Vec2::new(s * 0.38, -s * 0.04),
                ],
                stroke,
            );
            painter.rect_stroke(
                Rect::from_center_size(c + Vec2::new(0.0, s * 0.10), Vec2::new(s * 0.62, s * 0.52)),
                2,
                stroke,
                StrokeKind::Inside,
            );
        }
        NavIcon::Layers => {
            painter.add(Shape::convex_polygon(
                vec![
                    c + Vec2::new(0.0, -s * 0.35),
                    c + Vec2::new(s * 0.38, -s * 0.13),
                    c + Vec2::new(0.0, s * 0.09),
                    c + Vec2::new(-s * 0.38, -s * 0.13),
                ],
                Color32::TRANSPARENT,
                stroke,
            ));
            painter.line(
                vec![
                    c + Vec2::new(-s * 0.35, s * 0.03),
                    c + Vec2::new(0.0, s * 0.23),
                    c + Vec2::new(s * 0.35, s * 0.03),
                ],
                stroke,
            );
            painter.line(
                vec![
                    c + Vec2::new(-s * 0.35, s * 0.21),
                    c + Vec2::new(0.0, s * 0.41),
                    c + Vec2::new(s * 0.35, s * 0.21),
                ],
                stroke,
            );
        }
        NavIcon::Download => {
            painter.line(
                vec![c + Vec2::new(0.0, -s * 0.40), c + Vec2::new(0.0, s * 0.20)],
                stroke,
            );
            painter.line(
                vec![
                    c + Vec2::new(-s * 0.22, s * 0.00),
                    c + Vec2::new(0.0, s * 0.23),
                    c + Vec2::new(s * 0.22, s * 0.00),
                ],
                stroke,
            );
            painter.line(
                vec![
                    c + Vec2::new(-s * 0.38, s * 0.30),
                    c + Vec2::new(-s * 0.38, s * 0.42),
                    c + Vec2::new(s * 0.38, s * 0.42),
                    c + Vec2::new(s * 0.38, s * 0.30),
                ],
                stroke,
            );
        }
        NavIcon::Search => {
            painter.circle_stroke(c + Vec2::new(-s * 0.10, -s * 0.10), s * 0.27, stroke);
            painter.line_segment(
                [
                    c + Vec2::new(s * 0.12, s * 0.12),
                    c + Vec2::new(s * 0.37, s * 0.37),
                ],
                stroke,
            );
        }
        NavIcon::Box => {
            painter.add(Shape::convex_polygon(
                vec![
                    c + Vec2::new(0.0, -s * 0.38),
                    c + Vec2::new(s * 0.38, -s * 0.18),
                    c + Vec2::new(0.0, s * 0.02),
                    c + Vec2::new(-s * 0.38, -s * 0.18),
                ],
                Color32::TRANSPARENT,
                stroke,
            ));
            painter.line(
                vec![
                    c + Vec2::new(-s * 0.38, -s * 0.18),
                    c + Vec2::new(-s * 0.38, s * 0.23),
                    c + Vec2::new(0.0, s * 0.44),
                    c + Vec2::new(s * 0.38, s * 0.23),
                    c + Vec2::new(s * 0.38, -s * 0.18),
                    c + Vec2::new(0.0, s * 0.02),
                    c + Vec2::new(0.0, s * 0.44),
                ],
                stroke,
            );
        }
        NavIcon::Settings => {
            painter.circle_stroke(c, s * 0.20, stroke);
            for index in 0..8 {
                let angle = std::f32::consts::TAU * index as f32 / 8.0;
                let direction = Vec2::angled(angle);
                painter.line_segment([c + direction * s * 0.29, c + direction * s * 0.41], stroke);
            }
            painter.circle_stroke(c, s * 0.31, stroke);
        }
        NavIcon::Monitor => {
            painter.rect_stroke(
                Rect::from_center_size(
                    c + Vec2::new(0.0, -s * 0.08),
                    Vec2::new(s * 0.76, s * 0.55),
                ),
                2,
                stroke,
                StrokeKind::Inside,
            );
            painter.line_segment(
                [c + Vec2::new(0.0, s * 0.20), c + Vec2::new(0.0, s * 0.36)],
                stroke,
            );
            painter.line_segment(
                [
                    c + Vec2::new(-s * 0.20, s * 0.37),
                    c + Vec2::new(s * 0.20, s * 0.37),
                ],
                stroke,
            );
        }
        NavIcon::Chevron => {
            painter.line(
                vec![
                    c + Vec2::new(-s * 0.22, -s * 0.10),
                    c + Vec2::new(0.0, s * 0.12),
                    c + Vec2::new(s * 0.22, -s * 0.10),
                ],
                stroke,
            );
        }
        NavIcon::Sparkles => {
            painter.line(
                vec![
                    c + Vec2::new(0.0, -s * 0.43),
                    c + Vec2::new(s * 0.11, -s * 0.11),
                    c + Vec2::new(s * 0.42, 0.0),
                    c + Vec2::new(s * 0.11, s * 0.11),
                    c + Vec2::new(0.0, s * 0.42),
                    c + Vec2::new(-s * 0.11, s * 0.11),
                    c + Vec2::new(-s * 0.42, 0.0),
                    c + Vec2::new(-s * 0.11, -s * 0.11),
                    c + Vec2::new(0.0, -s * 0.43),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    c + Vec2::new(s * 0.27, -s * 0.38),
                    c + Vec2::new(s * 0.27, -s * 0.18),
                ],
                Stroke::new(1.2, color),
            );
        }
        NavIcon::User => {
            painter.circle_stroke(c + Vec2::new(0.0, -s * 0.17), s * 0.18, stroke);
            painter.line(
                vec![
                    c + Vec2::new(-s * 0.34, s * 0.37),
                    c + Vec2::new(-s * 0.28, s * 0.13),
                    c + Vec2::new(0.0, s * 0.03),
                    c + Vec2::new(s * 0.28, s * 0.13),
                    c + Vec2::new(s * 0.34, s * 0.37),
                ],
                stroke,
            );
        }
        NavIcon::Folder => {
            painter.line(
                vec![
                    c + Vec2::new(-s * 0.40, -s * 0.24),
                    c + Vec2::new(-s * 0.12, -s * 0.24),
                    c + Vec2::new(-s * 0.02, -s * 0.12),
                    c + Vec2::new(s * 0.38, -s * 0.12),
                    c + Vec2::new(s * 0.38, s * 0.30),
                    c + Vec2::new(-s * 0.40, s * 0.30),
                    c + Vec2::new(-s * 0.40, -s * 0.24),
                ],
                stroke,
            );
        }
        NavIcon::Logout => {
            painter.line(
                vec![
                    c + Vec2::new(-s * 0.36, -s * 0.34),
                    c + Vec2::new(s * 0.08, -s * 0.34),
                    c + Vec2::new(s * 0.08, s * 0.34),
                    c + Vec2::new(-s * 0.36, s * 0.34),
                    c + Vec2::new(-s * 0.36, s * 0.16),
                ],
                stroke,
            );
            painter.line(
                vec![c + Vec2::new(-s * 0.10, 0.0), c + Vec2::new(s * 0.40, 0.0)],
                stroke,
            );
            painter.line(
                vec![
                    c + Vec2::new(s * 0.23, -s * 0.16),
                    c + Vec2::new(s * 0.40, 0.0),
                    c + Vec2::new(s * 0.23, s * 0.16),
                ],
                stroke,
            );
        }
        NavIcon::Close => {
            painter.line_segment(
                [
                    c + Vec2::new(-s * 0.28, -s * 0.28),
                    c + Vec2::new(s * 0.28, s * 0.28),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    c + Vec2::new(s * 0.28, -s * 0.28),
                    c + Vec2::new(-s * 0.28, s * 0.28),
                ],
                stroke,
            );
        }
    }
}

fn paint_window_icon(painter: &Painter, center: Pos2, action: WindowAction, color: Color32) {
    let stroke = Stroke::new(1.0, color);
    match action {
        WindowAction::Minimize => {
            painter.line_segment(
                [center + Vec2::new(-5.0, 3.0), center + Vec2::new(5.0, 3.0)],
                stroke,
            );
        }
        WindowAction::Maximize => {
            painter.rect_stroke(
                Rect::from_center_size(center, Vec2::splat(9.0)),
                0,
                stroke,
                StrokeKind::Inside,
            );
        }
        WindowAction::Close => {
            painter.line_segment(
                [center + Vec2::new(-4.0, -4.0), center + Vec2::new(4.0, 4.0)],
                stroke,
            );
            painter.line_segment(
                [center + Vec2::new(4.0, -4.0), center + Vec2::new(-4.0, 4.0)],
                stroke,
            );
        }
    }
}

fn paint_avatar(painter: &Painter, rect: Rect, radius: u8) {
    theme::fill_round(painter, rect, radius, Color32::from_rgb(211, 160, 110));
    let hair_height = rect.height() * 0.30;
    painter.rect_filled(
        Rect::from_min_size(rect.left_top(), Vec2::new(rect.width(), hair_height)),
        0,
        Color32::from_rgb(66, 47, 34),
    );
    painter.rect_filled(
        Rect::from_min_size(
            rect.left_top() + Vec2::new(0.0, hair_height),
            Vec2::new(rect.width() * 0.16, rect.height() * 0.42),
        ),
        0,
        Color32::from_rgb(72, 50, 38),
    );
    let eye_size = Vec2::new(rect.width() * 0.14, rect.height() * 0.13);
    for eye_x in [0.27, 0.61] {
        painter.rect_filled(
            Rect::from_min_size(
                rect.left_top() + Vec2::new(rect.width() * eye_x, rect.height() * 0.46),
                eye_size,
            ),
            0,
            Color32::from_rgb(70, 55, 77),
        );
    }
    painter.rect_filled(
        Rect::from_min_size(
            rect.left_top() + Vec2::new(rect.width() * 0.39, rect.height() * 0.72),
            Vec2::new(rect.width() * 0.23, rect.height() * 0.08),
        ),
        0,
        Color32::from_rgb(112, 70, 51),
    );
    theme::stroke_round(painter, rect, radius, 1.0, theme::alpha(Color32::BLACK, 52));
}

fn paint_block(
    painter: &Painter,
    center: Pos2,
    size: f32,
    top: Color32,
    left: Color32,
    right: Color32,
) {
    let half_width = size * 0.45;
    let half_height = size * 0.24;
    let depth = size * 0.37;
    painter.add(Shape::convex_polygon(
        vec![
            center + Vec2::new(0.0, -half_height),
            center + Vec2::new(half_width, 0.0),
            center + Vec2::new(0.0, half_height),
            center + Vec2::new(-half_width, 0.0),
        ],
        top,
        Stroke::new(0.7, theme::white_alpha(36)),
    ));
    painter.add(Shape::convex_polygon(
        vec![
            center + Vec2::new(-half_width, 0.0),
            center + Vec2::new(0.0, half_height),
            center + Vec2::new(0.0, half_height + depth),
            center + Vec2::new(-half_width, depth),
        ],
        left,
        Stroke::NONE,
    ));
    painter.add(Shape::convex_polygon(
        vec![
            center + Vec2::new(half_width, 0.0),
            center + Vec2::new(0.0, half_height),
            center + Vec2::new(0.0, half_height + depth),
            center + Vec2::new(half_width, depth),
        ],
        right,
        Stroke::NONE,
    ));
}
