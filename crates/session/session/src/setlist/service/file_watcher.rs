//! Polling-based file watcher for individual song RPP files.
//!
//! Periodically checks modification times of tracked song files and emits
//! [`SongFileChanged`] events when a file has been updated on disk.

#![allow(dead_code)]

use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use tokio::sync::mpsc;
use tracing::{debug, trace};

/// How often the watcher polls file modification times.
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Emitted when a watched song RPP file's modification time changes.
#[derive(Debug, Clone)]
pub struct SongFileChanged {
    pub path: PathBuf,
    pub song_index: usize,
    pub song_name: String,
}

/// A single tracked song file.
struct WatchedSong {
    path: PathBuf,
    song_index: usize,
    song_name: String,
    last_modified: Option<SystemTime>,
}

impl WatchedSong {
    /// Read the file's current mtime. Returns `None` if the file doesn't exist
    /// or metadata can't be read.
    fn current_mtime(&self) -> Option<SystemTime> {
        std::fs::metadata(&self.path)
            .ok()
            .and_then(|m| m.modified().ok())
    }

    /// Check whether the file has been modified since the last poll.
    /// Updates `last_modified` and returns `true` if the mtime changed.
    fn check_changed(&mut self) -> bool {
        let current = self.current_mtime();
        match (self.last_modified, current) {
            // Both present and different → changed
            (Some(prev), Some(now)) if now != prev => {
                self.last_modified = Some(now);
                true
            }
            // File appeared (was missing, now exists) → changed
            (None, Some(now)) => {
                self.last_modified = Some(now);
                true
            }
            // No change, or file still missing
            _ => {
                self.last_modified = current;
                false
            }
        }
    }
}

/// Polls a set of song RPP files for modification-time changes and sends
/// [`SongFileChanged`] events over a channel.
pub struct SongFileWatcher {
    songs: Vec<WatchedSong>,
    tx: mpsc::Sender<SongFileChanged>,
}

impl SongFileWatcher {
    /// Create a new watcher for the given song files.
    ///
    /// Each tuple is `(path, song_index, song_name)`.
    ///
    /// Returns the watcher and a receiver for change events.
    pub fn new(songs: Vec<(PathBuf, usize, String)>) -> (Self, mpsc::Receiver<SongFileChanged>) {
        let (tx, rx) = mpsc::channel(64);

        let watched: Vec<WatchedSong> = songs
            .into_iter()
            .map(|(path, song_index, song_name)| {
                let last_modified = std::fs::metadata(&path)
                    .ok()
                    .and_then(|m| m.modified().ok());
                WatchedSong {
                    path,
                    song_index,
                    song_name,
                    last_modified,
                }
            })
            .collect();

        debug!("SongFileWatcher created for {} songs", watched.len());

        (Self { songs: watched, tx }, rx)
    }

    /// Spawn the polling loop. Returns a join handle that runs until all
    /// receivers are dropped (channel closes) or the task is aborted.
    #[must_use]
    pub fn start(mut self) -> architect::platform::JoinHandle<()> {
        architect::platform::spawn(async move {
            loop {
                tokio::time::sleep(POLL_INTERVAL).await;

                for song in &mut self.songs {
                    if song.check_changed() {
                        debug!(
                            path = %song.path.display(),
                            index = song.song_index,
                            name = %song.song_name,
                            "Song file changed on disk"
                        );
                        let event = SongFileChanged {
                            path: song.path.clone(),
                            song_index: song.song_index,
                            song_name: song.song_name.clone(),
                        };
                        if self.tx.send(event).await.is_err() {
                            debug!("SongFileWatcher receiver dropped, shutting down");
                            return;
                        }
                    }
                }

                trace!("SongFileWatcher poll tick complete");
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watched_song_detects_mtime_change() {
        let dir = std::env::temp_dir().join("fts-watcher-test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_song.rpp");
        std::fs::write(&path, "initial").unwrap();

        let initial_mtime = std::fs::metadata(&path).unwrap().modified().unwrap();

        let mut ws = WatchedSong {
            path: path.clone(),
            song_index: 0,
            song_name: "Test Song".into(),
            last_modified: Some(initial_mtime),
        };

        // No change yet
        assert!(!ws.check_changed());

        // Touch the file — sleep briefly to ensure mtime advances
        std::thread::sleep(Duration::from_millis(50));
        std::fs::write(&path, "modified").unwrap();

        // mtime should differ now
        let new_mtime = std::fs::metadata(&path).unwrap().modified().unwrap();
        if new_mtime != initial_mtime {
            assert!(ws.check_changed());
        }

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn watched_song_missing_file_no_panic() {
        let mut ws = WatchedSong {
            path: PathBuf::from("/nonexistent/file.rpp"),
            song_index: 0,
            song_name: "Missing".into(),
            last_modified: None,
        };
        assert!(!ws.check_changed());
    }
}
