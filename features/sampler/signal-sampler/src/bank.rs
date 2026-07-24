//! SamplerBank — holds N named EngineInstances and zero or more
//! Preset runtimes, mixes everything into a stereo master output.
//!
//! Single-engine load paths (`load_pack`, `load_block`,
//! `load_engine_spec`) install one `EngineInstance` under the caller's
//! id. The Engine's first port is summed into master at render time.
//!
//! Preset load (`load_preset_spec`) builds a full `PresetRuntime`,
//! registering each engine under `<prefix>:<engine_id>` for back-compat
//! with note-on calls keyed off those ids — and also installs the
//! preset graph for true multi-bus rendering.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::block::{BlockSpec, ParamOverride, SamplerBlock};
use crate::engine::cache::{EvictStats, PreloadStats, SampleCache};
use crate::engine_spec::{EngineLayerSpec, EngineSpec, PortSpec};
use crate::runtime::{EngineInstance, PresetRuntime};

use crate::InstrumentId;
use std::path::PathBuf;

/// Default block-frame size used to pre-allocate engine/module scratches
/// when the bank is created. Real-world callbacks may use a different
/// size; the runtime resizes on the FIRST render in that case (one-time
/// allocation, then stable).
const DEFAULT_BLOCK_FRAMES: usize = 4096;
const FAST_AUDITION_PRELOAD_SAMPLES: usize = 64;
const PERFORMANCE_PRELOAD_SAMPLES: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(Default)]
pub enum PreloadProfile {
    FastAudition,
    #[default]
    Performance,
    Full,
    DrumKit,
    PianoCenterOut,
    OrchestralArticulation,
}


impl PreloadProfile {
    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "fast-audition" | "fast_audition" | "audition" | "fast" => Some(Self::FastAudition),
            "performance" | "perf" => Some(Self::Performance),
            "full" | "full-preload" | "full_preload" => Some(Self::Full),
            "drum-kit" | "drum_kit" | "drum" | "drums" => Some(Self::DrumKit),
            "piano-center-out" | "piano_center_out" | "piano" => Some(Self::PianoCenterOut),
            "orchestral-articulation" | "orchestral_articulation" | "orchestral" => {
                Some(Self::OrchestralArticulation)
            }
            _ => None,
        }
    }

    /// How many samples this profile preloads eagerly (`None` = all).
    pub fn preload_cap(self) -> Option<usize> {
        match self {
            Self::FastAudition => Some(FAST_AUDITION_PRELOAD_SAMPLES),
            Self::Performance => Some(PERFORMANCE_PRELOAD_SAMPLES),
            _ => None,
        }
    }

    fn ordered_paths(self, block: &SamplerBlock) -> Vec<PathBuf> {
        let mut paths = match self {
            Self::Full => block.sample_paths_owned(),
            _ => block.sample_paths_centered(60),
        };
        let limit = match self {
            Self::FastAudition => Some(FAST_AUDITION_PRELOAD_SAMPLES),
            Self::Performance => Some(PERFORMANCE_PRELOAD_SAMPLES),
            _ => None,
        };
        if let Some(limit) = limit {
            paths.truncate(limit);
        }
        paths
    }

    fn engine_priority(self, engine_type: &str) -> u8 {
        match self {
            Self::DrumKit => drum_priority(engine_type),
            Self::PianoCenterOut => {
                if engine_type.contains("piano") || engine_type.contains("keys") {
                    0
                } else {
                    8
                }
            }
            Self::OrchestralArticulation => orchestral_priority(engine_type),
            _ => 0,
        }
    }
}

/// Pending preload work attached to a freshly-registered engine. Returned
/// by `register_pack` so callers can either spawn a per-pack preload
/// thread (single load) or pool the work into a coordinator (engine
/// presets that span multiple packs).
struct PendingPreload {
    cache: SampleCache,
    paths: Vec<PathBuf>,
}

fn resolve_relative(path_str: &str, base_dir: &Path) -> PathBuf {
    let p = PathBuf::from(path_str);
    if p.is_absolute() { p } else { base_dir.join(p) }
}

/// Map an instrument tag to a drum-kit preload priority. Lower values are
/// loaded first.
pub fn drum_priority(instrument: &str) -> u8 {
    match instrument {
        "kick" => 0,
        "snare" => 1,
        "hi-hat" => 2,
        "ride" => 3,
        "tom" => 4,
        "crash" => 5,
        "splash" => 6,
        "china" => 7,
        "stack" => 8,
        "cymbal" => 9,
        "effects" => 10,
        _ => 11,
    }
}

fn orchestral_priority(instrument: &str) -> u8 {
    let lower = instrument.to_ascii_lowercase();
    if lower.contains("solo") || lower.contains("lead") {
        0
    } else if lower.contains("violin") || lower.contains("1v") {
        1
    } else if lower.contains("viola") || lower.contains("cello") {
        2
    } else if lower.contains("bass") {
        3
    } else if lower.contains("brass") || lower.contains("horn") || lower.contains("trombone") {
        4
    } else if lower.contains("wind") || lower.contains("flute") || lower.contains("clarinet") {
        5
    } else {
        8
    }
}

/// One free-standing instrument (not part of a preset). Each holds
/// a complete `EngineInstance` so multi-mic layer routing applies even
/// for single-pack loads (in the simplest case there's one default
/// layer subscribed to mic 0).
pub struct InstrumentSlot {
    pub engine: EngineInstance,
    pub muted: bool,
    preload_target: Option<usize>,
}

/// Holds multiple instruments + preset runtimes and mixes them.
pub struct SamplerBank {
    /// Free-standing instruments registered under string ids
    /// (load_pack / load_block / load_engine_spec).
    instruments: HashMap<InstrumentId, InstrumentSlot>,
    /// Loaded presets keyed by id_prefix.
    presets: HashMap<String, PresetRuntime>,
    /// MIDI channel → instrument ID routing (channel 1–16, 1-based index).
    midi_channels: HashMap<u8, InstrumentId>,
    /// Per-note routing for free-standing instruments. Drum-kit presets
    /// route through their `PresetRuntime.note_routing` instead. Kept as
    /// a single-instrument fallback so direct `note_on` still works.
    note_routing: HashMap<u8, InstrumentId>,
    /// Maps every instrument id (free-standing OR `<prefix>:<engine>`)
    /// to the preset prefix that owns it, when applicable. Lets
    /// note_on/note_off route to the right engine inside the preset.
    instrument_to_preset: HashMap<InstrumentId, (String, String)>, // id -> (prefix, engine_id)
    /// Instrument that unrouted MIDI falls back to — the most recently loaded
    /// free-standing instrument. Lets a single-instrument live rig play without
    /// an explicit `set_midi_channel` mapping (a keyboard on any channel reaches
    /// the loaded instrument).
    default_instrument: Option<InstrumentId>,
    sample_rate: u32,
    block_frames: usize,
    preload_generation: Arc<AtomicU64>,
    cache_budget_bytes: Option<usize>,
    preload_profile: PreloadProfile,
}

impl SamplerBank {
    pub fn new(sample_rate: u32) -> Self {
        Self::with_cache_budget(sample_rate, None)
    }

    pub fn with_cache_budget(sample_rate: u32, cache_budget_bytes: Option<usize>) -> Self {
        Self {
            instruments: HashMap::new(),
            presets: HashMap::new(),
            midi_channels: HashMap::new(),
            note_routing: HashMap::new(),
            instrument_to_preset: HashMap::new(),
            default_instrument: None,
            sample_rate,
            block_frames: DEFAULT_BLOCK_FRAMES,
            preload_generation: Arc::new(AtomicU64::new(0)),
            cache_budget_bytes,
            // `FTS_PRELOAD_PROFILE` overrides (e.g. "fast-audition" on
            // phones, where a Performance-sized piano preload is GBs of
            // decoded PCM); unset/unknown = the normal default.
            preload_profile: std::env::var("FTS_PRELOAD_PROFILE")
                .ok()
                .and_then(|s| PreloadProfile::from_name(&s))
                .unwrap_or_default(),
        }
    }

