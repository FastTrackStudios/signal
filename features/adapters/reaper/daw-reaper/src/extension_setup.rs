//! Common REAPER-extension startup helpers.
//!
//! Anything an FTS-style REAPER extension typically wants to register
//! at `plugin_main` lives here as a one-call helper:
//!
//! - [`init_surviving_broadcasters`] — wires the per-domain broadcasters
//!   that survived the architect-rpc port (item / tempo_map / fx /
//!   routing / take). These power push-based event subscriptions.
//! - [`register_control_surface`] — registers the FTS DAW control
//!   surface against the supplied [`reaper_medium::ReaperSession`]
//!   with mode resolved from the `FTS_CSURF_MODE` env var
//!   (push-only / full / off). Honors `FTS_CSURF_DISABLED=1` as a
//!   hard opt-out.
//! - [`register_window_geometry_actions`] — registers the 8 FTS window
//!   nudge / grow REAPER actions (`FTS_WINDOW_*`) for the focused
//!   window.
//!
//! Each helper logs success / failure but never panics.

use std::sync::Arc;

use reaper_high::Reaper as HighReaper;
use reaper_medium::{OwnedGaccelRegister, ReaperSession};
use tracing::{info, warn};

use crate::control_surface::DawControlSurface;

/// Bundle of in-process handles created by [`init_audio_sync`].
///
/// The host stores these so the periodic timer callback can refresh
/// the multi-project slot table via [`refresh_audio_sync_registry`]
/// and the clock-sync bootstrap has the single-project cell to share
/// across the buffer-snapshot and clock-sync paths.
#[derive(Clone)]
pub struct AudioSyncHandle {
    pub cell: Arc<daw_audio_sync::SnapshotCell>,
    pub registry: Arc<daw_audio_sync::registry::ProjectRegistry>,
}

/// Initialise the broadcaster machinery for the domains that still
/// push events (items / tempo map / fx / routing / takes). Safe to
/// call once per plugin startup; individual `init_*_broadcaster`
/// functions are idempotent.
pub fn init_surviving_broadcasters() {
    crate::item::init_item_broadcaster();
    crate::tempo_map::init_tempo_map_broadcaster();
    crate::fx_stream::init_fx_broadcaster();
    crate::routing_stream::init_routing_broadcaster();
    crate::take_stream::init_take_broadcaster();
    info!("Surviving broadcasters initialised (item / tempo / fx / routing / take)");
}

/// Register the DAW control surface against `session`.
///
/// Mode is resolved from the `FTS_CSURF_MODE` env var. Set
/// `FTS_CSURF_DISABLED=1` to skip registration entirely (used by
/// tests / minimal builds). The registration handle is intentionally
/// dropped — `ReaperSession` owns the surface and unregisters at
/// REAPER shutdown.
pub fn register_control_surface(session: &mut ReaperSession) {
    if std::env::var("FTS_CSURF_DISABLED").as_deref() == Ok("1") {
        info!("Control surface disabled via FTS_CSURF_DISABLED=1");
        return;
    }

    use reaper_high::MiddlewareControlSurface;
    let mode = crate::CsurfMode::from_env();
    let csurf = MiddlewareControlSurface::new(DawControlSurface::with_mode(mode));
    match session.plugin_register_add_csurf_inst(Box::new(csurf)) {
        Ok(_handle) => info!("DAW control surface registered (mode={mode:?})"),
        Err(e) => warn!("Failed to register DAW control surface: {e}"),
    }
}

/// Register the FTS window-geometry REAPER actions (8 total).
///
/// All actions target [`daw_proto::WindowTarget::Focused`] so the user
/// picks the target by clicking before pressing the binding. Steps
/// match daw-bridge: 10 px nudges, 50 px grows.
///
/// Mirrors the gaccel-after-register_action fix used by the action
/// registry service: `reaper.register_action` only registers a gaccel
/// during `wake_up`, so post-wake_up registrations need a manual
/// `plugin_register_add_gaccel` call to actually appear in REAPER's
/// action list.
pub fn register_window_geometry_actions() {
    use crate::window_geometry as wg;
    use daw_proto::WindowTarget;
    use reaper_high::ActionKind;

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

    for (name, desc, op) in actions {
        register_extension_action(name, desc, *op);
    }
    info!(
        count = actions.len(),
        "Registered FTS window-geometry actions (FTS_WINDOW_*)"
    );
}

/// Register a single extension action via reaper-high, plus the
/// matching gaccel (so it actually appears in REAPER's action list
/// even when registered after wake_up).
fn register_extension_action(name: &'static str, description: &'static str, op: fn()) {
    let high = HighReaper::get();
    let action = high.register_action(
        name,
        description,
        None,
        op,
        reaper_high::ActionKind::NotToggleable,
    );
    let cmd_id = action.command_id();

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
            warn!(
                action = name,
                error = ?e,
                "Failed to register gaccel for window-geometry action"
            );
        }
    }
    // Drop the registration handle — REAPER owns it now.
    let _ = action;
}

