//! Drums — the drum-kit *definition + test* layer over the shared sampler
//! engine.
//!
//! Drums need no drum-specific DSP: a kit is plain multi-sampled zones (with
//! round-robins + velocity layers) played by [`signal_sampler::SamplerRig`],
//! the same engine the strings and keys features use. This crate owns the
//! drum-kit *definitions* (how to load and route a kit), the behaviour *spec*
//! (`spec/`), and the kit-loading *tests* as they land.

use std::path::Path;

use signal_sampler::{InstrumentId, PreloadProfile, SamplerRig};

/// General-MIDI percussion channel (0-indexed 9 = MIDI channel 10).
pub const GM_DRUM_CHANNEL: u8 = 9;

/// Load a full drum kit from a `.signalpreset` (one engine per piece +
/// GM `note_routing` + the send-based multi-mic [`DrumMixer`]) and route it to
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
    rig.load_instrument_with_config(id, config.as_ref(), zones.as_ref(), root.as_ref(), section, mic)
        .map_err(|e| e.to_string())?;
    rig.set_solo_mic(id, Some(mic.to_string()));
    rig.set_midi_channel(id, GM_DRUM_CHANNEL);
    Ok(())
}
