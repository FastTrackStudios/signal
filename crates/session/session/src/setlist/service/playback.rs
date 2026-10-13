//! Playback commands: play, pause, stop, toggle, loop controls

use super::SetlistServiceImpl;
use daw::service::ProjectContext;
use daw::service::transport::service::Transport;
use session_proto::SessionServiceError;
use tracing::{debug, warn};

impl<D> SetlistServiceImpl<D>
where
    D: Transport,
{
    pub(crate) async fn toggle_playback_impl(&self) -> Result<(), SessionServiceError>
    where
        D: architect::MaybeSendSync,
    {
        debug!("toggle_playback");

        // Use cached active song ID for instant lookup (no RPC calls)
        if let Some(song) = self.get_cached_active_song().await {
            if let Err(e) = self
                .daw
                .play_pause(ProjectContext::Project(song.project_guid))
            {
                warn!("Failed to toggle playback: {}", e);
            }
        } else {
            warn!("No active song to toggle playback (navigate to a song first)");
        }
        Ok(())
    }

    pub(crate) async fn play_impl(&self) -> Result<(), SessionServiceError>
    where
        D: architect::MaybeSendSync,
    {
        debug!("play");

        // Use cached active song ID for instant lookup (no RPC calls)
        if let Some(song) = self.get_cached_active_song().await {
            if let Err(e) = self.daw.play(ProjectContext::Project(song.project_guid)) {
                warn!("Failed to play: {}", e);
            }
        } else {
            warn!("No active song to play (navigate to a song first)");
        }
        Ok(())
    }

    pub(crate) async fn pause_impl(&self) -> Result<(), SessionServiceError>
    where
        D: architect::MaybeSendSync,
    {
        debug!("pause");

        // Use cached active song ID for instant lookup (no RPC calls)
        if let Some(song) = self.get_cached_active_song().await {
            if let Err(e) = self.daw.pause(ProjectContext::Project(song.project_guid)) {
                warn!("Failed to pause: {}", e);
            }
        } else {
            warn!("No active song to pause (navigate to a song first)");
        }
        Ok(())
    }

    pub(crate) async fn stop_impl(&self) -> Result<(), SessionServiceError>
    where
        D: architect::MaybeSendSync,
    {
        debug!("stop");

        // Use cached active song ID for instant lookup (no RPC calls)
        if let Some(song) = self.get_cached_active_song().await {
            if let Err(e) = self.daw.stop(ProjectContext::Project(song.project_guid)) {
                warn!("Failed to stop: {}", e);
            }
        } else {
            warn!("No active song to stop (navigate to a song first)");
        }
        Ok(())
    }

    pub(crate) fn toggle_song_loop_impl(&self) {
        debug!("toggle_song_loop");

        if let Err(e) = self.daw.toggle_loop(ProjectContext::Current) {
            warn!("Failed to toggle song loop: {}", e);
        }
    }

    pub(crate) fn toggle_section_loop_impl() {
        debug!("toggle_section_loop");
        // TODO: Implement section-specific loop using loop points
        warn!("toggle_section_loop not yet implemented");
    }

    pub(crate) fn set_loop_region_impl(start_seconds: f64, end_seconds: f64) {
        debug!("set_loop_region: {} - {}", start_seconds, end_seconds);
        // TODO: Implement setting loop region
        warn!("set_loop_region not yet implemented");
    }

    pub(crate) fn clear_loop_impl(&self) {
        debug!("clear_loop");

        if let Err(e) = self.daw.set_loop(ProjectContext::Current, false) {
            warn!("Failed to clear loop: {}", e);
        }
    }
}
