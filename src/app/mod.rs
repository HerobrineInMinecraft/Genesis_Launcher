//! 方块启动器（Genesis Launcher）的顶层应用模块。
//!
//! 本模块是整个启动器的"外壳"与"中枢"，承载 eframe 的 [`App`] 实现 [`LauncherApp`]。
//! [`LauncherApp`] 持有全部 UI 状态、动画状态、网络请求接收器（mpsc receiver）、
//! 本地设置（[`LauncherSettings`]）以及游戏版本 / 新闻 / 资源 / 更新等数据，
//! 并通过 `mod.rs` 中分发的子模块（shell、pages、layout、tasks 等）完成界面绘制与交互。
//!
//! 本文件（`src/app/mod.rs`）主要负责：
//! - 定义 [`LauncherApp`] 主状态结构体（约 200 个字段，覆盖所有页面与弹窗的瞬时状态）；
//! - 定义持久化设置类型 [`LauncherSettings`] 及其从 `settings.ini` 的读写、迁移逻辑；
//! - 定义若干用于界面/皮肤渲染的辅助数据结构（如 [`SkinCuboid`]、[`SkinPreviewFace`]）；
//! - 定义页面的枚举（[`Page`]）、弹窗/菜单枚举（[`Menu`]）、内容分类枚举
//!   （[`VersionContentKind`]）等导航与状态枚举；
//! - 提供一批自由函数：ISO 时间戳解析、设置路径、皮肤/收藏图标加载、外观模式解析等。
//!
//! 大部分具体绘制逻辑位于子模块（pages / shell / layout / ui_helpers 等），
//! 这里集中管理状态与少量纯计算/IO 辅助函数。
//!
//! 注意：本文件体积极大，所有 UI 状态都集中在 [`LauncherApp`] 这一个结构体中，
//! 以便各子模块通过 `&mut self` / `&self` 共享访问，避免复杂的状态拆分。

#![allow(clippy::needless_borrow)]

use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::animation::{
    CardMotion, animation_strength, card_text_opacity, configure_animation_strength,
    outgoing_page_opacity, popup_opacity, spring_popup, spring_smooth,
    version_content_text_opacity,
};
use crate::exporter::{
    ModpackExportFormat, VersionExportOptions, export_version_archive, pick_export_file,
    sanitize_filename, start_modpack_export_worker,
};
use eframe::egui::{
    self, Color32, CursorIcon, Id, Painter, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Vec2,
};
use eframe::{App, CreationContext, Frame};

use crate::brand;
use crate::game::JavaRuntime;
use crate::hero::{
    DEFAULT_HERO_TITLE, default_hero_title_char_colors, normalize_hero_title,
    paint_styled_hero_title_line, parse_rgb_color, parse_rgb_list, parse_u16_list,
    styled_hero_title_lines,
};
use crate::loader::{LoaderInstallUpdate, LoaderKind, display_version as loader_display_version};
use crate::news::MinecraftNews;
use crate::resources::{ResourceCategory, ResourceProject};
use crate::scene;
use crate::sessions::{RecentSession, SessionKind};
use crate::skin::{
    default_skin_pixels, detect_slim_skin, install_local_skin_resource_pack, load_player_skin,
    pick_image_file, save_player_skin,
};
use crate::theme;
use crate::update::{self, LauncherRelease};
use crate::vector;
use crate::versions::{GameVersion, available_versions};

// 以下是 app 模块的子模块（均为私有），各自负责界面某一侧面：
// animations：动画插值/弹簧运动工具；avatars：玩家头像相关；
// bootstrap：首次启动初始化；icons：内置图标资源；
// input：键盘/输入快捷键处理；layout：布局测量辅助；
// pages：各页面的具体绘制；settings：设置页相关枚举/类型；
// shell：窗口外壳与整体框架绘制；skin_rendering：皮肤 3D 预览渲染；
// tasks：后台任务（下载/安装/导出等）状态机；tests：单元测试；
// ui_helpers：通用 UI 绘制小工具；utils：杂项工具函数。
mod animations;
mod avatars;
mod background;
mod bootstrap;
mod error_log;
mod icons;
mod input;
mod layout;
mod memory;
mod pages;
mod settings;
mod shell;
mod skin_rendering;
mod tasks;
#[cfg(test)]
mod tests;
mod ui_helpers;
mod utils;
mod version_name;

// 将常用子模块的内容直接引入当前作用域，避免在各处写冗长的路径前缀。
use avatars::*;
use background::*;
use error_log::*;
use icons::*;
use layout::*;
use memory::*;
use settings::{AnimationLevel, SettingsSection};
use skin_rendering::*;
use ui_helpers::*;
use utils::*;
use version_name::*;

// ===== 以下为界面布局相关的尺寸常量（单位：逻辑像素 / 设计坐标系）=====
// 这些常量统一控制窗口标题栏、左侧导航栏、内容区留白、版本列表行高、
// 各卡片尺寸等，便于在整個 UI 中保持一致的视觉间距。

/// 提供 TITLEBARHEIGHT 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const TITLEBAR_HEIGHT: f32 = 42.0; // 自定义无边框窗口顶部标题栏的高度
/// 提供 RAILWIDTH 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const RAIL_WIDTH: f32 = 86.0; // 左侧导航栏（rail）的宽度
/// 提供 内容PAD 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const CONTENT_PAD: f32 = 23.0; // 内容区四周的内边距
/// 提供 版本ROWHEIGHT 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const VERSION_ROW_HEIGHT: f32 = 51.5; // 版本列表中每一行的高度
/// 提供 版本菜单MAXHEIGHT 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const VERSION_MENU_MAX_HEIGHT: f32 = 430.0; // 版本下拉菜单的最大展开高度（超过则内部滚动）
/// 提供 RECENT卡片SIZE 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const RECENT_CARD_SIZE: Vec2 = Vec2::new(246.0, 187.0); // 首页"最近游玩"卡片的固定尺寸
/// 提供 SETTINGSITEMSPACING 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const SETTINGS_ITEM_SPACING: f32 = 49.0; // 设置页各分组项之间的纵向间距
/// 提供 SETTING卡片PADDING 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const SETTING_CARD_PADDING_X: f32 = 14.0; // 设置卡片内部的横向内边距
/// 设置卡片内容首行距离行顶部的偏移。
///
/// 卡片自身在行内上下缩进 4.5px，加上 14px 内容内边距后得到该值。
const SETTING_CARD_CONTENT_TOP_OFFSET: f32 = 18.5;
/// 提供 内容关闭BOUNCESECONDS 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const CONTENT_CLOSE_BOUNCE_SECONDS: f32 = 0.18; // 内容弹窗关闭时回弹动画的持续时间（秒）
/// 提供 皮肤HISTORYLIMIT 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const SKIN_HISTORY_LIMIT: usize = 64; // 皮肤编辑撤销/重做历史栈的最大条目数
/// 提供 SETTINGSREVISION 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const SETTINGS_REVISION: u32 = 2; // 当前 settings.ini 文件格式的版本号，用于迁移判断
// 皮肤 PNG 通过 `ColorImage::from_rgba_unmultiplied` 以左上角为原点上传，
// 图集 UV 与 egui 纹理 V 轴方向一致，因此正常绘制不能做全局 `1-v` 翻转。
// 该开关只用于兼容将来可能出现的左下原点纹理源；当前图像管线必须保持 false。
const SKIN_UV_Y_FLIP: bool = false;
// 反投影得到的归一化坐标允许极小的浮点越界，避免面边界上的点击被误判为面外。
const UV_BOUNDARY_EPSILON: f32 = 1e-5;
// 两基向量构成的平行四边形面积低于该阈值时，逆变换不可靠，应直接裁掉该退化面。
const UV_DEGENERATE_EPSILON: f32 = 1e-6;

/// 把 Minecraft 皮肤图集的归一化 UV 转换为 egui 纹理坐标系。
///
/// 把皮肤图集 UV 约束到 egui 纹理坐标范围。
///
/// 当前皮肤的 RGBA 数据以左上角为原点上传到 egui，图集 Y 与纹理 V 同向，
/// 因此默认不翻转。3D 网格采样和鼠标反投影仍共用此函数，确保两侧方向一致。
fn map_skin_atlas_uv_to_egui(uv: Pos2) -> Pos2 {
    let texture_y = if SKIN_UV_Y_FLIP { 1.0 - uv.y } else { uv.y };
    Pos2::new(uv.x.clamp(0.0, 1.0), texture_y.clamp(0.0, 1.0))
}

// 记录一条"最近游玩过的游戏版本"，用于在设置/首页展示最近游玩列表。
#[derive(Clone, PartialEq, Eq)]
/// 定义 RecentPlay 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
struct RecentPlay {
    version: String, // 版本标识（profile 名或版本目录名）
    played_at: u64,  // 上次游玩的时间戳（Unix 秒），用于排序与去重
}

// 表示一个用户收藏的网站（社区页"收藏"功能）。
#[derive(Clone, PartialEq, Eq)]
/// 定义 FavoriteSite 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
struct FavoriteSite {
    title: String,       // 收藏项展示用的标题
    url: String,         // 点击后打开的网址
    icon_source: String, // 图标来源：可能是内置图标 key（如 "favorite-icon:minecraft"）或图片 URL
}

// 后台线程用于把"收藏站点图标"下载结果回传给主线程的通道接收端。
// 元组为 (图标 URL / 标识, 下载得到的 PNG 字节)；主线程据此生成 egui 纹理。
/// 声明 FavoriteIconReceiver 的类型别名。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
type FavoriteIconReceiver = std::sync::mpsc::Receiver<Vec<(String, Vec<u8>)>>;

// 构建"版本 -> 最近游玩时间戳"的映射表，供版本列表按热度排序使用。
// 数据源有两类：① 启动器自身记录的最近游玩（settings.recent_versions）；
// ② 若提供了官方游戏目录，则额外读取官方启动器 profiles 记录，取两者时间戳的较大值。
// 参数：settings 为当前设置；game_directory 为可选的游戏实例目录（None 表示只使用自身记录）。
// 返回：版本标识到 Unix 秒时间戳的映射（越大表示越近）。
/// 实现 版本recency映射 相关的业务流程。
///
/// # 参数
/// - `settings`：由调用方传入的settings。
/// - `game_directory`：由调用方传入的游戏目录。
/// # 返回
/// 返回 `HashMap<String, u64>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn version_recency_map(
    settings: &LauncherSettings,
    game_directory: Option<&std::path::Path>,
) -> HashMap<String, u64> {
    // 先放入启动器自身记录的最近游玩时间戳
    let mut recency: HashMap<String, u64> = settings
        .recent_versions
        .iter()
        .map(|recent| (recent.version.clone(), recent.played_at))
        .collect();
    // 若有官方游戏目录，再合并官方启动器 profiles 中的 lastUsed 时间戳，取较大值
    if let Some(game_directory) = game_directory {
        for (version, played_at) in official_profile_recency(game_directory) {
            recency
                .entry(version)
                // 已记录则保留更近的时间戳；否则插入官方记录
                .and_modify(|known| *known = (*known).max(played_at))
                .or_insert(played_at);
        }
    }
    recency
}

// 计算版本下拉菜单中版本的展示顺序（返回的是下标序列，而非版本本身）。
// 排序规则：最近游玩时间戳大的排前面（降序）；时间戳相同则按下标升序，保证稳定。
// 参数：versions 为待排序的版本列表；recency 为 version_recency_map 生成的映射。
// 返回：排序后的版本下标数组。
/// 实现 版本菜单order 相关的业务流程。
///
/// # 参数
/// - `versions`：由调用方传入的版本。
/// - `recency`：由调用方传入的recency。
/// # 返回
/// 返回 `Vec<usize>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn version_menu_order(versions: &[GameVersion], recency: &HashMap<String, u64>) -> Vec<usize> {
    // 初始化为 0..N 的下标序列
    let mut order: Vec<usize> = (0..versions.len()).collect();
    order.sort_by(|left, right| {
        // 先按"最近游玩时间戳"降序：用右减左实现从大到小
        recency
            .get(&versions[*right].version)
            .copied()
            .unwrap_or(0)
            .cmp(&recency.get(&versions[*left].version).copied().unwrap_or(0))
            // 时间戳相同（或都没有记录）时按下标升序，维持稳定排序
            .then_with(|| left.cmp(right))
    });
    order
}

