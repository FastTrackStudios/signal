//! frame's fonts.
//!
//! A [`FontBook`] holds font families — variable or static, several faces
//! each — and picks the face a text style asks for: by family name, weight,
//! italic and width ([`FontQuery`]), falling back through a chain of
//! families when the named one is missing or lacks a glyph.
//!
//! Variable faces are instanced on their axes (`wght`, `wdth`, and `ital`
//! or `slnt` for italics); a style a family can't draw is synthesised — an
//! oblique skew for a missing italic, an outline expansion for a missing
//! bold — the way a browser does.
//!
//! [`FontBook::bundled`] is the default book: every family in
//! [`BUNDLED`] embedded in the binary, with `Inter` as the fallback. Hosts
//! clone it and [`register`](FontBook::register) more; a document carries
//! its own fonts as `font/*` assets, which the renderer registers over the
//! host's book for that document.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};

use kurbo::{Affine, BezPath};
use skrifa::attribute::{Attributes, Style};
use skrifa::instance::{Location, LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::raw::FileRef;
use skrifa::string::StringId;
use skrifa::{FontRef, GlyphId, MetadataProvider, Tag};

mod bundled;
pub use bundled::{BUNDLED, BundledFamily, INTER, INTER_ITALIC};

/// The shear of a synthetic oblique (x += `SKEW` · height above baseline).
pub const SYNTHETIC_SKEW: f64 = 0.22;

/// How far a synthetic bold grows each side of a glyph, per px of size.
pub const SYNTHETIC_BOLD: f64 = 0.024;

/// The family every lookup falls back to in the bundled book.
pub const DEFAULT_FAMILY: &str = "Inter";

// ── bytes ────────────────────────────────────────────────────────────────

/// A font file's bytes: embedded, or shared (registered at run time).
#[derive(Clone)]
pub enum FontBytes {
    Static(&'static [u8]),
    Shared(Arc<[u8]>),
}

impl AsRef<[u8]> for FontBytes {
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Static(b) => b,
            Self::Shared(b) => b,
        }
    }
}

impl From<&'static [u8]> for FontBytes {
    fn from(b: &'static [u8]) -> Self {
        Self::Static(b)
    }
}

impl From<Vec<u8>> for FontBytes {
    fn from(b: Vec<u8>) -> Self {
        Self::Shared(b.into())
    }
}

impl From<Arc<[u8]>> for FontBytes {
    fn from(b: Arc<[u8]>) -> Self {
        Self::Shared(b)
    }
}

impl std::fmt::Debug for FontBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FontBytes({} bytes)", self.as_ref().len())
    }
}

// ── faces ────────────────────────────────────────────────────────────────

/// A variation axis: `(min, default, max)` in user units.
pub type AxisRange = (f32, f32, f32);

/// One face of a family: a font in a file, with the styles it covers.
#[derive(Debug)]
pub struct Face {
    /// The family it belongs to (the font's typographic family name, or the
    /// name it was registered under).
    pub family: String,
    bytes: FontBytes,
    /// Index in a collection (0 for a plain font file).
    pub index: u32,
    /// Weights it draws: `(min, max)`, equal for a static face.
    pub weight: (f32, f32),
    /// Widths it draws, percent of normal: `(min, max)`.
    pub width: (f32, f32),
    /// An italic (or oblique) face.
    pub italic: bool,
    pub wght: Option<AxisRange>,
    pub wdth: Option<AxisRange>,
    pub slnt: Option<AxisRange>,
    pub ital: Option<AxisRange>,
    id: u64,
}

static NEXT_FACE: AtomicU64 = AtomicU64::new(1);

fn axis(font: &FontRef<'_>, tag: &[u8; 4]) -> Option<AxisRange> {
    let tag = Tag::new(tag);
    font.axes()
        .iter()
        .find(|a| a.tag() == tag)
        .map(|a| (a.min_value(), a.default_value(), a.max_value()))
}

