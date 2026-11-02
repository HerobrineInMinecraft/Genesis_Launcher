//! 主入口文件：Minecraft启动器
//! 功能：命令行诊断报告 + eframe GUI主程序，Windows平台做高DPI、圆角窗口适配
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // Release下隐藏Windows控制台黑窗口

// 模块声明
mod animation; // 动画状态、插值逻辑
mod app; // eframe App主状态 LauncherApp
mod brand; // 品牌常量（启动器名称、标识等）
mod download; // 下载逻辑、版本清单、Java运行时安装
mod exporter; // 导出功能（配置/存档/整合包导出）
mod favorites; // 收藏服务器/版本
mod game; // Minecraft游戏目录探测、版本校验、启动流程
mod hero; // 首页大图、宣传组件
mod loader; // 加载器（Fabric / Forge / NeoForge）版本拉取、安装
mod modpack; // 整合包相关解析、导入
mod news; // 新闻公告拉取
mod resources; // Modrinth资源搜索（Mod、资源包等）
mod scene; // UI场景路由（首页/版本/资源/设置等页面）
mod sessions; // 最近游玩会话记录
mod skin; // 皮肤渲染、皮肤上传
mod theme; // 配色主题、样式
mod update; // 启动器自身更新检查、更新包下载
mod vector; // 矢量常量、画布尺寸定义
mod versions; // Minecraft原版版本列表、版本元数据解析

use app::LauncherApp;
use eframe::egui;

