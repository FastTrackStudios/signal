//! Classify theme artwork, so components can be written from what the
//! originals actually are rather than from what a control "should" look
//! like.
//!
//!     cargo run -p daw-theme-art --example analyse -- <dir> [name-filter]
//!
//! Both components written so far were wrong the same way — they invented
//! modelling the source did not have. `mcp_bg` turned out to be a flat fill
//! tile and `mcp_volbg` a translucent hairline. Reading a few hundred
//! images one `magick txt:` dump at a time is not viable, so this reports
//! the shape of each one.

use std::collections::HashMap;

/// What kind of drawing an image is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    /// Nothing but markers and transparency.
    Empty,
    /// One opaque colour over the whole drawable area.
    Flat,
    /// One colour, translucent — an overlay that tints what is behind it.
    Overlay,
    /// A thin line or two: grooves, dividers, separators.
    Hairline,
    /// Varies along one axis only — a gradient strip.
    Gradient,
    /// Everything else: real artwork.
    Complex,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Flat => "flat",
            Self::Overlay => "overlay",
            Self::Hairline => "hairline",
            Self::Gradient => "gradient",
            Self::Complex => "complex",
        }
    }
}

const MARKERS: [[u8; 4]; 2] = [[255, 0, 255, 255], [255, 255, 0, 255]];

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args
        .next()
        .unwrap_or_else(|| "features/reaper/fts-theme/FastTrackStudio/.source-art".into());
    let filter = args.next().unwrap_or_default();

    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {dir}: {e}"))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "png"))
        .filter(|p| {
            filter.is_empty()
                || p.file_stem()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.contains(&filter))
        })
        .collect();
    entries.sort();

    let mut tally: HashMap<&str, usize> = HashMap::new();
    println!(
        "{:<32} {:>9} {:>6} {:>7} {:>8}  {:<9} sprite",
        "image", "size", "opaq%", "colours", "markers", "kind"
    );

    for path in &entries {
        let Ok(img) = image::open(path) else { continue };
        let img = img.to_rgba8();
        let name = path.file_stem().unwrap().to_string_lossy();

        let mut colours: HashMap<[u8; 4], usize> = HashMap::new();
        let mut markers = 0usize;
        let mut drawn = 0usize;
        for px in img.pixels() {
            if MARKERS.contains(&px.0) {
                markers += 1;
                continue;
            }
            if px.0[3] == 0 {
                continue;
            }
            drawn += 1;
            *colours.entry(px.0).or_default() += 1;
        }

        let total = (img.width() * img.height()) as usize - markers;
        let opaque_pct = if total == 0 {
            0.0
        } else {
            drawn as f32 * 100.0 / total as f32
        };
        let cells = sprite_cells(&img);
        let kind = classify(&img, &colours, drawn, markers);
        *tally.entry(kind.label()).or_default() += 1;

        println!(
            "{:<32} {:>4}x{:<4} {:>6.1} {:>7} {:>8}  {:<9} {}",
            name,
            img.width(),
            img.height(),
            opaque_pct,
            colours.len(),
            markers,
            kind.label(),
            if cells > 1 {
                format!("{cells} cells")
            } else {
                String::new()
            }
        );
    }

    println!("\n{} images", entries.len());
    let mut counts: Vec<_> = tally.into_iter().collect();
    counts.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (kind, n) in counts {
        println!("  {n:>4}  {kind}");
    }
}

/// How many equal-width cells this image is a strip of.
///
/// REAPER packs a control's states side by side — `mcp_solo_off` is three
/// 21x20 cells (normal, hover, pressed) in one 63x20 file. Missing that and
/// treating the file as a single button is how "complex, 125 colours"
/// happens for what is really one simple shape drawn three times.
///
/// Detected by testing whether the image is N horizontal repeats that
/// differ only in brightness — the states of one control, not N different
/// pictures.
fn sprite_cells(img: &image::RgbaImage) -> u32 {
    let (w, h) = (img.width(), img.height());
    for n in [4u32, 3, 2] {
        if w % n != 0 || w / n < 4 {
            continue;
        }
        let cw = w / n;
        // Compare alpha silhouettes: the states share a shape even when
        // their colours differ completely (solo off is grey, solo on amber).
        let same_shape = (0..h).all(|y| {
            (0..cw).all(|x| {
                let a0 = img.get_pixel(x, y).0[3] > 8;
                (1..n).all(|c| (img.get_pixel(x + c * cw, y).0[3] > 8) == a0)
            })
        });
        if same_shape {
            return n;
        }
    }
    1
}

fn classify(
    img: &image::RgbaImage,
    colours: &HashMap<[u8; 4], usize>,
    drawn: usize,
    _markers: usize,
) -> Kind {
    if drawn == 0 {
        return Kind::Empty;
    }
    if colours.len() == 1 {
        let (px, _) = colours.iter().next().unwrap();
        // A single colour that is *translucent* is an overlay — the theme's
        // dominant technique, and a different component from a flat fill.
        return if px[3] < 250 {
            Kind::Overlay
        } else {
            Kind::Flat
        };
    }

    let (w, h) = (img.width(), img.height());
    // Hairline: almost nothing is drawn, and it is confined to a few rows
    // or columns.
    let area = (w * h) as usize;
    if drawn * 8 < area {
        return Kind::Hairline;
    }

    // Gradient: every row identical (varies down) or every column identical
    // (varies across). Cheap to test, and it is a genuinely different
    // component — a linearGradient rather than a stack of rects.
    let row_uniform = (0..h).all(|y| (1..w).all(|x| img.get_pixel(x, y) == img.get_pixel(0, y)));
    let col_uniform = (0..w).all(|x| (1..h).all(|y| img.get_pixel(x, y) == img.get_pixel(x, 0)));
    if row_uniform || col_uniform {
        return Kind::Gradient;
    }

    Kind::Complex
}
