//! The same window, rasterized to a PNG instead of opened.
//!
//! ```sh
//! cargo run -p expression-editor-standalone --example shot -- \
//!     guitar --mode guitar --out target/gui-shots/guitar.png
//! ```
//!
//! This is where a mode's committed screenshot comes from. It paints
//! [`App`] — the *same* root the window mounts, not a lookalike — on a
//! headless Blitz DOM through `dioxus-test`'s CPU rasterizer. CPU
//! rather than the wgpu offscreen path because a screenshot that needs
//! a GPU cannot run on a CI box, and the point of committing pictures
//! is that they get regenerated.
//!
//! It takes the same arguments as `--example editor`, so the picture
//! and the window are the same load. A shot that came from a different
//! code path would be worse than no shot.

use std::path::PathBuf;

use dioxus::prelude::VirtualDom;
use dioxus_test::DocumentTester;
use expression_editor_standalone::cli::ArgsError;
use expression_editor_standalone::{App, Args, Runner, stage};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args = match Args::from_env() {
        Ok(a) => a,
        Err(e @ (ArgsError::Help | ArgsError::List)) => {
            print!("{e}");
            return;
        }
        Err(e) => {
            eprintln!("{e}\n\n{}", expression_editor_standalone::cli::USAGE);
            std::process::exit(2);
        }
    };

    let runner = match Runner::open(&args.source, &args.target, args.viewport(), args.mode) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    let out = args
        .out
        .clone()
        .unwrap_or_else(|| default_out(&runner.label));
    if let Some(dir) = out.parent()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        eprintln!("create {}: {e}", dir.display());
        std::process::exit(1);
    }

    let _daw = runner.daw;
    let mut editor = runner.loaded.into_editor();
    // `SHOT_SPAN=start..end` (seconds) zooms the shot to a window of
    // the material — a whole song in 1600 px is a texture, not a
    // picture of the editor.
    if let Ok(span) = std::env::var("SHOT_SPAN")
        && let Some((a, b)) = span.split_once("..")
        && let (Ok(a), Ok(b)) = (a.trim().parse::<f64>(), b.trim().parse::<f64>())
        && b > a
    {
        let ups = editor.doc.time_base.units_per_second(editor.bpm);
        editor.camera.t0 = a * ups;
        editor.camera.units_per_px = ((b - a) * ups / (args.width as f64).max(1.0)).max(1e-9);
    }
    if let Some(host) = runner.host.clone() {
        expression_editor_standalone::app::stage_with_host(editor, host);
    } else {
        stage(editor);
    }

    let dom = VirtualDom::new(App);
    let tester = DocumentTester::from_virtual_dom(dom)
        .with_window_size(args.width, args.height)
        .build();
    // Four pumps: mount, the canvas's own measure, and the two the
    // resulting layout change costs. Fewer and the roll paints at its
    // pre-measure size.
    for _ in 0..4 {
        let _ = tester.pump().await;
    }
    // `SHOT_CLICK=<testid>[,<testid>…]` clicks chrome before the
    // picture — how a shot shows the quantize drawer open.
    if let Ok(ids) = std::env::var("SHOT_CLICK") {
        for id in ids.split(',').filter(|s| !s.is_empty()) {
            match tester.query(dioxus_test::by_testid(id)).immediately() {
                Ok(el) => {
                    el.click();
                    tester.drain();
                    tester.relayout();
                }
                Err(e) => eprintln!("SHOT_CLICK {id}: {e:?}"),
            }
        }
        for _ in 0..3 {
            let _ = tester.pump().await;
        }
    }
    tester.render_png(&out);
    println!("shot: {}", out.display());
}

fn default_out(label: &str) -> PathBuf {
    let slug: String = label
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../target/gui-shots/expression-editor-standalone")
        .join(format!("{}.png", slug.trim_matches('-')))
}
