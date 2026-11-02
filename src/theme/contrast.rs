//! # `src/theme/contrast.rs`
//!
//! **功能域：** 自适应文字对比度。
//!
//! 本模块记录当前帧的背景图亮度和已经绘制的圆角表面，在文字落笔前估算该位置
//! 实际可见的底色。若原文字色与底色对比不足，则只调整亮度并尽量保留原有色相，
//! 使文字在背景图、卡片、按钮和半透明玻璃上都能保持可读。
//!
//! ## 模块职责
//! - 把用户背景图转换成低分辨率颜色采样表，避免每段文字都读取原始大图。
//! - 按绘制顺序记录当前帧的表面颜色，支持半透明卡片与按钮的叠加计算。
//! - 为每个文字位置选择满足最低对比度的文字颜色。
//!
//! ## 错误与并发
//! - 背景采样表由进程级读写锁保护；缺失或锁不可用时安全回退到白底。
//! - 当前帧表面栈只由 egui 绘制线程访问，因此使用线程局部存储。

use std::cell::RefCell;
use std::sync::{OnceLock, RwLock};

use eframe::egui::{Color32, Painter, Pos2, Rect};

/// 背景图最多保留的采样边长。文字对比度不需要原始分辨率，限制采样表可避免
/// 每次绘制文字时扫描数百万像素。
const MAX_BACKGROUND_SAMPLE_SIDE: usize = 256;

/// 背景图绘制后覆盖的一层黑色遮罩，与 `shell.rs` 的渲染顺序保持一致。
const BACKGROUND_OVERLAY_ALPHA: u8 = 52;

/// 普通字号文字的 WCAG 对比度下限。
const NORMAL_TEXT_MIN_CONTRAST: f32 = 4.5;

/// 大字号文字的对比度下限。大字号更容易辨认，因此采用 WCAG 的 3:1 标准。
const LARGE_TEXT_MIN_CONTRAST: f32 = 3.0;

/// 已经缩放并扁平化到 RGB 的背景图采样表。
#[derive(Clone)]
struct BackgroundSample {
    width: usize,
    height: usize,
    pixels: Vec<[u8; 3]>,
    average: [u8; 3],
}

/// 当前帧记录的一个可见填充表面。
#[derive(Clone, Copy)]
struct SurfaceSample {
    rect: Rect,
    color: Color32,
}

/// 单帧文字环境。`surfaces` 的先后顺序与 egui 绘制顺序一致。
struct FrameTextState {
    screen: Rect,
    background_image_active: bool,
    surfaces: Vec<SurfaceSample>,
}

impl Default for FrameTextState {
    fn default() -> Self {
        Self {
            screen: Rect::NOTHING,
            background_image_active: false,
            surfaces: Vec::new(),
        }
    }
}

/// 进程级背景图采样表。没有导入背景图时为 `None`。
static BACKGROUND_SAMPLE: OnceLock<RwLock<Option<BackgroundSample>>> = OnceLock::new();

thread_local! {
    /// 绘制发生在 egui 的界面线程，线程局部状态可以避免每个文字片段都争抢全局锁。
    static FRAME_TEXT_STATE: RefCell<FrameTextState> = RefCell::new(FrameTextState::default());
}

fn background_sample() -> &'static RwLock<Option<BackgroundSample>> {
    BACKGROUND_SAMPLE.get_or_init(|| RwLock::new(None))
}

/// 开始新一帧文字绘制。
///
/// `image_active` 必须与本帧是否真的绘制了用户背景图一致。强制更新页等不显示
/// 背景图的界面会传入 `false`，防止文字根据不可见的图片错误变色。
pub(crate) fn begin_text_frame(screen: Rect, image_active: bool) {
    FRAME_TEXT_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.screen = screen;
        state.background_image_active = image_active;
        state.surfaces.clear();
    });
}

/// 注册一个刚刚绘制的填充表面，供后续文字估算局部底色。
///
/// 极小的进度条和装饰点不会显著影响整段文字的可读性，记录它们反而可能让
/// 同一个词内部出现不同颜色，因此只登记有实际文字承载意义的面积。
pub(super) fn register_surface(painter: &Painter, rect: Rect, color: Color32) {
    if color.a() == 0
        || !rect.min.x.is_finite()
        || !rect.min.y.is_finite()
        || !rect.max.x.is_finite()
        || !rect.max.y.is_finite()
        || rect.width() <= 0.0
        || rect.height() <= 0.0
        || rect.width() * rect.height() < 24.0
    {
        return;
    }
    FRAME_TEXT_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.screen == Rect::NOTHING {
            state.screen = painter.ctx().content_rect();
        }
        // 一帧中出现异常多的填充时主动丢弃最早记录，保持内存和文字采样耗时稳定。
        if state.surfaces.len() >= 4096 {
            state.surfaces.drain(..1024);
        }
        state.surfaces.push(SurfaceSample { rect, color });
    });
}

