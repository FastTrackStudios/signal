//! Sample library playback engine for Signal.
//!
//! Loads and plays any sample library described by a `.styx` spec file:
//! orchestral strings, brass, winds, drums, piano — anything.
//!
//! # Architecture
//!
//! ```text
//! LibrarySpec  (loaded from a .styx spec file via facet-styx)
//!   + SampleMap  (scanned from extracted WAV root directory)
//!   = PlayerPatch  (combined playback context)
//!       → SampleEngine  (MIDI-driven voice engine, one per section/instrument)
//!           → SamplerBank  (N engines, MIDI channel routing, stereo mix)
//!               → SamplerRig  (daw-backed: SamplerBank on daw's AudioEngine)
//! ```
//!
//! # Library specs
//!
//! A library spec is a `.styx` file that describes:
//! - Instrument sections (1v, 2v, Va, Ce, Ba for strings; etc.)
//! - Articulations (Vibsus, Staccato, Leg, etc.)
//! - Dynamics (CC1 layers, velocity ranges)
//! - Legato engine (pre-delay zones, portamento threshold)
//! - Keyswitch/CC58 mapping
//!
//! Third-party libraries can be added by writing a new `.styx` spec file —
//! no code changes required.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use signal_sampler::SamplerRig;
//! use std::path::Path;
//!
//! let rig = SamplerRig::new()?;
//! rig.load_instrument(
//!     "strings_1v",
//!     Path::new("specs/cinematic-strings.styx"),
//!     Some(Path::new("/path/to/wavs")),
//!     "1v", "Mix",
//! )?;
//! rig.note_on("strings_1v", 60, 100);
//! rig.cc("strings_1v", 1, 80);
//! # Ok::<(), eyre::Error>(())
//! ```

pub mod api;
pub mod audio_soundsource;
pub mod bank;
pub mod block;
pub mod convolver;
pub mod document;
pub mod document_rt;
pub mod engine;
pub mod engine_spec;
pub mod instrument;
pub mod loudness;
pub mod midi;
pub mod mixer;
pub mod module_spec;
pub mod nam;
pub mod nam_calibrate;
pub mod native;
pub mod native_osc;
pub mod soundsource;
pub mod node_render;
pub mod nord;
pub mod pack_rewrite;
pub mod preset_registry;
pub mod preset_spec;
pub mod retag;
pub mod rig;
pub mod rig_library;
pub mod rig_manager;
pub mod rig_node;
pub mod rig_prefs;
pub mod rig_profile;
pub mod runtime;
pub mod sample_map;
pub mod sampler_rig;
pub mod spec;
pub mod stats;

pub use bank::{PreloadProfile, SamplerBank};
pub use block::{BlockParams, BlockSpec, SamplerBlock};
pub use convolver::Convolver;
pub use document::{
    DocCc, DocEvent, DocNote, DocumentBusRenderResult, DocumentRenderOptions, DocumentRenderResult,
    Schedule, ScheduledEvent, TempoPoint, TrackDocument, annotate, line_for_chan,
};
pub use document_rt::{BlockTransport, RealtimeScheduler};
pub use engine::cache::SignalPcmPack;
pub use engine::trace::{RenderTrace, TraceEvent, TraceKind, VoiceSpawn};
pub use engine::{ArticClass, EmittedMarker, LegatoFireEvent, LineId, PlayMode, SampleEngine};
pub use engine_spec::{BlockRef, EngineLayerSpec, EngineSpec, FxChainSlot, PortSpec, VoiceConfig};
pub use instrument::SamplerInstrument;
pub use mixer::{
    Bus, BusStrip, ChannelStrip, DirectChannel, DrumMixer, EngineStrip, FxBackend, FxSlotStrip,
    FxTarget, MixerLayout, MixerMeters, Send as MixerSend, SendStrip,
};
pub use module_spec::{ModulePort, ModuleSpec};
pub use nam::NamProcessor;
pub use audio_soundsource::AudioSoundsource;
pub use native_osc::{NativeOscillator, OscWave};
pub use soundsource::{Soundsource, SoundsourceKind, SoundsourceLeaf};
pub use node_render::{LeafBackend, RenderNode, build_node_backend};
pub use preset_registry::{PresetRegistry, PresetSource, RegisteredPreset};
pub use preset_spec::{
    MacroDef, MacroTarget, MasterFxSlot, NoteRoute, PresetEngineRef, PresetModuleRef, PresetSpec,
    RoutingRule,
};
pub use rig::{BlockImpl, DeviceInfo, GuitarRig, ModelId, RigBlock, SlotInfo};
pub use rig_library::{Library, RigPreset, RigScene, RigSection, RigSong};
pub use rig_manager::RigManager;
pub use rig_node::{Combine, Container, Param, RigNode, Role, Send, Zone};
pub use rig_prefs::RigAudioPrefs;
pub use rig_profile::{ProfileRig, RigPatch, RigProfile};
pub use runtime::{
    BufferRef, EngineInstance, LayerRuntime, ModuleInstance, PortRuntime, PresetRuntime,
    ResolvedEdge,
};
pub use sample_map::{SampleKey, SampleMap};
pub use midicore::MidiMonitor;
pub use sampler_rig::{BusTrack, InstrumentTrack, SamplerRig};
// Hardware MIDI input primitives live in `midicore` (the `midir` OS backend);
// re-export the selector + handle + event types so rig consumers (e.g. the
// strings TUI) don't need a direct midicore dependency.
pub use midicore;
pub use midicore::MidiEvent;
pub use midicore::PortSelector as MidiSelection;
pub use midicore::midir::MidiInput as MidiInputHandle;
pub use spec::LibrarySpec;
pub use stats::AudioStatsSnapshot;

