//! The expression editor in a real window.
//!
//! ```sh
//! cargo run -p expression-editor-standalone --example editor
//! cargo run -p expression-editor-standalone --example editor -- guitar
//! cargo run -p expression-editor-standalone --example editor -- song.rpp --track Vox
//! cargo run -p expression-editor-standalone --example editor -- part.mid --mode mpe
//! ```
//!
//! The window comes from `dioxus_native::launch_cfg` — Blitz → Vello →
//! winit, dioxus's own desktop path, the same one `--example serve`
//! uses. `dioxus::desktop::LaunchBuilder` would put WebKit/WRY behind
//! the same component and render it through a different engine
//! entirely, which is why it is never used here.

use dioxus::prelude::*;
use dioxus_native::{Config, LogicalSize, WindowAttributes, launch_cfg};
use expression_editor_standalone::cli::ArgsError;
use expression_editor_standalone::{App, Args, Runner, stage};

/// [`App`], plus the one thing only a window can supply.
///
/// The editor sizes its roll by subtracting its own chrome from the
/// space it has been given, and it cannot discover that space for
/// itself: dioxus-native delivers no element resize event. winit does
/// report the *window*, so turning a drag of the window edge into "your
/// space changed" is the host's job — here, and the REAPER panel's dock
/// callback there.
///
/// A wrapper in the example rather than inside [`App`] because `App` is
/// also mounted headless by `--example shot`, where `use_window` would
/// find no window to consume.
#[component]
fn WindowedApp() -> Element {
    let window = dioxus_native::use_window();

    // Report the size we already have, at mount.
    //
    // Not redundant with the event below: `SurfaceResized` fires when
    // the size *changes*, and on a window that opens at its final size
    // and is never dragged it may not fire at all. Without this the
    // editor never heard how much room it had, so it kept the viewport
    // its document was built with and the roll stayed the same size no
    // matter how big the window was — the bug this pair fixes.
    {
        let window = window.clone();
        use_hook(move || report(&*window, window.surface_size()));
    }

    dioxus_native::use_window_event(move |event, _| {
        if let dioxus_native::winit::event::WindowEvent::SurfaceResized(size) = event {
            report(&*window, *size);
        }
    });
    rsx! { App {} }
}

/// Hand the editor the window, in the units it works in.
///
/// winit reports physical pixels; the editor lays out in CSS pixels, so
/// the scale factor comes off here and nothing downstream has to know
/// the display's density.
fn report(
    window: &dyn dioxus_native::winit::window::Window,
    size: dioxus_native::PhysicalSize<u32>,
) {
    let scale = window.scale_factor();
    expression_editor_ui::available_space(size.width as f64 / scale, size.height as f64 / scale);
}

fn main() {
    let args = match Args::from_env() {
        Ok(a) => a,
        // Help and the scene list are successful runs that happen not
        // to open a window.
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

    println!(
        "{} — {} notes, mode {}",
        runner.label,
        runner.loaded.editor().doc.notes.len(),
        runner.loaded.editor().mode.label()
    );

    // The backend has to outlive the window: the session's location is
    // only meaningful against the `Standalone` it was read from, and
    // dropping it here would leave a document with nowhere to go back
    // to the moment write-back lands.
    let _daw = runner.daw;
    // A drum workspace stages its write half with it, so the quantize
    // panel's Apply and the slip drag reach the daw.
    match runner.host {
        Some(host) => {
            expression_editor_standalone::app::stage_with_host(runner.loaded.into_editor(), host)
        }
        None => stage(runner.loaded.into_editor()),
    }

    let window = WindowAttributes::default()
        .with_title(format!("Expression editor — {}", runner.label))
        .with_surface_size(LogicalSize::new(args.width as f64, args.height as f64));
    launch_cfg(
        WindowedApp,
        vec![],
        vec![Box::new(Config::new().with_window_attributes(window))],
    );
}
