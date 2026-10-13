//! REAPER ↔ DAW bridge extension.
//!
//! Loads daw-reaper's service implementations inside REAPER and exposes them
//! via Unix socket so external processes (tests, FTS, CLI tools) can control
//! the DAW through vox RPC. Integrated extensions use daw-extension-runtime
//! in-process instead of routing through this bridge.

mod routed_handler;
#[cfg(feature = "sync")]
mod sync_runtime;

use daw::{Layer, Services as _};

// ============================================================================
// RT-safe Global Allocator
// ============================================================================

#[global_allocator]
static ALLOCATOR: daw_allocator::FtsAllocator = daw_allocator::FtsAllocator::new();

use fragile::Fragile;
use reaper_high::{ActionKind, MainTaskMiddleware, Reaper as HighReaper};
use reaper_low::PluginContext;
use reaper_macros::reaper_extension_plugin;
use reaper_medium::OwnedGaccelRegister;
use reaper_medium::ReaperSession;
use std::cell::RefCell;
use std::error::Error;
use std::path::PathBuf;
use std::sync::OnceLock;
use tokio::net::UnixListener;
use tracing::{debug, info, warn};

use routed_handler::DawConnectionAcceptor;

// ============================================================================
// Global State (TaskSupport for main thread dispatch)
// ============================================================================

use crossbeam_channel::{Receiver, Sender};
use daw_allocator::{FtsRuntime, FtsRuntimeConfig, RtDetector};
use reaper_high::{MainThreadTask, TaskSupport};

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
            info!("Global TaskSupport initialized");
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

struct App {
    session: RefCell<ReaperSession>,
    #[allow(dead_code)]
    tokio_runtime: tokio::runtime::Runtime,
    task_middleware: RefCell<MainTaskMiddleware>,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App").finish_non_exhaustive()
    }
}

impl App {
    fn new(session: ReaperSession) -> Result<Self, Box<dyn Error>> {
        let tokio_runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
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

    fn initialize(&self) -> Result<(), Box<dyn Error>> {
        info!("Initializing daw-bridge...");

        self.tokio_runtime.block_on(async {
            register_daw_dispatcher().await;
        });

        info!("daw-bridge initialized");
        Ok(())
    }
}

// ============================================================================
// DAW Dispatcher Registration
// ============================================================================

async fn register_daw_dispatcher() {
    info!("Registering DAW dispatcher...");

    // Set TaskSupport for daw-reaper to use
    daw::reaper::set_task_support(Global::task_support());

    // Surviving (non-service) broadcasters — others retired with the
    // architect::rpc port.
    daw::reaper::init_item_broadcaster();
    daw::reaper::init_tempo_map_broadcaster();
    daw::reaper::init_fx_broadcaster();
    daw::reaper::init_routing_broadcaster();
    daw::reaper::init_take_broadcaster();
    info!("Surviving broadcasters initialized");

    let dock_host = daw_reaper_dioxus::ReaperDockHost::new();

    // Reaper's canonical service surface (impl Services for Reaper),
    // plus the dock-host bolt-on (different backend, so it ships
    // pre-mounted). The MIDI analysis service (`keyflow-daw-analysis`)
    // is mounted out-of-tree by fts-extensions; see daw-reaper
    // a823b67 for why.
    let daw_handler = daw::reaper::Reaper::layers()
        .merge(daw_proto::dock_host::layer(dock_host))
        .provide(daw::reaper::Reaper);

    let acceptor = DawConnectionAcceptor::new(daw_handler);

    // Start Unix socket server
    start_unix_socket_server(acceptor.clone());

    info!("DAW bridge registered (21 services, socket)");
}

// ============================================================================
// Unix Socket Server
// ============================================================================

/// Build the socket path for this REAPER instance.
///
/// Default: `/tmp/fts-daw-{pid}.sock` — matches what reaper-test discovers.
/// Override with `FTS_SOCKET` env var.
fn socket_path() -> PathBuf {
    std::env::var("FTS_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let pid = std::process::id();
            PathBuf::from(format!("/tmp/fts-daw-{pid}.sock"))
        })
}

