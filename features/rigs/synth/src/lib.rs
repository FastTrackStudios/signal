//! Synth — the software-synthesizer *definition + import* layer over the shared
//! sampler engine, the synth analogue of `signal-drums` / `signal-orchestra`.
//!
//! Where the orchestra and drums features describe sample libraries, `synth`
//! owns the Spectrasonics **Omnisphere** routing experiment: the full
//! Omnisphere 3 per-Part signal chain expressed as a composition tree
//! ([`signal_sampler::rig_node`]), plus the `.prt_omn` / `.mlt_omn` patch and
//! multi importer that maps real Omnisphere state onto that tree and realizes
//! Soundsource blocks against the local extraction.
//!
//! The presets are registered into a [`signal_sampler::PresetRegistry`] via
//! [`register_omnisphere`] — signal-sampler no longer knows about Omnisphere
//! (that would form a `sampler → synth → sampler` cycle), so any caller that
//! wants the Omnisphere built-ins calls this after `PresetRegistry::with_builtins`.

pub mod omni;
pub mod omni_import;
pub mod pack;

pub use omni::{omnisphere_preset, omnisphere_soundsource_preset};

/// Register the Omnisphere built-in presets into `r`.
///
/// Adds the placeholder Omnisphere Part unconditionally and the
/// soundsource-realized variant only on machines that have the local
/// extraction (guarding the `Option`, as the original built-in did).
pub fn register_omnisphere(r: &mut signal_sampler::PresetRegistry) {
    r.register_code(omni::omnisphere_preset());
    if let Some(omni) = omni::omnisphere_soundsource_preset() {
        r.register_code(omni);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use signal_sampler::{PresetRegistry, PresetSource};

    /// `register_omnisphere` adds the Omnisphere placeholder Part on top of the
    /// sampler built-ins — the assertion that used to live in signal-sampler's
    /// `with_builtins`, moved here now that omni belongs to synth.
    #[test]
    fn register_omnisphere_adds_the_part() {
        let mut reg = PresetRegistry::with_builtins();
        register_omnisphere(&mut reg);
        let omni = reg.get("Omnisphere").expect("Omnisphere registered");
        assert_eq!(omni.source, PresetSource::Code);
    }
}