// 读取官方 Minecraft 启动器的 profiles 文件，提取每个 profile 的"最后使用版本"与"最后使用时间"。
// 解析失败（文件不存在 / JSON 非法）时返回空映射，调用方会回退到只用启动器自身记录。
// 参数：game_directory 为官方游戏实例目录（其中包含 launcher_profiles.json）。
// 返回：版本标识 -> Unix 秒时间戳的映射。
/// 实现 official配置recency 相关的业务流程。
///
/// # 参数
/// - `game_directory`：由调用方传入的游戏目录。
/// # 返回
/// 返回 `HashMap<String, u64>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn official_profile_recency(game_directory: &std::path::Path) -> HashMap<String, u64> {
    // 读取官方启动器的 profiles JSON；读不到或不是合法 UTF-8 文本则跳过
    let Ok(contents) = std::fs::read_to_string(game_directory.join("launcher_profiles.json"))
    else {
        return HashMap::new();
    };
    // 反序列化为松散的 JSON 值，便于按字段名动态取值；解析失败返回空
    let Ok(profiles) = serde_json::from_str::<serde_json::Value>(&contents) else {
        return HashMap::new();
    };
    // 取顶层 "profiles" 对象，逐个 profile 提取 lastVersionId 与 lastUsed
    profiles
        .get("profiles")
        .and_then(serde_json::Value::as_object)
        .map(|profiles| {
            profiles
                .values()
                .filter_map(|profile| {
                    // 取出该 profile 最后使用的版本 id 与最后使用时间（ISO 字符串），缺失则跳过
                    let version = profile.get("lastVersionId")?.as_str()?.to_owned();
                    let played_at = parse_iso_timestamp(profile.get("lastUsed")?.as_str()?)?;
                    Some((version, played_at))
                })
                .collect()
        })
        .unwrap_or_default()
}

// 将官方启动器 profiles 中的 ISO 8601 时间戳（如 "2023-05-12T08:30:00+08:00" 或带 Z / UTC）解析为 Unix 秒。
// 官方 lastUsed 字段格式为 `YYYY-MM-DDTHH:MM:SS` 后可选跟时区偏移（+HH:MM 或 -HH:MM）。
// 解析失败或字段非法时返回 None。内部采用基于"纪元日"的日历算法，避免依赖 chrono。
// 参数：value 为待解析的时间字符串。返回：自 1970-01-01 起的秒数，或 None。
/// 实现 解析isotimestamp 相关的业务流程。
///
/// # 参数
/// - `value`：由调用方传入的value。
/// # 返回
/// 返回 `Option<u64>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn parse_iso_timestamp(value: &str) -> Option<u64> {
    // 至少需要到秒的 19 个字符（YYYY-MM-DDTHH:MM:SS），否则无法解析
    if value.len() < 19 {
        return None;
    }
    // 按固定位置切片取出年/月/日/时/分/秒（下标对应 ISO 格式的固定位）
    let year = value.get(0..4)?.parse::<i64>().ok()?;
    let month = value.get(5..7)?.parse::<i64>().ok()?;
    let day = value.get(8..10)?.parse::<i64>().ok()?;
    let hour = value.get(11..13)?.parse::<i64>().ok()?;
    let minute = value.get(14..16)?.parse::<i64>().ok()?;
    let second = value.get(17..19)?.parse::<i64>().ok()?;
    // 基础值域校验：月份 1–12、日期 1–31、时≤23、分≤59、秒≤60（含闰秒余量）
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    // 采用"以 3 月为岁首"的算法：1、2 月折算到上一年，便于计算闰年
    let adjusted_year = year - i64::from(month <= 2);
    // 400 年为一个大周期（gregorian cycle），先分离出"纪元"era 与"纪元内年份"
    let era = if adjusted_year >= 0 {
        adjusted_year
    } else {
        // 处理公元前的舍入
        adjusted_year - 399
    } / 400;
    let year_of_era = adjusted_year - era * 400;
    // 把月份平移到以 3 月为第 0 月（3→0, 4→1, …, 1 月/2 月→9/10）
    let shifted_month = month + if month > 2 { -3 } else { 9 };
    // 由"纪元内月序"推出当前是纪元内的第几天
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    // 纪元内总天数 = 平年天数 + 闰年余数（每 4 一闰、每 100 不闰、每 400 再闰）
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    // 总天数换算成秒，再加上当天的时分秒；719_468 是把纪元起点对齐到 1970-01-01 的偏移
    let mut timestamp =
        (era * 146_097 + day_of_era - 719_468) * 86_400 + hour * 3_600 + minute * 60 + second;
    // 解析可选的时区偏移部分（从下标 19 起），形如 +08:00 / -05:00
    let timezone = value.get(19..).unwrap_or("");
    if timezone.starts_with('+') || timezone.starts_with('-') {
        // 取出偏移的小时与分钟（如 "+08:00" -> 小时 08，分钟 00）
        let offset_hour = timezone.get(1..3)?.parse::<i64>().ok()?;
        let offset_minute = timezone.get(4..6)?.parse::<i64>().ok()?;
        let offset = offset_hour * 3_600 + offset_minute * 60;
        // 时间戳是 UTC 基准；偏移为 + 表示本地比 UTC 早，需要减去偏移还原成 UTC 秒
        timestamp += if timezone.starts_with('+') {
            -offset
        } else {
            offset
        };
    }
    // 最终转换为 u64（理论上不会为负，但防御性转换）
    u64::try_from(timestamp).ok()
}

// 顶部/标题栏区域的浮层菜单状态：当前是否有菜单打开，以及是哪一种。
#[derive(Clone, Copy, PartialEq, Eq)]
/// 定义 菜单 可选状态或事件集合。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
enum Menu {
    None,    // 没有打开任何浮层菜单
    Version, // 打开"版本选择"下拉菜单
    Profile, // 打开"玩家档案/账户"下拉菜单
}

// 启动器的一级页面（侧边导航栏切换的主视图）。
#[derive(Clone, Copy, PartialEq, Eq)]
/// 定义 Page 可选状态或事件集合。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
enum Page {
    Home,      // 首页：最近游玩、新闻、收藏等概览
    Versions,  // 版本库：本机已安装的游戏版本列表与管理
    Download,  // 下载：官方版本与社区资源（Mod/资源包等）浏览与安装
    Community, // 社区：新闻、收藏站点等
    Create,    // 创建：制作 Mod 工程包、皮肤等
    Settings,  // 设置：启动器各项配置
}

// 设置页的视图状态：记录当前小节与用于入场/切换的卡片位移动画。
#[derive(Clone, Copy)]
/// 定义 SettingsView 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
struct SettingsView {
    section: SettingsSection,   // 当前显示的设置小节（来自 settings 子模块）
    chrome_motion: CardMotion,  // 设置页外层"框架/侧栏"的入场运动（弹簧动画状态）
    content_motion: CardMotion, // 设置页内容卡片的入场运动
    draw_chrome: bool,          // 是否绘制外层框架（用于离场动画期间先隐藏）
    draw_footer: bool,          // 是否绘制页脚
}

// 下载页中"官方版本详情"面板（点击某个官方版本后滑入的详情视图）。
#[derive(Clone)]
/// 定义 DownloadDetailView 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
struct DownloadDetailView {
    version: crate::download::OfficialVersion, // 被查看的官方版本元数据
    motion: CardMotion,                        // 详情面板滑入/滑出的位移动画状态
    active: bool,                              // 该详情视图当前是否处于激活（可见）状态
}

// 下载页中"社区资源项目详情"面板（点击某个 Mod/资源包项目后滑入的详情视图）。
#[derive(Clone)]
/// 定义 ResourceDetailView 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
struct ResourceDetailView {
    project: Option<ResourceProject>, // 被查看的资源项目（None 表示尚未加载或已关闭）
    motion: CardMotion,               // 详情面板滑入/滑出位移动画状态
    active: bool,                     // 该详情视图当前是否处于激活状态
}

// 版本库中某个"已安装版本详情"面板（点击版本行后滑入的详情视图）。
#[derive(Clone)]
/// 定义 VersionDetailView 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
struct VersionDetailView {
    index: usize,         // 该版本在版本列表中的下标
    version: GameVersion, // 被查看的已安装版本元数据
    motion: CardMotion,   // 详情面板滑入/滑出位移动画状态
    active: bool,         // 该详情视图当前是否处于激活状态
}

// 侧边导航栏按钮的悬浮提示（tooltip）状态：记录当前高亮的按钮与其矩形。
#[derive(Clone, Copy)]
/// 定义 NavTooltip 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
struct NavTooltip {
    id: u8,              // 提示唯一标识（对应某个导航按钮），用于区分动画目标
    label: &'static str, // 提示文本（按钮名称）
    button: Rect,        // 对应按钮在屏幕上的矩形区域，用于定位提示气泡
}

// 版本列表的筛选方式（用于"版本库"与"官方下载"两处）。
#[derive(Clone, Copy, PartialEq, Eq)]
/// 定义 VersionFilter 可选状态或事件集合。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
enum VersionFilter {
    All,      // 全部版本（含正式版与快照等）
    Release,  // 仅正式版（stable release）
    Snapshot, // 仅快照 / 预览版
}

// 版本详情中"内容管理"面板的分类标签页：每个标签对应游戏目录下的一个子文件夹/功能。
#[derive(Clone, Copy, PartialEq, Eq)]
/// 定义 VersionContentKind 可选状态或事件集合。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
enum VersionContentKind {
    Mods,          // 模组（mods 文件夹）
    Saves,         // 存档（saves 文件夹）
    ShaderPacks,   // 光影包（shaderpacks 文件夹）
    ResourcePacks, // 材质包（resourcepacks 文件夹）
    Datapacks,     // 数据包（datapacks 文件夹）
    Export,        // 导出：把该版本打包成整合包（无对应文件夹）
}

impl VersionContentKind {
    // 返回该内容分类对应的游戏目录子文件夹名；Export 无文件夹，返回空串。
    /// 实现 folder 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `&'static str`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn folder(self) -> &'static str {
        match self {
            Self::Mods => "mods",
            Self::Saves => "saves",
            Self::ShaderPacks => "shaderpacks",
            Self::ResourcePacks => "resourcepacks",
            Self::Datapacks => "datapacks",
            Self::Export => "", // 导出是功能入口而非文件夹
        }
    }

    // 返回该分类在标签页上展示的中文名称。
    /// 实现 标签 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `&'static str`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn label(self) -> &'static str {
        match self {
            Self::Mods => "模组",
            Self::Saves => "存档",
            Self::ShaderPacks => "光影",
            Self::ResourcePacks => "材质包",
            Self::Datapacks => "数据包",
            Self::Export => "导出",
        }
    }

    // 返回该分类在标签栏中的固定下标（0..5），用于按列布局与动画。
    /// 实现 index 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `usize`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn index(self) -> usize {
        match self {
            Self::Mods => 0,
            Self::Saves => 1,
            Self::ShaderPacks => 2,
            Self::ResourcePacks => 3,
            Self::Datapacks => 4,
            Self::Export => 5,
        }
    }
}

// 版本内容管理面板中，某一内容分类（mods/saves/...）下的单个条目展示数据。
/// 定义 VersionContentEntry 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
struct VersionContentEntry {
    name: String,   // 条目名称（文件名或存档名等）
    detail: String, // 条目的附加说明（大小、修改时间等）
}

// "创建 Mod"工程向导中的用户输入数据（用于生成 mod 模板工程）。
#[derive(Clone)]
/// 定义 ModProjectSettings 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
struct ModProjectSettings {
    name: String,      // Mod 显示名称
    id: String,        // Mod 内部标识（modid）
    package: String,   // Java 包名（如 com.example.mymod）
    version: String,   // Mod 版本号
    minecraft: String, // 目标 Minecraft 版本
    loader: usize,     // 选用的加载器类型下标（对应 LoaderKind 列表）
}

// 玩家皮肤（Stevie 模型）可被拆分的身体部件，用于 3D 预览时按部件构建立方体。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// 定义 SkinPart 可选状态或事件集合。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
enum SkinPart {
    Head,     // 头
    Body,     // 躯干
    RightArm, // 右臂（玩家视角的右侧）
    LeftArm,  // 左臂
    RightLeg, // 右腿
    LeftLeg,  // 左腿
}

// 三维坐标点（皮肤 3D 模型使用的局部坐标，单位与皮肤像素无关、为抽象模型单位）。
#[derive(Clone, Copy, Debug)]
/// 定义 SkinPoint3 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
struct SkinPoint3 {
    x: f32,
    y: f32,
    z: f32,
}

impl SkinPoint3 {
    // 构造一个三维点（const 函数，可在编译期使用）。
    /// 提供 fn 使用的只读常量。
    ///
    /// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
    const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
}

// 皮肤模型中的一个长方体（box）：对应某个身体部件在模型空间内的包围盒与贴图位置。
#[derive(Clone, Copy, Debug)]
/// 定义 SkinCuboid 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
struct SkinCuboid {
    part: SkinPart,  // 该长方体属于哪个身体部件
    min: SkinPoint3, // 包围盒的最小角（模型坐标）
    max: SkinPoint3, // 包围盒的最大角
    uv_x: f32,       // 贴图（UV）起始横坐标（皮肤纹理像素）
    uv_y: f32,       // 贴图（UV）起始纵坐标
}

