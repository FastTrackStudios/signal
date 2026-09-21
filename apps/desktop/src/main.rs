//! `FastTrackStudio` — the unified app.
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
// The TONE3000 OAuth landing strip — the registered redirect URI, served
// by the engine because no GUI here can host the authorization page.
#[cfg(all(feature = "signal", not(target_arch = "wasm32")))]
mod engine_tone3000;
#[cfg(all(feature = "signal", not(target_arch = "wasm32")))]
mod engines;
/// In-memory log ring (tracing capture + panic hook) — rendered by the
/// keys Logs tab on the phone, harmless elsewhere.
#[expect(dead_code)]
#[cfg(not(target_arch = "wasm32"))]
mod log_ring;
mod prefs;
// The shared "dial the engine" plumbing every remote surface uses (rig
// views + the browser session player).
/// Pack downloads — list + fetch `.signalpack`s from a pack host (the
/// studio engine or a hosted mirror) over the shared remote plumbing.
/// (Dead-code allowed: only the iOS keys shell drives it today.)
#[expect(dead_code)]
#[cfg(all(feature = "signal-guitar", not(target_arch = "wasm32")))]
mod pack_client;
/// (Dead-code allowed: which of these dialers get used depends on which rig
/// surfaces are compiled in — the guitar-only phone build needs a subset.)
#[expect(dead_code)]
#[cfg(any(feature = "signal", feature = "signal-guitar"))]
mod remote;
#[cfg(feature = "signal")]
mod rig_view;
/// The instrument menu — the app's front door, shared by the desktop
/// workspace and the phone shell. (Dead-code allowed: a `signal-guitar`-only
/// build that is not iOS compiles neither shell that renders it.)
#[expect(dead_code)]
#[cfg(any(feature = "signal", feature = "signal-guitar"))]
mod rigs;
// Browser keys rig (/rigs/keys/:profile): the keys AudioWorklet + streamed
// packs cached in OPFS + WebMIDI/demo MIDI. Wasm-only — the desktop app
// plays rigs through the engine instead.
#[cfg(all(feature = "signal", target_arch = "wasm32"))]
mod web_keys_rig;
// W9: the in-tab keys backend — KeysRigRemote's client surface served
// locally over architect's in-process LocalServer (wasm memory link).
#[cfg(all(feature = "signal", target_arch = "wasm32"))]
mod web_keys_backend;
#[cfg(all(feature = "signal", target_arch = "wasm32"))]
mod web_packs;
// W12: the decoder worker — sample decode OFF the audio thread (the
// worklet never decodes; see crates/signal/docs/browser-keys-rig.md).
#[cfg(all(feature = "signal", target_arch = "wasm32"))]
mod web_keys_decoder;
// W13: shared-memory streamer threads — the decoders write chunks straight
// into the heap the audio thread renders from.
// Rig surfaces reached from `rig_view` (itself signal-gated): the 4x4 ekit
// pad grid and the 2D sample-space map. Both use signal-{ekit,space}-proto,
// which only the `signal` feature pulls in, so they must carry the same gate
// — without it a session-only build fails to resolve them.
#[cfg(feature = "signal")]
mod ekit_view;
#[cfg(feature = "signal")]
mod space_view;
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
mod updates;
#[cfg(all(feature = "signal", target_arch = "wasm32"))]
mod web_keys_threads;
// ── iPhone: the in-process rig + the phone shell ────────────────────────
// iOS forbids child processes, so the signal engine runs inside the app
// (rig_engine.rs) and the UI is a phone-sized shell (mobile_view.rs).
#[cfg(all(feature = "signal-guitar", target_os = "ios"))]
mod ios_audio;
#[cfg(all(feature = "signal-guitar", target_os = "ios"))]
mod ios_orientation;
/// The phone keys rig view (Play + pack Library). Compiled on every
/// native target so the Linux workspace check covers it; only the iOS
/// shell mounts it.
#[allow(dead_code)]
#[cfg(all(feature = "signal-keys-rig", not(target_arch = "wasm32")))]
mod keys_view;
#[cfg(all(feature = "signal-guitar", target_os = "ios"))]
mod mobile_view;
/// The rig engine embedded IN-PROCESS. iOS has no choice (it cannot spawn a
/// child); desktop uses it by default because a rig that needs a second
/// process and a free port to make a sound is a worse default than one that
/// just plays. The supervised child is still there behind a preference.
#[expect(dead_code)]
#[cfg(all(feature = "signal-guitar", not(target_arch = "wasm32")))]
mod rig_engine;
// The macOS menu bar: audio readout + Settings… ⌘, (reads the embedded rig).
#[cfg(all(target_os = "macos", feature = "signal-guitar"))]
mod mac_menu;

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
        if let Some(guard) = architect_telemetry::init("signal") {
            std::mem::forget(guard);
        }
        let registry = tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "info,vox_core=warn,schema_deser=off".into()),
            )
            .with(tracing_subscriber::fmt::layer())
            .with(log_ring::RingLayer::new())
            .with(architect_telemetry::tracing_layer());
        match architect_telemetry::otel::init("signal") {
            Some((otel_guard, layers)) => {
                registry.with(layers).init();
                std::mem::forget(otel_guard);
            }
            None => registry.init(),
        }
        log_ring::install_panic_hook();
    }

    // Session: bring up the in-process engine (standalone daw + setlist

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