    pub fn cache_handle_for(&self, id: &str) -> Option<crate::engine::cache::SampleCache> {
        self.instruments
            .get(id)
            .map(|s| s.engine.block.cache_handle())
    }

    pub fn set_preload_profile(&mut self, profile: PreloadProfile) {
        self.preload_profile = profile;
    }

    fn next_preload_generation(&self) -> u64 {
        self.preload_generation.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn preload_cancel_token(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.preload_generation)
    }

    /// Build a synthetic single-Layer / single-Port EngineSpec for a
    /// free-standing block load.
    fn synth_default_engine_spec(name: &str) -> EngineSpec {
        EngineSpec {
            name: name.to_string(),
            description: String::new(),
            engine_type: String::new(),
            block: crate::engine_spec::BlockRef {
                pack: String::new(),
                overrides: Vec::new(),
            },
            layers: vec![EngineLayerSpec {
                id: "main".into(),
                mic: String::new(),
                gain_db: 0.0,
                pan: 0.0,
                bypass: false,
                fx_chain: Vec::new(),
            }],
            ports: vec![PortSpec {
                id: "main".into(),
                from: "main".into(),
            }],
            voice: Default::default(),
        }
    }

    /// Load a sample library from `spec_path` + optional `samples_root` WAV directory.
    pub fn load_instrument(
        &mut self,
        id: impl Into<InstrumentId>,
        spec_path: &Path,
        samples_root: Option<&Path>,
        section: impl Into<String>,
        mic: impl Into<String>,
    ) -> eyre::Result<()> {
        let id = id.into();
        let patch = match samples_root {
            Some(root) => crate::PlayerPatch::load(spec_path, root)?,
            None => {
                let spec = crate::LibrarySpec::from_file(spec_path)?;
                crate::PlayerPatch::from_spec(spec)
            }
        };
        let name = spec_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("block")
            .to_string();
        self.install_patch(id, patch, name, section, mic);
        Ok(())
    }

    /// Load an instrument whose samples come from `zones_path` but whose engine
    /// config (articulations / keyswitch / CC58 / legato / dynamics) comes from
    /// a separate descriptive spec — the way Cinematic Studio libraries ship.
    /// Without this, the zone spec alone has no articulation metadata, so
    /// articulation + keyswitch switching can't work. See
    /// [`PlayerPatch::load_merged`](crate::PlayerPatch::load_merged).
    pub fn load_instrument_with_config(
        &mut self,
        id: impl Into<InstrumentId>,
        config_path: &Path,
        zones_path: &Path,
        samples_root: &Path,
        section: impl Into<String>,
        mic: impl Into<String>,
    ) -> eyre::Result<()> {
        let id = id.into();
        let patch = crate::PlayerPatch::load_merged(config_path, zones_path, samples_root)?;
        let name = zones_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("block")
            .to_string();
        self.install_patch(id, patch, name, section, mic);
        Ok(())
    }

    /// Wrap a loaded [`PlayerPatch`](crate::PlayerPatch) into a bank instrument
    /// slot under `id`.
    fn install_patch(
        &mut self,
        id: InstrumentId,
        patch: crate::PlayerPatch,
        name: String,
        section: impl Into<String>,
        mic: impl Into<String>,
    ) {
        let engine = crate::SampleEngine::new(patch, self.sample_rate, section, mic);
        let block = SamplerBlock::from_engine(name, engine, crate::block::BlockParams::default());
        let synth_spec = Self::synth_default_engine_spec(&block.name.clone());
        let instance = EngineInstance::new(synth_spec, block, self.block_frames);
        tracing::info!("signal-sampler: loaded instrument {id:?}");
        // Unrouted live MIDI falls back to the most-recently loaded instrument.
        self.default_instrument = Some(id.clone());
        self.instruments.insert(
            id,
            InstrumentSlot {
                engine: instance,
                muted: false,
                preload_target: None,
            },
        );
    }

    /// Route live MIDI on any *unmapped* channel to `id` (a loaded instrument
    /// or preset prefix). This is what makes a single-instrument/single-kit
    /// live rig "react to all MIDI" without per-channel setup — presets don't
    /// set it automatically (only `load_instrument` does), so a drum kit that
    /// should play on any channel must set it explicitly.
    pub fn set_default_instrument(&mut self, id: impl Into<InstrumentId>) {
        self.default_instrument = Some(id.into());
    }

    pub fn load_pack(&mut self, id: impl Into<InstrumentId>, pack_path: &Path) -> eyre::Result<()> {
        let id = id.into();
        let cache = self.register_pack(id.clone(), pack_path)?;
        let pack_label = pack_path.display().to_string();
        let generation = self.next_preload_generation();
        let cancel = self.preload_cancel_token();
        if let Err(err) = std::thread::Builder::new()
            .name(format!("signal-preload:{}", pack_label))
            .spawn(move || {
                let start = std::time::Instant::now();
                tracing::info!(
                    pack = %pack_label,
                    paths = cache.paths.len(),
                    "background preload starting",
                );
                let stats = cache
                    .cache
                    .preload_cancelable(cache.paths.iter().map(|p| p.as_path()), || {
                        cancel.load(Ordering::Relaxed) != generation
                    });
                tracing::info!(
                    pack = %pack_label,
                    loaded = stats.loaded,
                    failed = stats.failed,
                    bytes = stats.bytes,
                    elapsed_ms = start.elapsed().as_millis() as u64,
                    "background preload complete",
                );
            })
        {
            tracing::warn!(err = %err, "failed to spawn signal-preload thread");
        }
        tracing::info!(
            id = ?id,
            pack = %pack_path.display(),
            "signal-sampler: loaded pack (preload streaming in background)",
        );
        Ok(())
    }

    fn register_pack(
        &mut self,
        id: InstrumentId,
        pack_path: &Path,
    ) -> eyre::Result<PendingPreload> {
        let block = SamplerBlock::from_pack(pack_path, self.sample_rate)?;
        let cache = block.cache_handle();
        let paths = self.preload_profile.ordered_paths(&block);
        let synth_spec = Self::synth_default_engine_spec(&block.name.clone());
        let instance = EngineInstance::new(synth_spec, block, self.block_frames);
        self.instruments.insert(
            id,
            InstrumentSlot {
                engine: instance,
                muted: false,
                preload_target: Some(paths.len()),
            },
        );
        Ok(PendingPreload { cache, paths })
    }

    pub fn load_block(
        &mut self,
        id: impl Into<InstrumentId>,
        block_path: &Path,
    ) -> eyre::Result<()> {
        let id = id.into();
        let pending = self.register_block(id.clone(), block_path)?;
        let label = block_path.display().to_string();
        let generation = self.next_preload_generation();
        let cancel = self.preload_cancel_token();
        if let Err(err) = std::thread::Builder::new()
            .name(format!("signal-preload:{}", label))
            .spawn(move || {
                let stats = pending
                    .cache
                    .preload_cancelable(pending.paths.iter().map(|p| p.as_path()), || {
                        cancel.load(Ordering::Relaxed) != generation
                    });
                tracing::info!(
                    block = %label,
                    loaded = stats.loaded,
                    failed = stats.failed,
                    "block preload complete",
                );
            })
        {
            tracing::warn!(err = %err, "failed to spawn signal-preload thread");
        }
        tracing::info!(id = ?id, block = %block_path.display(), "loaded block");
        Ok(())
    }

    pub fn unload_instrument(&mut self, id: &str) {
        self.next_preload_generation();
        self.all_notes_off(id);
        self.instruments.remove(id);
        self.midi_channels.retain(|_, v| v != id);
        self.instrument_to_preset.remove(id);

        if self.presets.remove(id).is_some() {
            let prefix = format!("{id}:");
            self.instrument_to_preset
                .retain(|instrument_id, (preset_prefix, _)| {
                    preset_prefix != id && !instrument_id.starts_with(&prefix)
                });
        }
    }

