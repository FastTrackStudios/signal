//! `engine_watch_session.rs` — the session half of the watch bridge:
//! setlist transport + mixer + the chord window over `/watch/v1/session`,
//! same HTTP+SSE shape as the rig half (`engine_watch.rs`).
//!
//! ```text
//! GET  /watch/v1/session/state                     → WatchSessionState (JSON)
//! GET  /watch/v1/session/events                    → SSE WatchSessionState
//! POST /watch/v1/session/transport/{cmd}           → play/pause/stop/toggle/
//!                                                    next-song/prev-song/
//!                                                    next-section/prev-section
//! POST /watch/v1/session/section/{song}/{section}  → seek to a section
//! POST /watch/v1/session/track/{guid}/{op}         → toggle-mute / toggle-solo
//! POST /watch/v1/session/track/{guid}/volume/{v}   → fader 0..1
//! ```
//!
//! Transport/setlist ride `SetlistServiceClient` over the same in-process
//! `LocalServer` as the rig bridge. The mixer is NOT on that router — track
//! state lives on the in-process `daw::get()` facade (exactly how
//! `mixer_view.rs` drives it), so track reads/commands call it directly.
//! Chords come from each song's keyflow chart (`chart_text`), parsed once
//! and flattened to `(measure, beat, symbol)`; the playhead (the
//! `active_indices` stream's `song_progress`) maps to a measure/beat via
//! the `measures()` tempo table, yielding the current chord + the next 3.

use std::collections::HashMap;
use std::sync::Arc;

use architect::LocalServer;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{KeepAlive, Sse};
use axum::routing::{get, post};
use axum::Router;
use session_proto::services::setlist_service::SetlistServiceClient;
use session_proto::setlist::ActiveIndices;
use session_proto::watch::{WatchChord, WatchSessionState, WatchTrack};
use tokio::sync::RwLock;

/// One flattened chart chord: absolute position + display symbol.
#[derive(Clone, Debug)]
struct FlatChord {
    measure: i32,
    beat: i32,
    subdivision: i32,
    symbol: String,
    /// Chart section the chord belongs to (for the lyric line).
    section: usize,
}

/// Per-song chart data, parsed once.
#[derive(Clone, Debug, Default)]
struct SongChart {
    chords: Vec<FlatChord>,
    /// Lyric line per chart section ("" when none).
    lyrics: Vec<String>,
    /// Measure → song-relative start seconds (the `measures()` table).
    measure_times: Vec<(i32, f64)>,
}

#[derive(Clone)]
struct SessionBridge {
    setlist: SetlistServiceClient,
    local: LocalServer,
    charts: Arc<RwLock<HashMap<usize, Arc<SongChart>>>>,
    revision: Arc<std::sync::atomic::AtomicU64>,
}

/// Build the session watch routes. `None` (disabled) if the setlist client
/// can't be established — the rig bridge still serves.
pub async fn router(local: LocalServer) -> Option<Router> {
    let setlist: SetlistServiceClient = match local.establish().await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "watch session bridge disabled: setlist client failed");
            return None;
        }
    };
    let bridge = SessionBridge {
        setlist,
        local,
        charts: Arc::new(RwLock::new(HashMap::new())),
        revision: Arc::new(std::sync::atomic::AtomicU64::new(0)),
    };
    Some(
        Router::new()
            .route("/watch/v1/session/state", get(state_snapshot))
            .route("/watch/v1/session/events", get(events))
            .route("/watch/v1/session/transport/{cmd}", post(transport))
            .route("/watch/v1/session/section/{song}/{section}", post(seek_section))
            .route("/watch/v1/session/track/{guid}/{op}", post(track_op))
            .route("/watch/v1/session/track/{guid}/volume/{v}", post(track_volume))
            .with_state(bridge),
    )
}

// ── Chart flattening ──────────────────────────────────────────────────────

