//! Block implementation discriminator (orthogonal to `BlockType`).
//!
//! [`BlockType`](crate::block::BlockType) captures *what a block does
//! semantically* — Amp, Drive, Reverb, EQ. [`BlockKind`] captures *how
//! that role is realized* — Native (built-in DSP), Neural Amp Modeler
//! (a `.nam` model), a hosted CLAP/VST3 plugin, or a custom backend.
//!
//! The two axes are independent: an Amp block can be `Native` (classic
//! waveshaper) or `Nam` (neural network) or `HostedPlugin` (loaded from
//! a third-party amp sim); a Drive block can be `Nam` (a neural-modeled
//! overdrive pedal) just as easily. The runtime FX backend
//! (`signal-sampler::mixer::FxBackend`) matches one-to-one with this
//! enum's variants — `BlockKind` is the persisted form, `FxBackend` is
//! the audio-thread instance.
//!
//! Defaults to [`BlockKind::Native`] so a block that does not say how it is
//! realized reads as built-in DSP.
//!
//! # Struct variants, not tuple variants
//!
//! Every non-`Native` variant is a struct variant carrying one named field.
//! That is not a style choice: facet-styx writes a newtype tuple variant in a
//! form it cannot read back (`@Nam"…"`), so a `BlockKind::Nam(NamRef)` could
//! not survive a round trip through the format the live rig stores its
//! library in — a NAM amp was unwritable. The same limitation already forced
//! `NodePathSegment` and `NodeOverrideOp` into struct variants.

use facet::Facet;
use serde::{Deserialize, Serialize};

/// How a block's DSP is realized at runtime.
///
/// Serialized with `#[serde(tag = "kind", content = "data")]` so a missing
/// field deserializes as [`BlockKind::Native`] (back-compat) and the
/// non-Native variants get a clean tagged-union JSON layout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Facet)]
#[serde(tag = "kind", content = "data")]
#[repr(C)]
#[derive(Default)]
pub enum BlockKind {
    /// Built-in DSP for this block type. Default.
    #[default]
    Native,
    /// Neural Amp Modeler — a `.nam` model file processed by
    /// `neural-amp-modeler` (FFI to `NeuralAmpModelerCore`). Works for any
    /// nonlinear/amplifier-shaped block (Amp, Drive, Cabinet, …).
    Nam { model: NamRef },
    /// Third-party CLAP / VST3 plugin loaded from disk.
    HostedPlugin { plugin: HostedPluginRef },
    /// Sampled playback from a sample-library spec — a Keyscape piano, an
    /// Omnisphere soundsource, a drum kit, an orchestral section.
    ///
    /// The kind the domain was missing, and the reason the keys rig could
    /// not be expressed as nodes: its every lane is a sampler, and a `Node`
    /// had no way to say so. A sample realization is not a plugin and not a
    /// neural model; it is a spec file plus which part of it to play.
    Sample { sample: SampleRef },
    /// Caller-defined backend identified by a string id; lookup happens
    /// in a host-supplied registry. Lets plugin authors add their own
    /// kinds without growing this enum.
    Custom { custom: CustomRef },
}

impl BlockKind {
    /// Short identifier for the variant — used in UI tags and log lines.
    #[must_use]
    pub const fn tag(&self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Nam { .. } => "nam",
            Self::HostedPlugin { .. } => "plugin",
            Self::Sample { .. } => "sample",
            Self::Custom { .. } => "custom",
        }
    }
}

// ─── Soundsource kind ───────────────────────────────────────────

/// Which kind of **generator** a layer's source is — the wire-visible
/// classification of a `Soundsource`.
///
/// The pluggable generator inside an instrument layer; see
/// `docs/spec/signal/soundsource.md` and
/// `features/sampler/signal-sampler/src/soundsource.rs`.
/// A third axis beside [`BlockType`](crate::block::BlockType) (semantic
/// role) and [`BlockKind`] (how the DSP is realized): `SoundsourceKind`
/// says what *generates* the sound in a source slot, so remotes can show
/// a source picker / per-kind editor without knowing the concrete engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum SoundsourceKind {
    /// Analog / wavetable synthesis (unison, FM, ring, harmonia).
    Oscillator,
    /// Sampled multisample playback (zone maps, round-robins, mics, loops) —
    /// Keyscape, Omnisphere soundsources, drum kits, orchestral libraries.
    Sample,
    /// Physically-modeled instrument — an excitation (hammer/bow/pluck/breath)
    /// driving a resonant model (strings, body/soundboard); may be
    /// sample-excited/hybrid.
    PhysicalModel,
    /// Live audio / file input as the layer's source — the guitar-DI case,
    /// plus cinematic beds, one-shots, and granular fodder.
    Audio,
}