    pub fn all_notes_off(&mut self, id: &str) {
        if let Some(slot) = self.instruments.get_mut(id) {
            slot.engine.all_notes_off();
            return;
        }
        if let Some(preset) = self.presets.get_mut(id) {
            preset.all_notes_off();
            return;
        }
        if let Some((prefix, engine_id)) = self.instrument_to_preset.get(id).cloned() {
            if let Some(preset) = self.presets.get_mut(&prefix) {
                if let Some(&idx) = preset.engine_id_to_idx.get(&engine_id) {
                    preset.engines[idx].all_notes_off();
                }
            }
        }
    }

    pub fn panic(&mut self, id: &str) {
        if let Some(slot) = self.instruments.get_mut(id) {
            slot.engine.panic();
            return;
        }
        if let Some(preset) = self.presets.get_mut(id) {
            preset.panic();
            return;
        }
        if let Some((prefix, engine_id)) = self.instrument_to_preset.get(id).cloned() {
            if let Some(preset) = self.presets.get_mut(&prefix) {
                if let Some(&idx) = preset.engine_id_to_idx.get(&engine_id) {
                    preset.engines[idx].panic();
                }
            }
        }
    }

    pub fn set_midi_channel(&mut self, id: impl Into<InstrumentId>, channel: u8) {
        self.midi_channels.insert(channel, id.into());
    }

    pub fn set_muted(&mut self, id: &str, muted: bool) {
        if let Some(slot) = self.instruments.get_mut(id) {
            slot.muted = muted;
            slot.engine.muted = muted;
            return;
        }
        // Try as preset:engine.
        if let Some((prefix, engine_id)) = self.instrument_to_preset.get(id).cloned() {
            if let Some(preset) = self.presets.get_mut(&prefix) {
                if let Some(&idx) = preset.engine_id_to_idx.get(&engine_id) {
                    preset.engines[idx].muted = muted;
                }
            }
        }
    }

    /// Reach an instrument's [`SamplerBlock`](crate::block::SamplerBlock) for a
    /// live config tweak (articulation / mic), resolving both the direct-slot
    /// and the `preset:engine` cases. No-op if `id` isn't loaded.
    fn with_block(&mut self, id: &str, f: impl FnOnce(&mut crate::block::SamplerBlock)) {
        if let Some(slot) = self.instruments.get_mut(id) {
            f(&mut slot.engine.block);
            return;
        }
        if let Some((prefix, engine_id)) = self.instrument_to_preset.get(id).cloned() {
            if let Some(preset) = self.presets.get_mut(&prefix) {
                if let Some(&idx) = preset.engine_id_to_idx.get(&engine_id) {
                    f(&mut preset.engines[idx].block);
                }
            }
        }
    }

    /// Pin an instrument to a single articulation (e.g. `"Leg"`); `None` clears.
    pub fn pin_articulation(&mut self, id: &str, artic: Option<String>) {
        self.with_block(id, |b| b.pin_articulation(artic));
    }

    /// Select an instrument's live articulation (keyswitch / CC58 equivalent).
    pub fn set_articulation(&mut self, id: &str, artic: impl Into<String>) {
        let artic = artic.into();
        self.with_block(id, |b| b.set_articulation(artic));
    }

    /// An instrument's current live articulation (reflects keyswitch / CC58).
    pub fn articulation(&self, id: &str) -> Option<String> {
        if let Some(slot) = self.instruments.get(id) {
            return Some(slot.engine.block.articulation().to_string());
        }
        if let Some((prefix, engine_id)) = self.instrument_to_preset.get(id) {
            if let Some(preset) = self.presets.get(prefix) {
                if let Some(&idx) = preset.engine_id_to_idx.get(engine_id) {
                    return Some(preset.engines[idx].block.articulation().to_string());
                }
            }
        }
        None
    }

    /// Switch an instrument's active microphone position (e.g. `"Mix"`).
    pub fn set_mic(&mut self, id: &str, mic_id: impl Into<String>) {
        let mic = mic_id.into();
        self.with_block(id, |b| b.set_mic(mic));
    }

    /// Restrict an instrument's zoned playback to a single mic; `None` plays all.
    pub fn set_solo_mic(&mut self, id: &str, mic_id: Option<String>) {
        self.with_block(id, |b| b.set_solo_mic(mic_id));
    }

    /// Sustain attack envelope (ms) for an instrument (CSS attack parameter).
    pub fn set_attack_ms(&mut self, id: &str, ms: u32) {
        self.with_block(id, |b| b.set_attack_ms(ms));
    }

    /// Sustain release fade (ms) for an instrument (CSS release parameter).
    pub fn set_release_ms(&mut self, id: &str, ms: u32) {
        self.with_block(id, |b| b.set_release_ms(ms));
    }

    /// Pin an instrument's RR-bearing triggers to a specific round-robin slot;
    /// `None` restores normal CC59 / cycle / random behaviour (A/B null harness).
    pub fn set_forced_rr(&mut self, id: &str, slot: Option<u32>) {
        self.with_block(id, |b| b.set_forced_rr(slot));
    }

    /// Read an instrument's block for inspection (spec, fire log, …),
    /// resolving both the direct-slot and the `preset:engine` cases.
    fn read_block<T>(
        &self,
        id: &str,
        f: impl FnOnce(&crate::block::SamplerBlock) -> T,
    ) -> Option<T> {
        if let Some(slot) = self.instruments.get(id) {
            return Some(f(&slot.engine.block));
        }
        if let Some((prefix, engine_id)) = self.instrument_to_preset.get(id) {
            if let Some(preset) = self.presets.get(prefix) {
                if let Some(&idx) = preset.engine_id_to_idx.get(engine_id) {
                    return Some(f(&preset.engines[idx].block));
                }
            }
        }
        None
    }

    /// Clone an instrument's loaded [`LibrarySpec`](crate::spec::LibrarySpec)
    /// (document annotation reads keyswitch / legato / short-note tables).
    pub fn instrument_spec(&self, id: &str) -> Option<crate::spec::LibrarySpec> {
        self.read_block(id, |b| b.patch().spec.clone())
    }

    /// Document-mode legato prefire — see
    /// [`SampleEngine::legato_prefire`](crate::engine::SampleEngine::legato_prefire).
    pub fn legato_prefire(&mut self, id: &str, note: u8, velocity: u8) {
        self.with_block(id, |b| b.legato_prefire(note, velocity));
    }

    /// Line-addressed legato prefire — see
    /// [`SampleEngine::legato_prefire_line`](crate::engine::SampleEngine::legato_prefire_line).
    pub fn legato_prefire_line(
        &mut self,
        id: &str,
        line: crate::engine::LineId,
        note: u8,
        velocity: u8,
    ) {
        self.with_block(id, |b| b.legato_prefire_line(line, note, velocity));
    }

    /// Line-addressed legato prefire carrying the schedule's lead (frames to
    /// the destination tick) — see
    /// [`SampleEngine::legato_prefire_line_lead`](crate::engine::SampleEngine::legato_prefire_line_lead).
    pub fn legato_prefire_line_lead(
        &mut self,
        id: &str,
        line: crate::engine::LineId,
        note: u8,
        velocity: u8,
        lead: u64,
    ) {
        self.with_block(id, |b| {
            b.legato_prefire_line_lead(line, note, velocity, lead)
        });
    }

    /// Per-instrument note-on, addressed by id (bypasses MIDI-channel
    /// routing — used by the document scheduler).
    pub fn note_on_instrument(&mut self, id: &str, note: u8, velocity: u8) {
        self.with_block(id, |b| b.note_on(note, velocity));
    }

    /// Line-addressed per-instrument note-on (document scheduler).
    pub fn note_on_instrument_line(
        &mut self,
        id: &str,
        line: crate::engine::LineId,
        note: u8,
        velocity: u8,
    ) {
        self.with_block(id, |b| b.note_on_line(line, note, velocity));
    }

    /// [`note_on_instrument_line`](Self::note_on_instrument_line) carrying
    /// the document scheduler's pre-roll lead (wall frames to the note's
    /// grid tick) for per-zone arrival alignment — see
    /// [`SampleEngine::note_on_line_lead`](crate::engine::SampleEngine::note_on_line_lead).
    pub fn note_on_instrument_line_lead(
        &mut self,
        id: &str,
        line: crate::engine::LineId,
        note: u8,
        velocity: u8,
        lead: u64,
    ) {
        self.with_block(id, |b| b.note_on_line_lead(line, note, velocity, lead));
    }