impl SkinCuboid {
    // 计算该长方体的边长（各轴 max - min），用于后续面生成与绘制。
    /// 实现 size 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `SkinPoint3`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn size(self) -> SkinPoint3 {
        SkinPoint3::new(
            self.max.x - self.min.x,
            self.max.y - self.min.y,
            self.max.z - self.min.z,
        )
    }
}

// 皮肤长方体被投影到屏幕后形成的"可见面"：含 4 个顶点、UV 矩形与法向量。
#[derive(Clone, Copy, Debug)]
/// 定义 SkinCuboidFace 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
struct SkinCuboidFace {
    face: usize,             // 面索引（0..5，标识长方体的哪个面）
    points: [SkinPoint3; 4], // 该面在模型空间的 4 个顶点（投影前）
    uv: Rect,                // 该面在皮肤纹理上对应的 UV 矩形（用于取色/贴图）
    normal: SkinPoint3,      // 该面的法向量，用于背面剔除与深度排序
}

// 皮肤编辑器的撤销/重做历史：分别用两个栈保存"编辑前像素"与"编辑后像素"。
#[derive(Default)]
/// 定义 SkinEditHistory 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
struct SkinEditHistory {
    undo: VecDeque<Vec<u8>>, // 撤销栈：保存每次编辑前的皮肤像素（RGBA 字节）
    redo: Vec<Vec<u8>>,      // 重做栈：保存被撤销掉的"编辑后"像素
}

impl SkinEditHistory {
    // 是否还能撤销（撤销栈非空）。
    /// 实现 canundo 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `bool`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    // 是否还能重做（重做栈非空）。
    /// 实现 canredo 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `bool`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    // 记录一次编辑：把编辑前像素压入撤销栈，并清空重做栈（产生新分支后无法再重做旧分支）。
    // 若前后像素完全相同则忽略（无实质改动，避免污染历史）。
    /// 实现 record 相关的业务流程。
    ///
    /// # 参数
    /// - `before`：由调用方传入的before。
    /// - `current`：由调用方传入的当前。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn record(&mut self, before: Vec<u8>, current: &[u8]) {
        if before.as_slice() == current {
            return;
        }
        // 超出历史上限时丢弃最旧的撤销记录
        if self.undo.len() >= SKIN_HISTORY_LIMIT {
            self.undo.pop_front();
        }
        self.undo.push_back(before);
        // 新编辑使既有的重做链失效
        self.redo.clear();
    }

    // 撤销：弹出最近一次"编辑前"像素作为结果，并把当前像素压入重做栈。
    /// 实现 undo 相关的业务流程。
    ///
    /// # 参数
    /// - `current`：由调用方传入的当前。
    /// # 返回
    /// 返回 `Option<Vec<u8>>`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn undo(&mut self, current: &[u8]) -> Option<Vec<u8>> {
        let previous = self.undo.pop_back()?;
        if self.redo.len() >= SKIN_HISTORY_LIMIT {
            self.redo.remove(0); // 重做栈也维持上限，丢弃最旧
        }
        self.redo.push(current.to_vec());
        Some(previous)
    }

    // 重做：弹出最近一次被撤销的"编辑后"像素作为结果，并把当前像素压回撤销栈。
    /// 实现 redo 相关的业务流程。
    ///
    /// # 参数
    /// - `current`：由调用方传入的当前。
    /// # 返回
    /// 返回 `Option<Vec<u8>>`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn redo(&mut self, current: &[u8]) -> Option<Vec<u8>> {
        let next = self.redo.pop()?;
        if self.undo.len() >= SKIN_HISTORY_LIMIT {
            self.undo.pop_front();
        }
        self.undo.push_back(current.to_vec());
        Some(next)
    }
}

// 用于唯一标识皮肤模型某部件的某个面（部件 + 面索引），作为预览面的去重/索引 key。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// 定义 SkinFaceKey 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
struct SkinFaceKey {
    part: SkinPart, // 所属身体部件
    face: usize,    // 该部件上的面索引
}

// 皮肤 3D 预览中一个已投影到二维屏幕的面：含屏幕上的 4 个顶点、UV 矩形、深度与着色。
#[derive(Clone, Debug)]
/// 定义 SkinPreviewFace 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
struct SkinPreviewFace {
    key: SkinFaceKey,  // 该面对应的部件/面 key
    points: [Pos2; 4], // 该面在屏幕（egui）坐标系下的 4 个顶点
    uv: Rect,          // 该面在皮肤纹理上对应的 UV 矩形
    depth: f32,        // 深度值，用于按从后到前的顺序绘制（画家算法）
    tint: Color32,     // 该面叠加的着色颜色（用于阴影/高光效果）
}

impl SkinPreviewFace {
    // 给定屏幕上的一个点，反算出它在皮肤纹理上的 UV 坐标（用于鼠标点选像素/取色）。
    // 用两条边构成的仿射变换求逆：u、v 为相对 (points[0], points[1], points[3]) 的重心参数。
    // 若面退化（行列式接近 0）或点在面外则返回 None。
    /// 实现 textureposition 相关的业务流程。
    ///
    /// # 参数
    /// - `point`：由调用方传入的point。
    /// # 返回
    /// 返回 `Option<Pos2>`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn texture_position(&self, point: Pos2) -> Option<Pos2> {
        // 面的两条基向量：水平边 (p0->p1) 与垂直边 (p0->p3)。
        // 输入点或顶点只要含 NaN/Infinity，都不能参与矩阵求逆，否则会污染后续像素索引。
        if !point.x.is_finite()
            || !point.y.is_finite()
            || self
                .points
                .iter()
                .any(|vertex| !vertex.x.is_finite() || !vertex.y.is_finite())
        {
            return None;
        }
        let horizontal = self.points[1] - self.points[0];
        let vertical = self.points[3] - self.points[0];
        // 计算由两基向量张成平行四边形的行列式（面积），接近 0 表示面退化不可解。
        // 除零保护和极小值保护必须同时存在：det 为 NaN 时普通比较不会进入错误分支。
        let determinant = horizontal.x * vertical.y - horizontal.y * vertical.x;
        if !determinant.is_finite() || determinant.abs() <= UV_DEGENERATE_EPSILON {
            return None;
        }
        // 待求点到起点的偏移向量。
        let offset = point - self.points[0];
        // 解线性方程组得到 uv 归一化坐标（范围 0..1 表示落在面内）。
        let u = (offset.x * vertical.y - offset.y * vertical.x) / determinant;
        let v = (horizontal.x * offset.y - horizontal.y * offset.x) / determinant;
        // 浮点求逆在边缘处可能得到 -1e-8 或 1.0000001；使用微小容差命中，
        // 但仍拒绝 NaN、无穷以及真正落在面外的坐标。
        if !u.is_finite()
            || !v.is_finite()
            || !(-UV_BOUNDARY_EPSILON..=1.0 + UV_BOUNDARY_EPSILON).contains(&u)
            || !(-UV_BOUNDARY_EPSILON..=1.0 + UV_BOUNDARY_EPSILON).contains(&v)
        {
            return None;
        }
        // 先把容差内越界值夹回 [0,1]，再把局部坐标映射到实际皮肤纹理矩形。
        let u = u.clamp(0.0, 1.0);
        let v = v.clamp(0.0, 1.0);
        let texture_x = self.uv.left() + self.uv.width() * u;
        let texture_y = self.uv.top() + self.uv.height() * v;
        if !texture_x.is_finite() || !texture_y.is_finite() {
            return None;
        }

        // 通过共享函数统一执行边界约束（必要时也统一处理纹理原点差异），
        // 保证网格采样、鼠标命中和像素写入使用完全相同的坐标方向。
        let uv = map_skin_atlas_uv_to_egui(Pos2::new(texture_x, texture_y));
        if !uv.x.is_finite() || !uv.y.is_finite() {
            return None;
        }

        // 返回前再次约束到纹理的归一化边界。调用方仍会把坐标乘 64 并向下取整，
        // 因此这里必须保证不会把 1.0 之外的异常值传给绘制像素索引。
        Some(uv)
    }
}

// Java 运行时选择器的目标范围：是给全局设置选，还是给某个版本实例单独选。
#[derive(Clone, PartialEq, Eq)]
/// 定义 JavaPickerTarget 可选状态或事件集合。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
enum JavaPickerTarget {
    Global,                                          // 全局默认 Java
    Version { directory: PathBuf, profile: String }, // 针对某个游戏目录下的特定版本（profile）
}

// 启动器的持久化设置：会从数据目录下的 `settings.ini` 读写，并支持从旧版文件迁移。
// 所有字段都带有合理的默认值（见 Default 实现）；load() 负责解析文本并做范围裁剪。
#[derive(Clone, PartialEq, Eq)]
/// 定义 LauncherSettings 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
struct LauncherSettings {
    memory_mb: u16,                         // 分配给游戏的最大内存（MB）
    game_directory: Option<PathBuf>,        // 当前选中的游戏实例目录（None 表示未指定）
    game_directories: Vec<PathBuf>,         // 已发现/已添加的全部游戏实例目录列表
    version_isolation: bool,                // 是否按版本隔离（.minecraft 独立目录）配置
    version_memory: Vec<String>, // 各版本单独的内存覆盖，格式 "目录\u{1f}profile\u{1f}内存"
    java_path: Option<PathBuf>,  // 全局默认 Java 可执行文件路径
    version_java: Vec<String>,   // 各版本单独 Java 覆盖，格式 "目录\u{1f}profile\u{1f}路径"
    version_names: Vec<String>,  // 各版本自定义显示名，格式 "目录\u{1f}profile\u{1f}名称"
    show_loader_versions: bool,  // 版本列表中是否显示加载器（Forge/Fabric 等）版本
    window_width: u32,           // 窗口宽度（像素）
    window_height: u32,          // 窗口高度（像素）
    fullscreen: bool,            // 是否全屏启动游戏
    extra_jvm_args: String,      // 额外的 JVM 启动参数
    player_name: String,         // 离线模式玩家名（默认 Steve）
    player_skin: Option<PathBuf>, // 玩家自定义皮肤图片路径
    player_skin_model_alex: bool, // 皮肤是否为细臂（Alex）模型
    notify_after_launch: bool,   // 启动后是否发出通知
    hide_after_launch: bool,     // 启动游戏后是否隐藏启动器
    animation_level: AnimationLevel, // 动画强度档位（来自 settings 子模块）
    tooltips: bool,              // 是否显示悬浮提示
    appearance_mode: theme::AppearanceMode, // 外观模式（简约/磨砂等）
    glass_transparency: u16,     // 玻璃（毛玻璃）透明度 0–100
    card_edge_clarity: u16,      // 卡片边缘清晰度 0–100
    corner_radius: u16,          // 窗口、卡片、按钮和输入框的统一圆角半径
    pointer_brightness: u16,     // 指针（光标光晕）亮度 0–200
    pointer_radius: u16,         // 指针光晕半径
    pointer_refresh_rate: u16,   // 指针刷新率（Hz）
    hero_title: String,          // 首页主标题文本
    hero_title_color: [u8; 3],   // 主标题整体颜色（RGB）
    hero_title_size: u16,        // 主标题字号
    hero_title_char_sizes: Vec<u16>, // 主标题逐字符字号（与字符数对齐）
    hero_title_char_colors: Vec<[u8; 3]>, // 主标题逐字符颜色
    background_image: Option<PathBuf>, // 用户导入的首页背景图片路径
    favorite_sites: Vec<FavoriteSite>, // 收藏站点列表（社区页）
    recent_versions: Vec<RecentPlay>, // 最近游玩版本列表
}

