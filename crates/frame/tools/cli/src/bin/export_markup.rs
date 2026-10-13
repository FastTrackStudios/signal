//! Write the generated demo control surface as frame markup.
//!
//! ```text
//! export_markup --out demo.frame [--theme stage-dark|daylight]
//! ```
//! The output is a starting point for hand- or model-authored surfaces:
//! open it in the desktop player (`frame demo.frame`) and edit — the window
//! hot-reloads on save.

use frame::surface::widgets::{Options, Theme, surface_for_catalog};

fn main() {
    if let Err(err) = run() {
        eprintln!("export_markup error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut out = None;
    let mut theme = Theme::stage_dark();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--out" => out = args.next(),
            "--theme" => {
                theme = match args.next().as_deref() {
                    Some("daylight") => Theme::daylight(),
                    Some("stage-dark") => Theme::stage_dark(),
                    other => return Err(format!("unknown theme: {other:?} (stage-dark|daylight)")),
                }
            }
            other => return Err(format!("unknown arg: {other}")),
        }
    }
    let out = out.ok_or("--out <file.frame> is required")?;

    let catalog = frame::surface::memory::demo_catalog();
    let surface = surface_for_catalog(
        &catalog,
        &Options {
            theme,
            ..Options::default()
        },
    );
    let scene = surface.to_scene();
    let text = frame::document::text::print(&scene);
    // Markup plus its `.frame.assets/` sidecar when the scene carries any.
    frame::document::io::save_surface(&out, &scene).map_err(|e| format!("writing {e}"))?;
    println!(
        "{{\"ok\":true,\"out\":\"{out}\",\"bytes\":{},\"controls\":{}}}",
        text.len(),
        surface.controls.len()
    );
    Ok(())
}