    /// Per-instrument note-off, addressed by id.
    pub fn note_off_instrument(&mut self, id: &str, note: u8) {
        self.with_block(id, |b| b.note_off(note));
    }

    /// Line-addressed per-instrument note-off (document scheduler).
    pub fn note_off_instrument_line(&mut self, id: &str, line: crate::engine::LineId, note: u8) {
        self.with_block(id, |b| b.note_off_line(line, note));
    }

    /// Per-instrument CC, addressed by id.
    pub fn cc_instrument(&mut self, id: &str, controller: u8, value: u8) {
        self.with_block(id, |b| b.cc(controller, value));
    }

    /// Line-addressed per-instrument CC (document scheduler — CC1/CC2 are
    /// per-line dynamics, other controllers engine-global).
    pub fn cc_instrument_line(
        &mut self,
        id: &str,
        line: crate::engine::LineId,
        controller: u8,
        value: u8,
    ) {
        self.with_block(id, |b| b.cc_line(line, controller, value));
    }

    /// Reactive legato-path trigger count for an instrument — see
    /// [`SampleEngine::reactive_legato_fires`](crate::engine::SampleEngine::reactive_legato_fires).
    pub fn reactive_legato_fires(&self, id: &str) -> u64 {
        self.read_block(id, |b| b.reactive_legato_fires())
            .unwrap_or(0)
    }

    /// Render ONE instrument into per-bus buffers routed by articulation
    /// class (stem-aware document rendering) — see
    /// [`EngineInstance::render_routed_buses`](crate::runtime::EngineInstance::render_routed_buses).
    /// Mirrors [`render`](Self::render)'s per-instrument math exactly, so
    /// with a single loaded instrument and every class routed to one bus the
    /// output is bit-identical to `render`. `outputs` must arrive zeroed.
    /// Returns false if `id` isn't loaded.
    pub fn render_instrument_routed_buses(
        &mut self,
        id: &str,
        outputs: &mut [Vec<f32>],
        route_longs: usize,
        route_shorts: usize,
    ) -> bool {
        let block_frames = outputs.first().map(|b| b.len() / 2).unwrap_or(0);
        if let Some(slot) = self.instruments.get_mut(id) {
            if !slot.muted {
                slot.engine
                    .render_routed_buses(outputs, route_longs, route_shorts, block_frames);
            }
            return true;
        }
        if let Some((prefix, engine_id)) = self.instrument_to_preset.get(id).cloned() {
            if let Some(preset) = self.presets.get_mut(&prefix) {
                if let Some(&idx) = preset.engine_id_to_idx.get(&engine_id) {
                    preset.engines[idx].render_routed_buses(
                        outputs,
                        route_longs,
                        route_shorts,
                        block_frames,
                    );
                    return true;
                }
            }
        }
        false
    }

    /// Explicitly set an instrument's legato mode (document mode forces
    /// expressive).
    pub fn set_legato_mode(&mut self, id: &str, enabled: bool, expressive: bool) {
        self.with_block(id, |b| b.set_legato_mode(enabled, expressive));
    }

    /// Explicitly set an instrument's play-mode policy — see
    /// [`PlayMode`](crate::engine::PlayMode).
    pub fn set_play_mode(&mut self, id: &str, mode: crate::engine::PlayMode) {
        self.with_block(id, |b| b.set_play_mode(mode));
    }

    /// An instrument's current play-mode policy.
    pub fn play_mode(&self, id: &str) -> Option<crate::engine::PlayMode> {
        self.read_block(id, |b| b.play_mode())
    }

    /// Enable/disable an instrument's legato transition fire log.
    pub fn set_legato_fire_log_enabled(&mut self, id: &str, enabled: bool) {
        self.with_block(id, |b| b.set_legato_fire_log_enabled(enabled));
    }

    /// Recorded legato transition firings for an instrument.
    pub fn legato_fire_log(&self, id: &str) -> Vec<crate::engine::LegatoFireEvent> {
        self.read_block(id, |b| b.legato_fire_log().to_vec())
            .unwrap_or_default()
    }

    /// Enable/disable an instrument's playback-emitted marker log.
    pub fn set_emitted_marker_log_enabled(&mut self, id: &str, enabled: bool) {
        self.with_block(id, |b| b.set_emitted_marker_log_enabled(enabled));
    }

    /// Markers emitted BY PLAYBACK for an instrument since the log was
    /// enabled (see [`crate::engine::EmittedMarker`]).
    pub fn emitted_markers(&self, id: &str) -> Vec<crate::engine::EmittedMarker> {
        self.read_block(id, |b| b.emitted_markers().to_vec())
            .unwrap_or_default()
    }

    /// Enable/disable an instrument's structured render trace.
    pub fn set_trace_enabled(&mut self, id: &str, enabled: bool) {
        self.with_block(id, |b| b.set_trace_enabled(enabled));
    }

    pub fn set_solo_notes(&mut self, id: &str, notes: Option<std::collections::BTreeSet<u8>>) {
        self.with_block(id, |b| b.set_solo_notes(notes.clone()));
    }

    pub fn set_pure_playback(&mut self, id: &str, on: bool) {
        self.with_block(id, |b| b.set_pure_playback(on));
    }

    /// The structured render trace for an instrument.
    pub fn render_trace(&self, id: &str) -> crate::engine::RenderTrace {
        self.read_block(id, |b| b.render_trace())
            .unwrap_or_default()
    }

    /// An instrument engine's running render position in frames.
    pub fn engine_frames_rendered(&self, id: &str) -> Option<u64> {
        self.read_block(id, |b| b.frames_rendered())
    }

    /// Warm the samples `note` would trigger for `id` under its current pin +
    /// solo mic (read-only on the cache; safe to call off-thread).
    pub fn warm_note(&self, id: &str, note: u8) -> PreloadStats {
        if let Some(slot) = self.instruments.get(id) {
            return slot.engine.block.warm_note(note);
        }
        if let Some((prefix, engine_id)) = self.instrument_to_preset.get(id) {
            if let Some(preset) = self.presets.get(prefix) {
                if let Some(&idx) = preset.engine_id_to_idx.get(engine_id) {
                    return preset.engines[idx].block.warm_note(note);
                }
            }
        }
        PreloadStats::default()
    }

    /// Mute a Module inside a loaded preset. Returns true if found.
    pub fn set_module_muted(&mut self, preset_prefix: &str, module_id: &str, muted: bool) -> bool {
        self.presets
            .get_mut(preset_prefix)
            .map(|p| p.set_module_muted(module_id, muted))
            .unwrap_or(false)
    }

    /// Read a port buffer from a preset's engine. Used by tests to verify
    /// per-bus separation.
    pub fn preset_engine_port_buffer(
        &self,
        preset_prefix: &str,
        engine_id: &str,
        port_id: &str,
    ) -> Option<Vec<f32>> {
        let preset = self.presets.get(preset_prefix)?;
        let &idx = preset.engine_id_to_idx.get(engine_id)?;
        let buf = preset.engines[idx].port_buffer(port_id)?;
        Some(buf.to_vec())
    }

    fn register_engine(
        &mut self,
        id: InstrumentId,
        spec: &EngineSpec,
        spec_dir: &Path,
    ) -> eyre::Result<PendingPreload> {
        let pack_path = resolve_relative(&spec.block.pack, spec_dir);
        let mut block = SamplerBlock::from_pack(&pack_path, self.sample_rate)?;
        block.apply_overrides(&spec.block.overrides);
        let cache = block.cache_handle();
        let paths = self.preload_profile.ordered_paths(&block);
        let instance = EngineInstance::new(spec.clone(), block, self.block_frames);
        self.instruments.insert(
            id,
            InstrumentSlot {
                engine: instance,
                muted: false,
                preload_target: Some(paths.len()),
            },
        );
        Ok(PendingPreload { cache, paths })
    }