/// 程序入口
fn main() -> eframe::Result {
    // 开启Windows每显示器独立DPI感知V2，高分屏缩放正常
    enable_per_monitor_dpi();

    // ========== 命令行诊断分支：--game-report ==========
    // 输出游戏目录、已安装版本、可启动版本、安装完整性、Java校验信息
    if std::env::args().any(|arg| arg == "--game-report") {
        // 读取参数指定目录，未传则自动探测游戏目录
        let requested = std::env::args()
            .position(|arg| arg == "--game-report")
            .and_then(|index| std::env::args().nth(index + 1))
            .map(std::path::PathBuf::from);
        // 参数目录优先，失败走自动发现
        if let Some(directory) = requested.or_else(game::discover_game_directory) {
            let versions = game::installed_versions(&directory);
            let launchable = game::launchable_versions(&directory);
            println!("game_directory={}", directory.display());
            println!("installed_versions={}", versions.join(", "));
            println!("launchable_versions={}", launchable.join(", "));
            // 校验第一个版本是否完整安装
            if let Some(version) = versions.first() {
                println!(
                    "installation_complete={}",
                    game::installation_complete(&directory, version, "原版")
                );
            }
            // 校验第一个可启动版本，获取启动配置与Java路径
            if let Some(version) = launchable.first() {
                match game::check_launch(&directory, version, "原版") {
                    Ok((profile, java)) => {
                        println!("launch_profile={profile}");
                        println!("java={}", java.display());
                    }
                    Err(error) => println!("launch_check_error={error}"),
                }
            }
        } else {
            println!("game_directory=not_found");
        }
        return Ok(());
    }

    // ========== 命令行诊断分支：--sessions-report <目录> ==========
    // 读取最近游玩会话，输出会话类型、名称、服务器地址、版本、最后游玩时间戳
    if let Some(index) = std::env::args().position(|arg| arg == "--sessions-report") {
        let directory = std::env::args()
            .nth(index + 1)
            .map(std::path::PathBuf::from);
        if let Some(directory) = directory {
            for session in sessions::discover_recent_sessions(&directory) {
                println!(
                    "{:?}\t{}\t{}\tversion={}\t{}",
                    session.kind,
                    session.name,
                    session.address.unwrap_or_default(),
                    session.version.unwrap_or_default(),
                    session.last_played.unwrap_or(0)
                );
            }
        } else {
            println!("sessions_error=usage --sessions-report <directory>");
        }
        return Ok(());
    }

    // ========== 命令行诊断分支：--thumbnail-report <图片路径> ==========
    // 读取图片，输出缩略图宽高，用于封面合法性校验
    if let Some(index) = std::env::args().position(|arg| arg == "--thumbnail-report") {
        if let Some(path) = std::env::args().nth(index + 1) {
            match std::fs::read(&path)
                .map_err(|error| error.to_string())
                .and_then(|bytes| {
                    image::load_from_memory(&bytes).map_err(|error| error.to_string())
                }) {
                Ok(image) => println!("thumbnail={}x{}", image.width(), image.height()),
                Err(error) => println!("thumbnail_error={error}"),
            }
        }
        return Ok(());
    }

    // ========== 命令行诊断分支：--news-report ==========
    // 拉取公告新闻，取前6条打印：日期｜分类｜标题｜图片字节长度
    if std::env::args().any(|arg| arg == "--news-report") {
        match news::fetch_latest_news() {
            Ok(entries) => {
                for entry in entries.iter().take(6) {
                    println!(
                        "{}\t{}\t{}\timage={}",
                        entry.date,
                        entry.category,
                        entry.title,
                        entry.image_bytes.as_ref().map(Vec::len).unwrap_or(0)
                    );
                }
            }
            Err(error) => println!("news_error={error}"),
        }
        return Ok(());
    }

    // ========== 命令行诊断分支：--update-report ==========
    // 检查启动器更新，打印版本、名称、页面链接、更新包信息、是否新版本
    if std::env::args().any(|arg| arg == "--update-report") {
        match update::fetch_latest_release() {
            Ok(release) => {
                println!("update_version={}", release.version);
                println!("update_name={}", release.name);
                println!("update_url={}", release.page_url);
                if let Some(asset) = release.asset.as_ref() {
                    println!("update_asset={}", asset.name);
                    println!("update_asset_size={}", asset.size);
                    println!(
                        "update_asset_sha256={}",
                        asset.sha256.as_deref().unwrap_or("")
                    );
                }
                println!(
                    "update_available={}",
                    update::is_newer_version(env!("CARGO_PKG_VERSION"), &release.version)
                );
            }
            Err(error) => println!("update_error={error}"),
        }
        return Ok(());
    }

    // ========== 命令行诊断分支：--update-download-report ==========
    // 测试更新包下载，输出下载后的本地路径
    if std::env::args().any(|arg| arg == "--update-download-report") {
        match update::fetch_latest_release() {
            Ok(release) => match release.asset.as_ref() {
                Some(asset) => match update::download_release_asset_for_report(asset) {
                    Ok(path) => println!("update_download={}", path.display()),
                    Err(error) => println!("update_download_error={error}"),
                },
                None => println!("update_download_error=release_has_no_windows_asset"),
            },
            Err(error) => println!("update_download_error={error}"),
        }
        return Ok(());
    }

    // ========== 命令行诊断分支：--loader-report <mc版本> <fabric|forge|neoforge> ==========
    // 拉取对应加载器可用版本列表，打印数量与前8个版本号
    if let Some(index) = std::env::args().position(|arg| arg == "--loader-report") {
        let args = std::env::args().collect::<Vec<_>>();
        let game_version = args.get(index + 1).cloned();
        let loader = args.get(index + 2).and_then(|value| match value.as_str() {
            "fabric" => Some(loader::LoaderKind::Fabric),
            "forge" => Some(loader::LoaderKind::Forge),
            "neoforge" => Some(loader::LoaderKind::NeoForge),
            _ => None,
        });
        match (game_version, loader) {
            (Some(game_version), Some(loader)) => {
                let receiver = loader::start_fetch_versions(game_version, loader);
                match receiver.recv() {
                    Ok(Ok(versions)) => {
                        println!("{} versions={}", loader.label(), versions.len());
                        for version in versions.iter().take(8) {
                            println!("{version}");
                        }
                    }
                    Ok(Err(error)) => println!("loader_error={error}"),
                    Err(error) => println!("loader_error={error}"),
                }
            }
            _ => println!(
                "loader_error=usage --loader-report <minecraft-version> <fabric|forge|neoforge>"
            ),
        }
        return Ok(());
    }

    // ========== 命令行诊断分支：--loader-install-report <目录> <mc版本> <加载器> <加载器版本> ==========
    // 静默安装加载器，流式打印安装阶段，最后输出成功/失败结果
    if let Some(index) = std::env::args().position(|arg| arg == "--loader-install-report") {
        let args = std::env::args().collect::<Vec<_>>();
        let directory = args.get(index + 1).map(std::path::PathBuf::from);
        let game_version = args.get(index + 2).cloned();
        let loader = args.get(index + 3).and_then(|value| match value.as_str() {
            "fabric" => Some(loader::LoaderKind::Fabric),
            "forge" => Some(loader::LoaderKind::Forge),
            "neoforge" => Some(loader::LoaderKind::NeoForge),
            _ => None,
        });
        let loader_version = args.get(index + 4).cloned();
        match (directory, game_version, loader, loader_version) {
            (Some(directory), Some(game_version), Some(loader), Some(loader_version)) => {
                let receiver =
                    loader::start_install(directory, game_version, loader, loader_version);
                while let Ok(update) = receiver.recv() {
                    println!("{}", update.stage);
                    if update.finished {
                        println!("result={} {}", update.success, update.message);
                        break;
                    }
                }
            }
            _ => println!(
                "loader_install_error=usage --loader-install-report <dir> <game> <loader> <version>"
            ),
        }
        return Ok(());
    }

    // ========== 命令行诊断分支：--manifest-report ==========
    // 拉取Minecraft官方版本清单，打印前8个版本（类型+id）
    if std::env::args().any(|arg| arg == "--manifest-report") {
        match download::fetch_official_versions() {
            Ok(versions) => {
                for version in versions.iter().take(8) {
                    println!("{} {}", version.kind, version.id);
                }
            }
            Err(error) => println!("manifest_error={error}"),
        }
        return Ok(());
    }

    // ========== 命令行诊断分支：--modrinth-report [关键词] [mc版本] ==========
    // 搜索Modrinth模组资源，默认关键词fabric、默认版本1.21.1，超时35秒，输出结果数量和前5项
    if let Some(index) = std::env::args().position(|arg| arg == "--modrinth-report") {
        let query = std::env::args()
            .nth(index + 1)
            .unwrap_or_else(|| "fabric".to_owned());
        let version = std::env::args()
            .nth(index + 2)
            .unwrap_or_else(|| "1.21.1".to_owned());
        let receiver =
            resources::start_search(resources::ResourceCategory::Mod, query, version, None);
        match receiver.recv_timeout(std::time::Duration::from_secs(35)) {
            Ok(Ok(projects)) => {
                println!("modrinth_results={}", projects.len());
                for project in projects.iter().take(5) {
                    println!("{}\t{}", project.title, project.id);
                }
            }
            Ok(Err(error)) => println!("modrinth_error={error}"),
            Err(error) => println!("modrinth_error={error}"),
        }
        return Ok(());
    }

    // ========== 命令行诊断分支：--runtime-report [组件名] ==========
    // 查询Java运行时清单信息，默认组件 java-runtime-delta
    if let Some(index) = std::env::args().position(|arg| arg == "--runtime-report") {
        let component = std::env::args()
            .nth(index + 1)
            .unwrap_or_else(|| "java-runtime-delta".to_owned());
        match download::java_runtime_manifest_report(&component) {
            Ok(report) => println!("{report}"),
            Err(error) => println!("runtime_error={error}"),
        }
        return Ok(());
    }

    // ========== 命令行诊断分支：--runtime-install-report <目录> <组件> <主版本号> ==========
    // 静默安装指定Java运行时
    if let Some(index) = std::env::args().position(|arg| arg == "--runtime-install-report") {
        let args = std::env::args().collect::<Vec<_>>();
        let directory = args.get(index + 1).map(std::path::PathBuf::from);
        let component = args.get(index + 2).map(String::as_str);
        let major = args
            .get(index + 3)
            .and_then(|value| value.parse::<u32>().ok());
        match (directory, component, major) {
            (Some(directory), Some(component), Some(major)) => {
                match download::install_java_runtime_for_report(&directory, component, major) {
                    Ok(()) => println!("runtime_install=ok"),
                    Err(error) => println!("runtime_install_error={error}"),
                }
            }
            _ => println!(
                "runtime_install_error=usage --runtime-install-report <directory> <component> <major>"
            ),
        }
        return Ok(());
    }

    // ========== 命令行诊断分支：--install-report <目录> <版本ID> ==========
    // 静默安装原版Minecraft版本，流式输出阶段、百分比、文件进度，完成打印结果
    if let Some(index) = std::env::args().position(|arg| arg == "--install-report") {
        let args = std::env::args().collect::<Vec<_>>();
        let directory = args.get(index + 1).map(std::path::PathBuf::from);
        let version = args.get(index + 2).cloned();
        match (directory, version) {
            (Some(directory), Some(version)) => {
                let receiver = download::start_install_by_id(directory, version);
                let mut last_percent = u64::MAX;
                while let Ok(update) = receiver.recv() {
                    let percent = update
                        .completed_bytes
                        .saturating_mul(100)
                        .checked_div(update.total_bytes)
                        .unwrap_or(0);
                    // 只在百分比变化时打印，避免刷屏
                    if percent != last_percent {
                        println!(
                            "{} {percent}% {}/{}",
                            update.stage, update.files_done, update.files_total
                        );
                        last_percent = percent;
                    }
                    if update.finished {
                        println!("result={} {}", update.success, update.message);
                        break;
                    }
                }
            }
            _ => println!("install_error=usage --install-report <directory> <version>"),
        }
        return Ok(());
    }

    // ========== 命令行诊断分支：--launch-now [游戏目录] ==========
    // 直接启动游戏，不打开GUI，打印版本、进程PID、Java名称
    if std::env::args().any(|arg| arg == "--launch-now") {
        let requested = std::env::args()
            .position(|arg| arg == "--launch-now")
            .and_then(|index| std::env::args().nth(index + 1))
            .map(std::path::PathBuf::from);
        if let Some(directory) = requested.or_else(game::discover_game_directory) {
            let launchable = game::launchable_versions(&directory);
            if let Some(version) = launchable.first() {
                let options = game::LaunchOptions::default();
                match game::launch(&directory, version, "原版", &options) {
                    Ok(details) => {
                        println!(
                            "launched={} pid={} java={}",
                            details.version, details.process_id, details.java_name
                        );
                    }
                    Err(error) => println!("launch_error={error}"),
                }
            } else {
                println!("launch_error=no_launchable_versions");
            }
        } else {
            println!("launch_error=game_directory_not_found");
        }
        return Ok(());
    }

    // ========== 无命令行参数：启动GUI ==========
    // 解析 --size WxH 参数，设置窗口初始大小；默认 1440x900
    let requested_size = std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find_map(|pair| (pair[0] == "--size").then_some(pair[1].as_str()))
        .and_then(parse_size)
        .unwrap_or([1440.0, 900.0]);

    // 视口配置：无边框、最小尺寸、最大画布尺寸、窗口图标
    let viewport = egui::ViewportBuilder::default()
        .with_title(brand::NAME)
        .with_inner_size(requested_size)
        .with_min_inner_size([1080.0, 720.0])
        .with_max_inner_size(vector::ARTBOARD_SIZE)
        .with_decorations(false)
        // 固定窗口尺寸：不允许系统边缘缩放或最大化。
        .with_resizable(false)
        .with_transparent(true)
        .with_icon(app_icon());

    // eframe 原生窗口选项：WGPU 透明渲染器、4 倍 MSAA 抗锯齿、居中。
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        centered: true,
        multisampling: 4,
        ..Default::default()
    };

    // 运行GUI主循环，构造LauncherApp实例
    eframe::run_native(
        brand::NAME,
        options,
        Box::new(|cc| Ok(Box::new(LauncherApp::new(cc)))),
    )
}

