//! Sampler-block runtime — Phase 1 of the Block → Module → Layer → Engine
//! hierarchy described in the project docs.
//!
//! A [`SamplerBlock`] is the unit-of-loading for a single `.signalpack`. It
//! wraps a [`SampleEngine`] (the existing voice-allocation + render core)
//! and adds block-level parameters (gain, pan, transpose) that apply to the
//! engine's output. Future block types (`OscillatorBlock`, `FilterBlock`, …)
//! will live alongside this module and share a common `Block` trait.
//!
//! Loading paths:
//! - **`SamplerBlock::from_spec`** — given a parsed [`BlockSpec`] (a
//!   `.signalblock` file), opens its referenced `.signalpack` and applies
//!   the spec's params.
//! - **`SamplerBlock::from_pack`** — bare-pack convenience: creates a
//!   default-params block from a `.signalpack` directly. Used when the TUI
//!   loads a pack without an accompanying `.signalblock`.

use std::path::{Path, PathBuf};

use facet::Facet;

use crate::engine::cache::{EvictStats, SampleCache};
use crate::{PlayerPatch, SampleEngine, SamplerError, VoiceConfig};

// ── Persisted form ──────────────────────────────────────────────────────────

/// One block-parameter override — a `(name, value)` pair with raw f32
/// (unlike `signal_proto::ParameterValue` which is normalized 0..1).
///
/// Used in two places:
/// - **Module slots** carry `Vec<ParamOverride>` to tweak a block's params
///   on a per-slot basis without authoring a separate `.signalblock`.
/// - **Layer/Engine modulation targets** will eventually drive these too,
///   reusing the same name-keyed dispatch in [`SamplerBlock::apply_override`].
#[derive(Debug, Clone, Facet)]
pub struct ParamOverride {
    pub param: String,
    pub value: f32,
}

impl ParamOverride {
    pub fn new(param: impl Into<String>, value: f32) -> Self {
        Self {
            param: param.into(),
            value,
        }
    }
}

/// Block-level audio params applied at the SamplerBlock's output.
///
/// All defaults are no-ops: `gain_db=0`, `pan=0`, `transpose=0`,
/// `tune_cents=0`. Loading a `.signalpack` with no `.signalblock` file
/// produces a block with default params — same audio you'd get today.
#[derive(Debug, Clone, Default, Facet)]
pub struct BlockParams {
    /// Linear gain in decibels. Default 0 dB.
    #[facet(default)]
    pub gain_db: f32,
    /// Stereo pan. -1.0 = full left, 0 = centre, 1.0 = full right.
    #[facet(default)]
    pub pan: f32,
    /// Transpose incoming MIDI notes by this many semitones before
    /// dispatching to the engine. Useful for octave-shifted drum kits or
    /// pitched samplers.
    #[facet(default)]
    pub transpose: i8,
    /// Fine-tune in cents (informational for now — not yet applied to
    /// per-voice playback rate at the engine level).
    #[facet(default)]
    pub tune_cents: i16,
}

/// Parsed `.signalblock` file. References ONE `.signalpack` plus its
/// block-level parameters.
#[derive(Debug, Clone, Facet)]
pub struct BlockSpec {
    pub name: String,
    /// Discriminator for future block types. `"sampler"` is the only
    /// currently-supported value; others (`"oscillator"`, `"filter"`, …)
    /// will be added as Block trait grows.
    #[facet(default)]
    pub block_type: String,
    /// Path to the referenced `.signalpack`. Relative paths resolve from
    /// the directory containing the `.signalblock` file; absolute paths
    /// are used as-is.
    pub pack: String,
    #[facet(default)]
    pub params: BlockParams,
}

impl BlockSpec {
    pub fn from_file(path: &Path) -> Result<Self, SamplerError> {
        let text = std::fs::read_to_string(path)?;
        facet_styx::from_str(&text).map_err(|e| SamplerError::SpecParse(e.to_string()))
    }
}

// ── Runtime ────────────────────────────────────────────────────────────────