use std::path::Path;

pub mod pack {
    //! `.signalpack` reader utilities.
    //!
    //! Cheap header-only inspection without decoding any audio.

    use super::{LibrarySpec, SamplerError, SignalPcmPack};
    use std::path::Path;

    /// Result of [`read_pack_header`] — parsed library spec plus pack stats.
    /// No audio data is loaded.
    #[derive(Debug, Clone)]
    pub struct PackHeader {
        pub spec: LibrarySpec,
        pub sample_count: usize,
        pub size_bytes: u64,
    }

    /// Cheap header-only read from a `.signalpack`.
    ///
    /// Returns the embedded `LibrarySpec` plus pack statistics. No audio is
    /// decoded — suitable for browse/list panels.
    pub fn read_pack_header(pack_path: &Path) -> Result<PackHeader, SamplerError> {
        let pack = SignalPcmPack::open(pack_path)?;
        let spec = parse_embedded_spec(&pack)?;
        let size_bytes = std::fs::metadata(pack_path).map(|m| m.len()).unwrap_or(0);
        Ok(PackHeader {
            spec,
            sample_count: pack.entry_count(),
            size_bytes,
        })
    }

    pub(crate) fn parse_embedded_spec(pack: &SignalPcmPack) -> Result<LibrarySpec, SamplerError> {
        let text = pack
            .embedded_spec()
            .ok_or_else(|| SamplerError::SpecParse("pack carries no embedded spec".into()))?;
        match pack.embedded_spec_format() {
            Some("toml") => LibrarySpec::from_toml(text),
            _ => LibrarySpec::from_styx(text),
        }
    }
}

pub use pack::{PackHeader, read_pack_header};

/// Identifier for a loaded instrument within the bank.
pub type InstrumentId = String;

// ── Error ─────────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum SamplerError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("spec parse error: {0}")]
    SpecParse(String),

    #[error("invalid MIDI note name: {0:?}")]
    BadNoteName(String),

    #[error("spec missing section {0:?}")]
    MissingSection(String),

    #[error("spec missing articulation {0:?}")]
    MissingArticulation(String),
}

// ── PlayerPatch ───────────────────────────────────────────────────────────────

/// A fully loaded library patch: spec + sample index.
///
/// Two playback modes are supported:
/// - **Convention mode** — sample lookup goes through `map` and decodes
///   `(section, articulation, mic, dynamic, note, direction, rr)`. Used by
///   libraries with regular filename conventions (Cinematic Strings, Keyscape).
/// - **Zone mode** — `spec.zones` is non-empty. Sample lookup goes through
///   `zone_paths` and resolves `(note, velocity, rr_idx)` directly via
///   [`resolve_zone`](Self::resolve_zone). Used by Spectrasonics-style
///   libraries (Omnisphere, Trilian) where the keymap is metadata, not
///   filenames.
pub struct PlayerPatch {
    pub spec: LibrarySpec,
    pub map: SampleMap,
    /// Absolute paths parallel to `spec.zones` (zone mode only; empty otherwise).
    pub zone_paths: Vec<std::path::PathBuf>,
    /// Absolute paths parallel to `spec.grooves`. Empty when the spec has none.
    pub groove_paths: Vec<std::path::PathBuf>,
    /// Absolute paths parallel to `spec.wavetables`. Empty when the spec has none.
    pub wavetable_paths: Vec<std::path::PathBuf>,
    pub prepared_cache_dir: Option<std::path::PathBuf>,
    /// When loaded from a `.signalpack`, the pack itself supplies all audio.
    /// `SampleEngine` builds its cache directly from this rather than from
    /// disk, so no on-disk source files need exist.
    pub pack: Option<SignalPcmPack>,
}

