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
//!
//! It is ALSO the engine itself: `fasttrackstudio --engine` runs the
//! headless signal engine (`engine_main.rs`) — the same binary the
//! systemd unit and the Engines supervisor launch.

use dioxus::prelude::*;

#[cfg(all(feature = "signal", not(target_arch = "wasm32")))]
mod engine_main;
#[cfg(all(feature = "signal", not(target_arch = "wasm32")))]
mod engines;
#[cfg(feature = "session")]
mod guide;
mod prefs;
#[cfg(feature = "signal")]
mod rig_view;
#[cfg(feature = "session")]
mod session_engine;
#[cfg(feature = "session")]
mod session_view;
#[cfg(not(target_arch = "wasm32"))]
mod updates;

fn main() {
    // NVIDIA + Wayland: force the WebKitGTK webview through XWayland before
    // tao builds the event loop (`gtk::init` reads GDK_BACKEND there). Dioxus
    // sets these itself, but only inside `App::new`, AFTER the event loop is
    // built — so its GDK_BACKEND=x11 (the switch that actually cures the
    // NVIDIA/Wayland DMABUF lag) lands too late and never takes. Do it here,
    // before any GTK/tao init. No effect in --engine mode (no webview).
    #[cfg(target_os = "linux")]
    if std::env::var("XDG_SESSION_TYPE").as_deref() == Ok("wayland") {
        // SAFETY: single-threaded, before any GTK init or thread spawn.
        unsafe {
            std::env::set_var("GDK_BACKEND", "x11");
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }

    // `fasttrackstudio --engine` = the headless signal engine (the former
    // `signal-engine` binary): no GUI, no in-process session engine — just
    // the rig core + its vox router + the embedded web remote. Dispatch
    // before ANY app setup (tracing, session bootstrap, dioxus).
    #[cfg(all(feature = "signal", not(target_arch = "wasm32")))]
    if std::env::args().skip(1).any(|a| a == "--engine") {
        engine_main::run();
        return;
    }

    #[cfg(not(target_arch = "wasm32"))]
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

    launch_app();
}

/// Desktop: a frameless window — the app draws its own top bar (the
/// header doubles as title bar: drag surfaces + window controls).
#[cfg(not(target_arch = "wasm32"))]
fn launch_app() {
    use dioxus::desktop::tao::dpi::LogicalSize;
    use dioxus::desktop::{Config, WindowBuilder};
    let window = WindowBuilder::new()
        .with_title("FastTrackStudio")
        .with_decorations(false)
        .with_inner_size(LogicalSize::new(1280.0, 820.0))
        .with_min_inner_size(LogicalSize::new(720.0, 480.0));
    dioxus::LaunchBuilder::new()
        .with_cfg(Config::new().with_window(window).with_menu(None))
        .launch(App);
}

#[cfg(target_arch = "wasm32")]
fn launch_app() {
    dioxus::launch(App);
}

/// Top-level workspaces. Which ones exist depends on compiled features;
/// Home always exists — it's the landing page the others hang off.
#[derive(Clone, Copy, PartialEq)]
enum Workspace {
    Home,
    #[cfg(feature = "signal")]
    Signal,
    #[cfg(feature = "session")]
    Session,
    #[cfg(feature = "session")]
    Charts,
}

impl Workspace {
    fn all() -> Vec<(Self, &'static str)> {
        vec![
            (Self::Home, "Home"),
            #[cfg(feature = "signal")]
            (Self::Signal, "Signal"),
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

fn load_last_workspace() -> Option<Workspace> {
    let saved = prefs::get("last-workspace")?;
    Workspace::all()
        .into_iter()
        .find(|(_, label)| *label == saved)
        .map(|(w, _)| w)
}

fn store_last_workspace(w: Workspace) {
    prefs::set("last-workspace", w.label());
}

/// Web deep link: `#signal`, `#session`, `#charts`, `#home` (first hash
/// segment; `#signal/guitar` also picks the rig).
#[cfg(target_arch = "wasm32")]
fn hash_workspace() -> Option<Workspace> {
    let hash = web_sys::window()?.location().hash().ok()?;
    let first = hash.trim_start_matches('#').split('/').next()?;
    Workspace::all()
        .into_iter()
        .find(|(_, label)| label.eq_ignore_ascii_case(first))
        .map(|(w, _)| w)
}

/// Where the app lands: the URL hash (web), else the persisted last
/// choice, else Home.
fn initial_workspace() -> Option<Workspace> {
    #[cfg(target_arch = "wasm32")]
    if let Some(w) = hash_workspace() {
        return Some(w);
    }
    Some(load_last_workspace().unwrap_or(Workspace::Home))
}

#[component]
fn App() -> Element {
    let mut current = use_signal(initial_workspace);
    let mut settings_open = use_signal(|| false);

    rsx! {
        // Global reset: the frameless WebView keeps the platform's default 8px
        // body margin + white page background, which shows as a white border
        // around the dark 100vh app. Zero it and paint the page dark.
        document::Style { {"html,body{margin:0;padding:0;height:100%;background:#0a0a0a;overflow:hidden;}*{box-sizing:border-box;}"} }
        SessionChrome {}
        ResizeHandles {}
        div {
            style: "display: flex; flex-direction: column; height: 100vh; background: #0a0a0a; color: #e4e4e7; font-family: sans-serif;",
            // Workspace bar — the app-level switcher (domain views own
            // their internal navigation).
            // The header IS the title bar (the native decorations are off
            // on desktop): the wordmark and the flexible gap are drag
            // surfaces, double-click toggles maximize, and the window
            // controls live at the far right.
            header {
                style: "display: flex; align-items: center; gap: 8px; padding: 6px 0 6px 12px; border-bottom: 1px solid #27272a; user-select: none;",
                span {
                    style: "font-weight: 700; letter-spacing: 1px; font-size: 13px; cursor: default;",
                    onmousedown: move |_| drag_window(),
                    ondoubleclick: move |_| toggle_maximize(),
                    "FASTTRACKSTUDIO"
                }
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
                div {
                    style: "flex: 1; align-self: stretch;",
                    onmousedown: move |_| drag_window(),
                    ondoubleclick: move |_| toggle_maximize(),
                }
                EnginesArea {}
                button {
                    style: "padding: 4px 10px; border-radius: 6px; background: transparent; color: #a1a1aa; border: 1px solid #27272a; font-size: 12px;",
                    onclick: move |_| settings_open.toggle(),
                    "Settings"
                }
                WindowControls {}
            }
            if settings_open() {
                SettingsPanel {}
            }
            main { style: "flex: 1; min-height: 0; display: flex;",
                match current() {
                    Some(Workspace::Home) | None => rsx! {
                        HomeView { current }
                    },
                    #[cfg(feature = "signal")]
                    Some(Workspace::Signal) => rsx! {
                        // Rig picker → the chosen rig's remote over vox —
                        // the same surface as the web remote, pointed at
                        // the (supervised or remote) signal engine.
                        rig_view::SignalWorkspace {}
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
                }
            }
        }
    }
}

// ── Custom window chrome (desktop is frameless) ─────────────────────────────

fn drag_window() {
    #[cfg(not(target_arch = "wasm32"))]
    dioxus::desktop::window().drag();
}

fn toggle_maximize() {
    #[cfg(not(target_arch = "wasm32"))]
    dioxus::desktop::window().toggle_maximized();
}

/// Minimize / maximize / close — the right end of the title bar.
#[cfg(not(target_arch = "wasm32"))]
#[component]
fn WindowControls() -> Element {
    const BTN: &str = "width: 40px; align-self: stretch; display: flex; align-items: center; justify-content: center; background: transparent; border: none; color: #a1a1aa; font-size: 13px; cursor: default; padding: 0;";
    rsx! {
        div { style: "display: flex; align-self: stretch; margin-left: 4px;",
            button {
                style: BTN,
                onclick: move |_| dioxus::desktop::window().set_minimized(true),
                "–"
            }
            button {
                style: BTN,
                onclick: move |_| toggle_maximize(),
                "▢"
            }
            button {
                style: BTN,
                onclick: move |_| dioxus::desktop::window().close(),
                "✕"
            }
        }
    }
}

/// The browser draws its own chrome.
#[cfg(target_arch = "wasm32")]
#[component]
fn WindowControls() -> Element {
    rsx! {}
}

/// Invisible edge/corner strips that restore native-feeling resize on
/// the frameless window (decorations off also removes the compositor's
/// resize borders). Corners render after edges so they win the hit test.
#[cfg(not(target_arch = "wasm32"))]
#[component]
fn ResizeHandles() -> Element {
    use dioxus::desktop::tao::window::ResizeDirection as Dir;
    let handles: &[(&str, Dir)] = &[
        (
            "top: 0; left: 12px; right: 12px; height: 5px; cursor: ns-resize;",
            Dir::North,
        ),
        (
            "bottom: 0; left: 12px; right: 12px; height: 5px; cursor: ns-resize;",
            Dir::South,
        ),
        (
            "left: 0; top: 12px; bottom: 12px; width: 5px; cursor: ew-resize;",
            Dir::West,
        ),
        (
            "right: 0; top: 12px; bottom: 12px; width: 5px; cursor: ew-resize;",
            Dir::East,
        ),
        ("top: 0; left: 0; width: 12px; height: 12px; cursor: nwse-resize;", Dir::NorthWest),
        ("top: 0; right: 0; width: 12px; height: 12px; cursor: nesw-resize;", Dir::NorthEast),
        ("bottom: 0; left: 0; width: 12px; height: 12px; cursor: nesw-resize;", Dir::SouthWest),
        ("bottom: 0; right: 0; width: 12px; height: 12px; cursor: nwse-resize;", Dir::SouthEast),
    ];
    rsx! {
        for (pos, dir) in handles.iter().copied() {
            div {
                style: "position: fixed; z-index: 2147483647; {pos}",
                onmousedown: move |_| {
                    let _ = dioxus::desktop::window().drag_resize_window(dir);
                },
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[component]
fn ResizeHandles() -> Element {
    rsx! {}
}

// ── Home — the landing page ─────────────────────────────────────────────────

/// One workspace card on the Home page. Disabled cards are features not
/// compiled into this binary.
#[component]
fn HomeCard(
    title: &'static str,
    body: &'static str,
    target: Option<Workspace>,
    current: Signal<Option<Workspace>>,
) -> Element {
    let enabled = target.is_some();
    rsx! {
        button {
            style: if enabled {
                "display: flex; flex-direction: column; align-items: flex-start; gap: 8px; width: 220px; padding: 18px 16px; border-radius: 10px; background: #111113; color: #e4e4e7; border: 1px solid #27272a; text-align: left; cursor: pointer;"
            } else {
                "display: flex; flex-direction: column; align-items: flex-start; gap: 8px; width: 220px; padding: 18px 16px; border-radius: 10px; background: #0c0c0e; color: #52525b; border: 1px solid #1c1c1f; text-align: left;"
            },
            disabled: !enabled,
            onclick: move |_| {
                if let Some(w) = target {
                    current.set(Some(w));
                    store_last_workspace(w);
                }
            },
            span { style: "font-size: 16px; font-weight: 700;", "{title}" }
            span { style: "font-size: 12px; color: #a1a1aa; line-height: 1.5;", "{body}" }
            if !enabled {
                span { style: "font-size: 11px; color: #52525b;",
                    if cfg!(target_arch = "wasm32") { "coming to the web build" } else { "not in this build" }
                }
            }
        }
    }
}

#[component]
fn HomeView(current: Signal<Option<Workspace>>) -> Element {
    #[cfg(feature = "signal")]
    let signal_target = Some(Workspace::Signal);
    #[cfg(not(feature = "signal"))]
    let signal_target: Option<Workspace> = None;
    #[cfg(feature = "session")]
    let (session_target, charts_target) = (Some(Workspace::Session), Some(Workspace::Charts));
    #[cfg(not(feature = "session"))]
    let (session_target, charts_target): (Option<Workspace>, Option<Workspace>) = (None, None);

    rsx! {
        div { style: "display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 24px; flex: 1;",
            div { style: "display: flex; flex-direction: column; align-items: center; gap: 6px;",
                span { style: "font-size: 22px; font-weight: 700; letter-spacing: 2px;", "FASTTRACKSTUDIO" }
                span { style: "font-size: 12px; color: #71717a;", "Headless engines, remotes everywhere. Pick a surface." }
            }
            div { style: "display: flex; gap: 14px; flex-wrap: wrap; justify-content: center;",
                HomeCard {
                    title: "Session",
                    body: "Setlists and playback — the live show: songs, transport, guide.",
                    target: session_target,
                    current,
                }
                HomeCard {
                    title: "Signal",
                    body: "Live rigs — pick a rig (guitar, tracks, …) and control its engine, local or across the network.",
                    target: signal_target,
                    current,
                }
                HomeCard {
                    title: "Charts",
                    body: "keyflow chart writing — song analysis, chord charts, arrangement.",
                    target: charts_target,
                    current,
                }
            }
        }
    }
}

// ── Engines status area (header) ────────────────────────────────────────────

/// Compact per-engine status: dot + name + start/stop. The signal engine
/// is a supervised child process; the session engine is in-process.
#[cfg(all(feature = "signal", not(target_arch = "wasm32")))]
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

/// Web build: engines are always remote processes — the browser can't
/// supervise them, it just connects. (Also the native session-only
/// build's static status.)
#[cfg(any(not(feature = "signal"), target_arch = "wasm32"))]
#[component]
fn EnginesArea() -> Element {
    if cfg!(target_arch = "wasm32") {
        rsx! {
            div { style: "display: flex; align-items: center; gap: 6px; font-size: 12px;",
                span { style: "color: #52525b; font-size: 11px;", "engines are remote" }
            }
        }
    } else {
        rsx! {
            div { style: "display: flex; align-items: center; gap: 6px; font-size: 12px;",
                span { style: "width: 8px; height: 8px; border-radius: 999px; background: #22c55e;" }
                span { style: "color: #a1a1aa;", "Session" }
                span { style: "color: #52525b; font-size: 11px;", "(in-process)" }
            }
        }
    }
}

// ── Settings (version + update check stub) ─────────────────────────────────

#[component]
fn SettingsPanel() -> Element {
    #[allow(unused_mut)]
    let mut update_msg = use_signal(String::new);

    rsx! {
        div { style: "display: flex; align-items: center; gap: 12px; padding: 8px 12px; border-bottom: 1px solid #27272a; background: #111113; font-size: 12px;",
            span { style: "font-weight: 600;", "Settings" }
            span { style: "color: #a1a1aa;", "FastTrackStudio v{env!(\"CARGO_PKG_VERSION\")}" }
            UpdateCheck { msg: update_msg }
            if !update_msg().is_empty() {
                span { style: "color: #a1a1aa;", "{update_msg}" }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[component]
fn UpdateCheck(msg: Signal<String>) -> Element {
    rsx! {
        button {
            style: "padding: 3px 10px; border-radius: 5px; background: transparent; color: #a1a1aa; border: 1px solid #27272a; font-size: 11px;",
            onclick: move |_| {
                use updates::Updater as _;
                let text = match updates::CodebergUpdater.check_for_updates() {
                    updates::UpdateStatus::UpToDate => "Up to date.".to_string(),
                    updates::UpdateStatus::Available(info) => {
                        format!("Update available: v{}", info.version)
                    }
                    updates::UpdateStatus::Failed(e) => format!("Check failed: {e}"),
                };
                msg.set(text);
            },
            "Check for updates"
        }
    }
}

/// Web build: the deployment updates itself — nothing to check.
#[cfg(target_arch = "wasm32")]
#[component]
fn UpdateCheck(msg: Signal<String>) -> Element {
    let _ = msg;
    rsx! {}
}

/// The comprehensive Tailwind sheet — built by `just tailwind` from
/// `input.css`, which scans every UI crate (app src, signal-ui,
/// guitar-ui, session-ui, fts-ui, dock). Inlined rather than loaded as
/// an external stylesheet so it can't go stale against a committed file
/// (the same sheet `rig_view` inlines as `SIGNAL_TAILWIND`).
#[cfg(feature = "session")]
const APP_TAILWIND: &str = include_str!("../assets/tailwind-signal.css");

/// App-level chrome the session feature contributes: the compiled
/// Tailwind sheet session-ui's components style themselves with, and
/// the always-mounted event bridge (hub → global signals).
#[cfg(feature = "session")]
#[component]
fn SessionChrome() -> Element {
    rsx! {
        document::Style { {APP_TAILWIND} }
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
