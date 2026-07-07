//! Orchestra — the orchestral-strings *definition + test* layer over the shared
//! sampler engine.
//!
//! There is no separate orchestra DSP: the sampler engine
//! ([`signal_sampler::SamplerRig`]) already implements the CSS articulation
//! behaviour (legato / shorts / sustain, see `signal-sampler`'s `engine`
//! module). This crate owns the orchestra-specific *definitions* (how to load
//! and drive a strings library), its behaviour *spec* (`spec/`), and the A/B
//! *tests* that verify the engine reproduces a real Cinematic Studio Strings
//! render (`examples/`).

use std::path::Path;

use signal_sampler::SamplerRig;

/// Default install root for Cinematic Studio Strings (per-machine; override in
/// the caller). The A/B examples use this unless a path is passed.
pub const CSS_ROOT: &str =
    "/run/media/AudioHaven/Sampled/Orchestral/Cinematic Series/Cinematic Studio Strings";

/// The articulation-config styx that maps CSS zones/articulations onto the
/// engine (lives in the sample-collector repo).
pub const CSS_CONFIG: &str =
    "/run/media/Development/FastTrackStudio/sample-collector/specs/cinematic-strings.styx";

/// CSS "Arco attack" ships at `$mmirg = 30/127`; Kontakt's cubic
/// `ENGINE_PAR_ATTACK` makes that ≈ 198 ms — the sustain bloom the engine
/// applies to sustain-layer voices.
pub const CSS_ATTACK_MS: u32 = 198;
/// `$tukcw` — the note-off overlap fade.
pub const CSS_RELEASE_MS: u32 = 400;

/// Load a CSS strings section (e.g. `"1st Violins"`) into `rig` under `id`,
/// wired with the engine settings that match a real CSS-in-Kontakt render
/// (solo mic, arco-attack bloom, release overlap). This is the orchestra
/// feature's core *definition*: everything the engine needs to sound like CSS.
///
/// `css_root` is the library install dir; `config` is the articulation styx.
// r[impl orchestra.load.css-match]
// r[impl orchestra.load.section-zones]
pub fn load_strings(
    rig: &SamplerRig,
    id: &str,
    section: &str,
    mic: &str,
    css_root: impl AsRef<Path>,
    config: impl AsRef<Path>,
) -> Result<(), String> {
    let css_root = css_root.as_ref();
    let zones = css_root.join("_patches").join(section).join("library.styx");
    rig.load_instrument_with_config(id, config.as_ref(), &zones, css_root, section, mic)
        .map_err(|e| e.to_string())?;
    rig.set_solo_mic(id, Some(mic.to_string()));
    rig.set_midi_channel(id, 0);
    rig.set_attack_ms(id, CSS_ATTACK_MS);
    rig.set_release_ms(id, CSS_RELEASE_MS);
    Ok(())
}
