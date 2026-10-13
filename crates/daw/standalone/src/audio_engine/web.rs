//! Browser bindings for [`ProjectRenderer`].
//!
//! A wasm-bindgen wrapper that an AudioWorklet processor instantiates
//! once on the worklet thread, then drives via `render(out_l, out_r)`
//! every block. Audio sources are supplied as already-decoded f32 PCM
//! (browsers decode via `AudioContext.decodeAudioData`), so the Rust
//! side never has to do any I/O.
//!
//! Build with `--features web --target wasm32-unknown-unknown`. The
//! companion JS in `examples/web_worklet/processor.js` shows the
//! minimal `AudioWorkletProcessor` glue.
//!
//! Reference layout (a typical app):
//!
//! ```text
//! main thread:                    audio thread (AudioWorklet):
//! ┌─────────────────┐             ┌────────────────────────────┐
//! │ fetch RPP text  │             │ WebRenderer (instance #1)  │
//! │ load_rpp_text() │             │   render(out_l, out_r)     │
//! │ decode audio    │  postMessage│     └─► ProjectRenderer    │
//! │ attach_source() ├────────────►│              └─► snapshot   │
//! │ play() / pause()│             │                  + mix      │
//! └─────────────────┘             └────────────────────────────┘
//! ```
//!
//! In the simplest setup both sides share a `Standalone` via the
//! worklet's `port` postMessage protocol carrying serialized actions
//! (proto trait methods). The Rust side here doesn't prescribe the
//! protocol — it just exposes the renderer.

#![cfg(all(target_arch = "wasm32", feature = "web"))]

use std::sync::Arc;

use wasm_bindgen::prelude::*;

use super::decoder::DecodedAudio;
use super::render::ProjectRenderer;
use crate::audio_engine::materialize::{attach_audio_source, detach_audio_source};
use crate::sync::Standalone;
use crate::transport_engine::{InstantSeconds, PlayStateRepr, SampleClock, TransportShared};

/// Browser-side wrapper. Owns a `Standalone` + selected project guid
/// + sample-rate-aware shared transport. Cheap to construct;
/// expensive operations (decode, parse) live on the calling side.
#[wasm_bindgen]
pub struct WebRenderer {
    daw: Standalone,
    /// The SELECTED project — the one [`render`] renders and every transport
    /// / mixer op targets. ONE renderer hosts the whole setlist as separate
    /// projects (exactly the native engine's layout); a song switch is
    /// [`select_project`](Self::select_project), never a rebuild. `RefCell`
    /// is fine: the worklet global scope is single-threaded.
    project_guid: std::cell::RefCell<String>,
    sample_rate: u32,
    /// The selected project's transport (swapped by `select_project`).
    shared: std::cell::RefCell<Arc<TransportShared>>,
    /// Quantized seek queued by [`seek_seconds_at`](Self::seek_seconds_at):
    /// `(execute_at_sample, target_sample)`. Executed by [`render`] on the
    /// block where the playhead reaches `execute_at` — the audio thread jumps
    /// on the boundary itself (block-accurate), no port round-trip.
    pending_seek: std::cell::Cell<Option<(u64, u64)>>,
}

#[wasm_bindgen]
impl WebRenderer {
    /// Construct a fresh renderer + seed a project. The worklet should
    /// hand its actual `sampleRate` so the sample clock matches the
    /// browser's output.
    #[wasm_bindgen(constructor)]
    pub fn new(sample_rate: u32) -> WebRenderer {
        let daw = Standalone::new();
        // Fixed guid, NOT uuid::new_v4(): `AudioWorkletGlobalScope` has no
        // `crypto` — getrandom would panic when the renderer is constructed
        // on the audio thread (the intended home; see examples/web_worklet).
        // "web-project" is the DEFAULT selected project (single-song
        // callers); setlists add one project per song via [`add_project`]
        // and switch with [`select_project`].
        let project_guid = daw.seed_project(daw_proto::ProjectInfo {
            guid: "web-project".to_string(),
            name: "web".into(),
            path: String::new(),
        });
        let bundle = daw.transport_engine_for(&project_guid);
        // The worklet drives advance() — don't double-tick from the
        // soft clock.
        bundle.disable_soft_clock();
        bundle.shared.set_sample_rate(sample_rate);
        let shared = bundle.shared.clone();
        WebRenderer {
            daw,
            project_guid: std::cell::RefCell::new(project_guid),
            sample_rate,
            shared: std::cell::RefCell::new(shared),
            pending_seek: std::cell::Cell::new(None),
        }
    }

