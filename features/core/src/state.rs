//! Signal's state: one blob, the same in every host.
//!
//! A [`SignalState`] is what an instance saves (the CLAP's state chunk, a
//! track's state in Studio's session): its [`Instance`] mode and the
//! [`Preset`] it plays. The preset is the whole rig or kit; one preset
//! serves both modes:
//!
//! - **multi-channel**: one instance runs every lane, each fed from its
//!   own input channels and summed into its output channels;
//! - **per-track**: the instance on a track is a port of a shared
//!   [`session`](crate::session) and runs only the lanes of its port
//!   (`Kick/In`), fed from the track's own channels. The kit's instances
//!   share the preset, its macros and its hits through the session; the
//!   host routes and sums the audio.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use serde::{Deserialize, Serialize};

use crate::block::BlockKind;
use crate::host::AssetRef;
use crate::platform::Platforms;
use crate::session::SessionLink;

/// The state format's version. A newer state than this build reads is
/// refused, not guessed at.
pub const STATE_VERSION: u32 = 1;

/// What an instance saves and loads.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SignalState {
    pub version: u32,
    #[serde(default)]
    pub instance: Instance,
    pub preset: Preset,
}

/// How this instance runs the preset.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "kebab-case")]
pub enum Instance {
    /// One instance, every lane: the multi-channel mode.
    #[default]
    MultiChannel,
    /// One port of a session shared by the instances on a kit's tracks.
    Port(SessionLink),
}

/// A rig, a chain or a kit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    /// Input channels the multi-channel mode takes.
    #[serde(default = "two")]
    pub inputs: u16,
    /// Output channels the multi-channel mode gives.
    #[serde(default = "two")]
    pub outputs: u16,
    pub lanes: Vec<LaneSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub macros: Vec<MacroSpec>,
}

const fn two() -> u16 {
    2
}

/// A chain from some input channels to some output channels, or into
/// another lane (a bus).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LaneSpec {
    pub name: String,
    /// The port this lane belongs to in the per-track mode (`Kick/In`):
    /// the instance on that track runs it. `None`: every port's instance
    /// that has no lane of its own runs it (a plain chain or rig).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<String>,
    /// Input channels, 0-based (multi-channel mode). One is mono, fed to
    /// both sides of the chain; two are a stereo pair; none, an instrument
    /// lane that only plays from MIDI. In the per-track mode a lane takes
    /// the track's first one or two channels instead.
    #[serde(default)]
    pub input: Vec<u16>,
    /// First output channel (the pair `output`, `output + 1`), when the
    /// lane goes to the outputs rather than to another lane (`to`).
    #[serde(default)]
    pub output: u16,
    /// The lane this one feeds instead of the outputs: a bus (`Kick`,
    /// `Drums`). A lane fed by others sums them into its own input.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// Copies of this lane into other lanes, beside where it goes: the
    /// parallel compressors, the reverb returns, a Sub fed by the Sum.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sends: Vec<SendSpec>,
    /// Detect drum hits on this lane's input (a `Trig` mic): blocks keyed
    /// to this lane are played by them (a Sample) and keyed by its audio
    /// (a Gate).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<TriggerSpec>,
    #[serde(default)]
    pub gain_db: f32,
    #[serde(default)]
    pub mute: bool,
    pub blocks: Vec<BlockSpec>,
    /// Each phase's gain-neutral make-up (measured: `Graph::phase_levels`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trims: Vec<PhaseTrim>,
}

/// A send from a lane to another.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SendSpec {
    /// The lane it feeds.
    pub to: String,
    #[serde(default)]
    pub gain_db: f32,
    /// Taken before the lane's chain (the raw mics into a Sub), rather
    /// than after it (the default: what the lane sounds like).
    #[serde(default)]
    pub pre: bool,
}

impl SendSpec {
    #[must_use]
    pub fn to(lane: impl Into<String>, gain_db: f32) -> Self {
        Self {
            to: lane.into(),
            gain_db,
            pre: false,
        }
    }