/// Windows平台：启用每显示器独立DPI感知V2，高分屏缩放正常
#[cfg(target_os = "windows")]
fn enable_per_monitor_dpi() {
    use windows_sys::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
    };
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

/// 非Windows平台：空实现
#[cfg(not(target_os = "windows"))]
fn enable_per_monitor_dpi() {}

/// Windows：获取桌面工作区域（排除任务栏），返回左上角坐标 + 宽高
#[cfg(target_os = "windows")]
pub fn desktop_work_area() -> Option<([f32; 2], [f32; 2])> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SPI_GETWORKAREA, SystemParametersInfoW};

    let mut rect = windows_sys::Win32::Foundation::RECT::default();
    let succeeded = unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            (&mut rect as *mut windows_sys::Win32::Foundation::RECT).cast(),
            0,
        )
    };
    if succeeded == 0 {
        return None;
    }
    Some((
        [rect.left as f32, rect.top as f32],
        [
            (rect.right - rect.left) as f32,
            (rect.bottom - rect.top) as f32,
        ],
    ))
}

/// 非Windows平台：暂不实现桌面工作区读取
#[cfg(not(target_os = "windows"))]
pub fn desktop_work_area() -> Option<([f32; 2], [f32; 2])> {
    None
}

/// 解析尺寸字符串 "WxH" 为 [width, height]
fn parse_size(value: &str) -> Option<[f32; 2]> {
    let (width, height) = value.split_once('x')?;
    Some([width.parse().ok()?, height.parse().ok()?])
}

