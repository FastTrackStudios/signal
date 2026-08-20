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
// The watchOS remote's HTTP+SSE bridge (mounted by engine mode; watchOS
// can't speak vox over WebSocket).
#[cfg(all(feature = "signal", not(target_arch = "wasm32")))]
mod engine_watch;
// The session half of the watch bridge (setlist transport + mixer + chord
// window) — needs both the rig bridge plumbing and the session domain.
#[cfg(all(feature = "signal", feature = "session", not(target_arch = "wasm32")))]
mod engine_watch_session;
#[cfg(all(feature = "signal", not(target_arch = "wasm32")))]
mod engines;
#[cfg(all(feature = "session", not(target_arch = "wasm32")))]
mod guide;
mod prefs;
/// In-memory log ring (tracing capture + panic hook) — rendered by the
/// keys Logs tab on the phone, harmless elsewhere.
#[allow(dead_code)]
#[cfg(not(target_arch = "wasm32"))]
mod log_ring;
// The shared "dial the engine" plumbing every remote surface uses (rig
// views + the browser session player).
#[cfg(any(feature = "signal", feature = "session", feature = "signal-guitar"))]
mod remote;
/// Pack downloads — list + fetch `.signalpack`s from a pack host (the
/// studio engine or a hosted mirror) over the shared remote plumbing.
/// (Dead-code allowed: only the iOS keys shell drives it today.)
#[allow(dead_code)]
#[cfg(all(feature = "signal-guitar", not(target_arch = "wasm32")))]
mod pack_client;
#[cfg(feature = "signal")]
mod rig_view;
mod ekit_view;
mod space_view;
// The in-process session player (daw-standalone + audio + guide) is
// native-only; the wasm build is a remote of the network engine instead.
#[cfg(all(feature = "session", not(target_arch = "wasm32")))]
mod session_engine;
#[cfg(all(feature = "session", not(target_arch = "wasm32")))]
mod mixer_view;
#[cfg(all(feature = "session", not(target_arch = "wasm32")))]
mod lyric_sync_view;
#[cfg(all(feature = "session", not(target_arch = "wasm32")))]
mod session_view;
// Browser flavor of the Session workspace: SetlistService over the shared
// `/vox` link to `fasttrackstudio --engine`.
#[cfg(all(feature = "session", target_arch = "wasm32"))]
mod session_remote_view;
// Standalone web surface at /{org}/{collection}: dials the task-server's
// per-org CollectionService and lists that collection's songs. Additive —
// the wasm entry branches to it only when the URL matches (see launch_app);
// every other URL is the normal app shell.
#[cfg(all(feature = "session", target_arch = "wasm32"))]
mod collection_browser;
// The browser chart pane: the active song's keyflow chart (CPU engraver →
// SVG) with a playhead highlight driven by the transport streams.
#[cfg(all(feature = "session", target_arch = "wasm32"))]
mod session_chart_pane;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
mod updates;
// ── iPhone: the in-process rig + the phone shell ────────────────────────
// iOS forbids child processes, so the signal engine runs inside the app
// (rig_engine.rs) and the UI is a phone-sized shell (mobile_view.rs).
#[cfg(all(feature = "signal-guitar", target_os = "ios"))]
mod ios_audio;
#[cfg(all(feature = "signal-guitar", target_os = "ios"))]
mod ios_orientation;
#[cfg(all(feature = "signal-guitar", target_os = "ios"))]
mod rig_engine;
#[cfg(all(feature = "signal-guitar", target_os = "ios"))]
mod mobile_view;
/// The phone keys rig view (Play + pack Library). Compiled on every
/// native target so the Linux workspace check covers it; only the iOS
/// shell mounts it.
#[allow(dead_code)]
#[cfg(all(feature = "signal-keys-rig", not(target_arch = "wasm32")))]
mod keys_view;

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

    // `--keys` / `--workspace X --rig Y`: open straight to a place instead of
    // wherever the app was last left. Before the engine branch so `--engine`
    // stays unaffected.
    #[cfg(not(target_arch = "wasm32"))]
    apply_open_args();

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
    {
        use tracing_subscriber::layer::SubscriberExt as _;
        use tracing_subscriber::util::SubscriberInitExt as _;
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "info,vox_core=warn,schema_deser=off".into()),
            )
            .with(tracing_subscriber::fmt::layer())
            .with(log_ring::RingLayer::new())
            .init();
        log_ring::install_panic_hook();
    }

    // Session: bring up the in-process engine (standalone daw + setlist
    // service + demo setlist) before the UI. Failure is non-fatal — the
    // Session workspace shows an offline notice.
    #[cfg(all(feature = "session", not(target_arch = "wasm32")))]
    match session_engine::bootstrap_blocking() {
        Ok(()) => tracing::info!("session engine ready (in-process daw-standalone)"),
        Err(e) => tracing::error!("session engine failed to start: {e:?}"),
    }

    // iPhone: configure the audio session (duplex, speaker default), then
    // run the signal engine IN-PROCESS — iOS forbids child processes, so
    // the engines.rs supervisor path doesn't exist here.
    #[cfg(all(feature = "signal-guitar", target_os = "ios"))]
    {
        // The container ROOT isn't writable on iOS — only Documents/,
        // Library/, tmp/. Root all app config under one Files-app-visible
        // folder, Documents/FastTrackStudio/ (file sharing is on), so the
        // seeded guitar config lands writably AND the user can drop
        // keys/drums sample packs in by hand.
        if let Some(home) = std::env::var_os("HOME") {
            let app_root = std::path::PathBuf::from(&home).join("Documents/FastTrackStudio");
            let _ = std::fs::create_dir_all(&app_root);
            // signal_config_dir() = $XDG_CONFIG_HOME/signal →
            // Documents/FastTrackStudio/signal.
            // SAFETY: single-threaded, before the engine bootstrap spawns.
            unsafe { std::env::set_var("XDG_CONFIG_HOME", &app_root) };
            // Downloaded keys packs (pack_client.rs) live beside the config,
            // Files-app visible; the keys rig scans this dir.
            #[cfg(feature = "signal-keys-rig")]
            {
                let packs = app_root.join("Packs/Keys");
                let _ = std::fs::create_dir_all(&packs);
                // SAFETY: single-threaded, before the engine bootstrap spawns.
                unsafe { std::env::set_var("FTS_KEYSCAPE_PACKS", &packs) };
                // Phone RAM: preload only the audition set; everything else
                // decodes on first note-on instead of ballooning at load.
                // SAFETY: single-threaded, before the engine bootstrap spawns.
                unsafe { std::env::set_var("FTS_PRELOAD_PROFILE", "fast-audition") };
            }
        }
        ios_audio::configure();
        match rig_engine::bootstrap_blocking() {
            Ok(()) => tracing::info!("rig engine ready (in-process)"),
            Err(e) => tracing::error!("rig engine failed to start: {e:?}"),
        }
    }

    launch_app();
}

