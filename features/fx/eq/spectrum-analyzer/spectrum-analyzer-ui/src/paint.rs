//! Vello painters for the analyzer, composited into the EQ graph's scene.
//!
//! These are intentionally decoupled from eq-ui's `CoordMapper`: the caller
//! passes mapping closures (`freq_to_x`, `db_to_y`) so the same painter works
//! for any coordinate system. The `Scene` / `PaintScene` types come from the
//! shared `nice-plug-dioxus` stack, so they are identical to eq-ui's.

use nice_plug_dioxus::prelude::vello::kurbo::{Affine, BezPath, Stroke};
use nice_plug_dioxus::prelude::vello::peniko::{Color, Fill};
use nice_plug_dioxus::widget::{PaintScene as _, Scene};

/// Build a polyline path over `(freq_hz, db)` points, skipping bins outside the
/// visible frequency range. Returns `None` if fewer than two points are visible.
fn spectrum_path<FX, FY>(
    freq_hz: &[f32],
    db: &[f32],
    min_freq: f64,
    max_freq: f64,
    freq_to_x: &FX,
    db_to_y: &FY,
) -> Option<BezPath>
where
    FX: Fn(f64) -> f64,
    FY: Fn(f64) -> f64,
{
    let mut path = BezPath::new();
    let mut started = false;
    for (&f, &d) in freq_hz.iter().zip(db.iter()) {
        let f = f as f64;
        if f < min_freq || f > max_freq {
            continue;
        }
        let x = freq_to_x(f);
        let y = db_to_y(d as f64);
        if !started {
            path.move_to((x, y));
            started = true;
        } else {
            path.line_to((x, y));
        }
    }
    if started {
        Some(path)
    } else {
        None
    }
}

/// Paint a spectrum as a stroked line plus a translucent fill down to `bottom_y`.
#[allow(clippy::too_many_arguments)]
pub fn paint_spectrum_fill<FX, FY>(
    scene: &mut Scene,
    freq_hz: &[f32],
    db: &[f32],
    min_freq: f64,
    max_freq: f64,
    bottom_y: f64,
    stroke: Color,
    fill: Color,
    freq_to_x: FX,
    db_to_y: FY,
    transform: Affine,
) where
    FX: Fn(f64) -> f64,
    FY: Fn(f64) -> f64,
{
    let Some(path) = spectrum_path(freq_hz, db, min_freq, max_freq, &freq_to_x, &db_to_y) else {
        return;
    };

    scene.stroke(&Stroke::new(1.0), transform, stroke, None, &path);

    // Close the path down to the bottom for the fill.
    let mut fill_path = path.clone();
    let first_x = freq_to_x(min_freq.max(freq_hz.first().copied().unwrap_or(0.0) as f64));
    let last_visible = freq_hz
        .iter()
        .rev()
        .find(|&&f| (f as f64) <= max_freq)
        .copied()
        .unwrap_or(0.0) as f64;
    fill_path.line_to((freq_to_x(last_visible), bottom_y));
    fill_path.line_to((first_x, bottom_y));
    fill_path.close_path();
    scene.fill(Fill::NonZero, transform, fill, None, &fill_path);
}

/// Paint a spectrum as a stroked line only (no fill) — use for overlays such as
/// the external/sidechain spectrum.
#[allow(clippy::too_many_arguments)]
pub fn paint_spectrum_line<FX, FY>(
    scene: &mut Scene,
    freq_hz: &[f32],
    db: &[f32],
    min_freq: f64,
    max_freq: f64,
    stroke: Color,
    width: f64,
    freq_to_x: FX,
    db_to_y: FY,
    transform: Affine,
) where
    FX: Fn(f64) -> f64,
    FY: Fn(f64) -> f64,
{
    if let Some(path) = spectrum_path(freq_hz, db, min_freq, max_freq, &freq_to_x, &db_to_y) {
        scene.stroke(&Stroke::new(width), transform, stroke, None, &path);
    }
}

/// Paint collision regions as translucent red vertical bands. `strength` is the
/// per-bin collision value (0..0.9) aligned with `freq_hz`.
#[allow(clippy::too_many_arguments)]
pub fn paint_collisions<FX>(
    scene: &mut Scene,
    freq_hz: &[f32],
    strength: &[f32],
    min_freq: f64,
    max_freq: f64,
    top_y: f64,
    bottom_y: f64,
    freq_to_x: FX,
    transform: Affine,
) where
    FX: Fn(f64) -> f64,
{
    use nice_plug_dioxus::prelude::vello::kurbo::Rect;
    for i in 0..freq_hz.len().min(strength.len()) {
        let s = strength[i];
        if s <= 0.0 {
            continue;
        }
        let f = freq_hz[i] as f64;
        if f < min_freq || f > max_freq {
            continue;
        }
        let x = freq_to_x(f);
        // Width spans to the next bin so adjacent collisions merge visually.
        let next_f = freq_hz.get(i + 1).map(|&v| v as f64).unwrap_or(max_freq);
        let x2 = freq_to_x(next_f.min(max_freq));
        let alpha = (s / 0.9).clamp(0.0, 1.0) * 0.5;
        let color = Color::from_rgba8(255, 60, 60, (alpha * 255.0) as u8);
        let rect = Rect::new(x, top_y, x2.max(x + 1.0), bottom_y);
        scene.fill(Fill::NonZero, transform, color, None, &rect);
    }
}