    pub fn load_engine_spec(
        &mut self,
        id: impl Into<InstrumentId>,
        spec: &EngineSpec,
        spec_dir: &Path,
    ) -> eyre::Result<()> {
        let id = id.into();
        let pending = self.register_engine(id.clone(), spec, spec_dir)?;
        let label = spec.name.clone();
        let generation = self.next_preload_generation();
        let cancel = self.preload_cancel_token();
        if let Err(err) = std::thread::Builder::new()
            .name(format!("signal-preload-engine:{}", label))
            .spawn(move || {
                let stats = pending
                    .cache
                    .preload_cancelable(pending.paths.iter().map(|p| p.as_path()), || {
                        cancel.load(Ordering::Relaxed) != generation
                    });
                tracing::info!(
                    engine = %label,
                    loaded = stats.loaded,
                    failed = stats.failed,
                    "engine preload complete",
                );
            })
        {
            tracing::warn!(err = %err, "failed to spawn signal-preload-engine thread");
        }
        tracing::info!(
            id = ?id,
            engine = %spec.name,
            engine_type = %spec.engine_type,
            layers = spec.layers.len(),
            "loaded engine",
        );
        Ok(())
    }

    /// Load a [`PresetSpec`] — instantiates each engine ref under
    /// `<id_prefix>:<engine_id>`, registers a `PresetRuntime` for the
    /// routing graph, populates note routing, and spawns a coordinator
    /// thread that preloads engines in drum priority order.
    pub fn load_preset_spec(
        &mut self,
        id_prefix: &str,
        preset: &crate::preset_spec::PresetSpec,
        preset_dir: &Path,
    ) -> eyre::Result<Vec<InstrumentId>> {
        // Build engines first so we can collect block + spec + id triples
        // for the PresetRuntime constructor.
        let mut engine_triples: Vec<(String, EngineSpec, SamplerBlock)> = Vec::new();
        let mut preload_work: Vec<(InstrumentId, SampleCache, Vec<PathBuf>, u8)> = Vec::new();
        let mut slot_ids: Vec<InstrumentId> = Vec::new();

        for engine_ref in &preset.engines {
            let engine_path = resolve_relative(&engine_ref.engine, preset_dir);
            let engine_dir = engine_path.parent().unwrap_or(Path::new(""));
            let engine_spec = EngineSpec::from_file(&engine_path)?;
            let pack_path = resolve_relative(&engine_spec.block.pack, engine_dir);

            let mut block = SamplerBlock::from_pack(&pack_path, self.sample_rate)?;
            block.apply_overrides(&engine_spec.block.overrides);
            block.apply_overrides(&engine_ref.overrides);
            let cache = block.cache_handle();
            let paths = self.preload_profile.ordered_paths(&block);
            let prio = self
                .preload_profile
                .engine_priority(&engine_spec.engine_type);

            let id: InstrumentId = format!("{id_prefix}:{}", engine_ref.id);
            slot_ids.push(id.clone());
            preload_work.push((id.clone(), cache, paths, prio));
            self.instrument_to_preset
                .insert(id.clone(), (id_prefix.to_string(), engine_ref.id.clone()));
            engine_triples.push((engine_ref.id.clone(), engine_spec, block));
        }

        let runtime = PresetRuntime::build(
            preset,
            preset_dir,
            self.block_frames,
            self.sample_rate,
            engine_triples,
        )
        .map_err(|e| eyre::eyre!("preset runtime build failed: {e}"))?;

        // Replace any prior preset registered under the same prefix.
        self.presets.insert(id_prefix.to_string(), runtime);

        // Sort preload work by drum priority.
        preload_work.sort_by_key(|(_, _, _, p)| *p);

        let preset_label = preset.name.clone();
        let total_paths: usize = preload_work.iter().map(|(_, _, p, _)| p.len()).sum();
        let generation = self.next_preload_generation();
        let cancel = self.preload_cancel_token();
        if let Err(err) = std::thread::Builder::new()
            .name(format!("signal-preload-preset:{}", preset_label))
            .spawn(move || {
                let start = std::time::Instant::now();
                tracing::info!(
                    preset = %preset_label,
                    engines = preload_work.len(),
                    paths = total_paths,
                    "preset preload starting (priority order)",
                );
                let mut total_loaded = 0;
                for (engine_id, cache, paths, prio) in &preload_work {
                    if cancel.load(Ordering::Relaxed) != generation {
                        tracing::info!(preset = %preset_label, "preset preload cancelled");
                        break;
                    }
                    let s = std::time::Instant::now();
                    let stats = cache.preload_cancelable(paths.iter().map(|p| p.as_path()), || {
                        cancel.load(Ordering::Relaxed) != generation
                    });
                    total_loaded += stats.loaded;
                    tracing::info!(
                        preset = %preset_label,
                        engine = %engine_id,
                        priority = prio,
                        loaded = stats.loaded,
                        elapsed_ms = s.elapsed().as_millis() as u64,
                        "preset engine ready",
                    );
                }
                tracing::info!(
                    preset = %preset_label,
                    loaded = total_loaded,
                    total = total_paths,
                    elapsed_ms = start.elapsed().as_millis() as u64,
                    "preset preload complete",
                );
            })
        {
            tracing::warn!(err = %err, "failed to spawn signal-preload-preset thread");
        }

        tracing::info!(
            preset = %preset.name,
            engines = preset.engines.len(),
            modules = preset.modules.len(),
            routes = preset.routing.len(),
            "loaded preset",
        );
        Ok(slot_ids)
    }

    fn register_block(
        &mut self,
        id: InstrumentId,
        block_path: &Path,
    ) -> eyre::Result<PendingPreload> {
        let spec = BlockSpec::from_file(block_path)?;
        let dir = block_path.parent().unwrap_or(Path::new(""));
        let block = SamplerBlock::from_spec(spec, dir, self.sample_rate)?;
        let cache = block.cache_handle();
        let paths = self.preload_profile.ordered_paths(&block);
        let synth_spec = Self::synth_default_engine_spec(&block.name.clone());
        let instance = EngineInstance::new(synth_spec, block, self.block_frames);
        self.instruments.insert(
            id,
            InstrumentSlot {
                engine: instance,
                muted: false,
                preload_target: Some(paths.len()),
            },
        );
        Ok(PendingPreload { cache, paths })
    }

    pub fn preload_progress(&self, id: &str) -> (usize, usize) {
        if let Some(slot) = self.instruments.get(id) {
            if let Some(total) = slot.preload_target {
                return (slot.engine.block.loaded_sample_count().min(total), total);
            }
            return (
                slot.engine.block.loaded_sample_count(),
                slot.engine.block.total_sample_count(),
            );
        }
        if let Some((prefix, engine_id)) = self.instrument_to_preset.get(id) {
            if let Some(preset) = self.presets.get(prefix) {
                if let Some(&idx) = preset.engine_id_to_idx.get(engine_id) {
                    let b = &preset.engines[idx].block;
                    return (b.loaded_sample_count(), b.total_sample_count());
                }
            }
        }
        if let Some(preset) = self.presets.get(id) {
            return preset
                .engines
                .iter()
                .fold((0, 0), |(loaded, total), engine| {
                    (
                        loaded + engine.block.loaded_sample_count(),
                        total + engine.block.total_sample_count(),
                    )
                });
        }
        (0, 0)
    }

    pub fn active_voices(&self, id: &str) -> usize {
        if let Some(slot) = self.instruments.get(id) {
            return slot.engine.active_voices();
        }
        if let Some((prefix, engine_id)) = self.instrument_to_preset.get(id) {
            if let Some(preset) = self.presets.get(prefix) {
                if let Some(&idx) = preset.engine_id_to_idx.get(engine_id) {
                    return preset.engines[idx].active_voices();
                }
            }
        }
        if let Some(preset) = self.presets.get(id) {
            return preset.active_voices();
        }
        0
    }

    pub fn stolen_voices(&self, id: &str) -> usize {
        if let Some(slot) = self.instruments.get(id) {
            return slot.engine.stolen_voices();
        }
        if let Some((prefix, engine_id)) = self.instrument_to_preset.get(id) {
            if let Some(preset) = self.presets.get(prefix) {
                if let Some(&idx) = preset.engine_id_to_idx.get(engine_id) {
                    return preset.engines[idx].stolen_voices();
                }
            }
        }
        if let Some(preset) = self.presets.get(id) {
            return preset.stolen_voices();
        }
        0
    }