fn start_unix_socket_server(acceptor: DawConnectionAcceptor) {
    let path = socket_path();

    // Remove stale socket from a previous run
    let _ = std::fs::remove_file(&path);

    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(e) => {
            warn!("Failed to bind Unix socket at {}: {}", path.display(), e);
            return;
        }
    };

    info!("Unix socket server listening on {}", path.display());

    tokio::task::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _addr)) => {
                    info!("Client connected via Unix socket");
                    let acceptor = acceptor.clone();
                    tokio::task::spawn(async move {
                        use vox::MetadataExt as _;
                        let link = vox_stream::StreamLink::unix(stream);
                        // vox 0.10 lane model: hand every inbound lane to the
                        // shared LayerRouter (dispatches by method id).
                        let router = acceptor.router();
                        let lane_acceptor = vox::lane_acceptor_fn(move |req, connection| {
                            let role = req.metadata().meta_str("role").unwrap_or("unknown");
                            info!("Accepting lane: role={}", role);
                            connection.handle_with(router.clone());
                            Ok(())
                        });
                        match vox::acceptor_on(link)
                            .on_lane(lane_acceptor)
                            .establish_connection()
                            .await
                        {
                            Ok(_connection) => {
                                debug!("Unix socket session established");
                                std::future::pending::<()>().await;
                            }
                            Err(e) => {
                                warn!("Unix socket handshake failed: {:?}", e);
                            }
                        }
                    });
                }
                Err(e) => {
                    warn!("Unix socket accept error: {}", e);
                }
            }
        }
    });
}

// ============================================================================
// RT Thread Detection
// ============================================================================

/// Wraps REAPER's `IsInRealTimeAudio()` function pointer for RT detection.
struct ReaperRtDetector {
    is_in_rt_audio: unsafe extern "C" fn() -> i32,
}

// Safety: The function pointer is a static C function — safe to call from any thread.
unsafe impl Send for ReaperRtDetector {}
unsafe impl Sync for ReaperRtDetector {}

impl RtDetector for ReaperRtDetector {
    fn is_rt_thread(&self) -> bool {
        unsafe { (self.is_in_rt_audio)() != 0 }
    }
}

// ============================================================================
// Timer Callback & Entry Point
// ============================================================================

static APP_INSTANCE: OnceLock<Fragile<App>> = OnceLock::new();

fn get_app() -> Option<&'static Fragile<App>> {
    APP_INSTANCE.get()
}

/// Timer callback for periodic updates (runs on main thread ~30Hz)
/// Deferred eager-load of FTS CLAP plugins. Runs once on the first timer
/// tick so REAPER's CLAP scanner has already finished scanning.
static FX_PLUGINS_LOADED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Shared audio-thread snapshot cell. Audio hook writes per-buffer
/// state; main thread / RPC reads via this handle.
pub static AUDIO_SYNC_CELL: OnceLock<std::sync::Arc<daw_audio_sync::SnapshotCell>> =
    OnceLock::new();

/// Multi-project registry — one snapshot cell per open project,
/// populated by the multi-project audio hook + the main-thread
/// updater that maps `enum_projects` → slot.
pub static PROJECT_REGISTRY: OnceLock<std::sync::Arc<daw_audio_sync::registry::ProjectRegistry>> =
    OnceLock::new();

/// Live clock-sync session. `None` until the bound task succeeds; held
/// here so the Diagnostics RPC can read the peer table without
/// re-binding sockets.
pub static CLOCK_SYNC: OnceLock<std::sync::Arc<daw_audio_sync::clock_sync::ClockSync>> =
    OnceLock::new();

/// Live drift corrector. Holds the spawned task; dropping (extension
/// unload) stops the controller.
pub static DRIFT_CORRECTOR: OnceLock<std::sync::Arc<daw_audio_sync::drift::DriftCorrector>> =
    OnceLock::new();