/// Runtime sampler block: one `.signalpack` + block-level params + the
/// engine that voices it.
pub struct SamplerBlock {
    pub name: String,
    engine: SampleEngine,
    params: BlockParams,
    /// Linear gain derived from `params.gain_db` — cached so the audio
    /// callback doesn't recompute it every block.
    gain_lin: f32,
    /// Equal-power pan multipliers for L/R — cached.
    pan_l: f32,
    pan_r: f32,
    /// Scratch buffer used when block params demand post-processing
    /// (gain != 1.0 or pan != 0.0). Pre-allocated; never resized inside
    /// the audio callback.
    scratch: Vec<f32>,
    resize_events: u64,
}

impl SamplerBlock {
    /// Create a SamplerBlock from a parsed `BlockSpec`. `spec_dir` is the
    /// directory of the `.signalblock` file, used to resolve a relative
    /// `pack` path.
    pub fn from_spec(
        spec: BlockSpec,
        spec_dir: &Path,
        sample_rate: u32,
    ) -> Result<Self, SamplerError> {
        let pack_buf = PathBuf::from(&spec.pack);
        let pack_path = if pack_buf.is_absolute() {
            pack_buf
        } else {
            spec_dir.join(pack_buf)
        };
        Self::build(spec.name, &pack_path, spec.params, sample_rate)
    }

    /// Construct a SamplerBlock from an already-built `SampleEngine`.
    /// Used by code paths that need to supply a non-default section/mic
    /// at load time (e.g. legacy `load_instrument`); for new code prefer
    /// `from_pack` or `from_spec`.
    pub fn from_engine(name: String, engine: SampleEngine, params: BlockParams) -> Self {
        let gain_lin = db_to_lin(params.gain_db);
        let (pan_l, pan_r) = pan_gains(params.pan);
        Self {
            name,
            engine,
            params,
            gain_lin,
            pan_l: pan_l * gain_lin,
            pan_r: pan_r * gain_lin,
            scratch: Vec::new(),
            resize_events: 0,
        }
    }

    /// Pre-size the optional gain/pan render scratch for the expected audio
    /// callback size. No-op for default-gain/default-pan blocks at render time,
    /// but avoids a first-callback allocation when block params need scratch.
    pub fn preallocate_render_scratch(&mut self, block_frames: usize) {
        let len = block_frames.saturating_mul(2);
        if self.scratch.len() != len {
            self.scratch.resize(len, 0.0);
        }
    }

    /// Create a SamplerBlock from a bare `.signalpack` with default params.
    /// Convenience for callers that don't have a `.signalblock` file.
    pub fn from_pack(pack_path: &Path, sample_rate: u32) -> Result<Self, SamplerError> {
        let name = pack_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("block")
            .to_string();
        Self::build(name, pack_path, BlockParams::default(), sample_rate)
    }

    fn build(
        name: String,
        pack_path: &Path,
        params: BlockParams,
        sample_rate: u32,
    ) -> Result<Self, SamplerError> {
        let patch = PlayerPatch::from_pack(pack_path)?;
        let section = patch
            .spec
            .sections
            .first()
            .map(|s| s.id.clone())
            .unwrap_or_default();
        let mic = patch
            .spec
            .mics
            .first()
            .map(|m| m.id.clone())
            .unwrap_or_default();
        let engine = SampleEngine::new(patch, sample_rate, section, mic);
        let gain_lin = db_to_lin(params.gain_db);
        let (pan_l, pan_r) = pan_gains(params.pan);
        Ok(Self {
            name,
            engine,
            params,
            gain_lin,
            pan_l,
            pan_r,
            scratch: Vec::new(),
            resize_events: 0,
        })
    }

    // ── MIDI / playback ────────────────────────────────────────────────────

    /// Note-on with a per-trigger articulation override (percussion routing).
    /// See [`SampleEngine::note_on_articulated`].
    pub fn note_on_articulated(&mut self, note: u8, velocity: u8, articulation: Option<&str>) {
        let n = transposed(note, self.params.transpose);
        self.engine.note_on_articulated(n, velocity, articulation);
    }

