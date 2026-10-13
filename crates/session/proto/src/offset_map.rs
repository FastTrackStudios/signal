//! Setlist offset map — bidirectional position mapping between song-local and
//! setlist-global coordinate spaces.
//!
//! All types and methods here are pure data with no DAW dependency.

use crate::SongId;
use crate::setlist::Setlist;
use daw_proto::TimeSignature;
use facet::Facet;

/// Offset entry for a single song within the setlist timeline.
#[derive(Clone, Debug, PartialEq, Facet)]
pub struct SongOffset {
    /// Position of this song in the setlist (0-based).
    pub index: usize,
    /// Stable song identifier.
    pub song_id: SongId,
    /// DAW project GUID — links to `Song.project_guid`.
    pub project_guid: String,
    /// Cumulative start time in the setlist timeline (seconds).
    pub global_start_seconds: f64,
    /// Cumulative start position in quarter-notes.
    pub global_start_qn: f64,
    /// Song duration in seconds (including count-in).
    pub duration_seconds: f64,
    /// Song duration in quarter-notes.
    pub duration_qn: f64,
    /// Count-in duration that precedes the song's content.
    pub count_in_seconds: f64,
    /// Tempo at song start (BPM). Falls back to 120 if unknown.
    pub start_tempo: f64,
    /// Time signature at song start. Falls back to 4/4 if unknown.
    pub start_time_sig: TimeSignature,
}

impl SongOffset {
    /// Global end time of this song in the setlist timeline.
    #[must_use]
    pub fn global_end_seconds(&self) -> f64 {
        self.global_start_seconds + self.duration_seconds
    }

    /// Global end position of this song in quarter-notes.
    #[must_use]
    pub fn global_end_qn(&self) -> f64 {
        self.global_start_qn + self.duration_qn
    }
}

/// Maps every song in a setlist to a global timeline position, enabling
/// bidirectional conversion between song-local and setlist-global coordinates.
#[derive(Clone, Debug, PartialEq, Facet)]
pub struct SetlistOffsetMap {
    /// Per-song offset entries, ordered by setlist position.
    pub songs: Vec<SongOffset>,
    /// Total setlist duration in seconds.
    pub total_seconds: f64,
    /// Total setlist duration in quarter-notes.
    pub total_qn: f64,
}

impl SetlistOffsetMap {
    /// Build an offset map from an existing [`Setlist`].
    ///
    /// Each song's allocated time = `duration_with_count_in()`. Quarter-note
    /// durations are estimated from tempo (seconds × BPM / 60).
    #[must_use]
    pub fn from_setlist(setlist: &Setlist) -> Self {
        let mut songs = Vec::with_capacity(setlist.songs.len());
        let mut cumulative_seconds = 0.0;
        let mut cumulative_qn = 0.0;

        for (index, song) in setlist.songs.iter().enumerate() {
            let tempo = song.tempo.unwrap_or(120.0);
            let time_sig = song
                .time_signature
                .unwrap_or_else(|| TimeSignature::new(4, 4));
            let count_in = song.count_in_seconds.unwrap_or(0.0);
            let duration_seconds = song.duration() + count_in;
            let duration_qn = seconds_to_qn(duration_seconds, tempo);

            songs.push(SongOffset {
                index,
                song_id: song.id.clone(),
                project_guid: song.project_guid.clone(),
                global_start_seconds: cumulative_seconds,
                global_start_qn: cumulative_qn,
                duration_seconds,
                duration_qn,
                count_in_seconds: count_in,
                start_tempo: tempo,
                start_time_sig: time_sig,
            });

            cumulative_seconds += duration_seconds;
            cumulative_qn += duration_qn;
        }

        Self {
            songs,
            total_seconds: cumulative_seconds,
            total_qn: cumulative_qn,
        }
    }

    /// Convert a song-local position (seconds) to a setlist-global position.
    ///
    /// `local_seconds` is relative to the song's start (0 = beginning of song,
    /// including count-in time).
    #[must_use]
    pub fn project_to_setlist(&self, song_index: usize, local_seconds: f64) -> Option<f64> {
        let offset = self.songs.get(song_index)?;
        Some(offset.global_start_seconds + local_seconds)
    }

    /// Convert a setlist-global position (seconds) to a song-local position.
    ///
    /// Returns `(song_index, local_seconds)` where `local_seconds` is relative
    /// to that song's start. Uses binary search for O(log n) lookup.
    #[must_use]
    pub fn setlist_to_project(&self, global_seconds: f64) -> Option<(usize, f64)> {
        if self.songs.is_empty() || global_seconds < 0.0 {
            return None;
        }

        // Binary search: find the last song whose global_start_seconds <= global_seconds
        let idx = match self.songs.binary_search_by(|s| {
            s.global_start_seconds
                .partial_cmp(&global_seconds)
                .unwrap_or(std::cmp::Ordering::Equal)
        }) {
            Ok(i) => i,            // exact match on a boundary
            Err(0) => return None, // before the first song
            Err(i) => i.checked_sub(1)?,
        };

        let offset = self.songs.get(idx)?;
        let local = global_seconds - offset.global_start_seconds;

        // Clamp: if past this song's end, still return it (last song case)
        if local > offset.duration_seconds {
            // Actually in the next song — shouldn't happen with correct binary search,
            // but guard against floating-point edge cases
            if let Some(next) = self.songs.get(idx.checked_add(1)?) {
                return Some((next.index, global_seconds - next.global_start_seconds));
            }
        }

        Some((offset.index, local))
    }

