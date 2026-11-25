//! A kit's Rescue EQ (the `channel` algorithm of the `eq` family) cuts below
//! its low cut, from the spec's values alone — no control moved — and the
//! same after the preset goes through its saved form (a state load, a
//! render copy's rebuild). It once passed every kit's audio unchanged.

use signal_core::graph::{self, BuildOptions};
use signal_core::host::NoHost;
use signal_core::kit::{KitPieces, drum_kit};
use signal_core::state::Preset;
use signal_core::{BlockFactory, BuiltinFactory};

const SR: f32 = 48_000.0;
const MAX: usize = 512;
/// The kit's hi hat: a 250 Hz Rescue low cut.
const HAT: &str = "Drums/Cymbals/Hi Hat";

/// The kit with the hi hat's chain cut to its Rescue EQ alone.
fn rescue_only() -> Preset {
    let mut preset = drum_kit(&KitPieces::default());
    let hat = preset
        .lanes
        .iter_mut()
        .find(|l| l.name == HAT)
        .expect("the hi hat");
    hat.blocks.retain(|b| b.id == "rescue_eq");
    assert_eq!(hat.blocks.len(), 1, "the hi hat has its Rescue EQ");
    preset
}

/// The level (dB) the hi hat's port leaves a `hz` sine at, settled.
fn gain_at(preset: &Preset, hz: f32) -> f64 {
    let factories: [&dyn BlockFactory; 1] = [&BuiltinFactory];
    let (g, report) = graph::build(
        preset,
        &BuildOptions {
            sample_rate: SR,
            max_block: MAX,
            port: Some(HAT),
            factories: &factories,
            host: &NoHost,
        },
    );
    assert!(
        report.warnings.iter().all(|w| !w.contains("rescue_eq")),
        "{:?}",
        report.warnings
    );
    let (mut controller, mut port) = signal_core::processor_pair();
    let _ = port.install(g);
    let frames = 48_000;
    let (mut sum_in, mut sum_out) = (0.0_f64, 0.0_f64);
    let mut at = 0;
    while at < frames {
        let end = (at + MAX).min(frames);
        let mut l: Vec<f32> = (at..end)
            .map(|i| 0.25 * (i as f32 * hz * std::f32::consts::TAU / SR).sin())
            .collect();
        let input = l.clone();
        let mut r = l.clone();
        port.set_position(at as u64);
        port.process(&mut [&mut l, &mut r], 2, end - at, &[], &[]);
        if at >= frames / 2 {
            sum_in += input.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>();
            sum_out += l.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>();
        }
        at = end;
    }
    controller.collect();
    10.0 * (sum_out / sum_in).log10()
}

fn assert_cuts(preset: &Preset, what: &str) {
    let low = gain_at(preset, 63.0);
    assert!(
        low < -20.0,
        "{what}: 63 Hz under a 250 Hz low cut: {low} dB"
    );
    let high = gain_at(preset, 4_000.0);
    assert!(high.abs() < 1.0, "{what}: 4 kHz passes: {high} dB");
}

#[test]
fn a_kits_rescue_eq_cuts_below_its_low_cut() {
    let preset = rescue_only();
    assert_cuts(&preset, "built from the spec");
    let saved = serde_json::to_string(&preset).expect("saved");
    let loaded: Preset = serde_json::from_str(&saved).expect("loaded");
    assert_cuts(&loaded, "after a state load");
}