    pub fn note_on(&mut self, note: u8, velocity: u8) {
        let n = transposed(note, self.params.transpose);
        self.engine.note_on(n, velocity);
    }

    pub fn warm_note_samples(&self, note: u8, velocity: u8) -> crate::engine::cache::PreloadStats {
        let n = transposed(note, self.params.transpose);
        self.engine.warm_note_samples(n, velocity)
    }

    pub fn note_off(&mut self, note: u8) {
        let n = transposed(note, self.params.transpose);
        self.engine.note_off(n);
    }

    /// Document-mode legato prefire — see [`SampleEngine::legato_prefire`].
    pub fn legato_prefire(&mut self, note: u8, velocity: u8) {
        let n = transposed(note, self.params.transpose);
        self.engine.legato_prefire(n, velocity);
    }

    // ── Line-addressed dispatch (document scheduler / divisi allocators) ────

    /// Line-addressed note-on — see [`SampleEngine::note_on_line`].
    pub fn note_on_line(&mut self, line: crate::engine::LineId, note: u8, velocity: u8) {
        let n = transposed(note, self.params.transpose);
        self.engine.note_on_line(line, n, velocity);
    }

    /// Line-addressed note-off — see [`SampleEngine::note_off_line`].
    pub fn note_off_line(&mut self, line: crate::engine::LineId, note: u8) {
        let n = transposed(note, self.params.transpose);
        self.engine.note_off_line(line, n);
    }

    /// Line-addressed CC — see [`SampleEngine::cc_line`].
    pub fn cc_line(&mut self, line: crate::engine::LineId, controller: u8, value: u8) {
        self.engine.cc_line(line, controller, value);
    }

    /// Line-addressed legato prefire — see [`SampleEngine::legato_prefire_line`].
    pub fn legato_prefire_line(&mut self, line: crate::engine::LineId, note: u8, velocity: u8) {
        let n = transposed(note, self.params.transpose);
        self.engine.legato_prefire_line(line, n, velocity);
    }

    /// Line-addressed legato prefire with the schedule's lead — see
    /// [`SampleEngine::legato_prefire_line_lead`].
    pub fn legato_prefire_line_lead(
        &mut self,
        line: crate::engine::LineId,
        note: u8,
        velocity: u8,
        lead: u64,
    ) {
        let n = transposed(note, self.params.transpose);
        self.engine
            .legato_prefire_line_lead(line, n, velocity, Some(lead));
    }

    /// Reactive legato-path trigger count — see
    /// [`SampleEngine::reactive_legato_fires`].
    pub fn reactive_legato_fires(&self) -> u64 {
        self.engine.reactive_legato_fires()
    }

    /// Render into a flat (bus × mic) scratch matrix routed by articulation
    /// class — see [`SampleEngine::render_matrix`]. Like
    /// [`render_multi`](Self::render_multi), block gain/pan are NOT applied
    /// here (they live at the Layer level in the multi-mic runtime).
    pub fn render_matrix(
        &mut self,
        outputs: &mut [Vec<f32>],
        nmics: usize,
        route_longs: usize,
        route_shorts: usize,
    ) {
        self.engine
            .render_matrix(outputs, nmics, route_longs, route_shorts);
    }

    /// Explicitly set the legato mode — see [`SampleEngine::set_legato_mode`].
    pub fn set_legato_mode(&mut self, enabled: bool, expressive: bool) {
        self.engine.set_legato_mode(enabled, expressive);
    }

    /// Explicitly set the play-mode policy — see [`SampleEngine::set_play_mode`].
    pub fn set_play_mode(&mut self, mode: crate::engine::PlayMode) {
        self.engine.set_play_mode(mode);
    }

    /// Current play-mode policy — see [`SampleEngine::play_mode`].
    pub fn play_mode(&self) -> crate::engine::PlayMode {
        self.engine.play_mode()
    }

