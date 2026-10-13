//! Implementation of [`session_proto::setlist_actions::SetlistActions`]
//! — build / load-demo / dump-ruler-state.
//!
//! Runs **synchronously on REAPER's main thread** (where the action
//! callback fires), so the sync trait methods on `daw_reaper::Reaper` can
//! be called directly — no `main_thread::query` bounce, no Tokio
//! runtime, no `architect::platform::spawn`. Same pattern as
//! `keyflow_actions`, and the same reason it works.
//!
//! Setlist storage: at mount time `register` stashes the
//! `SetlistServiceImpl`'s `Arc<RwLock<Option<Setlist>>>` so the
//! sync writer here and the RPC reader (`SetlistService::setlist`)
//! share one piece of state.

use std::sync::Arc;
use std::sync::OnceLock;

use architect::action::ActionBackend;
use daw::service::ProjectContext;
use daw::service::transport::service::Transport as TransportService;
use daw::service::{Markers, Projects, Regions, TempoMap};
use session_proto::{AdvanceMode, Setlist, Song};
use tokio::sync::RwLock;

use crate::setlist::service::SetlistServiceImpl;
use crate::song::builder::SongBuilder;
use session_proto::setlist_actions::{SetlistActions, register_setlist_actions};

/// The setlist storage cell shared with `SetlistServiceImpl`.
static SETLIST_STORE: OnceLock<Arc<RwLock<Option<Setlist>>>> = OnceLock::new();

/// Stash the `SetlistServiceImpl`'s setlist cell so the action
/// handler writes to the same `Arc<RwLock<>>` the RPC service reads.
///
/// Idempotent — first call wins; later calls ignored so re-mounting
/// (shouldn't happen but defensive) can't blow up plugin startup.
pub fn register<D>(svc: &SetlistServiceImpl<D>) {
    let _ = SETLIST_STORE.set(svc.setlist.clone());
}

/// Stamp the canonical demo markers + section regions into the current
/// REAPER project (3 songs of typical worship-set structure: count-in,
/// song-start, verse/pre/chorus/bridge/outro regions, song-end, =END
/// render bound), then rebuild the cached `Setlist` so it immediately
/// reflects what was stamped.
///
/// All work is sync; no spawning, no Tokio runtime needed — the caller
/// is a REAPER action handler already on the main thread.
fn load_demo<D>(daw: &D)
where
    D: Projects + TransportService + Markers + Regions + TempoMap,
{
    tracing::info!("[session] load_demo_setlist action — stamping demo markers/regions");
    let started = architect::platform::now();
    match crate::setlist::service::demo::stamp_demo_setlist_with(daw) {
        Ok(()) => {
            tracing::info!(
                "[session] demo markers stamped in {:?}; rebuilding setlist",
                started.elapsed()
            );
            build(daw);
        }
        Err(e) => {
            tracing::warn!("[session] load_demo_setlist: stamping failed: {e:?}");
        }
    }
}

/// Scan open project tabs, parse SONGSTART/SONGEND markers + section
/// regions, and write the result into the cached `Setlist`. Idempotent —
/// rerun to pick up edits.
fn build<D>(daw: &D)
where
    D: Projects,
{
    tracing::info!("[session] build_setlist action — building synchronously");
    let started = architect::platform::now();
    let setlist = build_setlist_sync(daw);
    let song_count = setlist.songs.len();
    match SETLIST_STORE.get() {
        Some(slot) => match slot.try_write() {
            Ok(mut guard) => {
                *guard = Some(setlist);
                tracing::info!(
                    "[session] build_setlist action completed ({} songs in {:?})",
                    song_count,
                    started.elapsed()
                );
            }
            Err(_) => {
                tracing::warn!(
                    "[session] build_setlist: setlist cell busy — \
                     another writer is mid-build, skipping"
                );
            }
        },
        None => {
            tracing::warn!(
                "[session] build_setlist action: setlist store not \
                 registered (mounted_services_with_daw never called?)"
            );
        }
    }
}

/// Walk every open REAPER project tab and synthesise a `Song` from
/// each via `SongBuilder::build_native` (markers + regions + lanes —
/// no chart / fingerprint hydration). Returns an empty setlist when
/// nothing is open. Sync because callers are REAPER action handlers.
fn build_setlist_sync<D>(daw: &D) -> Setlist
where
    D: Projects,
{
    let projects = daw.list();
    let mut songs: Vec<Song> = Vec::with_capacity(projects.len());
    for project in projects {
        let ctx = ProjectContext::Project(project.guid.clone());
        match SongBuilder::build_native(ctx) {
            Ok(mut project_songs) => {
                tracing::debug!(
                    project = %project.name,
                    guid = %project.guid,
                    songs = project_songs.len(),
                    "[session] built songs from project"
                );
                songs.append(&mut project_songs);
            }
            Err(err) => {
                tracing::warn!(
                    project = %project.name,
                    guid = %project.guid,
                    "[session] SongBuilder failed for project: {err}"
                );
            }
        }
    }

    Setlist {
        id: None,
        name: format!("Setlist - {}", chrono::Local::now().format("%Y-%m-%d")),
        advance_mode: AdvanceMode::default(),
        songs,
    }
}

