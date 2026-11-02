//! # `src/theme/mod.rs`
//!
//! **功能域：** 颜色、字体、玻璃表面、指针光效和屏幕空间着色器。
//!
//! 主题公共入口。集中管理颜色模式、玻璃材质、着色器程序、圆角表面和按钮视觉。
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `GREEN`
//! - `GREEN_BRIGHT`
//! - `DEFAULT_CORNER_RADIUS`
//! - `MIN_CORNER_RADIUS`
//! - `MAX_CORNER_RADIUS`
//! - `corner_radius`
//! - `AppearanceMode`
//! - `fn`
//! - `from_stored`
//! - `configure_effects`
//! - `appearance_mode`
//! - `background`
//! - `ink`
//! - `ink_soft`
//! - `muted`
//! - `line`
//! - `line_strong`
//! - `setup_glass`
//! - `GlassSpec`
//! - `new`
//! - `hovered`
//!
//! ## 错误与并发
//! - 可失败操作返回 ``Result<_, String>``（或其他明确错误类型），错误文本用于日志和界面提示。
//! - 实现中包含线程、通道或共享状态；同步边界必须与现有轮询和生命周期保持一致。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use std::sync::atomic::{AtomicU8, AtomicU16, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use eframe::egui::{self, Color32, CornerRadius, Painter, Rect, Stroke, StrokeKind, Vec2};
use eframe::glow::HasContext;

mod contrast;
mod fonts;
mod pointer;
mod surface;
mod window_mask_wgpu;

pub(crate) use contrast::{
    adaptive_text_color, adaptive_text_color_at, begin_text_frame, clear_background_image,
    color_at_position, set_background_image,
};
pub use fonts::{bold_font, font, mono_font, setup};
pub use pointer::{pointer_glow, pointer_glow_overlay};
pub use surface::{
    alpha, black_alpha, bold_text, center_text, center_text_exact, fill_round, horizontal_fade,
    mono_text, soft_orb, stroke_round, text, white_alpha, wrapped_text,
};

/// 提供 GREEN 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub const GREEN: Color32 = Color32::from_rgb(100, 216, 92);
/// 提供 GREENBRIGHT 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub const GREEN_BRIGHT: Color32 = Color32::from_rgb(155, 242, 130);
/// 默认圆角半径。窗口外框、卡片、按钮和输入框统一使用该逻辑像素值。
pub const DEFAULT_CORNER_RADIUS: u8 = 16;

/// 设置界面允许的最小圆角半径，避免用户误调成尖角窗口。
pub const MIN_CORNER_RADIUS: u8 = 4;

/// 设置界面允许的最大圆角半径。
pub const MAX_CORNER_RADIUS: u8 = 32;

/// 提供 玻璃TRANSPARENCY 使用的进程级静态状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
static GLASS_TRANSPARENCY: AtomicU16 = AtomicU16::new(82);
/// 提供 卡片EDGECLARITY 使用的进程级静态状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
static CARD_EDGE_CLARITY: AtomicU16 = AtomicU16::new(55);
/// 提供 指针BRIGHTNESS 使用的进程级静态状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
static POINTER_BRIGHTNESS: AtomicU16 = AtomicU16::new(100);
/// 提供 指针RADIUS 使用的进程级静态状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
static POINTER_RADIUS: AtomicU16 = AtomicU16::new(155);
/// 提供 APPEARANCEMODE 使用的进程级静态状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
static APPEARANCE_MODE: AtomicU8 = AtomicU8::new(AppearanceMode::Simple as u8);
/// 当前全局圆角半径（逻辑像素）。每帧由持久化设置同步到该原子值。
static CORNER_RADIUS: AtomicU8 = AtomicU8::new(DEFAULT_CORNER_RADIUS);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
/// 定义 AppearanceMode 可选状态或事件集合。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub enum AppearanceMode {
    Simple = 0,
    Frosted = 1,
    Liquid = 2,
}

impl AppearanceMode {
    /// 提供 fn 使用的只读常量。
    ///
    /// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Simple => "简洁",
            Self::Frosted => "磨砂玻璃",
            Self::Liquid => "液态玻璃",
        }
    }

    /// 实现 从stored 相关的业务流程。
    ///
    /// # 参数
    /// - `value`：由调用方传入的value。
    /// # 返回
    /// 返回 `Option<Self>`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub fn from_stored(value: &str) -> Option<Self> {
        match value {
            "simple" => Some(Self::Simple),
            "frosted" | "liquid" | "clear" | "regular" => Some(Self::Frosted),
            _ => None,
        }
    }

    /// 实现 当前 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 返回
    /// 返回 `Self`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn current() -> Self {
        match APPEARANCE_MODE.load(Ordering::Relaxed) {
            0 => Self::Simple,
            2 => Self::Liquid,
            _ => Self::Frosted,
        }
    }
}

