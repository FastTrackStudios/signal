//! Live rig view-state — one hook that seeds from the request/response
//! services and then goes live on the `#[subscribe]` event stream.

use dioxus::prelude::*;

use signal_guitar_proto::rig::{RigClient, RigEvent, RigStreamClient};
use signal_guitar_proto::{LevelProgress, LiveBlock, LiveNode, MacroKnobView, PerformanceModel, RigPerf};

use crate::meters::meter_level;

/// The signals a rig view renders from. `Copy` (signals are handles), so it
/// passes freely into closures and children.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct RigViewState {
    /// Audio engine open and processing.
    pub running: Signal<bool>,
    /// Why it is not, in words (empty while it runs).
    pub audio_error: Signal<String>,
    /// Perceptual input level (0..1, sqrt-curved).
    pub in_level: Signal<f64>,
    /// Perceptual output level (0..1, sqrt-curved).
    pub out_level: Signal<f64>,
    /// Raw input peak in dBFS (−90..0) — the Control view's gate/comp
    /// visualizations need real dB, not the perceptual meter curve.
    pub in_peak_db: Signal<f32>,
    /// Raw output peak in dBFS (−90..0).
    pub out_peak_db: Signal<f32>,
    /// Stereo peaks in dBFS: (in L, in R, out L, out R).
    pub stereo_db: Signal<(f32, f32, f32, f32)>,
    /// The incoming monitor mix at the headphone mixer, dBFS (L, R).
    pub mix_db: Signal<(f32, f32)>,
    /// Compressor gain reduction (dB, positive = reducing).
    pub comp_gr_db: Signal<f32>,
    /// Input spectrum (dB per log bin, 20 Hz–20 kHz), ~15 Hz.
    pub spectrum: Signal<Vec<f32>>,
    /// Compressor rolling telemetry `(input 0..1, gr 0..1)`, oldest→newest.
    /// Each compressor block's rolling trace, by block name:
    /// `(input_peaks, gain_reduction, gr_db)` — see `RigEvent::CompWave`.
    pub comp_wave: Signal<std::collections::HashMap<String, (Vec<f32>, Vec<f32>, f32)>>,
    /// Live performance model (stacks, fx bypass, boost, tempo).
    pub perf: Signal<PerformanceModel>,
    /// The active patch's FX chain.
    pub blocks: Signal<Vec<LiveBlock>>,
    /// The rig as nodes — every block *and* container, with the presets each
    /// can be recalled as. What `blocks` cannot say: a chain is flat, so a
    /// Module has nowhere to appear in it.
    pub nodes: Signal<Vec<LiveNode>>,
    /// Name of the active patch (raw backend name, e.g. "Crunch Edge").
    pub active_patch: Signal<Option<String>>,
    /// What the rig costs to run — render time, load, dropouts. Rides on the
    /// status payload, so it updates at meter rate.
    pub dsp: Signal<RigPerf>,
    /// The last patch-levelling pass. Seeded from the backend so a remote that
    /// connects after a pass still sees its results.
    pub levelling: Signal<LevelProgress>,
    /// The active patch's macro bar, values included.
    pub macros: Signal<Vec<MacroKnobView>>,
}

