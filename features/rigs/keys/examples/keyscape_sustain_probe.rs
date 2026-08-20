//! Diagnose "note dies right after the attack while held" on the LA Custom
//! Rhodes. For each key: strike it (no note-off), hold ~2 s, and compare the
//! attack peak to the level still sounding at ~1.5 s. A healthy long Rhodes
//! sample should still be clearly audible; a "died" note collapses to ~0.
//!   cargo run -p signal-keys --example keyscape_sustain_probe

use std::path::Path;

use signal_sampler::SamplerRig;

const PACK: &str = "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/\
Packs/Rhodes - LA Custom.signalpack";
const ID: &str = "rhodes";
const SR: u32 = 48_000;
const BLK: usize = 512;

/// Render `secs` of audio; return (peak over the whole span, peak in the
/// window `[from_s, to_s)`).
fn render_window(
    rig: &SamplerRig,
    buf: &mut [f32],
    secs: f32,
    from_s: f32,
    to_s: f32,
) -> (f32, f32) {
    let blocks = (secs * SR as f32 / BLK as f32) as usize;
    let from_b = (from_s * SR as f32 / BLK as f32) as usize;
    let to_b = (to_s * SR as f32 / BLK as f32) as usize;
    let (mut whole, mut win) = (0.0f32, 0.0f32);
    for b in 0..blocks {
        buf.iter_mut().for_each(|s| *s = 0.0);
        let _ = rig.render_offline(buf);
        let mut bpk = 0.0f32;
        for &s in buf.iter() {
            bpk = bpk.max(s.abs());
        }
        whole = whole.max(bpk);
        if b >= from_b && b < to_b {
            win = win.max(bpk);
        }
    }
    (whole, win)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rig = SamplerRig::new_offline(SR);
    println!("loading {PACK} …");
    rig.load_pack(ID, Path::new(PACK))?;
    rig.set_midi_channel(ID, 0);
    rig.set_default_instrument(ID);
    let _ = rig.preload_instrument(ID);

    let mut buf = vec![0.0f32; BLK * 2];
    // Warm the renderer.
    let _ = render_window(&rig, &mut buf, 0.2, 0.0, 0.1);

    // "Hold a note for a second or two and you should still hear it" — measure
    // the level still sounding at ~1.0 s across the whole keyboard.
    println!("\n{:>4} {:>8} {:>8}  verdict", "note", "attack", "@1.0s");
    let mut died = Vec::new();
    let mut alive = 0;
    for note in 21u8..=108 {
        rig.midi_message(0, 0x90, note, 100);
        let (attack, sustain) = render_window(&rig, &mut buf, 1.3, 0.95, 1.05);
        rig.midi_message(0, 0x80, note, 0);
        let _ = render_window(&rig, &mut buf, 0.8, 0.0, 0.1); // let the tail die
                                                              // Audible at 1 s = still ≥1% of the attack and above the noise floor.
        let ok = sustain >= 0.002 && sustain >= 0.01 * attack;
        if ok {
            alive += 1
        } else {
            died.push(note)
        }
        if note % 3 == 0 {
            println!(
                "{note:>4} {attack:>8.4} {sustain:>8.4}  {}",
                if ok { "ok" } else { "DIED" }
            );
        }
    }
    println!(
        "\naudible-at-1s: {alive}/{}   still-dead: {died:?}",
        108 - 21 + 1
    );
    Ok(())
}
