//! The pointer's shape on the window: the system's, or a painted glyph.
//!
//! The system's shapes have no `[`, no `]`, no seam and no fade — which
//! is to say none of the shapes that tell you *which* edge you are about
//! to take, or which way a fade runs. The editor already draws those
//! (`expression_editor_paint::cursor`); here they are rasterized into
//! real window cursors, so the arrangement and the Editor point the same
//! way and the shape is the OS pointer itself, not a picture following it.
//!
//! A custom cursor can only be made through the event loop, which a
//! pointer move's handler does not have. So a glyph asked for is queued,
//! the system shape nearest it stands in, and the window-event hook
//! ([`use_cursor_sink`]) makes the queued ones on the next event — the
//! very next pointer move — and puts the glyph up. Made once, kept.

#[cfg(feature = "native")]
use std::cell::RefCell;
#[cfg(feature = "native")]
use std::collections::HashMap;
use std::rc::Rc;

use anyrender::{PaintScene, Scene};
use cursor_icon::CursorIcon;
use expression_editor_core::cursor::Cursor;
use expression_editor_paint::cursor::{SIZE, draw};
use vello::kurbo::{Affine, BezPath, Stroke};
use vello::peniko::Color;

/// A painted pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Glyph {
    /// One of the Editor's: `[` and `]`, the hand, the zoom, the pencil…
    Editor(Cursor),
    /// `][` — the boundary two butted items share, which moves as one.
    Seam,
    /// The fade-in's handle: a ramp rising to the right.
    FadeIn,
    /// The fade-out's: a ramp falling to the right.
    FadeOut,
}

/// What the pointer should look like.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Icon(CursorIcon),
    Glyph(Glyph),
}

impl Shape {
    /// The system shape nearest it — what a host that cannot make
    /// custom cursors shows (the web), and what stands in until one is
    /// made.
    #[must_use]
    pub const fn icon(self) -> CursorIcon {
        match self {
            Self::Icon(icon) => icon,
            Self::Glyph(Glyph::Seam | Glyph::FadeIn | Glyph::FadeOut) => CursorIcon::ColResize,
            Self::Glyph(Glyph::Editor(cursor)) => icon_of(cursor),
        }
    }
}

/// The Editor's glyph as the nearest system shape.
const fn icon_of(cursor: Cursor) -> CursorIcon {
    match cursor {
        Cursor::Arrow | Cursor::Audition | Cursor::Mute => CursorIcon::Default,
        Cursor::Crosshair
        | Cursor::MarqueeAdd
        | Cursor::MarqueeToggle
        | Cursor::Pencil
        | Cursor::Curve
        | Cursor::Brush
        | Cursor::Razor => CursorIcon::Crosshair,
        Cursor::EdgeLeft => CursorIcon::WResize,
        Cursor::EdgeRight => CursorIcon::EResize,
        Cursor::StretchLeft | Cursor::StretchRight | Cursor::Playhead | Cursor::Split => {
            CursorIcon::ColResize
        }
        Cursor::Move => CursorIcon::Move,
        Cursor::MoveH | Cursor::RazorEdge => CursorIcon::EwResize,
        Cursor::MoveV | Cursor::Velocity | Cursor::Scale | Cursor::Handle(_) => {
            CursorIcon::NsResize
        }
        Cursor::Copy => CursorIcon::Copy,
        Cursor::Eraser | Cursor::NoteEraser => CursorIcon::Cell,
        Cursor::Hand => CursorIcon::Grab,
        Cursor::HandClosed => CursorIcon::Grabbing,
        Cursor::Zoom => CursorIcon::ZoomIn,
        Cursor::Text => CursorIcon::Text,
        Cursor::Forbidden => CursorIcon::NotAllowed,
    }
}

/// The glyph's box: twice the Editor's glyph, so a hotspot at a corner
/// still has room to draw in every direction. The hotspot is its centre.
const BOX: f64 = SIZE * 2.0;

impl Glyph {
    /// The glyph drawn in a [`BOX`]-square box, its hotspot at the centre.
    #[must_use]
    pub fn scene(self) -> Scene {
        let mut scene = Scene::new();
        match self {
            Self::Editor(cursor) => draw(&mut scene, cursor, SIZE, SIZE),
            Self::Seam => seam(&mut scene),
            Self::FadeIn => fade(&mut scene, true),
            Self::FadeOut => fade(&mut scene, false),
        }
        scene
    }