/// 加载窗口图标：优先读取 Grass.png，读取失败回退到内置矢量绘制的草方块图标
fn app_icon() -> egui::IconData {
    if let Ok(image) = image::load_from_memory(include_bytes!("../Grass.png")) {
        let rgba = image
            .resize_exact(256, 256, image::imageops::FilterType::Lanczos3)
            .to_rgba8();
        return egui::IconData {
            rgba: rgba.into_raw(),
            width: 256,
            height: 256,
        };
    }
    // 回退图标：64×64草方块矢量绘制
    /// 提供 SIZE 使用的只读常量。
    ///
    /// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
    const SIZE: usize = 64;
    let mut pixels = vec![0_u8; SIZE * SIZE * 4];

    // 绘制外边框深色背景
    for y in 0..SIZE {
        for x in 0..SIZE {
            let edge = x.min(SIZE - 1 - x).min(y.min(SIZE - 1 - y));
            if edge < 4 {
                continue;
            }
            set_icon_pixel(&mut pixels, x, y, [23, 32, 26, 255]);
        }
    }

    let center = [32.0_f32, 28.0_f32];
    // 绘制草方块顶面、左侧、右侧斜面
    for y in 0..SIZE {
        for x in 0..SIZE {
            let px = x as f32 - center[0];
            let py = y as f32 - center[1];
            let top = point_in_quad(
                [px, py],
                [[0.0, -15.0], [22.0, 0.0], [0.0, 15.0], [-22.0, 0.0]],
            );
            let left = point_in_quad(
                [px, py],
                [[-22.0, 0.0], [0.0, 15.0], [0.0, 31.0], [-22.0, 16.0]],
            );
            let right = point_in_quad(
                [px, py],
                [[22.0, 0.0], [0.0, 15.0], [0.0, 31.0], [22.0, 16.0]],
            );
            if top {
                set_icon_pixel(&mut pixels, x, y, [126, 218, 105, 255]);
            } else if left {
                set_icon_pixel(
                    &mut pixels,
                    x,
                    y,
                    if py < 20.0 {
                        [64, 134, 65, 255]
                    } else {
                        [91, 66, 47, 255]
                    },
                );
            } else if right {
                set_icon_pixel(
                    &mut pixels,
                    x,
                    y,
                    if py < 20.0 {
                        [78, 157, 78, 255]
                    } else {
                        [112, 81, 55, 255]
                    },
                );
            }
        }
    }

    egui::IconData {
        rgba: pixels,
        width: SIZE as u32,
        height: SIZE as u32,
    }
}

