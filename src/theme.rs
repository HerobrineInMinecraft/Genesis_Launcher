use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Painter,
    Pos2, Rect, Shape, Stroke, StrokeKind, Vec2,
};
use eframe::glow::HasContext;

pub const BG: Color32 = Color32::from_rgb(10, 15, 12);
pub const SURFACE: Color32 = Color32::from_rgb(17, 24, 19);
pub const INK: Color32 = Color32::from_rgb(241, 247, 235);
pub const INK_SOFT: Color32 = Color32::from_rgb(208, 218, 206);
pub const MUTED: Color32 = Color32::from_rgb(143, 158, 146);
pub const GREEN: Color32 = Color32::from_rgb(100, 216, 92);
pub const GREEN_BRIGHT: Color32 = Color32::from_rgb(155, 242, 130);
pub const LINE: Color32 = Color32::from_rgba_premultiplied(23, 24, 22, 26);
pub const LINE_STRONG: Color32 = Color32::from_rgba_premultiplied(37, 40, 36, 43);
pub const RADIUS: u8 = 16;

const BOLD_FAMILY: &str = "launcher-bold";

static GLASS_TRANSPARENCY: AtomicU16 = AtomicU16::new(82);
static POINTER_BRIGHTNESS: AtomicU16 = AtomicU16::new(100);
static POINTER_RADIUS: AtomicU16 = AtomicU16::new(155);
static LIQUID_GLASS_ENABLED: AtomicBool = AtomicBool::new(true);
static LIQUID_GLASS_CLEAR: AtomicBool = AtomicBool::new(false);

pub fn configure_effects(
    glass_transparency: u16,
    pointer_brightness: u16,
    pointer_radius: u16,
    liquid_glass_enabled: bool,
    liquid_glass_clear: bool,
) {
    GLASS_TRANSPARENCY.store(glass_transparency.clamp(0, 100), Ordering::Relaxed);
    POINTER_BRIGHTNESS.store(pointer_brightness.min(200), Ordering::Relaxed);
    POINTER_RADIUS.store(pointer_radius.clamp(20, 400), Ordering::Relaxed);
    LIQUID_GLASS_ENABLED.store(liquid_glass_enabled, Ordering::Relaxed);
    LIQUID_GLASS_CLEAR.store(liquid_glass_clear, Ordering::Relaxed);
}

fn glass_tint_strength() -> f32 {
    let transparency = GLASS_TRANSPARENCY.load(Ordering::Relaxed) as f32 / 100.0;
    0.02 + (1.0 - transparency) * 0.30
}

fn glass_alpha() -> f32 {
    glass_alpha_for(GLASS_TRANSPARENCY.load(Ordering::Relaxed))
}

fn glass_alpha_for(transparency: u16) -> f32 {
    let transparency = transparency as f32;
    1.0 - ((transparency - 20.0) / 80.0).clamp(0.0, 1.0) * 0.45
}

fn pointer_brightness() -> f32 {
    POINTER_BRIGHTNESS.load(Ordering::Relaxed) as f32 / 100.0
}

fn pointer_radius() -> f32 {
    POINTER_RADIUS.load(Ordering::Relaxed) as f32
}

fn liquid_glass_enabled() -> bool {
    LIQUID_GLASS_ENABLED.load(Ordering::Relaxed)
}

fn liquid_glass_clear() -> bool {
    LIQUID_GLASS_CLEAR.load(Ordering::Relaxed)
}

pub fn setup(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    load_font(
        &mut fonts,
        "cjk-regular",
        r"C:\Windows\Fonts\msyh.ttc",
        0,
        FontFamily::Proportional,
    );
    load_font(
        &mut fonts,
        "cjk-monospace",
        r"C:\Windows\Fonts\msyh.ttc",
        0,
        FontFamily::Monospace,
    );

    let family = FontFamily::Name(Arc::from(BOLD_FAMILY));
    if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\msyhbd.ttc") {
        fonts
            .font_data
            .insert("cjk-bold".to_owned(), Arc::new(FontData::from_owned(bytes)));
        fonts.families.insert(
            family,
            vec![
                "cjk-bold".to_owned(),
                "Ubuntu-Light".to_owned(),
                "Hack".to_owned(),
            ],
        );
    }

    ctx.set_fonts(fonts);

    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BG;
    visuals.window_fill = SURFACE;
    visuals.extreme_bg_color = Color32::from_rgb(8, 12, 9);
    visuals.override_text_color = Some(INK);
    visuals.selection.bg_fill = Color32::from_rgba_premultiplied(92, 92, 80, 115);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    ctx.set_visuals(visuals);

    ctx.style_mut_of(egui::Theme::Dark, |style| {
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.animation_time = 0.13;
        style.visuals.text_options.font_hinting = true;
        style.visuals.text_options.subpixel_binning = false;
    });
}