/// Window position, inner size, whether to go borderless fullscreen, and
/// whether to open maximized — each `None`/`false` meaning "let the platform
/// decide".
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
type WindowPlacement = (Option<(f64, f64)>, Option<(f64, f64)>, bool, bool);

/// Desktop: a frameless window — the app draws its own top bar (the
/// header doubles as title bar: drag surfaces + window controls).
/// Where the window opens, for multi-monitor desks and small laptops alike.
/// Placement is a *runtime*, per-machine concern (Dioxus.toml configures
/// bundling, not windows). Each setting is read from the environment first,
/// then from this machine's prefs (`$XDG_CONFIG_HOME/fts/<key>`, one value
/// per file), so a desk sets it once and every launch — `dx serve`
/// hot-reloads included — lands in the same place:
///
/// | env var | pref key | value |
/// |---|---|---|
/// | `FTS_WINDOW_POS` | `window-pos` | `"6560,0"` — top-left corner in desktop coordinates (`kscreen-doctor -o` on KDE prints each screen's geometry) |
/// | `FTS_WINDOW_SIZE` | `window-size` | `"1760x1100"` — inner size when not fullscreen |
/// | `FTS_WINDOW_FULLSCREEN` | `window-fullscreen` | `1` — borderless fullscreen on whichever monitor the position lands on |
/// | `FTS_WINDOW_MAXIMIZED` | `window-maximized` | `1` — open maximized: the screen's usable area, around the menu bar and Dock (`0` to opt out) |
///
/// With no size set, the window opens maximized on macOS — exactly the space
/// the screen has — and [`default_window_size`] is what it restores to.
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
fn window_placement() -> WindowPlacement {
    fn setting(var: &str, key: &str) -> Option<String> {
        std::env::var(var)
            .ok()
            .or_else(|| prefs::get(key))
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    }
    fn pair(var: &str, key: &str, sep: char) -> Option<(f64, f64)> {
        let raw = setting(var, key)?;
        let (a, b) = raw.split_once(sep)?;
        Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
    }
    let truthy = |v: String| v != "0" && !v.eq_ignore_ascii_case("false");
    let fullscreen = setting("FTS_WINDOW_FULLSCREEN", "window-fullscreen")
        .is_some_and(truthy);
    let size = pair("FTS_WINDOW_SIZE", "window-size", 'x');
    let maximized = setting("FTS_WINDOW_MAXIMIZED", "window-maximized")
        .map(truthy)
        .unwrap_or(cfg!(target_os = "macos") && size.is_none());
    (
        pair("FTS_WINDOW_POS", "window-pos", ','),
        size,
        fullscreen,
        maximized,
    )
}

/// 2560x1440, shrunk to fit the main screen where it can be measured.
///
/// The size matters beyond looks on macOS: a window larger than its screen
/// still renders every frame but never presents one, so the app sits on its
/// first frame forever ("Looking for the signal engine…" over a rig that is
/// running fine). Linux desks set their size explicitly, so it is left alone
/// there.
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
fn default_window_size() -> (f64, f64) {
    let (w, h): (f64, f64) = (2560.0, 1440.0);
    match main_screen_size() {
        // Room for the menu bar and a margin; never below the min size.
        Some((sw, sh)) => (w.min(sw - 40.0).max(720.0), h.min(sh - 80.0).max(480.0)),
        None => (w, h),
    }
}