/// 实现 configureeffects 相关的业务流程。
///
/// # 参数
/// - `ctx`：由调用方传入的ctx。
/// - `glass_transparency`：由调用方传入的玻璃transparency。
/// - `card_edge_clarity`：由调用方传入的卡片edgeclarity。
/// - `pointer_brightness`：由调用方传入的指针brightness。
/// - `pointer_radius`：由调用方传入的指针radius。
/// - `appearance_mode`：由调用方传入的appearancemode。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn configure_effects(
    ctx: &egui::Context,
    glass_transparency: u16,
    card_edge_clarity: u16,
    pointer_brightness: u16,
    pointer_radius: u16,
    corner_radius: u16,
    appearance_mode: AppearanceMode,
) {
    GLASS_TRANSPARENCY.store(glass_transparency.clamp(0, 100), Ordering::Relaxed);
    CARD_EDGE_CLARITY.store(card_edge_clarity.clamp(0, 100), Ordering::Relaxed);
    POINTER_BRIGHTNESS.store(pointer_brightness.min(200), Ordering::Relaxed);
    POINTER_RADIUS.store(pointer_radius.clamp(20, 400), Ordering::Relaxed);
    CORNER_RADIUS.store(
        corner_radius.clamp(MIN_CORNER_RADIUS as u16, MAX_CORNER_RADIUS as u16) as u8,
        Ordering::Relaxed,
    );
    let previous = APPEARANCE_MODE.swap(appearance_mode as u8, Ordering::Relaxed);
    if previous != appearance_mode as u8 {
        apply_context_visuals(ctx, appearance_mode);
    }
}

/// 返回当前所有圆角表面使用的逻辑像素半径。
///
/// 此值在每帧渲染开始前由 `configure_effects` 更新；窗口外框使用同一个值，
/// 再按窗口 DPI 换算成物理像素，从而保持窗口与卡片视觉一致。
pub fn corner_radius() -> u8 {
    CORNER_RADIUS.load(Ordering::Relaxed)
}

/// 实现 玻璃tintstrength 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `f32`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn glass_tint_strength() -> f32 {
    let transparency = GLASS_TRANSPARENCY.load(Ordering::Relaxed) as f32 / 100.0;
    0.02 + (1.0 - transparency) * 0.30
}

/// 实现 玻璃alpha 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `f32`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn glass_alpha() -> f32 {
    glass_alpha_for(GLASS_TRANSPARENCY.load(Ordering::Relaxed))
}

/// 实现 卡片edgeclarity 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `f32`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn card_edge_clarity() -> f32 {
    CARD_EDGE_CLARITY.load(Ordering::Relaxed) as f32 / 100.0
}

/// 实现 玻璃alphafor 相关的业务流程。
///
/// # 参数
/// - `transparency`：由调用方传入的transparency。
/// # 返回
/// 返回 `f32`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn glass_alpha_for(transparency: u16) -> f32 {
    let transparency = transparency as f32;
    1.0 - ((transparency - 20.0) / 80.0).clamp(0.0, 1.0) * 0.45
}

/// 实现 指针brightness 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `f32`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn pointer_brightness() -> f32 {
    POINTER_BRIGHTNESS.load(Ordering::Relaxed) as f32 / 100.0
}

/// 实现 指针radius 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `f32`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn pointer_radius() -> f32 {
    POINTER_RADIUS.load(Ordering::Relaxed) as f32
}

/// 实现 appearancemode 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `AppearanceMode`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn appearance_mode() -> AppearanceMode {
    AppearanceMode::current()
}

/// 实现 background 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Color32`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn background() -> Color32 {
    Color32::from_rgb(255, 255, 255)
}

/// 实现 ink 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Color32`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn ink() -> Color32 {
    Color32::from_rgb(24, 27, 24)
}

/// 实现 inksoft 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Color32`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn ink_soft() -> Color32 {
    Color32::from_rgb(52, 57, 52)
}

/// 实现 muted 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Color32`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn muted() -> Color32 {
    Color32::from_rgb(104, 111, 104)
}

/// 实现 line 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Color32`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn line() -> Color32 {
    Color32::from_black_alpha(25)
}

/// 实现 linestrong 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `Color32`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn line_strong() -> Color32 {
    Color32::from_black_alpha(44)
}

/// 实现 应用contextvisuals 相关的业务流程。
///
/// # 参数
/// - `ctx`：由调用方传入的ctx。
/// - `_mode`：由调用方传入的mode。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn apply_context_visuals(ctx: &egui::Context, _mode: AppearanceMode) {
    let mut visuals = egui::Visuals::light();
    visuals.panel_fill = background();
    visuals.window_fill = Color32::WHITE;
    visuals.extreme_bg_color = Color32::from_rgb(238, 240, 237);
    visuals.override_text_color = Some(ink());
    visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(61, 150, 70, 74);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, line());
    ctx.set_visuals(visuals);
}

#[derive(Clone, Copy)]
/// 定义 GlassUniforms 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
struct GlassUniforms {
    tint: [f32; 3],
    accent: [f32; 3],
    time: f32,
    hovered: f32,
    pointer: [f32; 2],
    pointer_active: f32,
    blur_radius: f32,
    tint_strength: f32,
    pointer_brightness: f32,
    pointer_radius: f32,
    glass_alpha: f32,
    clear: f32,
    liquid: f32,
    top_highlight: f32,
}

