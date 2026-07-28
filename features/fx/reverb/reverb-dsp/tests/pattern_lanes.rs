//! MSEG pattern-lane behavior on the reverb chain.

use audiocore_dsp::mseg::{Mseg, MsegLane, MsegPoint, MsegRate};
use audiocore_dsp::{AudioConfig, Processor};
use reverb_dsp::{AlgorithmType, ReverbChain};

const SR: f64 = 48000.0;

fn make() -> ReverbChain {
    let mut c = ReverbChain::new();
    c.set_algorithm(AlgorithmType::Hall);
    c.mix = 1.0;
    c.params.decay = 0.4;
    c.tempo_bpm = Some(120.0);
    c.update(AudioConfig {
        sample_rate: SR,
        max_buffer_size: 4096,
    });
    c
}

#[test]
fn wet_lane_duty_cycles_the_output() {
    let mut c = make();
    c.set_wet_pattern(Some(MsegLane::new(Mseg::gate(0.5), MsegRate::SyncBeats(1.0))));

    // Steady sine; the wet output must alternate loud/quiet at the
    // half-beat (12000-sample) boundary.
    let n = 96_000;
    let mut l: Vec<f64> = (0..n)
        .map(|i| (core::f64::consts::TAU * 300.0 * i as f64 / SR).sin() * 0.4)
        .collect();
    let mut r = l.clone();
    c.process(&mut l, &mut r);

    // Compare on-vs-off windows in a late cycle (24000-sample beat).
    let cyc = 24_000;
    let base = 2 * cyc;
    let on: f64 = l[base + 2000..base + 10_000].iter().map(|x| x * x).sum();
    let off: f64 = l[base + cyc / 2 + 2000..base + cyc / 2 + 10_000]
        .iter()
        .map(|x| x * x)
        .sum();
    assert!(
        on > off * 20.0,
        "wet gate should duty-cycle the output: on={on:.4} off={off:.4}"
    );
    for v in &l {
        assert!(v.is_finite());
    }
}

#[test]
fn send_lane_gates_the_reverb_input() {
    // Send gate at 0 during the second half-beat: notes played there
    // must produce far less tail than notes in the open half.
    let run = |note_at_closed_half: bool| -> f64 {
        let mut c = make();
        c.set_send_pattern(Some(MsegLane::new(
            Mseg::gate(0.5),
            MsegRate::SyncBeats(1.0),
        )));
        let cyc = 24_000usize;
        let note_start = if note_at_closed_half { cyc / 2 + 2000 } else { 2000 };
        let n = cyc * 3;
        let mut l = vec![0.0f64; n];
        for i in 0..2000 {
            l[note_start + i] =
                (core::f64::consts::TAU * 400.0 * i as f64 / SR).sin() * 0.6;
        }
        let mut r = l.clone();
        c.process(&mut l, &mut r);
        // Tail energy after the note, away from later gate openings of
        // the same audio (measure the immediate 4k window).
        l[note_start + 2200..note_start + 6200]
            .iter()
            .map(|x| x * x)
            .sum()
    };
    let open = run(false);
    let closed = run(true);
    assert!(
        open > closed * 5.0,
        "closed-half notes should barely reach the reverb: open={open:.4} closed={closed:.4}"
    );
}

#[test]
fn clear_tails_point_kills_the_wash() {
    let mut c = make();
    c.params.decay = 0.9;
    c.update_params();
    let mut points = vec![MsegPoint::new(0.0, 1.0)];
    let mut clear_pt = MsegPoint::new(0.5, 1.0);
    clear_pt.clear_tails = true;
    points.push(clear_pt);
    c.set_wet_pattern(Some(MsegLane::new(
        Mseg::new(points),
        MsegRate::SyncBeats(2.0), // one cycle = 2 beats = 48000 samples
    )));

    // Ring the hall, go silent; the tail must die abruptly when the
    // clear point crosses at 24000 samples into the cycle.
    let n = 96_000;
    let mut l = vec![0.0f64; n];
    for i in 0..4000 {
        l[i] = (core::f64::consts::TAU * 350.0 * i as f64 / SR).sin() * 0.7;
    }
    let mut r = l.clone();
    c.process(&mut l, &mut r);

    let before: f64 = l[18_000..23_000].iter().map(|x| x * x).sum();
    let after: f64 = l[25_000..30_000].iter().map(|x| x * x).sum();
    assert!(
        after < before * 0.05,
        "clear-tails point should kill the wash: before={before:.4} after={after:.4}"
    );
}
