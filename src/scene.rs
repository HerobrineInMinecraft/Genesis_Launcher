use eframe::egui::{self, Color32, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2};

use crate::theme;

pub fn paint_world(painter: &Painter, rect: Rect, time: f64) {
    let clip = painter.clone();
    let p = |x: f32, y: f32| {
        Pos2::new(
            rect.left() + rect.width() * x,
            rect.top() + rect.height() * y,
        )
    };
    let scale = (rect.width() / 1308.0).clamp(0.72, 1.35);

    theme::vertical_gradient(
        &clip,
        rect,
        Color32::from_rgb(66, 141, 190),
        Color32::from_rgb(204, 225, 194),
    );

    let sun = p(0.79, 0.16);
    for radius in [96.0, 72.0, 48.0] {
        clip.circle_filled(sun, radius * scale, theme::white_alpha(7));
    }
    clip.rect_filled(
        Rect::from_center_size(sun, Vec2::splat(84.0 * scale)),
        1,
        theme::alpha(Color32::from_rgb(255, 239, 171), 238),
    );
    clip.rect_filled(
        Rect::from_center_size(sun, Vec2::new(112.0 * scale, 61.0 * scale)),
        1,
        theme::alpha(Color32::from_rgb(255, 242, 181), 185),
    );

    let drift = ((time * 3.0) as f32 % 70.0) - 35.0;
    paint_cloud(&clip, p, -0.02, 0.18, 0.16, drift * 0.35, scale);
    paint_cloud(&clip, p, 0.51, 0.13, 0.11, drift, scale);
    paint_cloud(&clip, p, 0.84, 0.34, 0.09, drift * 0.55, scale);
    paint_cloud(&clip, p, 0.17, 0.37, 0.07, drift * 0.8, scale);

    poly(
        &clip,
        rect,
        &[
            (0.0, 0.58),
            (0.09, 0.46),
            (0.16, 0.54),
            (0.25, 0.41),
            (0.33, 0.51),
            (0.40, 0.37),
            (0.49, 0.49),
            (0.57, 0.36),
            (0.67, 0.50),
            (0.75, 0.39),
            (0.83, 0.52),
            (0.91, 0.42),
            (1.0, 0.55),
            (1.0, 1.0),
            (0.0, 1.0),
        ],
        Color32::from_rgb(86, 127, 124),
    );
    poly(
        &clip,
        rect,
        &[
            (0.0, 0.68),
            (0.10, 0.55),
            (0.18, 0.64),
            (0.29, 0.51),
            (0.38, 0.62),
            (0.47, 0.50),
            (0.57, 0.64),
            (0.66, 0.52),
            (0.76, 0.65),
            (0.86, 0.53),
            (0.94, 0.63),
            (1.0, 0.56),
            (1.0, 1.0),
            (0.0, 1.0),
        ],
        Color32::from_rgb(55, 103, 91),
    );
    poly(
        &clip,
        rect,
        &[
            (0.0, 0.75),
            (0.12, 0.63),
            (0.22, 0.73),
            (0.33, 0.59),
            (0.44, 0.72),
            (0.55, 0.60),
            (0.67, 0.73),
            (0.78, 0.62),
            (0.89, 0.72),
            (1.0, 0.63),
            (1.0, 1.0),
            (0.0, 1.0),
        ],
        Color32::from_rgb(43, 83, 72),
    );

    paint_floating_island(&clip, p, scale, time);

    poly(
        &clip,
        rect,
        &[(0.52, 0.78), (0.61, 0.72), (0.70, 0.77), (0.62, 0.82)],
        Color32::from_rgb(91, 170, 191),
    );
    poly(
        &clip,
        rect,
        &[(0.61, 0.72), (0.70, 0.77), (0.70, 0.99), (0.61, 0.94)],
        Color32::from_rgb(36, 99, 113),
    );
    poly(
        &clip,
        rect,
        &[(0.52, 0.78), (0.62, 0.73), (0.61, 0.95), (0.52, 1.0)],
        Color32::from_rgb(61, 140, 160),
    );
    for (x, y, width) in [
        (0.555, 0.815, 0.04),
        (0.606, 0.84, 0.06),
        (0.553, 0.89, 0.045),
        (0.612, 0.925, 0.052),
    ] {
        clip.rect_filled(
            Rect::from_center_size(p(x, y), Vec2::new(rect.width() * width, 2.0 * scale)),
            1,
            theme::alpha(Color32::from_rgb(209, 241, 235), 76),
        );
    }

    poly(
        &clip,
        rect,
        &[
            (0.0, 0.82),
            (0.09, 0.74),
            (0.18, 0.79),
            (0.27, 0.73),
            (0.38, 0.82),
            (0.47, 0.76),
            (0.57, 0.84),
            (0.68, 0.76),
            (0.79, 0.83),
            (0.90, 0.74),
            (1.0, 0.82),
            (1.0, 1.0),
            (0.0, 1.0),
        ],
        Color32::from_rgb(50, 101, 61),
    );
    poly(
        &clip,
        rect,
        &[
            (0.0, 0.91),
            (0.11, 0.81),
            (0.21, 0.88),
            (0.31, 0.80),
            (0.42, 0.90),
            (0.54, 0.82),
            (0.67, 0.90),
            (0.78, 0.82),
            (0.89, 0.88),
            (1.0, 0.79),
            (1.0, 1.0),
            (0.0, 1.0),
        ],
        Color32::from_rgb(70, 132, 77),
    );

    paint_tree(&clip, p(0.385, 0.72), 0.85 * scale);
    paint_cottage(&clip, p(0.76, 0.70), scale);

    let cube_positions = [
        (0.105, 0.72, 0.47),
        (0.205, 0.80, 0.36),
        (0.695, 0.78, 0.40),
        (0.075, 0.89, 0.30),
    ];
    for (index, (x, y, size)) in cube_positions.into_iter().enumerate() {
        let colors = if index % 2 == 0 {
            (
                Color32::from_rgb(111, 185, 85),
                Color32::from_rgb(97, 70, 49),
                Color32::from_rgb(122, 91, 60),
            )
        } else {
            (
                Color32::from_rgb(127, 198, 104),
                Color32::from_rgb(108, 78, 53),
                Color32::from_rgb(137, 102, 66),
            )
        };
        paint_cube(
            &clip,
            p(x, y),
            rect.width() * size,
            colors.0,
            colors.1,
            colors.2,
        );
    }

    for (index, (x, y)) in [
        (0.34, 0.45),
        (0.43, 0.34),
        (0.55, 0.45),
        (0.65, 0.32),
        (0.88, 0.30),
        (0.26, 0.29),
    ]
    .into_iter()
    .enumerate()
    {
        let pulse = ((time as f32 * 0.45 + index as f32).sin() * 0.5 + 0.5) * 0.6 + 0.2;
        clip.rect_filled(
            Rect::from_center_size(p(x, y), Vec2::splat((2.5 + index as f32 * 0.18) * scale)),
            0,
            theme::alpha(Color32::from_rgb(255, 249, 203), (130.0 * pulse) as u8),
        );
    }

    theme::horizontal_fade(
        &clip,
        rect,
        theme::alpha(Color32::from_rgb(8, 18, 13), 220),
        theme::alpha(Color32::from_rgb(10, 21, 15), 42),
        Color32::TRANSPARENT,
    );
    theme::vertical_gradient(
        &clip,
        Rect::from_min_max(
            Pos2::new(rect.left(), rect.bottom() - rect.height() * 0.35),
            rect.right_bottom(),
        ),
        Color32::TRANSPARENT,
        theme::alpha(Color32::from_rgb(5, 12, 8), 215),
    );
    clip.rect_filled(rect, theme::RADIUS, theme::black_alpha(12));
}

