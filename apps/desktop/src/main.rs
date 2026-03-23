//! Signal Desktop — standalone Dioxus desktop app for signal chain management.
//!
//! Provides the same signal UI as fts-control but without session, DAW discovery,
//! or input-actions dependencies. Useful for quick iteration on signal domain UI.

use dioxus::desktop::{tao::window::WindowBuilder, Config};
use dioxus::prelude::*;

use signal::connect_db_seeded;
use signal_ui::SignalRoot;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("Starting Signal Desktop");

    let cfg = Config::new().with_window(
        WindowBuilder::new()
            .with_title("Signal")
            .with_inner_size(dioxus::desktop::tao::dpi::LogicalSize::new(1400.0, 900.0)),
    );

    LaunchBuilder::desktop().with_cfg(cfg).launch(App);
}

/// Root component — bootstraps the Signal controller, then delegates to [`SignalRoot`].
#[component]
fn App() -> Element {
    // Database path: use $SIGNAL_DB or default to ~/.local/share/signal/signal.db
    let db_path = std::env::var("SIGNAL_DB").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        let dir = std::path::PathBuf::from(home).join(".local/share/signal");
        std::fs::create_dir_all(&dir).ok();
        dir.join("signal.db").to_string_lossy().into_owned()
    });

    let controller = use_resource(move || {
        let path = db_path.clone();
        async move { connect_db_seeded(&path).await }
    });

    let read = controller.read();
    match &*read {
        Some(Ok(signal)) => rsx! {
            document::Stylesheet { href: asset!("/assets/tailwind.css") }
            SignalRoot { controller: signal.clone() }
        },
        Some(Err(e)) => {
            let msg = format!("Failed to initialize Signal: {e}");
            rsx! {
                div { class: "flex items-center justify-center h-screen bg-zinc-950 text-red-400",
                    "{msg}"
                }
            }
        }
        None => rsx! {
            div { class: "flex items-center justify-center h-screen bg-zinc-950 text-zinc-500",
                "Loading…"
            }
        },
    }
}
