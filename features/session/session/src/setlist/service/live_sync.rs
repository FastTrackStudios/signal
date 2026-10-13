//! Live setlist sync — watches individual song RPPs and applies changes
//! to the combined setlist in real time.
//!
//! ## Flow
//!
//! 1. [`SongFileWatcher`] detects a song RPP was saved to disk
//! 2. Parse the updated song RPP
//! 3. Diff against the song's section in the current setlist (with offset)
//! 4. Apply the diff to the setlist RPP in memory
//! 5. Write the updated setlist to disk
//! 6. Signal REAPER to reload the setlist project tab

#![allow(dead_code)]

use std::path::PathBuf;

use daw::file::diff::{self, ApplyOptions, DiffOptions};
use daw::file::io::read_project;
use daw::file::types::ReaperProject;
use session_proto::SetlistOffsetMap;
use tracing::{debug, info, warn};

use super::file_watcher::{SongFileChanged, SongFileWatcher};

/// State for live setlist sync.
pub struct LiveSyncState {
    /// Paths to individual song RPPs (indexed by song position in setlist)
    song_paths: Vec<PathBuf>,
    /// Path to the combined setlist RPP
    setlist_path: PathBuf,
    /// Offset map for position translation
    offset_map: SetlistOffsetMap,
    /// Last-known parsed version of each song (for diffing)
    song_snapshots: Vec<Option<ReaperProject>>,
    /// Current combined setlist in memory
    setlist_project: ReaperProject,
}

impl LiveSyncState {
    /// Create a new sync state from song paths and a generated setlist.
    #[must_use]
    pub fn new(
        song_paths: Vec<PathBuf>,
        setlist_path: PathBuf,
        offset_map: SetlistOffsetMap,
        setlist_project: ReaperProject,
    ) -> Self {
        // Take initial snapshots of each song
        let song_snapshots: Vec<Option<ReaperProject>> =
            song_paths.iter().map(|p| read_project(p).ok()).collect();

        Self {
            song_paths,
            setlist_path,
            offset_map,
            song_snapshots,
            setlist_project,
        }
    }

    /// Handle a song file change: diff, apply, write.
    ///
    /// Returns `true` if the setlist was updated.
    pub fn handle_song_changed(&mut self, event: &SongFileChanged) -> bool {
        let idx = event.song_index;
        if idx >= self.song_paths.len() {
            warn!(
                "Song index {} out of range (have {} songs)",
                idx,
                self.song_paths.len()
            );
            return false;
        }

        // Parse the updated song
        let new_song = match read_project(&event.path) {
            Ok(project) => project,
            Err(e) => {
                warn!(
                    "Failed to parse updated song RPP {}: {}",
                    event.path.display(),
                    e
                );
                return false;
            }
        };

        // Get the old snapshot (or skip if we don't have one)
        let Some(old_song) = self.song_snapshots.get(idx).and_then(|opt| opt.as_ref()) else {
            info!(
                "No previous snapshot for song {}, taking initial snapshot",
                idx
            );
            if let Some(slot) = self.song_snapshots.get_mut(idx) {
                *slot = Some(new_song);
            }
            return false;
        };

        // Get offset for this song
        let song_offset = self
            .offset_map
            .songs
            .get(idx)
            .map_or(0.0, |s| s.global_start_seconds);
        let song_end = self
            .offset_map
            .songs
            .get(idx)
            .map_or(f64::MAX, |s| s.global_start_seconds + s.duration_seconds);

        // Diff the old song against the setlist section
        let diff_options = DiffOptions {
            position_offset: song_offset,
            time_window: Some((song_offset, song_end)),
            match_tracks_by_name: true,
            match_items_by_name: true,
        };

        let project_diff =
            diff::diff_projects_with_options(old_song, &self.setlist_project, &diff_options);

        if project_diff.is_empty() {
            debug!("No changes detected for song {} ({})", idx, event.song_name);
            if let Some(slot) = self.song_snapshots.get_mut(idx) {
                *slot = Some(new_song);
            }
            return false;
        }

        info!(
            "Song {} ({}) changed: {}",
            idx,
            event.song_name,
            project_diff.summary()
        );

        // Check if the change is structural (requires full regeneration)
        if project_diff.is_structural() {
            info!(
                "Song {} ({}) has structural changes — triggering full setlist regeneration",
                idx, event.song_name
            );
            if let Some(slot) = self.song_snapshots.get_mut(idx) {
                *slot = Some(new_song);
            }
            return self.regenerate_setlist();
        }

        // Incremental patch: diff the NEW song against the setlist
        let new_diff =
            diff::diff_projects_with_options(&new_song, &self.setlist_project, &diff_options);

        // Apply the diff to the setlist
        let apply_options = ApplyOptions {
            position_offset: song_offset,
            match_tracks_by_name: true,
        };

        diff::apply::apply_diff(&mut self.setlist_project, &new_diff, &apply_options);

        // Update the snapshot
        if let Some(slot) = self.song_snapshots.get_mut(idx) {
            *slot = Some(new_song);
        }

        // Write the updated setlist to disk
        self.write_setlist();

        true
    }