/// 导入或更换背景图后更新文字对比度采样表。
pub(crate) fn set_background_image(image: &eframe::egui::ColorImage) {
    let sample = build_background_sample(image);
    if let Ok(mut current) = background_sample().write() {
        *current = Some(sample);
    }
}

/// 移除背景图后清除文字对比度采样表。
pub(crate) fn clear_background_image() {
    if let Ok(mut current) = background_sample().write() {
        *current = None;
    }
}

/// 在指定屏幕位置自动选择可读文字色。
///
/// `font_size` 用于选择普通文字或大字号的最低对比度标准。函数会先尝试保留
/// 调用方指定的色相，只有对比度不足时才逐步向黑或白过渡。
pub(crate) fn adaptive_text_color_at(
    painter: &Painter,
    position: Pos2,
    requested: Color32,
    font_size: f32,
) -> Color32 {
    if requested.a() < 16 {
        return requested;
    }
    let background = effective_background_color(painter, position);
    let minimum = if font_size >= 18.0 {
        LARGE_TEXT_MIN_CONTRAST
    } else {
        NORMAL_TEXT_MIN_CONTRAST
    };
    improve_contrast(requested, background, minimum)
}

/// 没有精确文字位置时使用背景图平均色选择可读文字色。
///
/// 该入口保留给图标线条等非文字调用；常规文字应优先使用
/// [`adaptive_text_color_at`] 读取对应位置的局部背景。
pub(crate) fn adaptive_text_color(requested: Color32) -> Color32 {
    if requested.a() < 16 {
        return requested;
    }
    let image_active = FRAME_TEXT_STATE.with(|state| state.borrow().background_image_active);
    let background = if image_active {
        background_sample()
            .read()
            .ok()
            .and_then(|sample| sample.as_ref().map(|sample| sample.average))
            .map(apply_background_overlay)
    } else {
        None
    }
    .unwrap_or([255, 255, 255]);
    improve_contrast(requested, background, NORMAL_TEXT_MIN_CONTRAST)
}

/// 计算文字位置最终可见的 RGB 底色。
fn effective_background_color(painter: &Painter, position: Pos2) -> [u8; 3] {
    FRAME_TEXT_STATE.with(|state| {
        let state = state.borrow();
        let screen = frame_screen(painter, &state);
        let mut color = base_background_color(&state, screen, position);
        for surface in &state.surfaces {
            if surface.rect.contains(position) {
                color = blend_over(color, surface.color);
            }
        }
        color
    })
}

fn frame_screen(painter: &Painter, state: &FrameTextState) -> Rect {
    if state.screen == Rect::NOTHING {
        painter.ctx().content_rect()
    } else {
        state.screen
    }
}

fn base_background_color(state: &FrameTextState, screen: Rect, position: Pos2) -> [u8; 3] {
    if state.background_image_active {
        sample_background_color(screen, position)
            .map(apply_background_overlay)
            .unwrap_or([255, 255, 255])
    } else {
        [255, 255, 255]
    }
}

/// 返回状态栏位置实际可见的背景色，供 Windows 顶部系统边框同步使用。
pub(crate) fn color_at_position(painter: &Painter, position: Pos2) -> [u8; 3] {
    FRAME_TEXT_STATE.with(|state| {
        let state = state.borrow();
        let screen = frame_screen(painter, &state);
        base_background_color(&state, screen, position)
    })
}

/// 根据背景图的 cover 绘制方式，把屏幕坐标映射到采样表坐标。
fn sample_background_color(screen: Rect, position: Pos2) -> Option<[u8; 3]> {
    if screen.width() <= 1.0 || screen.height() <= 1.0 {
        return None;
    }
    let sample = background_sample().read().ok()?;
    let sample = sample.as_ref()?;
    let source_aspect = sample.width as f32 / sample.height.max(1) as f32;
    let target_aspect = screen.width() / screen.height();
    let mut u = ((position.x - screen.left()) / screen.width()).clamp(0.0, 1.0);
    let mut v = ((position.y - screen.top()) / screen.height()).clamp(0.0, 1.0);
    if source_aspect > target_aspect {
        let visible = target_aspect / source_aspect;
        u = (1.0 - visible) * 0.5 + u * visible;
    } else {
        let visible = source_aspect / target_aspect;
        v = (1.0 - visible) * 0.5 + v * visible;
    }
    let center_x = (u * sample.width.saturating_sub(1) as f32).round() as usize;
    let center_y = (v * sample.height.saturating_sub(1) as f32).round() as usize;
    let mut sum = [0u32; 3];
    let mut count = 0u32;
    for y in center_y.saturating_sub(1)..=(center_y + 1).min(sample.height.saturating_sub(1)) {
        for x in center_x.saturating_sub(1)..=(center_x + 1).min(sample.width.saturating_sub(1)) {
            let pixel = sample.pixels[y * sample.width + x];
            sum[0] += pixel[0] as u32;
            sum[1] += pixel[1] as u32;
            sum[2] += pixel[2] as u32;
            count += 1;
        }
    }
    (count > 0).then(|| {
        [
            (sum[0] / count) as u8,
            (sum[1] / count) as u8,
            (sum[2] / count) as u8,
        ]
    })
}