fn load_font(fonts: &mut FontDefinitions, name: &str, path: &str, index: u32, family: FontFamily) {
    let Ok(bytes) = std::fs::read(path) else {
        return;
    };
    let data = Arc::new(FontData::from_blob(Arc::new(bytes), index));
    fonts.font_data.insert(name.to_owned(), data);
    let priority = fonts.families.entry(family).or_default();
    priority.insert(0, name.to_owned());
}

pub fn font(size: f32) -> FontId {
    let size = tuned_font_size(size);
    let family = if size <= 11.0 {
        FontFamily::Name(Arc::from(BOLD_FAMILY))
    } else {
        FontFamily::Proportional
    };
    FontId::new(size, family)
}

pub fn bold_font(size: f32) -> FontId {
    FontId::new(
        tuned_font_size(size),
        FontFamily::Name(Arc::from(BOLD_FAMILY)),
    )
}

pub fn mono_font(size: f32) -> FontId {
    FontId::new(tuned_font_size(size), FontFamily::Monospace)
}

fn tuned_font_size(size: f32) -> f32 {
    (size.max(9.2) * 1.04 * 2.0).round() / 2.0
}

fn optical_text_offset(size: f32) -> f32 {
    -(tuned_font_size(size) * 0.055).clamp(0.55, 1.35)
}

fn snap_text_position(painter: &Painter, position: Pos2) -> Pos2 {
    let pixels_per_point = painter.ctx().pixels_per_point();
    Pos2::new(
        (position.x * pixels_per_point).round() / pixels_per_point,
        (position.y * pixels_per_point).round() / pixels_per_point,
    )
}

pub fn alpha(color: Color32, alpha: u8) -> Color32 {
    let [r, g, b, _] = color.to_array();
    Color32::from_rgba_unmultiplied(r, g, b, alpha)
}

pub fn black_alpha(alpha: u8) -> Color32 {
    Color32::from_black_alpha(alpha)
}

pub fn white_alpha(alpha: u8) -> Color32 {
    Color32::from_white_alpha(alpha)
}

pub fn fill_round(painter: &Painter, rect: Rect, radius: u8, color: Color32) {
    painter.rect_filled(rect, radius, color);
}

pub fn stroke_round(painter: &Painter, rect: Rect, radius: u8, width: f32, color: Color32) {
    painter.rect_stroke(rect, radius, Stroke::new(width, color), StrokeKind::Inside);
}

pub fn text(
    painter: &Painter,
    position: Pos2,
    value: impl ToString,
    size: f32,
    color: Color32,
) -> Rect {
    painter.text(
        snap_text_position(
            painter,
            position + Vec2::new(0.0, optical_text_offset(size)),
        ),
        Align2::LEFT_TOP,
        value,
        font(size),
        color,
    )
}

pub fn bold_text(
    painter: &Painter,
    position: Pos2,
    value: impl ToString,
    size: f32,
    color: Color32,
) -> Rect {
    painter.text(
        snap_text_position(
            painter,
            position + Vec2::new(0.0, optical_text_offset(size)),
        ),
        Align2::LEFT_TOP,
        value,
        bold_font(size),
        color,
    )
}

pub fn center_text(
    painter: &Painter,
    center: Pos2,
    value: impl ToString,
    size: f32,
    color: Color32,
    bold: bool,
) -> Rect {
    painter.text(
        snap_text_position(
            painter,
            center + Vec2::new(0.0, optical_text_offset(size) * 0.45),
        ),
        Align2::CENTER_CENTER,
        value,
        if bold { bold_font(size) } else { font(size) },
        color,
    )
}