fn name(font: &FontRef<'_>, id: StringId) -> Option<String> {
    let s = font.localized_strings(id).english_or_first()?.to_string();
    (!s.trim().is_empty()).then_some(s)
}

impl Face {
    /// Read face `index` of `bytes`; `family` overrides the font's own name.
    #[must_use]
    pub fn read(bytes: FontBytes, index: u32, family: Option<&str>) -> Option<Self> {
        let font = font_ref(bytes.as_ref(), index)?;
        let family = match family {
            Some(f) => f.to_string(),
            None => name(&font, StringId::TYPOGRAPHIC_FAMILY_NAME)
                .or_else(|| name(&font, StringId::FAMILY_NAME))?,
        };
        let attrs = Attributes::new(&font);
        let (wght, wdth, slnt, ital) = (
            axis(&font, b"wght"),
            axis(&font, b"wdth"),
            axis(&font, b"slnt"),
            axis(&font, b"ital"),
        );
        let w = attrs.weight.value();
        let weight = wght.map_or((w, w), |(lo, _, hi)| (lo, hi));
        let s = attrs.stretch.percentage();
        let width = wdth.map_or((s, s), |(lo, _, hi)| (lo, hi));
        let italic =
            !matches!(attrs.style, Style::Normal) || ital.is_some_and(|(_, d, _)| d >= 0.5);
        Some(Self {
            family,
            bytes,
            index,
            weight,
            width,
            italic,
            wght,
            wdth,
            slnt,
            ital,
            id: NEXT_FACE.fetch_add(1, Ordering::Relaxed),
        })
    }

    /// The font file's bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        self.bytes.as_ref()
    }

    /// The shared bytes (cheap to clone).
    #[must_use]
    pub fn font_bytes(&self) -> &FontBytes {
        &self.bytes
    }

    /// A process-unique id, for caches keyed by face.
    #[must_use]
    pub fn id(&self) -> u64 {
        self.id
    }

    #[must_use]
    pub fn font(&self) -> Option<FontRef<'_>> {
        font_ref(self.bytes.as_ref(), self.index)
    }

    /// Whether it can draw italics itself (an italic face, or an `ital` /
    /// `slnt` axis).
    #[must_use]
    pub fn can_italic(&self) -> bool {
        self.italic || self.ital.is_some() || self.slnt.is_some_and(|(lo, _, _)| lo < 0.0)
    }
}

fn font_ref(bytes: &[u8], index: u32) -> Option<FontRef<'_>> {
    match FileRef::new(bytes).ok()? {
        FileRef::Font(f) => Some(f),
        FileRef::Collection(c) => c.get(index).ok(),
    }
}

// ── queries ──────────────────────────────────────────────────────────────

/// What a text style asks for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontQuery<'a> {
    /// Family name (case, spaces and hyphens don't matter), a generic name
    /// (`sans-serif`, `serif`, `monospace`, `cursive`, `fantasy`,
    /// `system-ui`) or an alias; `None` = the book's first fallback.
    pub family: Option<&'a str>,
    /// 1–1000; 400 regular, 700 bold.
    pub weight: f32,
    pub italic: bool,
    /// Percent of normal width (CSS `font-stretch`): 75 condensed, 125
    /// expanded.
    pub width: f32,
}

impl Default for FontQuery<'_> {
    fn default() -> Self {
        Self {
            family: None,
            weight: 400.0,
            italic: false,
            width: 100.0,
        }
    }
}

impl<'a> FontQuery<'a> {
    #[must_use]
    pub fn family(family: &'a str) -> Self {
        Self {
            family: Some(family),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn weight(mut self, weight: f32) -> Self {
        self.weight = weight;
        self
    }

    #[must_use]
    pub fn italic(mut self, italic: bool) -> Self {
        self.italic = italic;
        self
    }

    #[must_use]
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }
}

/// A text style as a document states it: every field optional, the style
/// name free-form (`"Bold Italic"`, `"Condensed Light"`, `"italic"`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FontSpec {
    pub family: Option<String>,
    pub weight: Option<f64>,
    pub style: Option<String>,
    pub width: Option<f64>,
}

