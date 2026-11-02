//! # `src/theme/window_mask_wgpu.rs`
//!
//! **功能域：** WGPU 自定义窗口外缘。
//!
//! 本模块在 egui 主渲染通道末尾绘制全屏圆角遮罩，把窗口外侧像素的 Alpha
//! 与颜色乘以覆盖率。窗口半径完全来自启动器设置，不依赖 Windows 原生圆角。
//!
//! ## 模块职责
//! - 创建并缓存 WGPU 窗口遮罩管线。
//! - 根据物理像素尺寸与 DPI 更新圆角参数。
//! - 在所有 egui 形状之后应用平滑 Alpha 遮罩。
//!
//! ## 错误与并发
//! - 资源存入 egui-wgpu 的 callback resources，仅 GPU 绘制阶段访问。
//! - 回调对象只保存纯数据，满足 `Send + Sync` 要求。

use std::num::NonZeroU64;

use eframe::egui::{self, Painter, Rect};
use eframe::egui_wgpu::{self, CallbackResources, CallbackTrait, RenderState, ScreenDescriptor};
use eframe::wgpu;

struct WindowMaskResources {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
}

struct WindowMaskCallback {
    corner_radius: f32,
}

impl CallbackTrait for WindowMaskCallback {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen: &ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(resources) = resources.get::<WindowMaskResources>() else {
            return Vec::new();
        };
        let values = [
            screen.size_in_pixels[0] as f32,
            screen.size_in_pixels[1] as f32,
            self.corner_radius * screen.pixels_per_point,
            screen.pixels_per_point,
        ];
        let bytes = unsafe {
            std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), std::mem::size_of_val(&values))
        };
        queue.write_buffer(&resources.uniform_buffer, 0, bytes);
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        resources: &CallbackResources,
    ) {
        let Some(resources) = resources.get::<WindowMaskResources>() else {
            return;
        };
        render_pass.set_pipeline(&resources.pipeline);
        render_pass.set_bind_group(0, &resources.bind_group, &[]);
        render_pass.draw(0..4, 0..1);
    }
}

pub(super) fn setup(render_state: &RenderState) {
    let device = &render_state.device;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("window-mask-shader"),
        source: wgpu::ShaderSource::Wgsl(WINDOW_MASK_SHADER.into()),
    });
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("window-mask-bind-group-layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: NonZeroU64::new(16),
            },
            count: None,
        }],
    });
    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("window-mask-uniforms"),
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("window-mask-bind-group"),
        layout: &bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform_buffer.as_entire_binding(),
        }],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("window-mask-pipeline-layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let multiply_by_mask = wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::Zero,
            dst_factor: wgpu::BlendFactor::SrcAlpha,
            operation: wgpu::BlendOperation::Add,
        },
        alpha: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::Zero,
            dst_factor: wgpu::BlendFactor::SrcAlpha,
            operation: wgpu::BlendOperation::Add,
        },
    };
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("window-mask-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: 4,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: render_state.target_format,
                blend: Some(multiply_by_mask),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });

    render_state
        .renderer
        .write()
        .callback_resources
        .insert(WindowMaskResources {
            pipeline,
            bind_group,
            uniform_buffer,
        });
}

pub(super) fn paint(painter: &Painter, screen: Rect, corner_radius: u8) {
    painter.add(egui_wgpu::Callback::new_paint_callback(
        screen,
        WindowMaskCallback {
            corner_radius: corner_radius as f32,
        },
    ));
}

const WINDOW_MASK_SHADER: &str = r#"
struct MaskUniforms {
    screen_size: vec2<f32>,
    radius: f32,
    border_inset: f32,
};

@group(0) @binding(0) var<uniform> uniforms: MaskUniforms;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

fn rounded_box(point: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
    let edge = abs(point) - half_size + radius;
    return min(max(edge.x, edge.y), 0.0)
        + length(max(edge, vec2<f32>(0.0)))
        - radius;
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let feather = max(1.25, uniforms.border_inset * 0.75);
    // 让遮罩的直边落在窗口外一小段距离。这样顶部/左右/底部第一行保持
    // 完全不透明，只有四个圆角弯道会进行抗锯齿羽化，不会形成白色横线。
    let half_size = uniforms.screen_size * 0.5 + vec2<f32>(feather + uniforms.border_inset);
    let center = uniforms.screen_size * 0.5;
    let distance_to_edge = rounded_box(
        position.xy - center,
        half_size,
        uniforms.radius
    );
    let coverage = 1.0 - smoothstep(-feather, feather, distance_to_edge);
    return vec4<f32>(coverage, coverage, coverage, coverage);
}
"#;