impl SoundsourceKind {
    /// Short identifier — used in UI tags, styx files, and log lines.
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Oscillator => "oscillator",
            Self::Sample => "sample",
            Self::PhysicalModel => "physical-model",
            Self::Audio => "audio",
        }
    }

    /// Human-readable name for pickers.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Oscillator => "Oscillator",
            Self::Sample => "Sample",
            Self::PhysicalModel => "Physical Model",
            Self::Audio => "Audio",
        }
    }

    /// Parse a [`tag`](Self::tag) back into the kind.
    #[must_use]
    pub fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "oscillator" => Some(Self::Oscillator),
            "sample" => Some(Self::Sample),
            "physical-model" => Some(Self::PhysicalModel),
            "audio" => Some(Self::Audio),
            _ => None,
        }
    }

    /// All kinds in display order (for source pickers).
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[
            Self::Oscillator,
            Self::Sample,
            Self::PhysicalModel,
            Self::Audio,
        ]
    }
}

impl crate::block::BlockType {
    /// Classify a block type as a generator: the [`SoundsourceKind`] its
    /// source-slot backend renders as, or `None` for processors /
    /// modulators / utilities that are not generators.
    ///
    /// Mirrors the signal-sampler native registry: `Oscillator`/`Wavetable`
    /// are the Oscillator soundsources, `Sampler` the Sample soundsource,
    /// `Harmonic` (City Grand waveguide) / `Formant` (City Wurli) the
    /// physically-modeled ones, and `Input` is the layer's live-audio
    /// source (the guitar DI).
    #[must_use]
    pub const fn soundsource_kind(self) -> Option<SoundsourceKind> {
        match self {
            Self::Oscillator | Self::Wavetable => Some(SoundsourceKind::Oscillator),
            Self::Sampler => Some(SoundsourceKind::Sample),
            Self::Harmonic | Self::Formant => Some(SoundsourceKind::PhysicalModel),
            Self::Input => Some(SoundsourceKind::Audio),
            _ => None,
        }
    }
}

/// Reference to a `.nam` model file. `model_id` is an optional stable id
/// (URL or hash) for content-addressed lookups; absent means "use the
/// path as the id".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Facet)]
pub struct NamRef {
    pub model_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
}

/// Reference to a hosted plugin slot.
///
/// `format` matches `daw::plugin::PluginFormat`'s variants by name
/// (Clap / Vst3 / Lv2) and is stored as a string so a preset survives
/// format additions without a schema bump.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Facet)]
pub struct HostedPluginRef {
    pub format: String,
    pub path: String,
    /// Optional per-plugin state blob (CLAP / VST3 state chunk) saved by
    /// the host's `save_state`. Base64 or raw — host decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_b64: Option<String>,
}

/// Reference to a sample library and the part of it a block plays.
///
/// Everything but `spec_path` is optional in the sense that empty means "the
/// obvious one" — the spec's first section, its first mic, the spec file's
/// own directory. A single-instrument library therefore needs only the path,
/// while an orchestral library that holds sixty sections and five mic
/// positions can name exactly one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Facet)]
pub struct SampleRef {
    /// Path to the library spec — a `library.styx` or a `.signalpack`.
    pub spec_path: String,
    /// Root the spec's WAV/zone paths resolve against. Empty ⇒ the spec
    /// file's parent directory.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub samples_root: String,
    /// Section id inside the library (e.g. `"1v"`). Empty ⇒ the first.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub section: String,
    /// Mic position id (e.g. `"Mix"`). Empty ⇒ the first.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mic: String,
}

/// Caller-defined backend reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Facet)]
pub struct CustomRef {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_native() {
        assert!(matches!(BlockKind::default(), BlockKind::Native));
        assert_eq!(BlockKind::default().tag(), "native");
    }