pub fn mono_text(
    painter: &Painter,
    position: Pos2,
    value: impl ToString,
    size: f32,
    color: Color32,
) -> Rect {
    painter.text(
        snap_text_position(
            painter,
            position + Vec2::new(0.0, optical_text_offset(size)),
        ),
        Align2::LEFT_TOP,
        value,
        mono_font(size),
        color,
    )
}

pub fn wrapped_text(
    painter: &Painter,
    position: Pos2,
    value: impl Into<String>,
    size: f32,
    color: Color32,
    wrap_width: f32,
) {
    let galley = painter.layout(value.into(), font(size), color, wrap_width);
    painter.galley(
        snap_text_position(
            painter,
            position + Vec2::new(0.0, optical_text_offset(size)),
        ),
        galley,
        color,
    );
}

pub fn vertical_gradient(painter: &Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}

pub fn horizontal_fade(
    painter: &Painter,
    rect: Rect,
    left: Color32,
    middle: Color32,
    right: Color32,
) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), left);
    mesh.colored_vertex(rect.center_top(), middle);
    mesh.colored_vertex(rect.right_top(), right);
    mesh.colored_vertex(rect.left_bottom(), left);
    mesh.colored_vertex(rect.center_bottom(), middle);
    mesh.colored_vertex(rect.right_bottom(), right);
    mesh.add_triangle(0, 1, 4);
    mesh.add_triangle(0, 4, 3);
    mesh.add_triangle(1, 2, 5);
    mesh.add_triangle(1, 5, 4);
    painter.add(egui::Shape::mesh(mesh));
}

pub fn soft_orb(painter: &Painter, center: Pos2, radius: f32, color: Color32, opacity: u8) {
    const SEGMENTS: usize = 32;
    const RINGS: [f32; 5] = [0.0, 0.32, 0.62, 0.85, 1.0];
    const ALPHAS: [f32; 5] = [1.0, 0.68, 0.34, 0.10, 0.0];
    let [r, g, b, _] = color.to_array();
    let mut mesh = egui::Mesh::default();
    for (ring, (radius_scale, alpha_scale)) in RINGS.iter().zip(ALPHAS).enumerate() {
        let ring_radius = radius * radius_scale;
        let alpha = (opacity as f32 * alpha_scale).round() as u8;
        for segment in 0..SEGMENTS {
            let angle = std::f32::consts::TAU * segment as f32 / SEGMENTS as f32;
            mesh.colored_vertex(
                center + Vec2::angled(angle) * ring_radius,
                Color32::from_rgba_unmultiplied(r, g, b, alpha),
            );
            if ring > 0 {
                let previous = (ring - 1) * SEGMENTS + segment;
                let previous_next = (ring - 1) * SEGMENTS + (segment + 1) % SEGMENTS;
                let current = ring * SEGMENTS + segment;
                let current_next = ring * SEGMENTS + (segment + 1) % SEGMENTS;
                mesh.add_triangle(previous as u32, current as u32, previous_next as u32);
                mesh.add_triangle(previous_next as u32, current as u32, current_next as u32);
            }
        }
    }
    painter.add(Shape::mesh(mesh));
}

pub fn pointer_glow(painter: &Painter, position: Pos2, strength: f32, _screen: Rect) {
    let clip = painter.clone();
    let brightness = pointer_brightness();
    let strength = strength.clamp(0.0, 1.0) * brightness;
    if strength <= 0.001 {
        return;
    }
    let radius_scale = pointer_radius() / 155.0;
    soft_orb(
        &clip,
        position + Vec2::new(-14.0, -10.0),
        178.0 * radius_scale,
        Color32::from_rgb(119, 230, 155),
        (24.0 * strength.min(1.5)).round().min(255.0) as u8,
    );
    soft_orb(
        &clip,
        position + Vec2::new(12.0, 8.0),
        78.0 * radius_scale,
        Color32::from_rgb(188, 255, 207),
        (15.0 * strength.min(1.5)).round().min(255.0) as u8,
    );
}

pub fn pointer_glow_overlay(painter: &Painter, position: Pos2, strength: f32, _screen: Rect) {
    let clip = painter.clone();
    let brightness = pointer_brightness();
    let strength = strength.clamp(0.0, 1.0) * brightness;
    if strength <= 0.001 {
        return;
    }
    let radius_scale = pointer_radius() / 155.0;
    soft_orb(
        &clip,
        position,
        118.0 * radius_scale,
        Color32::from_rgb(145, 246, 170),
        (20.0 * strength.min(1.5)).round().min(255.0) as u8,
    );
    soft_orb(
        &clip,
        position,
        48.0 * radius_scale,
        Color32::from_rgb(220, 255, 225),
        (15.0 * strength.min(1.5)).round().min(255.0) as u8,
    );
}

