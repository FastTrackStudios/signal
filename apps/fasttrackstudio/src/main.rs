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
//! The app is also the engine *manager*: the header's Engines area
//! supervises the `signal-engine` child process (see `engines.rs`),
//! while the session engine runs in-process (`session_engine.rs`). The
//! Rig workspace embeds `signal-guitar-ui`'s remote over a real vox
//! WebSocket (`rig_view.rs`); the Session surface embeds `session-ui`'s
//! performance layout; the Charts surface is keyflow's home.

use dioxus::prelude::*;

#[cfg(feature = "signal")]
mod engines;
#[cfg(feature = "session")]
mod guide;
#[cfg(feature = "signal")]
mod rig_view;
#[cfg(feature = "session")]
mod session_engine;
#[cfg(feature = "session")]
mod session_view;
mod updates;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,vox_core=warn,schema_deser=off".into()),
        )
        .init();

    // Session: bring up the in-process engine (standalone daw + setlist
    // service + demo setlist) before the UI. Failure is non-fatal — the
    // Session workspace shows an offline notice.
    #[cfg(feature = "session")]
    match session_engine::bootstrap_blocking() {
        Ok(()) => tracing::info!("session engine ready (in-process daw-standalone)"),
        Err(e) => tracing::error!("session engine failed to start: {e:?}"),
    }

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

    fn label(self) -> &'static str {
        Self::all()
            .into_iter()
            .find(|(w, _)| *w == self)
            .map(|(_, l)| l)
            .unwrap_or("?")
    }
}

// ── Landing / last-workspace persistence ────────────────────────────────────

fn last_workspace_path() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(
        std::path::Path::new(&home)
            .join(".config/fts")
            .join("last-workspace"),
    )
}

fn load_last_workspace() -> Option<Workspace> {
    let saved = std::fs::read_to_string(last_workspace_path()?).ok()?;
    let saved = saved.trim();
    Workspace::all()
        .into_iter()
        .find(|(_, label)| *label == saved)
        .map(|(w, _)| w)
}

fn store_last_workspace(w: Workspace) {
    let Some(path) = last_workspace_path() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, w.label());
}

/// Where the app lands: the persisted last choice, else Rig when the
/// signal engine is reachable, else Session (else whatever exists).
fn initial_workspace() -> Option<Workspace> {
    if let Some(saved) = load_last_workspace() {
        return Some(saved);
    }
    #[cfg(feature = "signal")]
    if engines::signal_running() {
        return Some(Workspace::Rig);
    }
    #[cfg(feature = "session")]
    return Some(Workspace::Session);
    #[allow(unreachable_code)]
    Workspace::all().first().map(|(w, _)| *w)
}