    /// The selected project's guid (owned clone — internal state is a cell).
    fn current(&self) -> String {
        self.project_guid.borrow().clone()
    }

    /// The selected project's transport.
    fn transport(&self) -> Arc<TransportShared> {
        self.shared.borrow().clone()
    }

    /// Seed one MORE project (a setlist song). Idempotent per guid. The new
    /// project's transport is configured like the default one (no soft clock,
    /// worklet sample rate) but it is NOT selected — call
    /// [`select_project`](Self::select_project).
    #[wasm_bindgen(js_name = addProject)]
    pub fn add_project(&self, guid: &str, name: &str) {
        if self.daw.read_project(guid, |_| ()).is_some() {
            return;
        }
        let guid = self.daw.seed_project(daw_proto::ProjectInfo {
            guid: guid.to_string(),
            name: name.to_string(),
            path: String::new(),
        });
        let bundle = self.daw.transport_engine_for(&guid);
        bundle.disable_soft_clock();
        bundle.shared.set_sample_rate(self.sample_rate);
    }

    /// Select which project [`render`] renders (a setlist song switch). The
    /// graph, tracks, and any attached PCM persist across switches — this
    /// swaps the transport + meter bank only. No-op for the already-selected
    /// project or an unknown guid.
    #[wasm_bindgen(js_name = selectProject)]
    pub fn select_project(&self, guid: &str) {
        if *self.project_guid.borrow() == guid {
            return;
        }
        if self.daw.read_project(guid, |_| ()).is_none() {
            return;
        }
        self.pending_seek.set(None);
        let bundle = self.daw.transport_engine_for(guid);
        *self.shared.borrow_mut() = bundle.shared.clone();
        *self.project_guid.borrow_mut() = guid.to_string();
        // Meter bank per selected project (track order == stem order).
        let count = daw_proto::Tracks::count(
            &self.daw,
            daw_proto::ProjectContext::Project(guid.to_string()),
        ) as usize;
        self.daw.set_meters(crate::metering::Meters::new(count));
    }

    /// Parse RPP text into the renderer's project. Audio sources are
    /// NOT materialized — call [`attach_audio_source_pcm`] for each
    /// take whose source you've decoded.
    #[cfg(any(feature = "rpp-project", feature = "rpp-project-wasm"))]
    #[wasm_bindgen(js_name = loadRppText)]
    pub fn load_rpp_text(&self, rpp_text: &str) -> Result<JsValue, JsValue> {
        // Replace the project entirely so reloads are clean.
        // (For incremental updates, callers should use the trait
        // surface directly.)
        let summary =
            crate::project_loader::load_rpp_text(&self.daw, "web", "/web/in-memory.rpp", rpp_text)
                .map_err(|e| JsValue::from_str(&e))?;
        let _ = summary; // counts available via separate accessors below
        Ok(JsValue::NULL)
    }

    /// Attach an already-decoded source for `take_guid`. JS callers
    /// typically get the PCM via `AudioContext.decodeAudioData` →
    /// `AudioBuffer.getChannelData(ch)` → `Float32Array`.
    #[wasm_bindgen(js_name = attachAudioSource)]
    pub fn attach_audio_source_pcm(
        &self,
        take_guid: &str,
        interleaved_pcm: &[f32],
        channels: u32,
        sample_rate: u32,
    ) {
        attach_audio_source(
            &self.daw,
            &self.current(),
            take_guid,
            DecodedAudio::charged(
                interleaved_pcm.to_vec(),
                channels.max(1) as u16,
                sample_rate.max(1),
                "web-pcm",
            ),
        );
    }

