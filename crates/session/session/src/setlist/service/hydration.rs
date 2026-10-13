//! Song hydration logic: cache lookups, chart extraction, fingerprint checking

use super::{
    CHART_REFRESH_FALLBACK_POLL_MS, HYDRATION_CONCURRENCY, MIDI_TRACK_TAG, SetlistServiceImpl,
};
use crate::song::builder::SongBuilder;
use architect::platform::Instant;
use daw::service::{ProjectContext, ProjectInfo, Projects};
use keyflow_daw_analysis::{DetectedChord, MidiChartData, MidiChartRequest, MidiChartsClient};
use session_proto::{Song, SongChartHydration, SongDetectedChord, SongId};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};

#[derive(Clone)]
pub struct SongCacheEntry {
    pub(crate) project_name: String,
    pub(crate) chart_fingerprint: Option<String>,
    pub(crate) songs: Vec<Song>,
}

#[derive(Clone)]
pub struct ProjectLoad {
    pub(crate) index: usize,
    pub(crate) guid: String,
    pub(crate) project_name: String,
}

impl<D> SetlistServiceImpl<D>
where
    D: Projects + architect::MaybeSendSync,
{
    /// Chart source of last resort: keyflow chart text stamped onto the
    /// project itself (ext-state `FTS/chart_text`, read over the global
    /// `daw::` facade). This is how songs carry their chart on backends
    /// without a MIDI chart-analysis service — the demo engine stamps the
    /// bundled charts here, and any organize/combine flow can do the same
    /// for imported charts. The fingerprint is a hash of the text, so
    /// edits invalidate caches exactly like the MIDI path.
    async fn fetch_ext_state_chart(project_guid: &str) -> Option<MidiChartData> {
        let daw = daw::get()?;
        let ctx = daw::service::ProjectContext::Project(project_guid.to_string());
        let chart_text = daw
            .ext_state()
            .get_project(
                ctx,
                super::CHART_EXT_STATE_SECTION,
                super::CHART_EXT_STATE_KEY,
            )
            .await
            .ok()??;
        if chart_text.trim().is_empty() {
            return None;
        }
        let fingerprint = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            chart_text.hash(&mut hasher);
            format!("extstate:{:016x}", hasher.finish())
        };
        Some(MidiChartData {
            source_track_name: "ext-state".to_string(),
            source_fingerprint: fingerprint,
            chart_text,
            chords: Vec::new(),
        })
    }
    pub(crate) fn song_is_hydrated(song: &Song) -> bool {
        !song.sections.is_empty() || song.end_seconds > song.start_seconds
    }

    pub(crate) async fn ensure_song_hydrated(&self, index: usize) -> Option<Song>
    where
        D: Clone + 'static,
    {
        let current = self.get_song_internal(index).await?;
        if Self::song_is_hydrated(&current) {
            return Some(current);
        }

        // `Projects::get` on `daw_reaper::Reaper` hits REAPER's main-
        // thread-only FFI (`file()` -> `EnumProjects` with a buffer, to
        // read the project's filename) — bounce it, same as every other
        // `self.daw.*` call reachable from an RPC handler.
        let daw = self.daw.clone();
        let project_guid = current.project_guid.clone();
        let project_name =
            daw_proto::main_thread::query(move || daw.get(&project_guid).map(|info| info.name))
                .await
                .flatten()
                .unwrap_or_else(|| current.project_guid.clone());
        let load = ProjectLoad {
            index,
            guid: current.project_guid.clone(),
            project_name,
        };
        let rebuilt = self
            .build_songs_with_cache(&load, Some(&current))
            .await
            .into_iter()
            .next()?;
        self.cache_chart_payload_for_song(&rebuilt).await;
        let mut rebuilt_light = rebuilt.clone();
        Self::strip_song_chart_payload(&mut rebuilt_light);

        let updated = {
            let mut guard = self.setlist.write().await;
            let Some(ref mut setlist) = *guard else {
                return Some(rebuilt_light);
            };
            let result = setlist.songs.get_mut(index).map_or(false, |song| {
                *song = rebuilt_light.clone();
                true
            });
            drop(guard);
            result
        };

        if updated {
            self.hydration_bus.emit((index, rebuilt_light.clone()));
            self.emit_cached_chart_payload_for_song(index, &rebuilt_light.project_guid)
                .await;
        }

        Some(rebuilt_light)
    }

    pub(crate) fn parse_project_name_fallback(project_name: &str) -> String {
        let base = project_name.strip_suffix(".rpp").unwrap_or(project_name);
        let base = base.split(" - ").next().unwrap_or(base).trim();
        // Drop a leading zero-padded setlist-order prefix ("00 Praise" → "Praise")
        // so transient name-only placeholders don't show the ordering index.
        let digits = base.chars().take_while(char::is_ascii_digit).count();
        let base = if (2..=3).contains(&digits)
            && base.get(digits..).is_some_and(|s| s.starts_with(' '))
        {
            base.get(digits.saturating_add(1)..)
                .unwrap_or("")
                .trim_start()
        } else {
            base
        };
        base.to_string()
    }

    pub(crate) fn make_song_id(
        existing_song: Option<&Song>,
        cached_song: Option<&Song>,
        _guid: &str,
    ) -> SongId {
        if let Some(song) = existing_song {
            return song.id.clone();
        }
        if let Some(song) = cached_song {
            return song.id.clone();
        }
        SongId::new()
    }

    pub(crate) fn make_name_only_song(
        guid: &str,
        project_name: &str,
        existing_song: Option<&Song>,
        cached_song: Option<&Song>,
    ) -> Song {
        Song {
            id: Self::make_song_id(existing_song, cached_song, guid),
            name: Self::parse_project_name_fallback(project_name),
            project_guid: guid.to_string(),
            start_seconds: 0.0,
            end_seconds: 0.0,
            count_in_seconds: None,
            sections: Vec::new(),
            comments: Vec::new(),
            tempo: None,
            time_signature: None,
            measure_positions: Vec::new(),
            chart_text: None,
            parsed_chart: None,
            detected_chords: Vec::new(),
            chart_fingerprint: None,
            advance_mode: None,
            color: None,
        }
    }

    pub(crate) fn strip_song_chart_payload(song: &mut Song) {
        song.chart_text = None;
        song.parsed_chart = None;
        song.detected_chords.clear();
    }

    fn map_detected_chords(chords: Vec<DetectedChord>) -> Vec<SongDetectedChord> {
        chords
            .into_iter()
            .map(|chord| SongDetectedChord {
                symbol: chord.symbol,
                start_ppq: chord.start_ppq,
                end_ppq: chord.end_ppq,
                root_pitch: chord.root_pitch,
                velocity: chord.velocity,
            })
            .collect()
    }

    /// Build a `MidiChartsClient` over the same vox channel daw is using.
    /// Chart service is registered by session's DAW service layer at startup;
    /// we just need a client over the existing `Caller`.
    ///
    /// Returns `None` when the global `Daw` hasn't been initialised —
    /// happens in the in-process `fts-extensions` host, which calls
    /// `daw::init_from_parts` instead of the desktop-style
    /// `Daw::init(caller)`. Callers must handle `None` by skipping
    /// chart hydration; without this guard the build path panics on
    /// the worker thread and the awaiting RPC client hangs forever
    /// (no response ever flows back).
    fn chart_client() -> Option<MidiChartsClient> {
        Some(MidiChartsClient::new(daw::get()?.caller().clone()))
    }

    pub(crate) async fn fetch_midi_chart_data(project_guid: &str) -> Option<MidiChartData> {
        let req = MidiChartRequest::new(
            Some(project_guid.to_string()),
            Some(MIDI_TRACK_TAG.to_string()),
        );
        // Same 2s safety cap as fetch_midi_source_fingerprint — see
        // the comment there for the rationale.
        let client = Self::chart_client()?;
        let call = client.generate_chart_data(req);
        let Ok(res) = architect::platform::timeout(Duration::from_secs(2), call).await else {
            debug!(
                "MIDI chart generation timed out (>2s) for project {} — skipping",
                project_guid
            );
            return None;
        };
        match res {
            Ok(data) => Some(data),
            Err(vox_err) => {
                debug!(
                    "MIDI chart generation unavailable for project {}: {}",
                    project_guid, vox_err
                );
                None
            }
        }
    }

    pub(crate) async fn fetch_midi_source_fingerprint(&self, project_guid: &str) -> Option<String> {
        let support_state = *self.fingerprint_method_supported.read().await;
        if support_state == Some(false) {
            return None;
        }

        let req = MidiChartRequest::new(
            Some(project_guid.to_string()),
            Some(MIDI_TRACK_TAG.to_string()),
        );
        // Cap the chart service call at 2s so a hung keyflow handler
        // (e.g. mid-edit while another agent is iterating on it) can't
        // wedge `build_from_open_projects` forever. On timeout we treat
        // it as "fingerprint unavailable" — the build still produces
        // valid songs, just without chart hydration.
        let client = Self::chart_client()?;
        let call = client.source_fingerprint(req);
        let Ok(res) = architect::platform::timeout(Duration::from_secs(2), call).await else {
            debug!(
                "MIDI source fingerprint timed out (>2s) for project {} — skipping",
                project_guid
            );
            return None;
        };
        match res {
            Ok(fingerprint) => {
                if support_state != Some(true) {
                    *self.fingerprint_method_supported.write().await = Some(true);
                }
                Some(fingerprint)
            }
            Err(vox_err) => {
                if matches!(vox_err, vox::VoxError::UnknownMethod) {
                    let mut guard = self.fingerprint_method_supported.write().await;
                    if *guard != Some(false) {
                        info!(
                            "MIDI chart service unavailable on this bridge; disabling fingerprint \
                             polling (load fts-extensions to enable keyflow chart analysis)"
                        );
                    }
                    *guard = Some(false);
                } else {
                    debug!(
                        "MIDI source fingerprint unavailable for project {}: {}",
                        project_guid, vox_err
                    );
                }
                None
            }
        }
    }

    pub(crate) async fn should_run_fallback_chart_refresh(&self, project_guid: &str) -> bool {
        let now = architect::platform::now();
        self.last_chart_refresh_attempt
            .with_write(|map| {
                if let Some(last) = map.get(project_guid)
                    && now.saturating_duration_since(*last)
                        < Duration::from_millis(CHART_REFRESH_FALLBACK_POLL_MS)
                {
                    return false;
                }
                map.insert(project_guid.to_string(), now);
                true
            })
            .await
    }

    pub(crate) fn apply_chart_data(song: &mut Song, chart_data: MidiChartData) {
        song.chart_fingerprint = Some(chart_data.source_fingerprint);
        // Parse the generated chart text into the structured Chart so clients
        // receive the full chart — including mid-song time-signature changes —
        // not just the source text. (Previously left `None` because the Chart's
        // maps couldn't cross the vox wire; the JIT-codec fix lifted that.)
        // The Song only rides infrequent setlist/hydration events, never the
        // per-tick transport path, so carrying the parsed chart here is cheap.
        match keyflow::parse(chart_data.chart_text.as_str()) {
            Ok(chart) => song.parsed_chart = Some(chart),
            Err(e) => {
                tracing::warn!(
                    project_guid = %song.project_guid,
                    "failed to parse generated chart text into Chart: {e}"
                );
                song.parsed_chart = None;
            }
        }
        song.chart_text = Some(chart_data.chart_text);
        song.detected_chords = Self::map_detected_chords(chart_data.chords);
    }

    pub(crate) fn chart_payload_from_song(song: &Song) -> Option<SongChartHydration> {
        let chart_text = song.chart_text.clone()?;
        let chart_fingerprint = song.chart_fingerprint.clone()?;
        Some(SongChartHydration {
            project_guid: song.project_guid.clone(),
            chart_text,
            detected_chords: song.detected_chords.clone(),
            chart_fingerprint,
        })
    }

    pub(crate) async fn cache_chart_payload_for_song(&self, song: &Song) {
        if let Some(payload) = Self::chart_payload_from_song(song) {
            self.chart_cache
                .insert(song.project_guid.clone(), payload)
                .await;
        }
    }

    pub(crate) async fn emit_cached_chart_payload_for_song(
        &self,
        index: usize,
        project_guid: &str,
    ) {
        if let Some(payload) = self.chart_cache.get(&project_guid.to_string()).await {
            self.chart_hydration_bus.emit((index, payload));
        }
    }

    pub(crate) async fn fetch_project_loads(projects: Vec<ProjectInfo>) -> Vec<ProjectLoad> {
        let semaphore = Arc::new(Semaphore::new(HYDRATION_CONCURRENCY));
        let mut loads = Vec::with_capacity(projects.len());
        for (index, project) in projects.into_iter().enumerate() {
            if let Ok(_permit) = semaphore.clone().acquire_owned().await {
                loads.push(ProjectLoad {
                    index,
                    guid: project.guid,
                    project_name: project.name,
                });
            }
        }
        loads
    }

    pub(crate) async fn build_songs_with_cache(
        &self,
        load: &ProjectLoad,
        existing_song: Option<&Song>,
    ) -> Vec<Song> {
        let source_fingerprint = self.fetch_midi_source_fingerprint(&load.guid).await;

        if let Some(cached) = self.song_cache.get(&load.guid).await {
            let fingerprint_matches = match source_fingerprint.as_ref() {
                Some(fingerprint) => cached.chart_fingerprint.as_deref() == Some(fingerprint),
                None => true,
            };

            if cached.project_name == load.project_name && fingerprint_matches {
                let mut songs = cached.songs;
                if let Some(first) = songs.first_mut() {
                    first.id = Self::make_song_id(existing_song, Some(first), &load.guid);
                }
                return songs;
            }
        }

        let chart_data = match Self::fetch_midi_chart_data(&load.guid).await {
            Some(data) => Some(data),
            // No MIDI chart service on this backend — fall back to chart
            // text stamped on the project (ext-state `FTS/chart_text`).
            None => Self::fetch_ext_state_chart(&load.guid).await,
        };

        // Extract songs through the backend-agnostic `SongBuilder::build(&Project)`
        // over the `daw` facade. The `Project`'s service calls (info / markers /
        // regions / tempo) bounce to the backend's required thread internally
        // (REAPER's main thread via daw_proto::main_thread; inline for
        // standalone), so this same path works against any daw backend — no
        // direct REAPER FFI, unlike the former `SongBuilder::build_native`.
        let build_result = match daw::get() {
            Some(daw) => match daw.project(load.guid.clone()).await {
                Ok(project) => SongBuilder::build(&project).await,
                Err(e) => Err(eyre::eyre!("resolve project {}: {e}", load.guid)),
            },
            None => Err(eyre::eyre!("daw facade not initialised")),
        };

        match build_result {
            Ok(mut songs) => {
                // Apply chart data and IDs to the first song
                if let Some(first_song) = songs.first_mut() {
                    first_song.id = Self::make_song_id(existing_song, None, &load.guid);
                    if let Some(data) = chart_data {
                        Self::apply_chart_data(first_song, data);
                    }
                    self.cache_chart_payload_for_song(first_song).await;
                }

                // Cache all songs for this project
                let cached_songs: Vec<Song> = songs
                    .iter()
                    .map(|s| {
                        let mut cached = s.clone();
                        Self::strip_song_chart_payload(&mut cached);
                        cached
                    })
                    .collect();
                let chart_fp = songs.first().and_then(|s| s.chart_fingerprint.clone());
                self.song_cache
                    .insert(
                        load.guid.clone(),
                        SongCacheEntry {
                            project_name: load.project_name.clone(),
                            chart_fingerprint: chart_fp,
                            songs: cached_songs,
                        },
                    )
                    .await;

                songs
            }
            Err(e) => {
                warn!(
                    "Failed to extract song from project {} ({}): {}",
                    load.index.saturating_add(1),
                    load.guid,
                    e
                );
                Vec::new()
            }
        }
    }

    pub(crate) async fn refresh_active_song_chart_if_changed(&self) -> bool
    where
        D: Clone + 'static,
    {
        // Avoid expensive chart fingerprint/chart generation probes while playing.
        // These probes can take tens of milliseconds and compete with transport streaming.
        if self.get_cached_indices().await.is_playing {
            return false;
        }

        let active_song_id = self.active_song_id.read().await.clone();
        let Some(active_song_id) = active_song_id else {
            return false;
        };

        let (song_index, song_snapshot) = {
            let guard = self.setlist.read().await;
            let Some(ref setlist) = *guard else {
                return false;
            };
            let Some((index, song)) = setlist
                .songs
                .iter()
                .enumerate()
                .find(|(_, song)| song.id.as_str() == active_song_id)
            else {
                return false;
            };
            let result = (index, song.clone());
            drop(guard);
            result
        };

        let fingerprint_supported = *self.fingerprint_method_supported.read().await != Some(false);
        if !fingerprint_supported
            && !self
                .should_run_fallback_chart_refresh(&song_snapshot.project_guid)
                .await
        {
            return false;
        }

        // `Projects::get` on `daw_reaper::Reaper` hits REAPER's main-thread-
        // only FFI (EnumProjects to resolve the guid) — bounce it, same as
        // every other `self.daw.*` call reachable from this background
        // chart-refresh poll.
        let daw = self.daw.clone();
        let project_guid = song_snapshot.project_guid.clone();
        let project_exists =
            daw_proto::main_thread::query(move || -> bool { daw.get(&project_guid).is_some() })
                .await
                .unwrap_or(false);
        if !project_exists {
            return false;
        }
        let source_fingerprint = if fingerprint_supported {
            self.fetch_midi_source_fingerprint(&song_snapshot.project_guid)
                .await
        } else {
            None
        };

        if let Some(ref fingerprint) = source_fingerprint
            && song_snapshot.chart_fingerprint.as_deref() == Some(fingerprint.as_str())
        {
            return false;
        }

        let Some(chart_data) = Self::fetch_midi_chart_data(&song_snapshot.project_guid).await
        else {
            return false;
        };
        if source_fingerprint.is_none()
            && song_snapshot.chart_fingerprint.as_deref()
                == Some(chart_data.source_fingerprint.as_str())
        {
            return false;
        }

        let mut updated_song = song_snapshot.clone();
        Self::apply_chart_data(&mut updated_song, chart_data);
        self.cache_chart_payload_for_song(&updated_song).await;
        let mut updated_song_light = updated_song.clone();
        Self::strip_song_chart_payload(&mut updated_song_light);

        let updated = {
            let mut guard = self.setlist.write().await;
            let Some(ref mut setlist) = *guard else {
                return false;
            };
            let result = setlist.songs.get_mut(song_index).map_or(false, |song| {
                *song = updated_song_light.clone();
                true
            });
            drop(guard);
            result
        };

        if updated {
            self.hydration_bus
                .emit((song_index, updated_song_light.clone()));
            self.emit_cached_chart_payload_for_song(song_index, &updated_song_light.project_guid)
                .await;
            let project_name = self
                .daw
                .get(&song_snapshot.project_guid)
                .map_or_else(|| song_snapshot.project_guid.clone(), |info| info.name);
            self.song_cache
                .insert(
                    song_snapshot.project_guid.clone(),
                    SongCacheEntry {
                        project_name,
                        chart_fingerprint: updated_song_light.chart_fingerprint.clone(),
                        songs: vec![updated_song_light],
                    },
                )
                .await;
            return true;
        }
        false
    }
}
