//! # `src/app/skin_rendering.rs`
//!
//! **功能域：** 启动器应用外壳、页面状态、交互流程和原生 egui 绘制。
//!
//! 执行 3D 皮肤投影、光栅化和纹理裁剪
//!
//! ## 模块职责
//! - 保持本文件声明的类型、函数和状态转换职责单一。
//! - 仅通过稳定的模块接口与其他文件协作，避免复制共享业务规则。
//!
//! ## 主要接口
//! - `skin_cuboids`
//! - `skin_cuboid_faces`
//! - `rotate_skin_point`
//! - `project_skin_point`
//! - `skin_preview_faces`
//! - `skin_outer_layers`
//! - `skin_head_preview_faces`
//! - `paint_skin_model`
//! - `skin_face_at`
//! - `paint_rounded_texture_region`
//! - `paint_rounded_image`
//!
//! ## 错误与并发
//! - 可失败的外部操作在调用前由调用方或更底层模块完成错误转换。
//! - 当前实现以同步调用为主；新增线程或共享状态时需明确所有权和取消策略。
//!
//! ## 安全与平台
//! - 本文件不应接受未经上游校验的外部路径、URL 或序列化数据。
//! - 代码保持平台无关，平台差异应继续封装在明确的 ``cfg`` 边界内。

use super::*;

/// 实现 皮肤cuboids 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `[SkinCuboid`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn skin_cuboids(alex: bool) -> [SkinCuboid; 6] {
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