#[component]
fn App() -> Element {
    let mut current = use_signal(initial_workspace);
    let mut settings_open = use_signal(|| false);

    rsx! {
        SessionChrome {}
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
                        onclick: move |_| {
                            current.set(Some(w));
                            store_last_workspace(w);
                        },
                        "{label}"
                    }
                }
                div { style: "flex: 1;" }
                EnginesArea {}
                button {
                    style: "padding: 4px 10px; border-radius: 6px; background: transparent; color: #a1a1aa; border: 1px solid #27272a; font-size: 12px;",
                    onclick: move |_| settings_open.toggle(),
                    "Settings"
                }
            }
            if settings_open() {
                SettingsPanel {}
            }
            main { style: "flex: 1; min-height: 0; display: flex;",
                match current() {
                    #[cfg(feature = "signal")]
                    Some(Workspace::Rig) => rsx! {
                        // The guitar-rig remote over vox ws — the same
                        // surface as the web remote, pointed at the
                        // (supervised) signal engine.
                        rig_view::RigWorkspace {}
                    },
                    #[cfg(feature = "session")]
                    Some(Workspace::Session) => rsx! {
                        // The setlist player: session-ui's performance
                        // layout + transport strip over the in-process
                        // daw-standalone engine.
                        session_view::SessionWorkspace {}
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

// ── Engines status area (header) ────────────────────────────────────────────

/// Compact per-engine status: dot + name + start/stop. The signal engine
/// is a supervised child process; the session engine is in-process.
#[cfg(feature = "signal")]
#[component]
fn EnginesArea() -> Element {
    let mut signal_up = use_signal(engines::signal_running);
    let mut owned = use_signal(engines::signal_owned);
    let mut last_err = use_signal(String::new);

    // Poll the port every 2s — localhost connect/refuse resolves fast.
    use_future(move || async move {
        loop {
            architect::platform::sleep(std::time::Duration::from_millis(2000)).await;
            signal_up.set(engines::signal_running());
            owned.set(engines::signal_owned());
        }
    });

    rsx! {
        div { style: "display: flex; align-items: center; gap: 10px; font-size: 12px;",
            div { style: "display: flex; align-items: center; gap: 6px;",
                span {
                    style: if signal_up() {
                        "width: 8px; height: 8px; border-radius: 999px; background: #22c55e;"
                    } else {
                        "width: 8px; height: 8px; border-radius: 999px; background: #52525b;"
                    }
                }
                span { style: "color: #a1a1aa;", "Signal" }
                if signal_up() {
                    if owned() {
                        button {
                            style: "padding: 2px 8px; border-radius: 5px; background: transparent; color: #a1a1aa; border: 1px solid #27272a; font-size: 11px;",
                            onclick: move |_| {
                                if let Err(e) = engines::stop_signal() { last_err.set(e); }
                                signal_up.set(engines::signal_running());
                                owned.set(engines::signal_owned());
                            },
                            "Stop"
                        }
                    } else {
                        span { style: "color: #52525b; font-size: 11px;", "(external)" }
                    }
                } else {
                    button {
                        style: "padding: 2px 8px; border-radius: 5px; background: transparent; color: #a1a1aa; border: 1px solid #27272a; font-size: 11px;",
                        onclick: move |_| {
                            match engines::start_signal() {
                                Ok(_) => last_err.set(String::new()),
                                Err(e) => last_err.set(e),
                            }
                            owned.set(engines::signal_owned());
                        },
                        "Start"
                    }
                }
            }
            if cfg!(feature = "session") {
                div { style: "display: flex; align-items: center; gap: 6px;",
                    span { style: "width: 8px; height: 8px; border-radius: 999px; background: #22c55e;" }
                    span { style: "color: #a1a1aa;", "Session" }
                    span { style: "color: #52525b; font-size: 11px;", "(in-process)" }
                }
            }
            if !last_err().is_empty() {
                span { style: "color: #ef4444; font-size: 11px;", "{last_err}" }
            }
        }
    }
}

#[cfg(not(feature = "signal"))]
#[component]
fn EnginesArea() -> Element {
    rsx! {
        div { style: "display: flex; align-items: center; gap: 6px; font-size: 12px;",
            span { style: "width: 8px; height: 8px; border-radius: 999px; background: #22c55e;" }
            span { style: "color: #a1a1aa;", "Session" }
            span { style: "color: #52525b; font-size: 11px;", "(in-process)" }
        }
    }
}

// ── Settings (version + update check stub) ─────────────────────────────────

#[component]
fn SettingsPanel() -> Element {
    let mut update_msg = use_signal(String::new);

    rsx! {
        div { style: "display: flex; align-items: center; gap: 12px; padding: 8px 12px; border-bottom: 1px solid #27272a; background: #111113; font-size: 12px;",
            span { style: "font-weight: 600;", "Settings" }
            span { style: "color: #a1a1aa;", "FastTrackStudio v{updates::current_version()}" }
            button {
                style: "padding: 3px 10px; border-radius: 5px; background: transparent; color: #a1a1aa; border: 1px solid #27272a; font-size: 11px;",
                onclick: move |_| {
                    use updates::Updater as _;
                    let msg = match updates::CodebergUpdater.check_for_updates() {
                        updates::UpdateStatus::UpToDate => "Up to date.".to_string(),
                        updates::UpdateStatus::Available(info) => {
                            format!("Update available: v{}", info.version)
                        }
                        updates::UpdateStatus::Failed(e) => format!("Check failed: {e}"),
                    };
                    update_msg.set(msg);
                },
                "Check for updates"
            }
            if !update_msg().is_empty() {
                span { style: "color: #a1a1aa;", "{update_msg}" }
            }
        }
    }
}

/// App-level chrome the session feature contributes: the compiled
/// Tailwind sheet session-ui's components style themselves with, and
/// the always-mounted event bridge (hub → global signals).
#[cfg(feature = "session")]
#[component]
fn SessionChrome() -> Element {
    rsx! {
        document::Stylesheet { href: asset!("/assets/tailwind.css") }
        session_view::SessionEventBridge {}
    }
}

#[cfg(not(feature = "session"))]
#[component]
fn SessionChrome() -> Element {
    rsx! {}
}

#[component]
fn Placeholder(title: &'static str, body: &'static str) -> Element {
    rsx! {
        div { style: "display: flex; flex-direction: column; align-items: center; gap: 8px; max-width: 480px; text-align: center; margin: auto;",
            span { style: "font-size: 20px; font-weight: 700;", "{title}" }
            span { style: "font-size: 13px; color: #a1a1aa;", "{body}" }
        }
    }
}