/// Window position, inner size, and whether to go borderless
/// fullscreen — each `None` meaning "let the platform decide".
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
type WindowPlacement = (Option<(f64, f64)>, Option<(f64, f64)>, bool);

/// Desktop: a frameless window — the app draws its own top bar (the
/// header doubles as title bar: drag surfaces + window controls).
/// Where the window opens, for multi-monitor desks. Placement is a *runtime*
/// concern (Dioxus.toml configures bundling, not windows), so it rides on env
/// vars — set them once in the `dx serve` command and every hot-reload lands
/// in the same place instead of being dragged back:
///
/// - `FTS_WINDOW_POS="6560,0"` — top-left corner in desktop coordinates
///   (`kscreen-doctor -o` on KDE prints each screen's geometry).
/// - `FTS_WINDOW_SIZE="2560x1440"` — inner size when not fullscreen.
/// - `FTS_WINDOW_FULLSCREEN=1` — borderless fullscreen on whichever monitor
///   the position lands on.
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
fn window_placement() -> WindowPlacement {
    fn pair(var: &str, sep: char) -> Option<(f64, f64)> {
        let raw = std::env::var(var).ok()?;
        let (a, b) = raw.split_once(sep)?;
        Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
    }
    let fullscreen = std::env::var("FTS_WINDOW_FULLSCREEN")
        .map(|v| v != "0" && !v.eq_ignore_ascii_case("false"))
        .unwrap_or(false);
    (pair("FTS_WINDOW_POS", ','), pair("FTS_WINDOW_SIZE", 'x'), fullscreen)
}