// 为设置提供出厂默认值。load() 会以 default() 为基底，再用读取到的 settings.ini 覆盖。
impl Default for LauncherSettings {
    /// 实现 默认 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `Self`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn default() -> Self {
        let memory_limit = memory_limit_mb();
        Self {
            // 默认分配 6GB 内存给游戏（主流配置下的稳妥值，之后会被 clamp 校验）
            memory_mb: 6144_u16.min(memory_limit),
            // 默认未指定游戏目录，等待自动发现逻辑填充
            game_directory: None,
            game_directories: Vec::new(),
            // 默认开启版本隔离（每个版本独立的 .minecraft 目录），避免互相污染
            version_isolation: true,
            version_memory: Vec::new(),
            java_path: None,
            version_java: Vec::new(),
            version_names: Vec::new(),
            // 默认不在版本列表中显示加载器版本（列表更简洁）
            show_loader_versions: false,
            // 默认窗口尺寸 1280x720（16:9 标准分辨率）
            window_width: 1280,
            window_height: 720,
            fullscreen: false,
            extra_jvm_args: String::new(),
            // 默认离线玩家名为官方经典角色名 Steve
            player_name: "Steve".to_owned(),
            player_skin: None,
            player_skin_model_alex: false,
            // 默认在游戏启动后弹出通知
            notify_after_launch: true,
            hide_after_launch: false,
            // 默认动画强度为中等档位
            animation_level: AnimationLevel::Medium,
            tooltips: true,
            // 默认外观为"简约"模式
            appearance_mode: theme::AppearanceMode::Simple,
            // 玻璃透明度默认 82（偏通透但仍可读）
            glass_transparency: 82,
            // 卡片边缘清晰度默认 55（兼顾柔和与辨识度）
            card_edge_clarity: 55,
            // 全部圆角表面与 Windows 窗口区域统一使用 16 逻辑像素
            corner_radius: theme::DEFAULT_CORNER_RADIUS as u16,
            // 指针光晕亮度默认 100（基准亮度）
            pointer_brightness: 100,
            // 指针光晕半径默认 155 逻辑像素
            pointer_radius: 155,
            // 指针刷新率默认 60Hz（与常见显示器刷新率一致）
            pointer_refresh_rate: 60,
            // 主标题取品牌默认文案，颜色为偏暖的米白色
            hero_title: DEFAULT_HERO_TITLE.to_owned(),
            hero_title_color: [245, 250, 238],
            // 主标题默认字号 58，逐字符字号全部初始化为同一值
            hero_title_size: 58,
            hero_title_char_sizes: vec![58; DEFAULT_HERO_TITLE.chars().count()],
            // 逐字符颜色取品牌预设的渐变配色（hero 模块提供）
            hero_title_char_colors: default_hero_title_char_colors(),
            background_image: None,
            favorite_sites: Vec::new(),
            recent_versions: Vec::new(),
        }
    }
}

// LauncherSettings 的行为方法：加载/保存、外观解析、版本级内存/Java 覆盖的增删查。
impl LauncherSettings {
    // 从磁盘加载设置。优先读取当前数据目录的 settings.ini；若不存在则尝试旧版（legacy）目录，
    // 并在加载成功后写回新位置完成迁移。若两者皆无，则仅做游戏目录自动发现后返回默认值。
    // 解析过程中会对所有数值做合法性校验与范围裁剪，保证字段始终有效。
    /// 实现 加载 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `Self`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn load() -> Self {
        let mut settings = Self::default();
        // 若数据目录下存在之前保存的玩家皮肤，则恢复为当前皮肤
        let recovered_skin = brand::data_directory().join("player-skin.png");
        if recovered_skin.is_file() {
            settings.player_skin = Some(recovered_skin);
        }
        // 确定从哪个路径加载设置：优先当前目录；否则旧版目录（需迁移）；再否则全新启动
        let current_path = settings_path();
        let legacy_path = legacy_settings_path();
        let (load_path, migrate_to_current) = if current_path.is_file() {
            (current_path, false)
        } else if legacy_path.is_file() {
            // 旧版目录存在：从它读取并在最后写回新位置以完成迁移
            (legacy_path, true)
        } else {
            // 完全没有设置文件：自动发现游戏目录后直接返回默认值
            settings.game_directories = crate::game::discover_game_directories();
            settings.game_directory = settings.game_directories.first().cloned();
            return settings;
        };
        // 读取设置文本；读不到则同样回退到自动发现目录的默认值
        let Ok(contents) = std::fs::read_to_string(load_path) else {
            settings.game_directories = crate::game::discover_game_directories();
            settings.game_directory = settings.game_directories.first().cloned();
            return settings;
        };
        // 以下临时变量用于在逐行解析过程中暂存"需要二次加工"的旧字段（兼容旧版与逐字符样式）
        let mut settings_revision = 0;
        let mut stored_liquid_glass = None;
        let mut stored_liquid_glass_mode = None;
        let mut stored_appearance_mode = None;
        let mut stored_animation_level = None;
        let mut stored_legacy_animations = None;
        let mut stored_hero_title_char_sizes = None;
        let mut stored_hero_title_char_colors = None;
        // 逐行解析 settings.ini：每行形如 "key=value"，空行或缺少 '=' 的行跳过
        for line in contents.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            // 按字段名分发处理；解析失败通常回退到已有（默认）值，保证健壮性
            match key {
                // 数字/布尔类字段：解析失败时保留默认值
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
                // 各版本单独的内存/Java 覆盖：用不可见的控制字符 \u{1f}（单元分隔符）作字段分隔，
                // 避免与文件路径中的常见字符冲突。格式为 "目录\u{1f}profile\u{1f}值"。
                "version_memory" => {
                    // 从右侧切出 memory 部分（profile 名可能含路径字符，故从右拆分更稳妥）
                    if let Some((profile, memory)) = value.rsplit_once('\u{1f}')
                        && let Ok(memory) = memory.parse::<u16>()
                    {
                        settings.version_memory.push(format!(
                            "{profile}\u{1f}{}",
                            memory.clamp(2048, memory_limit_mb())
                        ));
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
                "version_name" => {
                    if let Some((profile, name)) = value.rsplit_once('\u{1f}')
                        && !profile.is_empty()
                        && !name.trim().is_empty()
                    {
                        settings
                            .version_names
                            .push(format!("{profile}\u{1f}{}", name.trim()));
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
                "player_name" => {
                    if let Ok(name) = normalize_player_name(value) {
                        settings.player_name = name;
                    }
                }
                "player_skin" => {
                    if !value.is_empty() {
                        settings.player_skin = Some(PathBuf::from(value));
                    }
                }
                "player_skin_model_alex" => {
                    settings.player_skin_model_alex = value == "true";
                }
                "notify_after_launch" => settings.notify_after_launch = value == "true",
                "hide_after_launch" => settings.hide_after_launch = value == "true",
                // 动画档位：新字段直接映射为 AnimationLevel；旧字段 "animations"（布尔）稍后转换
                "animation_level" => {
                    stored_animation_level = match value {
                        "off" => Some(AnimationLevel::Off),
                        "weak" => Some(AnimationLevel::Weak),
                        "medium" => Some(AnimationLevel::Medium),
                        "strong" => Some(AnimationLevel::Strong),
                        _ => None,
                    }
                }
                "animations" => stored_legacy_animations = Some(value == "true"),
                "tooltips" => settings.tooltips = value == "true",
                "settings_revision" => {
                    settings_revision = value.parse().unwrap_or_default();
                }
                // 外观模式（新字段）。旧版的 liquid_glass* 仅在未设置新字段时作为回退
                "appearance_mode" => {
                    stored_appearance_mode = theme::AppearanceMode::from_stored(value)
                }
                "liquid_glass" => stored_liquid_glass = Some(value == "true"),
                "liquid_glass_mode" => {
                    stored_liquid_glass_mode = Some(value.to_owned());
                }
                "glass_transparency" => {
                    settings.glass_transparency =
                        value.parse().unwrap_or(settings.glass_transparency)
                }
                "card_edge_clarity" => {
                    settings.card_edge_clarity = value.parse().unwrap_or(settings.card_edge_clarity)
                }
                "corner_radius" => {
                    settings.corner_radius = value.parse().unwrap_or(settings.corner_radius)
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
                // 主标题：把保存时被转义的 "\n"（字面两字符）还原为真正换行，再做规范化
                "hero_title" => {
                    settings.hero_title = normalize_hero_title(&value.replace("\\n", "\n"))
                }
                "hero_title_color" => {
                    settings.hero_title_color =
                        parse_rgb_color(value).unwrap_or(settings.hero_title_color)
                }
                "hero_title_size" => {
                    settings.hero_title_size = value.parse().unwrap_or(settings.hero_title_size)
                }
                "hero_title_char_sizes" => stored_hero_title_char_sizes = Some(value.to_owned()),
                "hero_title_char_colors" => stored_hero_title_char_colors = Some(value.to_owned()),
                "background_image" => {
                    settings.background_image = (!value.is_empty()).then(|| PathBuf::from(value));
                }
                // 收藏站点：用 \u{1f} 分隔 标题/URL/图标来源 三个字段，交给专用解析函数
                "favorite_site" => {
                    if let Some(site) = parse_favorite_site(value) {
                        settings.favorite_sites.push(site);
                    }
                }
                // 最近游玩版本：以制表符 '\t' 分隔 "时间戳\t版本名"，时间戳解析失败或版本名为空则跳过
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
        // 解析完成后做字段整合与合法性裁剪
        // 根据版本号与新/旧外观字段决定最终外观模式
        settings.appearance_mode = resolve_appearance_mode(
            settings_revision,
            stored_appearance_mode,
            stored_liquid_glass,
            stored_liquid_glass_mode.as_deref(),
        );
        // 动画档位：新字段优先；否则由旧布尔字段推导（关 -> Off，否则 Medium）
        settings.animation_level =
            stored_animation_level.unwrap_or(match stored_legacy_animations {
                Some(false) => AnimationLevel::Off,
                _ => AnimationLevel::Medium,
            });
        // 对全部数值字段做范围裁剪，防止异常值破坏 UI 或启动参数
        settings.memory_mb = settings.memory_mb.clamp(2048, memory_limit_mb());
        settings.window_width = settings.window_width.clamp(640, 7680);
        settings.window_height = settings.window_height.clamp(360, 4320);
        settings.glass_transparency = settings.glass_transparency.clamp(20, 100);
        settings.card_edge_clarity = settings.card_edge_clarity.clamp(0, 100);
        settings.corner_radius = settings.corner_radius.clamp(
            theme::MIN_CORNER_RADIUS as u16,
            theme::MAX_CORNER_RADIUS as u16,
        );
        settings.pointer_brightness = settings.pointer_brightness.clamp(0, 200);
        settings.pointer_radius = settings.pointer_radius.clamp(40, 320);
        settings.pointer_refresh_rate = settings.pointer_refresh_rate.clamp(20, 144);
        settings.hero_title_size = settings.hero_title_size.clamp(24, 80);
        // 逐字符字号：解析为 u16 列表并逐个裁剪到 8–96
        if let Some(sizes) = stored_hero_title_char_sizes.as_deref() {
            let sizes = parse_u16_list(sizes)
                .into_iter()
                .map(|size| size.clamp(8, 96))
                .collect();
            settings.hero_title_char_sizes = sizes;
        }
        // 逐字符颜色：解析为 RGB 列表
        if let Some(colors) = stored_hero_title_char_colors.as_deref() {
            settings.hero_title_char_colors = parse_rgb_list(colors);
        }
        // 再次规范化主标题，并使逐字符字号/颜色数组与字符数对齐
        settings.hero_title = normalize_hero_title(&settings.hero_title);
        settings.sync_hero_title_char_styles();
        // 额外 JVM 参数中的换行统一替换为空格，避免破坏启动命令行
        settings.extra_jvm_args = settings.extra_jvm_args.replace(['\r', '\n'], " ");
        // 最近游玩：按时间倒序、按版本名去重、最多保留 32 条
        settings
            .recent_versions
            .sort_by_key(|recent| std::cmp::Reverse(recent.played_at));
        settings
            .recent_versions
            .dedup_by(|left, right| left.version == right.version);
        settings.recent_versions.truncate(32);
        // 重新自动发现游戏目录并合并（去重 + 校验），保证列表最新
        for directory in crate::game::discover_game_directories() {
            settings.add_game_directory(directory);
        }
        // 若之前显式指定过游戏目录，也一并加入并保留
        if let Some(directory) = settings.game_directory.clone() {
            settings.add_game_directory(directory);
        }
        // 剔除失效的游戏目录；若当前选中目录已失效则回退到列表首个
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
        // 若是从旧版目录迁移而来，则把规范化后的设置写回当前路径，完成迁移
        if migrate_to_current {
            let _ = settings.save();
        }
        settings
    }

    // 判断动画是否开启（Off 档位表示关闭）。
    /// 实现 animationsenabled 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `bool`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn animations_enabled(&self) -> bool {
        self.animation_level != AnimationLevel::Off
    }

    // 使"逐字符字号/颜色"数组与当前主标题的字符数保持一致：
    // 不足则按全局字号/颜色补齐，超出则截断。保证绘制时下标不越界。
    /// 实现 syncherotitlecharstyles 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn sync_hero_title_char_styles(&mut self) {
        let count = self.hero_title.chars().count();
        self.hero_title_char_sizes
            .resize(count, self.hero_title_size.clamp(8, 96));
        self.hero_title_char_colors
            .resize(count, self.hero_title_color);
        self.hero_title_char_sizes.truncate(count);
        self.hero_title_char_colors.truncate(count);
    }

    /// 按字符位置对齐编辑前后的逐字样式。
    ///
    /// 使用公共前缀和后缀找出本次实际修改区间：插入的文字在对应位置使用
    /// 当前默认颜色和字号，被删除文字的样式同时删除，其余字符样式保持原位。
    fn reconcile_hero_title_char_styles(&mut self, previous_title: &str) {
        let old_chars = previous_title.chars().collect::<Vec<_>>();
        let new_chars = self.hero_title.chars().collect::<Vec<_>>();
        self.hero_title_char_sizes
            .resize(old_chars.len(), self.hero_title_size);
        self.hero_title_char_colors
            .resize(old_chars.len(), self.hero_title_color);
        self.hero_title_char_sizes.truncate(old_chars.len());
        self.hero_title_char_colors.truncate(old_chars.len());

        let prefix = old_chars
            .iter()
            .zip(&new_chars)
            .take_while(|(old, new)| old == new)
            .count();
        let max_suffix = old_chars.len().min(new_chars.len()).saturating_sub(prefix);
        let suffix = old_chars
            .iter()
            .rev()
            .zip(new_chars.iter().rev())
            .take(max_suffix)
            .take_while(|(old, new)| old == new)
            .count();
        let inserted = new_chars.len().saturating_sub(prefix + suffix);

        let mut sizes = Vec::with_capacity(new_chars.len());
        let mut colors = Vec::with_capacity(new_chars.len());
        sizes.extend_from_slice(&self.hero_title_char_sizes[..prefix]);
        colors.extend_from_slice(&self.hero_title_char_colors[..prefix]);
        sizes.extend(std::iter::repeat_n(
            self.hero_title_size.clamp(8, 96),
            inserted,
        ));
        colors.extend(std::iter::repeat_n(self.hero_title_color, inserted));
        if suffix > 0 {
            sizes.extend_from_slice(&self.hero_title_char_sizes[old_chars.len() - suffix..]);
            colors.extend_from_slice(&self.hero_title_char_colors[old_chars.len() - suffix..]);
        }
        self.hero_title_char_sizes = sizes;
        self.hero_title_char_colors = colors;
        self.sync_hero_title_char_styles();
    }

    // 把游戏目录加入列表（若它确实是有效游戏目录且尚未存在）。重复判断区分大小写地按平台处理。
    /// 实现 添加游戏目录 相关的业务流程。
    ///
    /// # 参数
    /// - `directory`：由调用方传入的目录。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn add_game_directory(&mut self, directory: PathBuf) {
        // 不是有效游戏目录，或已存在相同路径，则忽略
        if !crate::game::is_game_directory(&directory)
            || self
                .game_directories
                .iter()
                .any(|known| same_path(known, &directory))
        {
            return;
        }
        self.game_directories.push(directory);
        // 按路径字符串排序，保持列表稳定有序
        self.game_directories
            .sort_by(|left, right| left.to_string_lossy().cmp(&right.to_string_lossy()));
    }

    // 查询指定游戏目录下某版本的内存覆盖值（MB）。找不到则返回 None，调用方回退到全局值。
    /// 实现 版本内存 相关的业务流程。
    ///
    /// # 参数
    /// - `directory`：由调用方传入的目录。
    /// - `profile`：由调用方传入的配置。
    /// # 返回
    /// 返回 `Option<u16>`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn version_memory_mb(&self, directory: &std::path::Path, profile: &str) -> Option<u16> {
        let key = version_profile_key(directory, profile);
        // 遍历内存覆盖列表，按 "目录\u{1f}profile\u{1f}内存" 从右拆分匹配 key
        self.version_memory.iter().find_map(|entry| {
            let (known_key, memory) = entry.rsplit_once('\u{1f}')?;
            (known_key.eq_ignore_ascii_case(&key))
                .then(|| memory.parse::<u16>().ok())
                .flatten()
        })
    }

    // 设置（覆盖）指定版本的内存值：先移除该 key 已有记录，再裁剪到本机物理内存范围内。
    /// 实现 设置版本内存 相关的业务流程。
    ///
    /// # 参数
    /// - `directory`：由调用方传入的目录。
    /// - `profile`：由调用方传入的配置。
    /// - `memory_mb`：由调用方传入的内存。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn set_version_memory(&mut self, directory: &std::path::Path, profile: &str, memory_mb: u16) {
        let key = version_profile_key(directory, profile);
        // 删除该 key 旧记录（以 "目录\u{1f}profile\u{1f}" 开头）
        self.version_memory
            .retain(|entry| !entry.starts_with(&format!("{key}\u{1f}")));
        self.version_memory.push(format!(
            "{key}\u{1f}{}",
            memory_mb.clamp(2048, memory_limit_mb())
        ));
    }

