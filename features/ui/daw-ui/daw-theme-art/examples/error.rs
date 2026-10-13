//! Every vector control ranked by how wrong it looks, worst first.
//!
//!     cargo run -p daw-theme-art --example error
//!     cargo run -p daw-theme-art --example error -- envcp_bgsel   # one image
//!     cargo run -p daw-theme-art --example error -- --map envcp_bgsel
//!
//! `align` answers "is this in the right place", pass or fail. It cannot
//! answer "does this look right", and it has passed images that plainly
//! do not: a whole missing accent bar on `envcp_bgsel` scored 0.706 and
//! read as a rounding difference, because its colour check trips only
//! when the summed per-channel means move by more than 24 — which a
//! small bright feature on a large plate never does.
//!
//! This reports the mean absolute per-pixel difference, both images
//! composited over black. It is the number that tracks what shows on
//! screen, and it is what found the six plates that had each quietly
//! dropped a vertical rule.
//!
//! `--map` prints a signed difference map for one image instead:
//! positive where we painted too much light, blank under ±3. That is the
//! form that says *where* and *which way*, which no single score does —
//! a gear drawn with eight teeth showed +144 at three and nine o'clock
//! and −93 at ±68°, which is a six-tooth gear stated as plainly as it
//! can be.
//!
//! **Markers are excluded from both.** They are copied verbatim by the
//! exporter, so comparing them only ever reports magenta against
//! whatever the component drew underneath. On a 48×12 background that is
//! most of the total, and it is why the missing accent bar sat unnoticed
//! through a full pass of the family.

use daw_theme_art::{DerivedSpec, export};
use image::RgbaImage;

const MARKERS: [[u8; 4]; 2] = [[255, 0, 255, 255], [255, 255, 0, 255]];

fn marker(p: [u8; 4]) -> bool {
    MARKERS.contains(&p)
}

/// One pixel's mean channel value, composited over black.
fn level(p: [u8; 4]) -> f32 {
    (0..3)
        .map(|i| p[i] as f32 * p[3] as f32 / 255.0)
        .sum::<f32>()
        / 3.0
}

fn scored(src: &RgbaImage, ours: &RgbaImage) -> Option<f32> {
    if src.dimensions() != ours.dimensions() {
        return None;
    }
    let (mut total, mut n) = (0.0f64, 0u64);
    for (a, b) in src.pixels().zip(ours.pixels()) {
        if marker(a.0) || marker(b.0) {
            continue;
        }
        for i in 0..3 {
            let (x, y) = (
                a.0[i] as f32 * a.0[3] as f32 / 255.0,
                b.0[i] as f32 * b.0[3] as f32 / 255.0,
            );
            total += (x - y).abs() as f64;
            n += 1;
        }
    }
    (n > 0).then(|| (total / n as f64) as f32)
}

fn print_map(name: &str, src: &RgbaImage, ours: &RgbaImage) {
    println!("{name}  ours minus source, blank under ±3, `m` where a marker is");
    for y in 0..src.height() {
        let mut row = String::new();
        for x in 0..src.width() {
            let (a, b) = (src.get_pixel(x, y).0, ours.get_pixel(x, y).0);
            if marker(a) || marker(b) {
                row.push_str("   m");
                continue;
            }
            let d = level(b) - level(a);
            if d.abs() < 3.0 {
                row.push_str("   .");
            } else {
                row.push_str(&format!("{d:+4.0}"));
            }
        }
        println!("{y:3} {row}");
    }
}

fn main() {
    let dir = std::path::Path::new("features/reaper/fts-theme/FastTrackStudio/.source-art");
    let theme = std::path::Path::new("features/reaper/fts-theme/FastTrackStudio");
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let map = args.first().is_some_and(|a| a == "--map");
    if map {
        args.remove(0);
    }

    let names: Vec<String> = if args.is_empty() {
        export::generatable()
            .into_iter()
            .map(str::to_string)
            .collect()
    } else {
        args
    };

    let mut rows: Vec<(f32, String)> = Vec::new();
    for name in &names {
        let Ok(src) = image::open(dir.join(format!("{name}.png"))) else {
            continue;
        };
        let src = src.to_rgba8();
        let spec = DerivedSpec::from_image(&src);
        let Ok(ours) = export::render_control(name, &spec) else {
            continue;
        };
        // Compare against what is on disk when it is there — the whole
        // point is to catch a component and its exported PNG disagreeing.
        let ours = image::open(theme.join(format!("{name}.png")))
            .map(|i| i.to_rgba8())
            .unwrap_or(ours);
        if map {
            print_map(name, &src, &ours);
            continue;
        }
        if let Some(e) = scored(&src, &ours) {
            rows.push((e, name.to_string()));
        }
    }

    rows.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (e, name) in &rows {
        println!("{e:7.2}  {name}");
    }
}
