//! Synth-rig wire contract — the detachable-GUI boundary for the software-synth
//! rig (Omnisphere patch import). Sibling of `signal-keys-proto`: the rig core
//! hosts a composition tree (Quadzone → layers → oscillator/filter/amp/FX) on
//! the shared engine; every front-end is a vox remote speaking the
//! [`synth::SynthRig`] service.
//!
//! All types are plain `facet::Facet` data (no Dioxus, no audio backend), so
//! this crate compiles for wasm + embedded.

use facet::Facet;

// ── Wire types ──────────────────────────────────────────────────────────────

/// One loadable synth preset (an imported Omnisphere `.prt_omn` / `.mlt_omn`).
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct SynthPreset {
    /// Display name (the patch file's stem).
    pub name: String,
    /// Broad category for grouping ("Pads", "Bass", "Lead", "Poly", …) — the
    /// patch's library folder.
    pub kind: String,
    /// Whether this is the currently-loaded preset.
    pub loaded: bool,
}

/// One browsable soundsource pack in the library — a faceted, tag-driven
/// projection of `signal_browser::pack_registry::PackEntry` for the BROWSE view.
/// Metadata only (no audio): the UI fetches every item once and filters
/// client-side. `tags` are encoded `"category:value"` keys
/// ([`signal_proto::tagging::StructuredTag::encode`]); re-parse them with
/// `StructuredTag::parse` into a `TagSet` to reuse the faceted-filter engine.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct BrowseItem {
    /// Absolute path to the loadable file — the stable item id.
    pub id: String,
    /// Display name (`LibrarySpec.name`, falling back to the file stem).
    pub name: String,
    /// Loadable kind label ("pack" | "engine" | "preset" | "module" | "block").
    pub kind: String,
    /// `LibrarySpec.instrument` (empty when unclassified).
    pub instrument: String,
    /// `LibrarySpec.category` (empty when unclassified).
    pub category: String,
    /// `LibrarySpec.vendor`.
    pub vendor: String,
    /// Structured tags as encoded `"category:value"` keys.
    pub tags: Vec<String>,
    /// `LibrarySpec.style`.
    pub style: Vec<String>,
    /// Path-component groups for tree-style display / grouping.
    pub folder: Vec<String>,
    /// Total samples in the pack body.
    pub sample_count: u32,
}

/// A node in the loaded composition tree (Quadzone → layers → blocks) — the
/// structure the control view renders as selectable boxes.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct SynthNode {
    /// Path id addressing this node (`"omnisphere"`, `"omnisphere/layer-a"`, …).
    pub id: String,
    /// Display label ("Omnisphere", "Layer A", "Filter 1", …).
    pub label: String,
    /// Node role: "preset" | "engine" | "layer" | "module" | "block".
    pub role: String,
    /// Whether this node currently produces sound (has a live backend).
    pub live: bool,
    /// Child nodes, in order.
    pub children: Vec<SynthNode>,
}

/// Live transport + meter snapshot — the high-rate poll payload.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct SynthStatus {
    pub running: bool,
    /// The loaded preset's display name, if any.
    pub loaded_preset: Option<String>,
    /// Master output peak (linear 0..~1).
    pub master_peak: f32,
    /// Active voices.
    pub voices: u32,
    /// The attached MIDI input port name, if any (None = omni / all).
    pub midi_port: Option<String>,
    /// Master output gain (linear; 1.0 = unity). Summed multi-layer patches run
    /// hot, so this defaults to a pad below unity.
    pub volume: f32,
}

/// One sample zone in a soundsource's keymap — a faithful projection of the
/// sampler's `ZoneSpec` for the Mapping editor. A rectangle spanning
/// `key_min..=key_max` × `vel_min..=vel_max`; zones sharing that window and
/// differing by `rr_index` are round-robins (drawn stacked/striped).
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct SynthZone {
    pub file: String,
    pub key_min: u8,
    pub key_max: u8,
    pub root_key: u8,
    pub vel_min: u8,
    pub vel_max: u8,
    /// Round-robin slot (0-based) + mode ("" / "cycle" / "random" / "no-repeat-random").
    pub rr_index: u32,
    pub rr_mode: String,
    pub gain_db: f32,
    pub pan: f32,
    pub tune_cents: f32,
    /// Sustain loop window (`loop_end > loop_start` ⇒ loops).
    pub loop_start: u32,
    pub loop_end: u32,
    pub trigger_mode: String,
    /// Mic id (→ [`SynthMic::id`]; empty = mic-agnostic).
    pub mic: String,
    /// Articulation id (→ [`SynthArticulation::id`]).
    pub articulation: String,
    /// CC1 dynamic layer label (ppp..fff), if any.
    pub dynamic: String,
    /// Logical group id.
    pub group: String,
    /// Processed-copy variant label (e.g. "Mixed").
    pub variant: String,
}

