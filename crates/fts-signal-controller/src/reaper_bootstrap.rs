//! REAPER bootstrap for the fts-signal-controller CLAP plugin.
//!
//! Called by daw-bridge's eager loader via `FtsReaperInit`. Initializes
//! reaper-rs, registers a ~30Hz timer callback for scene switching and
//! macro mapping.

use crossbeam_channel::{Receiver, Sender};
use daw::reaper::bootstrap::*;
use std::error::Error;
use std::sync::{Mutex, OnceLock};
use tracing::info;

#[cfg(feature = "macro-timer")]
use crate::macro_timer;
#[cfg(feature = "scene-timer")]
use crate::scene_timer;

/// Bootstrap state.
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
    if let Some(m) = MIDDLEWARE.get() {
        if let Ok(mut mw) = m.lock() {
            mw.run();
        }
    }

    #[cfg(feature = "scene-timer")]
    scene_timer::poll();

    #[cfg(feature = "macro-timer")]
    macro_timer::poll();
}

/// Called by daw-bridge's eager loader (via `dlopen` + symbol lookup).
///
/// Named `FtsReaperInit` instead of `ReaperPluginEntry` so REAPER's CLAP
/// scanner doesn't treat this as a REAPER extension.
#[no_mangle]
pub unsafe extern "C" fn FtsReaperInit(
    h_instance: HINSTANCE,
    rec: *mut reaper_plugin_info_t,
) -> std::os::raw::c_int {
    let static_context = static_plugin_context();
    bootstrap_extension_plugin(h_instance, rec, static_context, plugin_init)
}

fn plugin_init(context: PluginContext) -> Result<(), Box<dyn Error>> {
    let log_file = std::fs::File::create("/tmp/fts-signal-controller-bootstrap.log")
        .expect("Failed to create bootstrap log");
    tracing_subscriber::fmt()
        .with_writer(std::sync::Mutex::new(log_file))
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::DEBUG.into()),
        )
        .init();

    info!("FTS Signal Controller: FtsReaperInit called, initializing...");

    match HighReaper::load(context).setup() {
        Ok(_) => info!("FTS Signal Controller: reaper-high initialized"),
        Err(_) => info!("FTS Signal Controller: reaper-high already initialized"),
    }

    let (task_sender, task_receiver): (Sender<MainThreadTask>, Receiver<MainThreadTask>) =
        crossbeam_channel::unbounded();
    let task_support = TaskSupport::new(task_sender.clone());

    let middleware = MainTaskMiddleware::new(task_sender, task_receiver);
    MIDDLEWARE
        .set(Mutex::new(middleware))
        .map_err(|_| "Task middleware already initialized")?;

    let mut session = ReaperSession::load(context);
    session.plugin_register_add_timer(plugin_timer_callback)?;
    let _ = Box::leak(Box::new(session));
    info!("FTS Signal Controller: timer callback registered");

    let task_support_ref: &'static TaskSupport = Box::leak(Box::new(task_support));
    daw::reaper::set_task_support(task_support_ref);
    info!("FTS Signal Controller: daw-reaper TaskSupport configured");

    let (dummy_sender, _) = crossbeam_channel::unbounded();
    BOOTSTRAP
        .set(Bootstrap {
            _task_support: TaskSupport::new(dummy_sender),
        })
        .map_err(|_| "Bootstrap already initialized")?;

    info!("FTS Signal Controller: REAPER bootstrap complete");
    Ok(())
}