/// Refresh `ProjectRegistry` slot assignments from REAPER's open
/// projects. Runs on the main thread every timer tick. New projects
/// get a fresh UUID; vanished projects have their slot cleared.
///
/// State: a per-process map (`PROJECT_ID_MAP`) keyed by ReaProject
/// pointer (as `usize`) → assigned `[u8; 16]` id, so the same
/// project keeps the same id across timer ticks and across slot
/// reshuffles when other projects close.
fn refresh_audio_sync_registry(registry: &daw_audio_sync::registry::ProjectRegistry) {
    use reaper_medium::{ProjectRef, ReaProject};
    use std::collections::HashMap;
    use std::sync::Mutex;
    static PROJECT_ID_MAP: std::sync::OnceLock<Mutex<HashMap<usize, [u8; 16]>>> =
        std::sync::OnceLock::new();
    let id_map = PROJECT_ID_MAP.get_or_init(|| Mutex::new(HashMap::new()));

    // Enumerate open projects.
    let reaper = reaper_high::Reaper::get().medium_reaper();
    let mut seen: Vec<(ReaProject, [u8; 16])> = Vec::new();
    for tab in 0..=daw_audio_sync::registry::MAX_PROJECTS {
        match reaper.enum_projects(ProjectRef::Tab(tab as u32), 0) {
            Some(res) => {
                let ptr_key = res.project.as_ptr() as usize;
                let id = {
                    let mut map = match id_map.lock() {
                        Ok(g) => g,
                        Err(_) => return,
                    };
                    *map.entry(ptr_key).or_insert_with(|| {
                        let id = uuid::Uuid::new_v4();
                        *id.as_bytes()
                    })
                };
                seen.push((res.project, id));
            }
            None => break,
        }
    }

    // Assign each seen project to a slot. Try to keep the existing
    // mapping (find_slot); fall back to find_vacant.
    for (project, id) in &seen {
        let slot_idx = registry
            .find_slot(*project)
            .or_else(|| registry.find_vacant());
        if let Some(idx) = slot_idx
            && let Some(slot) = registry.slot(idx)
        {
            slot.assign(*project, *id);
        }
    }

    // Clear slots for projects no longer open.
    for idx in 0..daw_audio_sync::registry::MAX_PROJECTS {
        let Some(slot) = registry.slot(idx) else {
            continue;
        };
        let Some((current_project, _)) = slot.current() else {
            continue;
        };
        let still_open = seen
            .iter()
            .any(|(p, _)| p.as_ptr() == current_project.as_ptr());
        if !still_open {
            slot.clear();
            // Also clean the id map.
            if let Ok(mut map) = id_map.lock() {
                map.remove(&(current_project.as_ptr() as usize));
            }
        }
    }
}

extern "C" fn timer_callback() {
    // catch_unwind prevents panics from unwinding through the C ABI boundary
    // (which is UB). Any panic inside is logged and the timer keeps running.
    let result = std::panic::catch_unwind(|| {
        // Deferred eager-load: run once after REAPER startup is complete
        if !FX_PLUGINS_LOADED.load(std::sync::atomic::Ordering::Relaxed) {
            FX_PLUGINS_LOADED.store(true, std::sync::atomic::Ordering::Relaxed);
            daw::reaper::eager_load_fx_plugins();
        }

        // Measure REAPER's actual timer tick rate when FTS_TIMER_PROBE=1.
        // Gives ground truth for what the dispatcher ceiling actually
        // is — the SDK doc says "roughly 30 times per second" but
        // observed rate can differ. Logs every 30 ticks.
        if std::env::var("FTS_TIMER_PROBE").as_deref() == Ok("1") {
            use std::sync::atomic::{AtomicU64, Ordering};
            use std::time::Instant;
            static LAST: std::sync::OnceLock<std::sync::Mutex<Instant>> =
                std::sync::OnceLock::new();
            static COUNT: AtomicU64 = AtomicU64::new(0);
            let last = LAST.get_or_init(|| std::sync::Mutex::new(Instant::now()));
            let now = Instant::now();
            let mut guard = last.lock().unwrap();
            let dt = now.duration_since(*guard);
            *guard = now;
            let n = COUNT.fetch_add(1, Ordering::Relaxed);
            if n.is_multiple_of(30) && n > 0 {
                tracing::info!(tick = n, "FTS_TIMER_PROBE dt={dt:?}");
            }
        }

        if let Some(app_fragile) = get_app() {
            let app = app_fragile.get();

            // Process main-thread task queue (reaper-high TaskSupport)
            app.process_tasks();

            // Process daw-allocator main-thread tasks (closures from any thread)
            if let Some(runtime) = FtsRuntime::try_get() {
                runtime.process_main_thread_tasks();
            }

            // Poll surviving broadcasters. FX / routing / track
            // streams will land as DawEventHub channels (Phase 2 of
            // docs/streaming-design.md) — transport is wired now.
            daw::reaper::poll_and_broadcast_items();
            daw::reaper::poll_and_broadcast_tempo_map();
            daw::reaper::poll_and_broadcast_transport();
            daw::reaper::poll_and_broadcast_markers();
            daw::reaper::poll_and_broadcast_regions();
            daw::reaper::poll_and_broadcast_tracks();
            daw::reaper::poll_and_broadcast_fx();
            daw::reaper::poll_and_broadcast_routing();
            daw::reaper::poll_and_broadcast_takes();
            daw::reaper::poll_and_broadcast_meters();
            // Reads a slice of the tracks per tick rather than all of
            // them, so the cost is flat whatever the session's size. It
            // is the only thing that sees a group edited by hand in
            // REAPER's own matrix dialog, and the only way a window
            // that has just attached learns the grouping at all — the
            // bulk track read leaves it empty because reading it is
            // about a hundred calls per track.
            daw::reaper::poll_and_broadcast_grouping();

            // Process deferred toolbar operations
            daw::reaper::process_toolbar_ops();

            // Refresh the multi-project audio-sync slot assignments.
            // Cheap (one enum_projects loop) and lets the audio
            // hook observe newly-opened tabs / clear closed ones.
            if let Some(registry) = PROJECT_REGISTRY.get() {
                refresh_audio_sync_registry(registry);
            }
        }
    });
    if let Err(e) = result {
        warn!("timer_callback panicked: {:?}", e);
    }
}

