#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod brand;
mod download;
mod favorites;
mod game;
mod loader;
mod modpack;
mod news;
mod resources;
mod scene;
mod sessions;
mod theme;
mod update;
mod vector;

use app::LauncherApp;
use eframe::egui;

fn main() -> eframe::Result {
    enable_per_monitor_dpi();

    if std::env::args().any(|arg| arg == "--game-report") {
        let requested = std::env::args()
            .position(|arg| arg == "--game-report")
            .and_then(|index| std::env::args().nth(index + 1))
            .map(std::path::PathBuf::from);
        if let Some(directory) = requested.or_else(game::discover_game_directory) {
            let versions = game::installed_versions(&directory);
            let launchable = game::launchable_versions(&directory);
            println!("game_directory={}", directory.display());
            println!("installed_versions={}", versions.join(", "));
            println!("launchable_versions={}", launchable.join(", "));
            if let Some(version) = versions.first() {
                println!(
                    "installation_complete={}",
                    game::installation_complete(&directory, version, "原版")
                );
            }
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

    let requested_size = std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find_map(|pair| (pair[0] == "--size").then_some(pair[1].as_str()))
        .and_then(parse_size)
        .unwrap_or([1440.0, 900.0]);

    let viewport = egui::ViewportBuilder::default()
        .with_title(brand::NAME)
        .with_inner_size(requested_size)
        .with_min_inner_size([1080.0, 720.0])
        .with_max_inner_size(vector::ARTBOARD_SIZE)
        .with_decorations(false)
        .with_resizable(true)
        .with_transparent(false)
        .with_icon(app_icon());

    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        centered: true,
        multisampling: 4,
        ..Default::default()
    };

    eframe::run_native(
        brand::NAME,
        options,
        Box::new(|cc| Ok(Box::new(LauncherApp::new(cc)))),
    )
}

#[cfg(target_os = "windows")]
fn enable_per_monitor_dpi() {
    use windows_sys::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
    };
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

#[cfg(not(target_os = "windows"))]
fn enable_per_monitor_dpi() {}

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

#[cfg(not(target_os = "windows"))]
pub fn desktop_work_area() -> Option<([f32; 2], [f32; 2])> {
    None
}

fn parse_size(value: &str) -> Option<[f32; 2]> {
    let (width, height) = value.split_once('x')?;
    Some([width.parse().ok()?, height.parse().ok()?])
}

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
    const SIZE: usize = 64;
    let mut pixels = vec![0_u8; SIZE * SIZE * 4];

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

fn set_icon_pixel(pixels: &mut [u8], x: usize, y: usize, color: [u8; 4]) {
    let offset = (y * 64 + x) * 4;
    pixels[offset..offset + 4].copy_from_slice(&color);
}

#[cfg(target_os = "windows")]
pub fn apply_rounded_window() -> bool {
    use windows_sys::Win32::Graphics::Dwm::{
        DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmExtendFrameIntoClientArea,
        DwmSetWindowAttribute,
    };
    use windows_sys::Win32::UI::Controls::MARGINS;
    use windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW;

    let title: Vec<u16> = format!("{}\0", brand::NAME).encode_utf16().collect();
    let window = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
    if window.is_null() {
        return false;
    }

    let preference = DWMWCP_ROUND;
    let margins = MARGINS {
        cxLeftWidth: 1,
        cxRightWidth: 1,
        cyTopHeight: 1,
        cyBottomHeight: 1,
    };
    unsafe {
        DwmExtendFrameIntoClientArea(window, &margins);
        DwmSetWindowAttribute(
            window,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            (&preference as *const i32).cast(),
            std::mem::size_of_val(&preference) as u32,
        ) == 0
    }
}

#[cfg(not(target_os = "windows"))]
pub fn apply_rounded_window() -> bool {
    false
}