/// The main display's size in points.
#[cfg(target_os = "macos")]
fn main_screen_size() -> Option<(f64, f64)> {
    #[repr(C)]
    struct CGRect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGMainDisplayID() -> u32;
        fn CGDisplayBounds(display: u32) -> CGRect;
    }
    // SAFETY: plain CoreGraphics queries with no preconditions.
    let bounds = unsafe { CGDisplayBounds(CGMainDisplayID()) };
    (bounds.w > 0.0 && bounds.h > 0.0).then_some((bounds.w, bounds.h))
}

#[cfg(not(any(target_os = "macos", target_arch = "wasm32", target_os = "ios")))]
fn main_screen_size() -> Option<(f64, f64)> {
    None
}

/// The window, on Blitz — the renderer that can be handed a painted scene.
///
/// `dioxus_native::launch_cfg` is Blitz → Vello → winit. The alternative,
/// `dioxus::desktop::LaunchBuilder`, puts WebKit/WRY behind the same
/// components and renders them through a different engine entirely — and a
/// WebView cannot host a custom widget at all: dioxus panics on the first DOM
/// mutation carrying an `Any` attribute and takes the window down. Since every
/// processor effect editor is a vello-painted custom widget, this is the only
/// desktop path on which the real editors exist.
#[cfg(all(
    not(any(target_arch = "wasm32", target_os = "ios")),
    not(feature = "webview")
))]
fn launch_app() {
    use dioxus_native::{Config, LogicalSize, WindowAttributes, launch_cfg};
    let (pos, size, fullscreen, maximized) = window_placement();
    // 2560x1440 by default. The rig's control surface is a wall of panels —
    // the drive board, three module panels, the time section, the footswitch
    // grid — and at 1280 they are all legible and none of them is usable.
    // `FTS_WINDOW_SIZE=WxH` still wins, and the window is resizable; this is
    // only where it starts.
    let (w, h) = size.unwrap_or_else(default_window_size);
    let mut window = WindowAttributes::default()
        .with_title("FastTrackStudio")
        .with_decorations(false)
        .with_surface_size(LogicalSize::new(w, h))
        .with_min_surface_size(LogicalSize::new(720.0, 480.0))
        .with_maximized(maximized && !fullscreen);
    // Position first: borderless fullscreen picks the monitor the window is
    // on, so placing it inside the target screen is what selects that screen.
    if let Some((x, y)) = pos {
        window = window.with_position(dioxus_native::winit::dpi::LogicalPosition::new(x, y));
    }
    if fullscreen {
        window = window.with_fullscreen(Some(
            dioxus_native::winit::monitor::Fullscreen::Borderless(None),
        ));
    }
    launch_cfg(
        App,
        vec![],
        vec![Box::new(Config::new().with_window_attributes(window))],
    );
    // The event loop returned (last window closed) — reap the engine we
    // spawned so it doesn't outlive the app. The engine's own watchdog is the
    // backstop for exits that never reach here (SIGKILL, crash).
    #[cfg(feature = "signal")]
    engines::shutdown();
}

/// The same window on WebKit/WRY — the escape hatch, painted surfaces absent.
#[cfg(all(
    not(any(target_arch = "wasm32", target_os = "ios")),
    feature = "webview"
))]
fn launch_app() {
    use dioxus::desktop::tao::dpi::{LogicalPosition, LogicalSize};
    use dioxus::desktop::tao::window::Fullscreen;
    use dioxus::desktop::{Config, WindowBuilder};
    let (pos, size, fullscreen, maximized) = window_placement();
    let mut window = WindowBuilder::new()
        .with_title("FastTrackStudio")
        .with_decorations(false)
        .with_inner_size({
            let (w, h) = size.unwrap_or_else(default_window_size);
            LogicalSize::new(w, h)
        })
        .with_min_inner_size(LogicalSize::new(720.0, 480.0))
        .with_maximized(maximized && !fullscreen);
    if let Some((x, y)) = pos {
        window = window.with_position(LogicalPosition::new(x, y));
    }
    if fullscreen {
        window = window.with_fullscreen(Some(Fullscreen::Borderless(None)));
    }
    dioxus::LaunchBuilder::new()
        .with_cfg(Config::new().with_window(window).with_menu(None))
        .launch(App);
    #[cfg(feature = "signal")]
    engines::shutdown();
}