/// REAPER extension entry point.
#[reaper_extension_plugin]
fn plugin_main(context: PluginContext) -> Result<(), Box<dyn Error>> {
    // Initialize tracing to /tmp/daw-bridge-{pid}.log so multi-instance
    // tests don't clobber each other's logs.
    let pid = std::process::id();
    let log_path = format!("/tmp/daw-bridge-{pid}.log");
    let log_file = std::fs::File::create(&log_path).expect("Failed to create daw-bridge log file");
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        // Suppress cranelift / wgpu noise; keep our crates + dependencies
        // we care about at info.
        "info,cranelift_jit=warn,cranelift_codegen=warn,wgpu=warn,wgpu_core=warn,wgpu_hal=warn,naga=warn".into()
    });

    tracing_subscriber::fmt()
        .with_writer(std::sync::Mutex::new(log_file))
        .with_env_filter(env_filter)
        .init();

    info!("daw-bridge starting...");

    // Store plugin context for PluginLoaderService before REAPER consumes it
    daw::reaper::set_plugin_context(context);

    // Install SWELL function pointers for cross-platform window/menu APIs
    // (used by the FTS Extensions menu hook, the input TranslateAccel
    // handler, and the screenset window-geometry capture / apply path).
    // `make_available_globally` is idempotent in practice — repeated loads
    // would just race the OnceLock, so we ignore the result.
    let _ = reaper_low::Swell::make_available_globally(reaper_low::Swell::load(context));

    // Initialize REAPER high-level API
    match HighReaper::load(context).setup() {
        Ok(_) => {
            info!("REAPER high-level API initialized");
            // wake_up() registers the global hookcommand AND toggleaction
            // hooks (HighLevelHookCommand / HighLevelToggleAction). Without
            // it our ActionKind::Toggleable closure is never queried, so
            // toggle indicators in REAPER's action list / toolbars stay
            // frozen at the initial value.
            match HighReaper::get().wake_up() {
                Ok(()) => info!("REAPER high-level API woke up (toggleaction hook registered)"),
                Err(e) => {
                    tracing::error!(
                        "REAPER high-level API wake_up FAILED: {e} — toggle action \
                         indicators will not update"
                    );
                }
            }
        }
        Err(_) => debug!("REAPER high-level API already initialized"),
    }

    // Initialize RT-safe allocator runtime.
    // Must happen after REAPER loads but before any RT audio processing.
    let reaper = HighReaper::get();
    if let Some(is_in_rt_audio) = reaper.medium_reaper().low().pointers().IsInRealTimeAudio {
        let detector = ReaperRtDetector { is_in_rt_audio };
        FtsRuntime::init(
            &ALLOCATOR,
            FtsRuntimeConfig {
                dealloc_channel_capacity: 10_000,
                rt_detector: Box::new(detector),
            },
        );
        info!("RT allocator initialized (async deallocation enabled)");
    } else {
        warn!("IsInRealTimeAudio not available — RT allocator running without async deallocation");
    }

    // Create a medium-level API session
    let session = ReaperSession::load(context);

    // Create the App (initializes Global/TaskSupport)
    let app = App::new(session)?;

    // Initialize (register DAW dispatcher + socket server)
    app.initialize()?;

    // FTS CLAP plugin eager-loading is deferred to the first timer tick
    // so REAPER's CLAP scanner finishes first (avoids dlopen conflicts).

    // Store app globally
    APP_INSTANCE
        .set(Fragile::new(app))
        .expect("App already initialized");

    // Register timer callback for periodic updates and Extensions menu hook
    let app = APP_INSTANCE.get().expect("App should be initialized").get();
    let mut session = app.session.borrow_mut();
    session.plugin_register_add_timer(timer_callback)?;

    // Faster main-thread tick — bumps REAPER's internal MISC_TIMER
    // (id 666) to a higher rate via SWELL's SetTimer. Drives all
    // plugin_register("timer") callbacks at the new rate, which cuts
    // architect dispatcher wait (0..tick) for inbound RPCs.
    //
    // Same trick as the public `reaper_60fps` extension. Opt-in via
    // FTS_MISC_TIMER_HZ (e.g. 60, 120, 240). Unix-only for now
    // (Windows would need User32::SetTimer instead of SWELL).
    #[cfg(target_family = "unix")]
    if let Ok(hz) = std::env::var("FTS_MISC_TIMER_HZ")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .map(|hz| hz.clamp(15, 1000))
        .ok_or(())
    {
        // REAPER's misc timer id is 666. Re-arming SetTimer with the
        // same (hwnd, id) replaces the existing timer's interval.
        const MISC_TIMER: usize = 666;
        let rate_ms = (1000 / hz).max(1);
        let swell = reaper_low::Swell::load(*session.reaper().low().plugin_context());
        let hwnd = session.reaper().get_main_hwnd();
        // SAFETY: hwnd from REAPER, timer id matches REAPER's misc
        // timer slot, null TIMERPROC delegates to REAPER's existing
        // WM_TIMER handler.
        unsafe {
            swell.SetTimer(hwnd.as_ptr(), MISC_TIMER, rate_ms, None);
        }
        info!("FTS_MISC_TIMER_HZ={hz} — REAPER misc timer rearmed at {rate_ms}ms");
    }

    daw::reaper::register_extension_menu(&mut session);

    daw::reaper::register_project_importer(&mut session)?;

    // Audio-thread observer: writes per-buffer (host_micros, playhead,
    // sample_rate, len) into a lock-free seqlock cell that the main
    // thread + sync engine read. Foundation for sample-accurate
    // multi-machine sync.
    //
    // Two hooks register together so single-project consumers
    // (DriftCorrector, default Diagnostics RPCs) keep working while
    // multi-project consumers (FTS-session, per-project drift) get
    // independent per-project snapshots via the registry.
    let audio_sync_cell = {
        let rt_reaper = session.create_real_time_reaper();
        let (cell, hook) = daw_audio_sync::build_hook(rt_reaper);
        daw_audio_sync::set_global_cell(cell.clone());
        let _ = AUDIO_SYNC_CELL.set(cell.clone());
        match session.audio_reg_hardware_hook_add(Box::new(hook)) {
            Ok(_) => info!("audio-sync single-project hook registered"),
            Err(e) => warn!("audio-sync single-project hook failed: {e}"),
        }
        cell
    };
    {
        let rt_reaper = session.create_real_time_reaper();
        let (registry, hook) = daw_audio_sync::registry::build_multi_project_hook(rt_reaper);
        daw_audio_sync::registry::set_global_registry(registry.clone());
        let _ = PROJECT_REGISTRY.set(registry);
        match session.audio_reg_hardware_hook_add(Box::new(hook)) {
            Ok(_) => info!("audio-sync multi-project hook registered"),
            Err(e) => warn!("audio-sync multi-project hook failed: {e}"),
        }
    }

    // ClockSync — UDP peer discovery + PTP-style offset estimation +
    // sample-position broadcast. Opt-in via FTS_AUDIO_SYNC_PORT so
    // existing test rigs and single-instance setups don't bind random
    // sockets. Default port is 7777 when env var is set without a
    // numeric value.
    if let Ok(raw) = std::env::var("FTS_AUDIO_SYNC_PORT") {
        let port = raw
            .parse::<u16>()
            .unwrap_or(daw_audio_sync::clock_sync::DEFAULT_PORT);
        let cell_for_sync = audio_sync_cell.clone();
        // Use the bridge's own tokio runtime (plugin_main runs from a
        // C ABI thread without an ambient runtime — Handle::current
        // panics, try_current returns None).
        let handle = app.tokio_runtime.handle().clone();
        handle.spawn(async move {
            let cell_for_drift = cell_for_sync.clone();
            match daw_audio_sync::clock_sync::ClockSync::bind(
                port,
                daw_audio_sync::clock_sync::DEFAULT_MULTICAST,
                Some(cell_for_sync),
            )
            .await
            {
                Ok(cs) => {
                    info!(
                        peer_id = ?cs.peer_id,
                        port,
                        "clock-sync bound; multicast peer discovery + position broadcast active"
                    );
                    let arc = std::sync::Arc::new(cs);
                    daw_audio_sync::set_global_clock_sync(arc.clone());

                    // Drift corrector: dispatches CSurf_OnPlayRateChange
                    // via TaskSupport when local position diverges from
                    // the elected leader's projected position. Off by
                    // default — opt in via FTS_AUDIO_SYNC_DRIFT=1
                    // (still safe to enable: actuator is a no-op
                    // when leader is None / drift below deadband).
                    if std::env::var("FTS_AUDIO_SYNC_DRIFT").as_deref() == Ok("1") {
                        let ts = Global::task_support();
                        let corrector = daw_audio_sync::drift::DriftCorrector::spawn(
                            cell_for_drift,
                            arc.clone(),
                            daw_audio_sync::drift::DriftConfig::default(),
                            move |rate| {
                                use reaper_medium::PlaybackSpeedFactor;
                                let _ = ts.do_later_in_main_thread_asap(move || {
                                    let reaper = reaper_high::Reaper::get();
                                    let speed = PlaybackSpeedFactor::new(rate);
                                    reaper.medium_reaper().csurf_on_play_rate_change(speed);
                                });
                            },
                        );
                        let arc_corrector = std::sync::Arc::new(corrector);
                        daw_audio_sync::set_global_drift_corrector(arc_corrector.clone());
                        let _ = DRIFT_CORRECTOR.set(arc_corrector);
                        info!("drift correction enabled — proportional, ±1% cap");
                    }

                    let _ = CLOCK_SYNC.set(arc);
                }
                Err(e) => warn!(?e, "clock-sync bind failed"),
            }
        });
    }

    // Push-based change-detection via REAPER's IReaperControlSurface
    // callbacks. Mode is selected by the FTS_CSURF_MODE env var:
    //
    //   FTS_CSURF_MODE=full       — publish every callback (sub-tick push
    //                               for read-only subscribers like web UIs
    //                               and MIDI/OSC surface adapters). Don't
    //                               combine with the bidirectional sync
    //                               runtime: applies → REAPER → callback
    //                               → publish creates echo loops.
    //   FTS_CSURF_MODE=push-only  — default. Publish only events that have
    //                               no equivalent in the 30 Hz poller (FX
    //                               params, marker nudges). Safe to pair
    //                               with bidirectional sync.
    //   FTS_CSURF_MODE=off        — register but no-op every callback.
    //   FTS_CSURF_DISABLED=1      — skip registration entirely (back-compat).
    //
    // The returned RegistrationHandle is dropped; ReaperSession owns the
    // boxed surface and unregisters on REAPER shutdown.
    if std::env::var("FTS_CSURF_DISABLED").as_deref() != Ok("1") {
        use reaper_high::MiddlewareControlSurface;
        let mode = daw::reaper::CsurfMode::from_env();
        let csurf = MiddlewareControlSurface::new(daw::reaper::DawControlSurface::with_mode(mode));
        match session.plugin_register_add_csurf_inst(Box::new(csurf)) {
            Ok(_handle) => {
                info!("daw control surface registered (mode={mode:?})");
            }
            Err(e) => {
                warn!("failed to register daw control surface: {e}");
            }
        }
    }

    drop(session);

    register_window_geometry_actions();

    #[cfg(feature = "sync")]
    if std::env::var("FTS_SYNC_ENABLED").as_deref() == Ok("1") {
        let app = APP_INSTANCE.get().expect("App should be initialized").get();
        let runtime = app.tokio_runtime.handle().clone();
        // Construct an in-process Daw handle pointing at the REAPER dispatcher
        // we registered above. Don't rely on the daw::get() facade singleton —
        // daw-bridge never calls init_from_parts, so it stays None.
        runtime.spawn(async move {
            match daw::reaper::build_extension_daw().await {
                Ok(daw) => {
                    if let Err(e) = sync_runtime::start(daw).await {
                        warn!("sync runtime start failed: {e}");
                    }
                }
                Err(e) => warn!("sync runtime: failed to build daw handle: {e}"),
            }
        });
    }

    info!("daw-bridge initialized successfully");
    Ok(())
}