    // 清除指定版本的内存覆盖（恢复为全局值）。
    /// 实现 clear版本内存 相关的业务流程。
    ///
    /// # 参数
    /// - `directory`：由调用方传入的目录。
    /// - `profile`：由调用方传入的配置。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn clear_version_memory(&mut self, directory: &std::path::Path, profile: &str) {
        let key = version_profile_key(directory, profile);
        self.version_memory
            .retain(|entry| !entry.starts_with(&format!("{key}\u{1f}")));
    }

    // 查询指定版本的 Java 覆盖路径；找不到返回 None（使用全局 Java）。
    /// 实现 版本Java路径 相关的业务流程。
    ///
    /// # 参数
    /// - `directory`：由调用方传入的目录。
    /// - `profile`：由调用方传入的配置。
    /// # 返回
    /// 返回 `Option<PathBuf>`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn version_java_path(&self, directory: &std::path::Path, profile: &str) -> Option<PathBuf> {
        let key = version_profile_key(directory, profile);
        self.version_java.iter().find_map(|entry| {
            let (known_key, java) = entry.rsplit_once('\u{1f}')?;
            // key 比较时忽略大小写（Windows 路径不区分大小写）
            known_key
                .eq_ignore_ascii_case(&key)
                .then(|| PathBuf::from(java))
        })
    }

    // 设置或清除指定版本的 Java 覆盖：java 为 None 表示清除该覆盖。
    /// 实现 设置版本Java路径 相关的业务流程。
    ///
    /// # 参数
    /// - `directory`：由调用方传入的目录。
    /// - `profile`：由调用方传入的配置。
    /// - `java`：由调用方传入的Java。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn set_version_java_path(
        &mut self,
        directory: &std::path::Path,
        profile: &str,
        java: Option<&std::path::Path>,
    ) {
        let key = version_profile_key(directory, profile);
        // 先删除该 key 旧记录
        self.version_java
            .retain(|entry| !entry.starts_with(&format!("{key}\u{1f}")));
        if let Some(java) = java {
            self.version_java
                .push(format!("{key}\u{1f}{}", java.to_string_lossy()));
        }
    }

    /// 读取某个版本的启动器显示名称。
    ///
    /// 该名称只影响界面展示，不改变版本目录名、JSON id 或启动参数。
    fn version_name(&self, directory: &std::path::Path, profile: &str) -> Option<String> {
        let key = version_profile_key(directory, profile);
        self.version_names.iter().find_map(|entry| {
            let (known_key, name) = entry.rsplit_once('\u{1f}')?;
            known_key
                .eq_ignore_ascii_case(&key)
                .then(|| name.trim().to_owned())
                .filter(|name| !name.is_empty())
        })
    }

    /// 设置或清除某个版本的自定义显示名称。
    #[cfg(test)]
    fn set_version_name(&mut self, directory: &std::path::Path, profile: &str, name: Option<&str>) {
        let key = version_profile_key(directory, profile);
        self.version_names
            .retain(|entry| !entry.starts_with(&format!("{key}\u{1f}")));
        if let Some(name) = name {
            let name = name
                .replace(['\r', '\n', '\u{1f}'], " ")
                .trim()
                .chars()
                .take(48)
                .collect::<String>();
            if !name.is_empty() {
                self.version_names.push(format!("{key}\u{1f}{name}"));
            }
        }
    }

    /// 迁移真实版本重命名后保存在设置中的版本级引用。
    ///
    /// 内存和 Java 覆盖迁移到新 profile 键；旧显示别名及新真实名称对应的
    /// 别名会被删除，确保界面优先显示用户刚设置的真实版本名。
    fn rename_version_references(
        &mut self,
        directory: &std::path::Path,
        old_profile: &str,
        new_profile: &str,
    ) {
        let old_key = version_profile_key(directory, old_profile);
        let new_key = version_profile_key(directory, new_profile);
        for entries in [&mut self.version_memory, &mut self.version_java] {
            for entry in entries.iter_mut() {
                let Some((key, value)) = entry.rsplit_once('\u{1f}') else {
                    continue;
                };
                if key.eq_ignore_ascii_case(&old_key) {
                    *entry = format!("{new_key}\u{1f}{value}");
                }
            }
            entries.dedup_by(|left, right| left == right);
        }
        self.version_names.retain(|entry| {
            let Some((key, _)) = entry.rsplit_once('\u{1f}') else {
                return false;
            };
            !key.eq_ignore_ascii_case(&old_key) && !key.eq_ignore_ascii_case(&new_key)
        });
        for recent in &mut self.recent_versions {
            if recent.version == old_profile {
                recent.version = new_profile.to_owned();
            }
        }
        self.recent_versions
            .dedup_by(|left, right| left.version == right.version);
    }

    // 把当前设置序列化为 settings.ini 并写入数据目录（必要时先创建父目录）。
    // 单行 "key=value" 格式；逐字符字号/颜色、收藏站点、最近版本、版本覆盖等用特殊分隔符写入。
    // 返回 io 错误（如无法创建目录或写入文件）。
    /// 实现 保存 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `std::io::Result<()>`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn save(&self) -> std::io::Result<()> {
        let path = settings_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // 先把逐字符字号序列化为 "v1|v2|..." 形式（用 '|' 分隔）
        let hero_title_char_sizes = self
            .hero_title_char_sizes
            .iter()
            .map(u16::to_string)
            .collect::<Vec<_>>()
            .join("|");
        // 逐字符颜色序列化为 "RRGGBB|RRGGBB|..."（十六进制大写）
        let hero_title_char_colors = self
            .hero_title_char_colors
            .iter()
            .map(|color| format!("{:02X}{:02X}{:02X}", color[0], color[1], color[2]))
            .collect::<Vec<_>>()
            .join("|");
        let mut contents = format!(
            "memory_mb={}\ngame_directory={}\ngame_directories={}\nversion_isolation={}\njava_path={}\nshow_loader_versions={}\nwindow_width={}\nwindow_height={}\nfullscreen={}\njvm_args={}\nplayer_name={}\nplayer_skin={}\nplayer_skin_model_alex={}\nnotify_after_launch={}\nhide_after_launch={}\nanimation_level={}\ntooltips={}\nsettings_revision={}\nappearance_mode={}\nglass_transparency={}\ncard_edge_clarity={}\ncorner_radius={}\npointer_brightness={}\npointer_radius={}\npointer_refresh_rate={}\nhero_title={}\nhero_title_color={:02X}{:02X}{:02X}\nhero_title_size={}\nhero_title_char_sizes={}\nhero_title_char_colors={}\n",
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
            self.player_name,
            self.player_skin
                .as_ref()
                .map(|path| path.to_string_lossy())
                .unwrap_or_default(),
            self.player_skin_model_alex,
            self.notify_after_launch,
            self.hide_after_launch,
            self.animation_level.stored(),
            self.tooltips,
            SETTINGS_REVISION,
            // 外观模式序列化：Liquid 模式已并入 Frosted，统一写成 "frosted"
            match self.appearance_mode {
                theme::AppearanceMode::Simple => "simple",
                theme::AppearanceMode::Frosted => "frosted",
                theme::AppearanceMode::Liquid => "frosted",
            },
            self.glass_transparency,
            self.card_edge_clarity,
            self.corner_radius,
            self.pointer_brightness,
            self.pointer_radius,
            self.pointer_refresh_rate,
            self.hero_title.replace(['\r', '\n'], "\\n"),
            self.hero_title_color[0],
            self.hero_title_color[1],
            self.hero_title_color[2],
            self.hero_title_size,
            hero_title_char_sizes,
            hero_title_char_colors
        );
        // 主字段块：用 format! 一次性拼出所有标量字段（含 settings_revision 当前版本号）
        // 逐字符字段与上述临时拼接的字符串随后填入。
        for recent in &self.recent_versions {
            // 最近版本行：时间戳 + 制表符 + 版本名（版本名中的换行/制表符需去除以免破坏格式）
            contents.push_str(&format!(
                "recent_version={}\t{}\n",
                recent.played_at,
                recent.version.replace(['\r', '\n', '\t'], " ")
            ));
        }
        for site in &self.favorite_sites {
            // 收藏站点行：标题\u{1f}URL\u{1f}图标来源，各字段去除非法字符
            contents.push_str(&format!(
                "favorite_site={}\u{1f}{}\u{1f}{}\n",
                site.title.replace(['\r', '\n', '\u{1f}'], " "),
                site.url.replace(['\r', '\n', '\u{1f}'], ""),
                site.icon_source.replace(['\r', '\n', '\u{1f}'], "")
            ));
        }
        // 把每个版本内存/Java 覆盖各写成独立一行
        for memory in &self.version_memory {
            contents.push_str(&format!("version_memory={memory}\n"));
        }
        for java in &self.version_java {
            contents.push_str(&format!("version_java={java}\n"));
        }
        for name in &self.version_names {
            contents.push_str(&format!("version_name={name}\n"));
        }
        if let Some(background) = &self.background_image {
            contents.push_str(&format!(
                "background_image={}\n",
                background.to_string_lossy().replace(['\r', '\n'], "")
            ));
        }
        // 一次性写入文件（覆盖旧内容）
        std::fs::write(path, contents)
    }
}

