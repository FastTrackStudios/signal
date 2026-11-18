//! Fretted — sampled guitars and basses over the shared sampler engine.
//!
//! A library here is a decoded Kontakt instrument (Impact Soundworks'
//! Shreddage 3.5 line first) played by [`sampler::SamplerRig`]: its zones
//! carry the string they were recorded on, and the engine's fretboard stage
//! places each note on a string and fret the way the library's script does.
//! This crate owns the definitions — where each library is installed and
//! which spec drives it — and the null tests against Kontakt renders.

use std::path::{Path, PathBuf};

use sampler::SamplerRig;

/// A sampled fretted library: where it is installed and which spec drives
/// it. One of its instruments is the patch folder
/// `<root>/_patches/<instrument>/` (`library.styx` zones, `section.styx`
/// overlay), so a new library is a row in [`LIBRARIES`], never engine code.
#[derive(Debug, Clone, Copy)]
pub struct Library {
    /// Short name (`abyss`, `argent`, `darkwall`).
    pub name: &'static str,
    /// Install root: the folder holding `_patches/` and the sample folders.
    pub root: &'static str,
    /// The library spec, relative to the checkout.
    pub spec: &'static str,
}

impl Library {
    /// The library spec's path in this checkout.
    #[must_use]
    pub fn spec_path(&self) -> PathBuf {
        utils::paths::checkout_path(self.spec)
    }

    /// One instrument's zone file.
    #[must_use]
    pub fn zones_path(&self, instrument: &str) -> PathBuf {
        Path::new(self.root)
            .join("_patches")
            .join(instrument)
            .join("library.styx")
    }
}

/// The libraries this crate can load.
pub const LIBRARIES: &[Library] = &[
    Library {
        name: "abyss",
        root: "/run/media/AudioHaven/Sampled/Guitar/Shreddage Abyss",
        spec: "features/rigs/fretted/specs/shreddage-abyss.styx",
    },
    Library {
        name: "argent",
        root: "/run/media/AudioHaven/Sampled/Guitar/Shreddage Argent",
        spec: "features/rigs/fretted/specs/shreddage-argent.styx",
    },
    Library {
        name: "darkwall",
        root: "/run/media/AudioHaven/Sampled/Guitar/Shreddage Darkwall",
        spec: "features/rigs/fretted/specs/shreddage-darkwall.styx",
    },
];

/// The library called `name` (see [`LIBRARIES`]).
#[must_use]
pub fn library(name: &str) -> Option<&'static Library> {
    LIBRARIES.iter().find(|l| l.name.eq_ignore_ascii_case(name))
}

/// Parse one instrument of `library` once, for many rigs: the patch carries
/// a shared sample cache, so every rig [`install_patch`] puts it in reads
/// the same decoded samples (an offline renderer running many takes).
///
/// # Errors
///
/// Returns an error if the spec or the instrument's zones cannot be loaded.
pub fn load_patch(library: &Library, instrument: &str) -> Result<sampler::PlayerPatch, String> {
    let mut patch = sampler::PlayerPatch::load_merged(
        &library.spec_path(),
        &library.zones_path(instrument),
        Path::new(library.root),
    )
    .map_err(|e| e.to_string())?;
    patch.shared_cache = Some(sampler::engine::cache::SampleCache::with_prepared(
        patch.prepared_cache_dir.as_deref(),
    ));
    Ok(patch)
}

/// Install a [`load_patch`] patch into `rig` under `id`, playing `mic` only.
///
/// # Errors
///
/// Returns an error if the rig's bank is poisoned.
pub fn install_patch(
    rig: &SamplerRig,
    id: &str,
    patch: sampler::PlayerPatch,
    instrument: &str,
    mic: &str,
) -> Result<(), String> {
    rig.load_patch(id, patch, "library", instrument, mic)
        .map_err(|e| e.to_string())?;
    rig.set_solo_mic(id, Some(mic.to_string()));
    rig.set_midi_channel(id, 0);
    Ok(())
}

/// Load one instrument of `library` into `rig` under `id`, playing `mic`
/// (the pickup or DI channel) only.
///
/// # Errors
///
/// Returns an error if the spec or the instrument's zones cannot be loaded.
pub fn load_instrument(
    rig: &SamplerRig,
    id: &str,
    library: &Library,
    instrument: &str,
    mic: &str,
) -> Result<(), String> {
    rig.load_instrument_with_config(
        id,
        &library.spec_path(),
        &library.zones_path(instrument),
        Path::new(library.root),
        instrument,
        mic,
    )
    .map_err(|e| e.to_string())?;
    rig.set_solo_mic(id, Some(mic.to_string()));
    rig.set_midi_channel(id, 0);
    Ok(())
}