#[derive(Clone, Copy)]
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
}

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
}

static GLASS_RENDERER: OnceLock<Mutex<Option<GlassRenderer>>> = OnceLock::new();

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

const GLASS_VERTEX_SHADER: &str = r#"#version 330 core
void main() {
    vec2 position = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    gl_Position = vec4(position * 2.0 - 1.0, 0.0, 1.0);
}
"#;

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
    float blur_radius = max(2.2, u_blur_radius) + lens * lens * (7.0 + u_hover * 2.0 + u_clear * 2.0);
    vec2 refraction = normal * lens * lens * (18.0 + u_hover * 5.0 + u_clear * 5.5) / u_screen;
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
    color = mix(color, chroma, lens * 0.42);
    color = mix(color, u_tint, u_tint_strength * (1.0 - u_clear * 0.55) + lens * 0.035);
    color = mix(color, u_accent, lens * 0.018);

    vec2 normalized = point / max(half_size, vec2(1.0));
    float top_light = smoothstep(-0.25, 0.95, normalized.y);
    float side_light = smoothstep(0.45, 1.0, abs(normalized.x));
    float inner_rim = 1.0 - smoothstep(-7.0, 0.0, distance_to_edge);
    float bevel = 1.0 - smoothstep(-edge_width * 0.48, 0.0, distance_to_edge);
    color += vec3(0.92, 1.0, 0.94) * inner_rim * (0.025 + top_light * 0.035);
    color += vec3(0.90, 0.99, 0.92) * bevel * 0.012;
    color += u_accent * lens * 0.038;
    color *= 1.0 - u_clear * 0.06;

    float caustic = sin(normalized.x * 3.1 - u_time * 0.52 + normalized.y * 1.7);
    caustic = pow(max(0.0, 1.0 - abs(caustic) * 2.5), 8.0);
    color += vec3(0.93, 1.0, 0.94) * caustic * top_light * 0.032;

    vec2 pointer_delta = gl_FragCoord.xy - u_pointer;
    float pointer_sigma = max(20.0, u_pointer_radius);
    float pointer_light = exp(-dot(pointer_delta, pointer_delta) / (2.0 * pointer_sigma * pointer_sigma));
    pointer_light *= u_pointer_active * u_pointer_brightness;
    color += vec3(0.40, 0.88, 0.53) * pointer_light * (0.12 + lens * 0.07);
    color += vec3(1.0, 1.0, 0.96) * pointer_light * 0.035;

    vec2 border_delta = gl_FragCoord.xy - u_pointer;
    float border_sigma = pointer_sigma * 0.68;
    float pointer_border = exp(-dot(border_delta, border_delta) / (2.0 * border_sigma * border_sigma));
    pointer_border *= inner_rim * u_pointer_active * u_pointer_brightness;
    color += vec3(1.0, 1.0, 0.98) * pointer_border * (0.32 + u_hover * 0.08);
    color += u_accent * pointer_border * 0.16;

    float bottom_edge = smoothstep(0.15, 1.0, -normalized.y) * lens;
    color *= 1.0 - bottom_edge * 0.035;
    float outer_line = 1.0 - smoothstep(-1.8, 0.0, distance_to_edge);
    color += vec3(1.0) * outer_line * (0.12 + u_hover * 0.045);

    frag_color = vec4(color, mask * u_glass_alpha * mix(1.0, 0.84, u_clear));
}
"#;

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

fn color_rgb(color: Color32) -> [f32; 3] {
    let [r, g, b, _] = color.to_array();
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
}

#[derive(Clone, Copy)]
pub struct GlassSpec {
    pub tint: Color32,
    pub accent: Color32,
    pub time: f64,
    pub hovered: bool,
    pub blur_radius: f32,
}

impl GlassSpec {
    pub fn new(tint: Color32, accent: Color32, time: f64, _seed: f32) -> Self {
        Self {
            tint,
            accent,
            time,
            hovered: false,
            blur_radius: 2.2,
        }
    }