    /// [`attach_audio_source_pcm`] targeting an EXPLICIT project — the
    /// setlist path, where decodes race song switches and must land on the
    /// song they were started for.
    #[wasm_bindgen(js_name = attachAudioSourceIn)]
    pub fn attach_audio_source_pcm_in(
        &self,
        project: &str,
        take_guid: &str,
        interleaved_pcm: &[f32],
        channels: u32,
    ) {
        attach_audio_source(
            &self.daw,
            project,
            take_guid,
            DecodedAudio::charged(
                interleaved_pcm.to_vec(),
                channels.max(1) as u16,
                self.sample_rate.max(1),
                "web-pcm",
            ),
        );
    }

    /// Seed one stem: a track + an audio item/take whose source points at
    /// `source_path`, keyed by the stable `take_guid` the caller will later
    /// pass to [`attach_audio_source_pcm`] once the browser has decoded the
    /// file. Mirrors the native `media_seed` layout (track → MIDI item → flip
    /// the active take to audio) but *pins* the take's guid to `take_guid` so
    /// the async decode can attach its PCM by that stable key
    /// (`ProjectState.audio_sources` is keyed by the active take's guid).
    ///
    /// The item is placed at 0s with a generously long length; the decoded
    /// source itself bounds the audible region (past its end reads silence),
    /// so no per-song length is needed here.
    #[wasm_bindgen(js_name = addStemTrack)]
    pub fn add_stem_track(&self, display_name: &str, take_guid: &str, source_path: &str) {
        let project = self.current();
        self.add_stem_track_in(&project, display_name, take_guid, source_path);
    }

    /// [`add_stem_track`] targeting an EXPLICIT project (a setlist song).
    #[wasm_bindgen(js_name = addStemTrackIn)]
    pub fn add_stem_track_in(
        &self,
        project: &str,
        display_name: &str,
        take_guid: &str,
        source_path: &str,
    ) {
        use daw_proto::midi::Midi;
        use daw_proto::{ItemRef, ProjectContext, Takes, TrackRef, Tracks};

        // A generous item length so the whole song plays; the decoded source
        // (however long) is what actually bounds playback.
        const ITEM_LEN_SECONDS: f64 = 3600.0;

        let ctx = ProjectContext::Project(project.to_string());
        let Ok(track) = Tracks::add(&self.daw, ctx.clone(), display_name, None) else {
            return;
        };
        let Some(loc) = Midi::create_midi_item(
            &self.daw,
            ctx.clone(),
            TrackRef::Guid(track),
            0.0,
            ITEM_LEN_SECONDS,
        ) else {
            return;
        };
        let ItemRef::Guid(item_guid) = &loc.item else {
            return;
        };
        let Some(active) = Takes::get_active_take(&self.daw, ctx, ItemRef::Guid(item_guid.clone()))
        else {
            return;
        };
        // Pin the freshly created take's guid to `take_guid` and flip it from
        // MIDI to an audio source pointing at `source_path`.
        self.daw.write_project(project, |p| {
            for tl in p.takes.values_mut() {
                for t in tl.takes.iter_mut() {
                    if t.guid == active.guid {
                        t.guid = take_guid.to_string();
                        t.is_midi = false;
                        t.source_type = daw_proto::item::SourceType::Audio;
                        t.source_file_path = Some(source_path.to_string());
                    }
                }
            }
        });

        // (Re)size the meter bank to the SELECTED project's track count so
        // `render_block` has a cell per track to write peaks into
        // ([`Self::track_peaks`] reads them for UI VU meters). Natively the
        // mixer attach does this; the web path has no mixer, so size it here.
        // Tracks are only added pre-playback, so resetting is harmless.
        if project == self.current() {
            let count = daw_proto::Tracks::count(
                &self.daw,
                daw_proto::ProjectContext::Project(project.to_string()),
            ) as usize;
            self.daw.set_meters(crate::metering::Meters::new(count));
        }
    }

    /// Drop a previously-attached source (searched in the SELECTED project).
    #[wasm_bindgen(js_name = detachAudioSource)]
    pub fn detach_audio_source(&self, take_guid: &str) {
        detach_audio_source(&self.daw, &self.current(), take_guid);
    }

    /// Drop a previously-attached source from an EXPLICIT project — used by
    /// the setlist path to free the outgoing song's PCM on a song switch.
    #[wasm_bindgen(js_name = detachAudioSourceIn)]
    pub fn detach_audio_source_in(&self, project: &str, take_guid: &str) {
        detach_audio_source(&self.daw, project, take_guid);
    }