/// 点是否在四边形内部：环绕同向检测（2D叉积判断 winding）
fn point_in_quad(point: [f32; 2], quad: [[f32; 2]; 4]) -> bool {
    let mut sign = 0.0_f32;
    for index in 0..4 {
        let a = quad[index];
        let b = quad[(index + 1) % 4];
        let cross = (b[0] - a[0]) * (point[1] - a[1]) - (b[1] - a[1]) * (point[0] - a[0]);
        if cross.abs() < 0.01 {
            continue;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if sign != cross.signum() {
            return false;
        }
    }
    true
}

/// 在像素缓冲区指定坐标写入RGBA像素
fn set_icon_pixel(pixels: &mut [u8], x: usize, y: usize, color: [u8; 4]) {
    let offset = (y * 64 + x) * 4;
    pixels[offset..offset + 4].copy_from_slice(&color);
}

/// Windows：关闭原生窗口圆角，外轮廓由启动器自绘 WGPU 遮罩决定。
///
/// WGPU 交换链提供透明合成，窗口半径不再受 Windows 小/标准两档限制。
/// DWM 只负责隐藏原生边框和非客户区，不参与窗口圆角形状。
#[cfg(target_os = "windows")]
pub fn apply_rounded_window(corner_radius: u16, pixels_per_point: f32) -> bool {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Dwm::{
        DWMNCRP_DISABLED, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE, DWMWA_NCRENDERING_POLICY,
        DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND, DwmSetWindowAttribute,
    };
    use windows_sys::Win32::Graphics::Gdi::{CreateRoundRectRgn, DeleteObject, SetWindowRgn};
    use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, GetWindowRect};

    let title: Vec<u16> = format!("{}\0", brand::NAME).encode_utf16().collect();
    let window = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
    if window.is_null() {
        return false;
    }

    unsafe {
        let non_client_policy = DWMNCRP_DISABLED;
        let non_client_result = DwmSetWindowAttribute(
            window,
            DWMWA_NCRENDERING_POLICY as u32,
            (&non_client_policy as *const i32).cast(),
            std::mem::size_of_val(&non_client_policy) as u32,
        );
        let mut window_rect = RECT::default();
        if GetWindowRect(window, &mut window_rect) == 0 {
            return false;
        }
        let width = (window_rect.right - window_rect.left).max(1);
        let height = (window_rect.bottom - window_rect.top).max(1);
        let radius_px = corner_radius.max(1) as f32 * pixels_per_point.max(0.5);
        let diameter = ((radius_px + 4.0) * 2.0)
            .round()
            .clamp(2.0, width.min(height) as f32) as i32;
        // 该区域只裁掉系统最外圈；多出的 4px 半径保证二值边界位于
        // WGPU 平滑 Alpha 羽化之外，不会重新产生窗口锯齿。
        let region = CreateRoundRectRgn(1, 1, width, height, diameter, diameter);
        if region.is_null() {
            return false;
        }
        if SetWindowRgn(window, region, 1) == 0 {
            let _ = DeleteObject(region);
            return false;
        }
        let preference = DWMWCP_DONOTROUND;
        let corner_result = DwmSetWindowAttribute(
            window,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            (&preference as *const i32).cast(),
            std::mem::size_of_val(&preference) as u32,
        );
        let border_color = DWMWA_COLOR_NONE;
        let border_result = DwmSetWindowAttribute(
            window,
            DWMWA_BORDER_COLOR as u32,
            (&border_color as *const u32).cast(),
            std::mem::size_of_val(&border_color) as u32,
        );
        corner_result == 0 || border_result == 0 || non_client_result == 0
    }
}