/// Resolved zone match: where to read the sample plus how to play it.
#[derive(Debug, Clone)]
pub struct ResolvedZone {
    pub path: std::path::PathBuf,
    /// Pitch at which the sample plays back unchanged.
    pub root_key: u8,
    /// Per-zone gain in dB.
    pub gain_db: f32,
    /// Per-zone fine-tune in cents (added on top of the root-key transposition).
    pub tune_cents: f32,
}

impl PlayerPatch {
    /// Load a spec and scan WAV files under `samples_root`.
    ///
    /// If the spec carries explicit `zones`, the patch is built in zone mode and
    /// the on-disk filename scan is skipped (zoned libraries have arbitrary
    /// filenames that the convention parser cannot handle).
    pub fn load(spec_path: &Path, samples_root: &Path) -> Result<Self, SamplerError> {
        let spec = LibrarySpec::from_file(spec_path)?;
        let zoned = !spec.zones.is_empty();
        let zone_paths = if zoned {
            spec.zones
                .iter()
                .map(|z| samples_root.join(&z.file))
                .collect()
        } else {
            Vec::new()
        };
        let groove_paths = spec
            .grooves
            .iter()
            .map(|g| samples_root.join(&g.file))
            .collect();
        let wavetable_paths = spec
            .wavetables
            .iter()
            .map(|w| samples_root.join(&w.file))
            .collect();
        // Filename-scan only when neither zones nor grooves carry the layout.
        let convention_mode = !zoned && spec.grooves.is_empty();
        let map = if convention_mode {
            SampleMap::scan(samples_root)?
        } else {
            SampleMap::empty()
        };
        let prepared_cache_dir = engine::cache::default_prepared_cache_dir(samples_root);
        let prepared_cache_dir = prepared_cache_dir
            .join("index.tsv")
            .exists()
            .then_some(prepared_cache_dir);
        Ok(Self {
            spec,
            map,
            zone_paths,
            groove_paths,
            wavetable_paths,
            prepared_cache_dir,
            pack: None,
        })
    }

    /// Load a patch whose **samples** come from `zones_path` (a zone spec) but
    /// whose **engine config** — sections / mics / dynamics / articulations /
    /// legato engine / keyswitch + CC58 map — comes from a separate descriptive
    /// spec at `config_path`.
    ///
    /// Cinematic Studio libraries ship this way: `zones.styx` (or a per-section
    /// `_patches/<section>/library.styx`) carries only the sample paths, and its
    /// own header says to load `cinematic-strings.styx` alongside it for the
    /// engine config. Without the config, the runtime has no articulation
    /// definitions or keyswitch/CC58 map, so articulation switching can't work.
    pub fn load_merged(
        config_path: &Path,
        zones_path: &Path,
        samples_root: &Path,
    ) -> Result<Self, SamplerError> {
        let mut patch = Self::load(zones_path, samples_root)?;
        let cfg = LibrarySpec::from_file(config_path)?;
        // Overlay the descriptive config; keep the zone spec's samples
        // (zones / zone_paths / grooves / wavetables / map stay as loaded).
        patch.spec.sections = cfg.sections;
        patch.spec.mics = cfg.mics;
        patch.spec.dynamics = cfg.dynamics;
        patch.spec.articulations = cfg.articulations;
        patch.spec.legato_engine = cfg.legato_engine;
        patch.spec.short_note_timing = cfg.short_note_timing;
        patch.spec.keyswitch = cfg.keyswitch;
        patch.spec.performance = cfg.performance;
        Ok(patch)
    }

    /// Build a patch from an already-parsed spec with an empty sample map.
    pub fn from_spec(spec: LibrarySpec) -> Self {
        Self {
            spec,
            map: SampleMap::empty(),
            zone_paths: Vec::new(),
            groove_paths: Vec::new(),
            wavetable_paths: Vec::new(),
            prepared_cache_dir: None,
            pack: None,
        }
    }