    // ── Mixer (track order == the order stems were added) ────────────
    //
    // These go through the same `Tracks` trait ops native GUIs use, so the
    // render snapshot picks them up on the next block — mute/solo routing
    // (incl. solo-passthrough) and fader gain are the graph's own logic.

    #[wasm_bindgen(js_name = setTrackMute)]
    pub fn set_track_mute(&self, index: u32, muted: bool) {
        use daw_proto::{ProjectContext, TrackRef, Tracks};
        let _ = Tracks::set_muted(
            &self.daw,
            ProjectContext::Project(self.current()),
            TrackRef::Index(index),
            muted,
        );
    }

    #[wasm_bindgen(js_name = setTrackSolo)]
    pub fn set_track_solo(&self, index: u32, soloed: bool) {
        use daw_proto::{ProjectContext, TrackRef, Tracks};
        let _ = Tracks::set_soloed(
            &self.daw,
            ProjectContext::Project(self.current()),
            TrackRef::Index(index),
            soloed,
        );
    }

    /// Fader gain, linear (1.0 = unity).
    #[wasm_bindgen(js_name = setTrackVolume)]
    pub fn set_track_volume(&self, index: u32, volume: f64) {
        use daw_proto::{ProjectContext, TrackRef, Tracks};
        let _ = Tracks::set_volume(
            &self.daw,
            ProjectContext::Project(self.current()),
            TrackRef::Index(index),
            volume,
        );
    }

    /// Every distinct source path referenced by takes in the loaded
    /// project, sorted. Browsers fetch each one (e.g. via `fetch()`
    /// to a Nextcloud / S3 / static host base URL), decode via
    /// `AudioContext.decodeAudioData`, then call
    /// [`attachAudioSource`](Self::attach_audio_source_pcm) for each
    /// matching take.
    #[wasm_bindgen(js_name = pathsToResolve)]
    pub fn paths_to_resolve(&self) -> Vec<JsValue> {
        self.daw
            .media_bay()
            .paths_to_resolve(daw_proto::ProjectContext::Project(self.current()))
            .into_iter()
            .map(|s| JsValue::from_str(&s))
            .collect()
    }

    /// All `(take_guid, source_path)` pairs in the loaded project,
    /// returned as a flat `[take, path, take, path, …]` JS array.
    /// Browsers use this to map fetched files back to the takes
    /// they belong to.
    #[wasm_bindgen(js_name = takeSources)]
    pub fn take_sources(&self) -> Vec<JsValue> {
        let mut out = Vec::new();
        let _ = self.daw.read_project(&self.current(), |p| {
            for tl in p.takes.values() {
                for take in &tl.takes {
                    let Some(path) = take.source_file_path.as_ref() else {
                        continue;
                    };
                    if path.is_empty() {
                        continue;
                    }
                    out.push(JsValue::from_str(&take.guid));
                    out.push(JsValue::from_str(path));
                }
            }
        });
        out
    }

    /// Render `frames` stereo frames into the two output channels.
    /// The worklet calls this with the buffers AudioWorkletProcessor
    /// provides each `process()` invocation.
    pub fn render(&self, out_left: &mut [f32], out_right: &mut [f32]) {
        let frames = out_left.len().min(out_right.len());
        let shared = self.transport();
        let playing = shared.play_state().is_advancing();
        if !playing {
            for s in out_left.iter_mut() {
                *s = 0.0;
            }
            for s in out_right.iter_mut() {
                *s = 0.0;
            }
            return;
        }
        // Execute a queued quantized seek on the block that reaches its
        // boundary — the jump lands ON the measure line (± one 128-frame
        // block, ~3 ms), so section changes splice seamlessly.
        if let Some((at, target)) = self.pending_seek.get() {
            let cur = shared.playhead_samples().0.max(0) as u64;
            if cur + frames as u64 >= at {
                shared.set_playhead(crate::transport_engine::InstantSamples(target as i64));
                self.pending_seek.set(None);
            }
        }
        // Varispeed: the timeline from the fractional playhead at the
        // transport rate — `advance` below moves it on by exactly that.
        let start = shared.playhead_samples_f64().max(0.0);
        let block = ProjectRenderer::new(&self.daw, &self.current(), self.sample_rate)
            .render_block_varispeed(start, frames, shared.playrate());
        for i in 0..frames {
            out_left[i] = block.samples[i * 2];
            out_right[i] = block.samples[i * 2 + 1];
        }
        shared.advance(frames as u32);
    }

