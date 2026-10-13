//! Standalone launcher binary (for testing outside Reaper).
//! Build with: cargo run --features standalone

#[cfg(feature = "standalone")]
fn main() {
    use architect_launcher_ui::components::Launcher;
    use architect_ui::prelude::{ThemeMode, ThemeProvider, ThemeState, default_theme_preset};
    use dioxus_native::prelude::*;

    tracing_subscriber::fmt()
        .with_env_filter("info,wgpu_hal=error,wgpu_core=error")
        .init();

    fn app() -> Element {
        let state = use_signal(|| fts_launcher::LauncherEngine::new().into_state());
        let theme_state = use_signal(|| ThemeState::new(default_theme_preset(), ThemeMode::Dark));
        // dioxus 0.7.9 handlers must return `()` (or async) — the diverging
        // `!` from process::exit no longer satisfies SpawnIfAsync, so pin the
        // closure's return type to `()` explicitly.
        let on_close = |_: ()| -> () {
            std::process::exit(0);
        };

        rsx! {
            Stylesheet { href: asset!("/assets/tailwind.css") }
            ThemeProvider { state: theme_state,
                Launcher {
                    state,
                    on_close: on_close,
                }
            }
        }
    }

    use std::any::Any;
    let window_attrs = winit::window::WindowAttributes::default()
        .with_title("FTS Launcher")
        .with_surface_size(winit::dpi::LogicalSize::new(800.0, 520.0))
        .with_decorations(false)
        .with_resizable(true);
    let configs: Vec<Box<dyn Any>> = vec![Box::new(window_attrs)];
    dioxus_native::launch_cfg(app, Vec::new(), configs);
}

#[cfg(not(feature = "standalone"))]
fn main() {
    eprintln!("Build with --features standalone to run as a standalone binary");
    std::process::exit(1);
}