#[cfg(target_arch = "wasm32")]
fn launch_app() {
    // Additive branch: a `/rigs/keys/{profile}` URL launches the browser
    // keys rig instead of the app shell.
    #[cfg(feature = "signal")]
    if web_keys_rig::route_matches() {
        dioxus::launch(web_keys_rig::KeysWebRig);
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
    #[cfg(feature = "signal")]
    Signal,
}

impl Workspace {
    fn all() -> Vec<(Self, &'static str)> {
        vec![
            #[cfg(feature = "signal")]
            (Self::Signal, "Signal"),
        ]
    }

    /// Where the app lands when nothing is remembered: the first workspace
    /// this build has. `None` only when no domain feature is compiled in.
    fn first() -> Option<Self> {
        Self::all().first().map(|(w, _)| *w)
    }

    fn label(self) -> &'static str {
        Self::all()
            .into_iter()
            .find(|(w, _)| *w == self)
            .map_or("?", |(_, l)| l)
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
    load_last_workspace().or_else(Workspace::first)
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
    // Two flags for running the real rig without consequences — see
    // `signal_guitar::library` for what they do. Translated to environment
    // here rather than threaded through as arguments because the rig core is
    // reached by more than one path (this app, `--engine`, the benchmark
    // examples) and all of them should honour the same switch.
    let silent = args.iter().any(|a| a == "--silent");
    let ephemeral = args.iter().any(|a| a == "--ephemeral" || a == "--no-save");
    // Design mode: the UI over a synthesised rig — no device, no MIDI, no DSP,
    // so several copies run side by side. See `signal_guitar::library`.
    let design = args.iter().any(|a| a == "--design" || a == "--mock");
    // SAFETY: single-threaded, before any GUI or thread init.
    unsafe {
        if let Some(w) = workspace {
            std::env::set_var("FTS_OPEN_WORKSPACE", w);
        }
        if let Some(r) = rig {
            std::env::set_var("FTS_OPEN_RIG", r);
        }
        if silent {
            std::env::set_var("SIGNAL_RIG_SILENT", "1");
        }
        if ephemeral {
            std::env::set_var("SIGNAL_RIG_EPHEMERAL", "1");
        }
        if design {
            std::env::set_var("SIGNAL_RIG_DESIGN", "1");
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
    let here = current().or_else(Workspace::first);

    // The workspace crumb carries every other workspace as its menu, so the
    // rail's icons are never the only way to change place.
    level.crumbs(
        here.map(|here| {
            vec![
                fts_chrome::Crumb::here(here.label()).with_menu(
                    Workspace::all()
                        .into_iter()
                        .map(|(w, label)| {
                            (
                                label.to_string(),
                                w == here,
                                Callback::new(move |()| go.call(w)),
                            )
                        })
                        .collect(),
                ),
            ]
        })
        .unwrap_or_default(),
    );
    // No side rails: switching workspace or rig is rare, and both already
    // live in the top bar (the "Signal ▾" and "Guitar ▾" crumb menus). The
    // one app-level flyout, Settings (engine status included), opens from
    // the gear beside the window controls.
    level.panels(vec![
        fts_chrome::PanelSpec::new("settings", "Settings", fts_chrome::Icon::Settings).width(300),
    ]);

    // The window's own controls, for whichever bar is showing: the app's, or
    // the one a view draws when it claims the bar (fts_chrome::use_bar_claim
    // — the guitar rig does, so there is one bar, not the app's over the
    // rig's). Desktop only; the browser and phone draw their own chrome.
    #[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
    fts_chrome::provide_window_actions(fts_chrome::WindowActions {
        drag: Callback::new(move |()| drag_window()),
        expand: Callback::new(move |()| toggle_maximize()),
        minimize: Callback::new(move |()| chrome::minimize()),
        maximize: Callback::new(move |()| toggle_maximize()),
        close: Callback::new(move |()| chrome::close()),
        settings: Some(Callback::new(move |()| chrome.toggle_panel("settings"))),
    });
    let bar_claimed = chrome.bar_claimed();

    rsx! {
        // Global reset: the frameless WebView keeps the platform's default 8px
        // body margin + white page background, which shows as a white border
        // around the dark 100vh app. Zero it and paint the page dark.
        document::Style { {"html,body{margin:0;padding:0;height:100%;background:#0a0a0a;overflow:hidden;}*{box-sizing:border-box;}"} }
        ResizeHandles {}
        PlatformMenu {}
        // ONE bar over two rails (fts_chrome::AppFrame). The bar is also the
        // title bar — the native decorations are off on desktop, so its slack
        // is the drag surface and the window controls sit at its right end.
        fts_chrome::AppFrame {
            top: rsx! {
                if !bar_claimed {
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
                    trailing: rsx! {
                        // No window shell there, so no WindowCluster — but
                        // settings still needs a way in.
                        if cfg!(any(target_arch = "wasm32", target_os = "ios")) {
                            fts_chrome::WindowButton {
                                icon: fts_chrome::Icon::Settings,
                                title: "Settings".to_string(),
                                on_click: move |()| chrome.toggle_panel("settings"),
                            }
                        }
                        fts_chrome::WindowCluster {}
                    },
                    on_drag: move |()| drag_window(),
                    on_expand: move |()| toggle_maximize(),
                }
                }
            },
            rail: rsx! {},
            // App-level flyouts. Views render their own inside their layout.
            panel: rsx! {
                fts_chrome::PanelHost { id: "settings".to_string(), SettingsPanel {} }
            },
            main { style: "flex: 1; min-width: 0; min-height: 0; display: flex;",
                match here {
                    // No domain feature compiled in — there is nowhere to go.
                    None => rsx! {
                        Placeholder {
                            title: "Nothing built",
                            body: "This binary has no domain features compiled in.",
                        }
                    },
                    #[cfg(feature = "signal")]
                    Some(Workspace::Signal) => rsx! {
                        // Rig picker → the chosen rig's remote over vox —
                        // the same surface as the web remote, pointed at
                        // the (supervised or remote) signal engine.
                        rig_view::SignalWorkspace {}
                    },
                }
            }
        }
    }
}

// ── Custom window chrome (desktop is frameless) ─────────────────────────────

/// The frameless window's own chrome, against whichever renderer is hosting it.
///
/// Blitz hands out a `winit` window; WRY hands out its own. They agree on what
/// these five gestures mean and disagree on every name, so the difference is
/// confined here rather than at each button.
///
/// `winit` has no `close()` — a window closes when the event loop stops owning
/// it — so the close button exits the process after the engine has been reaped,
/// which is what the WRY path's `close()` amounted to anyway.
#[cfg(all(
    not(any(target_arch = "wasm32", target_os = "ios")),
    not(feature = "webview")
))]
mod chrome {
    pub use dioxus_native::winit::window::ResizeDirection as Dir;