fn paint_cloud(
    painter: &Painter,
    p: impl Fn(f32, f32) -> Pos2,
    x: f32,
    y: f32,
    size: f32,
    drift: f32,
    scale: f32,
) {
    let origin = p(x, y) + Vec2::new(drift, 0.0);
    let s = size * scale;
    let cloud = Color32::from_rgba_unmultiplied(224, 239, 242, 47);
    painter.rect_filled(
        Rect::from_min_size(
            origin - Vec2::new(s * 0.45, 0.0),
            Vec2::new(s * 1.8, s * 0.20),
        ),
        0,
        cloud,
    );
    painter.rect_filled(
        Rect::from_min_size(
            origin - Vec2::new(s * 0.20, s * 0.20),
            Vec2::new(s * 1.12, s * 0.22),
        ),
        0,
        cloud,
    );
    painter.rect_filled(
        Rect::from_min_size(origin, Vec2::new(s * 0.55, s * 0.22)),
        0,
        cloud,
    );
}

fn paint_floating_island(painter: &Painter, p: impl Fn(f32, f32) -> Pos2, scale: f32, time: f64) {
    let bob = (time * 0.45).sin() as f32 * 2.2;
    let top = p(0.75, 0.31) + Vec2::Y * bob;
    let half = 88.0 * scale;
    let depth = 62.0 * scale;
    let top_poly = vec![
        top + Vec2::new(-half, 0.0),
        top + Vec2::new(0.0, -24.0 * scale),
        top + Vec2::new(half, 0.0),
        top + Vec2::new(0.0, 24.0 * scale),
    ];
    painter.add(egui::Shape::convex_polygon(
        top_poly,
        Color32::from_rgb(119, 192, 104),
        Stroke::NONE,
    ));
    painter.add(egui::Shape::convex_polygon(
        vec![
            top + Vec2::new(-half, 0.0),
            top + Vec2::new(0.0, 24.0 * scale),
            top + Vec2::new(0.0, 24.0 * scale + depth),
            top + Vec2::new(-half, depth),
        ],
        Color32::from_rgb(97, 70, 49),
        Stroke::NONE,
    ));
    painter.add(egui::Shape::convex_polygon(
        vec![
            top + Vec2::new(half, 0.0),
            top + Vec2::new(0.0, 24.0 * scale),
            top + Vec2::new(0.0, 24.0 * scale + depth),
            top + Vec2::new(half, depth),
        ],
        Color32::from_rgb(123, 90, 58),
        Stroke::NONE,
    ));
    painter.rect_filled(
        Rect::from_min_size(
            top + Vec2::new(-27.0 * scale, -5.0 * scale),
            Vec2::new(48.0 * scale, 8.0 * scale),
        ),
        1,
        theme::alpha(Color32::from_rgb(165, 226, 135), 105),
    );
    paint_tree(painter, top + Vec2::new(42.0, -16.0) * scale, 0.46 * scale);
}