    /// Enable/disable the legato transition fire log —
    /// see [`SampleEngine::set_legato_fire_log_enabled`].
    pub fn set_legato_fire_log_enabled(&mut self, enabled: bool) {
        self.engine.set_legato_fire_log_enabled(enabled);
    }

    /// Recorded legato transition firings —
    /// see [`SampleEngine::legato_fire_log`].
    pub fn legato_fire_log(&self) -> &[crate::engine::LegatoFireEvent] {
        self.engine.legato_fire_log()
    }

    /// Enable/disable the structured render trace —
    /// see [`SampleEngine::set_trace_enabled`].
    pub fn set_trace_enabled(&mut self, enabled: bool) {
        self.engine.set_trace_enabled(enabled);
    }

    /// The structured render trace — see [`SampleEngine::render_trace`].
    pub fn render_trace(&self) -> &crate::engine::RenderTrace {
        self.engine.render_trace()
    }

    /// Running engine render position in frames —
    /// see [`SampleEngine::frames_rendered`].
    pub fn frames_rendered(&self) -> u64 {
        self.engine.frames_rendered()
    }

    pub fn note_off_with_velocity(&mut self, note: u8, velocity: u8) {
        let n = transposed(note, self.params.transpose);
        self.engine.note_off_with_velocity(n, velocity);
    }

    pub fn all_notes_off(&mut self) {
        self.engine.all_notes_off();
    }

    pub fn panic(&mut self) {
        self.engine.panic();
    }

    pub fn cc(&mut self, controller: u8, value: u8) {
        self.engine.cc(controller, value);
    }

    pub fn channel_aftertouch(&mut self, value: u8) {
        self.engine.channel_aftertouch(value);
    }

    pub fn poly_aftertouch(&mut self, note: u8, value: u8) {
        let n = transposed(note, self.params.transpose);
        self.engine.poly_aftertouch(n, value);
    }

    /// Mic ids exposed by the underlying engine, in declaration order.
    /// Multi-output renderers index per-mic buffers against this list.
    pub fn mics(&self) -> &[String] {
        self.engine.mic_ids()
    }

    /// Render the block's voices into N per-mic stereo buffers.
    /// `outputs.len()` should equal `self.mics().len().max(1)`. The block's
    /// `gain_db`/`pan` are NOT applied here — those live at the Layer level
    /// in the multi-mic runtime. No allocation.
    pub fn render_multi(&mut self, outputs: &mut [Vec<f32>]) {
        self.engine.render_multi(outputs);
    }

    /// Mix the block's audio into `output` (interleaved stereo).
    ///
    /// If gain/pan are at defaults the engine renders straight into
    /// `output` (zero-overhead path). Otherwise we render to a scratch
    /// buffer and mix in with gain + pan applied.
    pub fn render(&mut self, output: &mut [f32]) {
        if self.gain_lin == 1.0 && self.params.pan == 0.0 {
            self.engine.render(output);
            return;
        }

        // Render the engine's contribution to scratch (cleared first), then
        // accumulate with per-channel gain into `output`.
        if self.scratch.len() != output.len() {
            self.scratch.resize(output.len(), 0.0);
            self.resize_events = self.resize_events.saturating_add(1);
            tracing::warn!(
                block = %self.name,
                frames = output.len() / 2,
                "SamplerBlock: resized render scratch"
            );
        }
        for s in self.scratch.iter_mut() {
            *s = 0.0;
        }
        self.engine.render(&mut self.scratch);
        for (out_pair, in_pair) in output.chunks_exact_mut(2).zip(self.scratch.chunks_exact(2)) {
            out_pair[0] += in_pair[0] * self.pan_l;
            out_pair[1] += in_pair[1] * self.pan_r;
        }
    }

    // ── Params ─────────────────────────────────────────────────────────────

    pub fn params(&self) -> &BlockParams {
        &self.params
    }

    pub fn set_gain_db(&mut self, db: f32) {
        self.params.gain_db = db;
        self.gain_lin = db_to_lin(db);
        let (l, r) = pan_gains(self.params.pan);
        self.pan_l = l * self.gain_lin;
        self.pan_r = r * self.gain_lin;
    }