/// A microphone position in a multi-mic soundsource.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct SynthMic {
    pub id: String,
    pub label: String,
    /// "blended" | "separate".
    pub kind: String,
    /// Auto-load on open.
    pub default: bool,
}

/// An articulation (Sustain, Staccato, …) grouping zones.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct SynthArticulation {
    pub id: String,
    pub label: String,
    /// ArticulationKind name (Sustain/Short/Legato/Release/…).
    pub kind: String,
    /// Round-robin count per note per dynamic.
    pub rr: u32,
}

/// A soundsource's full keymap for the Mapping editor — zones + the mic /
/// articulation / group vocabularies they reference.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct SynthMapping {
    /// Soundsource name (empty ⇒ no mapping / not found).
    pub name: String,
    /// Library vendor + instrument/category tags, for the header.
    pub vendor: String,
    pub zones: Vec<SynthZone>,
    pub mics: Vec<SynthMic>,
    pub articulations: Vec<SynthArticulation>,
    /// Distinct group ids present across the zones.
    pub groups: Vec<String>,
}

/// An ADSR envelope for the Edit page's interactive graph. `attack`/`decay`/
/// `release` are seconds; `sustain` is a level 0..1.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct SynthEnvelope {
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

/// A filter stage for the Edit page's response curve.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct SynthFilter {
    /// Display/model name ("Modified", "FATBOY", …).
    pub name: String,
    /// Cutoff in Hz (plot on a log axis).
    pub cutoff_hz: f32,
    /// Resonance 0..1.
    pub resonance: f32,
    /// Signed filter-envelope → cutoff depth (0..1 of the sweep range).
    pub env_depth: f32,
    /// "lowpass" | "highpass" | "bandpass" | "notch".
    pub mode: String,
    /// Pole count (slope).
    pub poles: u32,
}

/// One modulation route (source → target, signed depth).
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct SynthModRoute {
    pub source: String,
    pub target: String,
    pub depth: f32,
}

/// One layer (A/B/C/D) as a self-contained sub-instance, for the Edit page.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct SynthLayer {
    /// "Layer A".., and whether it's active in the patch.
    pub name: String,
    pub active: bool,
    /// The soundsource (Sample mode) or wavetable label driving this layer.
    pub source: String,
    /// Layer level 0..1.
    pub level: f32,
    /// The dual filter stages (usually 1–2).
    pub filters: Vec<SynthFilter>,
    pub amp_env: SynthEnvelope,
    pub filter_env: SynthEnvelope,
    /// Modulation routes scoped to this layer.
    pub routes: Vec<SynthModRoute>,
}

/// Omnisphere-style **Live global controls** — performance macros applied over
/// the loaded patch (all layers). Bipolar offset controls are neutral at `0.5`
/// (no change); unipolar controls are off at `0.0`. See
/// [`SynthGlobals::neutral`] for the default state.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct SynthGlobals {
    // Vibrato (unipolar).
    pub vibrato_rate: f32,
    pub vibrato_depth: f32,
    // Filter (bipolar offsets; `filter_env` is env-amount, unipolar-ish).
    pub filter_cutoff: f32,
    pub filter_reso: f32,
    pub filter_env: f32,
    // Unison (unipolar).
    pub unison_detune: f32,
    pub unison_amount: f32,
    // Amp envelope (bipolar A/D/S/R offsets + velocity sensitivity).
    pub amp_attack: f32,
    pub amp_decay: f32,
    pub amp_sustain: f32,
    pub amp_release: f32,
    /// Velocity → amplitude sensitivity. 0.5 = default, higher = more sensitive.
    pub amp_velocity: f32,
    // Filter envelope.
    pub filt_attack: f32,
    pub filt_decay: f32,
    pub filt_sustain: f32,
    pub filt_release: f32,
    pub filt_velocity: f32,
    // Ambience (reverb) — unipolar.
    pub ambience_amount: f32,
    pub ambience_length: f32,
    // Tone (3-band EQ, bipolar).
    pub tone_low: f32,
    pub tone_mid: f32,
    pub tone_high: f32,
    // Effects.
    pub effects_on: bool,
    /// Master limiter amount (unipolar).
    pub limiter: f32,
}