/// 定义 GlassRenderer 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
struct GlassRenderer {
    program: eframe::glow::Program,
    vertex_array: eframe::glow::VertexArray,
    backdrop_texture: eframe::glow::Texture,
    texture_size: [i32; 2],
    sampler: eframe::glow::UniformLocation,
    screen_size: eframe::glow::UniformLocation,
    rect: eframe::glow::UniformLocation,
    radius: eframe::glow::UniformLocation,
    time: eframe::glow::UniformLocation,
    tint: eframe::glow::UniformLocation,
    accent: eframe::glow::UniformLocation,
    hovered: eframe::glow::UniformLocation,
    pointer: eframe::glow::UniformLocation,
    pointer_active: eframe::glow::UniformLocation,
    blur_radius: eframe::glow::UniformLocation,
    tint_strength: eframe::glow::UniformLocation,
    pointer_brightness: eframe::glow::UniformLocation,
    pointer_radius: eframe::glow::UniformLocation,
    glass_alpha: eframe::glow::UniformLocation,
    clear: eframe::glow::UniformLocation,
    liquid: eframe::glow::UniformLocation,
    top_highlight: eframe::glow::UniformLocation,
}

/// 提供 玻璃RENDERER 使用的进程级静态状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
static GLASS_RENDERER: OnceLock<Mutex<Option<GlassRenderer>>> = OnceLock::new();

/// 实现 setup玻璃 相关的业务流程。
///
/// # 参数
/// - `gl`：由调用方传入的gl。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn setup_glass(gl: Arc<eframe::glow::Context>) {
    let renderer = unsafe { create_glass_renderer(&gl) };
    let renderer = match renderer {
        Ok(renderer) => Some(renderer),
        Err(error) => {
            eprintln!("Liquid glass shader unavailable: {error}");
            None
        }
    };
    *GLASS_RENDERER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("glass renderer lock") = renderer;
}

/// 注册 WGPU 自定义窗口圆角遮罩。
pub fn setup_wgpu_window(render_state: &eframe::egui_wgpu::RenderState) {
    window_mask_wgpu::setup(render_state);
}

/// 在全部界面绘制完成后应用用户设置的窗口圆角。
pub fn paint_window_mask(painter: &Painter, screen: Rect) {
    window_mask_wgpu::paint(painter, screen, corner_radius());
}

/// 实现 玻璃着色器ready 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `bool`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn glass_shader_ready() -> bool {
    GLASS_RENDERER
        .get()
        .and_then(|renderer| renderer.lock().ok())
        .is_some_and(|renderer| renderer.is_some())
}

unsafe fn create_glass_renderer(gl: &eframe::glow::Context) -> Result<GlassRenderer, String> {
    use eframe::glow;

    unsafe {
        let vertex = compile_glass_shader(gl, glow::VERTEX_SHADER, GLASS_VERTEX_SHADER)?;
        let fragment = compile_glass_shader(gl, glow::FRAGMENT_SHADER, GLASS_FRAGMENT_SHADER)?;
        let program = gl
            .create_program()
            .map_err(|error| format!("create program: {error}"))?;
        gl.attach_shader(program, vertex);
        gl.attach_shader(program, fragment);
        gl.link_program(program);
        gl.detach_shader(program, vertex);
        gl.detach_shader(program, fragment);
        gl.delete_shader(vertex);
        gl.delete_shader(fragment);
        if !gl.get_program_link_status(program) {
            let error = gl.get_program_info_log(program);
            gl.delete_program(program);
            return Err(format!("link program: {error}"));
        }
        let vertex_array = gl
            .create_vertex_array()
            .map_err(|error| format!("create vertex array: {error}"))?;
        let backdrop_texture = gl
            .create_texture()
            .map_err(|error| format!("create backdrop texture: {error}"))?;
        gl.bind_texture(glow::TEXTURE_2D, Some(backdrop_texture));
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_S,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_T,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.bind_texture(glow::TEXTURE_2D, None);
        let uniform = |name| {
            gl.get_uniform_location(program, name)
                .ok_or_else(|| format!("missing uniform {name}"))
        };
        Ok(GlassRenderer {
            program,
            vertex_array,
            backdrop_texture,
            texture_size: [0, 0],
            sampler: uniform("u_backdrop")?,
            screen_size: uniform("u_screen")?,
            rect: uniform("u_rect")?,
            radius: uniform("u_radius")?,
            time: uniform("u_time")?,
            tint: uniform("u_tint")?,
            accent: uniform("u_accent")?,
            hovered: uniform("u_hover")?,
            pointer: uniform("u_pointer")?,
            pointer_active: uniform("u_pointer_active")?,
            blur_radius: uniform("u_blur_radius")?,
            tint_strength: uniform("u_tint_strength")?,
            pointer_brightness: uniform("u_pointer_brightness")?,
            pointer_radius: uniform("u_pointer_radius")?,
            glass_alpha: uniform("u_glass_alpha")?,
            clear: uniform("u_clear")?,
            liquid: uniform("u_liquid")?,
            top_highlight: uniform("u_top_highlight")?,
        })
    }
}