    pub fn hovered(mut self, hovered: bool) -> Self {
        self.hovered = hovered;
        self
    }

    pub fn backdrop_blur(mut self, radius: f32) -> Self {
        self.blur_radius = radius.clamp(0.0, 24.0);
        self
    }
}

pub fn glass_panel(painter: &Painter, rect: Rect, radius: u8, spec: GlassSpec) {
    let GlassSpec {
        tint,
        accent,
        time,
        hovered,
        blur_radius,
    } = spec;
    let lift = if hovered { 1.4 } else { 0.0 };
    let shadow_offset = egui::vec2(0.0, 2.0 + lift);
    for (spread, width, opacity) in [
        (2.0, 5.0, 6_u8),
        (6.0, 9.0, 5),
        (13.0, 12.0, 4),
        (23.0, 14.0, 3),
    ] {
        painter.rect_stroke(
            rect.expand(spread).translate(shadow_offset),
            radius.saturating_add(spread as u8),
            Stroke::new(width, black_alpha(opacity)),
            StrokeKind::Outside,
        );
    }

    let pointer = painter.ctx().input(|input| input.pointer.hover_pos());
    let glass_enabled = liquid_glass_enabled();
    let clear = glass_enabled && liquid_glass_clear();
    let uniforms = GlassUniforms {
        tint: color_rgb(tint),
        accent: color_rgb(accent),
        time: time as f32,
        hovered: if hovered { 1.0 } else { 0.0 },
        pointer: pointer
            .map(|position| [position.x, position.y])
            .unwrap_or([-10_000.0, -10_000.0]),
        pointer_active: if pointer.is_some() { 1.0 } else { 0.0 },
        blur_radius,
        tint_strength: glass_tint_strength() * if clear { 0.46 } else { 1.0 },
        pointer_brightness: pointer_brightness(),
        pointer_radius: pointer_radius(),
        glass_alpha: if clear {
            (glass_alpha() - 0.10).max(0.40)
        } else {
            glass_alpha()
        },
        clear: if clear { 1.0 } else { 0.0 },
    };
    let shader_drawn = glass_enabled
        && rect.width() >= 120.0
        && rect.height() >= 34.0
        && paint_glass_backdrop(painter, rect, radius, uniforms);

    if !shader_drawn {
        let [r, g, b, _] = tint.to_array();
        let compact = rect.width() < 120.0 || rect.height() < 34.0;
        let transparency = GLASS_TRANSPARENCY.load(Ordering::Relaxed) as f32;
        let mut alpha = (120.0 - transparency * 0.55 + if compact { 25.0 } else { 0.0 })
            .clamp(52.0, 128.0) as u8;
        if clear {
            alpha = ((alpha as f32) * 0.70).max(34.0) as u8;
        }
        fill_round(
            painter,
            rect,
            radius,
            Color32::from_rgba_unmultiplied(r, g, b, alpha),
        );
        if glass_enabled {
            let top_height = (rect.height() * 0.50).min(96.0);
            painter.rect_filled(
                Rect::from_min_size(rect.left_top(), Vec2::new(rect.width(), top_height)),
                CornerRadius {
                    nw: radius,
                    ne: radius,
                    sw: 0,
                    se: 0,
                },
                white_alpha(if hovered { 12 } else { 8 }),
            );
        }
    }

    if clear {
        fill_round(painter, rect, radius, black_alpha(14));
    }

    stroke_round(
        painter,
        rect,
        radius,
        0.9,
        white_alpha(if hovered { 46 } else { 32 }),
    );
    stroke_round(
        painter,
        rect.shrink(1.8),
        radius.saturating_sub(2),
        1.0,
        black_alpha(20),
    );
}

pub fn liquid_button(painter: &Painter, rect: Rect, radius: u8, time: f64, hovered: bool) {
    if !liquid_glass_enabled() {
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
        stroke_round(painter, rect, radius, 1.0, white_alpha(72));
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
    fn glass_transparency_changes_output_alpha() {
        assert_eq!(glass_alpha_for(20), 1.0);
        assert!((glass_alpha_for(100) - 0.55).abs() < 0.001);
        assert!(glass_alpha_for(40) > glass_alpha_for(90));
    }
}
