//! Take ranking actions — set a take's REAPER rank marker (`:)`, `:))`,
//! `:)))`, or `:(`) to a specific level in one shot. REAPER's native
//! ranking actions only bump one step at a time; we manipulate the take
//! markers directly via `SetTakeMarker`.
//!
//! Three scopes × four levels = 12 actions:
//!
//! - **Play-position-minus-2s**: target = `(play_pos - 2s)` if playing,
//!   else edit cursor. Acts on the active take of every selected item
//!   that contains that project position.
//! - **Item-wide**: acts on the active take of every selected item; the
//!   marker is anchored at source position 0.
//! - **Mouse**: acts on the take under the mouse cursor; marker is
//!   placed at the mouse's project-time position.
//!
//! Marker write semantics: any existing rank marker (name parses as a
//! [`TakeRating`]) within ±0.5s of the target source position is
//! renamed in place; otherwise a new marker is appended. This keeps
//! repeated keystrokes at the same spot idempotent while still allowing
//! a take to carry multiple rank regions for comping workflows.

use daw::service::take_ranking::{TakeRankingActions, register_take_ranking_actions};
use daw::service::transport::service::Transport as _;
use daw::service::{
    ItemRef, Items as _, PlayState, ProjectContext, Projects as _, TakeMarkerCreate,
    TakeMarkerUpdate, TakeRating, TakeRef, Takes as _,
};
use daw_reaper::safe_wrappers::mouse::MouseSnapshot;
use tracing::{debug, info};

const REPLACE_WINDOW_SECS: f64 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RankAction {
    PlayPos1,
    PlayPos2,
    PlayPos3,
    PlayPosDown,
    Item1,
    Item2,
    Item3,
    ItemDown,
    Mouse1,
    Mouse2,
    Mouse3,
    MouseDown,
}

impl RankAction {
    fn scope(self) -> Scope {
        match self {
            Self::PlayPos1 | Self::PlayPos2 | Self::PlayPos3 | Self::PlayPosDown => {
                Scope::PlayPosMinus2s
            }
            Self::Item1 | Self::Item2 | Self::Item3 | Self::ItemDown => Scope::ItemWide,
            Self::Mouse1 | Self::Mouse2 | Self::Mouse3 | Self::MouseDown => Scope::MouseCursor,
        }
    }