unsafe fn compile_glass_shader(
    gl: &eframe::glow::Context,
    kind: u32,
    source: &str,
) -> Result<eframe::glow::Shader, String> {
    unsafe {
        let shader = gl
            .create_shader(kind)
            .map_err(|error| format!("create shader: {error}"))?;
        gl.shader_source(shader, source);
        gl.compile_shader(shader);
        if !gl.get_shader_compile_status(shader) {
            let error = gl.get_shader_info_log(shader);
            gl.delete_shader(shader);
            return Err(format!("compile shader: {error}"));
        }
        Ok(shader)
    }
}

/// 提供 玻璃VERTEX着色器 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const GLASS_VERTEX_SHADER: &str = r#"#version 330 core
void main() {
    vec2 position = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    gl_Position = vec4(position * 2.0 - 1.0, 0.0, 1.0);
}
"#;

/// 提供 玻璃FRAGMENT着色器 使用的只读常量。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
const GLASS_FRAGMENT_SHADER: &str = r#"#version 330 core
out vec4 frag_color;

uniform sampler2D u_backdrop;
uniform vec2 u_screen;
uniform vec4 u_rect;
uniform float u_radius;
uniform float u_time;
uniform vec3 u_tint;
uniform vec3 u_accent;
uniform float u_hover;
uniform vec2 u_pointer;
uniform float u_pointer_active;
uniform float u_blur_radius;
uniform float u_tint_strength;
uniform float u_pointer_brightness;
uniform float u_pointer_radius;
uniform float u_glass_alpha;
uniform float u_clear;
uniform float u_liquid;
uniform float u_top_highlight;

float rounded_box(vec2 point, vec2 half_size, float radius) {
    vec2 edge = abs(point) - half_size + radius;
    return min(max(edge.x, edge.y), 0.0) + length(max(edge, vec2(0.0))) - radius;
}

vec3 liquid_blur(vec2 uv, float radius_px) {
    vec2 step_px = vec2(radius_px) / u_screen;
    vec3 color = texture(u_backdrop, uv).rgb * 0.28;
    color += texture(u_backdrop, uv + vec2(step_px.x, 0.0)).rgb * 0.12;
    color += texture(u_backdrop, uv - vec2(step_px.x, 0.0)).rgb * 0.12;
    color += texture(u_backdrop, uv + vec2(0.0, step_px.y)).rgb * 0.12;
    color += texture(u_backdrop, uv - vec2(0.0, step_px.y)).rgb * 0.12;
    color += texture(u_backdrop, uv + step_px).rgb * 0.06;
    color += texture(u_backdrop, uv - step_px).rgb * 0.06;
    color += texture(u_backdrop, uv + vec2(step_px.x, -step_px.y)).rgb * 0.06;
    color += texture(u_backdrop, uv + vec2(-step_px.x, step_px.y)).rgb * 0.06;
    return color;
}

vec3 gaussian_blur(vec2 uv, float radius_px) {
    vec2 step_px = vec2(radius_px) / u_screen;
    vec3 color = texture(u_backdrop, uv).rgb * 0.14;
    color += texture(u_backdrop, uv + vec2(step_px.x, 0.0)).rgb * 0.115;
    color += texture(u_backdrop, uv - vec2(step_px.x, 0.0)).rgb * 0.115;
    color += texture(u_backdrop, uv + vec2(0.0, step_px.y)).rgb * 0.115;
    color += texture(u_backdrop, uv - vec2(0.0, step_px.y)).rgb * 0.115;
    color += texture(u_backdrop, uv + step_px).rgb * 0.065;
    color += texture(u_backdrop, uv - step_px).rgb * 0.065;
    color += texture(u_backdrop, uv + vec2(step_px.x, -step_px.y)).rgb * 0.065;
    color += texture(u_backdrop, uv + vec2(-step_px.x, step_px.y)).rgb * 0.065;
    color += texture(u_backdrop, uv + vec2(step_px.x * 2.0, 0.0)).rgb * 0.035;
    color += texture(u_backdrop, uv - vec2(step_px.x * 2.0, 0.0)).rgb * 0.035;
    color += texture(u_backdrop, uv + vec2(0.0, step_px.y * 2.0)).rgb * 0.035;
    color += texture(u_backdrop, uv - vec2(0.0, step_px.y * 2.0)).rgb * 0.035;
    return color;
}

