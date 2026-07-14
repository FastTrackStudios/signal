//! Live **Synth rig** — the vox-served software-synth rig behind the detachable
//! GUI. Sibling of `signal-keys`: where the keys rig plays Keyscape instruments,
//! the synth rig imports Omnisphere `.prt_omn` / `.mlt_omn` patches
//! ([`signal_synth::omni_import`]) into the shared composition-tree engine and
//! plays them from hardware MIDI or UI notes.
//!
//! The audio host itself is instrument-agnostic — the synth rig reuses
//! [`signal_keys::KeysRig`] (an output-only daw project hosting one composition
//! tree as a `PluginInstance`) rather than duplicating the engine plumbing. Only
//! the preset source (the Omnisphere library scan) and the wire service
//! ([`signal_synth_proto::synth::SynthRig`]) are synth-specific.

mod backend;
pub use backend::SynthRigBackend;
pub use signal_synth_proto as proto;
