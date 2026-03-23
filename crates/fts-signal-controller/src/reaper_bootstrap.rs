//! REAPER bootstrap for the fts-signal-controller CLAP plugin.
//!
//! Exports `ReaperPluginEntry` so the REAPER extension loader can eagerly load
//! this .so/.dylib and initialize REAPER API access within the plugin's address
//! space. This follows the Helgobox / fts-macros pattern.
//!
//! # Flow
//!
//! 1. Extension calls `ReaperPluginEntry` on this .dylib during REAPER startup
//! 2. We initialize reaper-rs, TaskSupport, daw-reaper, and register a timer
//! 3. The timer callback runs at ~30Hz on REAPER's main thread
//! 4. Scene switching is handled in the timer (reads timeline, mutes/unmutes sends)

use crossbeam_channel::{Receiver, Sender};
use daw::reaper::bootstrap::*;
use std::error::Error;
use std::sync::{Mutex, OnceLock};
use tracing::info;

use crate::scene_timer;

/// Bootstrap state, initialized by `ReaperPluginEntry`.
struct Bootstrap {
    _task_support: TaskSupport,
}

static BOOTSTRAP: OnceLock<Bootstrap> = OnceLock::new();

/// Task middleware for draining main-thread tasks in the timer callback.
static MIDDLEWARE: OnceLock<Mutex<MainTaskMiddleware>> = OnceLock::new();

/// Returns true if the REAPER bootstrap completed successfully.
pub fn is_bootstrapped() -> bool {
    BOOTSTRAP.get().is_some()
}

/// Timer callback registered with REAPER (~30Hz on main thread).
extern "C" fn plugin_timer_callback() {
    // 1. Drain main-thread tasks (keeps daw-reaper responsive)
    if let Some(m) = MIDDLEWARE.get() {
        if let Ok(mut mw) = m.lock() {
            mw.run();
        }
    }

    // 2. Scene switching: read playhead, check timeline, mute/unmute sends
    scene_timer::poll();
}

/// Called by the REAPER extension during eager load.
///
/// # Safety
///
/// `rec` must be a valid pointer to `reaper_plugin_info_t` or null.
/// `h_instance` is the DLL module handle.
#[no_mangle]
pub unsafe extern "C" fn ReaperPluginEntry(
    h_instance: HINSTANCE,
    rec: *mut reaper_plugin_info_t,
) -> std::os::raw::c_int {
    let static_context = static_plugin_context();
    bootstrap_extension_plugin(h_instance, rec, static_context, plugin_init)
}

/// Plugin initialization — called after validating the `PluginContext`.
fn plugin_init(context: PluginContext) -> Result<(), Box<dyn Error>> {
    // Set up tracing to a plugin-specific log file
    let log_file = std::fs::File::create("/tmp/fts-signal-controller-bootstrap.log")
        .expect("Failed to create bootstrap log");
    tracing_subscriber::fmt()
        .with_writer(std::sync::Mutex::new(log_file))
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::DEBUG.into()),
        )
        .init();

    info!("FTS Signal Controller: ReaperPluginEntry called, initializing...");

    // 1. Initialize reaper-high in this .dylib's address space
    match HighReaper::load(context).setup() {
        Ok(_) => info!("FTS Signal Controller: reaper-high initialized"),
        Err(_) => info!("FTS Signal Controller: reaper-high already initialized"),
    }

    // 2. Create TaskSupport channels for main-thread dispatch
    let (task_sender, task_receiver): (Sender<MainThreadTask>, Receiver<MainThreadTask>) =
        crossbeam_channel::unbounded();
    let task_support = TaskSupport::new(task_sender.clone());

    // 3. Create and store the task middleware (for timer callback)
    let middleware = MainTaskMiddleware::new(task_sender, task_receiver);
    MIDDLEWARE
        .set(Mutex::new(middleware))
        .map_err(|_| "Task middleware already initialized")?;

    // 4. Register timer callback (~30Hz on REAPER's main thread)
    let mut session = ReaperSession::load(context);
    session.plugin_register_add_timer(plugin_timer_callback)?;
    let _ = Box::leak(Box::new(session));
    info!("FTS Signal Controller: timer callback registered");

    // 5. Set TaskSupport for daw-reaper (this .dylib's own copy)
    let task_support_ref: &'static TaskSupport = Box::leak(Box::new(task_support));
    daw::reaper::set_task_support(task_support_ref);
    info!("FTS Signal Controller: daw-reaper TaskSupport configured");

    // 6. Store bootstrap state
    let (dummy_sender, _) = crossbeam_channel::unbounded();
    BOOTSTRAP
        .set(Bootstrap {
            _task_support: TaskSupport::new(dummy_sender),
        })
        .map_err(|_| "Bootstrap already initialized")?;

    info!("FTS Signal Controller: REAPER bootstrap complete");
    Ok(())
}