void main() {
    vec2 half_size = u_rect.zw * 0.5;
    vec2 center = u_rect.xy + half_size;
    vec2 point = gl_FragCoord.xy - center;
    float distance_to_edge = rounded_box(point, half_size, u_radius);
    float mask = 1.0 - smoothstep(-0.65, 0.65, distance_to_edge);
    if (mask <= 0.001) {
        discard;
    }

    float edge_width = clamp(u_radius * 0.95, 20.0, 52.0);
    float lens = 1.0 - smoothstep(-edge_width, -0.5, distance_to_edge);
    vec2 edge_point = abs(point) - half_size + u_radius;
    vec2 normal;
    if (edge_point.x > 0.0 && edge_point.y > 0.0) {
        normal = normalize(max(edge_point, vec2(0.0)) * sign(point));
    } else if (edge_point.x > edge_point.y) {
        normal = vec2(sign(point.x), 0.0);
    } else {
        normal = vec2(0.0, sign(point.y));
    }

    vec2 uv = gl_FragCoord.xy / u_screen;
    float blur_radius = max(2.2, u_blur_radius) + lens * lens * (1.5 + u_hover * 1.0 + u_liquid * 9.0);
    vec2 refraction = normal * lens * lens * (20.0 + u_hover * 5.0) * u_liquid / u_screen;
    vec2 sampled_uv = clamp(uv + refraction, vec2(0.0), vec2(1.0));
    vec3 backdrop = u_blur_radius > 6.0
        ? gaussian_blur(sampled_uv, blur_radius)
        : liquid_blur(sampled_uv, blur_radius);
    float luminance = dot(backdrop, vec3(0.2126, 0.7152, 0.0722));
    vec3 color = mix(vec3(luminance), backdrop, 1.22 + u_clear * 0.08);
    vec2 chroma_offset = normal * lens * 24.0 / u_screen;
    vec3 chroma = vec3(
        texture(u_backdrop, clamp(uv + chroma_offset, vec2(0.0), vec2(1.0))).r,
        texture(u_backdrop, uv).g,
        texture(u_backdrop, clamp(uv - chroma_offset, vec2(0.0), vec2(1.0))).b
    );
    color = mix(color, chroma, lens * 0.10 * u_liquid);
    color = mix(color, u_tint, u_tint_strength * (1.0 - u_clear * 0.55) + lens * 0.035);
    color = mix(color, u_accent, lens * 0.018);

    vec2 normalized = point / max(half_size, vec2(1.0));
    float top_light = smoothstep(-0.25, 0.95, normalized.y);
    float side_light = smoothstep(0.45, 1.0, abs(normalized.x));
    float inner_rim = 1.0 - smoothstep(-7.0, 0.0, distance_to_edge);
    float bevel = 1.0 - smoothstep(-edge_width * 0.48, 0.0, distance_to_edge);
    color += vec3(0.92, 1.0, 0.94) * inner_rim * (0.025 + top_light * 0.035) * u_top_highlight;
    color += vec3(0.90, 0.99, 0.92) * bevel * 0.012;
    color += u_accent * lens * 0.012 * u_liquid;
    color *= 1.0 - u_clear * 0.06;

    float caustic = sin(normalized.x * 3.1 - u_time * 0.52 + normalized.y * 1.7);
    caustic = pow(max(0.0, 1.0 - abs(caustic) * 2.5), 8.0);
    color += vec3(0.93, 1.0, 0.94) * caustic * top_light * 0.032 * u_liquid;

    vec2 pointer_delta = gl_FragCoord.xy - u_pointer;
    float pointer_sigma = max(20.0, u_pointer_radius);
    float pointer_light = exp(-dot(pointer_delta, pointer_delta) / (2.0 * pointer_sigma * pointer_sigma));
    pointer_light *= u_pointer_active * u_pointer_brightness;
    color += vec3(0.40, 0.88, 0.53) * pointer_light * (lens * 0.06 * u_liquid);
    color += vec3(1.0, 1.0, 0.96) * pointer_light * 0.015 * u_liquid;

    vec2 border_delta = gl_FragCoord.xy - u_pointer;
    float border_sigma = pointer_sigma * 0.68;
    float pointer_border = exp(-dot(border_delta, border_delta) / (2.0 * border_sigma * border_sigma));
    pointer_border *= inner_rim * u_pointer_active * u_pointer_brightness;
    color += vec3(1.0, 1.0, 0.98) * pointer_border * (u_liquid * 0.30);
    color += u_accent * pointer_border * 0.16 * u_liquid;

    float bottom_edge = smoothstep(0.15, 1.0, -normalized.y) * lens;
    color *= 1.0 - bottom_edge * 0.035;
    frag_color = vec4(color, mask * u_glass_alpha * mix(1.0, 0.84, u_liquid));
}
"#;

