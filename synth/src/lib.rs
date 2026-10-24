//! Synth — the software-synthesizer *definition + import* layer over the shared
//! sampler engine, the synth analogue of `signal-drums` / `signal-orchestra`.
//!
//! Where the orchestra and drums features describe sample libraries, `synth`
//! owns the Spectrasonics **Omnisphere** routing experiment: the full
//! Omnisphere 3 per-Part signal chain expressed as a composition tree
//! ([`sampler::rig_node`]), plus the `.prt_omn` / `.mlt_omn` patch and
//! multi importer that maps real Omnisphere state onto that tree and realizes
//! Soundsource blocks against the local extraction.
//!
//! The presets are registered into a [`sampler::PresetRegistry`] via
//! [`register_omnisphere`] — signal-sampler no longer knows about Omnisphere
//! (that would form a `sampler → synth → sampler` cycle), so any caller that
//! wants the Omnisphere built-ins calls this after `PresetRegistry::with_builtins`.

pub mod proto;
#[cfg(feature = "ui")]
pub mod ui;

/// The Signal Engine.
///
/// The canonical layer program every rig loads patches into
/// (source stack → filters → amp → FX + modulators).
pub mod engine;
/// Reading a Gig Performer rig back out of a `.gig` file.
///
/// The inverse of the exporter in [`crate::engine`].
pub mod gig;
pub mod omni;
pub mod omni_import;
// Building packs (the filesystem, the sample cache) is native-only.
#[cfg(not(target_arch = "wasm32"))]
pub mod pack;

pub use engine::{Source, signal_layer};
pub use omni::{omnisphere_preset, omnisphere_soundsource_preset};

/// Register the Omnisphere built-in presets into `r`.
///
/// Adds the placeholder Omnisphere Part unconditionally and the
/// soundsource-realized variant only on machines that have the local
/// extraction (guarding the `Option`, as the original built-in did).
pub fn register_omnisphere(r: &mut sampler::PresetRegistry) {
    r.register_code(omni::omnisphere_preset());
    if let Some(omni) = omni::omnisphere_soundsource_preset() {
        r.register_code(omni);
    }
}