/// Parse + flatten a song's chart and fetch its measure/time table.
async fn build_song_chart(b: &SessionBridge, song_index: usize) -> Arc<SongChart> {
    let mut out = SongChart::default();

    if let Ok(song) = b.setlist.song(song_index).await {
        // `song()` may predate chart hydration; the `song_chart` RPC is the
        // authoritative backfill (the late-remote path).
        let chart_text = match &song.chart_text {
            Some(t) => Some(t.clone()),
            None => b
                .setlist
                .song_chart(song_index)
                .await
                .ok()
                .flatten()
                .map(|h| h.chart_text),
        };
        let chart = match song.parsed_chart {
            Some(c) => Some(c),
            None => chart_text.as_deref().and_then(|t| match keyflow::parse(t) {
                Ok(c) => Some(c),
                Err(e) => {
                    tracing::warn!(song_index, error = ?e, "watch: chart parse failed");
                    None
                }
            }),
        };
        if let Some(chart) = chart {
            for (si, section) in chart.sections.iter().enumerate() {
                if let Some(track) = section.chord_track() {
                    for measure in &track.measures {
                        // `chords` is the legacy field; newer parses fill
                        // `rhythm_elements` only — read both.
                        let from_elements = measure.rhythm_elements.iter().filter_map(|e| {
                            match e {
                                keyflow::chart::types::RhythmElement::Chord(c) => Some(c),
                                _ => None,
                            }
                        });
                        let legacy = measure.chords.iter();
                        let chords: Vec<_> = if measure.rhythm_elements.is_empty() {
                            legacy.collect()
                        } else {
                            from_elements.collect()
                        };
                        for chord in chords {
                            if chord.rhythm.is_rest() {
                                continue;
                            }
                            let pos = &chord.position.total_duration;
                            out.chords.push(FlatChord {
                                measure: pos.measure,
                                beat: pos.beat,
                                subdivision: pos.subdivision,
                                symbol: chord
                                    .display_override
                                    .clone()
                                    .unwrap_or_else(|| chord.full_symbol.clone()),
                                section: si,
                            });
                        }
                    }
                }
                let lyric = section
                    .lyrics_track()
                    .and_then(|t| t.lyrics.as_ref())
                    .map(|line| {
                        let mut s = String::new();
                        for syl in &line.syllables {
                            s.push_str(&syl.text);
                            if !syl.hyphen_after {
                                s.push(' ');
                            }
                        }
                        s.trim_end().to_string()
                    })
                    .unwrap_or_default();
                out.lyrics.push(lyric);
            }
            out.chords
                .sort_by_key(|c| (c.measure, c.beat, c.subdivision));
        }
    }

    if let Ok(measures) = b.setlist.measures(song_index).await {
        out.measure_times = measures.iter().map(|m| (m.measure, m.time_seconds)).collect();
        out.measure_times.sort_by(|a, b| a.0.cmp(&b.0));
    }

    // Chart carried no chord tokens (section-outline charts) — fall back to
    // the song's MIDI-detected chords, mapping PPQ → seconds → measure/beat
    // through the measure table (960 PPQ per quarter, song tempo).
    if out.chords.is_empty() && !out.measure_times.is_empty()
        && let Ok(song) = b.setlist.song(song_index).await {
            let bpm = song.tempo.unwrap_or(120.0).max(1.0);
            for dc in &song.detected_chords {
                let sec = dc.start_ppq as f64 / 960.0 * 60.0 / bpm;
                if let Some((measure, frac)) = measure_at(&out, sec) {
                    out.chords.push(FlatChord {
                        measure,
                        beat: (frac * 4.0) as i32,
                        subdivision: 0,
                        symbol: dc.symbol.clone(),
                        section: 0,
                    });
                }
            }
            out.chords
                .sort_by_key(|c| (c.measure, c.beat, c.subdivision));
        }

    tracing::info!(
        song_index,
        chords = out.chords.len(),
        measures = out.measure_times.len(),
        first_chord = ?out.chords.first(),
        first_measure = ?out.measure_times.first(),
        lyric_sections = out.lyrics.iter().filter(|l| !l.is_empty()).count(),
        "watch: song chart built"
    );
    Arc::new(out)
}