/// 将 Windows 窗口最外圈边框设为状态栏当前颜色。
///
/// WGPU 自定义遮罩负责圆角，DWM 只保留一圈不可关闭的系统边框；同步其颜色
/// 可避免这 1px 边框在背景图上显示为黑条或白条。
#[cfg(target_os = "windows")]
pub fn set_window_border_color(rgb: [u8; 3]) -> bool {
    use windows_sys::Win32::Graphics::Dwm::{DWMWA_BORDER_COLOR, DwmSetWindowAttribute};
    use windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW;

    let title: Vec<u16> = format!("{}\0", brand::NAME).encode_utf16().collect();
    let window = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
    if window.is_null() {
        return false;
    }
    let color = (rgb[2] as u32) << 16 | (rgb[1] as u32) << 8 | rgb[0] as u32;
    unsafe {
        DwmSetWindowAttribute(
            window,
            DWMWA_BORDER_COLOR as u32,
            (&color as *const u32).cast(),
            std::mem::size_of_val(&color) as u32,
        ) == 0
    }
}

#[cfg(not(target_os = "windows"))]
pub fn set_window_border_color(_rgb: [u8; 3]) -> bool {
    false
}

/// 非 Windows 平台暂不设置原生窗口区域。
#[cfg(not(target_os = "windows"))]
pub fn apply_rounded_window(_corner_radius: u16, _pixels_per_point: f32) -> bool {
    false
}
