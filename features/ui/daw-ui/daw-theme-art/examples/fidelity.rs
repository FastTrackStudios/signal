//! Score every registered component against the art it replaces, and emit
//! a side-by-side sheet.
//!
//!     cargo run -p daw-theme-art --example fidelity -- <theme-image-dir>
//!
//! Fidelity is *relative*: we are redrawing Reapertips' greys in our own
//! palette, so what must match is the silhouette and the internal light/dark
//! structure, not absolute RGB. See `compare`.

use daw_theme_art::{DerivedSpec, compare, components, render_for};

fn main() {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "features/reaper/fts-theme/FastTrackStudio/.source-art".to_string());
    let dir = std::path::Path::new(&dir);
    let out = std::path::Path::new("target/art-fidelity");
    std::fs::create_dir_all(out).unwrap();

    let mut rows: Vec<(String, f32, f32, f32)> = Vec::new();

    for (name, component) in components::registry() {
        let src_path = dir.join(format!("{name}.png"));
        let Ok(src) = image::open(&src_path) else {
            eprintln!("skip {name}: no source at {}", src_path.display());
            continue;
        };
        let src = src.to_rgba8();
        let spec = DerivedSpec::from_image(&src);

        let ours = match render_for(component, &spec) {
            Ok(img) => img,
            Err(e) => {
                eprintln!("skip {name}: {e}");
                continue;
            }
        };

        match compare(&ours, &src) {
            Some(f) => {
                rows.push((name.to_string(), f.shape, f.structure, f.score()));
                // Side by side at 8x so a 4x4 is judgeable.
                let zoom = |img: &image::RgbaImage| {
                    image::imageops::resize(
                        img,
                        img.width() * 8,
                        img.height() * 8,
                        image::imageops::FilterType::Nearest,
                    )
                };
                let (a, b) = (zoom(&src), zoom(&ours));
                let mut sheet = image::RgbaImage::new(a.width() * 2 + 8, a.height());
                image::imageops::overlay(&mut sheet, &a, 0, 0);
                image::imageops::overlay(&mut sheet, &b, (a.width() + 8) as i64, 0);
                sheet.save(out.join(format!("{name}.png"))).unwrap();
            }
            None => eprintln!(
                "skip {name}: size mismatch — rendered {:?}, source {:?}",
                ours.dimensions(),
                src.dimensions()
            ),
        }
    }

    rows.sort_by(|a, b| a.3.partial_cmp(&b.3).unwrap());
    println!(
        "\n{:<22} {:>7} {:>10} {:>7}",
        "image", "shape", "structure", "score"
    );
    for (name, shape, structure, score) in &rows {
        println!("{name:<22} {shape:>7.3} {structure:>10.3} {score:>7.3}");
    }
    if !rows.is_empty() {
        let mean: f32 = rows.iter().map(|r| r.3).sum::<f32>() / rows.len() as f32;
        println!("\nmean score {mean:.3} over {} images", rows.len());
        println!("sheets in {}", out.display());
    }
}