    /// Full setlist regeneration — re-reads all song RPPs and rebuilds the
    /// combined setlist from scratch. Used when structural changes make
    /// incremental patching insufficient.
    fn regenerate_setlist(&mut self) -> bool {
        use daw::file::setlist_rpp::{
            build_song_infos_from_projects, concatenate_projects, measures_to_seconds,
        };
        use session_proto::{
            SongId,
            offset_map::{SetlistOffsetMap, SongOffset},
        };

        // Re-read all song RPPs
        let mut projects = Vec::new();
        let mut names = Vec::new();
        for (i, path) in self.song_paths.iter().enumerate() {
            match read_project(path) {
                Ok(project) => {
                    projects.push(project.clone());
                    names.push(format!("Song {}", i.saturating_add(1)));
                    if let Some(slot) = self.song_snapshots.get_mut(i) {
                        *slot = Some(project);
                    }
                }
                Err(e) => {
                    warn!("Failed to re-read song {}: {}", path.display(), e);
                    return false;
                }
            }
        }

        // Rebuild with 2-measure gap at 120 BPM (TODO: make configurable)
        let gap = measures_to_seconds(2, 120.0, 4);
        let name_refs: Vec<&str> = names.iter().map(std::string::String::as_str).collect();
        let song_infos = build_song_infos_from_projects(&projects, &name_refs, gap);
        let combined = concatenate_projects(&projects, &song_infos);

        // Update the offset map
        self.offset_map = SetlistOffsetMap {
            songs: song_infos
                .iter()
                .enumerate()
                .map(|(i, si)| SongOffset {
                    index: i,
                    song_id: SongId::new(),
                    project_guid: String::new(),
                    global_start_seconds: si.global_start_seconds,
                    global_start_qn: si.global_start_seconds * 2.0,
                    duration_seconds: si.duration_seconds,
                    duration_qn: si.duration_seconds * 2.0,
                    count_in_seconds: 0.0,
                    start_tempo: 120.0,
                    start_time_sig: daw::service::TimeSignature::new(4, 4),
                })
                .collect(),
            total_seconds: song_infos
                .last()
                .map_or(0.0, |s| s.global_start_seconds + s.duration_seconds),
            total_qn: 0.0,
        };

        self.setlist_project = combined;
        self.write_setlist();

        info!("Setlist regenerated from {} songs", projects.len());
        true
    }

    /// Write the current setlist project to disk, plus shell copies for each role.
    fn write_setlist(&self) {
        use daw::file::setlist_rpp::{generate_shell_copy, project_to_rpp_text};

        // Write master setlist
        let rpp_text = project_to_rpp_text(&self.setlist_project);
        match std::fs::write(&self.setlist_path, &rpp_text) {
            Ok(()) => info!(
                "Setlist written: {} ({} bytes)",
                self.setlist_path.display(),
                rpp_text.len()
            ),
            Err(e) => {
                warn!(
                    "Failed to write setlist to {}: {}",
                    self.setlist_path.display(),
                    e
                );
                return;
            }
        }

        // Regenerate shell copies for each role
        let roles = &["Vocals", "Guitar", "Keys", "Drum Enhancement"];
        let output_dir = self
            .setlist_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let setlist_stem = self
            .setlist_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Setlist");

        for role in roles {
            let shell = generate_shell_copy(&self.setlist_project, role);
            let shell_text = project_to_rpp_text(&shell);
            let shell_path = output_dir.join(format!("{role} - {setlist_stem}.RPP"));
            match std::fs::write(&shell_path, &shell_text) {
                Ok(()) => debug!(
                    "Shell copy written: {} ({} bytes)",
                    shell_path.display(),
                    shell_text.len()
                ),
                Err(e) => warn!("Failed to write shell copy {}: {}", shell_path.display(), e),
            }
        }
    }
}

/// Start live sync for a setlist.
///
/// Creates a file watcher for all song RPPs and spawns a task that
/// processes change events.
#[must_use]
pub fn start_live_sync(
    state: LiveSyncState,
) -> (
    architect::platform::JoinHandle<()>,
    tokio::sync::mpsc::Receiver<SongFileChanged>,
) {
    // Create the file watcher
    let songs: Vec<_> = state
        .song_paths
        .iter()
        .enumerate()
        .map(|(i, p)| (p.clone(), i, format!("Song {}", i.saturating_add(1))))
        .collect();

    let (watcher, rx) = SongFileWatcher::new(songs);

    // Start the polling loop
    let _watcher_handle = watcher.start();

    // Create a channel for forwarding events (so caller can also react)
    let (fwd_tx, fwd_rx) = tokio::sync::mpsc::channel(64);

    // Spawn the sync processing task
    let handle = architect::platform::spawn(async move {
        let mut state = state;
        let mut rx = rx;

        while let Some(event) = rx.recv().await {
            let updated = state.handle_song_changed(&event);
            if updated {
                // Forward the event so the caller knows a sync happened
                let _ = fwd_tx.send(event).await;
            }
        }

        debug!("Live sync task exiting (channel closed)");
    });

    (handle, fwd_rx)
}
