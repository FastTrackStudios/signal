//! What the source art says about its own stretch bands.
//!
//!     cargo run -p daw-theme-art --example slices
//!     cargo run -p daw-theme-art --example slices -- mcp_volbg
//!     cargo run -p daw-theme-art --example slices -- --rust
//!
//! Prints, for every image the vector controls draw, the magenta guide
//! runs measured off the art REAPER ships and the [`Slice`] they imply —
//! then, where the image is declared in `slice::MCP_ART`, whether the
//! declaration agrees.
//!
//! `--rust` prints the same measurements as table entries. That is how the
//! table was authored: measured, transcribed, reviewed, then locked by
//! `declared_slices_match_the_source_art` in `export`. Nothing here writes
//! the table — a table this file could rewrite on demand would assert
//! nothing when the art moved under it.
//!
//! Run it after touching any source art, or when adding an image.

use daw_theme_art::derive::guides;
use daw_theme_art::slice::{self, Band};
use daw_theme_art::{DerivedSpec, export};

/// `Middle(1, 22)` in the form the table is written in.
fn band(b: Band) -> String {
    match b {
        Band::Fixed => "Fixed".into(),
        Band::All => "All".into(),
        Band::Middle(lo, hi) => format!("Middle({lo:.0}, {hi:.0})"),
    }
}

fn main() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../features/reaper/fts-theme/FastTrackStudio/.source-art");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let as_rust = args.iter().any(|a| a == "--rust");
    let only: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();

    let mut mismatches = 0;
    for name in export::generatable() {
        if !only.is_empty() && !only.iter().any(|n| *n == name) {
            continue;
        }
        let Ok(src) = image::open(dir.join(format!("{name}.png"))) else {
            println!("{name:28} no source art");
            continue;
        };
        let src = src.to_rgba8();
        let spec = DerivedSpec::from_image(&src);
        let cells = export::cells_for(name, &spec);
        let g = guides(&src);
        let got = g.slice(&cells, spec.width, spec.height);

        if as_rust {
            let (cw, h) = (cells[0].1, spec.height);
            match (got.x, got.y) {
                (Band::Fixed, Band::Fixed) => {
                    println!("    fixed(\"{name}\", ({cw}.0, {h}.0)),")
                }
                (x, y) => println!(
                    "    NamedArt::new(\"{name}\", ({cw}.0, {h}.0), \
                     Slice::new({}, {})),",
                    band(x),
                    band(y),
                ),
            }
            continue;
        }

        let declared = slice::art(name);
        let flag = match declared {
            None => "—",
            Some(a) if a.slice == got => "ok",
            // `All` and `Fixed` are the same drawing to REAPER — see
            // `Guides::slice`.
            Some(a)
                if a.slice.x.guided() == got.x.guided() && a.slice.y.guided() == got.y.guided() =>
            {
                "ok"
            }
            Some(_) => {
                mismatches += 1;
                "DIFFERS"
            }
        };

        println!(
            "{name:28} {w:>3}x{h:<3} cell {cx:>2}+{cw:<3} \
             guides l{l} r{r} t{t} b{b}  →  x {x:16} y {y:16} {flag}",
            w = spec.width,
            h = spec.height,
            cx = cells[0].0,
            cw = cells[0].1,
            l = g.left,
            r = g.right,
            t = g.top,
            b = g.bottom,
            x = band(got.x),
            y = band(got.y),
        );
        if let Some(a) = declared
            && flag == "DIFFERS"
        {
            println!(
                "{:28} declared x {:16} y {:16}",
                "",
                band(a.slice.x),
                band(a.slice.y)
            );
        }
    }
    if mismatches > 0 {
        eprintln!("\n{mismatches} declarations disagree with the art");
    }
}
