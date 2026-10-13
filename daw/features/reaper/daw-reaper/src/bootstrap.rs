//! Plugin-side REAPER bootstrap types.
//!
//! Re-exports the reaper-rs types needed for CLAP plugins to initialize
//! REAPER API access via `ReaperPluginEntry`. This follows the Helgobox
//! pattern where each .dylib gets its own copy of Rust statics and needs
//! its own initialized `Reaper`, `TaskSupport`, and `daw-reaper` setup.
//!
//! # Usage
//!
//! ```rust,ignore
//! use daw::reaper::bootstrap::*;
//!
//! #[no_mangle]
//! pub unsafe extern "C" fn ReaperPluginEntry(
//!     h_instance: HINSTANCE,
//!     rec: *mut reaper_plugin_info_t,
//! ) -> std::os::raw::c_int {
//!     let ctx = static_plugin_context();
//!     bootstrap_extension_plugin(h_instance, rec, ctx, plugin_init)
//! }
//!
//! fn plugin_init(context: PluginContext) -> Result<(), Box<dyn Error>> {
//!     HighReaper::load(context).setup()?;
//!     // ...
//! }
//! ```

// reaper-low: raw FFI types for ReaperPluginEntry signature and direct API access
pub use reaper_low::Reaper as LowReaper;
pub use reaper_low::raw::HINSTANCE;
pub use reaper_low::raw::MediaItem_Take;
pub use reaper_low::raw::MediaTrack;
pub use reaper_low::raw::reaper_plugin_info_t;
pub use reaper_low::{PluginContext, bootstrap_extension_plugin, static_plugin_context};

// reaper-high: initialization, main-thread dispatch, and project/track access
pub use reaper_high::{
    MainTaskMiddleware, MainThreadTask, Reaper as HighReaper, TaskSupport, Track as HighTrack,
};

// reaper-medium: session registration for timer callbacks and play state
pub use reaper_medium::{ProjectContext as ReaperProjectContext, ReaperSession};

/// Initialize daw-reaper from a CLAP host context.
///
/// CLAP plugins call this during `initialize()` with the raw `clap_host*`
/// obtained from `InitContext::raw_host_context()`. This:
///
/// 1. Calls `host->get_extension("cockos.reaper_extension")` to get `reaper_plugin_info_t*`
/// 2. Builds a `PluginContext` from it
/// 3. Initializes reaper-high (`HighReaper`)
/// 4. Sets up `TaskSupport` and `MainTaskMiddleware` for main-thread dispatch
/// 5. Registers a timer callback for periodic work
///
/// Returns a [`ReaperSession`] that can be used to register timer callbacks,
/// or `None` if REAPER API is not available (e.g. running in a non-REAPER host).
///
/// # Safety
///
/// `raw_clap_host` must be a valid `*const clap_host` pointer from the CLAP host.
pub unsafe fn init_from_clap_host(
    raw_clap_host: *const std::ffi::c_void,
) -> Option<ReaperBootstrap> {
    if raw_clap_host.is_null() {
        return None;
    }

    // Cast to clap_host and call get_extension
    #[repr(C)]
    struct ClapHost {
        clap_version: [u32; 3],
        host_data: *mut std::ffi::c_void,
        name: *const std::ffi::c_char,
        vendor: *const std::ffi::c_char,
        url: *const std::ffi::c_char,
        version: *const std::ffi::c_char,
        get_extension: Option<
            unsafe extern "C" fn(
                host: *const ClapHost,
                extension_id: *const std::ffi::c_char,
            ) -> *const std::ffi::c_void,
        >,
        request_restart: Option<unsafe extern "C" fn(host: *const ClapHost)>,
        request_process: Option<unsafe extern "C" fn(host: *const ClapHost)>,
        request_callback: Option<unsafe extern "C" fn(host: *const ClapHost)>,
    }

    let host = raw_clap_host as *const ClapHost;
    let get_ext = unsafe { (*host).get_extension }?;

    let ext_id = b"cockos.reaper_extension\0";
    let rec_ptr = unsafe { get_ext(host, ext_id.as_ptr() as *const std::ffi::c_char) };
    if rec_ptr.is_null() {
        return None; // Not running in REAPER
    }

    let rec = unsafe { *(rec_ptr as *const reaper_plugin_info_t) };

    // Build PluginContext from the reaper_plugin_info_t
    let static_ctx = static_plugin_context();
    let h_instance = static_ctx.h_instance;
    let context = match unsafe { PluginContext::from_extension_plugin(h_instance, rec, static_ctx) }
    {
        Ok(c) => c,
        Err(_) => return None,
    };

    // Initialize reaper-high
    let _ = HighReaper::load(context).setup();

    // Set up TaskSupport for main-thread dispatch
    let (task_sender, task_receiver) = crossbeam_channel::unbounded();
    let task_support = TaskSupport::new(task_sender.clone());
    let middleware = MainTaskMiddleware::new(task_sender, task_receiver);

    // Set TaskSupport for daw-reaper
    let task_support_ref: &'static TaskSupport = Box::leak(Box::new(task_support));
    crate::set_task_support(task_support_ref);

    // Create a session for timer registration
    let session = ReaperSession::load(context);

    Some(ReaperBootstrap {
        middleware,
        session,
    })
}

/// Result of [`init_from_clap_host`]. Holds the resources needed for
/// REAPER main-thread dispatch and timer registration.
pub struct ReaperBootstrap {
    /// Drains main-thread tasks — call `run()` from your timer callback.
    pub middleware: MainTaskMiddleware,
    /// Session for registering timer callbacks with REAPER.
    pub session: ReaperSession,
}

