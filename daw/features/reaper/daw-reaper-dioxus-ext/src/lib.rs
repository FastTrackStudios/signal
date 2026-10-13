//! reaper-dioxus extension — REAPER plugin providing Dioxus rendering as a service.
//!
//! Loaded as a cdylib (`reaper_daw_dioxus.so` on Linux). On startup it:
//!
//! 1. Initializes REAPER + SWELL low-level globals.
//! 2. Builds a `daw-extension-runtime::ExtensionRuntime` (tokio runtime,
//!    main-thread task queue, in-process daw service).
//! 3. Registers the FTS UI Native + Desktop demo actions and panels shipped
//!    from `daw_ui::test_panels`.
//! 4. Drives `daw_reaper_dioxus::service::tick()` from the REAPER timer.
//! 5. Bumps REAPER's misc timer (id 666) to 60Hz so the renderer cadence
//!    matches a typical UI frame budget. See issue #17 / #11.
//!
//! Other extensions can also call `daw_reaper_dioxus::service::*` once we're
//! loaded — but for the demo path everything we need is in this binary.
//!
//! ## Installation
//!
//! Copy `reaper_daw_dioxus.so` into REAPER's `UserPlugins/` directory.

use std::cell::OnceCell;
use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, OnceLock};

use crossbeam_channel::{Receiver, Sender};
use daw::service::ActionEvent;
use daw_extension_runtime::{
    ActionDef as ExtActionDef, ActionRegistration, ExtensionRuntime, register_actions,
};
use fragile::Fragile;
use reaper_high::Reaper as HighReaper;
use reaper_low::{PluginContext, Reaper as LowReaper, Swell};
use reaper_macros::reaper_extension_plugin;
use tracing::{debug, info, warn};

/// REAPER's built-in misc timer id (the one driven by `plugin_register("timer", ...)`).
const MISC_TIMER_ID: usize = 666;
/// 60 Hz period in milliseconds.
const MISC_TIMER_60HZ_MS: u32 = 1000 / 60;

static LOW_REAPER: OnceLock<LowReaper> = OnceLock::new();
static LOW_SWELL: OnceLock<Swell> = OnceLock::new();

thread_local! {
    static APP: OnceCell<Fragile<App>> = const { OnceCell::new() };
}

struct App {
    runtime: ExtensionRuntime,
    /// Action handlers keyed by REAPER command name. Drained on the main
    /// thread via `process_pending_actions` so handlers can touch
    /// thread-local state (the dock panel registry) safely.
    handlers: HashMap<String, Arc<dyn Fn() + Send + Sync>>,
    /// Receive end of the action-name channel populated by the async
    /// `ActionEvent` dispatch task.
    pending_rx: Receiver<String>,
}

impl App {
    fn new(context: PluginContext) -> eyre::Result<Self> {
        let runtime = ExtensionRuntime::new(context)?;
        let daw = runtime.build_daw()?;

        // Pull the test-panel actions + panels from daw-ui. Build a lookup
        // map from command id → handler so the timer callback can dispatch
        // each fired action on the main thread (handlers touch the
        // thread-local PANELS registry — running them on a tokio worker
        // would silently no-op).
        let action_defs: Vec<daw_module::ActionDef> = daw_ui::test_panels::action_defs().into();
        let mut handlers: HashMap<String, Arc<dyn Fn() + Send + Sync>> = HashMap::new();
        for def in &action_defs {
            handlers.insert(def.command_id.clone(), def.handler.clone());
        }

        let (pending_tx, pending_rx) = crossbeam_channel::unbounded::<String>();

        // daw-extension-runtime's ActionDef takes &'static str. Leak the
        // owned strings — there are only two and they live for the whole
        // REAPER session, so the leak is bounded.
        let ext_action_defs: Vec<ExtActionDef> = action_defs
            .iter()
            .map(|d| ExtActionDef {
                command_name: Box::leak(d.command_id.clone().into_boxed_str()),
                description: Box::leak(d.display_name.clone().into_boxed_str()),
                toggleable: d.toggleable,
            })
            .collect();

        let daw_for_register = daw.clone();
        runtime.spawn(async move {
            match register_actions(&daw_for_register, &ext_action_defs).await {
                Ok(ActionRegistration {
                    rx,
                    registered,
                    failed,
                }) => {
                    info!(registered, failed, "FTS UI test actions registered");
                    drive_action_events(rx, pending_tx).await;
                }
                Err(e) => warn!("FTS UI test action registration failed: {e}"),
            }
        });

        // Init dock + mount both panels. dock::init needs the low-level
        // globals, which the caller (plugin_main) already installed.
        daw_reaper_dioxus::service::init();
        daw_reaper_dioxus::dock::init(LowReaper::get(), Swell::get());
        for panel in daw_ui::test_panels::panel_defs() {
            daw_reaper_dioxus::dock::register_panel_from_service(&panel);
        }
        // Auto-restoring previously-open panels is disabled while the
        // WebKit2GTK + SWELL embedding is still flaky — reopening multiple
        // desktop webviews on launch makes input-grab issues worse and
        // doubles compositing load. Re-enable once #10/#11 land.

        Ok(Self {
            runtime,
            handlers,
            pending_rx,
        })
    }

    fn timer(&self) {
        // Everything below runs with this extension's tokio runtime in
        // context. A panel's futures are polled by dioxus's scheduler on
        // this thread, and the daw subscriptions they start call
        // `tokio::task::spawn` — which aborts the process, non-unwinding,
        // when no runtime is current. That took REAPER down with it the
        // first time a panel actually opened.
        let _guard = self.runtime.handle().enter();

        self.runtime.process_tasks();
        // Drain pending actions on the main thread — handlers touch the
        // thread-local dock panel registry, so they MUST run here, not
        // on the async task that received the ActionEvent.
        while let Ok(command_name) = self.pending_rx.try_recv() {
            if let Some(handler) = self.handlers.get(&command_name) {
                handler();
            } else {
                tracing::trace!(%command_name, "no handler for command");
            }
        }
        daw_reaper_dioxus::service::tick();
    }
}