    pub fn total_stolen_voices(&self) -> usize {
        self.instruments
            .values()
            .map(|slot| slot.engine.stolen_voices())
            .sum::<usize>()
            + self
                .presets
                .values()
                .map(PresetRuntime::stolen_voices)
                .sum::<usize>()
    }

    pub fn total_cache_misses(&self) -> usize {
        self.instruments
            .values()
            .map(|slot| slot.engine.cache_misses())
            .sum::<usize>()
            + self
                .presets
                .values()
                .map(PresetRuntime::cache_misses)
                .sum::<usize>()
    }

    pub fn total_sample_misses(&self) -> usize {
        self.instruments
            .values()
            .map(|slot| slot.engine.sample_misses())
            .sum::<usize>()
            + self
                .presets
                .values()
                .map(PresetRuntime::sample_misses)
                .sum::<usize>()
    }

    pub fn recent_cache_misses(&self) -> Vec<String> {
        self.instruments
            .values()
            .flat_map(|slot| slot.engine.recent_cache_misses())
            .chain(
                self.presets
                    .values()
                    .flat_map(PresetRuntime::recent_cache_misses),
            )
            .take(8)
            .collect()
    }

    pub fn recent_sample_misses(&self) -> Vec<String> {
        self.instruments
            .values()
            .flat_map(|slot| slot.engine.recent_sample_misses())
            .chain(
                self.presets
                    .values()
                    .flat_map(PresetRuntime::recent_sample_misses),
            )
            .take(8)
            .collect()
    }

    pub fn resize_events(&self) -> u64 {
        self.instruments
            .values()
            .map(|slot| slot.engine.resize_events())
            .sum::<u64>()
            + self
                .presets
                .values()
                .map(PresetRuntime::resize_events)
                .sum::<u64>()
    }

    pub fn total_loaded_sample_bytes(&self) -> usize {
        self.instruments
            .values()
            .map(|slot| slot.engine.loaded_sample_bytes())
            .sum::<usize>()
            + self
                .presets
                .values()
                .map(PresetRuntime::loaded_sample_bytes)
                .sum::<usize>()
    }

    pub fn cache_budget_bytes(&self) -> Option<usize> {
        self.cache_budget_bytes
    }

    pub fn cache_over_budget_bytes(&self) -> usize {
        match self.cache_budget_bytes {
            Some(budget) => self.total_loaded_sample_bytes().saturating_sub(budget),
            None => 0,
        }
    }

    pub fn evict_cache_over_budget(&self) -> EvictStats {
        let Some(budget) = self.cache_budget_bytes else {
            return EvictStats {
                bytes_before: self.total_loaded_sample_bytes(),
                bytes_after: self.total_loaded_sample_bytes(),
                ..EvictStats::default()
            };
        };
        let bytes_before = self.total_loaded_sample_bytes();
        if bytes_before <= budget {
            return EvictStats {
                bytes_before,
                bytes_after: bytes_before,
                ..EvictStats::default()
            };
        }

        let cache_count = self.instruments.len()
            + self
                .presets
                .values()
                .map(|preset| preset.engines.len())
                .sum::<usize>();
        if cache_count == 0 {
            return EvictStats {
                bytes_before,
                bytes_after: bytes_before,
                ..EvictStats::default()
            };
        }
        let per_cache_budget = (budget / cache_count).max(1);
        let mut stats = EvictStats {
            bytes_before,
            bytes_after: bytes_before,
            ..EvictStats::default()
        };
        for slot in self.instruments.values() {
            let evicted = slot.engine.evict_cache_until_under_budget(per_cache_budget);
            stats.evicted += evicted.evicted;
            stats.bytes_freed += evicted.bytes_freed;
        }
        for preset in self.presets.values() {
            let evicted = preset.evict_cache_until_under_budget(per_cache_budget);
            stats.evicted += evicted.evicted;
            stats.bytes_freed += evicted.bytes_freed;
        }
        stats.bytes_after = self.total_loaded_sample_bytes();
        stats.bytes_freed = bytes_before.saturating_sub(stats.bytes_after);
        stats
    }

    pub fn preload_instrument(&mut self, id: &str) -> eyre::Result<PreloadStats> {
        if let Some(slot) = self.instruments.get_mut(id) {
            return Ok(slot.engine.block.preload_samples());
        }
        if let Some((prefix, engine_id)) = self.instrument_to_preset.get(id).cloned() {
            if let Some(preset) = self.presets.get_mut(&prefix) {
                if let Some(&idx) = preset.engine_id_to_idx.get(&engine_id) {
                    return Ok(preset.engines[idx].block.preload_samples());
                }
            }
        }
        Err(eyre::eyre!("instrument not loaded: {id}"))
    }

    pub fn warm_note_samples(&mut self, id: &str, note: u8, velocity: u8) -> PreloadStats {
        let routed = self.note_routing.get(&note).cloned();
        let target = routed.as_deref().unwrap_or(id);
        if let Some(slot) = self.instruments.get_mut(target) {
            return slot.engine.block.warm_note_samples(note, velocity);
        }
        if let Some(preset) = self.presets.get_mut(id) {
            if let Some(targets) = preset.note_routing.get(&note).cloned() {
                let mut out = PreloadStats::default();
                for (ti, _artic) in targets {
                    let s = preset.engines[ti].block.warm_note_samples(note, velocity);
                    out.loaded += s.loaded;
                    out.failed += s.failed;
                    out.bytes += s.bytes;
                }
                return out;
            }
        }
        if let Some((prefix, engine_id)) = self.instrument_to_preset.get(id).cloned() {
            if let Some(preset) = self.presets.get_mut(&prefix) {
                if let Some(&idx) = preset.engine_id_to_idx.get(&engine_id) {
                    return preset.engines[idx].block.warm_note_samples(note, velocity);
                }
            }
        }
        PreloadStats::default()
    }

    pub fn warm_midi_message_samples(
        &mut self,
        channel: u8,
        status: u8,
        data1: u8,
        data2: u8,
    ) -> PreloadStats {
        if (status & 0xF0) != 0x90 || data2 == 0 {
            return PreloadStats::default();
        }
        let Some(id) = self.midi_channels.get(&channel).cloned() else {
            return PreloadStats::default();
        };
        self.warm_note_samples(&id, data1, data2)
    }

    /// Apply a parameter override to a free-standing instrument's block.
    /// Used for live tweaks without a `.signalblock` reload.
    pub fn apply_block_override(&mut self, id: &str, ov: &ParamOverride) {
        if let Some(slot) = self.instruments.get_mut(id) {
            slot.engine.block.apply_override(ov);
        }
    }

    // ── MIDI dispatch ─────────────────────────────────────────────────────

    /// Dispatch a note-on. Resolution order:
    /// 1. If `id` references a preset (via instrument_to_preset),
    ///    dispatch to that preset's note routing or fall through to the
    ///    specific engine.
    /// 2. If `id` matches a free-standing instrument, dispatch directly.
    /// 3. Otherwise — try `id` as a preset prefix and use its note_routing.
    pub fn note_on(&mut self, id: &str, note: u8, velocity: u8) {
        // Free-standing fallback note routing.
        let routed = self.note_routing.get(&note).cloned();
        let target = routed.as_deref().unwrap_or(id);

        if let Some(slot) = self.instruments.get_mut(target) {
            slot.engine.block.note_on(note, velocity);
            return;
        }
        // id may itself be a preset prefix.
        if let Some(preset) = self.presets.get_mut(id) {
            if let Some(targets) = preset.note_routing.get(&note).cloned() {
                for (ti, artic) in targets {
                    preset.engines[ti]
                        .block
                        .note_on_articulated(note, velocity, artic.as_deref());
                }
                return;
            }
        }
        // id may be a `<prefix>:<engine>` ref.
        if let Some((prefix, engine_id)) = self.instrument_to_preset.get(id).cloned() {
            if let Some(preset) = self.presets.get_mut(&prefix) {
                if let Some(&idx) = preset.engine_id_to_idx.get(&engine_id) {
                    preset.engines[idx].block.note_on(note, velocity);
                }
            }
        }
    }