    #[must_use]
    pub fn pre(mut self) -> Self {
        self.pre = true;
        self
    }
}

/// Hit detection on a lane.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TriggerSpec {
    /// Above this (dBFS) a transient is a hit.
    #[serde(default = "default_trigger_db")]
    pub threshold_db: f32,
    /// A MIDI note that is also a hit of this lane: the host's hit
    /// markers placed as MIDI (Studio's `HITS_MIDI`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<u8>,
}

const fn default_trigger_db() -> f32 {
    -30.0
}

/// The mix phase a block belongs to: the pass of the mix it is part of,
/// in the order a chain runs them (Studio's `session::mix_phases`, by the
/// same slugs). Balance (gain staging and the faders) has no blocks, so
/// it isn't one here.
///
/// A lane's blocks run phase by phase, each phase's blocks together, and
/// each phase is gain neutral: its [`PhaseTrim`] brings it back to the
/// level it was given, so the chain sounds as loud on as bypassed and the
/// faders do the balance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    /// Make the signal usable: the live Rescue EQ, a trigger-keyed gate
    /// (the offline work, phase, alignment, consistency compression, is
    /// rendered before Signal sees it).
    Rescue,
    /// The sound of the track: stylized compression (groove, punch,
    /// thickness), tonal EQ, saturation, parallel filters, parallel
    /// compression.
    Tone,
    /// Take off what Tone exposed: de-essing, resonance suppression,
    /// clipping.
    Polish,
    /// How tracks sit against each other: relational EQ, separation,
    /// sidechain dynamics.
    Relational,
    /// Front to back: panning, reverb, delay, ambience.
    Depth,
    /// The choices: throws, creative filters, sound design.
    Creative,
}

impl Phase {
    /// Every phase with blocks, in the order a chain runs them.
    pub const ALL: [Self; 6] = [
        Self::Rescue,
        Self::Tone,
        Self::Polish,
        Self::Relational,
        Self::Depth,
        Self::Creative,
    ];

    /// Studio's slug for it.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Rescue => "rescue",
            Self::Tone => "tone",
            Self::Polish => "polish",
            Self::Relational => "relational",
            Self::Depth => "depth",
            Self::Creative => "creative",
        }
    }
}

/// A phase's make-up: what brings it back to the level it was given.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhaseTrim {
    pub phase: Phase,
    pub gain_db: f32,
}

/// One block in a lane.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BlockSpec {
    /// Unique in its lane: what macros and actions address.
    pub id: String,
    pub kind: BlockKind,
    /// Which of its family's algorithms the block runs (`ssl-e`, `plate`,
    /// `tape`): what its parameters are. `None`: the family's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub algorithm: Option<String>,
    /// Plain values by parameter id (the algorithm's controls); a missing
    /// one is its default.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, f32>,
    /// The lane this block listens to: a Gate's key (the piece's `Trig`),
    /// a Sample's hits (the lane whose trigger plays it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// The file it needs (a NAM capture, a cab IR).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<AssetRef>,
    /// State that isn't parameters, base64: a third-party plugin's own
    /// blob. Signal's blocks keep all theirs in `params` (one schema for
    /// the compiled-in block and its CLAP), so theirs is `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// Switched off by the player.
    #[serde(default)]
    pub bypass: bool,
    /// Where the preset says this block needs to run, narrowing what its
    /// kind can do ("Valhalla Supermassive 2.x, desktop"). `None`: wherever
    /// its kind runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires: Option<Platforms>,
    /// Built instead where this block can't be: one of Signal's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<Box<BlockSpec>>,
    /// The mix phase it belongs to. `None`: a lane outside the mix
    /// method (a rig, an instrument), whose blocks run as they are.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<Phase>,
}