/// Seed the rig view-state with one `status`/`perf`/`chain` fetch, then fold
/// every [`RigEvent`] from the stream into it.
///
/// Clients come from Dioxus context (provided by the host app root); absent
/// clients leave the state at its defaults, so the view renders a disconnected
/// shell gracefully.
pub fn use_rig_state() -> RigViewState {
    let rig = use_hook(try_consume_context::<RigClient>);
    let rig_stream = use_hook(try_consume_context::<RigStreamClient>);

    let mut running = use_signal(|| false);
    let mut audio_error = use_signal(String::new);
    let mut in_level = use_signal(|| 0.0f64);
    let mut out_level = use_signal(|| 0.0f64);
    let mut in_peak_db = use_signal(|| -90.0f32);
    let mut out_peak_db = use_signal(|| -90.0f32);
    let mut stereo_db = use_signal(|| (-90.0f32, -90.0f32, -90.0f32, -90.0f32));
    let mut comp_gr_db = use_signal(|| 0.0f32);
    let mut mix_db = use_signal(|| (-90.0f32, -90.0f32));
    let spectrum = use_signal(Vec::<f32>::new);
    let comp_wave = use_signal(std::collections::HashMap::<String, (Vec<f32>, Vec<f32>, f32)>::new);
    let mut perf = use_signal(PerformanceModel::default);
    let mut blocks = use_signal(Vec::<LiveBlock>::new);
    let mut nodes = use_signal(Vec::<LiveNode>::new);
    let mut active_patch = use_signal(|| None::<String>);
    let mut dsp = use_signal(RigPerf::default);
    let mut levelling = use_signal(LevelProgress::default);
    let mut macros = use_signal(Vec::<MacroKnobView>::new);

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
                    audio_error.set(s.audio_error.clone());
                    in_level.set(meter_level(s.input_peak));
                    out_level.set(meter_level(s.output_peak));
                    in_peak_db.set(peak_db(s.input_peak));
                    out_peak_db.set(peak_db(s.output_peak));
                    stereo_db.set((
                        peak_db(s.input_peak_l),
                        peak_db(s.input_peak_r),
                        peak_db(s.output_peak_l),
                        peak_db(s.output_peak_r),
                    ));
                    comp_gr_db.set(s.comp_gr_db);
                    mix_db.set((s.mix_db_l, s.mix_db_r));
                    active_patch.set(s.active_patch);
                    dsp.set(s.perf);
                }
                if let Ok(p) = rig.perf().await {
                    perf.set(p);
                }
                if let Ok(c) = rig.chain().await {
                    blocks.set(c);
                }
                if let Ok(n) = rig.nodes().await {
                    nodes.set(n);
                }
                if let Ok(l) = rig.level_progress().await {
                    levelling.set(l);
                }
                if let Ok(m) = rig.macros().await {
                    macros.set(m);
                }
            }
        });
    }

    // Live updates — meters at meter rate, perf/chain on mutation.
    {
        let rig_stream = rig_stream;
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
            {
                let rig_for_events = rig.clone();
                move |ev: RigEvent| {
                    let rig = rig_for_events.clone();
                    let (
                        running,
                        audio_error,
                        in_level,
                        out_level,
                        in_peak_db,
                        out_peak_db,
                        stereo_db,
                        mix_db,
                        comp_gr_db,
                        mut spectrum,
                        mut comp_wave,
                        mut perf,
                        mut blocks,
                        mut nodes,
                        active_patch,
                        dsp,
                        mut levelling,
                        mut macros,
                    ) = (
                        running,
                        audio_error,
                        in_level,
                        out_level,
                        in_peak_db,
                        out_peak_db,
                        stereo_db,
                        mix_db,
                        comp_gr_db,
                        spectrum,
                        comp_wave,
                        perf,
                        blocks,
                        nodes,
                        active_patch,
                        dsp,
                        levelling,
                        macros,
                    );
                    match ev {
                        RigEvent::Status(s) => {
                            // Each only when it changed: a set wakes every
                            // reader, and this arrives at meter rate.
                            fn put<T: PartialEq + 'static>(mut sig: Signal<T>, v: T) {
                                if *sig.peek() != v {
                                    sig.set(v);
                                }
                            }
                            put(running, s.running);
                            put(audio_error, s.audio_error.clone());
                            put(in_level, meter_level(s.input_peak));
                            put(out_level, meter_level(s.output_peak));
                            put(in_peak_db, peak_db(s.input_peak));
                            put(out_peak_db, peak_db(s.output_peak));
                            put(
                                stereo_db,
                                (
                                    peak_db(s.input_peak_l),
                                    peak_db(s.input_peak_r),
                                    peak_db(s.output_peak_l),
                                    peak_db(s.output_peak_r),
                                ),
                            );
                            put(comp_gr_db, s.comp_gr_db);
                            put(mix_db, (s.mix_db_l, s.mix_db_r));
                            put(active_patch, s.active_patch);
                            put(dsp, s.perf);
                        }
                        RigEvent::Levelling(l) => levelling.set(l),
                        RigEvent::Macros(m) => macros.set(m),
                        RigEvent::Perf(p) => perf.set(p),
                        RigEvent::Chain(c) => {
                            blocks.set(c);
                            // The tree changes with the chain — a patch switch
                            // can swap which capture a slot holds — and the
                            // event carries blocks only, so re-read it.
                            let rig = rig.clone();
                            spawn(async move {
                                if let Some(rig) = rig
                                    && let Ok(n) = rig.nodes().await
                                {
                                    nodes.set(n);
                                }
                            });
                        }
                        RigEvent::Spectrum(bins) => {
                            // Analyzer ballistics: instant attack, ~40 dB/s
                            // decay, plus a light 3-tap frequency smooth — the
                            // standard "looks right to a human" treatment.
                            let prev = spectrum.peek().clone();
                            let n = bins.len();
                            let mut out = Vec::with_capacity(n);
                            for i in 0..n {
                                let (a, b, c) =
                                    (bins[i.saturating_sub(1)], bins[i], bins[(i + 1).min(n - 1)]);
                                let fresh = (2.0f32.mul_add(b, a) + c) / 4.0;
                                let fallen = prev.get(i).copied().unwrap_or(-90.0) - 1.3; // per frame at ~30 Hz ≈ 40 dB/s
                                out.push(fresh.max(fallen).max(-90.0));
                            }
                            spectrum.set(out);
                        }
                        RigEvent::CompWave(trace) => {
                            // A soft 3-tap along time keeps the rolling traces
                            // fluid without hiding transients.
                            let smooth = |v: &[f32]| -> Vec<f32> {
                                let n = v.len();
                                (0..n)
                                    .map(|k| {
                                        (2.0f32.mul_add(v[k], v[k.saturating_sub(1)])
                                            + v[(k + 1).min(n - 1)])
                                            / 4.0
                                    })
                                    .collect()
                            };
                            let entry = (smooth(&trace.input), smooth(&trace.gr), trace.gr_db);
                            comp_wave.with_mut(|m| {
                                m.insert(trace.block, entry);
                            });
                        }
                    }
                }
            },
        );
    }

    RigViewState {
        running,
        audio_error,
        in_level,
        out_level,
        in_peak_db,
        out_peak_db,
        stereo_db,
        mix_db,
        comp_gr_db,
        spectrum,
        comp_wave,
        perf,
        blocks,
        nodes,
        active_patch,
        dsp,
        levelling,
        macros,
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