    pub fn set_pan(&mut self, pan: f32) {
        self.params.pan = pan.clamp(-1.0, 1.0);
        let (l, r) = pan_gains(self.params.pan);
        self.pan_l = l * self.gain_lin;
        self.pan_r = r * self.gain_lin;
    }

    pub fn set_transpose(&mut self, semitones: i8) {
        self.params.transpose = semitones;
    }

    pub fn set_tune_cents(&mut self, cents: i16) {
        self.params.tune_cents = cents;
    }

    /// Pin this block's engine to a single articulation (percussion kits) —
    /// only zones with the matching `articulation` fire, on any routed note.
    /// `None`/empty clears the pin. See [`SampleEngine::pin_articulation`].
    pub fn pin_articulation(&mut self, artic: Option<String>) {
        self.engine.pin_articulation(artic);
    }

    /// Select this block's live articulation (keyswitch / CC58 equivalent).
    /// See [`SampleEngine::set_articulation`].
    pub fn set_articulation(&mut self, artic: impl Into<String>) {
        self.engine.set_articulation(artic);
    }

    /// The block's current live articulation (reflects keyswitch / CC58).
    pub fn articulation(&self) -> &str {
        self.engine.articulation()
    }

    /// Sustain attack envelope in milliseconds (CSS attack parameter).
    pub fn set_attack_ms(&mut self, ms: u32) {
        let sr = self.engine.sample_rate();
        self.engine
            .set_attack_frames(crate::engine::ms_to_frames(ms, sr));
    }

    /// Sustain release fade in milliseconds (CSS release parameter).
    pub fn set_release_ms(&mut self, ms: u32) {
        let sr = self.engine.sample_rate();
        self.engine
            .set_release_frames(crate::engine::ms_to_frames(ms, sr));
    }

    /// Test/render override: pin RR-bearing triggers to a specific round-robin
    /// slot (`None` restores normal CC59 / cycle / random). See
    /// [`SampleEngine::set_forced_rr`].
    pub fn set_forced_rr(&mut self, slot: Option<u32>) {
        self.engine.set_forced_rr(slot);
    }

    /// Switch this block's engine to a microphone position (e.g. `"Mix"`).
    /// See [`SampleEngine::set_mic`].
    pub fn set_mic(&mut self, mic_id: impl Into<String>) {
        self.engine.set_mic(mic_id);
    }

    /// Restrict zoned playback to a single mic. See
    /// [`SampleEngine::set_solo_mic`].
    pub fn set_solo_mic(&mut self, mic_id: Option<String>) {
        self.engine.set_solo_mic(mic_id);
    }

    /// Warm the samples a note would trigger under the current pin + solo mic.
    /// See [`SampleEngine::warm_note`].
    pub fn warm_note(&self, note: u8) -> crate::engine::cache::PreloadStats {
        self.engine.warm_note(note)
    }

    /// Set an engine-wide choke group + the articulations that trigger it.
    /// See [`SampleEngine::set_choke_group`].
    pub fn set_choke_group(&mut self, group: Option<&str>, choke_on: &[String]) {
        self.engine.set_choke_group(group, choke_on);
    }

    pub fn apply_voice_config(&mut self, config: &VoiceConfig) {
        self.engine.set_voice_config(config);
    }

    /// Apply a single [`ParamOverride`]. Recognized parameter ids:
    ///
    /// - `gain_db` / `gain` — block gain in dB (typical -60 … +12)
    /// - `pan` — stereo pan in [-1.0, 1.0]
    /// - `transpose` — semitone offset (rounded to nearest i8)
    /// - `tune_cents` — fine tune in cents (rounded to nearest i16)
    ///
    /// Unknown ids are logged and skipped — forward-compat lets a future
    /// block type add parameters without breaking older callers.
    pub fn apply_override(&mut self, ov: &ParamOverride) {
        match ov.param.as_str() {
            "gain_db" | "gain" => self.set_gain_db(ov.value),
            "pan" => self.set_pan(ov.value),
            "transpose" => self.set_transpose(ov.value.round() as i8),
            "tune_cents" => self.set_tune_cents(ov.value.round() as i16),
            other => {
                tracing::warn!(
                    block = %self.name,
                    param = other,
                    value = ov.value,
                    "SamplerBlock: ignoring unknown parameter override"
                );
            }
        }
    }