// 根据新旧外观设置字段解析出最终要使用的外观模式，完成旧版配置的兼容映射。
// 参数：_revision 为设置版本号（当前未参与分支，保留用于将来迁移逻辑）；
// stored_mode 为新字段直接存储的模式；legacy_enabled/legacy_style 为旧版开关与样式。
// 规则：新字段优先（但 Liquid 降级为 Frosted）；否则由旧开关推导（开启->Frosted，关闭->Simple）。
/// 实现 resolveappearancemode 相关的业务流程。
///
/// # 参数
/// - `_revision`：由调用方传入的revision。
/// - `stored_mode`：由调用方传入的storedmode。
/// - `legacy_enabled`：由调用方传入的legacyenabled。
/// - `legacy_style`：由调用方传入的legacystyle。
/// # 返回
/// 返回 `theme::AppearanceMode`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn resolve_appearance_mode(
    _revision: u32,
    stored_mode: Option<theme::AppearanceMode>,
    legacy_enabled: Option<bool>,
    legacy_style: Option<&str>,
) -> theme::AppearanceMode {
    if let Some(mode) = stored_mode {
        return if mode == theme::AppearanceMode::Liquid {
            theme::AppearanceMode::Frosted
        } else {
            mode
        };
    }
    match legacy_enabled {
        Some(true) if legacy_style.is_some_and(|style| style.eq_ignore_ascii_case("clear")) => {
            theme::AppearanceMode::Frosted
        }
        Some(true) => theme::AppearanceMode::Frosted,
        Some(false) => theme::AppearanceMode::Simple,
        None => theme::AppearanceMode::Simple,
    }
}

// 返回当前数据目录下 settings.ini 的完整路径。
/// 实现 settings路径 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `PathBuf`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn settings_path() -> PathBuf {
    brand::data_directory().join("settings.ini")
}

// 返回旧版（迁移前）数据目录下 settings.ini 的路径。
/// 实现 legacysettings路径 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `PathBuf`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn legacy_settings_path() -> PathBuf {
    brand::legacy_data_directory().join("settings.ini")
}

// 把 settings.ini 中的单个收藏站点字段解析为 FavoriteSite。
// 字段以 \u{1f} 分隔：标题/URL/图标来源。标题或 URL 为空则视为无效，返回 None。
/// 实现 解析favoritesite 相关的业务流程。
///
/// # 参数
/// - `value`：由调用方传入的value。
/// # 返回
/// 返回 `Option<FavoriteSite>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
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

// 校验并规范化玩家名（离线模式）。要求长度 3–16、仅含 ASCII 字母数字与下划线。
// 成功返回规范化后的名字；失败返回中文错误描述。
/// 实现 normalize玩家名称 相关的业务流程。
///
/// # 参数
/// - `value`：由调用方传入的value。
/// # 返回
/// 成功时返回处理结果；失败时通过 `Err` 返回可诊断、可展示的错误信息。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn normalize_player_name(value: &str) -> Result<String, String> {
    let name = value.trim();
    if !(3..=16).contains(&name.len()) {
        return Err("玩家名称长度必须为 3–16 个字符".to_owned());
    }
    if !name
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err("玩家名称只能包含英文字母、数字和下划线".to_owned());
    }
    Ok(name.to_owned())
}

// 判断两个路径是否指向同一位置：Windows 下忽略大小写，其它平台精确比较。
/// 实现 same路径 相关的业务流程。
///
/// # 参数
/// - `left`：由调用方传入的left。
/// - `right`：由调用方传入的right。
/// # 返回
/// 返回 `bool`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn same_path(left: &std::path::Path, right: &std::path::Path) -> bool {
    let left = left.to_string_lossy();
    let right = right.to_string_lossy();
    if cfg!(windows) {
        left.eq_ignore_ascii_case(&right)
    } else {
        left == right
    }
}

// 构造"版本覆盖"记录用的组合 key：游戏目录（Windows 下小写）+ \u{1f} + profile 名。
// 统一小写目录可避免 Windows 路径大小写差异导致的重复记录。
/// 实现 版本配置key 相关的业务流程。
///
/// # 参数
/// - `directory`：由调用方传入的目录。
/// - `profile`：由调用方传入的配置。
/// # 返回
/// 返回 `String`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn version_profile_key(directory: &std::path::Path, profile: &str) -> String {
    let directory = directory.to_string_lossy();
    let directory = if cfg!(windows) {
        directory.to_lowercase()
    } else {
        directory.into_owned()
    };
    format!("{directory}\u{1f}{profile}")
}

// 从编译期嵌入的字节（include_bytes / 静态数组）加载图片并注册为 egui 纹理。
/// 实现 加载内嵌texture 相关的业务流程。
///
/// # 参数
/// - `ctx`：由调用方传入的ctx。
/// - `bytes`：由调用方传入的bytes。
/// - `name`：由调用方传入的名称。
/// # 返回
/// 返回 `Option<egui::TextureHandle>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn load_embedded_texture(
    ctx: &egui::Context,
    bytes: &'static [u8],
    name: &str,
) -> Option<egui::TextureHandle> {
    let image = image::load_from_memory(bytes).ok()?;
    let color_image = color_image_for_context(ctx, image);
    Some(ctx.load_texture(name, color_image, egui::TextureOptions::LINEAR))
}

// 内置的 6 个收藏站点图标资源：(站点 URL, PNG 字节, 纹理 key)。
// 这些图标随程序打包，避免初次打开社区页时还要联网下载常见站点图标。
/// 实现 builtinfavorite图标assets 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `[(&'static str, &'static [u8], &'static str)`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn builtin_favorite_icon_assets() -> [(&'static str, &'static [u8], &'static str); 6] {
    [
        (
            "https://www.minecraft.net/",
            include_bytes!("../../assets/favorite-icons/minecraft.png"),
            "favorite-icon:minecraft",
        ),
        (
            "https://zh.minecraft.wiki/",
            include_bytes!("../../assets/favorite-icons/wiki.png"),
            "favorite-icon:wiki",
        ),
        (
            "https://modrinth.com/",
            include_bytes!("../../assets/favorite-icons/modrinth.png"),
            "favorite-icon:modrinth",
        ),
        (
            "https://www.curseforge.com/minecraft",
            include_bytes!("../../assets/favorite-icons/curseforge.png"),
            "favorite-icon:curseforge",
        ),
        (
            "https://www.mcmod.cn/",
            include_bytes!("../../assets/favorite-icons/mcmod.png"),
            "favorite-icon:mcmod",
        ),
        (
            "https://www.planetminecraft.com/",
            include_bytes!("../../assets/favorite-icons/planetminecraft.png"),
            "favorite-icon:planetminecraft",
        ),
    ]
}

// 把解码后的图像转换为 egui 的 ColorImage，并按 GPU 纹理尺寸上限做等比缩放（超过则 Lanczos3 重采样）。
// 超过最大纹理边长的图片会被缩小，避免创建超大纹理导致驱动报错或显存爆满。
/// 实现 color图片forcontext 相关的业务流程。
///
/// # 参数
/// - `ctx`：由调用方传入的ctx。
/// - `image`：由调用方传入的图片。
/// # 返回
/// 返回 `egui::ColorImage`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn color_image_for_context(ctx: &egui::Context, image: image::DynamicImage) -> egui::ColorImage {
    // 查询当前 GPU 支持的最大纹理边长（至少取 1 以避免除零）
    let max_side = ctx.input(|input| input.max_texture_side).max(1) as u32;
    let image = if image.width() > max_side || image.height() > max_side {
        // 计算缩放比例（以较长边为基准），重采样到不超过上限
        let scale = max_side as f64 / image.width().max(image.height()) as f64;
        image.resize_exact(
            ((image.width() as f64 * scale).round() as u32).max(1),
            ((image.height() as f64 * scale).round() as u32).max(1),
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        image
    };
    // 转为 RGBA8 并包装为 egui ColorImage（使用未预乘的 alpha）
    let rgba = image.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw())
}

// 一条短暂的提示消息（toast）：显示后按时间自动淡出，UI 层据此判断是否需要继续展示。
/// 定义 Toast 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
struct Toast {
    message: String,   // 提示文本
    shown_at: Instant, // 创建时刻，用于计算已显示时长以触发淡出
}

