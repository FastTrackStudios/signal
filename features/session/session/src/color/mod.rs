//! Auto-colour — classify tracks and paint them, reactively.
//!
//! Session owns colour because colour is about to stop being a pure
//! function of a track's name: section-aware and setlist-aware colouring
//! need song structure, and only this crate has it.
//!
//! Two halves, deliberately separable:
//!
//! - [`classify`] decides what colour a track *should* be. It runs names
//!   through `monarchy_sort` and looks the resulting group path up in
//!   `music_catalog`'s palette — the same taxonomy the track organiser
//!   uses, so colours and grouping agree by construction rather than by
//!   two rule sets being kept in sync by hand.
//! - this module is the runtime around it: reactive re-application on
//!   track events, persistence of what auto-colour applied (so a colour
//!   the *user* set is never clobbered), the enable/disable toggle, and
//!   parent-colour inheritance for tracks the classifier has no opinion
//!   about.
//!
//! Previously this was two separate implementations — `daw_actions::auto_color`
//! (this runtime, with its own hand-rolled substring rules) and
//! `dynamic_template::auto_color` (the monarchy classifier, with no
//! runtime). Both registered actions; both appeared in REAPER's action
//! list. This is the merge: that runtime, that classifier.
//!
//! Contract in [`session_proto::color`].

pub mod classify;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use daw::service::{ExtState, ProjectContext, Projects, TrackEvent, TrackRef, Tracks};
use session_proto::color::{AutoColorActions, register_auto_color_actions};
use tokio::sync::broadcast::error::RecvError;

static STATE: OnceLock<Arc<AutoColorState>> = OnceLock::new();
static TIMER_REGISTERED: AtomicBool = AtomicBool::new(false);

const EXT_STATE_SECTION: &str = "FastTrackStudio.Session.AutoColor";
const EXT_STATE_ENABLED_KEY: &str = "enabled";
const PROJECT_STATE_SECTION: &str = "FTSAUTOCOLOR";
const PROJECT_STATE_APPLIED_COLORS_KEY: &str = "tracks";
const LEGACY_PROJECT_STATE_SECTION: &str = "FastTrackStudio.Session.AutoColor";
const LEGACY_PROJECT_STATE_APPLIED_COLORS_KEY: &str = "applied_colors";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoColorAction {
    ColorAll,
    ColorSelected,
    Toggle,
    ClearAll,
    ClearSelected,
}

struct AutoColorState {
    enabled: AtomicBool,
    subscribed: AtomicBool,
    pending_current: AtomicBool,
    pending_projects: Mutex<HashSet<String>>,
    applied_colors: Mutex<HashMap<String, u32>>,
    loaded_projects: Mutex<HashSet<String>>,
}

impl AutoColorState {
    fn new() -> Self {
        Self {
            enabled: AtomicBool::new(true),
            subscribed: AtomicBool::new(false),
            pending_current: AtomicBool::new(false),
            pending_projects: Mutex::new(HashSet::new()),
            applied_colors: Mutex::new(HashMap::new()),
            loaded_projects: Mutex::new(HashSet::new()),
        }
    }
}

/// What auto-colour decided for one track. `None` means "no opinion" —
/// the classifier didn't place it and no coloured parent was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AutoColorDecision {
    pub color: Option<u32>,
}

pub fn init(ctx: &daw::module::ModuleContext) {
    let _ = ctx;
    let _ = STATE.set(Arc::new(AutoColorState::new()));
    register_timer();
}

pub fn dispatch(action: AutoColorAction) {
    if let Err(err) = run_action(action) {
        tracing::error!(?action, ?err, "[session] Auto-color action failed");
    }
}

pub fn subscribe(ctx: &daw::module::ModuleContext) {
    let _ = STATE.set(Arc::new(AutoColorState::new()));
    register_timer();
    tracing::info!("[session] Auto-color subscribe: timer registered");

    let enabled = load_enabled();
    set_enabled(enabled);
    tracing::info!(enabled, "[session] Auto-color loaded enabled state");

    ctx.spawn(async {
        match ensure_reactive_updates() {
            Ok(()) => tracing::info!("[session] Auto-color reactive subscription started"),
            Err(err) => {
                tracing::warn!(?err, "[session] Failed to start auto-color subscription");
            }
        }

        let enabled = state()
            .map(|state| state.enabled.load(Ordering::Relaxed))
            .unwrap_or(false);
        tracing::info!(
            enabled,
            "[session] Auto-color post-subscribe state; will schedule initial apply if enabled"
        );
        if enabled {
            schedule_current_project_apply();
        }
    });
}