    /// The host window. A hook, so every caller must be inside a component —
    /// which they are: all five of these run from a title-bar button.
    fn window() -> std::sync::Arc<dyn dioxus_native::winit::window::Window> {
        dioxus_native::use_window()
    }

    pub fn drag() {
        let _ = window().drag_window();
    }

    pub fn toggle_maximize() {
        let w = window();
        let maximized = w.is_maximized();
        w.set_maximized(!maximized);
    }

    pub fn minimize() {
        window().set_minimized(true);
    }

    pub fn resize(dir: Dir) {
        let _ = window().drag_resize_window(dir);
    }

    pub fn close() {
        #[cfg(feature = "signal")]
        crate::engines::shutdown();
        std::process::exit(0);
    }
}

#[cfg(all(
    not(any(target_arch = "wasm32", target_os = "ios")),
    feature = "webview"
))]
mod chrome {
    pub use dioxus::desktop::tao::window::ResizeDirection as Dir;

    pub fn drag() {
        dioxus::desktop::window().drag();
    }
    pub fn toggle_maximize() {
        dioxus::desktop::window().toggle_maximized();
    }
    pub fn minimize() {
        dioxus::desktop::window().set_minimized(true);
    }
    pub fn resize(dir: Dir) {
        let _ = dioxus::desktop::window().drag_resize_window(dir);
    }
    pub fn close() {
        dioxus::desktop::window().close();
    }
}