    /// The glyph as straight-alpha RGBA, `scale` device pixels to a CSS
    /// pixel: `(pixels, side)`, the hotspot at `side / 2`.
    #[must_use]
    pub fn rgba(self, scale: f64) -> (Vec<u8>, u16) {
        use anyrender::ImageRenderer as _;
        let side = crate::num::index((BOX * scale.max(1.0)).round()).clamp(16, 128);
        let px = u32::try_from(side).unwrap_or(48);
        let scene = self.scene();
        let mut renderer = anyrender_vello_cpu::VelloCpuImageRenderer::new(px, px);
        let mut buffer = Vec::new();
        let zoom = f64::from(px) / BOX;
        renderer.render_to_vec(
            |painter| painter.append_scene(scene, Affine::scale(zoom)),
            &mut buffer,
        );
        // The renderer's pixels are premultiplied; a cursor's are not.
        for pixel in buffer.chunks_exact_mut(4) {
            let &mut [r, g, b, a] = pixel else { continue };
            if a > 0 && a < 255 {
                let un = |c: u8| {
                    u32::from(c)
                        .saturating_mul(255)
                        .saturating_add(u32::from(a >> 1))
                        .checked_div(u32::from(a))
                        .and_then(|v| u8::try_from(v).ok())
                        .unwrap_or(u8::MAX)
                };
                pixel.copy_from_slice(&[un(r), un(g), un(b), a]);
            }
        }
        (buffer, u16::try_from(px).unwrap_or(48))
    }
}

/// Halo then ink, as the Editor's glyphs are drawn — a cursor has to
/// read over a dark grid and a bright waveform alike.
fn passes(scene: &mut Scene, path: &BezPath) {
    let halo = Color::from_rgba8(0, 0, 0, 190);
    // The Editor's glyphs' own ink, so the two sets are one set.
    let ink = expression_editor_paint::paint::color(expression_editor_paint::theme::TEXT_BRIGHT);
    for (color, width) in [(halo, 3.4), (ink, 1.5)] {
        scene.stroke(&Stroke::new(width), Affine::IDENTITY, color, None, path);
    }
}

/// A horizontal double arrow through `y`, `half` either side of `c`.
fn double_arrow(path: &mut BezPath, c: f64, y: f64, half: f64) {
    path.move_to((c - half, y));
    path.line_to((c + half, y));
    for (tip, dir) in [(c - half, 1.0), (c + half, -1.0)] {
        path.move_to((3.5f64.mul_add(dir, tip), y - 3.5));
        path.line_to((tip, y));
        path.line_to((3.5f64.mul_add(dir, tip), y + 3.5));
    }
}

/// `][` with a double arrow through it: the seam moves both ways.
fn seam(scene: &mut Scene) {
    let c = BOX * 0.5;
    let mut path = BezPath::new();
    // `]` on the left: the end of the item before.
    path.move_to((c - 6.0, c - 8.0));
    path.line_to((c - 2.5, c - 8.0));
    path.line_to((c - 2.5, c + 8.0));
    path.line_to((c - 6.0, c + 8.0));
    // `[` on the right: the start of the one after.
    path.move_to((c + 6.0, c - 8.0));
    path.line_to((c + 2.5, c - 8.0));
    path.line_to((c + 2.5, c + 8.0));
    path.line_to((c + 6.0, c + 8.0));
    double_arrow(&mut path, c, c, 12.0);
    passes(scene, &path);
}

/// A fade's ramp, rising (`rising`, a fade-in) or falling to the right,
/// over a double arrow — its length drags sideways.
fn fade(scene: &mut Scene, rising: bool) {
    let c = BOX * 0.5;
    let (low, high) = (c + 3.0, c - 9.0);
    let (from, to) = if rising {
        ((c - 9.0, low), (c + 9.0, high))
    } else {
        ((c - 9.0, high), (c + 9.0, low))
    };
    let mut path = BezPath::new();
    // The ramp, curved as a fade is: slow at its quiet end.
    path.move_to(from);
    let bend = if rising {
        (c + 4.0, low)
    } else {
        (c - 4.0, low)
    };
    path.quad_to(bend, to);
    // The handle at its loud end, where the hand is.
    let handle = if rising { to } else { from };
    path.move_to((handle.0, handle.1 - 3.0));
    path.line_to((handle.0, handle.1 + 3.0));
    double_arrow(&mut path, c, c + 8.0, 9.0);
    passes(scene, &path);
}