fn run_action(action: AutoColorAction) -> eyre::Result<()> {
    match action {
        AutoColorAction::ColorAll => {
            set_enabled(true);
            save_enabled(true)?;
            ensure_reactive_updates()?;
            apply_to_current_project(false, true)
        }
        AutoColorAction::ColorSelected => apply_to_current_project(true, true),
        AutoColorAction::Toggle => {
            let enabled = !state()?.enabled.load(Ordering::Relaxed);
            set_enabled(enabled);
            save_enabled(enabled)?;
            if enabled {
                ensure_reactive_updates()?;
                schedule_current_project_apply();
            }
            Ok(())
        }
        AutoColorAction::ClearAll => {
            set_enabled(false);
            save_enabled(false)?;
            clear_current_project(false)
        }
        AutoColorAction::ClearSelected => clear_current_project(true),
    }
}

fn ensure_reactive_updates() -> eyre::Result<()> {
    let state = state()?;
    if state.subscribed.swap(true, Ordering::AcqRel) {
        return Ok(());
    }

    let mut rx = daw::reaper::event_hub().subscribe_tracks();

    let recv_loop = async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if should_schedule(&event.event) {
                        schedule_project_apply(event.project_guid);
                    }
                }
                Err(RecvError::Lagged(skipped)) => {
                    tracing::debug!(skipped, "[session] Auto-color sync stream lagged");
                    schedule_current_project_apply();
                }
                Err(RecvError::Closed) => break,
            }
        }
    };

    // `ensure_reactive_updates` is reached from two contexts:
    // - a REAPER action callback (main thread, no Tokio runtime): bounce
    //   through `daw::block_on` to enter the DAW runtime and `tokio::spawn`.
    // - `subscribe`'s `ctx.spawn` task (already on the DAW runtime): spawn on
    //   the ambient handle. Calling `daw::block_on` here would panic
    //   ("Cannot block_on from within a runtime") and silently kill the
    //   subscribe task — auto-color then never applies on startup or track
    //   add, despite being enabled.
    let spawned = match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            handle.spawn(recv_loop);
            true
        }
        Err(_) => daw::block_on(async move {
            tokio::spawn(recv_loop);
        })
        .is_some(),
    };

    if !spawned {
        state.subscribed.store(false, Ordering::Release);
        eyre::bail!("daw runtime not initialised; auto-color subscription not started");
    }

    Ok(())
}

const fn should_schedule(event: &TrackEvent) -> bool {
    matches!(
        event,
        TrackEvent::Added(_)
            | TrackEvent::Removed(_)
            | TrackEvent::Renamed { .. }
            | TrackEvent::Moved { .. }
    )
}

fn schedule_current_project_apply() {
    let Ok(state) = state() else {
        return;
    };
    if !state.enabled.load(Ordering::Relaxed) {
        return;
    }
    state.pending_current.store(true, Ordering::Relaxed);
}

fn schedule_project_apply(project_guid: String) {
    let Ok(state) = state().cloned() else {
        return;
    };
    if !state.enabled.load(Ordering::Relaxed) {
        return;
    }

    if let Ok(mut pending) = state.pending_projects.lock() {
        pending.insert(project_guid);
    }
}

fn register_timer() {
    if TIMER_REGISTERED.swap(true, Ordering::AcqRel) {
        return;
    }
    daw::register_timer(auto_color_timer);
}

fn auto_color_timer() {
    let Ok(state) = state().cloned() else {
        return;
    };
    if !state.enabled.load(Ordering::Relaxed) {
        return;
    }

    if state.pending_current.swap(false, Ordering::Relaxed) {
        tracing::info!("[session] Auto-color timer: applying to current project");
        if let Err(err) = apply_to_current_project(false, false) {
            tracing::warn!(?err, "[session] Startup auto-color pass failed");
        }
    }

    let pending_projects = match state.pending_projects.lock() {
        Ok(mut pending) => pending.drain().collect::<Vec<_>>(),
        Err(err) => {
            tracing::warn!(?err, "[session] Auto-color pending project lock poisoned");
            Vec::new()
        }
    };

    if !pending_projects.is_empty() {
        tracing::info!(
            count = pending_projects.len(),
            "[session] Auto-color timer: applying to projects from event hub"
        );
    }
    for project_guid in pending_projects {
        if let Err(err) = apply_to_project_guid(&project_guid, false, false) {
            tracing::warn!(?err, project_guid, "[session] Reactive auto-color failed");
        }
    }
}