/// Register the FTS window-geometry REAPER actions (nudge / grow).
///
/// All actions target [`WindowTarget::Focused`], so the user picks the
/// target by clicking before pressing the binding. Step size is hard-coded
/// at 10 px for nudges and 50 px for resizes; that matches REAPER's own
/// "Nudge" actions and is a comfortable single-tap step at typical DPI.
/// Tweaking step size is a follow-up — easiest path is per-extension
/// ExtState read inside each action's closure.
///
/// Mirrors the gaccel-after-register_action fix used by the action
/// registry service: `reaper.register_action` only registers a gaccel
/// during `wake_up`, so post-wake_up registrations need a manual
/// `plugin_register_add_gaccel` call to actually appear in REAPER's
/// action list (issue #15 / SWS toolbar pre-allocation interaction).
fn register_fts_extension_action(name: &'static str, description: &'static str, op: fn()) {
    let high = HighReaper::get();
    let action = high.register_action(name, description, None, op, ActionKind::NotToggleable);
    let cmd_id = action.command_id();

    // If REAPER already lists this cmd_id (e.g. it preregistered the name
    // from a toolbar), don't double-register the gaccel.
    let already_listed = high
        .main_section()
        .with_raw(|s| {
            (0..s.action_list_cnt()).any(|i| {
                s.get_action_by_index(i)
                    .map(|a| a.cmd() == cmd_id)
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);
    if !already_listed {
        let gaccel = OwnedGaccelRegister::without_key_binding(cmd_id, description);
        let mut session = high.medium_session();
        if let Err(e) = session.plugin_register_add_gaccel(gaccel) {
            warn!("Failed to register gaccel for '{name}': {e:?}");
        }
    }
    let _ = action;
}

fn register_window_geometry_actions() {
    use daw::reaper::window_geometry as wg;
    use daw_proto::WindowTarget;

    const NUDGE_STEP: i32 = 10;
    const GROW_STEP: i32 = 50;

    fn nudge(dx: i32, dy: i32) {
        if let Some(window) = wg::resolve_target(WindowTarget::Focused) {
            wg::nudge(window, dx, dy);
        }
    }
    fn grow(dw: i32, dh: i32) {
        if let Some(window) = wg::resolve_target(WindowTarget::Focused) {
            wg::grow(window, dw, dh);
        }
    }

    let actions: &[(&'static str, &'static str, fn())] = &[
        (
            "FTS_WINDOW_NUDGE_LEFT",
            "FTS: Nudge focused window left",
            || nudge(-NUDGE_STEP, 0),
        ),
        (
            "FTS_WINDOW_NUDGE_RIGHT",
            "FTS: Nudge focused window right",
            || nudge(NUDGE_STEP, 0),
        ),
        (
            "FTS_WINDOW_NUDGE_UP",
            "FTS: Nudge focused window up",
            || nudge(0, -NUDGE_STEP),
        ),
        (
            "FTS_WINDOW_NUDGE_DOWN",
            "FTS: Nudge focused window down",
            || nudge(0, NUDGE_STEP),
        ),
        (
            "FTS_WINDOW_GROW_WIDER",
            "FTS: Grow focused window wider",
            || grow(GROW_STEP, 0),
        ),
        (
            "FTS_WINDOW_GROW_NARROWER",
            "FTS: Shrink focused window narrower",
            || grow(-GROW_STEP, 0),
        ),
        (
            "FTS_WINDOW_GROW_TALLER",
            "FTS: Grow focused window taller",
            || grow(0, GROW_STEP),
        ),
        (
            "FTS_WINDOW_GROW_SHORTER",
            "FTS: Shrink focused window shorter",
            || grow(0, -GROW_STEP),
        ),
    ];

    for (name, desc, action) in actions {
        register_fts_extension_action(name, desc, *action);
    }
    info!(
        "Registered {} window-geometry actions (FTS_WINDOW_*)",
        actions.len()
    );
}