fn drag_window() {
    #[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
    chrome::drag();
}

fn toggle_maximize() {
    #[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
    chrome::toggle_maximize();
}

/// The OS menu bar's share of the app: on macOS, the audio readout and
/// Settings… ⌘, (see `mac_menu`). Nothing elsewhere.
#[component]
fn PlatformMenu() -> Element {
    #[cfg(all(target_os = "macos", feature = "signal-guitar"))]
    {
        let chrome = fts_chrome::use_chrome();
        rsx! {
            mac_menu::MacMenuBar { on_settings: move |()| chrome.toggle_panel("settings") }
        }
    }
    #[cfg(not(all(target_os = "macos", feature = "signal-guitar")))]
    {
        rsx! {}
    }
}

/// Invisible edge/corner strips that restore native-feeling resize on
/// the frameless window (decorations off also removes the compositor's
/// resize borders). Corners render after edges so they win the hit test.
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
#[component]
fn ResizeHandles() -> Element {
    use chrome::Dir;
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
                    chrome::resize(dir);
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
    #[expect(unused_mut)]
    let mut update_msg = use_signal(String::new);

    rsx! {
        div {
            style: "display: flex; flex-direction: column; align-items: flex-start; gap: 12px; \
                    padding: 12px; font-size: 12px;",
            span { style: "color: #a1a1aa;", "Signal v{env!(\"CARGO_PKG_VERSION\")}" }
            EnginesArea {}
            EngineModeSetting {}
            UpdateCheck { msg: update_msg }
            if !update_msg().is_empty() {
                span { style: "color: #a1a1aa;", "{update_msg}" }
            }
        }
    }
}

/// Where the rig's audio runs. Embedded is the default because opening a rig
/// should make a sound; supervised is for when the rig must outlive this
/// window — a live set where the GUI may be closed or crash, or a remote
/// engine driven from another machine.
///
/// Takes effect the next time a rig is opened rather than instantly: moving a
/// running rig between engines mid-note would glitch the audio, and the choice
/// is not one anyone makes mid-song.
#[cfg(all(feature = "signal", not(target_arch = "wasm32")))]
#[component]
fn EngineModeSetting() -> Element {
    use rig_view::EngineMode;
    let mut mode = use_signal(EngineMode::current);

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 5px;",
            span { style: "color: #71717a; font-size: 11px;", "Rig engine" }
            div { style: "display: flex; gap: 6px;",
                for (m, label) in [
                    (EngineMode::Embedded, "In this app"),
                    (EngineMode::Supervised, "Separate engine"),
                ] {
                    button {
                        key: "{label}",
                        style: if mode() == m {
                            "padding: 3px 10px; border-radius: 5px; background: #10283f; color: #7dd3fc; border: 1px solid #38bdf8; font-size: 11px;"
                        } else {
                            "padding: 3px 10px; border-radius: 5px; background: transparent; color: #a1a1aa; border: 1px solid #27272a; font-size: 11px;"
                        },
                        onclick: move |_| { m.store(); mode.set(m); },
                        "{label}"
                    }
                }
            }
            span { style: "color: #52525b; font-size: 10px;",
                if mode() == EngineMode::Embedded {
                    "Plays here. Stops when the app closes."
                } else {
                    "Runs as its own process, and keeps playing without the window."
                }
            }
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

/// The web build is always a remote, so there is nothing to choose.
#[cfg(not(all(feature = "signal", not(target_arch = "wasm32"))))]
#[component]
fn EngineModeSetting() -> Element {
    rsx! {}
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

#[component]
fn Placeholder(title: &'static str, body: &'static str) -> Element {
    rsx! {
        div { style: "display: flex; flex-direction: column; align-items: center; gap: 8px; max-width: 480px; text-align: center; margin: auto;",
            span { style: "font-size: 20px; font-weight: 700;", "{title}" }
            span { style: "font-size: 13px; color: #a1a1aa;", "{body}" }
        }
    }
}
