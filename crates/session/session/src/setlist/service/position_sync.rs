//! Bidirectional position sync between song tabs and the combined setlist tab.
//!
//! Monitors transport state across all open project tabs. When the user seeks
//! or plays in one tab, the corresponding position is computed via the
//! [`SetlistOffsetMap`] and applied to the other side:
//!
//! - **Song → Setlist**: Seek in song tab → compute global position → seek setlist tab
//! - **Setlist → Song**: Seek in setlist tab → compute (`song_index`, `local_pos`) → switch + seek song tab
//!
//! Echo suppression prevents infinite feedback loops.

use architect::platform::Instant;
use daw::rpc::{Daw, Project};
use session_proto::offset_map::SetlistOffsetMap;
use std::time::Duration;
use tracing::debug;

/// Tracks suppression state to prevent echo loops.
struct Suppression {
    /// GUID of the project we last wrote a position to.
    last_target_guid: Option<String>,
    /// When we last applied a position change.
    last_apply_time: Instant,
    /// Suppression window — ignore changes from the target within this window.
    window: Duration,
}

impl Suppression {
    fn new() -> Self {
        Self {
            last_target_guid: None,
            // web_time::Instant is page-load-relative on wasm; `now() - 10s`
            // underflow-panics there. Saturate to now() (native never underflows).
            last_apply_time: architect::platform::now()
                .checked_sub(Duration::from_secs(10))
                .unwrap_or_else(architect::platform::now),
            window: Duration::from_millis(200),
        }
    }

    /// Record that we applied a position change to the given project.
    fn suppress(&mut self, target_guid: &str) {
        self.last_target_guid = Some(target_guid.to_string());
        self.last_apply_time = architect::platform::now();
    }

    /// Check if a change from the given project should be ignored (echo).
    fn is_suppressed(&self, source_guid: &str) -> bool {
        if self.last_apply_time.elapsed() > self.window {
            return false;
        }
        self.last_target_guid
            .as_deref()
            .is_some_and(|g| g == source_guid)
    }
}

/// The position sync bridge.
///
/// Call `tick()` from the session polling loop (~30-60Hz).
pub struct PositionSyncBridge {
    /// The offset map for position translation.
    offset_map: SetlistOffsetMap,
    /// GUID of the combined setlist project (if open).
    setlist_guid: Option<String>,
    /// Last known positions per project GUID.
    last_positions: std::collections::HashMap<String, f64>,
    /// Last known playing state per project GUID.
    last_playing: std::collections::HashMap<String, bool>,
    /// Echo suppression.
    suppression: Suppression,
    /// Whether sync is enabled.
    enabled: bool,
}

#[allow(dead_code)]
impl PositionSyncBridge {
    /// Create a new bridge with the given offset map and setlist project GUID.
    #[must_use]
    pub fn new(offset_map: SetlistOffsetMap, setlist_guid: Option<String>) -> Self {
        Self {
            offset_map,
            setlist_guid,
            last_positions: std::collections::HashMap::new(),
            last_playing: std::collections::HashMap::new(),
            suppression: Suppression::new(),
            enabled: true,
        }
    }

    /// Update the offset map (e.g., after setlist rebuild).
    pub fn set_offset_map(&mut self, map: SetlistOffsetMap) {
        self.offset_map = map;
    }

    /// Set the GUID of the combined setlist project.
    pub fn set_setlist_guid(&mut self, guid: Option<String>) {
        self.setlist_guid = guid;
    }

