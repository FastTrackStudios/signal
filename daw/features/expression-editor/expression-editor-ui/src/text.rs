//! Labels for the painted roll.
//!
//! The roll draws about fifty short strings: note names on bodies, `C4`
//! down the keyboard, bar numbers along the ruler, marker names. As DOM
//! text those were laid out by Blitz; painted into a scene they have to
//! be shaped here, which means a font and a shaper.
//!
//! ## Why the font is embedded
//!
//! `parley::FontContext::new()` reads the system's fonts, so the same
//! project would draw differently on a different machine and not at all
//! in a browser. The roll's labels are chrome — they must be identical
//! everywhere a shot is compared, and present at all on wasm — so one
//! font ships with the crate.
//!
//! ## Why shaping is cached
//!
//! Shaping is far more expensive than drawing, and these strings repeat:
//! the same twenty pitch names every frame, the same bar numbers while
//! you scroll sideways. Shaped runs are therefore keyed by text and size
//! and reused, so a scroll re-lays nothing out — it only moves glyphs
//! that were shaped once. This is the difference between a scene rebuild
//! costing microseconds and costing milliseconds.

use std::collections::HashMap;

use anyrender::{Glyph, PaintScene};
use kurbo::Affine;
use peniko::{Color, Fill, FontData};

/// The label font.
///
/// DejaVu Sans, already vendored for the REAPER embed. Chosen because it
/// is here, covers the accidentals (`♯`/`♭`) the pitch names use, and is
/// metrically stable across platforms.
const FONT: &[u8] = include_bytes!("../../../reaper/reaper-embed/fonts/DejaVuSans.ttf");

/// One shaped run: a font, and where each glyph sits relative to the
/// run's origin.
#[derive(Clone)]
struct Run {
    font: FontData,
    size: f32,
    glyphs: Vec<Glyph>,
}

/// A shaped string, ready to be drawn anywhere.
#[derive(Clone, Default)]
pub struct Shaped {
    runs: Vec<Run>,
    /// Advance width in pixels, for centring.
    pub width: f64,
    /// Distance from the run's origin up to the cap line, for centring
    /// vertically against a note body.
    pub ascent: f64,
}

/// Shapes strings, and remembers what it shaped.
pub struct Labeller {
    fonts: parley::FontContext,
    layout: parley::LayoutContext<()>,
    /// Keyed by text and by size in tenths of a pixel — the roll uses a
    /// handful of sizes and thousands of repeats of a few dozen strings.
    cache: HashMap<(String, u32), Shaped>,
}

impl Default for Labeller {
    fn default() -> Self {
        Self::new()
    }
}

impl Labeller {
    pub fn new() -> Self {
        let mut fonts = parley::FontContext::new();
        fonts.collection.register_fonts(FONT.to_vec().into(), None);
        Self {
            fonts,
            layout: parley::LayoutContext::new(),
            cache: HashMap::new(),
        }
    }

    /// Shape `text` at `size`, or hand back the last time it was shaped.
    pub fn shape(&mut self, text: &str, size: f32) -> Shaped {
        let key = (text.to_string(), (size * 10.0) as u32);
        if let Some(hit) = self.cache.get(&key) {
            return hit.clone();
        }
        let shaped = self.shape_uncached(text, size);
        self.cache.insert(key, shaped.clone());
        shaped
    }

    fn shape_uncached(&mut self, text: &str, size: f32) -> Shaped {
        let mut builder = self.layout.ranged_builder(&mut self.fonts, text, 1.0, true);
        builder.push_default(parley::StyleProperty::FontSize(size));
        let mut layout: parley::Layout<()> = builder.build(text);
        // One line: every label here is a word or two and must never
        // wrap — a wrapped bar number would be worse than a clipped one.
        layout.break_all_lines(None);

        let mut runs = Vec::new();
        let mut ascent = 0.0f64;
        for line in layout.lines() {
            ascent = ascent.max(line.metrics().ascent as f64);
            for item in line.items() {
                let parley::PositionedLayoutItem::GlyphRun(run) = item else {
                    continue;
                };
                let font = run.run().font().clone();
                runs.push(Run {
                    font,
                    size: run.run().font_size(),
                    glyphs: run
                        .positioned_glyphs()
                        .map(|g| Glyph {
                            id: g.id as u32,
                            x: g.x,
                            y: g.y,
                        })
                        .collect(),
                });
            }
        }
        Shaped {
            runs,
            width: layout.width() as f64,
            ascent,
        }
    }
}

/// How a label sits against the point it is given.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Align {
    /// `x` is the left edge.
    Left,
    /// `x` is the centre.
    Center,
    /// `x` is the right edge — for the keyboard's labels, which sit
    /// against the gutter's inner edge whatever their width.
    Right,
}

/// Draw a shaped label sitting on the baseline `y`.
///
/// Baseline, not top. Converting a top to a baseline means adding the
/// font's ascent, which includes internal leading and so places text
/// noticeably lower than the eye expects — on a seven-pixel note body
/// that put the whole label *below* the note, dark-on-dark and
/// invisible. Every caller here is positioning text against something
/// (a note body, a key, a ruler), which is a baseline question.
pub fn draw(
    scene: &mut impl PaintScene,
    shaped: &Shaped,
    x: f64,
    y: f64,
    align: Align,
    color: Color,
    at: Affine,
) {
    let x = match align {
        Align::Left => x,
        Align::Center => x - shaped.width * 0.5,
        Align::Right => x - shaped.width,
    };
    let origin = at * Affine::translate((x, y));
    for run in &shaped.runs {
        scene.draw_glyphs(
            &run.font,
            run.size,
            true,
            &[],
            kurbo::Vec2::default(),
            Fill::NonZero,
            &anyrender::Paint::from(color),
            1.0,
            origin,
            None,
            run.glyphs.iter().copied(),
        );
    }
}
