//! The snare's Non-Linear keyed by its placed hits, as Studio runs it: the
//! kit's `Drums/Snare/Verb/Nonlin` port fed the snare's sum (a send, bleed
//! and all) and keyed by `Drums/Snare/Sum/Trig`'s placed hits. Its own
//! onset detector (+12 dB over a 150 ms follower) never fires on that sum;
//! keyed, each hit opens a gated burst.

use std::sync::Arc;

use signal_core::graph::{self, BuildOptions};
use signal_core::host::NoHost;
use signal_core::kit::{KitPieces, drum_kit};
use signal_core::session::{HitBus, Session, SessionId};
use signal_core::{BlockFactory, BuiltinFactory};

const SR: f32 = 48_000.0;
const MAX: usize = 1_024;
const NONLIN: &str = "Drums/Snare/Verb/Nonlin";
const SNARE_TRIG: &str = "Drums/Snare/Sum/Trig";
const FRAMES: usize = 48_000 * 3;
const HITS: [usize; 2] = [24_000, 84_000];

/// The snare's sum: the kit's bleed throughout, the snare ~6 dB over it.
fn snare_sum() -> Vec<f32> {
    let mut x = 0x1357_9bdf_u32;
    let mut out: Vec<f32> = (0..FRAMES)
        .map(|_| {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            0.3 * ((x >> 8) as f32 / 8_388_608.0 - 1.0)
        })
        .collect();
    for &h in &HITS {
        for (i, s) in out.iter_mut().enumerate().skip(h).take(4_800) {
            let t = (i - h) as f32;
            *s += 0.6 * (-t / 1_200.0).exp() * (t * 200.0 * std::f32::consts::TAU / SR).sin();
        }
    }
    out
}

fn render(keyed: bool, tag: &str) -> (Vec<f32>, usize) {
    let mut preset = drum_kit(&KitPieces::default());
    let piece = preset
        .lanes
        .iter()
        .position(|l| l.name == SNARE_TRIG)
        .and_then(|i| u16::try_from(i).ok())
        .expect("the snare's trigger");
    let lane = preset
        .lanes
        .iter_mut()
        .find(|l| l.name == NONLIN)
        .expect("the snare's Non-Linear");
    lane.mute = false;
    lane.gain_db = 0.0;
    let verb = lane
        .blocks
        .iter_mut()
        .find(|b| b.id == "verb")
        .expect("its verb");
    assert_eq!(verb.key.as_deref(), Some(SNARE_TRIG), "the kit keys it");
    if !keyed {
        verb.key = None;
    }
    *verb = verb.clone().param("decay", 0.08).param("extra_a", 0.3);
    let factories: [&dyn BlockFactory; 1] = [&BuiltinFactory];
    let (mut g, _) = graph::build(
        &preset,
        &BuildOptions {
            sample_rate: SR,
            max_block: MAX,
            port: Some(NONLIN),
            factories: &factories,
            host: &NoHost,
        },
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
        hits: HITS
            .iter()
            .map(|&h| (h as u64, 1.0_f32))
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    });
    let input = snare_sum();
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
fn unkeyed_the_snares_nonlinear_never_opens_on_its_hits() {
    let (out, latency) = render(false, "nonlin-unkeyed");
    // One window on the bleed's start from silence, then never again.
    let rest = peak(&out[latency + 14_000..]);
    assert!(rest < 1e-6, "silent on the hits: {rest}");
}

#[test]
fn keyed_by_the_snares_placed_hits_it_bursts_on_each() {
    let (out, latency) = render(true, "nonlin-keyed");
    let window = 12_000; // its 0.25 s
    assert!(
        peak(&out[..latency + HITS[0]]) < 1e-6,
        "silent before the first hit"
    );
    for &h in &HITS {
        let at = latency + h;
        let burst = peak(&out[at..at + window]);
        assert!(burst > 0.05, "a burst on the hit at {h}: {burst}");
        let after = peak(&out[at + window + 500..at + window + 20_000]);
        assert!(after < 1e-6, "shut after the window at {h}: {after}");
        assert!(
            peak(&out[at - 2_000..at]) < 1e-6,
            "not before its hit at {h}"
        );
    }
}