/// 实现 皮肤cuboidfaces 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `[SkinCuboidFace`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn skin_cuboid_faces(cuboid: SkinCuboid) -> [SkinCuboidFace; 6] {
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
            // 背面位于“两层深度 + 一个正面宽度”之后。躯干宽度 8、深度 4，
            // 不能再用 3 * depth 代替该偏移，否则会与左侧面或正面区域重叠。
            uv: uv(
                cuboid.uv_x + size.z * 2.0 + size.x,
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
            // 另一个侧面位于“一个深度 + 一个正面宽度”之后；该公式对正方形头
            // 和非正方形躯干都成立，避免把躯干 UV 错读成正面像素。
            uv: uv(
                cuboid.uv_x + size.z + size.x,
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

/// 实现 rotate皮肤point 相关的业务流程。
///
/// # 参数
/// - `yaw`：由调用方传入的yaw。
/// - `pitch`：由调用方传入的pitch。
/// # 返回
/// 返回 `SkinPoint3`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn rotate_skin_point(point: SkinPoint3, yaw: f32, pitch: f32) -> SkinPoint3 {
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

/// 实现 项目皮肤point 相关的业务流程。
///
/// # 参数
/// - `stage`：由调用方传入的stage。
/// - `scale`：由调用方传入的scale。
/// - `yaw`：由调用方传入的yaw。
/// - `pitch`：由调用方传入的pitch。
/// # 返回
/// 返回 `(Pos2, f32)`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn project_skin_point(
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

/// 实现 皮肤预览faces 相关的业务流程。
///
/// # 参数
/// - `scale`：由调用方传入的scale。
/// - `yaw`：由调用方传入的yaw。
/// - `pitch`：由调用方传入的pitch。
/// - `alex`：由调用方传入的alex。
/// # 返回
/// 返回 `Vec<SkinPreviewFace>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn skin_preview_faces(
    stage: Rect,
    scale: f32,
    yaw: f32,
    pitch: f32,
    alex: bool,
) -> Vec<SkinPreviewFace> {
    let light = SkinPoint3::new(-0.32, 0.62, 0.72);
    let base = skin_cuboids(alex);
    let outer = skin_outer_layers(base);
    let mut faces = Vec::with_capacity(72);
    for (cuboid, inflation) in base
        .into_iter()
        .map(|cuboid| (cuboid, 1.0))
        .chain(outer.into_iter().map(|cuboid| (cuboid, 1.045)))
    {
        let center = SkinPoint3::new(
            (cuboid.min.x + cuboid.max.x) * 0.5,
            (cuboid.min.y + cuboid.max.y) * 0.5,
            (cuboid.min.z + cuboid.max.z) * 0.5,
        );
        for face in skin_cuboid_faces(cuboid) {
            let normal = rotate_skin_point(face.normal, yaw, pitch);
            if normal.z <= 0.001 {
                continue;
            }
            let projected = face.points.map(|point| {
                project_skin_point(
                    SkinPoint3::new(
                        center.x + (point.x - center.x) * inflation,
                        center.y + (point.y - center.y) * inflation,
                        center.z + (point.z - center.z) * inflation,
                    ),
                    stage,
                    scale,
                    yaw,
                    pitch,
                )
            });
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

/// 实现 皮肤outerlayers 相关的业务流程。
///
/// 该函数不接收显式参数，直接使用所属对象或模块状态。
/// # 返回
/// 返回 `[SkinCuboid`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn skin_outer_layers(base: [SkinCuboid; 6]) -> [SkinCuboid; 6] {
    let uvs = [
        (32.0, 0.0),
        (16.0, 32.0),
        (40.0, 32.0),
        (48.0, 48.0),
        (0.0, 32.0),
        (0.0, 48.0),
    ];
    let mut outer = base;
    for (cuboid, (uv_x, uv_y)) in outer.iter_mut().zip(uvs) {
        cuboid.uv_x = uv_x;
        cuboid.uv_y = uv_y;
    }
    outer
}

/// 实现 皮肤head预览faces 相关的业务流程。
///
/// # 参数
/// - `scale`：由调用方传入的scale。
/// - `yaw`：由调用方传入的yaw。
/// - `pitch`：由调用方传入的pitch。
/// # 返回
/// 返回 `Vec<SkinPreviewFace>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn skin_head_preview_faces(
    stage: Rect,
    scale: f32,
    yaw: f32,
    pitch: f32,
) -> Vec<SkinPreviewFace> {
    let light = SkinPoint3::new(-0.32, 0.62, 0.72);
    let head = skin_cuboids(false)[0];
    let mut hat = head;
    hat.uv_x = 32.0;
    hat.uv_y = 0.0;
    let mut faces = Vec::with_capacity(12);
    for (cuboid, inflation) in [(head, 1.0), (hat, 1.065)] {
        let center = SkinPoint3::new(
            (cuboid.min.x + cuboid.max.x) * 0.5,
            (cuboid.min.y + cuboid.max.y) * 0.5,
            (cuboid.min.z + cuboid.max.z) * 0.5,
        );
        for face in skin_cuboid_faces(cuboid) {
            let normal = rotate_skin_point(face.normal, yaw, pitch);
            if normal.z <= 0.001 {
                continue;
            }
            let mut projected = [(Pos2::ZERO, 0.0); 4];
            for (index, point) in face.points.into_iter().enumerate() {
                let relative = rotate_skin_point(
                    SkinPoint3::new(
                        (point.x - center.x) * inflation,
                        (point.y - center.y) * inflation,
                        (point.z - center.z) * inflation,
                    ),
                    yaw,
                    pitch,
                );
                projected[index] = (
                    Pos2::new(
                        stage.center().x + relative.x * scale,
                        stage.center().y - relative.y * scale,
                    ),
                    relative.z,
                );
            }
            let depth = projected.iter().map(|(_, depth)| *depth).sum::<f32>() / 4.0;
            let lighting = (normal.x * light.x + normal.y * light.y + normal.z * light.z).max(0.0);
            let brightness = (214.0 + lighting * 40.0).clamp(0.0, 255.0) as u8;
            faces.push(SkinPreviewFace {
                key: SkinFaceKey {
                    part: SkinPart::Head,
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

/// 实现 绘制皮肤模型 相关的业务流程。
///
/// # 参数
/// - `texture`：由调用方传入的texture。
/// - `faces`：由调用方传入的faces。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn paint_skin_model(
    painter: &Painter,
    texture: &egui::TextureHandle,
    faces: &[SkinPreviewFace],
) {
    let mut mesh = egui::epaint::Mesh::with_texture(texture.id());
    for face in faces {
        let start = mesh.vertices.len() as u32;
        // 网格采样与 `texture_position` 必须经过同一个 UV 规范化函数。
        // 当前皮肤纹理与 egui 都使用左上原点，因此正常情况下 Y 坐标不解翻转。
        let uv = [
            map_skin_atlas_uv_to_egui(face.uv.left_top()),
            map_skin_atlas_uv_to_egui(face.uv.right_top()),
            map_skin_atlas_uv_to_egui(face.uv.right_bottom()),
            map_skin_atlas_uv_to_egui(face.uv.left_bottom()),
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

/// 实现 皮肤face 相关的业务流程。
///
/// # 参数
/// - `pointer`：由调用方传入的指针。
/// # 返回
/// 返回 `Option<&'a SkinPreviewFace>`，供上层继续渲染、判断或持久化。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn skin_face_at<'a>(
    mut faces: impl Iterator<Item = &'a SkinPreviewFace>,
    pointer: Pos2,
) -> Option<&'a SkinPreviewFace> {
    faces.find(|face| face.texture_position(pointer).is_some())
}

/// 实现 绘制roundedtextureregion 相关的业务流程。
///
/// # 参数
/// - `rect`：由调用方传入的rect。
/// - `radius`：由调用方传入的radius。
/// - `texture`：由调用方传入的texture。
/// - `source`：由调用方传入的source。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn paint_rounded_texture_region(
    painter: &Painter,
    rect: Rect,
    radius: f32,
    texture: &egui::TextureHandle,
    source: Rect,
) {
    let radius = radius.min(rect.width() * 0.5).min(rect.height() * 0.5);
    if radius < 0.5 {
        painter.image(texture.id(), rect, source, Color32::WHITE);
        return;
    }
    let pixels_per_point = painter.ctx().pixels_per_point().max(1.0);
    let physical_radius = radius * pixels_per_point;
    let corner_segments = (physical_radius * 0.55).ceil().clamp(12.0, 32.0) as usize;
    // 用 1 个物理像素宽的透明外环做覆盖率羽化。它不依赖 MSAA，
    // 因此图标、背景图和版本图标在小尺寸下也能得到平滑圆角。
    let feather = 1.0 / pixels_per_point;
    let inner_radius = (radius - feather * 0.35).max(0.0);
    let outer_radius = radius + feather * 0.65;
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
    let build_perimeter = |arc_radius: f32| {
        let mut points = Vec::with_capacity((corner_segments + 1) * 4);
        for (center, start, end) in corners {
            for segment in 0..=corner_segments {
                let angle = start + (end - start) * segment as f32 / corner_segments as f32;
                points.push(center + Vec2::angled(angle) * arc_radius);
            }
        }
        points
    };
    let inner_perimeter = build_perimeter(inner_radius);
    let outer_perimeter = build_perimeter(outer_radius);
    let uv = |point: Pos2| {
        Pos2::new(
            (source.left() + (point.x - rect.left()) / rect.width() * source.width())
                .clamp(source.left(), source.right()),
            (source.top() + (point.y - rect.top()) / rect.height() * source.height())
                .clamp(source.top(), source.bottom()),
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
    for point in &inner_perimeter {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: *point,
            uv: uv(*point),
            color: Color32::WHITE,
        });
    }
    for point in &outer_perimeter {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: *point,
            uv: uv(*point),
            color: Color32::TRANSPARENT,
        });
    }
    let perimeter_len = inner_perimeter.len();
    for index in 0..perimeter_len {
        let current = (index + 1) as u32;
        let next = ((index + 1) % perimeter_len + 1) as u32;
        let outer_current = (perimeter_len + index + 1) as u32;
        let outer_next = (perimeter_len + (index + 1) % perimeter_len + 1) as u32;
        mesh.add_triangle(0, current, next);
        mesh.add_triangle(current, outer_current, next);
        mesh.add_triangle(next, outer_current, outer_next);
    }
    painter.add(Shape::mesh(mesh));
}

/// 实现 绘制rounded图片 相关的业务流程。
///
/// # 参数
/// - `rect`：由调用方传入的rect。
/// - `radius`：由调用方传入的radius。
/// - `texture`：由调用方传入的texture。
/// - `source_size`：由调用方传入的sourcesize。
/// # 约束
/// 调用方应遵守模块边界；涉及文件、网络或线程时不得绕过本文件已有的校验与错误处理。
pub(super) fn paint_rounded_image(
    painter: &Painter,
    rect: Rect,
    radius: f32,
    texture: &egui::TextureHandle,
    source_size: [usize; 2],
) {
    let source_aspect = source_size[0] as f32 / source_size[1].max(1) as f32;
    let target_aspect = rect.width() / rect.height().max(1.0);
    let (u0, u1, v0, v1) = if source_aspect > target_aspect {
        let visible = target_aspect / source_aspect;
        ((1.0 - visible) * 0.5, (1.0 + visible) * 0.5, 0.0, 1.0)
    } else {
        let visible = source_aspect / target_aspect;
        (0.0, 1.0, (1.0 - visible) * 0.5, (1.0 + visible) * 0.5)
    };
    paint_rounded_texture_region(
        painter,
        rect,
        radius,
        texture,
        Rect::from_min_max(Pos2::new(u0, v0), Pos2::new(u1, v1)),
    );
}
