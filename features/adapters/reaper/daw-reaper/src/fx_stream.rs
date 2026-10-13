//! FX state polling + broadcasting.
//!
//! Mirrors the `item.rs` per-domain broadcaster pattern: own static
//! sender + cache, `init_fx_broadcaster()` / `subscribe_fx()` /
//! `poll_and_broadcast_fx()` API surface. Called from the
//! daw-bridge timer on the main thread.
//!
//! Diff strategy:
//! - For each open project, for every track, build `Track(guid)` and
//!   `Input(guid)` chains; also `Monitoring` once per poll.
//! - For each chain, call `Effects::list` and compare against the
//!   cached snapshot keyed by chain.
//! - Emit `Added`, `Removed`, `EnabledChanged` per the chain-level diff.
//!
//! TODO: parameter-level diffs (`ParameterChanged`) are not emitted.
//! Polling every parameter on every FX every tick is expensive; the
//! cheap alternative — polling only the last-touched FX — needs a
//! `Effects::last_touched()` call that we'd want to debounce. Leaving
//! the apply side wired for `ParameterChanged` so a future polling
//! source (or an explicit hook from REAPER's param-change callback)
//! can publish without further plumbing.

use daw_control::lock::LockExt;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use daw_proto::{Effects, Fx, FxChainContext, FxEvent, ProjectContext, Tracks};
use reaper_high::{Project, Reaper as ReaperHigh};
use reaper_medium::ProjectRef;
use tokio::sync::broadcast;

use crate::project_context::{MAX_PROJECT_TABS, project_guid};

/// Cached lightweight FX snapshot used for diffing.
#[derive(Clone, Debug)]
struct CachedFx {
    guid: String,
    index: u32,
    name: String,
    enabled: bool,
}

impl From<&Fx> for CachedFx {
    fn from(fx: &Fx) -> Self {
        Self {
            guid: fx.guid.clone(),
            index: fx.index,
            name: fx.name.clone(),
            enabled: fx.enabled,
        }
    }
}

/// Cache key: (project_guid, chain_key). chain_key is a stringified
/// FxChainContext discriminant so Monitoring / Track(g) / Input(g) all
/// hash distinctly.
type ChainKey = (String, String);

static FX_BROADCASTER: OnceLock<broadcast::Sender<FxEvent>> = OnceLock::new();
static FX_CACHE: OnceLock<Mutex<HashMap<ChainKey, Vec<CachedFx>>>> = OnceLock::new();

fn chain_key(project_guid: &str, ctx: &FxChainContext) -> ChainKey {
    let chain_part = match ctx {
        FxChainContext::Track(g) => format!("track:{g}"),
        FxChainContext::Input(g) => format!("input:{g}"),
        FxChainContext::Monitoring => "monitoring".to_string(),
    };
    (project_guid.to_string(), chain_part)
}

/// Initialize the FX event broadcaster. Idempotent.
pub fn init_fx_broadcaster() {
    let (tx, _rx) = broadcast::channel::<FxEvent>(1024);
    let _ = FX_BROADCASTER.set(tx);
    let _ = FX_CACHE.set(Mutex::new(HashMap::new()));
}

/// Subscribe to FX events. Returns `None` until [`init_fx_broadcaster`] runs.
pub fn subscribe_fx() -> Option<broadcast::Receiver<FxEvent>> {
    FX_BROADCASTER.get().map(|tx| tx.subscribe())
}

/// Push-published FX event from the control-surface callback path.
/// For `EnabledChanged` we also update the per-chain cache so the next
/// poll tick won't re-emit the same diff. Skips publish if the affected
/// fx isn't cached yet — the follower hasn't seen the Added event for
/// it, so emitting EnabledChanged ahead of the create floods the apply
/// pipeline with not-found lookups and starves the create event. The
/// next poll tick picks up both the FX + its initial state in order.
///
/// `ParameterChanged` and `PresetChanged` aren't cached (we don't poll
/// params), so they always go out — callback is the only path.
/// Main-thread only.
pub(crate) fn publish_from_callback(event: FxEvent) {
    let Some(tx) = FX_BROADCASTER.get() else {
        return;
    };
    if let FxEvent::EnabledChanged {
        context,
        fx_guid,
        enabled,
    } = &event
    {
        let Some(cache_cell) = FX_CACHE.get() else {
            return;
        };
        let cached = match cache_cell.lock() {
            Ok(mut cache) => {
                let chain_part = match context {
                    FxChainContext::Track(g) => format!("track:{g}"),
                    FxChainContext::Input(g) => format!("input:{g}"),
                    FxChainContext::Monitoring => "monitoring".to_string(),
                };
                let mut found = false;
                for ((_, cp), chain) in cache.iter_mut() {
                    if cp == &chain_part
                        && let Some(c) = chain.iter_mut().find(|c| &c.guid == fx_guid)
                    {
                        c.enabled = *enabled;
                        found = true;
                    }
                }
                found
            }
            Err(_) => false,
        };
        if !cached {
            return;
        }
    }
    let _ = tx.send(event);
}