fn apply_to_current_project(selected_only: bool, force: bool) -> eyre::Result<()> {
    apply_to_project(&ProjectContext::Current, selected_only, force)
}

fn apply_to_project_guid(project_guid: &str, selected_only: bool, force: bool) -> eyre::Result<()> {
    apply_to_project(
        &ProjectContext::Project(project_guid.to_string()),
        selected_only,
        force,
    )
}

fn apply_to_project(
    project: &ProjectContext,
    selected_only: bool,
    force: bool,
) -> eyre::Result<()> {
    let state = state()?.clone();
    let project_key = project_key(project);
    load_applied_colors_for_project(project, &project_key)?;
    let all = daw::reaper::Reaper.all(project.clone());
    let selected: HashSet<String> = if selected_only {
        daw::reaper::Reaper
            .selected(project.clone())
            .into_iter()
            .map(|track| track.guid)
            .collect()
    } else {
        HashSet::new()
    };

    let decisions = decide_colors(&all);
    let mut changed = 0usize;
    let mut applied_colors = state
        .applied_colors
        .lock()
        .map_err(|err| eyre::eyre!("Auto-color applied color lock poisoned: {err}"))?;
    let live_guids: HashSet<&str> = all.iter().map(|track| track.guid.as_str()).collect();
    applied_colors.retain(|key, _| {
        let Some((key_project, guid)) = key.split_once(':') else {
            return false;
        };
        key_project != project_key || live_guids.contains(guid)
    });

    for track in &all {
        if selected_only && !selected.contains(&track.guid) {
            continue;
        }
        let key = auto_color_key(&project_key, &track.guid);
        let current = track.color.unwrap_or(0);
        let previous = applied_colors.get(&key).copied().unwrap_or(0);
        let desired = decisions
            .get(&track.guid)
            .and_then(|decision| decision.color);

        if let Some(desired) = desired {
            if current != desired && (force || current == previous) {
                daw::reaper::Reaper.set_color(
                    project.clone(),
                    TrackRef::Guid(track.guid.clone()),
                    desired,
                )?;
                changed = changed.saturating_add(1);
            }
            applied_colors.insert(key, desired);
        } else if previous != 0 {
            if current == previous {
                daw::reaper::Reaper.set_color(
                    project.clone(),
                    TrackRef::Guid(track.guid.clone()),
                    0,
                )?;
                changed = changed.saturating_add(1);
            }
            applied_colors.remove(&key);
        }
    }
    let applied_colors_snapshot = applied_colors.clone();
    drop(applied_colors);

    save_applied_colors_for_project(&project, &project_key, &applied_colors_snapshot)?;

    tracing::debug!(
        changed,
        selected_only,
        force,
        "[session] Auto-color pass complete"
    );
    Ok(())
}

fn clear_current_project(selected_only: bool) -> eyre::Result<()> {
    let project = ProjectContext::Current;
    let project_key = project_key(&project);
    let selected: HashSet<String> = if selected_only {
        daw::reaper::Reaper
            .selected(project.clone())
            .into_iter()
            .map(|track| track.guid)
            .collect()
    } else {
        HashSet::new()
    };

    for track in daw::reaper::Reaper.all(project.clone()) {
        if selected_only && !selected.contains(&track.guid) {
            continue;
        }
        remove_applied_color(&project_key, &track.guid);
        if track.color.is_some() {
            daw::reaper::Reaper.set_color(project.clone(), TrackRef::Guid(track.guid), 0)?;
        }
    }
    save_current_applied_colors_for_project(&project, &project_key)?;
    Ok(())
}

fn state() -> eyre::Result<&'static Arc<AutoColorState>> {
    STATE
        .get()
        .ok_or_else(|| eyre::eyre!("Auto-color state not initialized"))
}

fn set_enabled(enabled: bool) {
    if let Some(state) = STATE.get() {
        state.enabled.store(enabled, Ordering::Relaxed);
    }
}

