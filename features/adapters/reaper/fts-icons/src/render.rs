use anyhow::{Context, Result};
use resvg::tiny_skia::{
    Color, FillRule, Paint, Path, PathBuilder, Pixmap, PixmapPaint, Stroke, Transform,
};
use resvg::usvg;
use std::sync::{Arc, OnceLock};

use crate::color;

/// usvg options with system fonts loaded (needed for `text:` sources).
/// The fontdb scan is slow, so it's done once and shared.
fn svg_options() -> usvg::Options<'static> {
    static DB: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    let db = DB.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        db.load_system_fonts();
        Arc::new(db)
    });
    let mut opt = usvg::Options::default();
    opt.fontdb = db.clone();
    opt
}

/// REAPER toolbar icon cell size at 100%. A full icon is 3 cells side by side
/// (normal / hover / clicked), so 90x30 at 100%, 135x45 at 150%, 180x60 at 200%.
pub const BASE_CELL: f32 = 30.0;
/// (scale factor, subfolder under toolbar_icons — "" = the folder itself)
pub const SCALES: [(f32, &str); 3] = [(1.0, ""), (1.5, "150"), (2.0, "200")];

/// Fully resolved style for one toolbar state. All sizes are px at 100%
/// (i.e. within a 30x30 cell) and get multiplied by the render scale.
#[derive(Clone, Debug)]
pub struct StateStyle {
    pub icon: Color,
    pub bg: Option<Color>,
    pub border: Option<Color>,
    pub border_width: f32,
    pub icon_size: f32,
    pub bg_size: f32,
    pub corner_radius: f32,
}

impl Default for StateStyle {
    fn default() -> Self {
        Self {
            icon: Color::WHITE,
            bg: None,
            border: None,
            border_width: 1.5,
            icon_size: 20.0,
            bg_size: 28.0,
            corner_radius: 5.0,
        }
    }
}

/// Render the 3-state strip at the given scale. `base_w` is the cell width
/// at 100% (30 = square, 60 = double-wide); height is always 30 at 100%.
pub fn render_strip(
    svg: &str,
    states: &[StateStyle; 3],
    scale: f32,
    base_w: f32,
) -> Result<Pixmap> {
    let cw = (base_w * scale).round() as u32;
    let ch = (BASE_CELL * scale).round() as u32;
    let mut strip = Pixmap::new(cw * 3, ch).context("pixmap alloc")?;
    for (i, st) in states.iter().enumerate() {
        let cell_px = render_cell(svg, st, scale, cw, ch)?;
        strip.draw_pixmap(
            (i as u32 * cw) as i32,
            0,
            cell_px.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );
    }
    Ok(strip)
}

fn render_cell(svg: &str, st: &StateStyle, scale: f32, cw: u32, ch: u32) -> Result<Pixmap> {
    let mut pm = Pixmap::new(cw, ch).context("pixmap alloc")?;
    let (w, h) = (cw as f32, ch as f32);

    // Background plate — bg_size sets the height; the same margin applies
    // on all sides, so wide cells get a proportionally wide plate
    if let Some(bg) = st.bg {
        let m = (BASE_CELL - st.bg_size) / 2.0 * scale;
        let rect = round_rect(m, m, w - m, h - m, st.corner_radius * scale);
        let mut paint = Paint::default();
        paint.set_color(bg);
        paint.anti_alias = true;
        pm.fill_path(
            &rect,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    // Icon: substitute currentColor with the rgb part, composite with its alpha.
    // Fit into a box icon_size tall whose width keeps the same margins.
    let tinted = svg.replace("currentColor", &color::to_rgb_hex(st.icon));
    let tree = usvg::Tree::from_data(tinted.as_bytes(), &svg_options()).context("parse svg")?;
    let size = tree.size();
    let im = (BASE_CELL - st.icon_size) / 2.0 * scale;
    let s = ((w - 2.0 * im) / size.width()).min((h - 2.0 * im) / size.height());
    let tx = (w - size.width() * s) / 2.0;
    let ty = (h - size.height() * s) / 2.0;
    let mut icon_pm = Pixmap::new(cw, ch).context("pixmap alloc")?;
    resvg::render(
        &tree,
        Transform::from_scale(s, s).post_translate(tx, ty),
        &mut icon_pm.as_mut(),
    );
    pm.draw_pixmap(
        0,
        0,
        icon_pm.as_ref(),
        &PixmapPaint {
            opacity: st.icon.alpha(),
            ..Default::default()
        },
        Transform::identity(),
        None,
    );

    // Border on top, inset by half the stroke so it stays inside the plate
    if let Some(border) = st.border {
        let bw = st.border_width * scale;
        let m = (BASE_CELL - st.bg_size) / 2.0 * scale + bw / 2.0;
        let rect = round_rect(
            m,
            m,
            w - m,
            h - m,
            (st.corner_radius * scale - bw / 2.0).max(0.0),
        );
        let mut paint = Paint::default();
        paint.set_color(border);
        paint.anti_alias = true;
        let stroke = Stroke {
            width: bw,
            ..Default::default()
        };
        pm.stroke_path(&rect, &paint, &stroke, Transform::identity(), None);
    }

    Ok(pm)
}

/// Rounded rect between two corners.
fn round_rect(x0: f32, y0: f32, x1: f32, y1: f32, radius: f32) -> Path {
    let r = radius.clamp(0.0, (x1 - x0).min(y1 - y0) / 2.0);
    const K: f32 = 0.552_284_8; // cubic approximation of a quarter circle
    let kr = K * r;
    let mut pb = PathBuilder::new();
    pb.move_to(x0 + r, y0);
    pb.line_to(x1 - r, y0);
    pb.cubic_to(x1 - r + kr, y0, x1, y0 + r - kr, x1, y0 + r);
    pb.line_to(x1, y1 - r);
    pb.cubic_to(x1, y1 - r + kr, x1 - r + kr, y1, x1 - r, y1);
    pb.line_to(x0 + r, y1);
    pb.cubic_to(x0 + r - kr, y1, x0, y1 - r + kr, x0, y1 - r);
    pb.line_to(x0, y0 + r);
    pb.cubic_to(x0, y0 + r - kr, x0 + r - kr, y0, x0 + r, y0);
    pb.close();
    pb.finish().expect("valid rect path")
}