async fn drive_action_events(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<ActionEvent>,
    tx: Sender<String>,
) {
    info!("FTS UI action dispatch loop started");
    while let Some(event) = rx.recv().await {
        // Plain destructuring, not `let…else`: `ActionEvent` has the one
        // variant, so a fallback arm is unreachable today. If a second
        // variant lands this stops compiling, which is the right prompt
        // to decide what the loop should do with it.
        let ActionEvent::Triggered { command_name } = event;
        debug!(%command_name, "action received");
        if tx.send(command_name).is_err() {
            info!("FTS UI dispatch channel closed");
            break;
        }
    }
    info!("FTS UI action stream closed");
}

extern "C" fn timer_callback() {
    APP.with(|cell| {
        if let Some(app) = cell.get() {
            app.get().timer();
        }
    });
}

#[reaper_extension_plugin]
fn plugin_main(context: PluginContext) -> Result<(), Box<dyn Error>> {
    init_tracing();
    info!("daw-reaper-dioxus-ext starting…");

    // Make low-level REAPER + SWELL globals available — required by
    // dock::init (uses Reaper::get() / Swell::get()) and by the 60Hz misc
    // timer override in this crate.
    let _ = LowReaper::make_available_globally(LowReaper::load(context));
    let _ = Swell::make_available_globally(Swell::load(context));
    let _ = LOW_REAPER.set(LowReaper::load(context));
    let _ = LOW_SWELL.set(Swell::load(context));

    if HighReaper::load(context).setup().is_ok() {
        info!("REAPER high-level API loaded");
        if let Err(e) = HighReaper::get().wake_up() {
            tracing::debug!("reaper wake_up: {e}");
        }
    }

    let app = App::new(context)?;
    app.runtime.add_timer(timer_callback)?;

    let stored = APP.with(|cell| cell.set(Fragile::new(app)).is_ok());
    if !stored {
        return Err("daw-reaper-dioxus-ext already initialized".into());
    }

    install_60hz_misc_timer();

    info!("daw-reaper-dioxus-ext loaded — both FTS UI panels available");
    Ok(())
}

/// Logs to a file *and* to stderr.
///
/// The file is what a human tails while REAPER runs. Stderr is what an
/// automated capture gets: a screenshot harness redirects REAPER's output
/// and would otherwise see nothing at all from this extension, which makes
/// "the panel did not open" indistinguishable from "the extension never
/// loaded". Both destinations, because they answer different questions.
///
/// The path follows `$XDG_STATE_HOME` like the other FTS extension rather
/// than being hardcoded under `/tmp`, and `FTS_DIOXUS_EXT_LOG` overrides it
/// outright — a harness running REAPER against a throwaway profile wants
/// the log somewhere it chose, not somewhere it has to go looking for.
fn init_tracing() {
    use tracing_subscriber::layer::{Layer, SubscriberExt};

    let path = std::env::var("FTS_DIOXUS_EXT_LOG")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| log_dir().join("daw-reaper-dioxus-ext.log"));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let filter = || {
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
    };
    let stderr = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_filter(filter());

    let registry = tracing_subscriber::registry().with(stderr);
    let _ = match std::fs::File::create(&path) {
        Ok(file) => {
            let to_file = tracing_subscriber::fmt::layer()
                .with_writer(std::sync::Mutex::new(file))
                .with_ansi(false)
                .with_filter(filter());
            tracing::subscriber::set_global_default(registry.with(to_file))
        }
        Err(_) => tracing::subscriber::set_global_default(registry),
    };
    info!(log = %path.display(), "tracing initialised");
}

/// `$XDG_STATE_HOME/fasttrackstudio`, falling back to `~/.local/state`.
fn log_dir() -> std::path::PathBuf {
    std::env::var("XDG_STATE_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::path::PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
                .join(".local/state")
        })
        .join("fasttrackstudio")
}

/// Bump REAPER's `MISC_TIMER` (id 666) on the main HWND from ~30Hz to 60Hz.
///
/// Mirrors the upstream `reaper_60fps` extension — register a transient
/// `"timer"` callback so REAPER's own MISC_TIMER is already running when
/// we kill+reset it.
fn install_60hz_misc_timer() {
    let Some(reaper) = LOW_REAPER.get() else {
        return;
    };
    unsafe {
        reaper.plugin_register(
            c"timer".as_ptr(),
            activate_60hz_misc_timer as *mut std::ffi::c_void,
        );
    }
}

extern "C" fn activate_60hz_misc_timer() {
    let Some(reaper) = LOW_REAPER.get() else {
        return;
    };
    let Some(swell) = LOW_SWELL.get() else {
        return;
    };

    // One-shot — deregister so this only runs once.
    unsafe {
        reaper.plugin_register(
            c"-timer".as_ptr(),
            activate_60hz_misc_timer as *mut std::ffi::c_void,
        );
    }

    let main_hwnd = reaper.GetMainHwnd();
    if main_hwnd.is_null() {
        warn!("60Hz misc timer: GetMainHwnd returned null; skipping override");
        return;
    }

    unsafe {
        swell.KillTimer(main_hwnd, MISC_TIMER_ID);
        swell.SetTimer(main_hwnd, MISC_TIMER_ID, MISC_TIMER_60HZ_MS, None);
    }
    info!(
        period_ms = MISC_TIMER_60HZ_MS,
        "REAPER misc timer (id 666) overridden to 60Hz"
    );
}
