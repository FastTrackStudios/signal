//! Drums — the drum-kit *definition + test* layer over the shared sampler
//! engine.
//!
//! Drums need no drum-specific DSP: a kit is plain multi-sampled zones (with
//! round-robins + velocity layers) played by [`signal_sampler::SamplerRig`],
//! the same engine the strings and keys features use. This crate owns the
//! drum-kit *definitions* (how to load and route a kit), the behaviour *spec*
//! (`spec/`), and the kit-loading *tests* as they land.

use std::path::Path;

use signal_sampler::midicore::{self, DrumMap, DrumMapConverter};
use signal_sampler::{InstrumentId, PreloadProfile, PresetSpec, SamplerRig};

mod backend;
pub mod cradle;
pub mod library;
mod lightguide;
pub mod mm2fx;
pub mod piece_space;
pub use backend::DrumRigBackend;
pub use lightguide::DrumLightGuide;
pub use signal_drums_proto as proto;

/// General-MIDI percussion channel (0-indexed 9 = MIDI channel 10).
pub const GM_DRUM_CHANNEL: u8 = 9;

/// Open a hardware drum controller and play a loaded kit through it, running
/// every event through a [`DrumMapConverter`] so a `from`-mapped kit (e.g. an
/// Alesis Strata Prime e-kit, [`DrumMap::StrataPrime`]) drives the loaded
/// sample library's note layout (e.g. [`DrumMap::Mm2`]).
///
/// Returns the live [`MidiInput`](signal_sampler::MidiInputHandle) — hold it
/// alive for as long as the kit should play. Requires a live (non-offline) rig.
pub fn attach_converted_kit(
    rig: &SamplerRig,
    selection: midicore::PortSelector,
    from: DrumMap,
    to: DrumMap,
) -> Result<signal_sampler::MidiInputHandle, String> {
    let mut conv = DrumMapConverter::new(from, to);
    rig.attach_midi_transformed(selection, move |ev| conv.convert(ev))
        .map_err(|e| e.to_string())
}

/// Load a full drum kit from a `.signalpreset` (one engine per piece +
/// GM `note_routing` + the send-based multi-mic `DrumMixer`) and route it to
/// the GM percussion channel. This is the loader for the native GGD-style kits
/// (e.g. Modern & Massive 2), whose `.signalpreset` files already carry the
/// note map and per-piece mic sets — unlike [`load_kit`], which loads a single
/// merged articulation/zone library.
///
/// Returns the per-engine instrument ids (`"<id>:<piece>"`).
///
/// NB: [`SamplerRig::note_on`] dispatches on MIDI channel 0 and drops notes for
/// unmapped channels, so callers driving the kit with `note_on` should either
/// map it to channel 0 or send `midi_message` on [`GM_DRUM_CHANNEL`]. This
/// loader maps [`GM_DRUM_CHANNEL`]; a hardware/e-kit feed on MIDI channel 10
/// therefore plays without extra wiring.
// r[impl drums.kit.gm-channel]
// r[impl drums.kit.sample-zones]
/// Load a `.signalpreset` kit as per-track daw tracks (the fully daw-based
/// mixer): parse the preset, then hand it to
/// [`SamplerRig::load_kit_tracks`]. MIDI reaches the kit through its routing
/// table ([`SamplerRig::kit_dispatch`]), not a bank channel.
pub fn load_kit_tracks(
    rig: &SamplerRig,
    id: &str,
    preset: impl AsRef<Path>,
) -> Result<Vec<InstrumentId>, String> {
    rig.set_preload_profile(PreloadProfile::DrumKit);
    let path = preset.as_ref();
    let spec = PresetSpec::from_file(path).map_err(|e| e.to_string())?;
    let dir = path.parent().unwrap_or(Path::new("")).to_path_buf();
    rig.load_kit_tracks(id, &spec, &dir)
        .map_err(|e| e.to_string())
}

/// As [`load_kit_tracks`] from an in-memory spec (the kit-designer swap path).
pub fn load_kit_tracks_spec(
    rig: &SamplerRig,
    id: &str,
    spec: &PresetSpec,
    dir: &Path,
) -> Result<Vec<InstrumentId>, String> {
    rig.set_preload_profile(PreloadProfile::DrumKit);
    rig.load_kit_tracks(id, spec, dir)
        .map_err(|e| e.to_string())
}

pub fn load_preset_kit(
    rig: &SamplerRig,
    id: &str,
    preset: impl AsRef<Path>,
) -> Result<Vec<InstrumentId>, String> {
    rig.set_preload_profile(PreloadProfile::DrumKit);
    let ids = rig
        .load_preset(id, preset.as_ref())
        .map_err(|e| e.to_string())?;
    rig.set_midi_channel(id, GM_DRUM_CHANNEL);
    // React to all MIDI: any channel that isn't explicitly mapped (e.g. a
    // keyboard on ch 1, not GM drums on ch 10) falls back to the kit.
    rig.set_default_instrument(id);
    Ok(ids)
}

/// Load a drum-kit sample library into `rig` under `id` and route it to the
/// GM percussion channel. A kit is ordinary engine zones — this is a thin
/// definition over the shared sampler loader, the drums analogue of the
/// orchestra `load_strings` and keys rig definitions.
///
/// `config` is the articulation/zone styx, `zones` the resolved `library.styx`,
/// `root` the samples root, `section`/`mic` the kit + mic to solo.
// r[impl drums.kit.gm-channel]
// r[impl drums.kit.sample-zones]
pub fn load_kit(
    rig: &SamplerRig,
    id: &str,
    config: impl AsRef<Path>,
    zones: impl AsRef<Path>,
    root: impl AsRef<Path>,
    section: &str,
    mic: &str,
) -> Result<(), String> {
    rig.load_instrument_with_config(
        id,
        config.as_ref(),
        zones.as_ref(),
        root.as_ref(),
        section,
        mic,
    )
    .map_err(|e| e.to_string())?;
    rig.set_solo_mic(id, Some(mic.to_string()));
    rig.set_midi_channel(id, GM_DRUM_CHANNEL);
    Ok(())
}