async fn song_chart(b: &SessionBridge, song_index: usize) -> Arc<SongChart> {
    if let Some(c) = b.charts.read().await.get(&song_index) {
        return c.clone();
    }
    let built = build_song_chart(b, song_index).await;
    b.charts.write().await.insert(song_index, built.clone());
    built
}

/// Map song-relative seconds to a fractional (measure, beat-fraction) via
/// the measure/time table; beats are interpolated linearly inside the
/// measure (good enough for a 4-chord window).
fn measure_at(chart: &SongChart, t: f64) -> Option<(i32, f64)> {
    let times = &chart.measure_times;
    if times.is_empty() {
        return None;
    }
    let i = match times.binary_search_by(|(_, start)| {
        start.partial_cmp(&t).unwrap_or(std::cmp::Ordering::Equal)
    }) {
        Ok(i) => i,
        Err(0) => 0,
        Err(i) => i - 1,
    };
    let (measure, start) = times[i];
    let end = times.get(i + 1).map(|(_, s)| *s).unwrap_or(start + 2.0);
    let frac = if end > start { ((t - start) / (end - start)).clamp(0.0, 1.0) } else { 0.0 };
    Some((measure, frac))
}

/// The chord window at song-relative seconds `t`: the chord under the
/// playhead + the next 3, and the current section's lyric line.
fn chord_window(chart: &SongChart, t: f64) -> (Vec<WatchChord>, String) {
    let Some((measure, frac)) = measure_at(chart, t) else {
        return (Vec::new(), String::new());
    };
    if chart.chords.is_empty() {
        return (Vec::new(), String::new());
    }
    // Beats per measure aren't tracked here; compare on (measure, beat
    // fraction) with the chord's beat scaled by an assumed 4 — the chords
    // are sorted, so any monotone key works as long as it's consistent.
    // Use partition_point on (measure, beat) against the playhead measure
    // + fractional beat in that measure's actual beat count, approximated
    // from the max beat seen in the measure.
    let beats_in_measure = chart
        .chords
        .iter()
        .filter(|c| c.measure == measure)
        .map(|c| c.beat + 1)
        .max()
        .unwrap_or(4)
        .max(4) as f64;
    let playhead_beat = frac * beats_in_measure;
    let idx = chart
        .chords
        .partition_point(|c| {
            c.measure < measure || (c.measure == measure && (c.beat as f64) <= playhead_beat)
        })
        .saturating_sub(1);
    // Chart-line semantics: the window is the current LINE of four chords
    // (advancing a line at a time), with the playhead chord highlighted —
    // not a rolling next-3 window.
    let line_start = (idx / 4) * 4;
    let window: Vec<WatchChord> = chart.chords[line_start..]
        .iter()
        .take(4)
        .enumerate()
        .map(|(k, c)| WatchChord {
            symbol: c.symbol.clone(),
            measure: c.measure,
            beat: c.beat,
            is_current: line_start + k == idx,
        })
        .collect();
    let lyric = chart
        .chords
        .get(idx)
        .and_then(|c| chart.lyrics.get(c.section))
        .cloned()
        .unwrap_or_default();
    (window, lyric)
}

// ── State building ────────────────────────────────────────────────────────