/// 启动器的核心应用状态（eframe 的 `App` 实现主体）。
///
/// 本结构体集中持有启动器的全部瞬时状态，按功能可大致分为以下几组：
/// - 启动流程状态（`launch_*`、`game_process_id`）：启动动画、已启动的游戏进程；
/// - 导航状态（`page`、`page_history`、`nav_*`）：当前页面、历史记录、侧边栏高亮动画；
/// - 各页面的局部状态（社区页、创建页、设置页、下载页、版本库页各自的一组字段）；
/// - 皮肤编辑器状态（`skin_*`）：像素缓冲、笔刷颜色、3D 预览角度、撤销/重做历史；
/// - 异步任务接收器（各类 `*_receiver`）：后台线程通过 mpsc 回传下载/搜索/安装进度，
///   主线程在每帧的 `update_timers` 中轮询并更新对应状态；
/// - 纹理缓存（`*_texture`）：把加载过的图片（新闻图、皮肤、背景等）缓存为 egui 纹理，
///   避免每帧重复解码；
/// - 动画状态：大量 `*_value/progress + *_velocity` 成对字段由弹簧动画系统驱动，
///   每帧在 update 函数中积分更新。
///
/// 字段虽多，但都只被 UI 线程访问（后台线程仅通过 channel 通信），因此无需加锁。
pub struct LauncherApp {
    // 版本库列表中当前选中的版本下标
    selected_version: usize,
    // 当前打开的顶部浮层菜单（版本选择 / 玩家档案），无菜单时为 Menu::None
    menu: Menu,
    // 版本下拉菜单弹入/弹出弹簧动画的当前位置（0=收起，1=完全展开）
    version_popup_value: f32,
    // 版本下拉菜单弹簧动画的当前速度（与 value 成对积分）
    version_popup_velocity: f32,
    // 玩家档案下拉菜单弹入/弹出弹簧动画的当前位置
    profile_popup_value: f32,
    // 玩家档案下拉菜单弹簧动画的当前速度
    profile_popup_velocity: f32,
    // 当前展开的新闻条目下标（社区页）；None 表示全部折叠
    news: Option<usize>,
    // 开始启动游戏的时刻；None 表示当前没有处于"启动中"状态
    launch_started: Option<Instant>,
    // 游戏启动流程是否已完成（用于切换"启动中"与"已启动"的 UI 表现）
    launch_complete: bool,
    // 当前显示的 toast 提示消息（None 表示没有提示）
    toast: Option<Toast>,
    // 启动器本次会话的开始时刻，用于首页/入场动画的进度计算
    started_at: Instant,
    // 待保存的截图输出路径（截图请求待处理时为 Some）
    screenshot_path: Option<PathBuf>,
    // 是否已请求截屏（请求被消费后复位）
    screenshot_requested: bool,
    // 最近一次成功应用的窗口物理尺寸与圆角半径；尺寸、DPI或设置变化时重新建立窗口区域
    window_shape_signature: Option<(u32, u32, u16)>,
    // 最近一次同步给 Windows 顶部边框的状态栏背景色
    window_border_color: Option<[u8; 3]>,
    // 已启动的游戏进程 PID；None 表示尚未启动游戏（用于后续跟踪/终止）
    game_process_id: Option<u32>,
    // 本次启动是否来自"最近游玩"卡片（影响启动后的界面表现）
    launch_recent: bool,
    // 是否显示皮肤预览相关的悬浮提示
    preview_tooltip: bool,
    // 指针光晕的实时预览开关（设置页调整参数时立即预览效果）
    pointer_glow_preview: bool,
    // 指针光晕的当前渲染位置（每帧向鼠标位置平滑趋近）
    pointer_glow_position: Pos2,
    // 指针光晕的当前强度（鼠标进入窗口淡入、离开淡出）
    pointer_glow_strength: f32,
    // 指针光晕位置是否已初始化（首次设为鼠标位置，避免从原点飞入）
    pointer_glow_initialized: bool,
    // 是否处于"DPI 信息上报"调试模式（启动后打印一次 DPI 数据并退出）
    report_dpi: bool,
    // DPI 信息是否已上报过（只上报一次）
    dpi_reported: bool,
    // 当前显示的一级页面（侧边栏导航目标）
    page: Page,
    // 页面导航历史栈（记录访问过的页面，用于返回操作）
    page_history: Vec<Page>,
    // 侧边导航栏当前选中的按钮下标
    nav_selected: usize,
    // 侧边栏高亮条的纵向位置（弹簧动画，向选中按钮滑动）
    nav_highlight_y: f32,
    // 侧边栏高亮条弹簧动画的当前速度
    nav_highlight_velocity: f32,
    // 侧边栏高亮条位置是否已初始化（避免首帧从顶部飞入）
    nav_highlight_initialized: bool,
    // 当前显示 tooltip 的导航按钮标识（0 表示无）
    nav_tooltip_id: u8,
    // 当前导航 tooltip 的文本内容
    nav_tooltip_label: &'static str,
    // 触发 tooltip 的按钮矩形区域（用于定位气泡）
    nav_tooltip_button: Rect,
    // 导航 tooltip 透明度/缩放动画的当前值
    nav_tooltip_value: f32,
    // 导航 tooltip 弹簧动画的当前速度
    nav_tooltip_velocity: f32,
    // tooltip 目标状态（true=正在淡入显示，false=淡出中）
    nav_tooltip_target: bool,
    // 页面切换前的旧页面（离场动画期间仍需绘制它）
    previous_page: Option<Page>,
    // 页面切换动画的进度（0=旧页完全在场，1=切换完成）
    page_transition_progress: f32,
    // 页面切换弹簧动画的当前速度
    page_transition_velocity: f32,
    // 页面切换的方向系数（+1 前进 / -1 返回，影响滑动方向）
    page_transition_direction: f32,
    // 社区页当前选中的标签页下标
    community_tab: usize,
    // 社区页内容区的滚动偏移
    community_scroll: f32,
    // 收藏站点编辑器弹窗是否打开
    favorite_editor_open: bool,
    // 收藏编辑器"标题"输入框内容
    favorite_title_input: String,
    // 收藏编辑器"网址"输入框内容
    favorite_url_input: String,
    // 收藏编辑器"图标来源"输入框内容
    favorite_icon_input: String,
    // 收藏编辑器的校验错误信息（None 表示无错误）
    favorite_editor_error: Option<String>,
    // 收藏编辑器弹窗弹出动画的当前值
    favorite_editor_value: f32,
    // 收藏编辑器弹窗弹簧动画的当前速度
    favorite_editor_velocity: f32,
    // 收藏编辑器弹出时"来源卡片"的屏幕矩形（用于缩放过渡动画起点）
    favorite_editor_source: Rect,
    // 收藏编辑器目标位置矩形（动画终点）
    favorite_editor_target: Rect,
    // 收藏编辑器内卡片的动画目标矩形
    favorite_editor_card_target: Rect,
    // 收藏编辑器目标滚动偏移（过渡动画收敛值）
    favorite_editor_target_scroll: f32,
    // 过渡动画结束后待提交/编辑的收藏站点
    favorite_editor_pending_site: Option<FavoriteSite>,
    // 修改玩家名对话框是否打开
    account_name_dialog_open: bool,
    // 玩家名对话框弹出动画的当前值
    account_name_dialog_value: f32,
    // 玩家名对话框弹簧动画的当前速度
    account_name_dialog_velocity: f32,
    // 玩家名对话框的输入框内容
    account_name_input: String,
    // 玩家名对话框的校验错误信息（None 表示无错误）
    account_name_error: Option<String>,
    // 版本重命名对话框锁定的游戏目录与真实 profile ID
    version_name_target: Option<VersionNameTarget>,
    // 重命名窗口是否处于打开目标状态（关闭时先播放回弹动画）
    version_name_dialog_open: bool,
    // 重命名窗口从按钮到中心的弹簧动画值
    version_name_dialog_value: f32,
    // 重命名窗口弹簧动画速度
    version_name_dialog_velocity: f32,
    // “重命名”按钮矩形，关闭动画沿该位置缩回
    version_name_dialog_source: Rect,
    // 版本重命名输入内容
    version_name_input: String,
    // 版本重命名校验错误
    version_name_error: Option<String>,
    // 首页主标题编辑模式下当前选中的字符下标（用于逐字符配色/字号）
    hero_title_selected_char: usize,
    // 收藏图标异步下载任务的 mpsc 接收端（任务完成前为 Some）
    favorite_icon_receiver: Option<FavoriteIconReceiver>,
    // 已经尝试过下载图标的站点标识列表（避免重复发起下载）
    favorite_icon_attempted: Vec<String>,
    // 已加载完成的收藏图标纹理缓存：站点标识 -> egui 纹理
    favorite_icon_textures: HashMap<String, egui::TextureHandle>,
    // 切换前的旧资源详情下标（详情面板过渡动画期间仍需绘制旧内容）
    previous_resource_detail: Option<usize>,
    // 资源详情面板切换动画的进度
    resource_detail_progress: f32,
    // 资源详情面板切换弹簧动画的当前速度
    resource_detail_velocity: f32,
    // 资源详情面板切换的方向系数（+1/-1）
    resource_detail_direction: f32,
    // 创建页当前选中的标签页下标
    create_tab: usize,
    // 皮肤编辑器的像素缓冲（RGBA 字节序列，64x64 皮肤图像的原始数据）
    skin_pixels: Vec<u8>,
    // 当前皮肤是否为细臂（Alex）模型（影响 3D 预览的手臂宽度）
    skin_model_alex: bool,
    // 皮肤画笔的当前颜色（RGBA，含透明度）
    skin_color: [u8; 4],
    // 是否处于橡皮擦模式（绘制时写入透明像素而非当前颜色）
    skin_erasing: bool,
    // 皮肤 3D 预览的偏航角（水平旋转角度）
    skin_preview_yaw: f32,
    // 皮肤 3D 预览的俯仰角（垂直旋转角度）
    skin_preview_pitch: f32,
    // 皮肤像素缓冲对应的 egui 纹理缓存（None 表示尚未创建或已失效）
    skin_texture: Option<egui::TextureHandle>,
    // 皮肤像素是否有未保存到纹理/文件的改动（脏标记）
    skin_dirty: bool,
    // 上一笔触经过的最后一个像素坐标（用于连续线条插值）
    skin_last_pixel: Option<(usize, usize)>,
    // 3D 预览中鼠标悬停的最后一个面（用于高亮与取色）
    skin_preview_last_face: Option<SkinFaceKey>,
    // 皮肤编辑的撤销/重做历史栈
    skin_history: SkinEditHistory,
    // 一次笔画（拖拽）开始前的完整像素快照（松开鼠标时整体记入历史）
    skin_stroke_before: Option<Vec<u8>>,
    // 皮肤编辑器的状态提示消息（保存成功/失败等）
    skin_status: Option<String>,
    // "创建 Mod"向导：Mod 显示名称输入
    mod_name_input: String,
    // "创建 Mod"向导：Mod 内部标识（modid）输入
    mod_id_input: String,
    // "创建 Mod"向导：Java 包名输入
    mod_package_input: String,
    // "创建 Mod"向导：Mod 版本号输入
    mod_version_input: String,
    // "创建 Mod"向导：目标 Minecraft 版本输入
    mod_minecraft_input: String,
    // "创建 Mod"向导：所选加载器在列表中的下标
    mod_loader_index: usize,
    // "创建 Mod"向导：用户选择的工程输出目录
    mod_output_directory: Option<PathBuf>,
    // "创建 Mod"向导：状态提示消息（生成成功/失败）
    mod_status: Option<String>,
    // 切换前的旧创建页标签（过渡动画期间仍需绘制旧内容）
    previous_create_tab: Option<usize>,
    // 创建页标签切换动画的进度
    create_transition_progress: f32,
    // 创建页标签切换弹簧动画的当前速度
    create_transition_velocity: f32,
    // 创建页标签切换的方向系数（+1/-1）
    create_transition_direction: f32,
    // 创建页标签指示器的当前位置（弹簧动画）
    create_tab_position: f32,
    // 创建页标签指示器弹簧动画的当前速度
    create_tab_velocity: f32,
    // 设置页当前显示的小节
    settings_section: SettingsSection,
    // 设置页侧栏高亮条的纵向位置（弹簧动画）
    settings_highlight_y: f32,
    // 设置页侧栏高亮条弹簧动画的当前速度
    settings_highlight_velocity: f32,
    // 设置页侧栏高亮条位置是否已初始化
    settings_highlight_initialized: bool,
    // 切换前的旧设置小节（过渡动画期间仍需绘制旧内容）
    previous_settings_section: Option<SettingsSection>,
    // 设置小节切换动画的进度
    settings_transition_progress: f32,
    // 设置小节切换弹簧动画的当前速度
    settings_transition_velocity: f32,
    // 设置小节切换的方向系数（+1/-1）
    settings_transition_direction: f32,
    // 当前生效的启动器设置（用户改动会即时写入这里）
    settings: LauncherSettings,
    // 上一次保存到磁盘的设置快照（用于 diff 检测是否需要持久化）
    saved_settings: LauncherSettings,
    // 启动游戏前的"启动设置"弹窗是否打开
    launch_settings_open: bool,
    // 启动设置弹窗弹出动画的当前值
    launch_popup_value: f32,
    // 启动设置弹窗弹簧动画的当前速度
    launch_popup_velocity: f32,
    // 用户导入的首页背景图纹理；None 表示不显示背景图片
    hero_background_texture: Option<egui::TextureHandle>,
    // 正式版图标纹理缓存
    release_icon_texture: Option<egui::TextureHandle>,
    // 快照版图标纹理缓存
    snapshot_icon_texture: Option<egui::TextureHandle>,
    // 加载器（Forge/Fabric 等）图标纹理缓存
    loader_icon_texture: Option<egui::TextureHandle>,
    // 系统中检测到的可用 Java 运行时列表
    java_runtimes: Vec<JavaRuntime>,
    // Java 选择器当前服务的目标（全局设置或某个具体版本）
    java_picker_target: Option<JavaPickerTarget>,
    // Java 选择器弹出时的锚点矩形（用于定位下拉面板）
    java_picker_anchor: Rect,
    // Java 选择器列表的滚动偏移
    java_picker_scroll: f32,
    // 版本导出对话框是否打开（预留字段，当前逻辑未读取）
    #[allow(dead_code)]
    version_export_open: bool,
    // 导出整合包时是否包含 mods 目录
    export_include_mods: bool,
    // 导出整合包时是否包含 config 目录
    export_include_config: bool,
    // 导出整合包时是否包含存档（saves）
    export_include_saves: bool,
    // 导出整合包时是否包含材质包（resourcepacks）
    export_include_resourcepacks: bool,
    // 导出整合包时是否包含光影包（shaderpacks）
    export_include_shaderpacks: bool,
    // 导出整合包的目标格式（压缩包类型等）
    export_format: ModpackExportFormat,
    // 导出格式下拉列表是否展开
    export_format_menu_open: bool,
    // 本机已安装的游戏版本列表（版本库页数据源）
    versions: Vec<GameVersion>,
    // 最近的游玩会话记录（首页"最近游玩"卡片数据源）
    recent_sessions: Vec<RecentSession>,
    // 新闻获取后台任务的 mpsc 接收端（任务完成前为 Some）
    news_receiver: Option<std::sync::mpsc::Receiver<Result<Vec<MinecraftNews>, String>>>,
    // 已成功获取的新闻条目列表
    news_items: Vec<MinecraftNews>,
    // 新闻获取失败的错误信息（None 表示无错误）
    news_error: Option<String>,
    // 各新闻条目的配图纹理缓存（与 news_items 下标对应；None 表示该条目尚无图）
    news_textures: Vec<Option<egui::TextureHandle>>,
    // 各新闻配图对应的缓存 key（与 news_textures 对齐，用于判断纹理是否需要重建）
    news_texture_keys: Vec<Option<String>>,
    // 启动器自身更新检查任务的 mpsc 接收端
    launcher_update_receiver: Option<std::sync::mpsc::Receiver<Result<LauncherRelease, String>>>,
    // 已获取的最新版本发布信息（有更新时非 None）
    launcher_update_result: Option<LauncherRelease>,
    // 启动器更新检查失败的错误信息
    launcher_update_error: Option<String>,
    // 本次更新检查是否由用户手动触发（影响无更新时的提示行为）
    launcher_update_manual: bool,
    // 强制更新目标版本信息（服务端要求强制升级时非 None）
    forced_update_release: Option<LauncherRelease>,
    // 强制更新下载进度事件的 mpsc 接收端
    forced_update_receiver: Option<std::sync::mpsc::Receiver<update::ForcedUpdateEvent>>,
    // 强制更新的下载进度（0.0–1.0）
    forced_update_progress: f32,
    // 强制更新过程的错误信息
    forced_update_error: Option<String>,
    // 强制更新安装包是否正在安装（防止重复触发）
    forced_update_installing: bool,
    // 版本库列表的滚动偏移
    version_scroll: f32,
    // 版本库页中当前选中的子页/版本下标
    version_page_selected: usize,
    // 版本库详情子页的滚动偏移
    version_page_scroll: f32,
    // 版本"所在文件夹"下拉菜单是否打开
    version_folder_menu_open: bool,
    // 版本"所在文件夹"下拉菜单的滚动偏移
    version_folder_scroll: f32,
    // 当前打开的"内容管理"分类对话框（mods/saves 等）
    version_content_dialog: Option<VersionContentKind>,
    // 内容管理对话框是否处于打开状态（含关闭动画期间）
    version_content_dialog_open: bool,
    // 每次打开使用新的图层实例 ID，避免复用旧关闭动画状态
    version_content_dialog_instance: u64,
    // 内容管理对话框弹出动画的当前值
    version_content_popup_value: f32,
    // 内容管理对话框弹簧动画的当前速度
    version_content_popup_velocity: f32,
    // 内容管理对话框弹出时的来源矩形（缩放过渡起点）
    version_content_popup_source: Rect,
    // 内容管理对话框关闭回弹动画已经过的时间（秒）
    version_content_close_elapsed: f32,
    // 内容管理对话框内容区的滚动偏移
    version_content_scroll: f32,
    // 切换前的旧版本详情下标（过渡动画期间仍需绘制旧内容）
    previous_version_detail: Option<usize>,
    // 版本详情面板切换动画的进度
    version_detail_progress: f32,
    // 版本详情面板切换弹簧动画的当前速度
    version_detail_velocity: f32,
    // 版本详情面板切换的方向系数（+1/-1）
    version_detail_direction: f32,
    // 版本库列表当前的筛选方式（全部/正式版/快照）
    version_filter: VersionFilter,
    // 官方版本清单获取任务的 mpsc 接收端
    official_manifest:
        Option<std::sync::mpsc::Receiver<Result<Vec<crate::download::OfficialVersion>, String>>>,
    // 已获取的官方可下载版本列表
    official_versions: Vec<crate::download::OfficialVersion>,
    // 官方版本清单获取失败的错误信息
    official_error: Option<String>,
    // 标记"正在打开浏览器访问官方下载页"（避免重复触发）
    official_browser_open: bool,
    // 官方版本列表中当前选中的版本下标
    official_selected: usize,
    // 官方版本列表的滚动偏移
    official_scroll: f32,
    // 切换前的旧下载详情下标（过渡动画期间仍需绘制旧内容）
    previous_download_detail: Option<usize>,
    // 下载详情面板切换动画的进度
    download_detail_progress: f32,
    // 下载详情面板切换弹簧动画的当前速度
    download_detail_velocity: f32,
    // 下载详情面板切换的方向系数（+1/-1）
    download_detail_direction: f32,
    // 下载页"选择加载器"下拉菜单是否打开
    download_loader_open: bool,
    // 加载器下拉菜单弹出动画的当前值
    download_loader_value: f32,
    // 加载器下拉菜单弹簧动画的当前速度
    download_loader_velocity: f32,
    // 下载页资源搜索框的输入内容
    download_search: String,
    // 下载页当前选中的资源分类（None 表示未选中/全部）
    download_resource_category: Option<ResourceCategory>,
    // 分类切换动画的当前位置
    download_category_position: f32,
    // 分类切换弹簧动画的当前速度
    download_category_velocity: f32,
    // 下载页整体视图切换动画的进度
    download_page_progress: f32,
    // 下载页视图切换弹簧动画的当前速度
    download_page_velocity: f32,
    // 下载页视图切换的方向系数（+1/-1）
    download_page_direction: f32,
    // 待执行的分类切换请求（等待背景模糊纹理就绪后再真正切换）
    download_switch_pending: Option<Option<ResourceCategory>>,
    // 分类切换背景模糊纹理生成任务的 mpsc 接收端
    download_switch_receiver: Option<std::sync::mpsc::Receiver<std::sync::Arc<egui::ColorImage>>>,
    // 本次分类切换请求的发起时刻（用于超时兜底）
    download_switch_requested_at: Option<Instant>,
    // 分类切换过渡用的模糊背景纹理
    download_switch_texture: Option<egui::TextureHandle>,
    // 模糊背景纹理的尺寸（宽、高，用于绘制时对齐）
    download_switch_texture_size: [usize; 2],
    // 已排队但尚未处理的分类切换请求（动画完成后再执行）
    download_switch_queued: Option<Option<ResourceCategory>>,
    // 当前展示的资源搜索结果列表
    resource_results: Vec<ResourceProject>,
    // 资源搜索后台任务的 mpsc 接收端
    resource_search_receiver:
        Option<std::sync::mpsc::Receiver<Result<Vec<ResourceProject>, String>>>,
    // 资源搜索失败的错误信息
    resource_search_error: Option<String>,
    // 已预取过详情的资源项目 key（避免重复请求详情）
    resource_prefetched_key: Option<String>,
    // 资源安装时选择的版本号（下拉菜单选中值）
    resource_version_filter: String,
    // 资源安装的目标 profile 名称（None 表示未选择）
    resource_target_profile: Option<String>,
    // 资源版本选择下拉菜单是否打开
    resource_version_menu_open: bool,
    // 资源版本下拉菜单的锚点矩形
    resource_version_anchor: Rect,
    // 资源版本下拉菜单的滚动偏移
    resource_version_scroll: f32,
    // 资源列表中当前选中的项目下标
    resource_selected: usize,
    // 资源列表的滚动偏移
    resource_scroll: f32,
    // 资源下载后台任务的 mpsc 接收端（回传下载结果路径或错误）
    resource_download_receiver: Option<std::sync::mpsc::Receiver<Result<PathBuf, String>>>,
    // 正在下载的资源名称（用于进度提示）
    resource_download_name: Option<String>,
    // 各资源分类对应的下载目标目录缓存（分类 -> 目录）
    resource_download_directories: HashMap<ResourceCategory, PathBuf>,
    // 下载、安装或导入失败时保留完整错误文本及已保存的日志文件路径
    download_error_log: Option<DownloadErrorLog>,
    // 官方下载列表当前的筛选方式（全部/正式版/快照）
    official_filter: VersionFilter,
    // 版本安装后台任务的 mpsc 接收端（回传安装进度事件）
    install_receiver: Option<std::sync::mpsc::Receiver<crate::download::InstallUpdate>>,
    // 最近一次收到的版本安装进度事件
    install_progress: Option<crate::download::InstallUpdate>,
    // 正在安装的版本 id（用于进度展示与去重）
    install_version_id: Option<String>,
    // 安装完成后是否自动启动游戏
    install_launch_after: bool,
    // 整合包导入后台任务的 mpsc 接收端
    modpack_import_receiver: Option<std::sync::mpsc::Receiver<crate::modpack::ModpackImportUpdate>>,
    // 最近一次收到的整合包导入进度事件
    modpack_import_update: Option<crate::modpack::ModpackImportUpdate>,
    // 整合包导出后台任务的 mpsc 接收端（回传导出结果路径或错误）
    modpack_export_receiver: Option<std::sync::mpsc::Receiver<Result<PathBuf, String>>>,
    // 加载器（Forge/Fabric 等）安装对话框是否打开
    loader_dialog_open: bool,
    // 加载器安装对话框中的基线（原版）版本名
    loader_base_version: String,
    // "先安装原版再装加载器"流程中待安装的原版版本信息
    loader_preinstall: Option<crate::download::OfficialVersion>,
    // 原版安装完成后待执行的加载器安装请求（加载器类型 + 版本名）
    pending_loader_after_base: Option<(LoaderKind, String)>,
    // 当前选中的加载器类型（Forge/Fabric 等；None 表示未选）
    loader_kind: Option<LoaderKind>,
    // 加载器版本列表获取任务的 mpsc 接收端
    loader_versions_receiver: Option<std::sync::mpsc::Receiver<Result<Vec<String>, String>>>,
    // 已获取的加载器版本列表
    loader_versions: Vec<String>,
    // 加载器版本获取失败的错误信息
    loader_versions_error: Option<String>,
    // 加载器版本列表中当前选中的下标
    loader_version_selected: usize,
    // 加载器版本列表的滚动偏移
    loader_version_scroll: f32,
    // 加载器安装后台任务的 mpsc 接收端（回传安装进度事件）
    loader_install_receiver: Option<std::sync::mpsc::Receiver<LoaderInstallUpdate>>,
    // 最近一次收到的加载器安装进度事件
    loader_install_update: Option<LoaderInstallUpdate>,
}