/// 实现 绘制玻璃backdrop 相关的业务流程。
///
/// # 参数
/// - `painter`：由调用方传入的painter。
/// - `rect`：由调用方传入的rect。
/// - `radius`：由调用方传入的radius。
/// - `uniforms`：由调用方传入的uniforms。
/// # 返回
/// 返回 `bool`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn paint_glass_backdrop(
    painter: &Painter,
    rect: Rect,
    radius: u8,
    uniforms: GlassUniforms,
) -> bool {
    if !glass_shader_ready() {
        return false;
    }
    let callback = eframe::egui_glow::CallbackFn::new(move |info, gl_painter| {
        let gl = gl_painter.gl();
        let Ok(mut renderer_guard) = GLASS_RENDERER.get_or_init(|| Mutex::new(None)).lock() else {
            return;
        };
        let Some(renderer) = renderer_guard.as_mut() else {
            return;
        };
        let [screen_width, screen_height] = info.screen_size_px;
        if screen_width == 0 || screen_height == 0 {
            return;
        }
        let viewport = info.viewport_in_pixels();
        if viewport.width_px <= 0 || viewport.height_px <= 0 {
            return;
        }
        unsafe {
            use eframe::glow;

            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(renderer.backdrop_texture));
            let size = [screen_width as i32, screen_height as i32];
            if renderer.texture_size != size {
                gl.copy_tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8, 0, 0, size[0], size[1], 0);
                renderer.texture_size = size;
            } else {
                let margin = (44.0 * info.pixels_per_point).ceil() as i32;
                let left = (viewport.left_px - margin).clamp(0, size[0]);
                let bottom = (viewport.from_bottom_px - margin).clamp(0, size[1]);
                let right = (viewport.left_px + viewport.width_px + margin).clamp(0, size[0]);
                let top = (viewport.from_bottom_px + viewport.height_px + margin).clamp(0, size[1]);
                if right > left && top > bottom {
                    gl.copy_tex_sub_image_2d(
                        glow::TEXTURE_2D,
                        0,
                        left,
                        bottom,
                        left,
                        bottom,
                        right - left,
                        top - bottom,
                    );
                }
            }
            gl.use_program(Some(renderer.program));
            gl.bind_vertex_array(Some(renderer.vertex_array));
            gl.uniform_1_i32(Some(&renderer.sampler), 0);
            gl.uniform_2_f32(
                Some(&renderer.screen_size),
                screen_width as f32,
                screen_height as f32,
            );
            gl.uniform_4_f32(
                Some(&renderer.rect),
                viewport.left_px as f32,
                viewport.from_bottom_px as f32,
                viewport.width_px as f32,
                viewport.height_px as f32,
            );
            gl.uniform_1_f32(
                Some(&renderer.radius),
                radius as f32 * info.pixels_per_point,
            );
            gl.uniform_1_f32(Some(&renderer.time), uniforms.time);
            gl.uniform_3_f32(
                Some(&renderer.tint),
                uniforms.tint[0],
                uniforms.tint[1],
                uniforms.tint[2],
            );
            gl.uniform_3_f32(
                Some(&renderer.accent),
                uniforms.accent[0],
                uniforms.accent[1],
                uniforms.accent[2],
            );
            gl.uniform_1_f32(Some(&renderer.hovered), uniforms.hovered);
            gl.uniform_2_f32(
                Some(&renderer.pointer),
                uniforms.pointer[0] * info.pixels_per_point,
                screen_height as f32 - uniforms.pointer[1] * info.pixels_per_point,
            );
            gl.uniform_1_f32(Some(&renderer.pointer_active), uniforms.pointer_active);
            gl.uniform_1_f32(
                Some(&renderer.blur_radius),
                uniforms.blur_radius * info.pixels_per_point,
            );
            gl.uniform_1_f32(Some(&renderer.tint_strength), uniforms.tint_strength);
            gl.uniform_1_f32(
                Some(&renderer.pointer_brightness),
                uniforms.pointer_brightness,
            );
            gl.uniform_1_f32(
                Some(&renderer.pointer_radius),
                uniforms.pointer_radius * info.pixels_per_point,
            );
            gl.uniform_1_f32(Some(&renderer.glass_alpha), uniforms.glass_alpha);
            gl.uniform_1_f32(Some(&renderer.clear), uniforms.clear);
            gl.uniform_1_f32(Some(&renderer.liquid), uniforms.liquid);
            gl.uniform_1_f32(Some(&renderer.top_highlight), uniforms.top_highlight);
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl.disable(glow::DEPTH_TEST);
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            gl.bind_vertex_array(None);
            gl.use_program(None);
            gl.bind_texture(glow::TEXTURE_2D, None);
        }
    });
    painter.add(egui::PaintCallback {
        rect,
        callback: Arc::new(callback),
    });
    true
}

/// 实现 colorrgb 相关的业务流程。
///
/// # 参数
/// - `color`：由调用方传入的color。
/// # 返回
/// 返回 `[f32`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn color_rgb(color: Color32) -> [f32; 3] {
    let [r, g, b, _] = color.to_array();
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
}

#[derive(Clone, Copy)]
/// 定义 GlassSpec 数据结构及其跨模块传递的状态。
///
/// 字段、变体或方法的具体含义由名称及所在模块契约约束；修改时应同步检查所有构造点和调用点。
pub struct GlassSpec {
    pub tint: Color32,
    pub accent: Color32,
    pub time: f64,
    pub hovered: bool,
    pub blur_radius: f32,
    pub top_highlight: f32,
    pub card_edge: bool,
}