impl FontSpec {
    /// This spec with `base` filling the fields it leaves unset (a run over
    /// its node). A run's style name only overrides what it names: its
    /// weight words set the weight unless the base states one explicitly.
    #[must_use]
    pub fn over(&self, base: &FontSpec) -> FontSpec {
        let own = self.style.as_deref().map(parse_style).unwrap_or_default();
        let weight = self.weight.or(own.weight.map(f64::from)).or(base.weight);
        let width = self.width.or(own.width.map(f64::from)).or(base.width);
        let style = match (&self.style, &base.style) {
            (Some(s), Some(b)) if !own.italic && parse_style(b).italic => {
                Some(format!("{s} italic"))
            }
            (Some(s), _) => Some(s.clone()),
            (None, b) => b.clone(),
        };
        FontSpec {
            family: self.family.clone().or_else(|| base.family.clone()),
            weight,
            style,
            width,
        }
    }

    /// The query this spec makes (unset: regular, upright, normal width).
    #[must_use]
    pub fn query(&self) -> FontQuery<'_> {
        let hints = self.style.as_deref().map(parse_style).unwrap_or_default();
        FontQuery {
            family: self.family.as_deref(),
            weight: self
                .weight
                .map(|w| w as f32)
                .or(hints.weight)
                .unwrap_or(400.0),
            italic: hints.italic,
            width: self
                .width
                .map(|w| w as f32)
                .or(hints.width)
                .unwrap_or(100.0),
        }
    }
}

/// What a style name says.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StyleHints {
    pub weight: Option<f32>,
    pub italic: bool,
    pub width: Option<f32>,
}

/// Read a style name — Figma's `"Semi Bold Italic"`, CSS-ish `"italic"`,
/// `"condensed bold"` — into weight, italic and width.
#[must_use]
pub fn parse_style(style: &str) -> StyleHints {
    let s: String = style
        .to_ascii_lowercase()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect();
    const WIDTHS: &[(&str, f32)] = &[
        ("ultracondensed", 50.0),
        ("extracondensed", 62.5),
        ("semicondensed", 87.5),
        ("condensed", 75.0),
        ("narrow", 75.0),
        ("compressed", 62.5),
        ("ultraexpanded", 200.0),
        ("extraexpanded", 150.0),
        ("semiexpanded", 112.5),
        ("expanded", 125.0),
        ("extended", 125.0),
        ("wide", 125.0),
    ];
    const WEIGHTS: &[(&str, f32)] = &[
        ("hairline", 100.0),
        ("thin", 100.0),
        ("extralight", 200.0),
        ("ultralight", 200.0),
        ("semilight", 350.0),
        ("light", 300.0),
        ("regular", 400.0),
        ("normal", 400.0),
        ("book", 400.0),
        ("medium", 500.0),
        ("semibold", 600.0),
        ("demibold", 600.0),
        ("extrabold", 800.0),
        ("ultrabold", 800.0),
        ("bold", 700.0),
        ("extrablack", 950.0),
        ("ultrablack", 950.0),
        ("black", 900.0),
        ("heavy", 900.0),
    ];
    let mut rest = s.clone();
    let mut width = None;
    for (k, v) in WIDTHS {
        if rest.contains(k) {
            width = Some(*v);
            rest = rest.replacen(k, "", 1);
            break;
        }
    }
    let weight = WEIGHTS
        .iter()
        .find(|(k, _)| rest.contains(k))
        .map(|(_, v)| *v);
    StyleHints {
        weight,
        italic: s.contains("italic") || s.contains("oblique"),
        width,
    }
}

