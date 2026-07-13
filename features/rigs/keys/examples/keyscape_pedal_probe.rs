//! Offline proof that the LA Custom Rhodes plays correctly *under the sustain
//! pedal*. Keyscape multi-sample packs ship a pedal-NOISE articulation
//! (`lacrped`, 6 fixed samples) that the engine used to mistake for a
//! pedal-down BODY and swap the whole keymap to — so every note held under the
//! pedal played the pedal clunk instead of the Rhodes tone. This probe holds a
//! note with the pedal down and confirms the body still sounds at full level.
//!   cargo run -p signal-keys --example keyscape_pedal_probe
//!
//! Also exercises the pedal-noise one-shots (CC64 up/down with no notes) and
//! the pedal-aware release tail.

use std::path::Path;

use signal_sampler::SamplerRig;

const PACK: &str = "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/\
Packs/Rhodes - LA Custom.signalpack";
const ID: &str = "rhodes";

/// Render `blocks × 512` frames and return the peak sample magnitude.
fn render_peak(rig: &SamplerRig, buf: &mut [f32], blocks: usize) -> f32 {
    let mut pk = 0.0f32;
    for _ in 0..blocks {
        buf.iter_mut().for_each(|s| *s = 0.0);
        let _ = rig.render_offline(buf);
        for &s in buf.iter() {
            pk = pk.max(s.abs());
        }
    }
    pk
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sr = 48_000;
    let rig = SamplerRig::new_offline(sr);
    println!("loading {PACK} …");
    rig.load_pack(ID, Path::new(PACK))?;
    rig.set_midi_channel(ID, 0);
    rig.set_default_instrument(ID);
    // Full preload so the pedal-noise samples (notes 0/1) are cached.
    let _ = rig.preload_instrument(ID);

    let mut buf = vec![0.0f32; 512 * 2];
    // Warm up so lazy samples decode.
    for _ in 0..80 {
        render_peak(&rig, &mut buf, 1);
        std::thread::sleep(std::time::Duration::from_millis(4));
    }

    // Measure the ATTACK peak of a note (strike, immediately capture).
    let strike_peak = |rig: &SamplerRig, buf: &mut [f32], note: u8| -> f32 {
        rig.midi_message(0, 0x90, note, 100);
        let pk = render_peak(rig, buf, 20); // capture attack + early decay
        let v = rig.active_voices(ID);
        rig.midi_message(0, 0x80, note, 0);
        let _ = render_peak(rig, buf, 70); // let the tail die before the next
        (pk, v).0
    };

    // ── A. no pedal: strike D4, measure the body ──────────────────────────
    let body_no_pedal = strike_peak(&rig, &mut buf, 62);
    println!("A. D4 body peak, no pedal   : {body_no_pedal:.4}");

    // ── B. pedal DOWN, then strike D4: body must still sound ──────────────
    rig.midi_message(0, 0xB0, 64, 127); // sustain pedal down
    let _ = render_peak(&rig, &mut buf, 8); // pedal-noise one-shot decays
    rig.midi_message(0, 0x90, 62, 100); // strike D4 under the pedal
    let body_pedal = render_peak(&rig, &mut buf, 20);
    println!("B. D4 body peak, pedal DOWN : {body_pedal:.4}  voices={}", rig.active_voices(ID));

    rig.midi_message(0, 0x80, 62, 0); // note-off (rings under pedal → relsl)
    rig.midi_message(0, 0xB0, 64, 0); // pedal up
    let _ = render_peak(&rig, &mut buf, 80);

    // ── C. bare pedal noise: CC64 down/up with no notes ───────────────────
    rig.midi_message(0, 0xB0, 64, 127);
    let noise_down = render_peak(&rig, &mut buf, 12);
    rig.midi_message(0, 0xB0, 64, 0);
    let noise_up = render_peak(&rig, &mut buf, 12);
    println!("C. pedal noise down/up peak : {noise_down:.4} / {noise_up:.4}");

    // ── verdict ───────────────────────────────────────────────────────────
    // The body under the pedal must be within a sane ratio of the un-pedaled
    // body. Before the fix it collapsed to the ~tiny pedal-noise sample.
    let ratio = if body_no_pedal > 0.0 { body_pedal / body_no_pedal } else { 0.0 };
    println!("\nbody(pedal)/body(no pedal) = {ratio:.2}");
    let ok = body_no_pedal > 0.01 && ratio > 0.3;
    println!(
        "{}",
        if ok {
            "PASS — note sounds as the Rhodes body under the sustain pedal"
        } else {
            "FAIL — note collapsed under the pedal (body swapped to pedal noise)"
        }
    );
    if !ok {
        std::process::exit(1);
    }
    Ok(())
}