// eframe 的 App trait 实现：eframe 每帧回调这里，本启动器的入口就只有这两个方法，
// 其余所有绘制/更新逻辑都由它们分发到子模块完成。
impl App for LauncherApp {
    // 返回窗口清屏底色。eframe 在每帧开始时用它填充整个视口。
    // 参数：_visuals 为当前视觉主题（本实现未使用）。
    // 返回：RGBA 归一化浮点数组（各分量 0.0–1.0）。
    /// 实现 clearcolor 相关的业务流程。
    ///
    /// # 参数
    /// - `_visuals`：由调用方传入的visuals。
    /// # 返回
    /// 返回 `[f32`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // 圆角窗口的四个角靠 DWM 抠出来，底色不能是 egui 默认的深黑，
        // 否则标题栏压不到的那一圈边就算画了底色，圆角外仍会透出一圈黑边。
        let [r, g, b, _] = theme::background().to_normalized_gamma_f32();
        [r, g, b, 0.0]
    }

    // 每帧的主入口：eframe 会以显示器/刷新率节奏反复调用本方法。
    // 执行顺序：更新计时器与动画 -> 处理键盘快捷键 -> 应用圆角窗口 -> 渲染全部界面
    // -> 持久化设置变更 -> 处理待保存的截图。
    // 参数：ui 为 egui 提供的根 Ui；_frame 为帧信息（未使用）。
    /// 实现 ui 相关的业务流程。
    ///
    /// # 参数
    /// - `ui`：由调用方传入的ui。
    /// - `_frame`：由调用方传入的frame。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut Frame) {
        // 克隆一份上下文句柄，方便后续各辅助方法独立使用（Context 内部为引用计数）
        let ctx = ui.ctx().clone();
        // DPI 调试模式：打印一次关键 DPI 参数后立刻请求关闭窗口并退出本帧
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
        // 推进计时器：轮询各后台任务 channel、更新 toast/动画等与时间相关的状态
        self.update_timers(&ctx);
        // 处理全局键盘快捷键（如截图、关闭菜单等）
        self.handle_shortcuts(&ctx);
        // 取根 Ui 的可用区域作为整个窗口的绘制范围
        let screen = ui.max_rect();
        // 窗口区域必须随缩放、DPI 和用户圆角设置同步更新。只有签名变化时才调用
        // Win32，避免每帧重复创建系统区域。
        let pixels_per_point = ctx.pixels_per_point();
        let shape_signature = (
            (screen.width() * pixels_per_point).round().max(1.0) as u32,
            (screen.height() * pixels_per_point).round().max(1.0) as u32,
            self.settings.corner_radius,
        );
        if self.window_shape_signature != Some(shape_signature)
            && crate::apply_rounded_window(self.settings.corner_radius, pixels_per_point)
        {
            self.window_shape_signature = Some(shape_signature);
            self.window_border_color = None;
        }
        // 把根 Ui 的最小尺寸固定为窗口大小，保证子布局铺满
        ui.set_min_size(screen.size());
        // 渲染整个界面（标题栏、侧边栏、当前页面及各弹窗）
        self.render(&ctx, ui, screen);
        // 若设置与上次保存的快照不一致，则把变更持久化到 settings.ini
        self.persist_settings();
        // 若有待处理的截图请求，则把当前帧保存到指定路径
        self.handle_screenshot(&ctx);
    }
}