    fn rating(self) -> TakeRating {
        match self {
            Self::PlayPos1 | Self::Item1 | Self::Mouse1 => TakeRating::UpRank(1),
            Self::PlayPos2 | Self::Item2 | Self::Mouse2 => TakeRating::UpRank(2),
            Self::PlayPos3 | Self::Item3 | Self::Mouse3 => TakeRating::UpRank(3),
            Self::PlayPosDown | Self::ItemDown | Self::MouseDown => TakeRating::DownRank,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Scope {
    PlayPosMinus2s,
    ItemWide,
    MouseCursor,
}

impl Scope {
    fn label(self) -> &'static str {
        match self {
            Scope::PlayPosMinus2s => "play-pos-2s",
            Scope::ItemWide => "item-wide",
            Scope::MouseCursor => "mouse",
        }
    }
}

pub fn dispatch(action: RankAction) {
    apply(action.scope(), action.rating());
}

// ─── Core ────────────────────────────────────────────────────────────────────

/// One take to mark: which take (item + take ref) and the source-time
/// position the rank marker anchors at.
struct Target {
    item: ItemRef,
    take: TakeRef,
    source_pos: f64,
}

fn apply(scope: Scope, rating: TakeRating) {
    let daw = studio::reaper::Reaper;
    let marker_name = rating.to_marker_name();

    let targets: Vec<Target> = match scope {
        Scope::PlayPosMinus2s => targets_play_pos_minus_2s(&daw),
        Scope::ItemWide => targets_item_wide(&daw),
        Scope::MouseCursor => targets_mouse_cursor().into_iter().collect(),
    };

    if targets.is_empty() {
        debug!(scope = scope.label(), rank = %marker_name, "[take-rank] No targets");
        return;
    }

    let undo = format!("Take rank: {} ({})", marker_name, scope.label());
    daw.begin_undo_block(ProjectContext::Current, &undo);
    for t in &targets {
        write_rank_marker(&daw, &t.item, &t.take, &marker_name, t.source_pos);
    }
    daw.end_undo_block(ProjectContext::Current, &undo, None);

    info!(
        scope = scope.label(),
        rank = %marker_name,
        targets = targets.len(),
        "[take-rank] Applied"
    );
}

/// Write (or replace-in-place) a rank marker on `take`. Any existing rank
/// marker within ±`REPLACE_WINDOW_SECS` of `source_pos` is renamed; else a
/// new marker is appended.
fn write_rank_marker(
    daw: &daw_reaper::Reaper,
    item: &ItemRef,
    take: &TakeRef,
    name: &str,
    source_pos: f64,
) {
    let markers = daw.get_take_markers(ProjectContext::Current, item.clone(), take.clone());
    let existing = markers.iter().find(|m| {
        TakeRating::from_marker_name(&m.name).is_some()
            && (m.source_position_seconds - source_pos).abs() <= REPLACE_WINDOW_SECS
    });
    match existing {
        Some(m) => {
            let _ = daw.set_take_marker(
                ProjectContext::Current,
                item.clone(),
                take.clone(),
                TakeMarkerUpdate {
                    index: m.index,
                    name: Some(name.to_string()),
                    source_position_seconds: Some(source_pos),
                    color: None,
                    ..Default::default()
                },
            );
            debug!(
                action = "update",
                index = m.index,
                source_pos,
                new = name,
                "[take-rank] Wrote marker"
            );
        }
        None => {
            let _ = daw.add_take_marker(
                ProjectContext::Current,
                item.clone(),
                take.clone(),
                TakeMarkerCreate {
                    name: name.to_string(),
                    source_position_seconds: source_pos,
                    color: None,
                    ..Default::default()
                },
            );
            debug!(action = "add", source_pos, name, "[take-rank] Wrote marker");
        }
    }
}

// ─── Scope target resolvers ─────────────────────────────────────────────────

/// Target = `(play_pos - 2s)` while playing/recording, else the edit cursor
/// (`Transport::get_position` returns play-or-edit). Acts on the active take
/// of each selected item that spans that project position.
fn targets_play_pos_minus_2s(daw: &daw_reaper::Reaper) -> Vec<Target> {
    let playing = matches!(
        daw.get_play_state(ProjectContext::Current),
        PlayState::Playing | PlayState::Recording
    );
    let cursor = daw.get_position(ProjectContext::Current);
    let target_pos = if playing {
        (cursor - 2.0).max(0.0)
    } else {
        cursor
    };

    let mut out = Vec::new();
    for it in daw.get_selected_items(ProjectContext::Current) {
        let start = it.position.as_seconds();
        let end = start + it.length.as_seconds();
        if target_pos < start || target_pos > end {
            continue;
        }
        let item = ItemRef::Guid(it.guid);
        let Some(take) = daw.get_active_take(ProjectContext::Current, item.clone()) else {
            continue;
        };
        let src = (target_pos - start) * take.play_rate + take.start_offset.as_seconds();
        if src < 0.0 {
            continue;
        }
        out.push(Target {
            item,
            take: TakeRef::Active,
            source_pos: src,
        });
    }
    out
}

/// Active take of every selected item; marker anchored at source 0.
fn targets_item_wide(daw: &daw_reaper::Reaper) -> Vec<Target> {
    daw.get_selected_items(ProjectContext::Current)
        .into_iter()
        .map(|it| Target {
            item: ItemRef::Guid(it.guid),
            take: TakeRef::Active,
            source_pos: 0.0,
        })
        .collect()
}

/// Take under the mouse cursor + its source-time position. Local-only (a
/// remote client has no REAPER mouse cursor), so this uses the daw-reaper
/// `MouseSnapshot` safe wrapper rather than a domain trait.
fn targets_mouse_cursor() -> Option<Target> {
    let snap = MouseSnapshot::capture();
    let (Some(item_guid), Some(take_guid), Some(source_pos)) = (
        snap.item_guid(),
        snap.take_guid(),
        snap.take_source_position(),
    ) else {
        debug!(
            item = snap.item.is_some(),
            take = snap.take.is_some(),
            "[take-rank] mouse: no take under cursor"
        );
        return None;
    };
    Some(Target {
        item: ItemRef::Guid(item_guid),
        take: TakeRef::Guid(take_guid),
        source_pos,
    })
}

// ── architect::actions implementation ───────────────────────────────────
//
// The action enum + `dispatch` below stay: they are the shared body every
// action method (and, where one exists, the RPC service impl) calls into.
// What's gone is the string-keyed `action_for_id` lookup and the
// `session_actions` `define_actions!` entries that declared the same
// `FTS_SESSION_*` command ids a second time.

/// Bridges the twelve take-ranking actions onto `#[architect::actions]`.
/// Every method forwards to the existing synchronous `dispatch` — no
/// behavior change, just a declarative front door with real metadata.
pub struct TakeRankingActionsImpl;

impl TakeRankingActions for TakeRankingActionsImpl {
    fn take_rank_playpos_1(&self) {
        dispatch(RankAction::PlayPos1);
    }
    fn take_rank_playpos_2(&self) {
        dispatch(RankAction::PlayPos2);
    }
    fn take_rank_playpos_3(&self) {
        dispatch(RankAction::PlayPos3);
    }
    fn take_rank_playpos_down(&self) {
        dispatch(RankAction::PlayPosDown);
    }
    fn take_rank_item_1(&self) {
        dispatch(RankAction::Item1);
    }
    fn take_rank_item_2(&self) {
        dispatch(RankAction::Item2);
    }
    fn take_rank_item_3(&self) {
        dispatch(RankAction::Item3);
    }
    fn take_rank_item_down(&self) {
        dispatch(RankAction::ItemDown);
    }
    fn take_rank_mouse_1(&self) {
        dispatch(RankAction::Mouse1);
    }
    fn take_rank_mouse_2(&self) {
        dispatch(RankAction::Mouse2);
    }
    fn take_rank_mouse_3(&self) {
        dispatch(RankAction::Mouse3);
    }
    fn take_rank_mouse_down(&self) {
        dispatch(RankAction::MouseDown);
    }
}

/// Registers all twelve take-ranking actions with `backend`.
pub fn register_actions<B>(backend: &B)
where
    B: ::architect::action::ActionBackend + ?Sized,
{
    register_take_ranking_actions(backend, std::sync::Arc::new(TakeRankingActionsImpl));
}

// ─── RPC service ───────────────────────────────────────────────────────

/// [`daw::service::TakeRankingService`] over the same handlers the
/// keyboard actions use. Every call bounces to REAPER's main thread —
/// the handlers below touch main-thread-only FFI, while the async RPC
/// dispatcher runs on the tokio runtime.
#[derive(Clone, Default)]
pub struct TakeRankingServiceImpl;

impl daw::service::TakeRankingService for TakeRankingServiceImpl {
    async fn apply_rank(
        &self,
        scope: daw::service::TakeRankScope,
        level: daw::service::TakeRankLevel,
    ) -> Result<(), daw::service::DawError> {
        let action = rank_action_for(scope, level);
        daw::main_thread::query(move || dispatch(action))
            .await
            .ok_or(daw::service::DawError::MainThreadUnavailable)?;
        Ok(())
    }
}

fn rank_action_for(
    scope: daw::service::TakeRankScope,
    level: daw::service::TakeRankLevel,
) -> RankAction {
    use daw::service::TakeRankLevel::*;
    use daw::service::TakeRankScope::*;
    match (scope, level) {
        (PlayPosMinus2s, One) => RankAction::PlayPos1,
        (PlayPosMinus2s, Two) => RankAction::PlayPos2,
        (PlayPosMinus2s, Three) => RankAction::PlayPos3,
        (PlayPosMinus2s, Down) => RankAction::PlayPosDown,
        (ItemWide, One) => RankAction::Item1,
        (ItemWide, Two) => RankAction::Item2,
        (ItemWide, Three) => RankAction::Item3,
        (ItemWide, Down) => RankAction::ItemDown,
        (MouseCursor, One) => RankAction::Mouse1,
        (MouseCursor, Two) => RankAction::Mouse2,
        (MouseCursor, Three) => RankAction::Mouse3,
        (MouseCursor, Down) => RankAction::MouseDown,
    }
}