fn project_key(project: &ProjectContext) -> String {
    match project {
        ProjectContext::Current => daw::reaper::Reaper
            .current()
            .map_or_else(|| "current".to_string(), |project| project.guid),
        ProjectContext::Project(guid) => guid.clone(),
    }
}

fn auto_color_key(project_key: &str, track_guid: &str) -> String {
    format!("{project_key}:{track_guid}")
}

fn remove_applied_color(project_key: &str, track_guid: &str) {
    let Some(state) = STATE.get() else {
        return;
    };
    let key = auto_color_key(project_key, track_guid);
    if let Ok(mut applied_colors) = state.applied_colors.lock() {
        applied_colors.remove(&key);
    }
}

fn load_applied_colors_for_project(
    project: &ProjectContext,
    project_key: &str,
) -> eyre::Result<()> {
    let state = state()?.clone();
    {
        let mut loaded = state
            .loaded_projects
            .lock()
            .map_err(|err| eyre::eyre!("Auto-color loaded project lock poisoned: {err}"))?;
        if !loaded.insert(project_key.to_string()) {
            return Ok(());
        }
    }

    let Some(serialized) = ExtState::get_project(
        &daw::reaper::Reaper,
        project.clone(),
        PROJECT_STATE_SECTION,
        PROJECT_STATE_APPLIED_COLORS_KEY,
    )
    .or_else(|| {
        ExtState::get_project(
            &daw::reaper::Reaper,
            project.clone(),
            LEGACY_PROJECT_STATE_SECTION,
            LEGACY_PROJECT_STATE_APPLIED_COLORS_KEY,
        )
    }) else {
        return Ok(());
    };

    let mut applied_colors = state
        .applied_colors
        .lock()
        .map_err(|err| eyre::eyre!("Auto-color applied color lock poisoned: {err}"))?;
    for line in serialized.lines() {
        let Some((guid, color)) = line.split_once('=') else {
            continue;
        };
        let color = color.trim().trim_start_matches("0x");
        let Ok(color) = u32::from_str_radix(color, 16) else {
            continue;
        };
        if color != 0 {
            applied_colors.insert(auto_color_key(project_key, guid.trim()), color);
        }
    }
    drop(applied_colors);
    Ok(())
}

fn save_current_applied_colors_for_project(
    project: &ProjectContext,
    project_key: &str,
) -> eyre::Result<()> {
    let state = state()?.clone();
    let applied_colors = state
        .applied_colors
        .lock()
        .map_err(|err| eyre::eyre!("Auto-color applied color lock poisoned: {err}"))?;
    save_applied_colors_for_project(project, project_key, &applied_colors)
}

fn save_applied_colors_for_project(
    project: &ProjectContext,
    project_key: &str,
    applied_colors: &HashMap<String, u32>,
) -> eyre::Result<()> {
    let prefix = format!("{project_key}:");
    let mut entries = applied_colors
        .iter()
        .filter_map(|(key, color)| {
            let guid = key.strip_prefix(&prefix)?;
            Some((guid, *color))
        })
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| a.0.cmp(b.0));

    let serialized = entries
        .into_iter()
        .map(|(guid, color)| format!("{guid}=0x{color:06X}"))
        .collect::<Vec<_>>()
        .join("\n");

    if serialized.is_empty() {
        ExtState::delete_project(
            &daw::reaper::Reaper,
            project.clone(),
            PROJECT_STATE_SECTION,
            PROJECT_STATE_APPLIED_COLORS_KEY,
        )?;
    } else {
        ExtState::set_project(
            &daw::reaper::Reaper,
            project.clone(),
            PROJECT_STATE_SECTION,
            PROJECT_STATE_APPLIED_COLORS_KEY,
            &serialized,
        )?;
    }
    Ok(())
}

fn load_enabled() -> bool {
    let Some(value) = ExtState::get(
        &daw::reaper::Reaper,
        EXT_STATE_SECTION,
        EXT_STATE_ENABLED_KEY,
    ) else {
        return true;
    };
    !matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "0" | "false" | "off" | "no"
    )
}

fn save_enabled(enabled: bool) -> eyre::Result<()> {
    ExtState::set(
        &daw::reaper::Reaper,
        EXT_STATE_SECTION,
        EXT_STATE_ENABLED_KEY,
        if enabled { "true" } else { "false" },
        true,
    )?;
    Ok(())
}