fn paint_tree(painter: &Painter, base: Pos2, scale: f32) {
    painter.rect_filled(
        Rect::from_min_size(
            base + Vec2::new(-5.0, 8.0) * scale,
            Vec2::new(10.0, 49.0) * scale,
        ),
        1,
        Color32::from_rgb(80, 57, 37),
    );
    let leaves = [
        (-32.0, -18.0, 38.0, 21.0, 45, 113, 63),
        (-15.0, -43.0, 41.0, 22.0, 54, 134, 70),
        (17.0, -20.0, 40.0, 22.0, 46, 116, 61),
        (-8.0, 0.0, 55.0, 23.0, 60, 143, 73),
        (-20.0, -27.0, 39.0, 16.0, 88, 165, 86),
    ];
    for (x, y, w, h, r, g, b) in leaves {
        painter.rect_filled(
            Rect::from_min_size(base + Vec2::new(x, y) * scale, Vec2::new(w, h) * scale),
            1,
            Color32::from_rgb(r, g, b),
        );
    }
    for (x, y) in [(-19.0, -30.0), (21.0, -18.0), (2.0, -39.0)] {
        painter.rect_filled(
            Rect::from_min_size(base + Vec2::new(x, y) * scale, Vec2::splat(7.0 * scale)),
            0,
            theme::alpha(Color32::from_rgb(154, 214, 111), 95),
        );
    }
}

fn paint_cottage(painter: &Painter, base: Pos2, scale: f32) {
    let wall = Rect::from_min_size(
        base + Vec2::new(-48.0, -48.0) * scale,
        Vec2::new(96.0, 66.0) * scale,
    );
    painter.rect_filled(wall, 1, Color32::from_rgb(128, 91, 56));
    painter.rect_filled(
        Rect::from_min_size(
            wall.left_top() + Vec2::new(36.0, 21.0) * scale,
            Vec2::new(24.0, 31.0) * scale,
        ),
        1,
        Color32::from_rgb(52, 38, 29),
    );
    painter.rect_filled(
        Rect::from_min_size(
            wall.left_top() + Vec2::new(10.0, 20.0) * scale,
            Vec2::new(17.0, 17.0) * scale,
        ),
        1,
        Color32::from_rgb(248, 208, 112),
    );
    painter.add(egui::Shape::convex_polygon(
        vec![
            base + Vec2::new(-64.0, -49.0) * scale,
            base + Vec2::new(0.0, -88.0) * scale,
            base + Vec2::new(64.0, -49.0) * scale,
            base + Vec2::new(45.0, -39.0) * scale,
            base + Vec2::new(0.0, -69.0) * scale,
            base + Vec2::new(-45.0, -39.0) * scale,
        ],
        Color32::from_rgb(177, 94, 66),
        Stroke::new(1.0, Color32::from_rgb(209, 125, 82)),
    ));
    painter.rect_filled(
        Rect::from_min_size(
            base + Vec2::new(-14.0, -75.0) * scale,
            Vec2::new(9.0, 20.0) * scale,
        ),
        1,
        Color32::from_rgb(89, 64, 49),
    );
}