/// Fold a family name for matching: lower case, letters and digits only.
#[must_use]
pub fn fold(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

// ── the book ─────────────────────────────────────────────────────────────

/// Font families, looked up by name, weight, italic and width.
///
/// Cheap to clone (faces are shared). Faces registered later win ties, so a
/// document's own `Inter` overrides the bundled one.
#[derive(Debug, Clone, Default)]
pub struct FontBook {
    faces: Vec<Arc<Face>>,
    fallback: Vec<String>,
    aliases: Vec<(String, String)>,
}

static BUNDLED_BOOK: LazyLock<FontBook> = LazyLock::new(|| {
    let mut book = FontBook::new();
    for fam in BUNDLED {
        for bytes in fam.files {
            book.register_as(Some(fam.name), FontBytes::Static(bytes));
        }
    }
    for (alias, family) in [
        ("sans-serif", "Inter"),
        ("sans", "Inter"),
        ("system-ui", "Inter"),
        ("ui", "Inter"),
        ("serif", "Playfair Display"),
        ("monospace", "JetBrains Mono"),
        ("mono", "JetBrains Mono"),
        ("cursive", "Dancing Script"),
        ("script", "Dancing Script"),
        ("fantasy", "Pirata One"),
        ("blackletter", "UnifrakturCook"),
        ("condensed", "Oswald"),
        ("DSEG7", "DSEG7 Classic"),
        ("DSEG14", "DSEG14 Classic"),
        ("seven-segment", "DSEG7 Classic"),
        ("fourteen-segment", "DSEG14 Classic"),
        ("lcd", "DSEG7 Classic"),
        ("dot-matrix", "Doto"),
        ("marker", "Caveat Brush"),
    ] {
        book.alias(alias, family);
    }
    book.set_fallback([DEFAULT_FAMILY]);
    book
});

impl FontBook {
    /// An empty book.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The default book: every [`BUNDLED`] family, `Inter` the fallback.
    #[must_use]
    pub fn bundled() -> &'static FontBook {
        &BUNDLED_BOOK
    }

    /// A book of one font file (every face in it), which is also the
    /// fallback — the single-font setup hosts had before the book.
    #[must_use]
    pub fn single(bytes: impl Into<FontBytes>) -> Self {
        let mut book = Self::new();
        let families = book.register(bytes);
        book.set_fallback(families);
        book
    }

    /// Register every face in a font file (or collection) under its own
    /// family name; returns the families added.
    pub fn register(&mut self, bytes: impl Into<FontBytes>) -> Vec<String> {
        self.register_as(None, bytes.into())
    }

    /// Register a font file's faces under `family` (or their own names).
    pub fn register_as(&mut self, family: Option<&str>, bytes: FontBytes) -> Vec<String> {
        let count = match FileRef::new(bytes.as_ref()) {
            Ok(FileRef::Collection(c)) => c.len(),
            Ok(FileRef::Font(_)) => 1,
            Err(_) => 0,
        };
        let mut added = Vec::new();
        for index in 0..count {
            if let Some(face) = Face::read(bytes.clone(), index, family) {
                if !added.contains(&face.family) {
                    added.push(face.family.clone());
                }
                self.faces.push(Arc::new(face));
            }
        }
        added
    }

    /// Add an already-read face (shared with another book).
    pub fn insert_face(&mut self, face: Arc<Face>) {
        self.faces.push(face);
    }

    /// Let `name` stand for `family`.
    pub fn alias(&mut self, name: &str, family: &str) {
        let key = fold(name);
        self.aliases.retain(|(a, _)| *a != key);
        self.aliases.push((key, family.to_string()));
    }

    /// Families tried, in order, when a lookup's family is missing or lacks
    /// a glyph.
    pub fn set_fallback<S: Into<String>>(&mut self, families: impl IntoIterator<Item = S>) {
        self.fallback = families.into_iter().map(Into::into).collect();
    }

    #[must_use]
    pub fn fallback(&self) -> &[String] {
        &self.fallback
    }

    #[must_use]
    pub fn faces(&self) -> &[Arc<Face>] {
        &self.faces
    }

    /// Every family, in registration order.
    #[must_use]
    pub fn families(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for f in &self.faces {
            if !out.iter().any(|o| fold(o) == fold(&f.family)) {
                out.push(&f.family);
            }
        }
        out
    }

    /// The family `name` (or its alias) names, when the book has it.
    #[must_use]
    pub fn family_name(&self, name: &str) -> Option<&str> {
        let key = fold(name);
        let key = self
            .aliases
            .iter()
            .rev()
            .find(|(a, _)| *a == key)
            .map_or(key, |(_, f)| fold(f));
        self.faces
            .iter()
            .rev()
            .find(|f| fold(&f.family) == key)
            .map(|f| f.family.as_str())
    }

    #[must_use]
    pub fn has_family(&self, name: &str) -> bool {
        self.family_name(name).is_some()
    }

    /// The best face of `family` for `q`, instanced.
    fn best_in(&self, family: &str, q: &FontQuery<'_>) -> Option<Resolved> {
        let key = fold(family);
        let mut best: Option<(&Arc<Face>, Score)> = None;
        for face in self.faces.iter().rev().filter(|f| fold(&f.family) == key) {
            let s = score(face, q);
            if best.as_ref().is_none_or(|(_, b)| s < *b) {
                best = Some((face, s));
            }
        }
        best.map(|(face, _)| Resolved::new(face.clone(), q))
    }

    /// The face `q` gets: its family's best match, else the first fallback
    /// family's, else any face.
    #[must_use]
    pub fn resolve(&self, q: &FontQuery<'_>) -> Option<Resolved> {
        q.family
            .and_then(|f| self.family_name(f))
            .and_then(|f| self.best_in(f, q))
            .or_else(|| self.fallback.iter().find_map(|f| self.best_in(f, q)))
            .or_else(|| self.faces.last().map(|f| Resolved::new(f.clone(), q)))
    }

    /// The faces tried for each character: the resolved face, then each
    /// fallback family (instanced for the same style), no face twice.
    #[must_use]
    pub fn chain(&self, q: &FontQuery<'_>) -> Chain {
        let mut faces: Vec<Resolved> = Vec::new();
        let mut push = |r: Option<Resolved>| {
            if let Some(r) = r
                && !faces.iter().any(|f| f.face.id == r.face.id)
            {
                faces.push(r);
            }
        };
        push(self.resolve(q));
        for f in &self.fallback {
            push(self.best_in(f, q));
        }
        Chain { faces }
    }
}

