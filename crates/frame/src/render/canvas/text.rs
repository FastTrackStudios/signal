//! Text as outlines.
//!
//! A host scene may have no text of its own (a LICE bitmap, a CPU
//! rasteriser without a glyph cache), and a glyph run's font handle does
//! not survive being replayed into a different anyrender version. Paths
//! do: every glyph is drawn as its outline — from the face frame's
//! [`FontBook`] resolves for the text's family, weight, italic and width,
//! instanced on its axes — cached per (face, glyph, size, instance), so an
//! embed's labels cost a few fills and replay anywhere.

use std::borrow::Cow;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use crate::render::fonts::Resolved;
pub use crate::render::fonts::{FontBook, FontSpec};
use kurbo::{Affine, BezPath};

/// frame's UI face (Inter, variable). Hosts may hand in their own book.
pub static INTER: &[u8] = crate::render::font_book::INTER;

type Cache = Mutex<HashMap<(u64, u32, u32, u64), Arc<BezPath>>>;

/// A font book and its outline cache.
#[derive(Clone)]
pub struct Font {
    book: Cow<'static, FontBook>,
    cache: Arc<Cache>,
}

impl std::fmt::Debug for Font {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Font")
            .field("families", &self.book.families())
            .finish()
    }
}

impl Default for Font {
    /// frame's bundled book.
    fn default() -> Self {
        Self::from_book(Cow::Borrowed(FontBook::bundled()))
    }
}

/// One line laid out: each glyph's outline, already placed.
#[derive(Debug, Clone, Default)]
pub struct Line {
    pub path: BezPath,
    pub width: f64,
}

fn spec_of(weight: f64) -> FontSpec {
    FontSpec {
        weight: Some(weight),
        ..FontSpec::default()
    }
}

impl Font {
    /// One font file (every face in it) as the whole book.
    #[must_use]
    pub fn new(bytes: Vec<u8>, _index: u32) -> Self {
        Self::from_book(Cow::Owned(FontBook::single(bytes)))
    }

    #[must_use]
    pub fn from_book(book: Cow<'static, FontBook>) -> Self {
        Self {
            book,
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    #[must_use]
    pub fn book(&self) -> &FontBook {
        &self.book
    }

    /// This font with the fonts `scene` carries registered over it (the
    /// outline cache is shared).
    #[must_use]
    pub fn for_scene(&self, scene: &crate::document::proto::Scene) -> Cow<'_, Font> {
        match crate::render::book_for_scene(&self.book, scene) {
            Cow::Borrowed(_) => Cow::Borrowed(self),
            Cow::Owned(book) => Cow::Owned(Self {
                book: Cow::Owned(book),
                cache: self.cache.clone(),
            }),
        }
    }

    /// Ascent of regular text at `size`, px.
    #[must_use]
    pub fn ascent(&self, size: f64) -> f64 {
        self.ascent_of(&FontSpec::default(), size)
    }

    /// Ascent of `spec` at `size`, px.
    #[must_use]
    pub fn ascent_of(&self, spec: &FontSpec, size: f64) -> f64 {
        self.book
            .resolve(&spec.query())
            .map_or(size * 0.8, |r| r.ascent(size))
    }

    /// How wide regular `text` sets at `size` with `spacing` px between
    /// letters.
    #[must_use]
    pub fn width(&self, text: &str, size: f64, spacing: f64) -> f64 {
        self.width_of(&FontSpec::default(), text, size, spacing)
    }

    /// How wide `text` sets in `spec`.
    #[must_use]
    pub fn width_of(&self, spec: &FontSpec, text: &str, size: f64, spacing: f64) -> f64 {
        crate::render::text_advance(&self.book, spec, text, size, spacing)
    }

    /// `text` at `weight` as one path, its pen starting at the origin on
    /// the baseline.
    #[must_use]
    pub fn line(&self, text: &str, size: f64, spacing: f64, weight: f64) -> Line {
        self.line_of(&spec_of(weight), text, size, spacing)
    }

    /// `text` set in `spec` as one path, pen from the origin on the
    /// baseline.
    #[must_use]
    pub fn line_of(&self, spec: &FontSpec, text: &str, size: f64, spacing: f64) -> Line {
        let chain = self.book.chain(&spec.query());
        let mut path = BezPath::new();
        let mut pen_x = 0.0;
        for ch in text.chars().filter(|c| *c != '\n') {
            let Some((face, gid)) = chain.pick(ch) else {
                continue;
            };
            if let Some(glyph) = self.glyph(face, gid, size) {
                path.extend(
                    (Affine::translate((pen_x, 0.0)) * (*glyph).clone())
                        .elements()
                        .iter()
                        .copied(),
                );
            }
            pen_x += face.advance(gid, size) + spacing;
        }
        Line { path, width: pen_x }
    }

    fn glyph(&self, face: &Resolved, gid: skrifa::GlyphId, size: f64) -> Option<Arc<BezPath>> {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        face.coords.hash(&mut h);
        (face.synthetic_bold, face.synthetic_italic).hash(&mut h);
        let key = (
            face.face.id(),
            gid.to_u32(),
            (size * 64.0).round() as u32,
            h.finish(),
        );
        if let Some(hit) = self.cache.lock().ok()?.get(&key) {
            return Some(hit.clone());
        }
        let path = Arc::new(face.outline(gid, size)?);
        if let Ok(mut cache) = self.cache.lock() {
            if cache.len() > 4096 {
                cache.clear();
            }
            cache.insert(key, path.clone());
        }
        Some(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyphs_have_outlines_and_widths() {
        let font = Font::default();
        let line = font.line("EQ 4 · 5.5dB", 7.0, 0.0, 400.0);
        assert!(!line.path.elements().is_empty());
        assert!((line.width - font.width("EQ 4 · 5.5dB", 7.0, 0.0)).abs() < 1e-6);
        assert!(font.width("COMP", 7.0, 0.0) > font.width("EQ", 7.0, 0.0));
        assert!(font.ascent(10.0) > 6.0);
    }

    #[test]
    fn family_and_weight_reach_the_outlines() {
        use kurbo::Shape;
        let font = Font::default();
        let ink = |spec: &FontSpec| font.line_of(spec, "HOT", 40.0, 0.0).path.area().abs();
        let regular = ink(&FontSpec::default());
        let black = ink(&spec_of(900.0));
        assert!(black > regular * 1.4, "black {black} vs regular {regular}");
        let lcd = FontSpec {
            family: Some("DSEG14 Classic".into()),
            ..FontSpec::default()
        };
        assert!(
            (ink(&lcd) - regular).abs() > 1.0,
            "a different family draws different glyphs"
        );
    }
}
