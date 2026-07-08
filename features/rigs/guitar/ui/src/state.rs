//! Live rig view-state — one hook that seeds from the request/response
//! services and then goes live on the `#[subscribe]` event stream.

use dioxus::prelude::*;

use signal_guitar_proto::rig::{RigClient, RigEvent, RigStreamClient};
use signal_guitar_proto::{LiveBlock, PerformanceModel};

use crate::meters::meter_level;

/// The signals a rig view renders from. `Copy` (signals are handles), so it
/// passes freely into closures and children.
#[derive(Clone, Copy, PartialEq)]
pub struct RigViewState {
    /// Audio engine open and processing.
    pub running: Signal<bool>,
    /// Perceptual input level (0..1, sqrt-curved).
    pub in_level: Signal<f64>,
    /// Perceptual output level (0..1, sqrt-curved).
    pub out_level: Signal<f64>,
    /// Raw input peak in dBFS (−90..0) — the Control view's gate/comp
    /// visualizations need real dB, not the perceptual meter curve.
    pub in_peak_db: Signal<f32>,
    /// Raw output peak in dBFS (−90..0).
    pub out_peak_db: Signal<f32>,
    /// Compressor gain reduction (dB, positive = reducing).
    pub comp_gr_db: Signal<f32>,
    /// Input spectrum (dB per log bin, 20 Hz–20 kHz), ~15 Hz.
    pub spectrum: Signal<Vec<f32>>,
    /// Live performance model (stacks, fx bypass, boost, tempo).
    pub perf: Signal<PerformanceModel>,
    /// The active patch's FX chain.
    pub blocks: Signal<Vec<LiveBlock>>,
    /// Name of the active patch (raw backend name, e.g. "Crunch Edge").
    pub active_patch: Signal<Option<String>>,
}

/// Seed the rig view-state with one `status`/`perf`/`chain` fetch, then fold
/// every [`RigEvent`] from the stream into it. Clients come from Dioxus
/// context (provided by the host app root); absent clients leave the state
/// at its defaults, so the view renders a disconnected shell gracefully.
pub fn use_rig_state() -> RigViewState {
    let rig = use_hook(try_consume_context::<RigClient>);
    let rig_stream = use_hook(try_consume_context::<RigStreamClient>);

    let mut running = use_signal(|| false);
    let mut in_level = use_signal(|| 0.0f64);
    let mut out_level = use_signal(|| 0.0f64);
    let mut in_peak_db = use_signal(|| -90.0f32);
    let mut out_peak_db = use_signal(|| -90.0f32);
    let mut comp_gr_db = use_signal(|| 0.0f32);
    let mut spectrum = use_signal(Vec::<f32>::new);
    let mut perf = use_signal(PerformanceModel::default);
    let mut blocks = use_signal(Vec::<LiveBlock>::new);
    let mut active_patch = use_signal(|| None::<String>);

    // Seed once — the event stream only carries *changes*; a fresh
    // subscriber needs the current state to start from.
    {
        let rig = rig.clone();
        use_future(move || {
            let rig = rig.clone();
            async move {
                let Some(rig) = rig else { return };
                if let Ok(s) = rig.status().await {
                    running.set(s.running);
                    in_level.set(meter_level(s.input_peak));
                    out_level.set(meter_level(s.output_peak));
                    in_peak_db.set(peak_db(s.input_peak));
                    out_peak_db.set(peak_db(s.output_peak));
                    comp_gr_db.set(s.comp_gr_db);
                    active_patch.set(s.active_patch);
                }
                if let Ok(p) = rig.perf().await {
                    perf.set(p);
                }
                if let Ok(c) = rig.chain().await {
                    blocks.set(c);
                }
            }
        });
    }

    // Live updates — meters at meter rate, perf/chain on mutation.
    {
        let rig_stream = rig_stream.clone();
        architect::use_stream(
            move |sink| {
                let rig_stream = rig_stream.clone();
                async move {
                    match rig_stream {
                        Some(s) => s.events(sink).await.is_ok(),
                        None => false,
                    }
                }
            },
            move |ev: RigEvent| {
                let (mut running, mut in_level, mut out_level, mut in_peak_db, mut out_peak_db, mut comp_gr_db, mut spectrum, mut perf, mut blocks, mut active_patch) =
                    (running, in_level, out_level, in_peak_db, out_peak_db, comp_gr_db, spectrum, perf, blocks, active_patch);
                match ev {
                    RigEvent::Status(s) => {
                        running.set(s.running);
                        in_level.set(meter_level(s.input_peak));
                        out_level.set(meter_level(s.output_peak));
                        in_peak_db.set(peak_db(s.input_peak));
                        out_peak_db.set(peak_db(s.output_peak));
                        comp_gr_db.set(s.comp_gr_db);
                        active_patch.set(s.active_patch);
                    }
                    RigEvent::Perf(p) => perf.set(p),
                    RigEvent::Chain(c) => blocks.set(c),
                    RigEvent::Spectrum(bins) => spectrum.set(bins),
                }
            },
        );
    }

    RigViewState {
        running,
        in_level,
        out_level,
        in_peak_db,
        out_peak_db,
        comp_gr_db,
        spectrum,
        perf,
        blocks,
        active_patch,
    }
}

/// Linear peak → dBFS, floored at −90.
fn peak_db(peak: f32) -> f32 {
    if peak <= 0.0 {
        -90.0
    } else {
        (20.0 * peak.log10()).max(-90.0)
    }
}