/// (italic mismatch, width distance, weight distance); lower is better.
type Score = (u8, f32, f32);

/// How well `face` matches `q`, lower is better: (italic mismatch, width
/// distance, weight distance — CSS font matching's order and direction).
fn score(face: &Face, q: &FontQuery<'_>) -> Score {
    // A real italic beats an italic axis beats a synthetic slant.
    let italic = match (q.italic, face.italic) {
        (true, true) | (false, false) => 0,
        (true, false) if face.can_italic() => 1,
        _ => 2,
    };
    let (wlo, whi) = face.width;
    let width = if q.width < wlo {
        (wlo - q.width) + if q.width <= 100.0 { 1000.0 } else { 0.0 }
    } else if q.width > whi {
        (q.width - whi) + if q.width > 100.0 { 1000.0 } else { 0.0 }
    } else {
        0.0
    };
    let (lo, hi) = face.weight;
    let w = q.weight;
    let weight = if (lo..=hi).contains(&w) {
        0.0
    } else if w > 500.0 {
        if lo > w { lo - w } else { 1000.0 + (w - hi) }
    } else if w < 400.0 {
        if hi < w { w - hi } else { 1000.0 + (lo - w) }
    } else if lo > w && lo <= 500.0 {
        lo - w
    } else if hi < w {
        500.0 + (w - hi)
    } else {
        1000.0 + (lo - w)
    };
    (italic, width, weight)
}

