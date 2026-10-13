//! Take ranking — set a take's rank marker (`:)`, `:))`, `:)))`, `:(`)
//! to a specific level in one shot, at a chosen scope.
//!
//! Contract only. The implementation drives a live backend and lives in
//! `daw-actions` (`daw_actions::take_ranking`), which is also where the
//! internal `Scope` / `RankAction` enums live — the types here are the
//! wire-side mirror.

use facet::Facet;

/// Scope for [`TakeRankingService::apply_rank`]. Wire-side mirror of
/// the `Scope` enum in `daw_actions::take_ranking`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum TakeRankScope {
    /// Active take of each selected item, marker at `(play_pos - 2s)`
    /// when playing or at edit cursor when stopped.
    PlayPosMinus2s,
    /// Active take of each selected item, marker at item start.
    ItemWide,
    /// Take under the mouse cursor, marker at mouse project-time.
    MouseCursor,
}

/// Rank level: 1..=3 stars (up-rank) or `Down`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum TakeRankLevel {
    One,
    Two,
    Three,
    Down,
}

pub mod take_ranking_service {
    use super::{TakeRankLevel, TakeRankScope};
    use crate::DawError;

    #[architect::rpc]
    pub trait TakeRankingService {
        /// Apply `level` at `scope`. Behavior matches the
        /// `daw_actions::take_ranking::apply` semantics: replace-if-near
        /// for position-based scopes, single marker per take for
        /// item-wide.
        async fn apply_rank(
            &self,
            scope: TakeRankScope,
            level: TakeRankLevel,
        ) -> Result<(), DawError>;
    }
}

#[cfg(feature = "vox")]
pub use take_ranking_service::TakeRankingServiceClient;
pub use take_ranking_service::{
    Service as TakeRankingServiceLayer, TakeRankingService, TakeRankingServiceDispatcher,
    layer as take_ranking_service_layer, serve as serve_take_ranking_service,
    take_ranking_service_rpc_service_descriptor, take_ranking_service_service_descriptor,
};

// ── Actions ─────────────────────────────────────────────────────────────

#[architect::actions(namespace = "FTS_SESSION")]
pub trait TakeRankingActions {
    #[action(
        description = "Set the active take's rank marker to :) at (play-pos - 2s) on every selected item, or at edit cursor if not playing",
        category = "Project",
        group = "Take Ranking"
    )]
    fn take_rank_playpos_1(&self);
    #[action(
        description = "Set the active take's rank marker to :)) at (play-pos - 2s) on every selected item, or at edit cursor if not playing",
        category = "Project",
        group = "Take Ranking"
    )]
    fn take_rank_playpos_2(&self);
    #[action(
        description = "Set the active take's rank marker to :))) at (play-pos - 2s) on every selected item, or at edit cursor if not playing",
        category = "Project",
        group = "Take Ranking"
    )]
    fn take_rank_playpos_3(&self);
    #[action(
        description = "Set the active take's rank marker to :( at (play-pos - 2s) on every selected item, or at edit cursor if not playing",
        category = "Project",
        group = "Take Ranking"
    )]
    fn take_rank_playpos_down(&self);
    #[action(
        description = "Set the active take's rank marker to :) at item start for every selected item",
        category = "Project",
        group = "Take Ranking"
    )]
    fn take_rank_item_1(&self);
    #[action(
        description = "Set the active take's rank marker to :)) at item start for every selected item",
        category = "Project",
        group = "Take Ranking"
    )]
    fn take_rank_item_2(&self);
    #[action(
        description = "Set the active take's rank marker to :))) at item start for every selected item",
        category = "Project",
        group = "Take Ranking"
    )]
    fn take_rank_item_3(&self);
    #[action(
        description = "Set the active take's rank marker to :( at item start for every selected item",
        category = "Project",
        group = "Take Ranking"
    )]
    fn take_rank_item_down(&self);
    #[action(
        description = "Set the rank marker to :) on the take under the mouse at the mouse's project-time position",
        category = "Project",
        group = "Take Ranking"
    )]
    fn take_rank_mouse_1(&self);
    #[action(
        description = "Set the rank marker to :)) on the take under the mouse at the mouse's project-time position",
        category = "Project",
        group = "Take Ranking"
    )]
    fn take_rank_mouse_2(&self);
    #[action(
        description = "Set the rank marker to :))) on the take under the mouse at the mouse's project-time position",
        category = "Project",
        group = "Take Ranking"
    )]
    fn take_rank_mouse_3(&self);
    #[action(
        description = "Set the rank marker to :( on the take under the mouse at the mouse's project-time position",
        category = "Project",
        group = "Take Ranking"
    )]
    fn take_rank_mouse_down(&self);
}

#[cfg(test)]
mod takeranking_id_tests {
    use super::*;

    /// The exact REAPER command-name strings. These predate the move out
    /// of `daw-actions` and must survive it — keybindings, toolbars and
    /// `extension_loads.rs` all depend on them.
    #[test]
    fn ids_match_pre_move_command_ids() {
        let ids: Vec<_> = TakeRankingActionsActions::all()
            .iter()
            .map(|m| m.id)
            .collect();
        assert_eq!(
            ids,
            vec![
                "FTS_SESSION_TAKE_RANK_PLAYPOS_1",
                "FTS_SESSION_TAKE_RANK_PLAYPOS_2",
                "FTS_SESSION_TAKE_RANK_PLAYPOS_3",
                "FTS_SESSION_TAKE_RANK_PLAYPOS_DOWN",
                "FTS_SESSION_TAKE_RANK_ITEM_1",
                "FTS_SESSION_TAKE_RANK_ITEM_2",
                "FTS_SESSION_TAKE_RANK_ITEM_3",
                "FTS_SESSION_TAKE_RANK_ITEM_DOWN",
                "FTS_SESSION_TAKE_RANK_MOUSE_1",
                "FTS_SESSION_TAKE_RANK_MOUSE_2",
                "FTS_SESSION_TAKE_RANK_MOUSE_3",
                "FTS_SESSION_TAKE_RANK_MOUSE_DOWN",
            ]
        );
    }
}