#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
fn launch_app() {
    use dioxus::desktop::tao::dpi::{LogicalPosition, LogicalSize};
    use dioxus::desktop::tao::window::Fullscreen;
    use dioxus::desktop::{Config, WindowBuilder};
    let (pos, size, fullscreen) = window_placement();
    let mut window = WindowBuilder::new()
        .with_title("FastTrackStudio")
        .with_decorations(false)
        .with_inner_size(match size {
            Some((w, h)) => LogicalSize::new(w, h),
            None => LogicalSize::new(1280.0, 820.0),
        })
        .with_min_inner_size(LogicalSize::new(720.0, 480.0));
    // Position first: borderless fullscreen picks the monitor the window is
    // on, so placing it inside the target screen is what selects that screen.
    if let Some((x, y)) = pos {
        window = window.with_position(LogicalPosition::new(x, y));
    }
    if fullscreen {
        window = window.with_fullscreen(Some(Fullscreen::Borderless(None)));
    }
    dioxus::LaunchBuilder::new()
        .with_cfg(Config::new().with_window(window).with_menu(None))
        .launch(App);
    // The desktop event loop returned (last window closed) — reap the engine we
    // spawned so it doesn't outlive the app. The engine's own watchdog is the
    // backstop for exits that never reach here (SIGKILL, crash).
    #[cfg(feature = "signal")]
    engines::shutdown();
}

#[cfg(target_arch = "wasm32")]
fn launch_app() {
    // Additive branch: a `/{org}/{collection}` URL launches the standalone
    // collection browser instead of the app shell. Every other path (root,
    // `#session` deep links, …) falls through to the normal app unchanged.
    #[cfg(feature = "session")]
    if collection_browser::route_matches() {
        dioxus::launch(collection_browser::CollectionBrowser);
        return;
    }
    dioxus::launch(App);
}