impl BlockSpec {
    /// One of Signal's own blocks, with its defaults.
    #[must_use]
    pub fn signal(id: impl Into<String>, block: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind: BlockKind::Signal(block.into()),
            algorithm: None,
            params: BTreeMap::new(),
            key: None,
            asset: None,
            state: None,
            bypass: false,
            requires: None,
            fallback: None,
            phase: None,
        }
    }

    /// In mix phase `phase`.
    #[must_use]
    pub const fn phase(mut self, phase: Phase) -> Self {
        self.phase = Some(phase);
        self
    }

    /// Listen to lane `lane` (a gate's key, a sample's hits).
    #[must_use]
    pub fn key(mut self, lane: impl Into<String>) -> Self {
        self.key = Some(lane.into());
        self
    }

    /// Run algorithm `id` of the block's family.
    #[must_use]
    pub fn algorithm(mut self, id: impl Into<String>) -> Self {
        self.algorithm = Some(id.into());
        self
    }

    #[must_use]
    pub fn param(mut self, id: impl Into<String>, value: f32) -> Self {
        self.params.insert(id.into(), value);
        self
    }

    #[must_use]
    pub fn asset(mut self, asset: AssetRef) -> Self {
        self.asset = Some(asset);
        self
    }

    #[must_use]
    pub fn fallback(mut self, fallback: Self) -> Self {
        self.fallback = Some(Box::new(fallback));
        self
    }
}

/// A macro knob: one of the instance's macro parameters, driving block
/// parameters by id (never a block's internals), so it works the same on
/// a compiled-in block and on its CLAP.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MacroSpec {
    pub name: String,
    pub targets: Vec<MacroTarget>,
}

/// One parameter a macro moves.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MacroTarget {
    /// The lane, by name.
    pub lane: String,
    /// The block, by id.
    pub block: String,
    /// The parameter, by id.
    pub param: String,
    #[serde(default)]
    pub mode: MacroMode,
    /// `Set`: the plain value at macro 0 and at macro 1 (either way
    /// round). `Modulate`: the normalized offset at macro 0 and at 1.
    pub min: f32,
    pub max: f32,
}

/// How a macro drives its parameter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MacroMode {
    /// It sets the parameter's value.
    #[default]
    Set,
    /// It offsets the parameter's own value, which is kept
    /// (`CLAP_EVENT_PARAM_MOD` on a CLAP backend).
    Modulate,
}

/// Why a state blob was refused.
#[derive(Debug)]
pub enum StateError {
    /// Not a Signal state.
    Parse(String),
    /// Written by a newer Signal than this one.
    TooNew { found: u32, supported: u32 },
}

impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(why) => write!(f, "not a Signal state: {why}"),
            Self::TooNew { found, supported } => {
                write!(
                    f,
                    "state version {found} is newer than this Signal reads ({supported})"
                )
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for StateError {}

impl SignalState {
    /// A multi-channel instance of `preset`.
    #[must_use]
    pub const fn new(preset: Preset) -> Self {
        Self {
            version: STATE_VERSION,
            instance: Instance::MultiChannel,
            preset,
        }
    }

    /// The blob: JSON, so a preset is readable and diffable.
    #[must_use]
    pub fn to_blob(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// Read a blob [`to_blob`](Self::to_blob) wrote.
    ///
    /// # Errors
    ///
    /// [`StateError`] if it isn't one, or is newer than this build.
    pub fn from_blob(blob: &[u8]) -> Result<Self, StateError> {
        let state: Self = serde_json::from_slice(blob).map_err(|e| {
            let mut why = String::new();
            let _ = fmt::write(&mut why, format_args!("{e}"));
            StateError::Parse(why)
        })?;
        if state.version > STATE_VERSION {
            return Err(StateError::TooNew {
                found: state.version,
                supported: STATE_VERSION,
            });
        }
        Ok(state)
    }
}

impl Preset {
    /// Stereo in, stereo out, nothing in between.
    #[must_use]
    pub fn passthrough() -> Self {
        Self {
            name: "Passthrough".into(),
            inputs: 2,
            outputs: 2,
            lanes: alloc::vec![LaneSpec {
                name: "Main".into(),
                port: None,
                input: alloc::vec![0, 1],
                output: 0,
                gain_db: 0.0,
                mute: false,
                blocks: Vec::new(),
                ..LaneSpec::default()
            }],
            macros: Vec::new(),
        }
    }
}
