//! Event Bridge
//!
//! Maps `SetlistEvent` variants to the global Dioxus signals that drive the UI.
//! Used by both the desktop app (from the subscription loop) and the web app
//! (from the `WebClientHandler`).

use crate::prelude::*;
use session_proto::{ActiveIndices, SetlistEvent, SongTransportState};

use crate::signals::{
    TransportState, ACTIVE_INDICES, ACTIVE_PLAYBACK_IS_PLAYING, ACTIVE_PLAYBACK_MUSICAL,
    PLAYBACK_STATE, SETLIST_STRUCTURE, SONG_CHARTS, SONG_TRANSPORT, TAKE_PEAKS, TAKE_UNDER_REVIEW,
};

/// Apply an active-song/section cursor update to the global UI signals.
///
/// Fed from the service's dedicated `active_indices` `#[subscribe]` stream
/// (the architect `PubSub<ActiveIndices>` hub) — the single source of truth
/// for the cursor. `apply_setlist_event` handles setlist *structure* and
/// per-song transport; this owns which song/section is current + playback
/// state. Both desktop and web consumers call this.
pub fn apply_active_indices(indices: &ActiveIndices) {
    *PLAYBACK_STATE.write() = if indices.is_playing {
        daw_proto::PlayState::Playing
    } else {
        daw_proto::PlayState::Stopped
    };
    *ACTIVE_INDICES.write() = indices.clone();
}

/// Apply a single `SetlistEvent` to the global UI signals.
///
/// This is the canonical mapping — both desktop and web apps should call this
/// instead of duplicating the signal-update logic. Cursor changes arrive on a
/// separate stream; see [`apply_active_indices`].
pub fn apply_setlist_event(event: &SetlistEvent) {
    match event {
        SetlistEvent::SetlistChanged(setlist) => {
            let valid_guids: std::collections::HashSet<String> = setlist
                .songs
                .iter()
                .map(|song| song.project_guid.clone())
                .collect();
            SONG_CHARTS
                .write()
                .retain(|guid, _| valid_guids.contains(guid));
            *SETLIST_STRUCTURE.write() = setlist.clone();
        }

        SetlistEvent::SongHydrated { index, song, .. } => {
            let mut setlist = SETLIST_STRUCTURE.write();
            if let Some(song_mut) = setlist.songs.get_mut(*index) {
                *song_mut = song.clone();
            }
        }

        SetlistEvent::SongChartHydrated { chart, .. } => {
            SONG_CHARTS
                .write()
                .insert(chart.project_guid.clone(), chart.clone());
            // Also seed the song's *stable* `chart_text` in SETLIST_STRUCTURE.
            // Charts are stripped from the Setlist payload and only ride these
            // hydration deltas, so without this the keyflow SOURCE editor — which
            // seeds from SETLIST_STRUCTURE (NOT the live SONG_CHARTS it writes
            // into, to avoid a re-seed loop) — has no text. The editor never
            // writes SETLIST_STRUCTURE, so this stays the original seed.
            let mut sl = SETLIST_STRUCTURE.write();
            for song in sl
                .songs
                .iter_mut()
                .filter(|s| s.project_guid == chart.project_guid)
            {
                song.chart_text = Some(chart.chart_text.clone());
            }
        }

        SetlistEvent::TransportUpdate(transports) => {
            apply_transport_update(transports);
        }

        // A take just ended. Every tablet in the room shows it, which
        // is the whole point — the thirty seconds after a pass is when
        // everybody still knows what they think of it.
        SetlistEvent::TakeRecorded { pass, .. } => {
            *TAKE_UNDER_REVIEW.write() = Some(pass.clone());
            // The peaks are read separately and arrive later; until
            // then the panel draws a line rather than the last take's
            // shape under this take's number.
            TAKE_PEAKS.write().clear();
        }

        // Somebody rated it. Applied even when it is this device's own
        // mark coming back: the service is the authority on what the
        // room thinks, and a local guess that disagreed would be a
        // rating that changed under somebody's hand.
        SetlistEvent::TakeMarked { pass, mark, .. } => {
            if let Some(under_review) = TAKE_UNDER_REVIEW.write().as_mut() {
                if under_review.number == *pass {
                    under_review.mark(mark.clone());
                }
            }
        }

        SetlistEvent::SongEntered { .. }
        | SetlistEvent::SongExited { .. }
        | SetlistEvent::SectionEntered { .. }
        | SetlistEvent::SectionExited { .. }
        | SetlistEvent::PositionChanged { .. } => {}
    }
}

fn apply_transport_update(transports: &[SongTransportState]) {
    let active_song_index = ACTIVE_INDICES.read().song_index;

    let mut transport_updates: Vec<(usize, TransportState)> = Vec::with_capacity(transports.len());
    let mut active_transport_update = None;

    {
        let setlist = SETLIST_STRUCTURE.read();
        let existing = SONG_TRANSPORT.read();

        for transport in transports {
            let loop_region_pct = transport.loop_region.as_ref().and_then(|region| {
                setlist.songs.get(transport.song_index).map(|song| {
                    let dur = song.duration();
                    if dur > 0.0 {
                        (
                            (region.start_seconds / dur).clamp(0.0, 1.0),
                            (region.end_seconds / dur).clamp(0.0, 1.0),
                        )
                    } else {
                        (0.0, 1.0)
                    }
                })
            });

            let next_state = TransportState {
                position: transport.position.clone(),
                bpm: transport.bpm,
                time_sig_num: i32::try_from(transport.time_sig_num).unwrap_or(4),
                time_sig_denom: i32::try_from(transport.time_sig_denom).unwrap_or(4),
                is_playing: transport.is_playing,
                is_looping: transport.is_looping,
                loop_region: loop_region_pct,
            };

            let changed = existing
                .get(&transport.song_index)
                .is_none_or(|e| *e != next_state);

            if changed {
                transport_updates.push((transport.song_index, next_state));
            }

            if Some(transport.song_index) == active_song_index {
                active_transport_update = Some((transport.is_playing, transport.position.musical));
            }
        }
    }

    if !transport_updates.is_empty() {
        let mut song_transport = SONG_TRANSPORT.write();
        for (idx, state) in transport_updates {
            song_transport.insert(idx, state);
        }
    }

    // Transport only feeds the per-song `SONG_TRANSPORT` map and the active
    // song's musical-position readout. The cursor itself (`ACTIVE_INDICES`,
    // `PLAYBACK_STATE`) is owned exclusively by `apply_active_indices`, fed
    // from the dedicated active-indices stream — no cursor writes here.
    if let Some((is_playing, musical)) = active_transport_update {
        if *ACTIVE_PLAYBACK_MUSICAL.peek() != musical {
            *ACTIVE_PLAYBACK_MUSICAL.write() = musical;
        }
        if *ACTIVE_PLAYBACK_IS_PLAYING.peek() != is_playing {
            *ACTIVE_PLAYBACK_IS_PLAYING.write() = is_playing;
        }
    }
}