impl SynthGlobals {
    /// The neutral state — every control at its no-effect value.
    pub fn neutral() -> Self {
        Self {
            vibrato_rate: 0.0,
            vibrato_depth: 0.0,
            filter_cutoff: 0.5,
            filter_reso: 0.5,
            filter_env: 0.5,
            unison_detune: 0.0,
            unison_amount: 0.0,
            amp_attack: 0.5,
            amp_decay: 0.5,
            amp_sustain: 0.5,
            amp_release: 0.5,
            amp_velocity: 0.5,
            filt_attack: 0.5,
            filt_decay: 0.5,
            filt_sustain: 0.5,
            filt_release: 0.5,
            filt_velocity: 0.5,
            ambience_amount: 0.0,
            ambience_length: 0.5,
            tone_low: 0.5,
            tone_mid: 0.5,
            tone_high: 0.5,
            effects_on: true,
            limiter: 0.0,
        }
    }
}

pub mod synth {
    //! Live synth-rig control. `SynthRig` → `SynthRigClient` / `SynthRigService`
    //! / `synth_rig_serve`, plus the `#[subscribe]` stream sibling.

    use facet::Facet;

    use super::{
        BrowseItem, SynthGlobals, SynthLayer, SynthMapping, SynthNode, SynthPreset, SynthStatus,
    };

    /// One live rig change. Every variant carries full state (idempotent
    /// re-application) so a reconnecting subscriber is correct after the next
    /// event of each kind.
    #[derive(Clone, Debug, PartialEq, Facet)]
    #[repr(C)]
    pub enum SynthEvent {
        /// Transport + meters (published at meter rate while running).
        Status(SynthStatus),
        /// The available presets changed (library scan).
        Library(Vec<SynthPreset>),
        /// The loaded composition tree changed (its layer/block structure).
        Tree(SynthNode),
        /// Recent MIDI activity (oldest first), for the monitor.
        Midi(Vec<midicore_proto::MidiEvent>),
    }

    #[architect::rpc]
    pub trait SynthRig {
        /// (Re-)open the audio device. Returns immediately; open happens
        /// off-thread.
        fn start(&self);
        /// Close the audio device.
        fn stop(&self);
        /// Live transport + meter snapshot.
        fn status(&self) -> SynthStatus;
        /// Every preset in the library (the browser).
        fn presets(&self) -> Vec<SynthPreset>;
        /// Every browsable soundsource pack in the library — the faceted BROWSE
        /// view's data. All items (metadata only); the UI filters client-side.
        /// Scanned once and cached on the backend.
        fn browse(&self) -> Vec<BrowseItem>;
        /// Load preset `index` from [`presets`](Self::presets).
        fn load_preset(&self, index: u32);
        /// The loaded composition tree (layers → blocks).
        fn tree(&self) -> SynthNode;
        /// Trigger a note from the UI (velocity 0 = note-off).
        fn trigger(&self, note: u32, velocity: u32);
        /// Enumerate hardware MIDI input ports.
        fn midi_ports(&self) -> Vec<String>;
        /// Attach a hardware MIDI input by name (empty = omni / all inputs).
        fn set_midi_port(&self, name: String);
        /// Set the master output gain in milli-units (1000 = unity, 250 = −12 dB).
        fn set_volume(&self, gain_milli: u32);
        /// Recent MIDI events seen by the core (oldest first), for the monitor.
        fn midi_recent(&self) -> Vec<midicore_proto::MidiEvent>;

        /// The soundsource names referenced by the loaded preset's layers that
        /// resolve against the local library — the Mapping editor's subjects.
        fn soundsources(&self) -> Vec<String>;
        /// The keymap for one soundsource (empty name ⇒ the first of the loaded
        /// preset). Read cheaply from the pack header — no audio decode.
        fn mapping(&self, soundsource: String) -> SynthMapping;
        /// The loaded preset's layers (A/B/C/D) with their source / filter /
        /// envelope / modulation, for the Edit page's single-layer editor.
        fn layers(&self) -> Vec<SynthLayer>;
        /// The current Live global controls (filter/env/vibrato/unison/…).
        fn globals(&self) -> SynthGlobals;
        /// Set the Live global controls; applied over the loaded patch.
        fn set_globals(&self, globals: SynthGlobals);

        /// Every rig change, as it happens.
        #[subscribe]
        fn events(&self) -> SynthEvent;
    }
}

pub use synth::{SynthEvent, SynthRig};