    #[test]
    fn json_round_trips_through_nam() {
        let k = BlockKind::Nam {
            model: NamRef {
                model_path: "/models/dumble.nam".into(),
                model_id: Some("sha256:abcd".into()),
            },
        };
        let j = serde_json::to_string(&k).unwrap();
        let back: BlockKind = serde_json::from_str(&j).unwrap();
        assert_eq!(k, back);
    }

    #[test]
    fn soundsource_kind_tags_round_trip() {
        for &k in SoundsourceKind::all() {
            assert_eq!(SoundsourceKind::from_tag(k.tag()), Some(k));
        }
        assert_eq!(SoundsourceKind::from_tag("granular"), None);
    }

    #[test]
    fn soundsource_kind_serde_round_trips() {
        for &k in SoundsourceKind::all() {
            let j = serde_json::to_string(&k).unwrap();
            let back: SoundsourceKind = serde_json::from_str(&j).unwrap();
            assert_eq!(k, back);
        }
    }

    #[test]
    fn block_types_classify_as_generators() {
        use crate::block::BlockType as T;
        assert_eq!(
            T::Oscillator.soundsource_kind(),
            Some(SoundsourceKind::Oscillator)
        );
        assert_eq!(
            T::Wavetable.soundsource_kind(),
            Some(SoundsourceKind::Oscillator)
        );
        assert_eq!(T::Sampler.soundsource_kind(), Some(SoundsourceKind::Sample));
        assert_eq!(
            T::Formant.soundsource_kind(),
            Some(SoundsourceKind::PhysicalModel)
        );
        assert_eq!(
            T::Harmonic.soundsource_kind(),
            Some(SoundsourceKind::PhysicalModel)
        );
        assert_eq!(T::Input.soundsource_kind(), Some(SoundsourceKind::Audio));
        // Processors are not generators.
        assert_eq!(T::Amp.soundsource_kind(), None);
        assert_eq!(T::Reverb.soundsource_kind(), None);
        assert_eq!(T::Lfo.soundsource_kind(), None);
    }

    #[test]
    fn json_round_trips_through_hosted() {
        let k = BlockKind::HostedPlugin {
            plugin: HostedPluginRef {
                format: "Clap".into(),
                path: "/usr/lib/clap/NAMVoyager.clap".into(),
                state_b64: None,
            },
        };
        let j = serde_json::to_string(&k).unwrap();
        let back: BlockKind = serde_json::from_str(&j).unwrap();
        assert_eq!(k, back);
    }

    /// The round trip the tuple variants could not do, and the reason they
    /// are gone: the live rig keeps its library as styx, so a NAM amp that
    /// cannot be written in that format cannot be saved at all.
    #[test]
    fn styx_round_trips_every_realization() {
        let kinds = [
            BlockKind::Native,
            BlockKind::Nam {
                model: NamRef {
                    model_path: "tone3000/1234/ac30.nam".into(),
                    model_id: Some("5678".into()),
                },
            },
            BlockKind::HostedPlugin {
                plugin: HostedPluginRef {
                    format: "Clap".into(),
                    path: "/usr/lib/clap/x.clap".into(),
                    state_b64: Some("AAAA".into()),
                },
            },
            BlockKind::Sample {
                sample: SampleRef {
                    spec_path: "keyscape/library.styx".into(),
                    section: "1v".into(),
                    mic: "Mix".into(),
                    ..SampleRef::default()
                },
            },
            BlockKind::Custom {
                custom: CustomRef {
                    id: "granular".into(),
                    config: None,
                },
            },
        ];
        // Wrapped in a struct because a styx document's root is a map — a
        // bare enum has nowhere to go. That is also how a kind is really
        // stored: as one field of a block.
        #[derive(Debug, PartialEq, Facet)]
        struct Holder {
            kind: BlockKind,
        }

        for kind in kinds {
            let tag = kind.tag();
            let holder = Holder { kind };
            let text = facet_styx::to_string(&holder).expect("write");
            let back: Holder = facet_styx::from_str(&text)
                .unwrap_or_else(|e| panic!("{tag} did not read back: {e}\n{text}"));
            assert_eq!(holder, back, "{tag}");
        }
    }
}