/// Send an FX event to the in-process broadcaster **and** the event bus.
///
/// Both, from one call, so a publish site cannot feed one and forget the
/// other — which is exactly how the bus ended up missing this domain.
fn emit(tx: &broadcast::Sender<FxEvent>, project_guid: &str, event: FxEvent) {
    let _ = tx.send(event.clone());
    crate::event_hub::hub().publish_fx(daw_proto::fx::FxStreamEvent {
        project_guid: project_guid.to_string(),
        event,
    });
}

/// Poll REAPER FX chain state for every open project. **Main thread only.**
pub fn poll_and_broadcast_fx() {
    let Some(tx) = FX_BROADCASTER.get() else {
        return;
    };
    // Either audience keeps the poller alive: the in-process broadcaster
    // subscribers, or anyone on the cross-domain bus. Gating on the
    // broadcaster alone meant a bus subscriber saw nothing at all.
    if tx.receiver_count() == 0 && crate::event_hub::hub().fx_subscriber_count() == 0 {
        return;
    }
    let Some(cache_cell) = FX_CACHE.get() else {
        return;
    };
    let mut cache = cache_cell.lock_recoverable("fx_stream::cache");

    let reaper_high = ReaperHigh::get();
    let medium = reaper_high.medium_reaper();

    let mut seen_keys: Vec<ChainKey> = Vec::new();

    for tab_index in 0..MAX_PROJECT_TABS {
        let Some(result) = medium.enum_projects(ProjectRef::Tab(tab_index), 0) else {
            break;
        };
        let project = Project::new(result.project);
        let project_guid_str = project_guid(&project);
        let project_ctx = ProjectContext::Project(project_guid_str.clone());

        // Enumerate chains: Track + Input per track, plus Monitoring once.
        let tracks = Tracks::all(&crate::Reaper, project_ctx.clone());
        let mut chains: Vec<FxChainContext> = Vec::with_capacity(tracks.len() * 2 + 1);
        for t in &tracks {
            chains.push(FxChainContext::Track(t.guid.clone()));
            chains.push(FxChainContext::Input(t.guid.clone()));
        }
        chains.push(FxChainContext::Monitoring);

        for chain in chains {
            let key = chain_key(&project_guid_str, &chain);
            seen_keys.push(key.clone());

            let fresh = Effects::list(&crate::Reaper, project_ctx.clone(), chain.clone());
            let fresh_cached: Vec<CachedFx> = fresh.iter().map(CachedFx::from).collect();

            match cache.get(&key) {
                None => {
                    // First poll for this chain — seed cache, don't emit.
                }
                Some(prev) => {
                    diff_and_emit(tx, &project_guid_str, &chain, prev, &fresh_cached, &fresh);
                }
            }
            cache.insert(key, fresh_cached);
        }
    }

    // GC entries for chains that no longer exist (e.g. project closed,
    // track removed).
    cache.retain(|k, _| seen_keys.contains(k));
}

fn diff_and_emit(
    tx: &broadcast::Sender<FxEvent>,
    project_guid: &str,
    ctx: &FxChainContext,
    prev: &[CachedFx],
    curr_cached: &[CachedFx],
    curr_full: &[Fx],
) {
    use std::collections::HashMap;

    let prev_by_guid: HashMap<&str, &CachedFx> =
        prev.iter().map(|f| (f.guid.as_str(), f)).collect();
    let curr_by_guid: HashMap<&str, &CachedFx> =
        curr_cached.iter().map(|f| (f.guid.as_str(), f)).collect();

    // Removed: in prev but not in curr.
    for p in prev {
        if !p.guid.is_empty() && !curr_by_guid.contains_key(p.guid.as_str()) {
            emit(
                tx,
                project_guid,
                FxEvent::Removed {
                    context: ctx.clone(),
                    fx_guid: p.guid.clone(),
                },
            );
        }
    }

    // Added / EnabledChanged: walk curr.
    for (i, c) in curr_cached.iter().enumerate() {
        if c.guid.is_empty() {
            continue;
        }
        match prev_by_guid.get(c.guid.as_str()) {
            None => {
                // Added — send the full Fx struct from curr_full.
                if let Some(fx) = curr_full.get(i) {
                    emit(
                        tx,
                        project_guid,
                        FxEvent::Added {
                            context: ctx.clone(),
                            fx: fx.clone(),
                        },
                    );
                }
            }
            Some(prev) => {
                if prev.enabled != c.enabled {
                    emit(
                        tx,
                        project_guid,
                        FxEvent::EnabledChanged {
                            context: ctx.clone(),
                            fx_guid: c.guid.clone(),
                            enabled: c.enabled,
                        },
                    );
                }
                // Moved: index changed. Emit if the chain length stayed
                // the same — otherwise add/remove dominates and the
                // index shift is just bookkeeping.
                if prev.index != c.index && prev.name == c.name {
                    emit(
                        tx,
                        project_guid,
                        FxEvent::Moved {
                            context: ctx.clone(),
                            fx_guid: c.guid.clone(),
                            old_index: prev.index,
                            new_index: c.index,
                        },
                    );
                }
            }
        }
    }
}