/// The mixer via the in-process daw facade (the router the bridge sees has
/// no track service — see module docs).
async fn mixer_tracks() -> Vec<WatchTrack> {
    let Some(daw) = daw::get() else { return Vec::new() };
    let Ok(project) = daw.current_project().await else { return Vec::new() };
    let Ok(tracks) = project.tracks().all().await else { return Vec::new() };
    tracks
        .iter()
        .map(|t| WatchTrack {
            guid: t.guid.clone(),
            name: t.name.clone(),
            index: t.index,
            muted: t.muted,
            soloed: t.soloed,
            volume: t.volume as f32,
            pan: t.pan as f32,
            is_folder: t.is_folder,
            color: t.color.unwrap_or(0),
        })
        .collect()
}

async fn build_state(b: &SessionBridge, indices: &ActiveIndices) -> WatchSessionState {
    let mut state = WatchSessionState {
        song_index: indices.song_index.map(|i| i as i32).unwrap_or(-1),
        section_index: indices.section_index.map(|i| i as i32).unwrap_or(-1),
        is_playing: indices.is_playing,
        song_progress: indices.song_progress.unwrap_or(0.0) as f32,
        section_progress: indices.section_progress.unwrap_or(0.0) as f32,
        revision: b.revision.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ..Default::default()
    };

    if let Ok(setlist) = b.setlist.setlist().await {
        state.songs = setlist.songs.iter().map(|s| s.name.clone()).collect();
        if let Some(song) = indices.song_index.and_then(|i| setlist.songs.get(i)) {
            state.sections = song.sections.iter().map(|s| s.name.clone()).collect();
            let chart = song_chart(b, indices.song_index.unwrap_or(0)).await;
            let t = indices.song_progress.unwrap_or(0.0) * song.duration();
            let (chords, lyric) = chord_window(&chart, t);
            state.chords = chords;
            state.lyric_line = lyric;
        }
    }

    state.tracks = mixer_tracks().await;
    state
}

// ── Handlers ──────────────────────────────────────────────────────────────

async fn current_indices(b: &SessionBridge) -> ActiveIndices {
    // No "get indices" RPC — synthesize a snapshot from the active song /
    // section queries; the SSE stream corrects it on the next publish.
    let mut indices = ActiveIndices::default();
    if let Ok(setlist) = b.setlist.setlist().await
        && let Ok(song) = b.setlist.active_song().await {
            indices.song_index = setlist.songs.iter().position(|s| s.id == song.id);
            if let Ok(section) = b.setlist.active_section().await {
                indices.section_index =
                    song.sections.iter().position(|s| s.section_id == section.section_id);
            }
        }
    indices
}

async fn state_snapshot(State(b): State<SessionBridge>) -> String {
    let indices = current_indices(&b).await;
    to_json(&build_state(&b, &indices).await)
}

fn to_json(state: &WatchSessionState) -> String {
    facet_json::to_string(state).unwrap_or_else(|e| {
        tracing::error!(error = %e, "watch session state serialize failed");
        "{}".to_string()
    })
}

async fn transport(State(b): State<SessionBridge>, Path(cmd): Path<String>) -> Result<(), StatusCode> {
    let r = match cmd.as_str() {
        "play" => b.setlist.play().await,
        "pause" => b.setlist.pause().await,
        "stop" => b.setlist.stop().await,
        "toggle" => b.setlist.toggle_playback().await,
        "next-song" => b.setlist.next_song().await,
        "prev-song" => b.setlist.previous_song().await,
        "next-section" => b.setlist.next_section().await,
        "prev-section" => b.setlist.previous_section().await,
        _ => return Err(StatusCode::NOT_FOUND),
    };
    r.map(|_| ()).map_err(|e| {
        tracing::warn!(error = ?e, cmd = %cmd, "watch session transport failed");
        StatusCode::BAD_GATEWAY
    })
}

async fn seek_section(
    State(b): State<SessionBridge>,
    Path((song, section)): Path<(u32, u32)>,
) -> Result<(), StatusCode> {
    b.setlist
        .seek_to_section(song as usize, section as usize)
        .await
        .map(|_| ())
        .map_err(|e| {
            tracing::warn!(error = ?e, song, section, "watch session seek failed");
            StatusCode::BAD_GATEWAY
        })
}