/// iPhone: the phone-sized shell (mobile_view.rs) over the in-process rig.
#[cfg(target_os = "ios")]
fn launch_app() {
    #[cfg(feature = "signal-guitar")]
    dioxus::launch(mobile_view::MobileApp);
    #[cfg(not(feature = "signal-guitar"))]
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
    #[cfg(all(feature = "session", not(target_arch = "wasm32")))]
    Arrangement,
    #[cfg(all(feature = "session", not(target_arch = "wasm32")))]
    Mixer,
    #[cfg(all(feature = "session", not(target_arch = "wasm32")))]
    LyricSync,
    #[cfg(feature = "charts")]
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
            #[cfg(all(feature = "session", not(target_arch = "wasm32")))]
            (Self::Arrangement, "Arrangement"),
            #[cfg(all(feature = "session", not(target_arch = "wasm32")))]
            (Self::Mixer, "Mixer"),
            #[cfg(all(feature = "session", not(target_arch = "wasm32")))]
            (Self::LyricSync, "Lyric Sync"),
            #[cfg(feature = "charts")]
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

    /// The rail glyph. The rail is icon-only, so this is how a workspace is
    /// recognised — labels are tooltips.
    fn icon(self) -> fts_chrome::Icon {
        use fts_chrome::Icon;
        match self {
            Self::Home => Icon::Home,
            #[cfg(feature = "signal")]
            Self::Signal => Icon::Signal,
            #[cfg(feature = "session")]
            Self::Session => Icon::Session,
            #[cfg(all(feature = "session", not(target_arch = "wasm32")))]
            Self::Arrangement => Icon::Arrangement,
            #[cfg(all(feature = "session", not(target_arch = "wasm32")))]
            Self::Mixer => Icon::Mixer,
            #[cfg(all(feature = "session", not(target_arch = "wasm32")))]
            Self::LyricSync => Icon::Lyrics,
            #[cfg(feature = "charts")]
            Self::Charts => Icon::Charts,
        }
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
/// Match a workspace by its label, case-insensitively (`"signal"`, `"Charts"`).
///
/// The label is already the user-facing name of the place, so it is the slug
/// too rather than inventing a second vocabulary for the command line.
#[cfg(not(target_arch = "wasm32"))]
fn workspace_from_slug(slug: &str) -> Option<Workspace> {
    Workspace::all()
        .into_iter()
        .find(|(_, label)| label.eq_ignore_ascii_case(slug.trim()))
        .map(|(w, _)| w)
}

fn initial_workspace() -> Option<Workspace> {
    // An explicit request beats the remembered workspace, for the same reason
    // as `FTS_OPEN_RIG` above it.
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(w) = std::env::var("FTS_OPEN_WORKSPACE")
        .ok()
        .and_then(|s| workspace_from_slug(&s))
    {
        return Some(w);
    }
    #[cfg(target_arch = "wasm32")]
    if let Some(w) = hash_workspace() {
        return Some(w);
    }
    Some(load_last_workspace().unwrap_or(Workspace::Home))
}

/// Turn `--workspace`/`--rig` (and the `--keys`-style shorthands) into the env
/// vars `initial_workspace` and `load_last_rig` read.
///
/// Env rather than threaded state because both readers are deep inside
/// component init, on both the desktop and wasm sides — and because it means
/// the same override works without a flag when launching from a unit file or
/// a wrapper script.
///
/// # Safety
/// Called once at the top of `main`, before any thread or GUI init.
#[cfg(not(target_arch = "wasm32"))]
fn apply_open_args() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value_of = |flag: &str| -> Option<String> {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let mut workspace = value_of("--workspace");
    let mut rig = value_of("--rig");
    // `--<rig>` shorthand: `--keys` is `--workspace signal --rig keys`. These
    // are the ones typed most often while iterating on a rig.
    for slug in ["guitar", "bass", "drums", "keys", "synth", "ekit"] {
        if args.iter().any(|a| a == &format!("--{slug}")) {
            workspace.get_or_insert_with(|| "signal".to_string());
            rig.get_or_insert_with(|| slug.to_string());
        }
    }
    // SAFETY: single-threaded, before any GUI or thread init.
    unsafe {
        if let Some(w) = workspace {
            std::env::set_var("FTS_OPEN_WORKSPACE", w);
        }
        if let Some(r) = rig {
            std::env::set_var("FTS_OPEN_RIG", r);
        }
    }
}

#[component]
fn App() -> Element {
    let mut current = use_signal(initial_workspace);
    // The app owns the chrome: one bar, the workspace rail, and the panel
    // rail every level publishes into (fts_chrome). Level 0 is the app's own
    // contribution — the workspace crumb and the engines/settings panels.
    let _chrome = fts_chrome::provide_chrome();
    let level = fts_chrome::use_chrome_level(0);
    let chrome = level.chrome();

    let go = use_callback(move |w: Workspace| {
        current.set(Some(w));
        store_last_workspace(w);
    });
    let here = current().unwrap_or(Workspace::Home);

    // The workspace crumb carries every other workspace as its menu, so the
    // rail's icons are never the only way to change place.
    level.crumbs(vec![fts_chrome::Crumb::here(here.label()).with_menu(
        Workspace::all()
            .into_iter()
            .map(|(w, label)| {
                (label.to_string(), w == here, Callback::new(move |_| go.call(w)))
            })
            .collect(),
    )]);
    level.panels(vec![
        fts_chrome::PanelSpec::new("engines", "Engines", fts_chrome::Icon::Engine).width(300),
        fts_chrome::PanelSpec::new("settings", "Settings", fts_chrome::Icon::Settings).width(300),
    ]);

    let rail_items: Vec<fts_chrome::RailItem> = Workspace::all()
        .into_iter()
        .map(|(w, label)| {
            fts_chrome::RailItem::new(
                label,
                label,
                w.icon(),
                current() == Some(w),
                Callback::new(move |_| go.call(w)),
            )
        })
        .collect();
    let sub_rail = chrome.sub_rail.read().clone();

    rsx! {
        // Global reset: the frameless WebView keeps the platform's default 8px
        // body margin + white page background, which shows as a white border
        // around the dark 100vh app. Zero it and paint the page dark.
        document::Style { {"html,body{margin:0;padding:0;height:100%;background:#0a0a0a;overflow:hidden;}*{box-sizing:border-box;}"} }
        SessionChrome {}
        ResizeHandles {}
        // ONE bar over two rails (fts_chrome::AppFrame). The bar is also the
        // title bar — the native decorations are off on desktop, so its slack
        // is the drag surface and the window controls sit at its right end.
        fts_chrome::AppFrame {
            top: rsx! {
                fts_chrome::TopBar {
                    leading: rsx! {
                        span {
                            style: "font-weight: 700; letter-spacing: 1px; font-size: 12px; \
                                    color: #71717a; cursor: default; padding-right: 2px;",
                            onmousedown: move |_| drag_window(),
                            ondoubleclick: move |_| toggle_maximize(),
                            "FTS"
                        }
                    },
                    trailing: rsx! { WindowControls {} },
                    on_drag: move |_| drag_window(),
                    on_expand: move |_| toggle_maximize(),
                }
            },
            rail: rsx! {
                fts_chrome::IconRail { items: rail_items, sub: sub_rail }
            },
            // App-level flyouts. Views render their own inside their layout.
            panel: rsx! {
                fts_chrome::PanelHost { id: "engines".to_string(),
                    div { style: "padding: 12px;", EnginesArea {} }
                }
                fts_chrome::PanelHost { id: "settings".to_string(), SettingsPanel {} }
            },
            right: rsx! { fts_chrome::PanelRail {} },
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
                    #[cfg(all(feature = "session", not(target_arch = "wasm32")))]
                    Some(Workspace::Session) => rsx! {
                        // The setlist player: session-ui's performance
                        // layout + transport strip over the in-process
                        // daw-standalone engine.
                        session_view::SessionWorkspace {}
                    },
                    #[cfg(all(feature = "session", target_arch = "wasm32"))]
                    Some(Workspace::Session) => rsx! {
                        // The browser is a remote: the same session-ui
                        // panels over SetlistService on the network
                        // engine's shared /vox router.
                        session_remote_view::SessionWorkspace {}
                    },
                    #[cfg(all(feature = "session", not(target_arch = "wasm32")))]
                    Some(Workspace::Arrangement) => rsx! {
                        // The real daw-ui arrangement view — the seeded Praise
                        // stems' audio clips laid out on the timeline.
                        div { style: "flex:1; min-height:0; display:flex;",
                            daw_ui::ArrangementView {}
                        }
                    },
                    #[cfg(all(feature = "session", not(target_arch = "wasm32")))]
                    Some(Workspace::Mixer) => rsx! {
                        // The real daw-ui mixer over the in-process daw engine —
                        // the seeded Praise stems with vol/pan/mute/solo + FX.
                        mixer_view::MixerWorkspace {}
                    },
                    #[cfg(all(feature = "session", not(target_arch = "wasm32")))]
                    Some(Workspace::LyricSync) => rsx! {
                        // Editable per-word lyric timings from a keyflow-sync
                        // TimingMap sidecar (forced alignment on the vocal stem).
                        lyric_sync_view::LyricSyncWorkspace {}
                    },
                    #[cfg(feature = "charts")]
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
    #[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
    dioxus::desktop::window().drag();
}

fn toggle_maximize() {
    #[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
    dioxus::desktop::window().toggle_maximized();
}

/// Minimize / maximize / close — the right end of the title bar.
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
#[component]
fn WindowControls() -> Element {
    use fts_chrome::{Icon, WindowButton};
    rsx! {
        div { style: "display: flex; align-items: center; gap: 2px; margin-left: 2px;",
            WindowButton {
                icon: Icon::Minimize,
                title: "Minimize".to_string(),
                on_click: move |_| dioxus::desktop::window().set_minimized(true),
            }
            WindowButton {
                icon: Icon::Maximize,
                title: "Maximize".to_string(),
                on_click: move |_| toggle_maximize(),
            }
            WindowButton {
                icon: Icon::Close,
                title: "Close".to_string(),
                danger: true,
                on_click: move |_| dioxus::desktop::window().close(),
            }
        }
    }
}

/// The browser/phone draws its own chrome.
#[cfg(any(target_arch = "wasm32", target_os = "ios"))]
#[component]
fn WindowControls() -> Element {
    rsx! {}
}

/// Invisible edge/corner strips that restore native-feeling resize on
/// the frameless window (decorations off also removes the compositor's
/// resize borders). Corners render after edges so they win the hit test.
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
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

#[cfg(any(target_arch = "wasm32", target_os = "ios"))]
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
    let session_target = Some(Workspace::Session);
    #[cfg(not(feature = "session"))]
    let session_target: Option<Workspace> = None;
    #[cfg(feature = "charts")]
    let charts_target = Some(Workspace::Charts);
    #[cfg(not(feature = "charts"))]
    let charts_target: Option<Workspace> = None;

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
        div {
            style: "display: flex; flex-direction: column; align-items: flex-start; gap: 12px; \
                    padding: 12px; font-size: 12px;",
            span { style: "color: #a1a1aa;", "FastTrackStudio v{env!(\"CARGO_PKG_VERSION\")}" }
            UpdateCheck { msg: update_msg }
            if !update_msg().is_empty() {
                span { style: "color: #a1a1aa;", "{update_msg}" }
            }
        }
    }
}

#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
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

/// Web/mobile build: the deployment updates itself — nothing to check.
#[cfg(any(target_arch = "wasm32", target_os = "ios"))]
#[component]
fn UpdateCheck(msg: Signal<String>) -> Element {
    let _ = msg;
    rsx! {}
}

/// The comprehensive Tailwind sheet — built by `just tailwind` from
/// `input.css`, which scans every UI crate (app src, signal-ui,
/// guitar-ui, session-ui, architect-ui, dock). Inlined rather than loaded as
/// an external stylesheet so it can't go stale against a committed file
/// (the same sheet `rig_view` inlines as `SIGNAL_TAILWIND`).
#[cfg(feature = "session")]
const APP_TAILWIND: &str = include_str!("../assets/tailwind-signal.css");

/// App-level chrome the session feature contributes: the compiled
/// Tailwind sheet session-ui's components style themselves with, and
/// the always-mounted event bridge (hub → global signals). Native
/// bridges the in-process engine's hubs; the browser bridges the
/// network engine's `#[subscribe]` streams over `/vox`.
#[cfg(feature = "session")]
#[component]
fn SessionChrome() -> Element {
    rsx! {
        document::Style { {APP_TAILWIND} }
        {
            #[cfg(not(target_arch = "wasm32"))]
            { rsx! { session_view::SessionEventBridge {} } }
            #[cfg(target_arch = "wasm32")]
            { rsx! { session_remote_view::SessionRemoteBridge {} } }
        }
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