/// Where shapes go: the window, through its cursors.
pub type Sink = Rc<dyn Fn(Shape)>;

/// The window's cursors: made, and wanted.
#[cfg(feature = "native")]
#[derive(Default)]
struct Cursors {
    made: HashMap<Glyph, winit::cursor::CustomCursor>,
    /// The glyph last asked for that was not made yet.
    wanted: Option<Glyph>,
}

/// A [`Sink`] onto this window, and the hook that makes its glyphs.
#[cfg(feature = "native")]
#[must_use]
pub fn use_cursor_sink() -> Sink {
    let window = dioxus_native::use_window();
    let cursors = dioxus::prelude::use_hook(|| Rc::new(RefCell::new(Cursors::default())));
    {
        let (window, cursors) = (window.clone(), Rc::clone(&cursors));
        dioxus_native::use_window_event(move |_, event_loop| {
            let Some(glyph) = cursors.borrow_mut().wanted.take() else {
                return;
            };
            let made = cursors.borrow().made.get(&glyph).cloned();
            let cursor = made.or_else(|| {
                let (rgba, side) = glyph.rgba(window.scale_factor());
                let hot = side / 2;
                let source =
                    winit::cursor::CustomCursorSource::from_rgba(rgba, side, side, hot, hot)
                        .ok()?;
                let cursor = event_loop.create_custom_cursor(source).ok()?;
                cursors.borrow_mut().made.insert(glyph, cursor.clone());
                Some(cursor)
            });
            if let Some(cursor) = cursor {
                window.set_cursor(winit::cursor::Cursor::Custom(cursor));
            }
        });
    }
    Rc::new(move |shape: Shape| match shape {
        Shape::Icon(icon) => {
            cursors.borrow_mut().wanted = None;
            window.set_cursor(winit::cursor::Cursor::Icon(icon));
        }
        Shape::Glyph(glyph) => {
            let made = cursors.borrow().made.get(&glyph).cloned();
            if let Some(cursor) = made {
                cursors.borrow_mut().wanted = None;
                window.set_cursor(winit::cursor::Cursor::Custom(cursor));
            } else {
                cursors.borrow_mut().wanted = Some(glyph);
                window.set_cursor(winit::cursor::Cursor::Icon(shape.icon()));
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every glyph rasterizes to something visible, its hotspot inside,
    /// and the seam and the fades are not the same picture.
    #[test]
    fn the_glyphs_rasterize() {
        let ink = |g: Glyph| {
            let (rgba, side) = g.rgba(1.0);
            assert_eq!(rgba.len(), usize::from(side) * usize::from(side) * 4);
            rgba
        };
        let seam = ink(Glyph::Seam);
        let fade_in = ink(Glyph::FadeIn);
        let fade_out = ink(Glyph::FadeOut);
        let bracket = ink(Glyph::Editor(Cursor::EdgeLeft));
        for picture in [&seam, &fade_in, &fade_out, &bracket] {
            let opaque = picture.chunks_exact(4).filter(|p| p[3] > 200).count();
            assert!(opaque > 20, "drawn: {opaque} opaque pixels");
        }
        assert_ne!(fade_in, fade_out, "the fades face opposite ways");
        assert_ne!(seam, bracket);
    }

    /// Where no glyph can be made, the nearest system shape stands in.
    #[test]
    fn a_glyph_falls_back_to_the_nearest_system_shape() {
        assert_eq!(
            Shape::Glyph(Glyph::Editor(Cursor::EdgeRight)).icon(),
            CursorIcon::EResize
        );
        assert_eq!(
            Shape::Glyph(Glyph::Editor(Cursor::Zoom)).icon(),
            CursorIcon::ZoomIn
        );
        assert_eq!(Shape::Glyph(Glyph::Seam).icon(), CursorIcon::ColResize);
    }
}