// ── resolved faces ───────────────────────────────────────────────────────

/// A face instanced for a query: its variation location and whatever has
/// to be synthesised.
#[derive(Debug, Clone)]
pub struct Resolved {
    pub face: Arc<Face>,
    location: Location,
    /// Normalized design coordinates (F2Dot14 bits, fvar order) — what a
    /// glyph-run API takes. Empty for the default instance.
    pub coords: Vec<i16>,
    /// Draw it slanted: the query wanted italic and the face has none.
    pub synthetic_italic: bool,
    /// Embolden it: the query wanted ≥ 200 more weight than the face has.
    pub synthetic_bold: bool,
}

impl Resolved {
    fn new(face: Arc<Face>, q: &FontQuery<'_>) -> Self {
        let mut settings: Vec<(&str, f32)> = Vec::new();
        if let Some((lo, _, hi)) = face.wght {
            settings.push(("wght", q.weight.clamp(lo, hi)));
        }
        if let Some((lo, _, hi)) = face.wdth {
            settings.push(("wdth", q.width.clamp(lo, hi)));
        }
        let mut synthetic_italic = false;
        if q.italic && !face.italic {
            if let Some((lo, _, hi)) = face.ital {
                settings.push(("ital", hi.max(lo)));
            } else if let Some((lo, _, _)) = face.slnt.filter(|(lo, _, _)| *lo < 0.0) {
                settings.push(("slnt", lo.max(-12.0)));
            } else {
                synthetic_italic = true;
            }
        }
        let location = face.font().map_or_else(Location::default, |f| {
            f.axes().location(settings.iter().copied())
        });
        let coords: Vec<i16> = location.coords().iter().map(|c| c.to_bits()).collect();
        let coords = if coords.iter().all(|c| *c == 0) {
            Vec::new()
        } else {
            coords
        };
        let synthetic_bold = q.weight >= 600.0 && face.weight.1 + 200.0 <= q.weight;
        Self {
            face,
            location,
            coords,
            synthetic_italic,
            synthetic_bold,
        }
    }

    #[must_use]
    pub fn font(&self) -> Option<FontRef<'_>> {
        self.face.font()
    }

    #[must_use]
    pub fn location(&self) -> LocationRef<'_> {
        LocationRef::from(&self.location)
    }

    /// The glyph for `ch`, when the face has one.
    #[must_use]
    pub fn glyph(&self, ch: char) -> Option<GlyphId> {
        self.font()?.charmap().map(ch).filter(|g| g.to_u32() != 0)
    }

    /// The synthetic-bold growth each side at `size`, px (0 without).
    #[must_use]
    pub fn embolden(&self, size: f64) -> f64 {
        if self.synthetic_bold {
            SYNTHETIC_BOLD * size
        } else {
            0.0
        }
    }

    /// `gid`'s advance at `size` px (a synthetic bold is that much wider).
    #[must_use]
    pub fn advance(&self, gid: GlyphId, size: f64) -> f64 {
        let Some(font) = self.font() else {
            return size * 0.6;
        };
        let gm = font.glyph_metrics(Size::new(size.max(1.0) as f32), self.location());
        f64::from(gm.advance_width(gid).unwrap_or_default()) + 2.0 * self.embolden(size)
    }

    /// Ascent at `size`, px.
    #[must_use]
    pub fn ascent(&self, size: f64) -> f64 {
        self.font().map_or(size * 0.8, |f| {
            f64::from(
                f.metrics(Size::new(size.max(1.0) as f32), self.location())
                    .ascent,
            )
        })
    }

    /// Descent at `size`, px (negative: below the baseline).
    #[must_use]
    pub fn descent(&self, size: f64) -> f64 {
        self.font().map_or(-size * 0.2, |f| {
            f64::from(
                f.metrics(Size::new(size.max(1.0) as f32), self.location())
                    .descent,
            )
        })
    }

    /// The per-glyph transform a synthetic italic needs (y-down space).
    #[must_use]
    pub fn skew(&self) -> Affine {
        if self.synthetic_italic {
            Affine::new([1.0, 0.0, -SYNTHETIC_SKEW, 1.0, 0.0, 0.0])
        } else {
            Affine::IDENTITY
        }
    }

    /// `gid`'s outline at `size` px, y down, pen at the origin on the
    /// baseline — instanced, slanted and emboldened as drawn.
    #[must_use]
    pub fn outline(&self, gid: GlyphId, size: f64) -> Option<BezPath> {
        let font = self.font()?;
        let glyph = font.outline_glyphs().get(gid)?;
        let mut pen = Pen(BezPath::new());
        glyph
            .draw(
                DrawSettings::unhinted(Size::new(size.max(1.0) as f32), self.location()),
                &mut pen,
            )
            .ok()?;
        let mut path = pen.0;
        let grow = self.embolden(size);
        if grow > 0.0 {
            path = kurbo::expand_path(
                &path,
                kurbo::Diagonal2::new(grow, grow * 0.5),
                kurbo::Join::Round,
                4.0,
                0.05,
            );
            path.apply_affine(Affine::translate((grow, 0.0)));
        }
        if self.synthetic_italic {
            path.apply_affine(self.skew());
        }
        Some(path)
    }
}