    /// Apply a slice of overrides in order. Used by Module loading to
    /// apply per-slot tweaks on top of a freshly built block.
    pub fn apply_overrides(&mut self, overrides: &[ParamOverride]) {
        for ov in overrides {
            self.apply_override(ov);
        }
    }

    // ── Engine pass-throughs ───────────────────────────────────────────────

    pub fn cache_handle(&self) -> SampleCache {
        self.engine.cache_handle()
    }

    pub fn sample_paths_centered(&self, center: u8) -> Vec<PathBuf> {
        self.engine.sample_paths_centered(center)
    }

    pub fn sample_paths_owned(&self) -> Vec<PathBuf> {
        self.engine.sample_paths_owned()
    }

    pub fn loaded_sample_count(&self) -> usize {
        self.engine.loaded_sample_count()
    }

    pub fn loaded_sample_bytes(&self) -> usize {
        self.engine.loaded_sample_bytes()
    }

    pub fn evict_cache_until_under_budget(&self, budget_bytes: usize) -> EvictStats {
        self.engine.evict_cache_until_under_budget(budget_bytes)
    }

    pub fn total_sample_count(&self) -> usize {
        self.engine.total_sample_count()
    }

    pub fn patch(&self) -> &PlayerPatch {
        self.engine.patch()
    }

    pub fn active_voices(&self) -> usize {
        self.engine.active_voices()
    }

    pub fn max_voices(&self) -> usize {
        self.engine.max_voices()
    }

    pub fn stolen_voices(&self) -> usize {
        self.engine.stolen_voices()
    }

    pub fn cache_misses(&self) -> usize {
        self.engine.cache_misses()
    }

    pub fn sample_misses(&self) -> usize {
        self.engine.sample_misses()
    }

    pub fn resize_events(&self) -> u64 {
        self.resize_events
    }

    pub fn recent_cache_misses(&self) -> Vec<String> {
        self.engine.recent_cache_misses()
    }

    pub fn recent_sample_misses(&self) -> Vec<String> {
        self.engine.recent_sample_misses()
    }

    /// Synchronously preload all referenced samples. Used by tests; the
    /// production path is the bank's background coordinator thread.
    pub fn preload_samples(&mut self) -> crate::engine::cache::PreloadStats {
        self.engine.preload_samples()
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn transposed(note: u8, semitones: i8) -> u8 {
    ((note as i16) + (semitones as i16)).clamp(0, 127) as u8
}

fn db_to_lin(db: f32) -> f32 {
    if db == 0.0 {
        1.0
    } else {
        10f32.powf(db / 20.0)
    }
}

/// Equal-power pan: at centre both L and R are 0.7071 (-3 dB).
fn pan_gains(pan: f32) -> (f32, f32) {
    let p = pan.clamp(-1.0, 1.0);
    let theta = (p + 1.0) * std::f32::consts::FRAC_PI_4;
    (theta.cos(), theta.sin())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_no_ops() {
        let p = BlockParams::default();
        assert_eq!(p.gain_db, 0.0);
        assert_eq!(p.pan, 0.0);
        assert_eq!(p.transpose, 0);
        assert_eq!(db_to_lin(0.0), 1.0);
    }

    #[test]
    fn pan_is_equal_power() {
        let (l, r) = pan_gains(0.0);
        assert!((l - r).abs() < 1e-6);
        assert!((l - 0.70710677).abs() < 1e-6);
    }

    #[test]
    fn transpose_clamps() {
        assert_eq!(transposed(60, 12), 72);
        assert_eq!(transposed(60, -12), 48);
        assert_eq!(transposed(120, 12), 127);
        assert_eq!(transposed(5, -10), 0);
    }
}