/// 把原始背景图按需缩小到固定上限，并预计算平均色。
fn build_background_sample(image: &eframe::egui::ColorImage) -> BackgroundSample {
    let source_width = image.size[0].max(1);
    let source_height = image.size[1].max(1);
    let scale =
        (MAX_BACKGROUND_SAMPLE_SIDE as f32 / source_width.max(source_height) as f32).min(1.0);
    let width = ((source_width as f32 * scale).round() as usize).max(1);
    let height = ((source_height as f32 * scale).round() as usize).max(1);
    let mut pixels = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let source_x = (x * source_width / width).min(source_width - 1);
            let source_y = (y * source_height / height).min(source_height - 1);
            let pixel = image.pixels[source_y * source_width + source_x];
            pixels.push(flatten_over_white(pixel));
        }
    }
    let mut average = [0u64; 3];
    for pixel in &pixels {
        average[0] += pixel[0] as u64;
        average[1] += pixel[1] as u64;
        average[2] += pixel[2] as u64;
    }
    let count = pixels.len().max(1) as u64;
    BackgroundSample {
        width,
        height,
        pixels,
        average: [
            (average[0] / count) as u8,
            (average[1] / count) as u8,
            (average[2] / count) as u8,
        ],
    }
}

fn flatten_over_white(color: Color32) -> [u8; 3] {
    blend_over([255, 255, 255], color)
}

fn apply_background_overlay(color: [u8; 3]) -> [u8; 3] {
    blend_over(color, Color32::from_black_alpha(BACKGROUND_OVERLAY_ALPHA))
}

/// 将 source 按自身 alpha 叠加到 destination 上。
fn blend_over(destination: [u8; 3], source: Color32) -> [u8; 3] {
    let [r, g, b, a] = source.to_array();
    let alpha = a as f32 / 255.0;
    let inverse = 1.0 - alpha;
    [
        (r as f32 * alpha + destination[0] as f32 * inverse).round() as u8,
        (g as f32 * alpha + destination[1] as f32 * inverse).round() as u8,
        (b as f32 * alpha + destination[2] as f32 * inverse).round() as u8,
    ]
}

/// 若原色对比度不足，则逐级向黑或白混合，保留尽可能多的原色相。
fn improve_contrast(requested: Color32, background: [u8; 3], minimum: f32) -> Color32 {
    if contrast_ratio(color_rgb(requested), background) >= minimum {
        return requested;
    }
    let background_luminance = relative_luminance(background);
    let target = if background_luminance > 0.42 {
        [20, 23, 20]
    } else {
        [247, 250, 247]
    };
    let requested_rgb = color_rgb(requested);
    for step in 1..=12 {
        let amount = step as f32 / 12.0;
        let candidate = [
            mix_channel(requested_rgb[0], target[0], amount),
            mix_channel(requested_rgb[1], target[1], amount),
            mix_channel(requested_rgb[2], target[2], amount),
        ];
        if contrast_ratio(candidate, background) >= minimum {
            return Color32::from_rgba_unmultiplied(
                candidate[0],
                candidate[1],
                candidate[2],
                requested.a(),
            );
        }
    }
    Color32::from_rgba_unmultiplied(target[0], target[1], target[2], requested.a())
}

fn color_rgb(color: Color32) -> [u8; 3] {
    let [r, g, b, _] = color.to_array();
    [r, g, b]
}

fn mix_channel(from: u8, to: u8, amount: f32) -> u8 {
    (from as f32 + (to as f32 - from as f32) * amount)
        .round()
        .clamp(0.0, 255.0) as u8
}

/// 使用 WCAG 的线性相对亮度计算对比度。
fn contrast_ratio(first: [u8; 3], second: [u8; 3]) -> f32 {
    let first = relative_luminance(first);
    let second = relative_luminance(second);
    let (lighter, darker) = if first >= second {
        (first, second)
    } else {
        (second, first)
    };
    (lighter + 0.05) / (darker + 0.05)
}

fn relative_luminance(color: [u8; 3]) -> f32 {
    let linear = color.map(|channel| {
        let value = channel as f32 / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    });
    linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_text_is_lightened_on_dark_background() {
        let adjusted = improve_contrast(
            Color32::from_rgb(24, 27, 24),
            [18, 22, 20],
            NORMAL_TEXT_MIN_CONTRAST,
        );
        assert!(contrast_ratio(color_rgb(adjusted), [18, 22, 20]) >= NORMAL_TEXT_MIN_CONTRAST);
    }

    #[test]
    fn readable_custom_color_is_preserved() {
        let requested = Color32::from_rgb(28, 34, 28);
        let adjusted = improve_contrast(requested, [255, 255, 255], NORMAL_TEXT_MIN_CONTRAST);
        assert_eq!(adjusted, requested);
    }
}
