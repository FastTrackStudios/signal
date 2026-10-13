//! Implementation of [`session_proto::playback::PlaybackActions`] —
//! `toggle_playback` and `toggle_song_loop`.
//!
//! Sibling to `setlist_actions.rs`: runs synchronously on REAPER's main
//! thread (where the action callback fires), calling `D`'s sync
//! `Transport` methods directly — no `main_thread::query` bounce, no
//! Tokio runtime. The contract, and the reasoning for why the setlist
//! navigation commands stay RPC-only, live in the proto module.

use std::sync::Arc;

use architect::action::ActionBackend;
use daw::service::ProjectContext;
use daw::service::transport::service::Transport as TransportService;
use session_proto::playback::{PlaybackActions, register_playback_actions};

/// Forwards to the same `Transport` calls
/// `SetlistServiceImpl::toggle_playback_impl` / `toggle_song_loop_impl`
/// make (`setlist_service/playback.rs`).
pub struct PlaybackActionsImpl<D> {
    daw: D,
}

impl<D> PlaybackActions for PlaybackActionsImpl<D>
where
    D: TransportService,
{
    fn toggle_playback(&self) {
        if let Err(e) = self.daw.play_pause(ProjectContext::Current) {
            tracing::warn!("[session] toggle_playback action: failed to toggle: {e}");
        }
    }

    fn toggle_song_loop(&self) {
        if let Err(e) = self.daw.toggle_loop(ProjectContext::Current) {
            tracing::warn!("[session] toggle_song_loop action: failed to toggle: {e}");
        }
    }
}

/// Registers `toggle_playback` / `toggle_song_loop` with `backend`. Call
/// once at module init, alongside `setlist_actions::register_actions`.
pub fn register_actions<D, B>(backend: &B, daw: D)
where
    D: TransportService + Send + Sync + 'static,
    B: ActionBackend + ?Sized,
{
    register_playback_actions(backend, Arc::new(PlaybackActionsImpl { daw }));
}
