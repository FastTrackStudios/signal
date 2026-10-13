//! REAPER extension for benchmarking DAW API performance.
//!
//! Compares three execution paths for bulk operations:
//!
//! 1. **Native reaper-rs** — direct C++ API calls on the main thread
//! 2. **Individual RPC** — one round-trip per operation via daw-bridge socket
//! 3. **Batch RPC** — single round-trip for N operations via BatchBuilder
//!
//! Install alongside daw-bridge. Set `FTS_PERF_TEST=1` to auto-run benchmarks
//! 5 seconds after REAPER starts. Results are written to `/tmp/daw-perf-test.log`.

mod bench;

use reaper_high::Reaper as HighReaper;
use reaper_low::PluginContext;
use reaper_macros::reaper_extension_plugin;
use reaper_medium::ReaperSession;
use std::cell::RefCell;
use std::error::Error;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{debug, info};

use crossbeam_channel::{Receiver, Sender};
use reaper_high::{MainTaskMiddleware, MainThreadTask, TaskSupport};

// ============================================================================
// Global State
// ============================================================================

static GLOBAL: OnceLock<Global> = OnceLock::new();

struct Global {
    task_support: TaskSupport,
    task_sender: Sender<MainThreadTask>,
    task_receiver: Receiver<MainThreadTask>,
}

impl Global {
    fn init() {
        GLOBAL.get_or_init(|| {
            let (task_sender, task_receiver) = crossbeam_channel::unbounded();
            Global {
                task_support: TaskSupport::new(task_sender.clone()),
                task_sender,
                task_receiver,
            }
        });
    }

    fn get() -> &'static Global {
        GLOBAL
            .get()
            .expect("Global not initialized — call Global::init() first")
    }

    fn task_support() -> &'static TaskSupport {
        &Global::get().task_support
    }

    fn create_task_middleware(&self) -> MainTaskMiddleware {
        MainTaskMiddleware::new(self.task_sender.clone(), self.task_receiver.clone())
    }
}

// ============================================================================
// Application State
// ============================================================================

#[derive(Debug)]
struct App {
    #[allow(dead_code)]
    session: RefCell<ReaperSession>,
    tokio_runtime: tokio::runtime::Runtime,
    task_middleware: RefCell<MainTaskMiddleware>,
}

impl App {
    fn new(session: ReaperSession) -> Result<Self, Box<dyn Error>> {
        let tokio_runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()?;

        Global::init();
        let task_middleware = Global::get().create_task_middleware();

        Ok(Self {
            session: RefCell::new(session),
            tokio_runtime,
            task_middleware: RefCell::new(task_middleware),
        })
    }

    fn process_tasks(&self) {
        self.task_middleware.borrow_mut().run();
    }
}

static APP_INSTANCE: OnceLock<fragile::Fragile<App>> = OnceLock::new();

fn get_app() -> Option<&'static fragile::Fragile<App>> {
    APP_INSTANCE.get()
}

// ============================================================================
// Timer Callback — process main-thread tasks + auto-trigger benchmarks
// ============================================================================

/// Number of timer ticks before auto-triggering benchmarks (~5s at 30Hz).
static TICK_COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static BENCH_STARTED: AtomicBool = AtomicBool::new(false);
static AUTO_RUN: AtomicBool = AtomicBool::new(false);

extern "C" fn timer_callback() {
    let _ = std::panic::catch_unwind(|| {
        if let Some(app) = get_app() {
            app.get().process_tasks();
        }

        // Auto-trigger benchmarks after ~5 seconds if FTS_PERF_TEST=1
        if AUTO_RUN.load(Ordering::Relaxed) && !BENCH_STARTED.load(Ordering::Relaxed) {
            let tick = TICK_COUNT.fetch_add(1, Ordering::Relaxed);
            if tick >= 150 {
                // ~5 seconds at 30Hz
                BENCH_STARTED.store(true, Ordering::Relaxed);
                run_benchmarks();
            }
        }
    });
}

// ============================================================================
// Entry Point
// ============================================================================

#[reaper_extension_plugin]
fn plugin_main(context: PluginContext) -> Result<(), Box<dyn Error>> {
    // Logging to /tmp/daw-perf-test.log
    let log_file = std::fs::File::create("/tmp/daw-perf-test.log")
        .expect("Failed to create /tmp/daw-perf-test.log");
    let subscriber = tracing_subscriber::fmt()
        .with_writer(std::sync::Mutex::new(log_file))
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .finish();
    // try_init: another extension may have already set a global subscriber
    let _ = tracing::subscriber::set_global_default(subscriber);

    info!("daw-perf-test starting...");

    // Initialize REAPER high-level API (separate singleton for this .so)
    match HighReaper::load(context).setup() {
        Ok(_) => info!("REAPER high-level API initialized"),
        Err(_) => debug!("REAPER high-level API already initialized"),
    }

    // Set TaskSupport for daw-reaper main_thread dispatch
    Global::init();
    daw::reaper::set_task_support(Global::task_support());

    let session = ReaperSession::load(context);
    let app = App::new(session)?;

    APP_INSTANCE
        .set(fragile::Fragile::new(app))
        .expect("App already initialized");

    let app = APP_INSTANCE.get().expect("App should be initialized").get();
    let mut session = app.session.borrow_mut();
    session.plugin_register_add_timer(timer_callback)?;
    drop(session);

    // Check if auto-run is requested via environment variable
    if std::env::var("FTS_PERF_TEST").is_ok() {
        AUTO_RUN.store(true, Ordering::Relaxed);
        info!("FTS_PERF_TEST=1 detected — benchmarks will auto-run in ~5 seconds");
    }

    info!("daw-perf-test loaded");
    info!("  To run benchmarks: set FTS_PERF_TEST=1 before starting REAPER");
    info!("  Results will be written to /tmp/daw-perf-test.log");
    Ok(())
}

/// Kick off benchmarks from the main thread.
fn run_benchmarks() {
    if let Some(app) = get_app() {
        let rt = &app.get().tokio_runtime;
        rt.spawn(async move {
            if let Err(e) = bench::run_all().await {
                tracing::error!("Benchmark suite failed: {e:?}");
            }
        });
    }
}