    /// Enable/disable position sync.
    pub const fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Main tick — detect position changes and sync across projects.
    ///
    /// Call from the session polling loop with the current transport states.
    pub async fn tick(&mut self, daw: &Daw) {
        if !self.enabled {
            return;
        }

        let setlist_guid = match &self.setlist_guid {
            Some(g) => g.clone(),
            None => return, // No setlist project open
        };

        // Get all open projects
        let Ok(projects) = daw.projects().await else {
            return;
        };

        // Collect current positions
        for project in &projects {
            let guid = project.guid().to_string();
            let Ok(transport) = project.transport().get_state().await else {
                continue;
            };

            let pos = transport
                .playhead_position
                .time
                .as_ref()
                .map(daw_proto::PositionInSeconds::as_seconds)
                .or_else(|| {
                    transport
                        .edit_position
                        .time
                        .as_ref()
                        .map(daw_proto::PositionInSeconds::as_seconds)
                })
                .unwrap_or(0.0);

            let playing = transport.is_playing();
            let prev_pos = self.last_positions.get(&guid).copied().unwrap_or(0.0);
            let prev_playing = self.last_playing.get(&guid).copied().unwrap_or(false);

            // Detect significant position change (not just playback advancing)
            let pos_delta = (pos - prev_pos).abs();
            let is_seek = !playing && pos_delta > 0.01 // Cursor moved while stopped
                || playing && pos_delta > 1.0; // Large jump while playing

            if is_seek && !self.suppression.is_suppressed(&guid) {
                if guid == setlist_guid {
                    // Setlist → Song: translate global → local
                    self.sync_setlist_to_song(daw, &projects, pos, &setlist_guid)
                        .await;
                } else {
                    // Song → Setlist: translate local → global
                    self.sync_song_to_setlist(daw, &guid, pos, &setlist_guid)
                        .await;
                }
            }

            // Detect play/stop changes
            if playing != prev_playing && !self.suppression.is_suppressed(&guid) {
                if guid == setlist_guid {
                    // Setlist started/stopped → mirror to active song
                    self.sync_play_state_to_song(daw, &projects, playing, pos, &setlist_guid)
                        .await;
                } else {
                    // Song started/stopped → mirror to setlist
                    self.sync_play_state_to_setlist(daw, &guid, playing, &setlist_guid)
                        .await;
                }
            }

            self.last_positions.insert(guid.clone(), pos);
            self.last_playing.insert(guid, playing);
        }
    }

    /// Song tab seeked → update setlist tab to corresponding global position.
    async fn sync_song_to_setlist(
        &mut self,
        daw: &Daw,
        song_guid: &str,
        local_pos: f64,
        setlist_guid: &str,
    ) {
        // Find which song this project corresponds to
        let Some(song_offset) = self.offset_map.song_by_guid(song_guid) else {
            return;
        };

        let global_pos = song_offset.global_start_seconds + local_pos;

        debug!(
            "Position sync: song {} @ {:.2}s → setlist @ {:.2}s",
            song_offset.index, local_pos, global_pos
        );

        // Apply to setlist project
        if let Ok(setlist_project) = daw.project(setlist_guid).await {
            self.suppression.suppress(setlist_guid);
            let _ = setlist_project.transport().set_position(global_pos).await;
        }
    }

    /// Setlist tab seeked → find the right song tab and seek it.
    async fn sync_setlist_to_song(
        &mut self,
        daw: &Daw,
        projects: &[Project],
        global_pos: f64,
        _setlist_guid: &str,
    ) {
        let Some((song_index, local_pos)) = self.offset_map.setlist_to_project(global_pos) else {
            return;
        };

        let Some(song_offset) = self.offset_map.songs.get(song_index) else {
            return;
        };

        debug!(
            "Position sync: setlist @ {:.2}s → song {} @ {:.2}s",
            global_pos, song_index, local_pos
        );

        // Find the song project by GUID and seek it
        for project in projects {
            if project.guid() == song_offset.project_guid {
                self.suppression.suppress(&song_offset.project_guid);
                let _ = project.transport().set_position(local_pos).await;

                // Also select this project tab so the user sees it
                let _ = daw.select_project(&song_offset.project_guid).await;
                return;
            }
        }
    }

    /// Song play state changed → mirror to setlist.
    async fn sync_play_state_to_setlist(
        &mut self,
        daw: &Daw,
        _song_guid: &str,
        playing: bool,
        setlist_guid: &str,
    ) {
        if let Ok(setlist) = daw.project(setlist_guid).await {
            self.suppression.suppress(setlist_guid);
            let transport = setlist.transport();
            if playing {
                let _ = transport.play().await;
            } else {
                let _ = transport.stop().await;
            }
        }
    }

    /// Setlist play state changed → mirror to active song.
    async fn sync_play_state_to_song(
        &mut self,
        _daw: &Daw,
        projects: &[Project],
        playing: bool,
        global_pos: f64,
        _setlist_guid: &str,
    ) {
        let Some((song_index, _local_pos)) = self.offset_map.setlist_to_project(global_pos) else {
            return;
        };

        let Some(song_offset) = self.offset_map.songs.get(song_index) else {
            return;
        };

        for project in projects {
            if project.guid() == song_offset.project_guid {
                self.suppression.suppress(&song_offset.project_guid);
                let transport = project.transport();
                if playing {
                    let _ = transport.play().await;
                } else {
                    let _ = transport.stop().await;
                }
                return;
            }
        }
    }
}
