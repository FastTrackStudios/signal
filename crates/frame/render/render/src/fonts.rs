//! Fonts for rendering: the host's [`FontBook`], plus the fonts a
//! document carries as `font/*` assets.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use document_proto::Scene;
pub use render_fonts::{
    BUNDLED, BundledFamily, Chain, DEFAULT_FAMILY, Face, FontBook, FontBytes, FontQuery, FontSpec,
    Resolved, StyleHints, parse_style,
};

pub use crate::anyrender_renderer::text_font;

/// Whether an asset's mime type is a font.
#[must_use]
pub fn is_font_mime(mime: &str) -> bool {
    let m = mime.to_ascii_lowercase();
    m.starts_with("font/")
        || matches!(
            m.as_str(),
            "application/font-sfnt"
                | "application/x-font-ttf"
                | "application/x-font-otf"
                | "application/vnd.ms-opentype"
        )
}

/// Faces read from document assets, by asset id — read once, so their ids
/// (and every glyph cache keyed on them) stay put frame to frame.
type Faces = Arc<Vec<Arc<Face>>>;

static DOC_FACES: std::sync::LazyLock<Mutex<HashMap<String, Faces>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

fn doc_faces(id: &str, bytes: &[u8]) -> Faces {
    if let Some(hit) = DOC_FACES.lock().ok().and_then(|c| c.get(id).cloned()) {
        return hit;
    }
    let mut book = FontBook::new();
    book.register(bytes.to_vec());
    let faces = Arc::new(book.faces().to_vec());
    if let Ok(mut c) = DOC_FACES.lock() {
        if c.len() > 256 {
            c.clear();
        }
        c.insert(id.to_string(), faces.clone());
    }
    faces
}

/// `base` with the fonts `scene` carries (its `font/*` assets) registered
/// over it — a shared surface brings its own lettering. Borrowed as is
/// when the document has none.
#[must_use]
pub fn book_for_scene<'a>(base: &'a FontBook, scene: &Scene) -> Cow<'a, FontBook> {
    let mut fonts = scene
        .assets
        .values()
        .filter(|a| is_font_mime(&a.mime))
        .peekable();
    if fonts.peek().is_none() {
        return Cow::Borrowed(base);
    }
    let mut book = base.clone();
    for asset in fonts {
        for face in doc_faces(&asset.id.0, &asset.bytes).iter() {
            book.insert_face(face.clone());
        }
    }
    Cow::Owned(book)
}

/// The advance width of `text` set in `font` at `size` px with
/// `letter_spacing` px after each character, measured the way the painter
/// sets it (the resolved face's instance, per-glyph fallback, synthetic
/// bold) — so a host puts a caret exactly where the painted glyphs end.
#[must_use]
pub fn text_advance(
    fonts: &FontBook,
    font: &FontSpec,
    text: &str,
    size: f64,
    letter_spacing: f64,
) -> f64 {
    fonts
        .chain(&font.query())
        .advance(text, size.max(1.0), letter_spacing)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(family: Option<&str>, weight: Option<f64>) -> FontSpec {
        FontSpec {
            family: family.map(Into::into),
            weight,
            ..FontSpec::default()
        }
    }

    #[test]
    fn text_advance_respects_weight_family_and_width() {
        let book = FontBook::bundled();
        let regular = text_advance(book, &spec(None, None), "Gain 12 dB", 14.0, 0.0);
        let bold = text_advance(book, &spec(None, Some(900.0)), "Gain 12 dB", 14.0, 0.0);
        assert!(bold > regular + 1.0, "black {bold} vs regular {regular}");
        // A static face asked for bold is emboldened, and sets wider for it.
        assert!(
            text_advance(
                book,
                &spec(Some("Michroma"), Some(800.0)),
                "RACK",
                14.0,
                0.0
            ) > text_advance(book, &spec(Some("Michroma"), None), "RACK", 14.0, 0.0)
        );
        let mono = text_advance(book, &spec(Some("JetBrains Mono"), None), "iiii", 14.0, 0.0);
        let mono_w = text_advance(book, &spec(Some("JetBrains Mono"), None), "WWWW", 14.0, 0.0);
        assert!(
            (mono - mono_w).abs() < 1e-6,
            "monospace: {mono} vs {mono_w}"
        );
        let narrow = FontSpec {
            family: Some("Saira".into()),
            width: Some(62.5),
            ..FontSpec::default()
        };
        assert!(
            text_advance(book, &narrow, "RACK", 14.0, 0.0)
                < text_advance(book, &spec(Some("Saira"), None), "RACK", 14.0, 0.0)
        );
        // Letter spacing is added after every character.
        let spaced = text_advance(book, &spec(None, None), "Gain 12 dB", 14.0, 1.0);
        assert!((spaced - regular - 10.0).abs() < 1e-9);
    }

    #[test]
    fn documents_bring_their_own_fonts() {
        let mut scene = Scene::new("Page");
        let oswald = render_fonts::BUNDLED
            .iter()
            .find(|f| f.name == "Oswald")
            .unwrap()
            .files[0];
        let host = FontBook::single(render_fonts::INTER);
        assert!(!host.has_family("Oswald"));
        assert!(matches!(book_for_scene(&host, &scene), Cow::Borrowed(_)));
        // A document font under a family the host doesn't have.
        scene.add_asset("font/ttf", oswald.to_vec());
        let book = book_for_scene(&host, &scene);
        assert!(book.has_family("Oswald"));
        // Read once: the same faces (ids) every frame.
        let id = |b: &FontBook| b.resolve(&FontQuery::family("Oswald")).map(|r| r.face.id());
        assert_eq!(id(&book), id(&book_for_scene(&host, &scene)));
        assert!(
            is_font_mime("font/otf")
                && is_font_mime("application/font-sfnt")
                && !is_font_mime("image/png")
        );
    }
}