impl GlassSpec {
    /// 实现 new 相关的业务流程。
    ///
    /// # 参数
    /// - `tint`：由调用方传入的tint。
    /// - `accent`：由调用方传入的accent。
    /// - `time`：由调用方传入的time。
    /// - `_seed`：由调用方传入的seed。
    /// # 返回
    /// 返回 `Self`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub fn new(tint: Color32, accent: Color32, time: f64, _seed: f32) -> Self {
        Self {
            tint,
            accent,
            time,
            hovered: false,
            blur_radius: 2.2,
            top_highlight: 1.0,
            card_edge: true,
        }
    }

    /// 实现 hovered 相关的业务流程。
    ///
    /// # 参数
    /// - `hovered`：由调用方传入的hovered。
    /// # 返回
    /// 返回 `Self`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub fn hovered(mut self, hovered: bool) -> Self {
        self.hovered = hovered;
        self
    }

    /// 实现 backdropblur 相关的业务流程。
    ///
    /// # 参数
    /// - `radius`：由调用方传入的radius。
    /// # 返回
    /// 返回 `Self`，供上层继续渲染、判断或持久化。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    pub fn backdrop_blur(mut self, radius: f32) -> Self {
        self.blur_radius = radius.clamp(0.0, 24.0);
        self
    }

    /// 控制玻璃内缘顶部高光强度。`1.0` 为默认高光，`0.0` 用于窗口最顶部
    /// 等需要避免出现白色亮线的结构区域。
    pub fn top_highlight(mut self, strength: f32) -> Self {
        self.top_highlight = strength.clamp(0.0, 1.0);
        self
    }

    /// 控制是否绘制独立的卡片描边。窗口边缘、标题栏等由外层遮罩统一定义
    /// 轮廓的表面应关闭此项，避免两个圆角轮廓相交。
    pub fn card_edge(mut self, visible: bool) -> Self {
        self.card_edge = visible;
        self
    }
}

/// 实现 玻璃panel 相关的业务流程。
///
/// # 参数
/// - `painter`：由调用方传入的painter。
/// - `rect`：由调用方传入的rect。
/// - `radius`：由调用方传入的radius。
/// - `spec`：由调用方传入的spec。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn glass_panel(painter: &Painter, rect: Rect, radius: u8, spec: GlassSpec) {
    glass_panel_impl(painter, rect, radius, spec, false);
}

/// 强制绘制磨砂玻璃，不受全局“简洁”外观模式影响。
///
/// 用于需要始终保留背景模糊的结构性区域，例如左侧导航栏。
pub fn frosted_glass_panel(painter: &Painter, rect: Rect, radius: u8, spec: GlassSpec) {
    glass_panel_impl(painter, rect, radius, spec, true);
}

fn glass_panel_impl(
    painter: &Painter,
    rect: Rect,
    radius: u8,
    spec: GlassSpec,
    force_frosted: bool,
) {
    let GlassSpec {
        tint: requested_tint,
        accent,
        time,
        hovered,
        blur_radius,
        top_highlight,
        card_edge,
    } = spec;
    let mode = if force_frosted {
        AppearanceMode::Frosted
    } else {
        appearance_mode()
    };
    if mode == AppearanceMode::Simple {
        fill_round(
            painter,
            rect,
            radius,
            if hovered {
                Color32::from_rgb(255, 255, 255)
            } else {
                Color32::from_rgb(252, 253, 251)
            },
        );
        if card_edge {
            paint_card_edge(painter, rect, radius);
        }
        return;
    }
    let tint = if matches!(mode, AppearanceMode::Frosted | AppearanceMode::Liquid) {
        Color32::from_rgb(244, 247, 243)
    } else {
        requested_tint
    };
    let pointer = painter.ctx().input(|input| input.pointer.hover_pos());
    let frosted = mode == AppearanceMode::Frosted;
    let enhanced_liquid = mode == AppearanceMode::Liquid;
    let uniforms = GlassUniforms {
        tint: color_rgb(tint),
        accent: color_rgb(accent),
        time: time as f32,
        hovered: if hovered { 1.0 } else { 0.0 },
        pointer: pointer
            .map(|position| [position.x, position.y])
            .unwrap_or([-10_000.0, -10_000.0]),
        pointer_active: if pointer.is_some() { 1.0 } else { 0.0 },
        blur_radius: if frosted {
            blur_radius.max(12.0)
        } else if enhanced_liquid {
            blur_radius.max(3.0)
        } else {
            blur_radius.max(8.0)
        },
        tint_strength: glass_tint_strength() * if enhanced_liquid { 0.46 } else { 0.78 },
        pointer_brightness: pointer_brightness(),
        pointer_radius: pointer_radius(),
        glass_alpha: if enhanced_liquid {
            (glass_alpha() - 0.10).max(0.40)
        } else {
            glass_alpha().max(0.68)
        },
        clear: 0.0,
        liquid: if enhanced_liquid { 1.0 } else { 0.0 },
        top_highlight,
    };
    // 着色器回调在 egui 形状提交后执行，文字绘制时无法从帧缓冲读取玻璃结果。
    // 这里登记 tint 与玻璃 alpha 的解析近似值，让玻璃上的文字按实际透出的底色
    // 自动调整对比度；着色器仍负责最终模糊、折射和环境高光。
    let [tint_r, tint_g, tint_b, _] = tint.to_array();
    contrast::register_surface(
        painter,
        rect,
        Color32::from_rgba_unmultiplied(
            tint_r,
            tint_g,
            tint_b,
            (uniforms.glass_alpha * 255.0).round().clamp(0.0, 255.0) as u8,
        ),
    );
    let shader_drawn = rect.width() >= 120.0
        && rect.height() >= 34.0
        && paint_glass_backdrop(painter, rect, radius, uniforms);

    if !shader_drawn {
        let [r, g, b, _] = tint.to_array();
        let compact = rect.width() < 120.0 || rect.height() < 34.0;
        let transparency = GLASS_TRANSPARENCY.load(Ordering::Relaxed) as f32;
        let mut alpha = (120.0 - transparency * 0.55 + if compact { 25.0 } else { 0.0 })
            .clamp(52.0, 128.0) as u8;
        if enhanced_liquid {
            alpha = ((alpha as f32) * 0.70).max(34.0) as u8;
        }
        fill_round(
            painter,
            rect,
            radius,
            Color32::from_rgba_unmultiplied(r, g, b, alpha),
        );
        if enhanced_liquid {
            let top_height = (rect.height() * 0.50).min(96.0);
            painter.rect_filled(
                Rect::from_min_size(rect.left_top(), Vec2::new(rect.width(), top_height)),
                CornerRadius {
                    nw: radius,
                    ne: radius,
                    sw: 0,
                    se: 0,
                },
                Color32::from_white_alpha(if hovered { 36 } else { 24 }),
            );
        }
    }

    if enhanced_liquid {
        fill_round(painter, rect, radius, black_alpha(14));
    }
    if card_edge {
        paint_card_edge(painter, rect, radius);
    }
}

