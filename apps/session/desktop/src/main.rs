//! Session — the unified app.
//!
//! One binary over the whole stack: chart writing (keyflow) and daw
//! integration (Session domain), feature-configured — `session` and/or
//! `charts` (default: both). Signal and Ignition are coordinated over
//! vox at runtime rather than linked in — see the repo README.
//!
//! The session engine runs in-process (`session_engine.rs`). The
//! Session surface embeds `session-ui`'s performance layout; the Charts
//! surface is keyflow's home.

use dioxus::prelude::*;

#[cfg(all(feature = "session", not(target_arch = "wasm32")))]
mod guide;
/// In-memory log ring (tracing capture + panic hook).
#[allow(dead_code)]
#[cfg(not(target_arch = "wasm32"))]
mod log_ring;
mod prefs;
// The shared "dial the engine" plumbing the browser session player uses.
#[cfg(feature = "session-domain")]
mod remote;
// The in-process session player (daw-standalone + audio + guide) is
// native-only; the wasm build is a remote of the network engine instead.
#[cfg(all(feature = "session", not(target_arch = "wasm32")))]
mod lyric_sync_view;
#[cfg(all(feature = "session", not(target_arch = "wasm32")))]
mod mixer_view;
#[cfg(all(feature = "session", not(target_arch = "wasm32")))]
mod session_engine;
// Recording Mode: dials a real, running REAPER's DAW socket instead of
// playing the setlist in-process. See its module doc for the split.
#[cfg(all(feature = "session", not(target_arch = "wasm32")))]
mod reaper_engine;
// Recording Mode's LAN control surface: forwards SetlistService calls to
// the real REAPER connection reaper_engine.rs already holds. See its
// module doc for why this needs to be a hand-implemented proxy rather
// than just re-mounting a router.
#[cfg(all(feature = "session", not(target_arch = "wasm32")))]
mod reaper_lan_proxy;
// Whichever of the two engines above is actually running, reduced to
// what the performance view needs from either.
#[cfg(all(feature = "session", not(target_arch = "wasm32")))]
mod active_engine;
#[cfg(all(feature = "session", not(target_arch = "wasm32")))]
mod session_view;
// `session-desktop --engine`: headless, serves the setlist over a
// LAN-reachable `/vox` WebSocket instead of opening a GUI window.
#[cfg(all(feature = "session", not(target_arch = "wasm32")))]
mod engine_server;
// `--engine --project/--setlist`: what the headless engine opens first —
// through the app's own song-opening path, so it needs the app (`native`).
#[cfg(all(feature = "session", feature = "native", not(target_arch = "wasm32")))]
mod engine_open;
// Home page data layer: the on-disk track libraries + their setlist notes.
#[cfg(all(feature = "session", not(target_arch = "wasm32")))]
mod setlist_library;
// Browser flavor of the Session workspace: SetlistService over the shared
// `/vox` link to `session-desktop --engine`.
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
/// The app on Blitz — see `docs/app-on-blitz.md`. The desktop's, and the
/// iPhone's: the same renderer and the same panels, laid out for the screen.
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
mod native;
#[cfg(all(feature = "session", target_arch = "wasm32"))]
mod session_chart_pane;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
mod updates;