fn paint_cube(
    painter: &Painter,
    center: Pos2,
    size: f32,
    top: Color32,
    left: Color32,
    right: Color32,
) {
    let w = size * 0.50;
    let h = size * 0.24;
    let d = size * 0.52;
    painter.add(egui::Shape::convex_polygon(
        vec![
            center + Vec2::new(0.0, -h),
            center + Vec2::new(w, 0.0),
            center + Vec2::new(0.0, h),
            center + Vec2::new(-w, 0.0),
        ],
        top,
        Stroke::new(0.8, theme::alpha(Color32::WHITE, 25)),
    ));
    painter.add(egui::Shape::convex_polygon(
        vec![
            center + Vec2::new(-w, 0.0),
            center + Vec2::new(0.0, h),
            center + Vec2::new(0.0, h + d),
            center + Vec2::new(-w, d),
        ],
        left,
        Stroke::NONE,
    ));
    painter.add(egui::Shape::convex_polygon(
        vec![
            center + Vec2::new(w, 0.0),
            center + Vec2::new(0.0, h),
            center + Vec2::new(0.0, h + d),
            center + Vec2::new(w, d),
        ],
        right,
        Stroke::NONE,
    ));
}

fn poly(painter: &Painter, rect: Rect, points: &[(f32, f32)], color: Color32) {
    let points = points
        .iter()
        .map(|(x, y)| {
            Pos2::new(
                rect.left() + rect.width() * x,
                rect.top() + rect.height() * y,
            )
        })
        .collect();
    painter.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
}

pub fn paint_news_art(painter: &Painter, rect: Rect, kind: NewsArt, time: f64) {
    let clip = painter.clone();
    let (start, _end) = match kind {
        NewsArt::Version => (Color32::from_rgb(25, 62, 55), Color32::from_rgb(15, 37, 31)),
        NewsArt::Mod => (Color32::from_rgb(39, 72, 44), Color32::from_rgb(19, 35, 25)),
        NewsArt::Community => (Color32::from_rgb(39, 51, 76), Color32::from_rgb(23, 31, 46)),
    };
    theme::fill_round(painter, rect, theme::RADIUS, start);
    clip.circle_filled(rect.center(), rect.width() * 0.42, theme::white_alpha(8));

    match kind {
        NewsArt::Version => {
            let center = rect.center() + Vec2::new(0.0, (time * 0.7).sin() as f32 * 1.5);
            let half = rect.width().min(rect.height()) * 0.18;
            clip.add(egui::Shape::convex_polygon(
                vec![
                    center + Vec2::new(0.0, -half),
                    center + Vec2::new(half, 0.0),
                    center + Vec2::new(0.0, half),
                    center + Vec2::new(-half, 0.0),
                ],
                Color32::from_rgb(78, 198, 194),
                Stroke::new(1.0, theme::alpha(Color32::WHITE, 105)),
            ));
            clip.add(egui::Shape::convex_polygon(
                vec![
                    center + Vec2::new(0.0, -half * 0.52),
                    center + Vec2::new(half * 0.52, 0.0),
                    center + Vec2::new(0.0, half * 0.52),
                    center + Vec2::new(-half * 0.52, 0.0),
                ],
                theme::alpha(Color32::from_rgb(190, 246, 238), 90),
                Stroke::NONE,
            ));
        }
        NewsArt::Mod => {
            for (index, offset) in [
                Vec2::new(-11.0, -13.0),
                Vec2::new(5.0, 10.0),
                Vec2::new(19.0, -16.0),
            ]
            .into_iter()
            .enumerate()
            {
                let bob = (time * 1.2 + index as f64).sin() as f32 * 2.0;
                let center = rect.center() + offset + Vec2::Y * bob;
                let size = rect.width() * if index == 0 { 0.20 } else { 0.13 };
                paint_cube(
                    &clip,
                    center,
                    size,
                    Color32::from_rgb(138, 213, 105),
                    Color32::from_rgb(69, 128, 68),
                    Color32::from_rgb(83, 151, 77),
                );
            }
        }
        NewsArt::Community => {
            let base = rect.center() + Vec2::new(0.0, 17.0);
            for (x, w, h, color) in [
                (-25.0, 20.0, 40.0, Color32::from_rgb(83, 98, 119)),
                (-4.0, 28.0, 54.0, Color32::from_rgb(97, 113, 136)),
                (25.0, 18.0, 32.0, Color32::from_rgb(73, 87, 107)),
            ] {
                clip.rect_filled(
                    Rect::from_min_size(base + Vec2::new(x, -h), Vec2::new(w, h)),
                    0,
                    color,
                );
            }
            clip.rect_filled(
                Rect::from_min_size(base + Vec2::new(-31.0, 0.0), Vec2::new(77.0, 7.0)),
                0,
                Color32::from_rgb(49, 60, 79),
            );
            clip.circle_filled(
                rect.right_top() + Vec2::new(-22.0, 22.0),
                7.0,
                Color32::from_rgb(239, 219, 151),
            );
        }
    }

    clip.rect_stroke(
        rect,
        theme::RADIUS,
        Stroke::new(1.0, theme::line()),
        StrokeKind::Inside,
    );
}

#[derive(Clone, Copy)]
pub enum NewsArt {
    Version,
    Mod,
    Community,
}