pub(crate) fn decide_colors(tracks: &[daw::service::Track]) -> HashMap<String, AutoColorDecision> {
    let by_guid: HashMap<&str, &daw::service::Track> = tracks
        .iter()
        .map(|track| (track.guid.as_str(), track))
        .collect();
    let by_name = classify::colors_by_track_name(tracks);
    let explicit: HashMap<&str, u32> = tracks
        .iter()
        .filter_map(|track| {
            by_name
                .get(track.name.as_str())
                .map(|color| (track.guid.as_str(), *color))
        })
        .collect();

    tracks
        .iter()
        .map(|track| {
            let color = explicit
                .get(track.guid.as_str())
                .copied()
                .or_else(|| inherited_parent_color(track, &by_guid, &explicit));
            (track.guid.clone(), AutoColorDecision { color })
        })
        .collect()
}

fn inherited_parent_color(
    track: &daw::service::Track,
    by_guid: &HashMap<&str, &daw::service::Track>,
    explicit: &HashMap<&str, u32>,
) -> Option<u32> {
    let mut current = track.parent_guid.as_deref();
    while let Some(parent_guid) = current {
        if let Some(color) = explicit.get(parent_guid) {
            return Some(*color);
        }
        current = by_guid
            .get(parent_guid)
            .and_then(|parent| parent.parent_guid.as_deref());
    }
    None
}

// ── architect::actions implementation ───────────────────────────────────
//
// Contract in `session_proto::color`.

/// Serves the five auto-colour actions.
pub struct AutoColorActionsImpl;

impl AutoColorActions for AutoColorActionsImpl {
    fn auto_color_color_all(&self) {
        dispatch(AutoColorAction::ColorAll);
    }
    fn auto_color_color_selected(&self) {
        dispatch(AutoColorAction::ColorSelected);
    }
    fn auto_color_toggle(&self) {
        dispatch(AutoColorAction::Toggle);
    }
    fn auto_color_clear_all(&self) {
        dispatch(AutoColorAction::ClearAll);
    }
    fn auto_color_clear_selected(&self) {
        dispatch(AutoColorAction::ClearSelected);
    }
}

/// Registers all five auto-color actions with `backend`.
pub fn register_actions<B>(backend: &B)
where
    B: ::architect::action::ActionBackend + ?Sized,
{
    register_auto_color_actions(backend, std::sync::Arc::new(AutoColorActionsImpl));
}

#[cfg(test)]
mod tests {
    use daw::service::Track;

    use super::*;

    fn track(guid: &str, index: u32, name: &str, parent_guid: Option<&str>) -> Track {
        let mut track = Track::new(guid.to_string(), index, name.to_string());
        track.parent_guid = parent_guid.map(str::to_string);
        track
    }

    /// Colours are asserted against `music_catalog` rather than literal
    /// hex. The whole reason this module merged two implementations is
    /// that the old runtime carried its *own* colour table which had
    /// drifted from the shared palette — "Guitars" was green here and
    /// blue everywhere else in the app. Hardcoding hex would let that
    /// happen again silently.
    fn guitars() -> u32 {
        music_catalog::lookup::color_for_name("Guitars")
            .expect("Guitars is a known group")
            .to_hex()
    }

    #[test]
    fn folder_track_named_after_a_group_takes_that_group_color() {
        let tracks = vec![track("folder", 0, "Guitars", None)];
        assert_eq!(decide_colors(&tracks)["folder"].color, Some(guitars()));
    }

    /// A track the classifier has no opinion about ("57" — an SM57 mic
    /// name) inherits from the nearest coloured ancestor.
    #[test]
    fn child_inherits_nearest_colored_parent() {
        let tracks = vec![
            track("folder", 0, "Guitars", None),
            track("child", 1, "57", Some("folder")),
        ];

        let decisions = decide_colors(&tracks);

        assert_eq!(decisions["folder"].color, Some(guitars()));
        assert_eq!(decisions["child"].color, Some(guitars()));
    }

    #[test]
    fn explicit_child_color_overrides_parent() {
        let tracks = vec![
            track("folder", 0, "Guitars", None),
            track("child", 1, "Lead Vocal", Some("folder")),
        ];

        let child = decide_colors(&tracks)["child"].color;

        assert!(child.is_some());
        assert_ne!(child, Some(guitars()), "vocal should not inherit Guitars");
    }
}