/// Log every marker and region in the current REAPER project, sorted
/// by lane then position, so the user (or me, debugging via tail-log)
/// can confirm what landed where after a build / demo / hand-edit.
/// Sync on the main thread — same reason the rest of dispatch is sync.
fn dump_ruler_state<D>(daw: &D)
where
    D: Projects + Markers + Regions,
{
    let project = ProjectContext::Current;

    // Lane table first. Probe 0..=10 because REAPER may have an
    // implicit lane at index 0 (the "automatic" slot) that
    // `ruler_lane_count` (which stops at the first empty name)
    // hides from us, and the user wants to know about it. Also
    // pull RULER_LANE_FLAGS:N so we can see which lane REAPER
    // considers the default marker / region target.
    tracing::info!("[session] === lanes (name_key_index 0-based) ===");
    for idx in 0u32..=6 {
        let name = daw.get_ruler_lane_name(project.clone(), idx);
        let flags = daw.get_project_info(project.clone(), &format!("RULER_LANE_FLAGS:{idx}"));
        let hidden = daw.get_project_info(project.clone(), &format!("RULER_LANE_HIDDEN:{idx}"));
        tracing::info!(
            "[session] lane {idx}: name={:?} flags={} hidden={}",
            name,
            flags,
            hidden,
        );
    }

    let markers = Markers::all(daw, project.clone());
    let regions = Regions::all(daw, project);

    tracing::info!(
        "[session] ruler state: {} marker(s), {} region(s)",
        markers.len(),
        regions.len()
    );

    // Markers — sort by (lane, position) so the dump groups by lane.
    let mut sorted_markers: Vec<_> = markers.iter().collect();
    sorted_markers.sort_by(|a, b| {
        let lane_ord = a.lane.unwrap_or(0).cmp(&b.lane.unwrap_or(0));
        lane_ord.then_with(|| {
            let ap = a.position.time.map_or(0.0, |t| t.as_seconds());
            let bp = b.position.time.map_or(0.0, |t| t.as_seconds());
            ap.partial_cmp(&bp).unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    tracing::info!("[session] === markers ===");
    for m in &sorted_markers {
        let pos = m.position.time.map_or(0.0, |t| t.as_seconds());
        tracing::info!(
            "[session] marker  lane={:?}  pos={:>7.3}  color={:?}  name={:?}  id={:?}",
            m.lane,
            pos,
            m.color,
            m.name,
            m.id,
        );
    }

    // Regions — same shape, plus end position.
    let mut sorted_regions: Vec<_> = regions.iter().collect();
    sorted_regions.sort_by(|a, b| {
        let lane_ord = a.lane.unwrap_or(0).cmp(&b.lane.unwrap_or(0));
        lane_ord.then_with(|| {
            let ap = a.time_range.start.time.map_or(0.0, |t| t.as_seconds());
            let bp = b.time_range.start.time.map_or(0.0, |t| t.as_seconds());
            ap.partial_cmp(&bp).unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    tracing::info!("[session] === regions ===");
    for r in &sorted_regions {
        let start = r.time_range.start.time.map_or(0.0, |t| t.as_seconds());
        let end = r.time_range.end.time.map_or(0.0, |t| t.as_seconds());
        tracing::info!(
            "[session] region  lane={:?}  start={:>7.3}  end={:>7.3}  color={:?}  name={:?}  id={:?}",
            r.lane,
            start,
            end,
            r.color,
            r.name,
            r.id,
        );
    }
}

// ── architect::actions implementation ───────────────────────────────────
//
// The contract lives in `session_proto::setlist_actions`. There is no
// longer a parallel `SetlistAction` enum / `action_for_id` / `dispatch`
// path, and no `session_actions` `define_actions!` entries declaring the
// same `FTS_SESSION_*` command ids a second time.

/// Serves the three setlist actions against a `daw` backend.
pub struct SetlistActionsImpl<D> {
    daw: D,
}

impl<D> SetlistActions for SetlistActionsImpl<D>
where
    D: Projects + TransportService + Markers + Regions + TempoMap,
{
    fn build_setlist(&self) {
        build(&self.daw);
    }

    fn load_demo_setlist(&self) {
        load_demo(&self.daw);
    }

    fn dump_ruler_state(&self) {
        dump_ruler_state(&self.daw);
    }
}

/// Registers all three setlist actions with `backend`.
///
/// Dispatches each through a fresh `SetlistActionsImpl` bound to `daw`.
/// `backend` can be a REAPER `ActionBackend`, a CLI command-tree builder,
/// an in-memory test double, etc. Call once at module init, alongside [`register`].
///
/// Once `daw-reaper` grows an `ActionBackend` impl (architect migration
/// phase 2), the call site is simply:
/// `setlist_actions::register_actions(&reaper_backend, self.daw.clone())`.
pub fn register_actions<D, B>(backend: &B, daw: D)
where
    D: Projects + TransportService + Markers + Regions + TempoMap + Send + Sync + 'static,
    B: ActionBackend + ?Sized,
{
    register_setlist_actions(backend, Arc::new(SetlistActionsImpl { daw }));
}
