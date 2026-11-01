use eframe::egui::Vec2;

/// Fixed vector-design resolution used by the launcher artboard.
pub const DESIGN_DPI: f32 = 648.0;

/// Canonical artboard size in logical vector units.
pub const ARTBOARD_SIZE: Vec2 = Vec2::new(1440.0, 900.0);

/// The on-screen window keeps the original logical artboard size.
pub const DESIGN_SURFACE_SIZE: Vec2 = ARTBOARD_SIZE;