fn main() {
    // iPhone: the page is drawn to the left and right edges, and minds the
    // camera housing itself (`native::ios_scene::island_on_left`) — a phone
    // on its side reports the housing's inset on both sides.
    #[cfg(target_os = "ios")]
    // SAFETY: the first thing main does, before any thread is started.
    unsafe {
        std::env::set_var("BLITZ_SAFE_AREA_SIDES", "0");
    }

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

    // `--workspace X`: open straight to a place instead of wherever the
    // app was last left.
    #[cfg(not(target_arch = "wasm32"))]
    apply_open_args();

    // Console logs (RUST_LOG-filtered fmt) + the in-memory log ring, plus
    // telemetry: Sentry (TASK_SENTRY_DSN) and OTLP export of traces/logs/
    // metrics when OTEL_EXPORTER_OTLP_ENDPOINT is set (http/protobuf →
    // the local collector on :4318). Hand-composed rather than
    // `architect_telemetry::init_tracing_full` because the app adds its own
    // RingLayer; the layer set is otherwise identical. The guards are
    // deliberately leaked — main hands control to the dioxus event loop,
    // which never returns normally.
    #[cfg(not(target_arch = "wasm32"))]
    {
        use tracing_subscriber::layer::SubscriberExt as _;
        use tracing_subscriber::util::SubscriberInitExt as _;
        if let Some(guard) = architect_telemetry::init("session") {
            std::mem::forget(guard);
        }
        // What is logged without RUST_LOG — and what is exported.
        const LOG_DEFAULT: &str = "info,vox_core=warn,schema_deser=off";
        let registry = tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| LOG_DEFAULT.into()),
            )
            .with(tracing_subscriber::fmt::layer())
            // And every run's log in a file of its own, to read afterwards:
            // `/tmp/fts-session/latest.log` (or `FTS_LOG_DIR`).
            .with(run_log().map(|file| {
                tracing_subscriber::fmt::layer()
                    .with_ansi(false)
                    .with_writer(std::sync::Mutex::new(file))
            }))
            .with(log_ring::RingLayer::new())
            .with(architect_telemetry::tracing_layer());
        match architect_telemetry::otel::init("session", LOG_DEFAULT) {
            Some((otel_guard, layers)) => {
                registry.with(layers).init();
                std::mem::forget(otel_guard);
            }
            None => registry.init(),
        }
        log_ring::install_panic_hook();
    }

    // `--engine`: headless — serves the setlist and (Live Mode) the whole
    // daw facade over a LAN-reachable /vox WebSocket and over iroh, for
    // other devices and for Session UIs attaching to it as a Remote — no
    // GUI window, never returns. Checked before the GUI's own session
    // bootstrap below since it replaces the whole rest of `main` rather
    // than adding to it. Arguments: see `engine_server::EngineArgs`.
    #[cfg(all(feature = "session", not(target_arch = "wasm32")))]
    if std::env::args().any(|a| a == "--engine") {
        let args = engine_server::EngineArgs::parse(&std::env::args().collect::<Vec<_>>());
        // Same mode switch the GUI path uses below — `--engine` just
        // replaces the GUI with a LAN server on top of whichever engine
        // that env var selects.
        let recording = std::env::var("FTS_SESSION_MODE").as_deref() == Ok("recording");
        if recording {
            match reaper_engine::bootstrap_blocking() {
                Ok(()) => {
                    tracing::info!("--engine: recording mode ready (connected to live REAPER)");
                }
                Err(e) => {
                    tracing::error!("--engine: recording mode failed to connect to REAPER: {e:?}");
                    std::process::exit(1);
                }
            }
            if args.open.is_some() {
                tracing::warn!(
                    "--engine: --project/--setlist ignored in recording mode (REAPER owns the project)"
                );
            }
        } else {
            match session_engine::bootstrap_blocking() {
                Ok(()) => tracing::info!("--engine: live mode ready (in-process daw-standalone)"),
                Err(e) => {
                    tracing::error!("--engine: session engine failed to start: {e:?}");
                    std::process::exit(1);
                }
            }
        }
        let rt = tokio::runtime::Runtime::new().expect("build the --engine server runtime");
        if let (false, Some(target), Some(engine)) =
            (recording, args.open.as_ref(), session_engine::engine())
        {
            #[cfg(feature = "native")]
            let opened = rt.block_on(engine_open::open(engine, target));
            // Songs open through the app's one path (session-daw): a build
            // without the app opens none, rather than a second way.
            #[cfg(not(feature = "native"))]
            let opened: eyre::Result<()> = {
                let _ = (engine, target);
                Err(eyre::eyre!(
                    "this build opens no songs: built without the app (feature `native`)"
                ))
            };
            if let Err(e) = opened {
                tracing::error!("--engine: could not open {target:?}: {e:?}");
                std::process::exit(1);
            }
        }
        if let Err(e) = rt.block_on(engine_server::run(&args)) {
            tracing::error!("--engine: server failed: {e:?}");
            std::process::exit(1);
        }
        return;
    }

    // The Blitz app opens its session on the studio's engine itself (see
    // `native::launch`); bringing the WRY app's engine up as well would put
    // two daw-standalone facades in one process.
    #[cfg(all(feature = "native", not(target_arch = "wasm32")))]
    {
        native::launch();
        return;
    }

    // Session: bring up the engine before the UI. Failure is non-fatal —
    // the Session workspace shows an offline notice.
    //
    // Live Mode (default): the in-process daw-standalone player. Recording
    // Mode: dial a real, already-running REAPER instead — set
    // `FTS_SESSION_MODE=recording` (a settings toggle is the next step;
    // this is the fastest correct switch for now). Recording Mode needs
    // REAPER already up with the FTS extension loaded (`just reaper
    // install` in fts-extensions, then start REAPER) — there is
    // deliberately no fallback to Live Mode on failure, so a
    // misconfigured Recording Mode fails loudly instead of silently
    // playing the setlist itself while someone thinks they're driving
    // REAPER.
    #[cfg(all(feature = "session", not(target_arch = "wasm32")))]
    if recording_mode_wanted() {
        match reaper_engine::bootstrap_blocking() {
            Ok(()) => tracing::info!("recording mode ready (connected to live REAPER)"),
            Err(e) => tracing::error!("recording mode failed to connect to REAPER: {e:?}"),
        }
    } else {
        match session_engine::bootstrap_blocking() {
            Ok(()) => tracing::info!("live mode ready (in-process daw-standalone)"),
            Err(e) => tracing::error!("session engine failed to start: {e:?}"),
        }
    }

    launch_app();
}