    /// Load a patch directly from a `.signalpack`.
    ///
    /// The pack's embedded styx supplies the [`LibrarySpec`]; sample data
    /// decodes straight from the pack body — no on-disk audio required.
    ///
    /// Wavetables are ignored entirely (out of scope for the sampler engine).
    /// Grooves with no explicit `zones` are surfaced as one synthesized zone
    /// per groove rooted at `slice_base_note` so a single MIDI key triggers
    /// the whole loop. Slicing and time-stretch are intentionally not
    /// implemented — the loop plays sample-rate-locked at original tempo.
    pub fn from_pack(pack_path: &Path) -> Result<Self, SamplerError> {
        use crate::engine::cache::SignalPcmPack;
        let pack = SignalPcmPack::open(pack_path)?;
        let mut spec = pack::parse_embedded_spec(&pack)?;

        // Stylus / groove libraries: synthesize one zone per groove so the
        // existing zone-mode playback path triggers the whole loop on
        // note-on. This is the documented simplification — no slicing, no
        // time-stretch, no tempo sync. A single key (slice_base_note, or 36
        // by default) plays the entire loop at its native sample rate.
        if spec.zones.is_empty() && !spec.grooves.is_empty() {
            for g in &spec.grooves {
                let root = if g.slice_base_note == 0 {
                    36
                } else {
                    g.slice_base_note
                };
                spec.zones.push(crate::spec::ZoneSpec {
                    file: g.file.clone(),
                    key_min: root,
                    key_max: root,
                    root_key: root,
                    vel_min: 0,
                    vel_max: 127,
                    rr_index: 0,
                    rr_mode: String::new(),
                    gain_db: 0.0,
                    pan: 0.0,
                    tune_cents: 0.0,
                    sample_start: 0,
                    sample_end: 0,
                    loop_start: 0,
                    loop_end: 0,
                    loop_xfade: 0,
                    fade_in: 0,
                    release_start: 0,
                    playback_mode: String::new(),
                    trigger_mode: String::new(),
                    trigger_cc: 0,
                    trigger_value_min: 0,
                    trigger_value_max: 0,
                    mic: String::new(),
                    articulation: String::new(),
                    dynamic: String::new(),
                    direction: String::new(),
                    interval: 0,
                    lead_in_ms: 0.0,
                    arrival_ms: 0.0,
                    group: String::new(),
                    group_polyphony: 0,
                    choke_group: String::new(),
                    off_by: Vec::new(),
                    section: String::new(),
                    variant: String::new(),
                });
            }
        }

        // Build zone_paths from the relative file fields. The pack's
        // entry_for_path uses suffix-matching, so a relative or absolute
        // path resolves the same entry — we use the relative path verbatim
        // so the lookup is direct.
        let zone_paths: Vec<std::path::PathBuf> = spec
            .zones
            .iter()
            .map(|z| std::path::PathBuf::from(&z.file))
            .collect();
        let groove_paths: Vec<std::path::PathBuf> = spec
            .grooves
            .iter()
            .map(|g| std::path::PathBuf::from(&g.file))
            .collect();

        // Convention-mode packs (older Keyscape, CSS pre-zone-rewrite) ship
        // with no `zones` array — the engine resolves samples by parsing the
        // filenames of every entry. Build the SampleMap directly from the
        // pack's entry list so playback works even when no on-disk source
        // is available.
        let map = if spec.zones.is_empty() && spec.grooves.is_empty() {
            SampleMap::from_paths(pack.entry_paths().map(|p| p.to_path_buf()))
        } else {
            SampleMap::empty()
        };

        // Re-derive each articulation's dynamic / RR list from the actual
        // sample map. The spec stored in the pack header is whatever the
        // importer wrote at pack-build time, which can be stale or wrong
        // (e.g. Keyscape Classic `clrr10` was authored with a single
        // "127" dynamic but the v01..v19 velocity-layer samples encode 19
        // dynamics — the spec list misses 18 of them). Trusting the map
        // here keeps spec + samples consistent regardless of importer
        // quirks.
        {
            use std::collections::{BTreeMap, BTreeSet};
            let mut by_artic: BTreeMap<String, (BTreeSet<String>, BTreeSet<usize>)> =
                BTreeMap::new();
            for (k, _) in map.iter() {
                let e = by_artic.entry(k.articulation.clone()).or_default();
                e.0.insert(k.dynamic.clone());
                e.1.insert(k.rr);
            }
            for a in spec.articulations.iter_mut() {
                if let Some((dyns, rrs)) = by_artic.get(&a.id) {
                    if !dyns.is_empty() {
                        a.dynamics = dyns.iter().cloned().collect();
                    }
                    if !rrs.is_empty() {
                        let max_rr = rrs.iter().max().copied().unwrap_or(0) + 1;
                        a.rr = max_rr.max(1);
                    }
                }
            }
        }

        Ok(Self {
            spec,
            map,
            zone_paths,
            groove_paths,
            wavetable_paths: Vec::new(),
            prepared_cache_dir: None,
            pack: Some(pack),
        })
    }