/// 实现 绘制卡片edge 相关的业务流程。
///
/// # 参数
/// - `painter`：由调用方传入的painter。
/// - `rect`：由调用方传入的rect。
/// - `radius`：由调用方传入的radius。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
fn paint_card_edge(painter: &Painter, rect: Rect, radius: u8) {
    let clarity = card_edge_clarity();
    if clarity <= 0.001 {
        return;
    }
    let alpha = (clarity * 72.0).round().clamp(1.0, 72.0) as u8;
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(1.0, black_alpha(alpha)),
        StrokeKind::Inside,
    );
}

/// 实现 liquidbutton 相关的业务流程。
///
/// # 参数
/// - `painter`：由调用方传入的painter。
/// - `rect`：由调用方传入的rect。
/// - `radius`：由调用方传入的radius。
/// - `time`：由调用方传入的time。
/// - `hovered`：由调用方传入的hovered。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub fn liquid_button(painter: &Painter, rect: Rect, radius: u8, time: f64, hovered: bool) {
    if appearance_mode() == AppearanceMode::Simple {
        fill_round(
            painter,
            rect,
            radius,
            if hovered {
                Color32::from_rgb(85, 194, 76)
            } else {
                Color32::from_rgb(65, 166, 61)
            },
        );
        stroke_round(
            painter,
            rect,
            radius,
            1.0,
            Color32::from_rgba_unmultiplied(18, 91, 30, 100),
        );
        return;
    }
    glass_panel(
        painter,
        rect,
        radius,
        GlassSpec::new(
            if hovered {
                Color32::from_rgb(74, 174, 67)
            } else {
                Color32::from_rgb(60, 153, 58)
            },
            GREEN_BRIGHT,
            time,
            4.8,
        )
        .hovered(hovered),
    );
    fill_round(
        painter,
        rect,
        radius,
        if hovered {
            alpha(Color32::from_rgb(90, 210, 78), 58)
        } else {
            alpha(Color32::from_rgb(70, 190, 67), 42)
        },
    );
    let top = Rect::from_min_size(
        rect.left_top(),
        Vec2::new(rect.width(), rect.height() * 0.56),
    );
    painter.rect_filled(
        top,
        CornerRadius {
            nw: radius,
            ne: radius,
            sw: 0,
            se: 0,
        },
        white_alpha(if hovered { 22 } else { 15 }),
    );
    stroke_round(
        painter,
        rect,
        radius,
        1.0,
        if hovered {
            white_alpha(118)
        } else {
            white_alpha(78)
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// 实现 玻璃transparencychangesoutputalpha 相关的业务流程。
    ///
    /// 该函数不接收显式参数，直接使用所属对象或模块状态。
    /// # 约束
    /// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
    fn glass_transparency_changes_output_alpha() {
        assert_eq!(glass_alpha_for(20), 1.0);
        assert!((glass_alpha_for(100) - 0.55).abs() < 0.001);
        assert!(glass_alpha_for(40) > glass_alpha_for(90));
    }
}