/// Whether to boot Recording Mode rather than Live Mode.
///
/// `FTS_SESSION_MODE` still decides when it is set — `recording` or `live`,
/// so either can be forced. Unset, the answer is simply "is there a REAPER to
/// drive?": if one is running with the FTS extension loaded, attaching to it
/// is what someone opening this app next to an open REAPER means.
///
/// It used to default to Live Mode unconditionally, so opening the app the
/// normal way (from the Dock, with REAPER already up) silently started the
/// in-process player instead — no connection, no error, and a setlist that
/// was not the one on screen in REAPER.
#[cfg(all(feature = "session", not(target_arch = "wasm32")))]
fn recording_mode_wanted() -> bool {
    match std::env::var("FTS_SESSION_MODE").as_deref() {
        Ok("recording") => true,
        Ok("live") => false,
        _ => {
            let found = !reaper_engine::discover_all().is_empty();
            if found {
                tracing::info!("a live REAPER is running — booting Recording Mode");
            }
            found
        }
    }
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
    (
        pair("FTS_WINDOW_POS", ','),
        pair("FTS_WINDOW_SIZE", 'x'),
        fullscreen,
    )
}

#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
fn launch_app() {
    use dioxus::desktop::tao::dpi::{LogicalPosition, LogicalSize};
    use dioxus::desktop::tao::window::Fullscreen;
    use dioxus::desktop::{Config, WindowBuilder};
    let (pos, size, fullscreen) = window_placement();
    let mut window = WindowBuilder::new()
        .with_title("Session")
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

/// iPhone: the phone-sized shell.
#[cfg(target_os = "ios")]
fn launch_app() {
    dioxus::launch(App);
}

/// Top-level workspaces. Which ones exist depends on compiled features;
/// Home always exists — it's the landing page the others hang off.
#[derive(Clone, Copy, PartialEq)]
enum Workspace {
    Home,
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

/// Web deep link: `#session`, `#charts`, `#home` (first hash segment).
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
/// Match a workspace by its label, case-insensitively (`"session"`, `"charts"`).
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
    // An explicit request beats the remembered workspace.
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

/// Turn `--workspace` into the env var `initial_workspace` reads.
///
/// Env rather than threaded state because the reader is deep inside
/// component init, on both the desktop and wasm sides — and because it
/// means the same override works without a flag when launching from a
/// unit file or a wrapper script.
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
    // SAFETY: single-threaded, before any GUI or thread init.
    if let Some(w) = value_of("--workspace") {
        unsafe { std::env::set_var("FTS_OPEN_WORKSPACE", w) };
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
    level.crumbs(vec![
        fts_chrome::Crumb::here(here.label()).with_menu(
            Workspace::all()
                .into_iter()
                .map(|(w, label)| {
                    (
                        label.to_string(),
                        w == here,
                        Callback::new(move |_| go.call(w)),
                    )
                })
                .collect(),
        ),
    ]);
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
            main { style: "flex: 1; min-height: 0; min-width: 0; display: flex;",
                match current() {
                    Some(Workspace::Home) | None => rsx! {
                        HomeView { current }
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
                        ArrangementWorkspace { current }
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
        (
            "top: 0; left: 0; width: 12px; height: 12px; cursor: nwse-resize;",
            Dir::NorthWest,
        ),
        (
            "top: 0; right: 0; width: 12px; height: 12px; cursor: nesw-resize;",
            Dir::NorthEast,
        ),
        (
            "bottom: 0; left: 0; width: 12px; height: 12px; cursor: nesw-resize;",
            Dir::SouthWest,
        ),
        (
            "bottom: 0; right: 0; width: 12px; height: 12px; cursor: nwse-resize;",
            Dir::SouthEast,
        ),
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
#[cfg(not(all(feature = "session", not(target_arch = "wasm32"))))]
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

#[cfg(all(feature = "session", not(target_arch = "wasm32")))]
#[component]
fn HomeView(current: Signal<Option<Workspace>>) -> Element {
    rsx! {
        HomeLibraryView { current }
    }
}

/// Non-native/no-session builds keep the plain launcher — the setlist
/// library is native-only (filesystem access, the in-process engine).
#[cfg(not(all(feature = "session", not(target_arch = "wasm32"))))]
#[component]
fn HomeView(current: Signal<Option<Workspace>>) -> Element {
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
                span { style: "font-size: 22px; font-weight: 700; letter-spacing: 2px;", "SESSION" }
                span { style: "font-size: 12px; color: #71717a;", "Setlists, songs, and charts. Pick a surface." }
            }
            div { style: "display: flex; gap: 14px; flex-wrap: wrap; justify-content: center;",
                HomeCard {
                    title: "Session",
                    body: "Setlists and playback — the live show: songs, transport, guide.",
                    target: session_target,
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

// ── Home library — setlists + track library, native session builds ─────────

/// The real Home page: browse the on-disk track libraries, build a
/// setlist, save it as a Task note, and load+play a saved one through
/// the in-process session engine.
#[cfg(all(feature = "session", not(target_arch = "wasm32")))]
#[component]
fn HomeLibraryView(current: Signal<Option<Workspace>>) -> Element {
    let mut library = use_signal(setlist_library::scan_libraries);
    let mut setlists = use_signal(setlist_library::list_setlists);
    let mut pending: Signal<Vec<setlist_library::LibraryEntry>> = use_signal(Vec::new);
    let mut new_name = use_signal(String::new);
    let mut destination_override: Signal<Option<String>> = use_signal(|| None);
    let mut load_status: Signal<Option<Result<String, String>>> = use_signal(|| None);
    let mut save_status: Signal<Option<Result<String, String>>> = use_signal(|| None);

    let suggested_destination = setlist_library::default_destination(&pending())
        .to_string_lossy()
        .to_string();
    let destination_value = destination_override().unwrap_or_else(|| suggested_destination.clone());

    let is_pending = move |path: &std::path::Path| pending().iter().any(|e| e.song.folder == path);

    let mut add_to_pending = move |entry: setlist_library::LibraryEntry| {
        if !is_pending(&entry.song.folder) {
            pending.write().push(entry);
        }
    };
    let mut remove_from_pending = move |folder: std::path::PathBuf| {
        pending.write().retain(|e| e.song.folder != folder);
    };
    let mut move_pending = move |index: usize, delta: isize| {
        let mut list = pending.write();
        let Some(new_index) = index.checked_add_signed(delta) else {
            return;
        };
        if new_index < list.len() {
            list.swap(index, new_index);
        }
    };

    let mut load_at = move |path: std::path::PathBuf| {
        let all = library();
        match setlist_library::read_setlist_songs(&path, &all) {
            Ok((songs, warnings)) if songs.is_empty() => {
                load_status.set(Some(Err(if warnings.is_empty() {
                    "this setlist has no songs".to_string()
                } else {
                    format!("no songs resolved — {}", warnings.join(", "))
                })));
            }
            Ok((songs, warnings)) => {
                spawn(async move {
                    // Recording Mode: open every song's real .RPP as its
                    // own REAPER tab (launching REAPER if nothing's
                    // running yet) instead of playing the setlist
                    // ourselves — see reaper_engine::load_playlist.
                    if std::env::var("FTS_SESSION_MODE").as_deref() == Ok("recording") {
                        let titles: Vec<String> = songs.iter().map(|s| s.title.clone()).collect();
                        match reaper_engine::load_playlist(&songs).await {
                            Ok(()) => {
                                let mut msg = format!(
                                    "opened {} in REAPER — building the setlist from what's now open",
                                    titles.join(" → ")
                                );
                                if !warnings.is_empty() {
                                    msg.push_str(&format!(
                                        " — {} link(s) unresolved: {}",
                                        warnings.len(),
                                        warnings.join(", ")
                                    ));
                                }
                                load_status.set(Some(Ok(msg)));
                                current.set(Some(Workspace::Session));
                                store_last_workspace(Workspace::Session);
                            }
                            Err(e) => load_status.set(Some(Err(format!("{e:?}")))),
                        }
                        return;
                    }

                    let Some(engine) = session_engine::engine() else {
                        load_status.set(Some(Err("session engine offline".to_string())));
                        return;
                    };
                    match engine.load_setlist(songs).await {
                        Ok(reports) => {
                            let total_tracks: usize = reports.iter().map(|r| r.track_count).sum();
                            let titles: Vec<&str> =
                                reports.iter().map(|r| r.title.as_str()).collect();
                            let mut msg = format!(
                                "loaded {} ({total_tracks} tracks) — audio streams in as each song plays",
                                titles.join(" → ")
                            );
                            if !warnings.is_empty() {
                                msg.push_str(&format!(
                                    " — {} link(s) unresolved: {}",
                                    warnings.len(),
                                    warnings.join(", ")
                                ));
                            }
                            load_status.set(Some(Ok(msg)));
                            current.set(Some(Workspace::Session));
                            store_last_workspace(Workspace::Session);
                        }
                        Err(e) => load_status.set(Some(Err(format!("{e:?}")))),
                    }
                });
            }
            Err(e) => load_status.set(Some(Err(format!("failed to read setlist: {e}")))),
        }
    };

    let save = {
        let destination_value = destination_value.clone();
        move |_| {
            let name = new_name().trim().to_string();
            if name.is_empty() {
                save_status.set(Some(Err("name the setlist first".to_string())));
                return;
            }
            let songs: Vec<_> = pending().iter().map(|e| e.song.clone()).collect();
            if songs.is_empty() {
                save_status.set(Some(Err("add at least one song".to_string())));
                return;
            }
            let dest = std::path::PathBuf::from(destination_value.clone());
            match setlist_library::create_setlist(&name, &songs, &dest) {
                Ok(path) => {
                    save_status.set(Some(Ok(format!("saved {}", path.display()))));
                    new_name.set(String::new());
                    destination_override.set(None);
                    pending.set(Vec::new());
                    setlists.set(setlist_library::list_setlists());
                }
                Err(e) => save_status.set(Some(Err(format!("failed to save: {e}")))),
            }
        }
    };

    rsx! {
        div { style: "display: flex; flex-direction: row; gap: 16px; flex: 1; min-height: 0; padding: 16px; overflow: hidden;",
            // ── Left column: saved setlists + the new-setlist builder ──
            div { style: "width: 340px; flex: none; display: flex; flex-direction: column; gap: 16px; min-height: 0; overflow-y: auto;",
                div { style: "display: flex; flex-direction: column; gap: 8px;",
                    span { style: "font-size: 12px; font-weight: 700; letter-spacing: 1px; color: #71717a; text-transform: uppercase;",
                        "Setlists"
                    }
                    if let Some(status) = load_status() {
                        StatusLine { status }
                    }
                    if setlists().is_empty() {
                        span { style: "font-size: 12px; color: #52525b;", "No setlists saved yet." }
                    } else {
                        for summary in setlists() {
                            SetlistRow {
                                key: "{summary.path.display()}",
                                title: summary.title.clone(),
                                library: summary.library,
                                onload: {
                                    let path = summary.path.clone();
                                    move |_| load_at(path.clone())
                                },
                            }
                        }
                    }
                }

                div { style: "height: 1px; background: #27272a;" }

                div { style: "display: flex; flex-direction: column; gap: 8px;",
                    span { style: "font-size: 12px; font-weight: 700; letter-spacing: 1px; color: #71717a; text-transform: uppercase;",
                        "New setlist"
                    }
                    input {
                        style: "background: #111113; color: #e4e4e7; border: 1px solid #27272a; border-radius: 6px; padding: 8px 10px; font-size: 13px;",
                        placeholder: "Name (e.g. \"Sunday, August 30 2026\")",
                        value: "{new_name}",
                        oninput: move |evt| new_name.set(evt.value()),
                    }
                    if pending().is_empty() {
                        span { style: "font-size: 12px; color: #52525b;", "Add songs from the library →" }
                    } else {
                        div { style: "display: flex; flex-direction: column; gap: 4px;",
                            for (i , entry) in pending().iter().cloned().enumerate() {
                                PendingRow {
                                    key: "{entry.song.folder.display()}",
                                    title: entry.song.title.clone(),
                                    artist: entry.song.artist.clone(),
                                    can_move_up: i > 0,
                                    can_move_down: i + 1 < pending().len(),
                                    onmoveup: move |_| move_pending(i, -1),
                                    onmovedown: move |_| move_pending(i, 1),
                                    onremove: {
                                        let folder = entry.song.folder.clone();
                                        move |_| remove_from_pending(folder.clone())
                                    },
                                }
                            }
                        }
                    }
                    label { style: "font-size: 11px; color: #71717a;", "Save to" }
                    input {
                        style: "background: #111113; color: #e4e4e7; border: 1px solid #27272a; border-radius: 6px; padding: 8px 10px; font-size: 12px; font-family: monospace;",
                        value: "{destination_value}",
                        oninput: move |evt| destination_override.set(Some(evt.value())),
                    }
                    if let Some(status) = save_status() {
                        StatusLine { status }
                    }
                    button {
                        style: "padding: 9px; border-radius: 6px; background: #16a34a; color: #f0fdf4; border: none; font-size: 13px; font-weight: 700; cursor: pointer;",
                        onclick: save,
                        "Save setlist"
                    }
                }
            }

            // ── Right column: the track library ─────────────────────────
            div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 8px; min-height: 0;",
                div { style: "display: flex; align-items: center; justify-content: space-between;",
                    span { style: "font-size: 12px; font-weight: 700; letter-spacing: 1px; color: #71717a; text-transform: uppercase;",
                        "Track library"
                    }
                    button {
                        style: "font-size: 11px; color: #71717a; background: transparent; border: 1px solid #27272a; border-radius: 6px; padding: 4px 8px; cursor: pointer;",
                        onclick: move |_| library.set(setlist_library::scan_libraries()),
                        "Rescan"
                    }
                }
                if library().is_empty() {
                    span { style: "font-size: 12px; color: #52525b;", "No songs found under the configured Tracks folders." }
                } else {
                    div { style: "display: flex; flex-direction: column; gap: 2px; overflow-y: auto; min-height: 0;",
                        for entry in library() {
                            LibrarySongRow {
                                key: "{entry.song.folder.display()}",
                                added: is_pending(&entry.song.folder),
                                title: entry.song.title.clone(),
                                artist: entry.song.artist.clone(),
                                key_label: entry.song.key.clone(),
                                library: entry.library,
                                stem_count: entry.song.stems.len(),
                                onadd: move |_| add_to_pending(entry.clone()),
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(all(feature = "session", not(target_arch = "wasm32")))]
#[component]
fn StatusLine(status: Result<String, String>) -> Element {
    match status {
        Ok(msg) => rsx! {
            span { style: "font-size: 11px; color: #4ade80;", "{msg}" }
        },
        Err(msg) => rsx! {
            span { style: "font-size: 11px; color: #f87171;", "{msg}" }
        },
    }
}

#[cfg(all(feature = "session", not(target_arch = "wasm32")))]
#[component]
fn SetlistRow(title: String, library: &'static str, onload: EventHandler<MouseEvent>) -> Element {
    rsx! {
        div {
            style: "display: flex; align-items: center; gap: 8px; padding: 8px 10px; border-radius: 8px; background: #111113; border: 1px solid #27272a;",
            div { style: "display: flex; flex-direction: column; flex: 1; min-width: 0;",
                span { style: "font-size: 13px; color: #e4e4e7; font-weight: 600;", "{title}" }
                span { style: "font-size: 10px; color: #52525b;", "{library}" }
            }
            button {
                style: "font-size: 11px; font-weight: 700; color: #0a0a0a; background: #e4e4e7; border: none; border-radius: 6px; padding: 6px 10px; cursor: pointer; white-space: nowrap;",
                onclick: move |evt| onload.call(evt),
                "Load & Play"
            }
        }
    }
}

#[cfg(all(feature = "session", not(target_arch = "wasm32")))]
#[component]
fn PendingRow(
    title: String,
    artist: String,
    can_move_up: bool,
    can_move_down: bool,
    onmoveup: EventHandler<MouseEvent>,
    onmovedown: EventHandler<MouseEvent>,
    onremove: EventHandler<MouseEvent>,
) -> Element {
    let btn = "background: transparent; border: none; color: #71717a; cursor: pointer; font-size: 12px; padding: 2px 4px;";
    rsx! {
        div { style: "display: flex; align-items: center; gap: 4px; padding: 4px 6px; border-radius: 6px; background: #0c0c0e;",
            div { style: "display: flex; flex-direction: column; flex: 1; min-width: 0;",
                span { style: "font-size: 12px; color: #e4e4e7;", "{title}" }
                span { style: "font-size: 10px; color: #52525b;", "{artist}" }
            }
            button { style: btn, disabled: !can_move_up, onclick: move |evt| onmoveup.call(evt), "↑" }
            button { style: btn, disabled: !can_move_down, onclick: move |evt| onmovedown.call(evt), "↓" }
            button {
                style: "background: transparent; border: none; color: #f87171; cursor: pointer; font-size: 12px; padding: 2px 4px;",
                onclick: move |evt| onremove.call(evt),
                "✕"
            }
        }
    }
}

#[cfg(all(feature = "session", not(target_arch = "wasm32")))]
#[component]
fn LibrarySongRow(
    added: bool,
    title: String,
    artist: String,
    key_label: Option<String>,
    library: &'static str,
    stem_count: usize,
    onadd: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        div {
            style: "display: flex; align-items: center; gap: 10px; padding: 8px 10px; border-radius: 8px;",
            div { style: "display: flex; flex-direction: column; flex: 1; min-width: 0;",
                span { style: "font-size: 13px; color: #e4e4e7;", "{title}" }
                span { style: "font-size: 11px; color: #71717a;",
                    "{artist}"
                    if let Some(k) = &key_label {
                        " · {k}"
                    }
                    " · {stem_count} stems · {library}"
                }
            }
            button {
                style: if added {
                    "font-size: 11px; color: #52525b; background: transparent; border: 1px solid #27272a; border-radius: 6px; padding: 5px 10px; cursor: default;"
                } else {
                    "font-size: 11px; font-weight: 700; color: #0a0a0a; background: #e4e4e7; border: none; border-radius: 6px; padding: 5px 10px; cursor: pointer;"
                },
                disabled: added,
                onclick: move |evt| onadd.call(evt),
                if added { "Added" } else { "+ Add" }
            }
        }
    }
}

// ── Arrangement workspace — daw-ui over the real session workflow modes ─────

/// A single glyph per [`session::modes::Mode`] — `daw-ui` has no icon-font
/// dependency, so these are plain emoji, matching the convention already
/// used for `daw-ui`'s own toolbar buttons.
#[cfg(all(feature = "session", not(target_arch = "wasm32")))]
fn mode_glyph(mode: session::modes::Mode) -> &'static str {
    use session::modes::Mode;
    match mode {
        Mode::Organize => "\u{1f5c2}", // card index dividers
        Mode::Write => "\u{270d}",     // writing hand
        Mode::Produce => "\u{1f39b}",  // control knobs
        Mode::Record => "\u{23fa}",    // record button
        Mode::Edit => "\u{2702}",      // scissors
        Mode::Mix => "\u{1f39a}",      // level slider
        Mode::Master => "\u{1f3c1}",   // checkered flag
        Mode::Live => "\u{1f3a4}",     // microphone
        Mode::Video => "\u{1f3ac}",    // clapper board
        Mode::Scoring => "\u{1f3bc}",  // musical score
    }
}

/// Hosts `daw-ui`'s `ArrangementView`, feeding it the real
/// `session::modes::Mode` set (Organize/Write/Produce/Record/Edit/Mix/
/// Master/Live/Video/Scoring) rather than an invented one — `daw-ui`
/// can't depend on `session` (backwards dependency edge), so this app
/// is what maps the real domain type into `daw-ui`'s generic
/// `ModeOption` shape.
///
/// State is a plain local signal, NOT `session::modes::set_mode` — that
/// function unconditionally calls into `daw::reaper::Reaper`
/// (`HighReaper::get()`), which panics without a real REAPER process.
/// This is display/selection only for now; nothing yet changes behavior
/// when the mode changes (no REAPER window layout to apply here).
#[cfg(all(feature = "session", not(target_arch = "wasm32")))]
#[component]
fn ArrangementWorkspace(current: Signal<Option<Workspace>>) -> Element {
    use session::modes::Mode;

    let mut active_mode = use_signal(|| Mode::Organize);
    let modes: Vec<daw_ui::ModeOption> = Mode::ALL
        .iter()
        .map(|m| daw_ui::ModeOption {
            slug: m.slug().to_string(),
            label: m.display_name().to_string(),
            glyph: mode_glyph(*m).to_string(),
        })
        .collect();

    rsx! {
        div { style: "flex:1; min-height:0; min-width:0; display:flex;",
            daw_ui::ArrangementView {
                modes,
                active_mode_slug: active_mode().slug().to_string(),
                on_mode_change: move |slug: String| {
                    if let Some(m) = Mode::from_slug(&slug) {
                        active_mode.set(m);
                    }
                },
                top_actions: mode_toolbar_actions(active_mode(), current),
            }
        }
    }
}

/// Per-mode toolbar content for the Arrangement workspace's `TopToolbar` —
/// "the organization mode has different toolbars" etc. Every button here
/// is wired to something real (a workspace switch, a live
/// `daw_control::Transport` call, or a real marker/region/tempo-map write
/// via `session::keyflow` against the in-process `daw-standalone` engine —
/// the same actions `fts-extensions`/`reaper-input` bind to hotkeys in
/// REAPER, run here without any REAPER dependency); modes with no
/// standalone-safe action yet (Write/Produce/Live/Video/Scoring — their
/// REAPER counterparts are window-layout/color actions gated behind
/// `session`'s `reaper` feature, unavailable in this app) render no extra
/// buttons rather than a fake one.
#[cfg(all(feature = "session", not(target_arch = "wasm32")))]
fn mode_toolbar_actions(
    mode: session::modes::Mode,
    mut current: Signal<Option<Workspace>>,
) -> Vec<daw_ui::ToolbarAction> {
    use session::keyflow::actions::{KeyflowAction, dispatch};
    use session::keyflow::time_signature::{TIME_SIGNATURES, insert_time_signature};
    use session::modes::Mode;
    use session::section_kinds::{MarkerKind, SectionKind};

    /// Fire one keyflow action against the live in-process engine — a
    /// no-op (traced, not panicking) if the engine isn't up yet, same
    /// as every other toolbar action in this app.
    fn run_keyflow(action: KeyflowAction) {
        let Some(engine) = session_engine::engine() else {
            tracing::debug!("session engine offline — ignoring keyflow toolbar action");
            return;
        };
        dispatch(&engine.standalone, action);
    }

    fn section_button(id: &'static str, label: &str, kind: SectionKind) -> daw_ui::ToolbarAction {
        let color = keyflow::sections::colors_for_section_type(&kind.section_type()).bright_css();
        daw_ui::ToolbarAction {
            id: id.to_string(),
            label: label.to_string(),
            glyph: String::new(),
            active: false,
            color: Some(color),
            on_click: EventHandler::new(move |_| run_keyflow(KeyflowAction::InsertSection(kind))),
        }
    }

    fn marker_button(
        id: &'static str,
        label: &str,
        glyph: &str,
        kind: MarkerKind,
    ) -> daw_ui::ToolbarAction {
        daw_ui::ToolbarAction {
            id: id.to_string(),
            label: label.to_string(),
            glyph: glyph.to_string(),
            active: false,
            color: Some(kind.css_color()),
            on_click: EventHandler::new(move |_| run_keyflow(KeyflowAction::InsertMarker(kind))),
        }
    }

    match mode {
        Mode::Organize => {
            let mut actions = vec![
                marker_button("count-in", "Count In", "", MarkerKind::CountIn),
                marker_button("mark-start", "=START", "", MarkerKind::Start),
                marker_button(
                    "song-start",
                    "Song Start",
                    "\u{2691}",
                    MarkerKind::SongStart,
                ),
                section_button("sec-intro", "Intro", SectionKind::Intro),
                section_button("sec-verse", "Verse", SectionKind::Verse),
                section_button("sec-prechorus", "Pre-CH", SectionKind::PreChorus),
                section_button("sec-chorus", "Chorus", SectionKind::Chorus),
                section_button("sec-bridge", "Bridge", SectionKind::Bridge),
                section_button("sec-outro", "Outro", SectionKind::Outro),
                section_button("sec-ending", "Ending", SectionKind::End),
                marker_button("song-end", "Song End", "\u{1f3c1}", MarkerKind::SongEnd),
                marker_button("mark-end", "=END", "", MarkerKind::End),
            ];
            actions.extend(TIME_SIGNATURES.iter().map(|&(num, denom)| {
                let label = format!("{num}/{denom}");
                let id = format!("timesig-{num}-{denom}");
                daw_ui::ToolbarAction {
                    id,
                    label,
                    glyph: String::new(),
                    active: false,
                    color: None,
                    on_click: EventHandler::new(move |evt: MouseEvent| {
                        let single_measure = evt.modifiers().shift();
                        let Some(engine) = session_engine::engine() else {
                            tracing::debug!(
                                "session engine offline — ignoring time-signature toolbar action"
                            );
                            return;
                        };
                        if let Err(e) =
                            insert_time_signature(&engine.standalone, num, denom, single_measure)
                        {
                            tracing::debug!(error = %e, num, denom, single_measure, "insert time signature failed");
                        }
                    }),
                }
            }));
            actions
        }
        Mode::Mix | Mode::Master => vec![daw_ui::ToolbarAction {
            id: "open-mixer".to_string(),
            label: "Mixer".to_string(),
            glyph: "\u{1f39a}".to_string(),
            active: false,
            color: None,
            on_click: EventHandler::new(move |_| current.set(Some(Workspace::Mixer))),
        }],
        Mode::Record => vec![daw_ui::ToolbarAction {
            id: "toggle-record".to_string(),
            label: "Record".to_string(),
            glyph: "\u{23fa}".to_string(),
            active: false,
            color: None,
            on_click: EventHandler::new(move |_| {
                spawn(async move {
                    let Some(daw) = daw_control::Daw::try_get() else {
                        return;
                    };
                    let Ok(project) = daw.current_project().await else {
                        return;
                    };
                    if let Err(e) = project.transport().toggle_recording().await {
                        tracing::debug!(error = %e, "toggle recording failed");
                    }
                });
            }),
        }],
        _ => Vec::new(),
    }
}

// ── Engines status area (header) ────────────────────────────────────────────

/// The session engine's status: in-process natively, remote on the web
/// build (the browser can't supervise a process, it just connects).
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
            span { style: "color: #a1a1aa;", "Session v{env!(\"CARGO_PKG_VERSION\")}" }
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
/// `input.css`, which scans every UI crate (app src, session-ui,
/// architect-ui, dock). Inlined rather than loaded as an external
/// stylesheet so it can't go stale against a committed file.
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

/// This run's log file: `session-<unix seconds>-<pid>.log` in
/// `FTS_LOG_DIR` (`/tmp/fts-session` by default), with `latest.log` beside
/// it pointing at the newest. `None` when the folder cannot be written —
/// the console log is still there.
#[cfg(not(target_arch = "wasm32"))]
fn run_log() -> Option<std::fs::File> {
    let dir = std::env::var_os("FTS_LOG_DIR").map_or_else(
        || std::path::PathBuf::from("/tmp/fts-session"),
        std::path::PathBuf::from,
    );
    std::fs::create_dir_all(&dir).ok()?;
    let started = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let name = format!("session-{started}-{}.log", std::process::id());
    let file = std::fs::File::create(dir.join(&name)).ok()?;
    let latest = dir.join("latest.log");
    let _ = std::fs::remove_file(&latest);
    #[cfg(unix)]
    let _ = std::os::unix::fs::symlink(&name, &latest);
    Some(file)
}