    /// Whether the patch is in zone mode (explicit `(key × vel × RR)` zones).
    pub fn is_zoned(&self) -> bool {
        !self.zone_paths.is_empty()
    }

    pub fn total_samples(&self) -> usize {
        let base = if self.is_zoned() {
            self.zone_paths.len()
        } else {
            self.map.total()
        };
        base + self.groove_paths.len() + self.wavetable_paths.len()
    }

    pub fn sample_paths(&self) -> Box<dyn Iterator<Item = &std::path::PathBuf> + '_> {
        let base: Box<dyn Iterator<Item = &std::path::PathBuf> + '_> = if self.is_zoned() {
            Box::new(self.zone_paths.iter())
        } else {
            Box::new(self.map.paths())
        };
        Box::new(
            base.chain(self.groove_paths.iter())
                .chain(self.wavetable_paths.iter()),
        )
    }

    /// Sample paths ordered by `|note - center|` so the middle of the
    /// keyboard decodes first and the extremes last. Used by the
    /// background preloader so the most-played range comes online first.
    /// `center` defaults to 60 (middle C) at the call site.
    ///
    /// - Zone-mode patches: zones sorted by `|root_key - center|`.
    /// - Convention-mode patches: sample-map entries sorted by `|note - center|`.
    /// - Grooves + wavetables: appended last (no inherent pitch priority).
    pub fn sample_paths_centered(&self, center: u8) -> Vec<std::path::PathBuf> {
        let center = center as i32;
        let mut out: Vec<std::path::PathBuf> = Vec::new();
        if self.is_zoned() {
            let mut indexed: Vec<(i32, &std::path::PathBuf)> = self
                .spec
                .zones
                .iter()
                .zip(self.zone_paths.iter())
                .map(|(z, p)| ((z.root_key as i32 - center).abs(), p))
                .collect();
            indexed.sort_by_key(|(d, _)| *d);
            out.extend(indexed.into_iter().map(|(_, p)| p.clone()));
        } else {
            let mut indexed: Vec<(i32, std::path::PathBuf)> = self
                .map
                .iter()
                .map(|(k, p)| ((k.note as i32 - center).abs(), p.clone()))
                .collect();
            indexed.sort_by_key(|(d, _)| *d);
            out.extend(indexed.into_iter().map(|(_, p)| p));
        }
        out.extend(self.groove_paths.iter().cloned());
        out.extend(self.wavetable_paths.iter().cloned());
        out
    }

    pub fn resolve(
        &self,
        section_id: &str,
        articulation_id: &str,
        mic_id: &str,
        dynamic: &str,
        target_note: u8,
        direction: &str,
        rr: usize,
    ) -> Option<(std::path::PathBuf, u8)> {
        self.map.resolve(
            &self.spec,
            section_id,
            articulation_id,
            mic_id,
            dynamic,
            target_note,
            direction,
            rr,
        )
    }

    /// Resolve a zone for `(note, velocity)`, RR-cycling within the matching set.
    ///
    /// Returns `None` if the patch is not in zone mode or no zone contains the
    /// `(note, velocity)` point. Multiple matching zones form a round-robin
    /// group; `rr_idx` is reduced modulo the group size.
    pub fn resolve_zone(&self, note: u8, velocity: u8, rr_idx: usize) -> Option<ResolvedZone> {
        if !self.is_zoned() {
            return None;
        }
        // Indices of zones whose (key range, vel range) contains the point.
        let candidates: Vec<usize> = self
            .spec
            .zones
            .iter()
            .enumerate()
            .filter(|(_, z)| z.contains(note, velocity))
            .map(|(i, _)| i)
            .collect();
        if candidates.is_empty() {
            return None;
        }
        let pick = candidates[rr_idx % candidates.len()];
        let z = &self.spec.zones[pick];
        Some(ResolvedZone {
            path: self.zone_paths[pick].clone(),
            root_key: z.root_key,
            gain_db: z.gain_db,
            tune_cents: z.tune_cents,
        })
    }

    pub fn legato_delay_expressive(&self, velocity: u8) -> Option<u32> {
        self.spec
            .legato_engine
            .as_ref()?
            .expressive
            .as_ref()?
            .delay_for_velocity(velocity)
    }

    pub fn legato_delay_low_latency(&self, velocity: u8) -> Option<u32> {
        self.spec
            .legato_engine
            .as_ref()?
            .low_latency
            .as_ref()?
            .delay_for_velocity(velocity)
    }

    pub fn short_note_pre_delay_ms(&self) -> u32 {
        self.spec
            .short_note_timing
            .as_ref()
            .map(|t| t.pre_delay_ms)
            .unwrap_or(0)
    }
}