    /// Convert a song-local position (quarter-notes) to a setlist-global QN position.
    #[must_use]
    pub fn project_to_setlist_qn(&self, song_index: usize, local_qn: f64) -> Option<f64> {
        let offset = self.songs.get(song_index)?;
        Some(offset.global_start_qn + local_qn)
    }

    /// Convert a setlist-global QN position to a song-local QN position.
    ///
    /// Returns `(song_index, local_qn)`. Uses binary search.
    #[must_use]
    pub fn setlist_to_project_qn(&self, global_qn: f64) -> Option<(usize, f64)> {
        if self.songs.is_empty() || global_qn < 0.0 {
            return None;
        }

        let idx = match self.songs.binary_search_by(|s| {
            s.global_start_qn
                .partial_cmp(&global_qn)
                .unwrap_or(std::cmp::Ordering::Equal)
        }) {
            Ok(i) => i,
            Err(0) => return None,
            Err(i) => i.checked_sub(1)?,
        };

        let offset = self.songs.get(idx)?;
        let local = global_qn - offset.global_start_qn;

        if local > offset.duration_qn {
            if let Some(next) = self.songs.get(idx.checked_add(1)?) {
                return Some((next.index, global_qn - next.global_start_qn));
            }
        }

        Some((offset.index, local))
    }

    /// Look up a song by its project GUID.
    #[must_use]
    pub fn song_by_guid(&self, project_guid: &str) -> Option<&SongOffset> {
        self.songs.iter().find(|s| s.project_guid == project_guid)
    }
}