    // ── Live MIDI (browser keys rig) ─────────────────────────────
    //
    // Raw + convenience producers for the wasm live-MIDI queue
    // (`Standalone::push_live_midi`): events are queued targeted at a
    // track of the SELECTED project and drained by the next render
    // block, exactly like the native rtrb ring. Track addressing is by
    // project track index (the order tracks were seeded), mirroring the
    // mixer methods above.

    /// The guid of the selected project's track at `index`.
    fn track_guid(&self, index: u32) -> Option<String> {
        use daw_proto::{ProjectContext, Tracks};
        let ctx = ProjectContext::Project(self.current());
        Tracks::all(&self.daw, ctx)
            .into_iter()
            .nth(index as usize)
            .map(|t| t.guid)
    }

    /// Raw 3-byte MIDI message to one track. `true` when the event was
    /// decoded and queued. Two-byte messages (program change / channel
    /// pressure) pass `0` for `data2`.
    #[wasm_bindgen(js_name = midiToTrack)]
    pub fn midi_to_track(&self, track_index: u32, status: u8, data1: u8, data2: u8) -> bool {
        let Some(guid) = self.track_guid(track_index) else {
            return false;
        };
        let Ok((ev, _)) = daw_proto::MidiEvent::decode(&[status, data1, data2]) else {
            return false;
        };
        self.daw.push_live_midi(&guid, ev)
    }

    /// Raw 3-byte MIDI message to EVERY track of the selected project —
    /// the keys-rig lane shape (each lane's zone filters its own notes).
    #[wasm_bindgen(js_name = midiToAllTracks)]
    pub fn midi_to_all_tracks(&self, status: u8, data1: u8, data2: u8) -> bool {
        let Ok((ev, _)) = daw_proto::MidiEvent::decode(&[status, data1, data2]) else {
            return false;
        };
        use daw_proto::{ProjectContext, Tracks};
        let ctx = ProjectContext::Project(self.current());
        let mut any = false;
        for t in Tracks::all(&self.daw, ctx) {
            any |= self.daw.push_live_midi(&t.guid, ev.clone());
        }
        any
    }

    /// Note-On (`channel` 0–15) to one track. Velocity 0 decodes as a
    /// Note-Off by MIDI convention.
    #[wasm_bindgen(js_name = noteOn)]
    pub fn note_on(&self, track_index: u32, channel: u8, key: u8, velocity: u8) -> bool {
        self.midi_to_track(track_index, 0x90 | (channel & 0x0F), key, velocity)
    }

    /// Note-Off to one track.
    #[wasm_bindgen(js_name = noteOff)]
    pub fn note_off(&self, track_index: u32, channel: u8, key: u8) -> bool {
        self.midi_to_track(track_index, 0x80 | (channel & 0x0F), key, 0)
    }

    /// Control Change to one track.
    #[wasm_bindgen(js_name = controlChange)]
    pub fn control_change(&self, track_index: u32, channel: u8, controller: u8, value: u8) -> bool {
        self.midi_to_track(track_index, 0xB0 | (channel & 0x0F), controller, value)
    }

    /// Pitch bend (14-bit raw, 8192 = center) to one track.
    #[wasm_bindgen(js_name = pitchBend)]
    pub fn pitch_bend(&self, track_index: u32, channel: u8, value14: u16) -> bool {
        let v = value14.min(16_383);
        self.midi_to_track(
            track_index,
            0xE0 | (channel & 0x0F),
            (v & 0x7F) as u8,
            (v >> 7) as u8,
        )
    }

    /// All Notes Off (CC 123) to every track of the selected project.
    #[wasm_bindgen(js_name = allNotesOff)]
    pub fn all_notes_off(&self) {
        self.midi_to_all_tracks(0xB0, 123, 0);
    }

