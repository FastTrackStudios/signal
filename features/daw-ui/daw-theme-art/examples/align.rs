//! Every vector control, measured against the art it replaces.
//!
//!     cargo run -p daw-theme-art --example align
//!
//! Alignment kept going wrong one control at a time, and each time it was
//! found by squinting at a contact sheet — the routing button sat two
//! pixels left, mute ran past its guide, FX lost a column off its edge.
//! None of that is visible in a fidelity score, which deliberately ignores
//! *where* a drawing sits so it can survive a retint.
//!
//! So this reports the thing that was actually wrong: the bounding box of
//! the drawn pixels in each cell, ours against the source's, in the
//! source's own coordinates. A control that is correctly shaped but a
//! pixel left shows up as `x -1`, which is what you cannot see and can
//! read.
//!
//! Colour is reported too, now that the palette is the source's: the
//! dominant non-marker colour of each cell, which catches a control drawn
//! from the wrong entry — a blue-cast grey against a neutral one.

use daw_theme_art::compare::compare;
use daw_theme_art::{DerivedSpec, export};
use image::RgbaImage;

fn source_dir() -> std::path::PathBuf {
    std::path::Path::new("features/reaper/fts-theme/FastTrackStudio/.source-art").to_path_buf()
}

const MARKERS: [[u8; 4]; 2] = [[255, 0, 255, 255], [255, 255, 0, 255]];

/// Bounding box of the drawn, non-marker pixels within `x0..x1`.
fn box_of(img: &RgbaImage, x0: u32, w: u32) -> Option<(u32, u32, u32, u32)> {
    let (mut lo_x, mut lo_y, mut hi_x, mut hi_y) = (u32::MAX, u32::MAX, 0, 0);
    for x in x0..(x0 + w).min(img.width()) {
        for y in 0..img.height() {
            let p = img.get_pixel(x, y).0;
            if p[3] <= 40 || MARKERS.contains(&p) {
                continue;
            }
            lo_x = lo_x.min(x);
            lo_y = lo_y.min(y);
            hi_x = hi_x.max(x);
            hi_y = hi_y.max(y);
        }
    }
    // Saturating, because a cell's ink can start left of the origin the
    // caller declared — that is a real finding about the image, and an
    // audit that panics on it reports nothing at all about the other
    // hundred and fifty.
    (lo_x != u32::MAX).then_some((lo_x.saturating_sub(x0), lo_y, hi_x.saturating_sub(x0), hi_y))
}

/// The mean colour of a cell, composited over black.
///
/// Two corrections over the obvious version, both of which reported
/// correct controls as wrong:
///
/// A *mean* rather than the most common value, because half these
/// controls are drawn with gradients — no single colour repeats often
/// enough to win a modal vote, so the flat 1px border won instead and
/// every gradient-faced button read as "#171717 vs #3d3d3d".
///
/// And every pixel weighted by its alpha, rather than only the opaque
/// ones. Much of this artwork is deliberately semi-transparent so the
/// track colour shows through, so filtering on `alpha > 200` samples the
/// glyphs and nothing else — which is how `track_fx_norm`, a dark button
/// with white letters, reported a mean of #e7e7e7.
fn dominant(img: &RgbaImage, x0: u32, w: u32) -> Option<[u8; 4]> {
    let (mut sum, mut n) = ([0u64; 3], 0u64);
    for x in x0..(x0 + w).min(img.width()) {
        for y in 0..img.height() {
            let p = img.get_pixel(x, y).0;
            if MARKERS.contains(&p) {
                continue;
            }
            for i in 0..3 {
                sum[i] += p[i] as u64 * p[3] as u64 / 255;
            }
            n += 1;
        }
    }
    (n > 0).then(|| {
        [
            (sum[0] / n) as u8,
            (sum[1] / n) as u8,
            (sum[2] / n) as u8,
            255,
        ]
    })
}

fn hex(c: [u8; 4]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn main() {
    let dir = source_dir();
    if !dir.is_dir() {
        eprintln!("no source art at {}", dir.display());
        return;
    }

    let mut rows: Vec<(i64, String)> = Vec::new();
    let (mut checked, mut aligned) = (0usize, 0usize);

    for name in export::generatable() {
        let Ok(src) = image::open(dir.join(format!("{name}.png"))) else {
            continue;
        };
        let src = src.to_rgba8();
        let spec = DerivedSpec::from_image(&src);
        let Ok(ours) = export::render_control(name, &spec) else {
            rows.push((i64::MAX, format!("{name:<24} FAILED TO RENDER")));
            continue;
        };

        checked += 1;
        let mut worst = 0i64;
        let mut notes: Vec<String> = Vec::new();

        for (i, &(x, w)) in spec.cells.iter().enumerate() {
            match (box_of(&src, x, w), box_of(&ours, x, w)) {
                (Some(a), Some(b)) => {
                    let d = [
                        b.0 as i64 - a.0 as i64,
                        b.1 as i64 - a.1 as i64,
                        b.2 as i64 - a.2 as i64,
                        b.3 as i64 - a.3 as i64,
                    ];
                    let m = d.iter().map(|v| v.abs()).max().unwrap_or(0);
                    worst = worst.max(m);
                    if m > 0 {
                        notes.push(format!(
                            "cell {i}: left {:+} top {:+} right {:+} bottom {:+}",
                            d[0], d[1], d[2], d[3]
                        ));
                    }
                }
                (Some(_), None) => {
                    worst = 99;
                    notes.push(format!("cell {i}: we drew nothing"));
                }
                (None, Some(_)) => {
                    worst = 99;
                    notes.push(format!("cell {i}: source is empty, we drew something"));
                }
                (None, None) => {}
            }
        }

        // Colour, on cell 0 only — the resting state is the one to match.
        if let Some(&(x, w)) = spec.cells.first()
            && let (Some(a), Some(b)) = (dominant(&src, x, w), dominant(&ours, x, w))
            && a != b
        {
            let d: i64 = (0..3).map(|i| (a[i] as i64 - b[i] as i64).abs()).sum();
            if d > 24 {
                notes.push(format!("colour {} vs ours {}", hex(a), hex(b)));
                worst = worst.max(1);
            }
        }

        let fid = compare(&src, &ours).map(|f| f.score()).unwrap_or(0.0);

        if notes.is_empty() {
            aligned += 1;
        } else {
            rows.push((
                worst,
                format!("{name:<24} score {fid:.3}  {}", notes.join("; ")),
            ));
        }
    }

    rows.sort_by_key(|(w, _)| -*w);
    for (_, line) in &rows {
        println!("{line}");
    }
    println!("\n{aligned}/{checked} pixel-aligned with the source");
}