/// Run `op` against the track resolved by GUID on the current project —
/// the `mixer_view::TrackSync` idiom.
async fn with_track<F, Fut>(guid: &str, op: F) -> Result<(), StatusCode>
where
    F: FnOnce(daw::rpc::TrackHandle) -> Fut,
    Fut: std::future::Future<Output = Result<(), daw::rpc::Error>>,
{
    let daw = daw::get().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let project = daw.current_project().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    let handle = project
        .tracks()
        .by_guid(guid)
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?
        .ok_or(StatusCode::NOT_FOUND)?;
    op(handle).await.map_err(|_| StatusCode::BAD_GATEWAY)
}

async fn track_op(
    State(_b): State<SessionBridge>,
    Path((guid, op)): Path<(String, String)>,
) -> Result<(), StatusCode> {
    // Toggles need current state: read the track list first.
    let current = mixer_tracks().await;
    let track = current.iter().find(|t| t.guid == guid).ok_or(StatusCode::NOT_FOUND)?;
    match op.as_str() {
        "toggle-mute" => {
            let muted = track.muted;
            with_track(&guid, |h| async move {
                if muted { h.unmute().await } else { h.mute().await }
            })
            .await
        }
        "toggle-solo" => {
            let soloed = track.soloed;
            with_track(&guid, |h| async move {
                if soloed { h.unsolo().await } else { h.solo().await }
            })
            .await
        }
        _ => Err(StatusCode::NOT_FOUND),
    }
}

async fn track_volume(
    State(_b): State<SessionBridge>,
    Path((guid, v)): Path<(String, f64)>,
) -> Result<(), StatusCode> {
    let v = v.clamp(0.0, 1.0);
    with_track(&guid, |h| async move { h.set_volume(v).await }).await
}

/// SSE: an initial snapshot, then a `WatchSessionState` per meaningful
/// `active_indices` event (indices/playing changed, or progress moved ≥1%).
async fn events(State(b): State<SessionBridge>) -> Result<axum::response::Response, StatusCode> {
    use session_proto::services::setlist_service::SetlistServiceStreamClient;

    let stream_client: SetlistServiceStreamClient = b.local.establish().await.map_err(|e| {
        tracing::warn!(error = %e, "watch session /events: stream client failed");
        StatusCode::BAD_GATEWAY
    })?;

    let (out_tx, out_rx) = tokio::sync::mpsc::channel::<String>(8);

    let indices = current_indices(&b).await;
    let _ = out_tx.send(to_json(&build_state(&b, &indices).await)).await;

    let bridge = b.clone();
    tokio::spawn(async move {
        let (tx, mut rx) = vox::channel::<ActiveIndices>();
        let call = stream_client.active_indices(tx);
        let pump = async {
            let mut last_key = (i64::MIN, i64::MIN, false, -1i64);
            while let Ok(Some(event)) = rx.recv().await {
                let mut indices: Option<ActiveIndices> = None;
                let _ = event.map(|ev| indices = Some(ev.clone()));
                let Some(indices) = indices else { continue };
                let key = (
                    indices.song_index.map(|i| i as i64).unwrap_or(-1),
                    indices.section_index.map(|i| i as i64).unwrap_or(-1),
                    indices.is_playing,
                    (indices.song_progress.unwrap_or(0.0) * 100.0) as i64,
                );
                if key == last_key {
                    continue;
                }
                last_key = key;
                let state = build_state(&bridge, &indices).await;
                if out_tx.send(to_json(&state)).await.is_err() {
                    break; // SSE client gone
                }
            }
        };
        tokio::select! {
            _ = pump => {}
            _ = call => {}
        }
    });

    use axum::response::IntoResponse as _;
    Ok(Sse::new(crate::engine_watch::SseStream(out_rx))
        .keep_alive(KeepAlive::default())
        .into_response())
}
