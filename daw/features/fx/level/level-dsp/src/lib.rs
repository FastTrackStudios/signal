//! Level DSP — vocal leveling & cleanup engine.
//!
//! `level` is the vocal-treatment counterpart to the pitch-focused `tune` FX:
//! it *rides* and *cleans* a vocal rather than tuning it. The crate is built
//! from one shared analysis core reused two ways, per the "both models" design:
//!
//! - **Offline** ([`fn@analyze`], [`render_gain_envelope`]) — Melodyne/macro-style:
//!   analyse a whole take, classify + segment it, and render a smoothed volume
//!   envelope the DAW can drop onto an automation lane or clip-gain. Sees the
//!   whole signal, so it can pick an adaptive silence floor and auto-target.
//! - **Realtime** ([`VocalRider`], [`Gate`], [`DeEsser`], [`DeBreather`]) —
//!   streaming, alloc-free processors that ride/gate/de-ess/de-breath live.
//!
//! Both share [`Classifier`], the ZCR + spectral-centroid + spectral-flux block
//! classifier. The bundled [`VocalLeveler`] wires the four realtime tools into
//! one chain (gate → de-breath → ride → de-ess) — the "does all four" node the
//! plugin exposes. Everything is clean-room; no third-party algorithm source is
//! vendored.
//!
//! DSP style matches the sibling fx crates (`comp-dsp`, `pitch-dsp`): plain
//! `std`, `f64`, allocation confined to construction/`set_sample_rate`.

pub mod analyze;
pub mod classify;
pub mod debreath;
pub mod deess;
pub mod drums;
pub mod envelope;
pub mod filter;
pub mod gate;
pub mod rider;
pub mod segment;

pub use analyze::{
    analyze, apply_envelope, render_gain_envelope, Analysis, AnalyzedBlock, GainPoint, RenderConfig,
};
pub use classify::{adaptive_silence_db, BlockClass, BlockFeatures, Classifier, ClassifyConfig};
pub use debreath::{DeBreathConfig, DeBreather};
pub use deess::{DeEssConfig, DeEsser};
pub use gate::{Gate, GateConfig};
pub use rider::{RiderConfig, VocalRider};
pub use segment::{auto_target_db, build_segments, Segment, SegmentConfig};

/// Enable/config for each stage of the bundled vocal chain.
#[derive(Clone, Copy, Debug)]
pub struct LevelerConfig {
    /// Gate stage (`None` = bypass).
    pub gate: Option<GateConfig>,
    /// Breath-ducking stage (`None` = bypass).
    pub debreath: Option<DeBreathConfig>,
    /// Level-riding stage (`None` = bypass).
    pub rider: Option<RiderConfig>,
    /// De-essing stage (`None` = bypass).
    pub deess: Option<DeEssConfig>,
    /// Block classifier configuration shared by the analysis stages.
    pub classify: ClassifyConfig,
    /// Initial adaptive silence floor (refine from an offline pre-pass), dBFS.
    pub silence_db: f64,
}

impl Default for LevelerConfig {
    fn default() -> Self {
        Self {
            gate: Some(GateConfig::default()),
            debreath: Some(DeBreathConfig::default()),
            rider: Some(RiderConfig::default()),
            deess: Some(DeEssConfig::default()),
            classify: ClassifyConfig::default(),
            silence_db: -45.0,
        }
    }
}

/// The bundled realtime vocal chain: gate → de-breath → ride → de-ess.
///
/// Mono in / mono out. Order is **gate → breath → sibilance → ride**
/// (#204), with the ride last because it is the curve you will edit most
/// and it should see everything the cleanup stages did.
///
/// This is the *realtime* chain, where each stage genuinely consumes the
/// previous one's output. The generated **envelopes** are deliberately
/// different: they are independent, share only one analysis pass, and
/// never consume each other's gain — because each has to be bypassable
/// on its own, and a curve keyed off another's output is not honestly
/// bypassable. See `envelope`.
pub struct VocalLeveler {
    gate: Option<Gate>,
    debreath: Option<DeBreather>,
    rider: Option<VocalRider>,
    deess: Option<DeEsser>,
    sample_rate: f64,
}

impl VocalLeveler {
    /// Build the chain for a sample rate.
    pub fn new(sample_rate: f64, cfg: LevelerConfig) -> Self {
        Self {
            gate: cfg.gate.map(|c| Gate::new(sample_rate, c)),
            debreath: cfg
                .debreath
                .map(|c| DeBreather::new(sample_rate, cfg.classify, c, cfg.silence_db)),
            rider: cfg
                .rider
                .map(|c| VocalRider::new(sample_rate, cfg.classify, c, cfg.silence_db)),
            deess: cfg.deess.map(|c| DeEsser::new(sample_rate, c)),
            sample_rate: sample_rate.max(1.0),
        }
    }

    /// Update the tuning of any *already-present* stages in place, without
    /// allocation — safe to call from the audio thread every block. Stages that
    /// were constructed as `None` stay bypassed (build with all stages present
    /// and use neutral params to bypass at runtime).
    pub fn set_stage_configs(
        &mut self,
        gate: GateConfig,
        debreath: DeBreathConfig,
        rider: RiderConfig,
        deess: DeEssConfig,
    ) {
        if let Some(g) = &mut self.gate {
            g.set_config(gate);
        }
        if let Some(d) = &mut self.debreath {
            d.set_config(debreath, self.sample_rate);
        }
        if let Some(r) = &mut self.rider {
            r.set_config(rider);
        }
        if let Some(d) = &mut self.deess {
            d.set_config(deess);
        }
    }

    /// Refine the adaptive silence floor for the spectrum-aware stages (e.g.
    /// after an offline [`fn@analyze`] pass over the same source).
    pub fn set_silence_db(&mut self, db: f64) {
        if let Some(r) = &mut self.rider {
            r.set_silence_db(db);
        }
        if let Some(d) = &mut self.debreath {
            d.set_silence_db(db);
        }
    }

    /// Clear all runtime state.
    pub fn reset(&mut self) {
        if let Some(g) = &mut self.gate {
            g.reset();
        }
        if let Some(d) = &mut self.debreath {
            d.reset();
        }
        if let Some(r) = &mut self.rider {
            r.reset();
        }
        if let Some(d) = &mut self.deess {
            d.reset();
        }
    }

    /// Process one mono sample through every enabled stage.
    #[inline]
    pub fn process_sample(&mut self, input: f64) -> f64 {
        let mut x = input;
        if let Some(g) = &mut self.gate {
            x = g.process_sample(x);
        }
        if let Some(d) = &mut self.debreath {
            x = d.process_sample(x);
        }
        // Sibilance before the ride: the ride is last so that what it
        // sees is already cleaned up, which is the reorder #204 made.
        if let Some(d) = &mut self.deess {
            x = d.process_sample(x);
        }
        if let Some(r) = &mut self.rider {
            x = r.process_sample(x);
        }
        x
    }
}