    pub fn note_off(&mut self, id: &str, note: u8) {
        let routed = self.note_routing.get(&note).cloned();
        let target = routed.as_deref().unwrap_or(id);
        if let Some(slot) = self.instruments.get_mut(target) {
            slot.engine.block.note_off(note);
            return;
        }
        if let Some(preset) = self.presets.get_mut(id) {
            if let Some(targets) = preset.note_routing.get(&note).cloned() {
                for (ti, _artic) in targets {
                    preset.engines[ti].block.note_off(note);
                }
                return;
            }
        }
        if let Some((prefix, engine_id)) = self.instrument_to_preset.get(id).cloned() {
            if let Some(preset) = self.presets.get_mut(&prefix) {
                if let Some(&idx) = preset.engine_id_to_idx.get(&engine_id) {
                    preset.engines[idx].block.note_off(note);
                }
            }
        }
    }

    pub fn note_off_with_velocity(&mut self, id: &str, note: u8, velocity: u8) {
        let routed = self.note_routing.get(&note).cloned();
        let target = routed.as_deref().unwrap_or(id);
        if let Some(slot) = self.instruments.get_mut(target) {
            slot.engine.block.note_off_with_velocity(note, velocity);
            return;
        }
        if let Some(preset) = self.presets.get_mut(id) {
            if let Some(targets) = preset.note_routing.get(&note).cloned() {
                for (ti, _artic) in targets {
                    preset.engines[ti]
                        .block
                        .note_off_with_velocity(note, velocity);
                }
                return;
            }
        }
        if let Some((prefix, engine_id)) = self.instrument_to_preset.get(id).cloned() {
            if let Some(preset) = self.presets.get_mut(&prefix) {
                if let Some(&idx) = preset.engine_id_to_idx.get(&engine_id) {
                    preset.engines[idx]
                        .block
                        .note_off_with_velocity(note, velocity);
                }
            }
        }
    }

    pub fn cc(&mut self, id: &str, controller: u8, value: u8) {
        if let Some(slot) = self.instruments.get_mut(id) {
            slot.engine.block.cc(controller, value);
            return;
        }
        if let Some((prefix, engine_id)) = self.instrument_to_preset.get(id).cloned() {
            if let Some(preset) = self.presets.get_mut(&prefix) {
                if let Some(&idx) = preset.engine_id_to_idx.get(&engine_id) {
                    preset.engines[idx].block.cc(controller, value);
                }
            }
        }
    }

    pub fn channel_aftertouch(&mut self, id: &str, value: u8) {
        if let Some(slot) = self.instruments.get_mut(id) {
            slot.engine.block.channel_aftertouch(value);
            return;
        }
        if let Some((prefix, engine_id)) = self.instrument_to_preset.get(id).cloned() {
            if let Some(preset) = self.presets.get_mut(&prefix) {
                if let Some(&idx) = preset.engine_id_to_idx.get(&engine_id) {
                    preset.engines[idx].block.channel_aftertouch(value);
                }
            }
        }
    }

    pub fn poly_aftertouch(&mut self, id: &str, note: u8, value: u8) {
        if let Some(slot) = self.instruments.get_mut(id) {
            slot.engine.block.poly_aftertouch(note, value);
            return;
        }
        if let Some((prefix, engine_id)) = self.instrument_to_preset.get(id).cloned() {
            if let Some(preset) = self.presets.get_mut(&prefix) {
                if let Some(&idx) = preset.engine_id_to_idx.get(&engine_id) {
                    preset.engines[idx].block.poly_aftertouch(note, value);
                }
            }
        }
    }

    pub fn midi_message(&mut self, channel: u8, status: u8, data1: u8, data2: u8) {
        // Explicit channel mapping wins; otherwise fall back to the default
        // instrument so a single-instrument live rig plays on any channel
        // without needing `set_midi_channel`.
        let id = match self
            .midi_channels
            .get(&channel)
            .cloned()
            .or_else(|| self.default_instrument.clone())
        {
            Some(id) => id,
            None => return,
        };
        let kind = status & 0xF0;
        match kind {
            0x80 => self.note_off_with_velocity(&id, data1, data2),
            0x90 => {
                if data2 == 0 {
                    self.note_off_with_velocity(&id, data1, data2);
                } else {
                    self.note_on(&id, data1, data2);
                }
            }
            0xB0 => self.cc(&id, data1, data2),
            0xA0 => self.poly_aftertouch(&id, data1, data2),
            0xD0 => self.channel_aftertouch(&id, data1),
            _ => {}
        }
    }

    /// Mix all un-muted instruments + presets into `output` (interleaved
    /// stereo, +=).
    pub fn render(&mut self, output: &mut [f32]) {
        let block_frames = output.len() / 2;
        // Free-standing instruments: each renders its EngineInstance,
        // then we sum its first port into master.
        for slot in self.instruments.values_mut() {
            if slot.muted {
                continue;
            }
            slot.engine.render(block_frames);
            if let Some(port) = slot.engine.ports.first() {
                let src = &slot.engine.layers[port.layer_index].out_buffer;
                for (m, s) in output.iter_mut().zip(src.iter()) {
                    *m += *s;
                }
            }
        }
        // Presets: full graph render.
        for preset in self.presets.values_mut() {
            preset.render(output);
        }
    }

    // ── Drum mixer (send-based mic routing) ──────────────────────────────

    /// Snapshot the drum mixer layout for a loaded preset (engines → close-mic
    /// channels + bus sends, plus shared bus tracks). `None` if the preset
    /// isn't a send-routed drum preset.
    pub fn preset_mixer_layout(&self, prefix: &str) -> Option<crate::mixer::MixerLayout> {
        self.presets.get(prefix)?.mixer().map(|m| m.layout())
    }

    /// Clone the lock-free peak-meter handle for a loaded preset's mixer.
    pub fn preset_mixer_meters(
        &self,
        prefix: &str,
    ) -> Option<std::sync::Arc<crate::mixer::MixerMeters>> {
        self.presets.get(prefix)?.mixer().map(|m| m.meters.clone())
    }

    pub fn set_mixer_piece_gain_db(&mut self, prefix: &str, i: usize, db: f32) {
        if let Some(m) = self.presets.get_mut(prefix).and_then(|p| p.mixer_mut()) {
            m.set_piece_gain_db(i, db);
        }
    }
    pub fn set_mixer_piece_mute(&mut self, prefix: &str, i: usize, muted: bool) {
        if let Some(m) = self.presets.get_mut(prefix).and_then(|p| p.mixer_mut()) {
            m.set_piece_mute(i, muted);
        }
    }
    pub fn set_mixer_piece_solo(&mut self, prefix: &str, i: usize, soloed: bool) {
        if let Some(m) = self.presets.get_mut(prefix).and_then(|p| p.mixer_mut()) {
            m.set_piece_solo(i, soloed);
        }
    }
    pub fn set_mixer_channel_gain_db(&mut self, prefix: &str, i: usize, db: f32) {
        if let Some(m) = self.presets.get_mut(prefix).and_then(|p| p.mixer_mut()) {
            m.set_channel_gain_db(i, db);
        }
    }
    pub fn set_mixer_channel_mute(&mut self, prefix: &str, i: usize, muted: bool) {
        if let Some(m) = self.presets.get_mut(prefix).and_then(|p| p.mixer_mut()) {
            m.set_channel_mute(i, muted);
        }
    }
    pub fn set_mixer_channel_solo(&mut self, prefix: &str, i: usize, soloed: bool) {
        if let Some(m) = self.presets.get_mut(prefix).and_then(|p| p.mixer_mut()) {
            m.set_channel_solo(i, soloed);
        }
    }
    pub fn set_mixer_send_level_db(&mut self, prefix: &str, i: usize, db: f32) {
        if let Some(m) = self.presets.get_mut(prefix).and_then(|p| p.mixer_mut()) {
            m.set_send_level_db(i, db);
        }
    }
    pub fn set_mixer_send_mute(&mut self, prefix: &str, i: usize, muted: bool) {
        if let Some(m) = self.presets.get_mut(prefix).and_then(|p| p.mixer_mut()) {
            m.set_send_mute(i, muted);
        }
    }
    pub fn set_mixer_send_solo(&mut self, prefix: &str, i: usize, soloed: bool) {
        if let Some(m) = self.presets.get_mut(prefix).and_then(|p| p.mixer_mut()) {
            m.set_send_solo(i, soloed);
        }
    }
    pub fn set_mixer_bus_gain_db(&mut self, prefix: &str, i: usize, db: f32) {
        if let Some(m) = self.presets.get_mut(prefix).and_then(|p| p.mixer_mut()) {
            m.set_bus_gain_db(i, db);
        }
    }
    pub fn set_mixer_bus_mute(&mut self, prefix: &str, i: usize, muted: bool) {
        if let Some(m) = self.presets.get_mut(prefix).and_then(|p| p.mixer_mut()) {
            m.set_bus_mute(i, muted);
        }
    }
    pub fn set_mixer_bus_solo(&mut self, prefix: &str, i: usize, soloed: bool) {
        if let Some(m) = self.presets.get_mut(prefix).and_then(|p| p.mixer_mut()) {
            m.set_bus_solo(i, soloed);
        }
    }
    pub fn set_mixer_master_gain_db(&mut self, prefix: &str, db: f32) {
        if let Some(m) = self.presets.get_mut(prefix).and_then(|p| p.mixer_mut()) {
            m.set_master_gain_db(db);
        }
    }
    pub fn set_mixer_master_mute(&mut self, prefix: &str, muted: bool) {
        if let Some(m) = self.presets.get_mut(prefix).and_then(|p| p.mixer_mut()) {
            m.set_master_mute(muted);
        }
    }

