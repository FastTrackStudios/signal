//! The transient shaper on the kit's rooms (`Drums/Rooms`, a port of its
//! own): attack down and sustain up, keyed by the snare's placed hits, so it
//! shapes the room after the snare and leaves the room's other hits (the
//! kick's, here) alone.

use std::sync::Arc;

use signal_core::graph::{self, BuildOptions};
use signal_core::host::NoHost;
use signal_core::kit::{KitPieces, drum_kit};
use signal_core::session::{HitBus, Session, SessionId};
use signal_core::state::BlockSpec;
use signal_core::{BlockFactory, BuiltinFactory};

const SR: f32 = 48_000.0;
const MAX: usize = 1_024;
const ROOMS: &str = "Drums/Rooms";
const SNARE_TRIG: &str = "Drums/Snare/Sum/Trig";
const FRAMES: usize = 48_000 * 2;
/// A room hit every half second; the snare's is the one at 1 s.
const EVERY: usize = 24_000;
const SNARE: usize = 48_000;

fn render(attack: f32, sustain: f32, tag: &str) -> (Vec<f32>, usize) {
    let mut preset = drum_kit(&KitPieces::default());
    let piece = preset
        .lanes
        .iter()
        .position(|l| l.name == SNARE_TRIG)
        .and_then(|i| u16::try_from(i).ok())
        .expect("the snare's trigger");
    preset
        .lanes
        .iter_mut()
        .find(|l| l.name == ROOMS)
        .expect("the rooms")
        .blocks
        .push(
            BlockSpec::signal("shape", "transient")
                .key(SNARE_TRIG)
                .param("attack", attack)
                .param("sustain", sustain)
                .param("key_window", 400.0),
        );
    let factories: [&dyn BlockFactory; 1] = [&BuiltinFactory];
    let (mut g, report) = graph::build(
        &preset,
        &BuildOptions {
            sample_rate: SR,
            max_block: MAX,
            port: Some(ROOMS),
            factories: &factories,
            host: &NoHost,
        },
    );
    assert!(
        report.warnings.iter().all(|w| !w.contains("shape")),
        "{:?}",
        report.warnings
    );
    let latency = g.latency() as usize;
    g.join(Arc::new(Session {
        id: SessionId(tag.into()),
        hits: HitBus::new(),
    }));
    let (mut controller, mut port) = signal_core::processor_pair();
    let _ = port.install(g);
    let _ = controller.send(signal_core::engine::Command::PlannedHits {
        piece,
        hits: vec![(SNARE as u64, 1.0_f32)].into_boxed_slice(),
    });
    let input: Vec<f32> = (0..FRAMES)
        .map(|i| {
            let t = (i % EVERY) as f32;
            0.3 * (-t / 2_880.0).exp() * (t * 180.0 * std::f32::consts::TAU / SR).sin()
        })
        .collect();
    let mut out = vec![0.0_f32; FRAMES];
    let mut at = 0;
    while at < FRAMES {
        let end = (at + MAX).min(FRAMES);
        let mut l = input[at..end].to_vec();
        let mut r = l.clone();
        port.set_position(at as u64);
        port.process(&mut [&mut l, &mut r], 2, end - at, &[], &[]);
        out[at..end].copy_from_slice(&l);
        at = end;
    }
    controller.collect();
    (out, latency)
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0, |m, s| m.max(s.abs()))
}

#[test]
fn a_keyed_shaper_shapes_the_room_after_the_snare_only() {
    let (flat, latency) = render(0.0, 0.0, "shape-flat");
    let (room, _) = render(-6.0, 6.0, "shape-room");
    let at = SNARE + latency;
    // The hit before the snare's (the kick's): the same, sample for sample.
    assert_eq!(
        &room[..at - 100],
        &flat[..at - 100],
        "untouched before the snare"
    );
    let front = peak(&room[at..at + 240]) / peak(&flat[at..at + 240]);
    let tail = peak(&room[at + 7_200..at + 9_600]) / peak(&flat[at + 7_200..at + 9_600]);
    assert!(front < 0.8, "the snare's attack pulled back: {front}");
    assert!(tail > 1.3, "its sustain lifted: {tail}");
    // The next room hit (past the 400 ms window): untouched again.
    let next = at + EVERY;
    let after = peak(&room[next + 2_000..next + 12_000]) / peak(&flat[next + 2_000..next + 12_000]);
    assert!(
        (after - 1.0).abs() < 1e-4,
        "the next hit, not the snare's, untouched: {after}"
    );
}