/// Estimate quarter-note count from seconds and tempo.
fn seconds_to_qn(seconds: f64, bpm: f64) -> f64 {
    seconds * bpm / 60.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setlist::Setlist;
    use crate::song::Song;

    fn make_song(name: &str, start: f64, end: f64, tempo: f64, count_in: Option<f64>) -> Song {
        Song {
            id: SongId::new(),
            name: name.to_string(),
            project_guid: format!("{{{name}}}"),
            start_seconds: start,
            end_seconds: end,
            count_in_seconds: count_in,
            sections: vec![],
            comments: vec![],
            tempo: Some(tempo),
            time_signature: Some(TimeSignature::new(4, 4)),
            measure_positions: vec![],
            chart_text: None,
            parsed_chart: None,
            detected_chords: vec![],
            chart_fingerprint: None,
            advance_mode: None,
            color: None,
        }
    }

    fn make_setlist(songs: Vec<Song>) -> Setlist {
        Setlist {
            id: None,
            name: "Test Setlist".to_string(),
            advance_mode: crate::setlist::AdvanceMode::Wait,
            songs,
        }
    }

    #[test]
    fn single_song_offset() {
        let setlist = make_setlist(vec![make_song("Song A", 0.0, 60.0, 120.0, None)]);
        let map = SetlistOffsetMap::from_setlist(&setlist);

        assert_eq!(map.songs.len(), 1);
        assert!((map.songs[0].global_start_seconds - 0.0).abs() < 1e-9);
        assert!((map.songs[0].duration_seconds - 60.0).abs() < 1e-9);
        assert!((map.total_seconds - 60.0).abs() < 1e-9);
        // 60s at 120 BPM = 120 QN
        assert!((map.total_qn - 120.0).abs() < 1e-9);
    }

    #[test]
    fn multi_song_cumulative_offsets() {
        let setlist = make_setlist(vec![
            make_song("Song A", 0.0, 30.0, 120.0, None),
            make_song("Song B", 0.0, 45.0, 90.0, None),
            make_song("Song C", 0.0, 60.0, 140.0, Some(5.0)),
        ]);
        let map = SetlistOffsetMap::from_setlist(&setlist);

        assert_eq!(map.songs.len(), 3);

        // Song A: starts at 0, duration 30s
        assert!((map.songs[0].global_start_seconds - 0.0).abs() < 1e-9);
        assert!((map.songs[0].duration_seconds - 30.0).abs() < 1e-9);

        // Song B: starts at 30s, duration 45s
        assert!((map.songs[1].global_start_seconds - 30.0).abs() < 1e-9);
        assert!((map.songs[1].duration_seconds - 45.0).abs() < 1e-9);

        // Song C: starts at 75s, duration 65s (60 + 5 count-in)
        assert!((map.songs[2].global_start_seconds - 75.0).abs() < 1e-9);
        assert!((map.songs[2].duration_seconds - 65.0).abs() < 1e-9);
        assert!((map.songs[2].count_in_seconds - 5.0).abs() < 1e-9);

        assert!((map.total_seconds - 140.0).abs() < 1e-9);
    }

    #[test]
    fn project_to_setlist_basic() {
        let setlist = make_setlist(vec![
            make_song("A", 0.0, 30.0, 120.0, None),
            make_song("B", 0.0, 45.0, 120.0, None),
        ]);
        let map = SetlistOffsetMap::from_setlist(&setlist);

        // 10s into song A → global 10s
        assert_eq!(map.project_to_setlist(0, 10.0), Some(10.0));
        // 10s into song B → global 40s (30 + 10)
        assert_eq!(map.project_to_setlist(1, 10.0), Some(40.0));
        // Invalid song index
        assert_eq!(map.project_to_setlist(2, 10.0), None);
    }

    #[test]
    fn setlist_to_project_basic() {
        let setlist = make_setlist(vec![
            make_song("A", 0.0, 30.0, 120.0, None),
            make_song("B", 0.0, 45.0, 120.0, None),
        ]);
        let map = SetlistOffsetMap::from_setlist(&setlist);

        // Global 10s → song A at 10s
        assert_eq!(map.setlist_to_project(10.0), Some((0, 10.0)));
        // Global 30s → song B at 0s (exactly on boundary → song B)
        assert_eq!(map.setlist_to_project(30.0), Some((1, 0.0)));
        // Global 50s → song B at 20s
        assert_eq!(map.setlist_to_project(50.0), Some((1, 20.0)));
        // Negative → None
        assert_eq!(map.setlist_to_project(-1.0), None);
    }

    #[test]
    fn roundtrip_identity() {
        let setlist = make_setlist(vec![
            make_song("A", 0.0, 30.0, 120.0, None),
            make_song("B", 0.0, 45.0, 90.0, Some(2.0)),
            make_song("C", 0.0, 60.0, 140.0, None),
        ]);
        let map = SetlistOffsetMap::from_setlist(&setlist);

        for song_idx in 0..3 {
            for &local in &[0.0, 5.0, 10.0, 15.0] {
                let global = map.project_to_setlist(song_idx, local).unwrap();
                let (back_idx, back_local) = map.setlist_to_project(global).unwrap();
                assert_eq!(back_idx, song_idx, "song index roundtrip failed");
                assert!(
                    (back_local - local).abs() < 1e-9,
                    "local position roundtrip failed: {back_local} != {local}"
                );
            }
        }
    }

    #[test]
    fn qn_conversion() {
        let setlist = make_setlist(vec![
            make_song("A", 0.0, 30.0, 120.0, None), // 60 QN
            make_song("B", 0.0, 30.0, 60.0, None),  // 30 QN
        ]);
        let map = SetlistOffsetMap::from_setlist(&setlist);

        assert!((map.songs[0].global_start_qn - 0.0).abs() < 1e-9);
        assert!((map.songs[0].duration_qn - 60.0).abs() < 1e-9);
        assert!((map.songs[1].global_start_qn - 60.0).abs() < 1e-9);
        assert!((map.songs[1].duration_qn - 30.0).abs() < 1e-9);

        // 30 QN into song B → global 90 QN
        assert_eq!(map.project_to_setlist_qn(1, 30.0), Some(90.0));

        // Global 70 QN → song B at 10 QN
        let (idx, local_qn) = map.setlist_to_project_qn(70.0).unwrap();
        assert_eq!(idx, 1);
        assert!((local_qn - 10.0).abs() < 1e-9);
    }

    #[test]
    fn song_by_guid_lookup() {
        let setlist = make_setlist(vec![
            make_song("A", 0.0, 30.0, 120.0, None),
            make_song("B", 0.0, 45.0, 90.0, None),
        ]);
        let map = SetlistOffsetMap::from_setlist(&setlist);

        assert!(map.song_by_guid("{A}").is_some());
        assert_eq!(map.song_by_guid("{A}").unwrap().index, 0);
        assert!(map.song_by_guid("{B}").is_some());
        assert_eq!(map.song_by_guid("{B}").unwrap().index, 1);
        assert!(map.song_by_guid("{C}").is_none());
    }

    #[test]
    fn empty_setlist() {
        let setlist = make_setlist(vec![]);
        let map = SetlistOffsetMap::from_setlist(&setlist);

        assert!(map.songs.is_empty());
        assert!((map.total_seconds - 0.0).abs() < 1e-9);
        assert!((map.total_qn - 0.0).abs() < 1e-9);
        assert_eq!(map.setlist_to_project(0.0), None);
        assert_eq!(map.project_to_setlist(0, 0.0), None);
    }

    #[test]
    fn position_after_last_song() {
        let setlist = make_setlist(vec![make_song("A", 0.0, 30.0, 120.0, None)]);
        let map = SetlistOffsetMap::from_setlist(&setlist);

        // Past the end of the only song — still returns song A with local > duration
        let result = map.setlist_to_project(50.0);
        assert!(result.is_some());
        let (idx, local) = result.unwrap();
        assert_eq!(idx, 0);
        assert!((local - 50.0).abs() < 1e-9);
    }
}