    // ── FX-chain plugin hosting ──────────────────────────────────────────

    /// Install a hosted plugin into a slot on the loaded preset's drum mixer
    /// (channel / bus / master). Returns the new slot index.
    pub fn install_mixer_plugin(
        &mut self,
        prefix: &str,
        target: crate::mixer::FxTarget,
        plugin: signal_plugin_host::HostedPlugin,
    ) -> Result<usize, signal_plugin_host::PluginError> {
        let mixer = self
            .presets
            .get_mut(prefix)
            .and_then(|p| p.mixer_mut())
            .ok_or_else(|| {
                signal_plugin_host::PluginError::LoadFailed(format!(
                    "no drum mixer for preset {prefix:?}"
                ))
            })?;
        mixer.install_plugin(target, plugin)
    }

    /// Install a hosted plugin on the preset's master FX chain (works for
    /// drum and non-drum presets — for drum presets this chain runs after
    /// the drum mixer's own master_fx).
    pub fn install_preset_master_plugin(
        &mut self,
        prefix: &str,
        plugin: signal_plugin_host::HostedPlugin,
    ) -> Result<usize, signal_plugin_host::PluginError> {
        let preset = self.presets.get_mut(prefix).ok_or_else(|| {
            signal_plugin_host::PluginError::LoadFailed(format!("no preset {prefix:?}"))
        })?;
        preset.install_master_plugin(plugin)
    }

    pub fn remove_mixer_plugin(
        &mut self,
        prefix: &str,
        target: crate::mixer::FxTarget,
        slot_idx: usize,
    ) {
        if let Some(m) = self.presets.get_mut(prefix).and_then(|p| p.mixer_mut()) {
            m.remove_plugin(target, slot_idx);
        }
    }

    pub fn remove_preset_master_plugin(&mut self, prefix: &str, slot_idx: usize) {
        if let Some(p) = self.presets.get_mut(prefix) {
            p.remove_master_plugin(slot_idx);
        }
    }

    pub fn set_mixer_slot_bypass(
        &mut self,
        prefix: &str,
        target: crate::mixer::FxTarget,
        slot_idx: usize,
        bypassed: bool,
    ) {
        if let Some(m) = self.presets.get_mut(prefix).and_then(|p| p.mixer_mut()) {
            m.set_slot_bypass(target, slot_idx, bypassed);
        }
    }

    pub fn set_mixer_slot_param(
        &self,
        prefix: &str,
        target: crate::mixer::FxTarget,
        slot_idx: usize,
        param_id: u32,
        value: f64,
    ) {
        if let Some(m) = self.presets.get(prefix).and_then(|p| p.mixer.as_ref()) {
            m.set_slot_param(target, slot_idx, param_id, value);
        }
    }

    pub fn mixer_slot_params(
        &mut self,
        prefix: &str,
        target: crate::mixer::FxTarget,
        slot_idx: usize,
    ) -> Option<Vec<signal_plugin_host::PluginParamInfo>> {
        let m = self.presets.get_mut(prefix).and_then(|p| p.mixer_mut())?;
        m.slot_params(target, slot_idx)
    }

    /// Install a NAM model on the drum mixer at `target`. The model is
    /// loaded from disk + prepared at the mixer's sample rate before the
    /// audio thread sees the slot — so the slot is render-ready the
    /// instant `install_nam` returns.
    pub fn install_mixer_nam(
        &mut self,
        prefix: &str,
        target: crate::mixer::FxTarget,
        model_path: impl AsRef<std::path::Path>,
    ) -> Result<usize, String> {
        let mixer = self
            .presets
            .get_mut(prefix)
            .and_then(|p| p.mixer_mut())
            .ok_or_else(|| format!("no drum mixer for preset {prefix:?}"))?;
        mixer.install_nam(target, model_path)
    }

    /// Install a NAM model on the preset's master FX chain (works for
    /// non-drum presets too).
    pub fn install_preset_master_nam(
        &mut self,
        prefix: &str,
        model_path: impl AsRef<std::path::Path>,
    ) -> Result<usize, String> {
        let preset = self
            .presets
            .get_mut(prefix)
            .ok_or_else(|| format!("no preset {prefix:?}"))?;
        preset.install_master_nam(model_path)
    }

    /// Set the input or output gain (dB) of a NAM slot. No-op for non-NAM
    /// backends and missing slots.
    pub fn set_mixer_nam_gain(
        &mut self,
        prefix: &str,
        target: crate::mixer::FxTarget,
        slot_idx: usize,
        input: bool,
        gain_db: f32,
    ) {
        if let Some(m) = self.presets.get_mut(prefix).and_then(|p| p.mixer_mut()) {
            m.set_nam_gain(target, slot_idx, input, gain_db);
        }
    }

    /// Render a loaded preset into per-mic buses (keyed by mic id). See
    /// [`PresetRuntime::render_buses`]. `None` if no preset under `prefix`.
    pub fn render_preset_buses(
        &mut self,
        prefix: &str,
        block_frames: usize,
    ) -> Option<std::collections::BTreeMap<String, Vec<f32>>> {
        self.presets
            .get_mut(prefix)
            .map(|p| p.render_buses(block_frames))
    }

    pub fn len(&self) -> usize {
        self.instruments.len()
            + self
                .presets
                .values()
                .map(|p| p.engines.len())
                .sum::<usize>()
    }

    pub fn is_empty(&self) -> bool {
        self.instruments.is_empty() && self.presets.is_empty()
    }

    pub fn instrument_ids(&self) -> Vec<&str> {
        let mut ids: Vec<&str> = self.instruments.keys().map(|s| s.as_str()).collect();
        ids.extend(self.instrument_to_preset.keys().map(|s| s.as_str()));
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preload_profile_names_parse() {
        assert_eq!(
            PreloadProfile::from_name("fast-audition"),
            Some(PreloadProfile::FastAudition)
        );
        assert_eq!(
            PreloadProfile::from_name("full_preload"),
            Some(PreloadProfile::Full)
        );
        assert_eq!(
            PreloadProfile::from_name("orchestral"),
            Some(PreloadProfile::OrchestralArticulation)
        );
        assert_eq!(PreloadProfile::from_name("unknown"), None);
    }

    #[test]
    fn orchestral_priority_prefers_core_sections() {
        assert!(orchestral_priority("solo violin") < orchestral_priority("effects"));
        assert!(orchestral_priority("cello") < orchestral_priority("effects"));
    }
}