// ─── Audio-sync + clock-sync ────────────────────────────────────────────────

/// Register the single-project + multi-project audio-thread snapshot
/// hooks against `session` and return the in-process handles the host
/// needs to keep alive. Returns `None` if either hook fails to
/// register — caller can log + proceed without sync.
///
/// Sets the corresponding `daw_audio_sync::set_global_*` slots so any
/// other consumer (DriftCorrector, Diagnostics RPC, etc.) sees the
/// same cells without threading them around.
pub fn init_audio_sync(session: &mut ReaperSession) -> Option<AudioSyncHandle> {
    let rt_reaper = session.create_real_time_reaper();
    let (cell, hook) = daw_audio_sync::build_hook(rt_reaper);
    daw_audio_sync::set_global_cell(cell.clone());
    if let Err(e) = session.audio_reg_hardware_hook_add(Box::new(hook)) {
        warn!(error = %e, "audio-sync single-project hook failed");
        return None;
    }
    info!("audio-sync single-project hook registered");

    let rt_reaper = session.create_real_time_reaper();
    let (registry, hook) = daw_audio_sync::registry::build_multi_project_hook(rt_reaper);
    daw_audio_sync::registry::set_global_registry(registry.clone());
    if let Err(e) = session.audio_reg_hardware_hook_add(Box::new(hook)) {
        warn!(error = %e, "audio-sync multi-project hook failed");
        return None;
    }
    info!("audio-sync multi-project hook registered");

    Some(AudioSyncHandle { cell, registry })
}

/// Bootstrap the ClockSync UDP peer-discovery + offset-estimation +
/// position-broadcast runtime against the supplied audio-sync `cell`.
///
/// `port` defaults to [`daw_audio_sync::clock_sync::DEFAULT_PORT`]
/// (7777). `enable_drift_correction` controls whether the
/// [`daw_audio_sync::drift::DriftCorrector`] spawns alongside —
/// pass `false` to skip rate-change actuation (peer discovery +
/// position broadcast still active).
///
/// Env-var overrides for backwards compat with daw-bridge tests:
/// `FTS_AUDIO_SYNC_PORT=<u16>` and `FTS_AUDIO_SYNC_DRIFT=1` take
/// precedence over the arguments when set.
///
/// The async bind runs on `runtime`; returns immediately. Any
/// failure logs a warning and leaves the rest of the extension
/// running.
pub fn init_clock_sync<F>(
    runtime: tokio::runtime::Handle,
    cell: Arc<daw_audio_sync::SnapshotCell>,
    port: u16,
    enable_drift_correction: bool,
    dispatch_rate_change: F,
) where
    F: Fn(f64) + Send + Sync + 'static,
{
    let dispatch_rate_change: Arc<dyn Fn(f64) + Send + Sync> = Arc::new(dispatch_rate_change);
    let port = std::env::var("FTS_AUDIO_SYNC_PORT")
        .ok()
        .and_then(|raw| raw.parse::<u16>().ok())
        .unwrap_or(port);
    let enable_drift_correction = std::env::var("FTS_AUDIO_SYNC_DRIFT")
        .ok()
        .map(|v| v == "1")
        .unwrap_or(enable_drift_correction);
    let cell_for_sync = cell.clone();

    runtime.spawn(async move {
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
                let arc = Arc::new(cs);
                daw_audio_sync::set_global_clock_sync(arc.clone());

                if enable_drift_correction {
                    let dispatch = dispatch_rate_change.clone();
                    let corrector = daw_audio_sync::drift::DriftCorrector::spawn(
                        cell_for_drift,
                        arc.clone(),
                        daw_audio_sync::drift::DriftConfig::default(),
                        move |rate| dispatch(rate),
                    );
                    daw_audio_sync::set_global_drift_corrector(Arc::new(corrector));
                    info!("drift correction enabled — proportional, ±1% cap");
                }
            }
            Err(e) => warn!(error = ?e, "clock-sync bind failed"),
        }
    });
}

/// Re-scan REAPER's open project tabs and assign each one to a slot
/// in the audio-sync registry. Intended to be called from the host's
/// ~30Hz timer callback. Cheap (one `enum_projects` loop); keeps the
/// audio hook's slot view in sync with REAPER's user-visible tabs.
///
/// Lifted verbatim from daw-bridge so the periodic refresh behavior
/// matches what existing test rigs expect.
pub fn refresh_audio_sync_registry(registry: &daw_audio_sync::registry::ProjectRegistry) {
    use reaper_medium::{ProjectRef, ReaProject};
    use std::collections::HashMap;
    use std::sync::Mutex;
    static PROJECT_ID_MAP: std::sync::OnceLock<Mutex<HashMap<usize, [u8; 16]>>> =
        std::sync::OnceLock::new();
    let id_map = PROJECT_ID_MAP.get_or_init(|| Mutex::new(HashMap::new()));

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
            if let Ok(mut map) = id_map.lock() {
                map.remove(&(current_project.as_ptr() as usize));
            }
        }
    }
}
