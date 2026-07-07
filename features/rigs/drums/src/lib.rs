//! Drums — the drum-kit *definition + test* layer over the shared sampler
//! engine.
//!
//! Drums need no drum-specific DSP: a kit is plain multi-sampled zones (with
//! round-robins + velocity layers) played by [`signal_sampler::SamplerRig`],
//! the same engine the strings and keys features use. This crate owns the
//! drum-kit *definitions* (how to load and route a kit), the behaviour *spec*
//! (`spec/`), and the kit-loading *tests* as they land.

use std::path::Path;

use signal_sampler::SamplerRig;

/// General-MIDI percussion channel (0-indexed 9 = MIDI channel 10).
pub const GM_DRUM_CHANNEL: u8 = 9;

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
