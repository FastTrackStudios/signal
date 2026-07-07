//! FTS Compressor — desktop app entry point.
//!
//! Run standalone (full featured, VelloCanvas renders, live DSP demo):
//!   cargo run -p comp-plugin --bin comp-desktop
//!   (or via the justfile: just run-comp)
//!
//! Run with dx serve (hot reload, layout/CSS only — VelloCanvas blank):
//!   cd apps/comp-plugin && dx serve

use comp_dsp::chain::CompChain;
use comp_plugin::{CompUiState, FtsCompParams, WAVEFORM_LEN};
use audiocore_dsp::AudioConfig;
use nih_plug_dioxus::SharedState;
use std::sync::atomic::Ordering;
use std::sync::Arc;

fn main() {
    let params = Arc::new(FtsCompParams::default());
    let ui_state = Arc::new(CompUiState::new(params));

    // Spawn background thread: runs synthetic audio through CompChain and writes
    // to waveform atomics so the display scrolls and responds to knob changes.
    let ui_clone = ui_state.clone();
    std::thread::spawn(move || run_demo_dsp(ui_clone));

    let shared = SharedState::new(ui_state);
    // Use the baseview standalone path — VelloCanvas (waveform) works here.
    nih_plug_dioxus::open_standalone_with_state(comp_plugin::editor::App, 900, 620, Some(shared));
}

// ── Demo DSP thread ───────────────────────────────────────────────────────────

const SAMPLE_RATE: f64 = 48000.0;

fn run_demo_dsp(ui: Arc<CompUiState>) {
    const BLOCK_SIZE: usize = 512;
    let block_dur = std::time::Duration::from_secs_f64(BLOCK_SIZE as f64 / SAMPLE_RATE);

    let mut chain = CompChain::new();
    chain.update(AudioConfig {
        sample_rate: SAMPLE_RATE,
        max_buffer_size: BLOCK_SIZE,
    });

    // ~240 waveform entries / second (matches plugin rate)
    let waveform_interval = (SAMPLE_RATE as usize / 240).max(1);
    let mut waveform_counter = 0usize;
    let mut waveform_peak = 0.0f32;
    let mut waveform_gr_peak = 0.0f32;
    let mut sample_clock: u64 = 0;

    loop {
        let t0 = std::time::Instant::now();

        sync_params(&ui, &mut chain);

        for _ in 0..BLOCK_SIZE {
            let t = sample_clock as f64 / SAMPLE_RATE;
            let x = demo_signal(t);
            let input_peak = x.abs() as f32;

            let mut l = x;
            let mut r = x;
            chain.process_sample(&mut l, &mut r);

            let gr = chain.comp.gain_reduction_db() as f32;
            ui.gain_reduction_db.store(gr, Ordering::Relaxed);

            // Peak input metering with exponential decay
            let in_db = if input_peak > 1e-6 {
                20.0 * input_peak.log10()
            } else {
                -100.0
            };
            let prev = ui.input_peak_db.load(Ordering::Relaxed);
            ui.input_peak_db.store(
                if in_db > prev { in_db } else { prev - 0.3 },
                Ordering::Relaxed,
            );

            // Waveform decimation — mirrors plugin process()
            waveform_peak = waveform_peak.max(input_peak);
            waveform_gr_peak = waveform_gr_peak.max(gr / 30.0);
            waveform_counter += 1;
            ui.waveform_phase.store(
                waveform_counter as f32 / waveform_interval as f32,
                Ordering::Relaxed,
            );

            if waveform_counter >= waveform_interval {
                let pos = ui.waveform_pos.load(Ordering::Relaxed) as usize % WAVEFORM_LEN;
                ui.waveform_input[pos].store(waveform_peak.min(1.0), Ordering::Relaxed);
                ui.waveform_gr[pos].store(waveform_gr_peak.min(1.0), Ordering::Relaxed);
                ui.waveform_pos.store((pos + 1) as f32, Ordering::Relaxed);
                ui.waveform_phase.store(0.0, Ordering::Relaxed);
                waveform_counter = 0;
                waveform_peak = 0.0;
                waveform_gr_peak = 0.0;
            }

            sample_clock += 1;
        }

        // Throttle to real time
        if let Some(remaining) = block_dur.checked_sub(t0.elapsed()) {
            std::thread::sleep(remaining);
        }
    }
}

/// Drum-machine style signal at 120 BPM: 4 hits per bar with varying amplitudes.
///
/// Beat 1 (kick):  loud  → clearly above threshold → visible GR pumping
/// Beat 2 (hi-hat): soft  → near/below threshold
/// Beat 3 (snare): medium → moderate GR
/// Beat 4 (hi-hat): soft  → near/below threshold
///
/// Each hit is a brief 150 Hz tone burst with fast exponential decay.
fn demo_signal(t: f64) -> f64 {
    const BPM: f64 = 120.0;
    const BEAT: f64 = 60.0 / BPM; // 0.5 s per beat
    const DECAY: f64 = 35.0; // 1/e at ~29 ms — drum-like transient
    const FREQ: f64 = 150.0;

    let bar = t % (BEAT * 4.0);
    let beat = (bar / BEAT).floor() as usize;
    let t_in_beat = bar - beat as f64 * BEAT;

    let amp: f64 = match beat {
        0 => 1.00, // kick  — peaks at 0 dBFS, well above -10 dB threshold
        1 => 0.32, // hi-hat — ~-10 dBFS, near threshold
        2 => 0.72, // snare  — ~-3 dBFS
        _ => 0.32, // hi-hat
    };

    let envelope = (-t_in_beat * DECAY).exp();
    let carrier = (t * 2.0 * std::f64::consts::PI * FREQ).sin();
    carrier * amp * envelope
}

/// Read current param values (set by UI knobs) into the CompChain.
fn sync_params(ui: &CompUiState, chain: &mut CompChain) {
    let p = &ui.params;
    let c = &mut chain.comp;
    c.set_threshold(p.threshold_db.value() as f64);
    c.set_ratio(p.ratio.value() as f64);
    c.set_attack_ms(p.attack_ms.value() as f64);
    c.set_release_ms(p.release_ms.value() as f64);
    c.set_knee(p.knee_db.value() as f64);
    c.set_style(p.style.value());
    c.style = p.style.value();
    c.auto_makeup = p.auto_makeup.value() > 0.5;
    c.feedback = p.feedback.value() as f64;
    c.channel_link = p.channel_link.value() as f64;
    c.inertia = p.inertia.value() as f64;
    c.inertia_decay = p.inertia_decay.value() as f64;
    c.ceiling = p.ceiling.value() as f64;
    c.set_fold(p.fold.value() as f64);
    c.input_gain_db = p.input_gain_db.value() as f64;
    c.output_gain_db = p.output_gain_db.value() as f64;
    c.set_range_db(p.range_db.value() as f64);
    c.hold_ms = p.hold_ms.value() as f64;
    chain.set_sidechain_freq(p.sidechain_freq.value() as f64);
    chain.set_lookahead(p.lookahead_ms.value() as f64);
    chain.comp.update(SAMPLE_RATE);
}
