//! Live single-amp engine for the **Signal Amp** front-end.
//!
//! A thin, UI-facing handle over the tested guitar-rig audio path: it opens the
//! native duplex audio engine ([`GuitarRig::open`]) and runs exactly one NAM
//! amp model as the active patch's chain, exposing just what an amp UI needs —
//! load/replace the model, read the input/output meters, and see the DSP load.
//! No profiles, patches, or footswitches leak into the front-end; the amp *is*
//! the product.
//!
//! ```no_run
//! use signal_guitar::AmpEngine;
//! use signal_sampler::RigAudioPrefs;
//! let mut amp = AmpEngine::open(&RigAudioPrefs::default())?;
//! amp.load_model("/path/to/amp.nam")?;
//! // audio is now live: guitar in → NAM → out
//! let (in_pk, out_pk) = (amp.input_peak(), amp.output_peak());
//! # Ok::<(), String>(())
//! ```

use std::path::Path;

use signal_sampler::GuitarRig;
use signal_sampler::RigAudioPrefs;
use signal_sampler::{ProfileRig, RigPatch, RigProfile};

/// A running amp: live duplex audio + at most one loaded NAM model.
pub struct AmpEngine {
    rig: ProfileRig,
    model_name: String,
    model_path: Option<String>,
}

impl AmpEngine {
    /// Open the live duplex audio (guitar in → out) with no amp loaded yet —
    /// the signal passes through clean until [`load_model`](Self::load_model).
    /// Device / channel / sample-rate come from `prefs`.
    // r[impl guitar.amp.clean-passthrough]
    pub fn open(prefs: &RigAudioPrefs) -> Result<Self, String> {
        let rig = GuitarRig::open(prefs).map_err(|e| e.to_string())?;
        Ok(Self {
            rig: ProfileRig::new(rig),
            model_name: "— no amp —".to_string(),
            model_path: None,
        })
    }

    /// Load a `.nam` model as the single amp block, replacing any current one.
    /// Installs a fresh one-patch profile whose only block is the NAM amp. The
    /// full model path is carried through directly (no id round-trip), so paths
    /// with spaces/quotes resolve correctly.
    // r[impl guitar.amp.full-path]
    // r[impl guitar.amp.single-patch]
    pub fn load_model(&mut self, path: impl AsRef<Path>) -> Result<(), String> {
        let path = path.as_ref();
        let path_str = path.to_string_lossy().to_string();
        let mut profile = RigProfile::new("Amp");
        profile.patches.push(RigPatch::amp("Amp", &path_str));
        self.rig.load_profile(profile, None)?;
        self.model_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("amp")
            .to_string();
        self.model_path = Some(path_str);
        Ok(())
    }

    /// The active patch's chain as [`RigBlock`]s — the amp/cab/FX blocks to
    /// render in the signal grid. Empty block names are filled with the model
    /// name so the grid always has a label. Empty if no model is loaded.
    // r[impl guitar.amp.chain-blocks]
    pub fn active_blocks(&self) -> Vec<signal_sampler::RigBlock> {
        let patch = self
            .rig
            .active_patch()
            .or_else(|| self.rig.patches().first());
        let mut blocks = patch.map(|p| p.chain.clone()).unwrap_or_default();
        for b in &mut blocks {
            if b.name.trim().is_empty() {
                b.name = self.model_name.clone();
            }
        }
        blocks
    }

    /// Display name of the loaded model (filename stem), or the no-amp label.
    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// Full path of the loaded model, if any.
    pub fn model_path(&self) -> Option<&str> {
        self.model_path.as_deref()
    }

    /// Peak input level (guitar reaching the rig), linear 0..~1.
    // r[impl guitar.amp.live-meters]
    pub fn input_peak(&self) -> f32 {
        self.rig.rig().input_peak()
    }

    /// Peak output level (what's sent to the speakers), linear 0..~1.
    pub fn output_peak(&self) -> f32 {
        self.rig.rig().output_peak()
    }

    /// Peak output level in dBFS (−∞..0), for a calibrated meter.
    pub fn output_peak_db(&self) -> f64 {
        self.rig.rig().output_peak_db()
    }

    /// Worst-case per-block render time in microseconds — the live DSP load.
    pub fn dsp_load_us(&self) -> u32 {
        self.rig.rig().peak_render_us()
    }
}

// `AmpEngine` is `Send` (movable across threads) but not `Sync` — with the
// pipewire backend it owns a `*mut pw_thread_loop`. Front-ends share it as
// `Arc<Mutex<AmpEngine>>`, which is `Send + Sync` because `AmpEngine: Send`.
#[allow(dead_code)]
fn _assert_send() {
    fn is_send<T: Send>() {}
    is_send::<AmpEngine>();
}