struct Pen(BezPath);

impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to((f64::from(x), -f64::from(y)));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to((f64::from(x), -f64::from(y)));
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.quad_to(
            (f64::from(cx0), -f64::from(cy0)),
            (f64::from(x), -f64::from(y)),
        );
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.curve_to(
            (f64::from(cx0), -f64::from(cy0)),
            (f64::from(cx1), -f64::from(cy1)),
            (f64::from(x), -f64::from(y)),
        );
    }
    fn close(&mut self) {
        self.0.close_path();
    }
}

/// The faces a style tries, in order (see [`FontBook::chain`]).
#[derive(Debug, Clone, Default)]
pub struct Chain {
    pub faces: Vec<Resolved>,
}

impl Chain {
    /// The face the style resolved to.
    #[must_use]
    pub fn primary(&self) -> Option<&Resolved> {
        self.faces.first()
    }

    /// The first face with a glyph for `ch` and that glyph; the primary
    /// face's missing glyph when none has one.
    #[must_use]
    pub fn pick(&self, ch: char) -> Option<(&Resolved, GlyphId)> {
        self.faces
            .iter()
            .find_map(|f| f.glyph(ch).map(|g| (f, g)))
            .or_else(|| self.primary().map(|f| (f, GlyphId::new(0))))
    }

    /// How wide `text` sets at `size` px with `spacing` px after each
    /// character (newlines skipped).
    #[must_use]
    pub fn advance(&self, text: &str, size: f64, spacing: f64) -> f64 {
        text.chars()
            .filter(|c| *c != '\n')
            .map(|c| self.pick(c).map_or(size * 0.6, |(f, g)| f.advance(g, size)) + spacing)
            .sum()
    }

    /// One line of `text` as a single path, pen from the origin on the
    /// baseline; returns the path and its advance.
    #[must_use]
    pub fn line(&self, text: &str, size: f64, spacing: f64) -> (BezPath, f64) {
        let mut out = BezPath::new();
        let mut x = 0.0;
        for c in text.chars().filter(|c| *c != '\n') {
            let Some((face, gid)) = self.pick(c) else {
                continue;
            };
            if let Some(p) = face.outline(gid, size) {
                out.extend((Affine::translate((x, 0.0)) * p).elements().iter().copied());
            }
            x += face.advance(gid, size) + spacing;
        }
        (out, x)
    }
}

#[cfg(test)]
mod tests;