// ── Plugin DAW initialization ────────────────────────────────────────

use crate::local_caller::LocalCaller;
use daw_control::Daw;

/// Create a `Daw` instance from a raw CLAP host pointer.
///
/// Called by `daw::init()` — not meant to be used directly by plugins.
/// Returns `(Daw, runtime)` or `None` if not running in REAPER.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn create_plugin_daw(
    raw_host_ptr: *const std::ffi::c_void,
) -> Option<(Daw, std::sync::Arc<tokio::runtime::Runtime>)> {
    let mut bootstrap = unsafe { init_from_clap_host(raw_host_ptr) }?;

    // Register internal timer for main-thread task draining
    static MIDDLEWARE: std::sync::OnceLock<std::sync::Mutex<MainTaskMiddleware>> =
        std::sync::OnceLock::new();
    let _ = MIDDLEWARE.set(std::sync::Mutex::new(bootstrap.middleware));

    extern "C" fn internal_timer() {
        let mw = unsafe { &*std::ptr::addr_of!(MIDDLEWARE) };
        if let Some(m) = mw.get()
            && let Ok(mut mw) = m.lock()
        {
            mw.run();
        }
        // Fire user timer callbacks registered via daw::register_timer()
        let cbs = unsafe { &*std::ptr::addr_of!(USER_TIMER_CALLBACKS) };
        if let Ok(callbacks) = cbs.lock() {
            for cb in callbacks.iter() {
                cb();
            }
        }
    }

    let _ = bootstrap.session.plugin_register_add_timer(internal_timer);
    let _ = Box::leak(Box::new(bootstrap.session));

    // Create the handler on the main thread (needs REAPER statics),
    // then spawn a thread for LocalCaller creation (needs a clean runtime).
    let handler = crate::plugin_services::create_daw_handler();

    let result = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .ok()?;

        let daw = runtime.block_on(async {
            let local = LocalCaller::new(handler).await.ok()?;
            let caller = local.caller();
            let _ = Box::leak(Box::new(local)); // Keep server alive
            Some(Daw::new(caller))
        })?;

        Some((daw, std::sync::Arc::new(runtime)))
    })
    .join()
    .ok()??;

    Some(result)
}

/// Register an additional timer callback (called by `daw::init`).
pub fn register_internal_timer(callback: fn()) {
    if let Ok(mut cbs) = USER_TIMER_CALLBACKS.lock() {
        cbs.push(callback);
    }
}

static USER_TIMER_CALLBACKS: std::sync::Mutex<Vec<fn()>> = std::sync::Mutex::new(Vec::new());

// ── Extension DAW initialization ─────────────────────────────────────────────

/// Create an in-process `Daw` for a standalone REAPER extension.
///
/// Unlike [`create_plugin_daw`] (which bootstraps REAPER from a CLAP host
/// pointer), this function assumes REAPER is already initialized — i.e. you
/// are inside a `#[reaper_extension_plugin]` entry point and have already
/// called:
///
/// ```rust,ignore
/// HighReaper::load(context).setup()?;
/// daw::reaper::set_task_support(&task_support);
/// ```
///
/// It wires all REAPER service implementations into a [`LocalCaller`]
/// (in-process vox memory channels) and returns the standard async [`Daw`]
/// handle. No SHM, no sockets, no dependency on daw-bridge.
///
/// The `LocalCaller` server task is leaked so it lives for the process
/// lifetime — call this once per extension.
///
/// # Example
///
/// ```rust,ignore
/// // Inside plugin_main / extension entry:
/// let daw = tokio_runtime.block_on(daw::reaper::build_extension_daw())?;
/// ```
pub async fn build_extension_daw() -> eyre::Result<Daw> {
    build_extension_daw_with(crate::plugin_services::create_daw_handler()).await
}

/// Like [`build_extension_daw`], but accepts an externally-built
/// `architect::LayerRouter` so consumers can layer their own dispatchers on
/// top of the standard daw services.
///
/// Typical use from an extension that wants to host non-daw services
/// (e.g. `keyflow-daw-analysis::MidiChartService`) on the same in-process
/// channel as the rest of the daw API:
///
/// ```rust,ignore
/// use daw_reaper::plugin_services::create_daw_handler;
///
/// let handler = create_daw_handler()
///     .with(midi_chart_service_descriptor(),
///           MidiChartServiceDispatcher::new(KeyflowMidiAnalysis::new(daw)));
/// let daw = tokio_runtime.block_on(
///     daw::reaper::build_extension_daw_with(handler)
/// )?;
/// ```
pub async fn build_extension_daw_with(handler: architect::LayerRouter) -> eyre::Result<Daw> {
    let local = LocalCaller::new(handler).await?;
    let caller = local.caller();
    // Leak LocalCaller to keep the server-side tokio task alive for the
    // process lifetime. The extension owns the process — this is intentional.
    let _ = Box::leak(Box::new(local));

    // Install it as the process-wide `Daw` as well as returning it.
    //
    // A Dioxus panel in this same process reaches the DAW through
    // `Daw::try_get()` — it is a component, with no way to be handed an
    // instance the extension happens to be holding. Without this the panel
    // mounts, renders, and sits on "Waiting for DAW connection…" forever
    // while a perfectly good `Daw` exists a few frames away.
    //
    // Already-initialised is not an error: a host that called `Daw::init`
    // itself keeps its own, and this returns the instance either way.
    let _ = Daw::init(caller.clone());
    Ok(Daw::new(caller))
}
