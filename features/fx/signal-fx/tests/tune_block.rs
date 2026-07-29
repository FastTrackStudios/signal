//! NativeTune: correction accuracy, retune independence, MIDI targets.

use signal_fx::NativeTune;
use signal_plugin_host::{PluginEvents, PluginInstance, PluginMidiEvent};

const SR: f64 = 48000.0;
const N: usize = 144_000;

fn sine(freq: f64, amp: f64, n: usize) -> Vec<f32> {
    (0..n).map(|i| (amp * (core::f64::consts::TAU * freq * i as f64 / SR).sin()) as f32).collect()
}

fn tone(buf: &[f32], freq: f64) -> f64 {
    let (mut re, mut im) = (0.0f64, 0.0f64);
    for (i, &x) in buf.iter().enumerate() {
        let ph = core::f64::consts::TAU * freq * i as f64 / SR;
        re += f64::from(x) * ph.cos();
        im += f64::from(x) * ph.sin();
    }
    (re * re + im * im).sqrt() / buf.len() as f64
}

fn run(t: &mut NativeTune, input: &[f32], block: usize, midi_at: Option<(usize, u8)>) -> Vec<f32> {
    t.prepare(SR, block as u32).unwrap();
    let n = input.len();
    let (mut ol, mut or) = (vec![0.0f32; n], vec![0.0f32; n]);
    for (bi, chunk) in input.chunks(block).enumerate() {
        let s = bi * block;
        let midi_events: Vec<PluginMidiEvent> = match midi_at {
            Some((at, key)) if at >= s && at < s + chunk.len() => vec![PluginMidiEvent {
                offset: (at - s) as u32,
                message: daw_proto::MidiEvent::NoteOn {
                    channel: midicore_proto::Channel::new(0),
                    key: midicore_proto::KeyNumber::new(key),
                    velocity: midicore_proto::Velocity::new(100),
                },
            }],
            _ => vec![],
        };
        t.process_block(
            chunk,
            chunk,
            &mut ol[s..s + chunk.len()],
            &mut or[s..s + chunk.len()],
            &PluginEvents { params: &[], midi: &midi_events, note_expressions: &[] },
        )
        .unwrap();
    }
    ol
}

#[test]
fn corrects_a_sharp_a_to_pitch() {
    // 452 Hz (A4 + ~47 cents) → chromatic snap must land at 440.
    let mut t = NativeTune::new(SR);
    t.set_named("retune_ms", 5.0);
    let input = sine(452.0, 0.5, N);
    let out = run(&mut t, &input, 512, None);
    let late = &out[N / 2..];
    let at_440 = tone(late, 440.0);
    let at_452 = tone(late, 452.0);
    assert!(
        at_440 > at_452 * 2.0,
        "output should center on 440: e440={at_440:e} e452={at_452:e}"
    );
    // Readback drives the tuner UI.
    let detected = t.param_value(signal_fx::TUNE_DETECTED_MIDI_ID).unwrap();
    assert!((detected - 69.47).abs() < 0.2, "detected ≈ A4+47c: {detected:.2}");
}

#[test]
fn retune_time_is_buffer_size_independent() {
    // The shipped plugin's bug: slew coeff applied per block. Here the
    // settle profile must match between 64- and 1024-sample blocks.
    let settle = |block: usize| -> f64 {
        let mut t = NativeTune::new(SR);
        t.set_named("retune_ms", 150.0);
        let input = sine(452.0, 0.5, N);
        let out = run(&mut t, &input, block, None);
        // Energy at the target in a mid window (during the glide).
        tone(&out[36_000..60_000], 440.0)
    };
    let fast = settle(64);
    let slow = settle(1024);
    let ratio = fast / slow.max(1e-12);
    assert!(
        (0.5..2.0).contains(&ratio),
        "retune progress must not depend on block size: {ratio:.2}"
    );
}

#[test]
fn midi_latch_overrides_the_scale() {
    // Input at A4 (in tune) but MIDI latches D4 (62): output must move
    // toward D (−7 semitones → ~293.7 Hz).
    let mut t = NativeTune::new(SR);
    t.set_named("midi_mode", 1.0);
    t.set_named("retune_ms", 5.0);
    let input = sine(440.0, 0.5, N);
    let out = run(&mut t, &input, 512, Some((4800, 62)));
    let late = &out[N / 2..];
    let at_d = tone(late, 293.66);
    let at_a = tone(late, 440.0);
    assert!(
        at_d > at_a,
        "MIDI latch should drag the note to D4: d={at_d:e} a={at_a:e}"
    );
}

#[test]
fn pc_bypass_leaves_blue_notes_alone() {
    // Bypass A (pc 9): a sharp A stays uncorrected.
    let mut t = NativeTune::new(SR);
    t.set_named("retune_ms", 5.0);
    t.set_named("pc_bypass_9", 1.0);
    let input = sine(452.0, 0.5, N);
    let out = run(&mut t, &input, 512, None);
    let late = &out[N / 2..];
    let at_452 = tone(late, 452.0);
    let at_440 = tone(late, 440.0);
    assert!(
        at_452 > at_440 * 2.0,
        "bypassed pitch class must pass as sung: e452={at_452:e} e440={at_440:e}"
    );
}
