//! FastTrackStudio — the unified app.
//!
//! One binary over the whole stack: chart writing (keyflow), setlist
//! creation, daw integration (Session domain) and the live guitar rig
//! (Signal domain), feature-configured — `signal`, `session`, or `full`
//! (default). Every domain remains a headless engine reached over vox;
//! this shell is a remote, the same architecture as the web remotes, so
//! Session usage, Signal usage, and combined usage are the same app
//! pointed at whichever engines are running.
//!
//! Phase-1 scaffold: workspace navigation + per-domain mount points.
//! The Signal surface embeds `signal-guitar-ui`'s remote; the Session
//! surface embeds `session-ui`'s performance layout; the Charts surface
//! is keyflow's home.

use dioxus::prelude::*;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();
    dioxus::launch(App);
}

/// Top-level workspaces. Which ones exist depends on compiled features.
#[derive(Clone, Copy, PartialEq)]
enum Workspace {
    #[cfg(feature = "signal")]
    Rig,
    #[cfg(feature = "session")]
    Session,
    #[cfg(feature = "session")]
    Charts,
}

impl Workspace {
    fn all() -> Vec<(Self, &'static str)> {
        vec![
            #[cfg(feature = "signal")]
            (Self::Rig, "Rig"),
            #[cfg(feature = "session")]
            (Self::Session, "Session"),
            #[cfg(feature = "session")]
            (Self::Charts, "Charts"),
        ]
    }
}

#[component]
fn App() -> Element {
    let spaces = Workspace::all();
    let mut current = use_signal(|| spaces.first().map(|(w, _)| *w));

    rsx! {
        div {
            style: "display: flex; flex-direction: column; height: 100vh; background: #0a0a0a; color: #e4e4e7; font-family: sans-serif;",
            // Workspace bar — the app-level switcher (domain views own
            // their internal navigation).
            header {
                style: "display: flex; align-items: center; gap: 8px; padding: 8px 12px; border-bottom: 1px solid #27272a;",
                span { style: "font-weight: 700; letter-spacing: 1px; font-size: 13px;", "FASTTRACKSTUDIO" }
                for (w, label) in Workspace::all() {
                    button {
                        style: if current() == Some(w) {
                            "padding: 4px 12px; border-radius: 6px; background: #e4e4e7; color: #0a0a0a; font-weight: 600; border: none; font-size: 12px;"
                        } else {
                            "padding: 4px 12px; border-radius: 6px; background: transparent; color: #a1a1aa; border: 1px solid #27272a; font-size: 12px;"
                        },
                        onclick: move |_| current.set(Some(w)),
                        "{label}"
                    }
                }
            }
            main { style: "flex: 1; min-height: 0; display: flex; align-items: center; justify-content: center;",
                match current() {
                    #[cfg(feature = "signal")]
                    Some(Workspace::Rig) => rsx! {
                        // Mount point: signal-guitar-ui remote (connects to
                        // rigd over vox — same surface as the web remote).
                        Placeholder { title: "Rig", body: "signal-guitar-ui remote mounts here — connect to signal-rigd (ws://localhost:4040/vox)." }
                    },
                    #[cfg(feature = "session")]
                    Some(Workspace::Session) => rsx! {
                        // Mount point: session-ui performance layout fed by
                        // the session engine's setlist stream.
                        Placeholder { title: "Session", body: "session-ui PerformanceLayout mounts here — setlists, songs, live charts from the session engine (:3030)." }
                    },
                    #[cfg(feature = "session")]
                    Some(Workspace::Charts) => rsx! {
                        // Mount point: keyflow chart writing.
                        Placeholder { title: "Charts", body: "keyflow chart writing lands here — song analysis, chord charts, arrangement." }
                    },
                    None => rsx! {
                        Placeholder { title: "No workspaces", body: "Build with --features signal, session, or full." }
                    },
                }
            }
        }
    }
}

#[component]
fn Placeholder(title: &'static str, body: &'static str) -> Element {
    rsx! {
        div { style: "display: flex; flex-direction: column; align-items: center; gap: 8px; max-width: 480px; text-align: center;",
            span { style: "font-size: 20px; font-weight: 700;", "{title}" }
            span { style: "font-size: 13px; color: #a1a1aa;", "{body}" }
        }
    }
}