    /// Panic — All Sound Off (CC 120) to every track of the selected project.
    pub fn panic(&self) {
        self.midi_to_all_tracks(0xB0, 120, 0);
    }

    // ── Transport ────────────────────────────────────────────────

    pub fn play(&self) {
        self.transport().set_play_state(PlayStateRepr::Playing);
    }
    pub fn pause(&self) {
        self.transport().set_play_state(PlayStateRepr::Paused);
    }
    pub fn stop(&self) {
        self.pending_seek.set(None);
        let shared = self.transport();
        shared.set_play_state(PlayStateRepr::Stopped);
        shared.set_playhead(crate::transport_engine::InstantSamples(0));
    }
    #[wasm_bindgen(js_name = isPlaying)]
    pub fn is_playing(&self) -> bool {
        self.transport().play_state().is_advancing()
    }
    #[wasm_bindgen(js_name = positionSeconds)]
    pub fn position_seconds(&self) -> f64 {
        let shared = self.transport();
        let clock = SampleClock::new(shared.sample_rate());
        clock.samples_to_seconds(shared.playhead_samples()).0
    }
    #[wasm_bindgen(js_name = seekSeconds)]
    pub fn seek_seconds(&self, seconds: f64) {
        // An immediate seek supersedes any queued quantized jump.
        self.pending_seek.set(None);
        let shared = self.transport();
        let clock = SampleClock::new(shared.sample_rate());
        shared.set_playhead(clock.seconds_to_samples(InstantSeconds(seconds)));
    }

    /// Queue a QUANTIZED seek: when the transport reaches `at` seconds the
    /// render callback jumps to `target` seconds (see [`render`]). A later
    /// call replaces the queued jump; an immediate [`seek_seconds`] cancels
    /// it.
    #[wasm_bindgen(js_name = seekSecondsAt)]
    pub fn seek_seconds_at(&self, target: f64, at: f64) {
        let clock = SampleClock::new(self.transport().sample_rate());
        let at_s = clock.seconds_to_samples(InstantSeconds(at)).0.max(0) as u64;
        let target_s = clock.seconds_to_samples(InstantSeconds(target)).0.max(0) as u64;
        self.pending_seek.set(Some((at_s, target_s)));
    }

    // ── Introspection ────────────────────────────────────────────

    #[wasm_bindgen(js_name = projectGuid)]
    pub fn project_guid(&self) -> String {
        self.current()
    }
    #[wasm_bindgen(js_name = trackCount)]
    pub fn track_count(&self) -> u32 {
        use daw_proto::Tracks;
        Tracks::count(
            &self.daw,
            daw_proto::ProjectContext::Project(self.current()),
        )
    }
    #[wasm_bindgen(js_name = audioSourceCount)]
    pub fn audio_source_count(&self) -> u32 {
        self.daw.audio_source_count(&self.current()) as u32
    }

    /// Per-track peak levels (0.0..=1.0, max of L/R), indexed by track order —
    /// the render path writes these into the `Meters` cells every block, so
    /// this is a lock-free read for UI VU meters.
    #[wasm_bindgen(js_name = trackPeaks)]
    pub fn track_peaks(&self) -> Vec<f32> {
        let meters = self.daw.meters();
        (0..meters.len())
            .map(|i| {
                meters
                    .cell(i)
                    .map(|c| c.peak(0).max(c.peak(1)))
                    .unwrap_or(0.0)
            })
            .collect()
    }
}

/// Rust-only surface (NOT exported to JS) for in-tree wrapper crates that
/// extend the worklet — e.g. the browser keys rig, which seeds a lane
/// project + inserts `PluginInstance`s on this renderer's `Standalone` and
/// wraps the whole thing in its own `#[wasm_bindgen]` type (daw-standalone
/// cannot depend on signal-sampler: the dependency arrow runs the other
/// way through the daw facade).
impl WebRenderer {
    /// The backing standalone daw (cheap to clone).
    pub fn standalone(&self) -> &Standalone {
        &self.daw
    }

    /// The renderer's output sample rate.
    pub fn output_sample_rate(&self) -> u32 {
        self.sample_rate
    }
}
