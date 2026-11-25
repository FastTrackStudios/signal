//! A `dyn_eq` keyed by another piece's placed hits, as Studio runs it: the
//! kit's overheads (`Drums/Cymbals/OHs`, a port of their own) cut at 450 Hz
//! only while a snare hit sounds, the snare's hits placed by the host
//! (`hits:<piece>` → `set_planned_hits`) on its trigger lane
//! (`Drums/Snare/Sum/Trig`, another port). Colt Capperrune's overhead trick,
//! without detecting the snare in the overheads.

use std::sync::Arc;

use signal_core::graph::{self, BuildOptions};
use signal_core::host::NoHost;
use signal_core::kit::{KitPieces, drum_kit};
use signal_core::session::{HitBus, Session, SessionId};
use signal_core::state::{BlockSpec, Preset};
use signal_core::{BlockFactory, BuiltinFactory};

const SR: f32 = 48_000.0;
const MAX: usize = 1_024;
const OHS: &str = "Drums/Cymbals/OHs";
const SNARE_TRIG: &str = "Drums/Snare/Sum/Trig";

fn preset(range_db: f32) -> (Preset, u16) {
    let mut preset = drum_kit(&KitPieces::default());
    let piece = preset
        .lanes
        .iter()
        .position(|l| l.name == SNARE_TRIG)
        .and_then(|i| u16::try_from(i).ok())
        .expect("the kit's snare trigger");
    let ohs = preset
        .lanes
        .iter_mut()
        .find(|l| l.name == OHS)
        .expect("the kit's overheads");
    ohs.blocks.push(
        BlockSpec::signal("snare_eq", "dyn_eq")
            .key(SNARE_TRIG)
            .param("band.1.used", 1.0)
            .param("band.1.freq", 450.0)
            .param("band.1.range", range_db)
            .param("band.1.source", 2.0)
            .param("band.1.hold", 80.0)
            .param("band.1.release", 60.0),
    );
    (preset, piece)
}

/// The overheads' render: a 450 Hz tone throughout, the snare's hits placed.
fn render(range_db: f32, placed: &[(u64, f32)], frames: usize, tag: &str) -> Vec<f32> {
    let (preset, piece) = preset(range_db);
    let factories: [&dyn BlockFactory; 1] = [&BuiltinFactory];
    let (mut g, report) = graph::build(
        &preset,
        &BuildOptions {
            sample_rate: SR,
            max_block: MAX,
            port: Some(OHS),
            factories: &factories,
            host: &NoHost,
        },
    );
    assert!(
        !report.warnings.iter().any(|w| w.contains("snare_eq")),
        "{:?}",
        report.warnings
    );
    g.join(Arc::new(Session {
        id: SessionId(tag.into()),
        hits: HitBus::new(),
    }));
    let (mut controller, mut ohs) = signal_core::processor_pair();
    let _ = ohs.install(g);
    let _ = controller.send(signal_core::engine::Command::PlannedHits {
        piece,
        hits: placed.to_vec().into_boxed_slice(),
    });
    let input: Vec<f32> = (0..frames)
        .map(|i| (i as f32 * 450.0 * std::f32::consts::TAU / SR).sin() * 0.25)
        .collect();
    let mut out = vec![0.0_f32; frames];
    let mut at = 0;
    while at < frames {
        let end = (at + MAX).min(frames);
        let mut l = input[at..end].to_vec();
        let mut r = l.clone();
        ohs.set_position(at as u64);
        ohs.process(&mut [&mut l, &mut r], 2, end - at, &[], &[]);
        out[at..end].copy_from_slice(&l);
        at = end;
    }
    controller.collect();
    out
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0, |m, s| m.max(s.abs()))
}

#[test]
fn placed_snare_hits_cut_the_overheads_only_while_they_sound() {
    let frames = 48_000 * 3;
    let placed = [(48_000_u64, 1.0_f32), (96_000, 1.0)];
    // The same graph, its band at no range: the reference (same latency).
    let flat = render(0.0, &placed, frames, "dyn-eq-flat");
    let cut = render(-4.0, &placed, frames, "dyn-eq-cut");
    let first_diff = flat
        .iter()
        .zip(&cut)
        .position(|(a, b)| a != b)
        .expect("the hits move the band");
    assert!(
        first_diff >= 48_000,
        "untouched before the first hit: differs at {first_diff}"
    );
    // Per 10 ms window, the cut against the flat render, in dB.
    let ratios: Vec<f32> = flat
        .chunks(480)
        .zip(cut.chunks(480))
        // The latency's silence at the start: no ratio.
        .map(|(a, b)| {
            if peak(a) < 1e-3 {
                0.0
            } else {
                20.0 * (peak(b) / peak(a)).log10()
            }
        })
        .collect();
    let deepest = ratios.iter().copied().fold(0.0, f32::min);
    assert!(
        (deepest + 4.0).abs() < 0.5,
        "−4 dB at 450 Hz on a hit: {deepest:.2} dB"
    );
    // Between the hits (hold and release over, before the
    // second) the overheads are the flat render's again, exactly.
    let latency = first_diff - 48_000;
    let between = 48_000 + 40_000 + latency..96_000 + latency;
    assert_eq!(
        &flat[between.clone()],
        &cut[between],
        "a null between the hits"
    );
    // Both hits cut.
    let cuts = ratios
        .windows(2)
        .filter(|w| w[0] > -1.0 && w[1] <= -1.0)
        .count();
    assert_eq!(cuts, 2, "one cut per placed hit: {ratios:?}");
}
