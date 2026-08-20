//! Offline proof that a Keyscape signalpack plays through the shared sampler:
//! load the LA Custom C7 Grand, hit middle C, render, and confirm audio.
//!   cargo run -p signal-keys --example keyscape_probe

use std::path::Path;

use signal_sampler::SamplerRig;

const PACK: &str = "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/\
Packs/LA Custom C7 Grand.signalpack";
const ID: &str = "piano";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sr = 48_000;
    let rig = SamplerRig::new_offline(sr);
    println!("loading {PACK} …");
    let t = std::time::Instant::now();
    rig.load_pack(ID, Path::new(PACK))?;
    rig.set_midi_channel(ID, 0);
    rig.set_default_instrument(ID);
    println!("loaded in {:?}", t.elapsed());

    let mut buf = vec![0.0f32; 512 * 2];
    // Warm up so lazy samples decode.
    for _ in 0..60 {
        buf.iter_mut().for_each(|s| *s = 0.0);
        let _ = rig.render_offline(&mut buf);
        std::thread::sleep(std::time::Duration::from_millis(8));
    }
    // Play a chord: C4 E4 G4 (60/64/67) at mf.
    for note in [60u8, 64, 67] {
        rig.midi_message(0, 0x90, note, 96);
    }
    let (mut pk, mut rms, mut n) = (0.0f32, 0.0f64, 0u64);
    for _ in 0..48 {
        buf.iter_mut().for_each(|s| *s = 0.0);
        rig.render_offline(&mut buf)?;
        for &s in buf.iter() {
            pk = pk.max(s.abs());
            rms += (s as f64) * (s as f64);
            n += 1;
        }
    }
    let rms = (rms / n as f64).sqrt();
    println!("voices: {}", rig.active_voices(ID));
    println!("peak {pk:.4}  rms {rms:.5}");
    println!(
        "\n{}",
        if pk > 0.001 {
            "PASS — Keyscape C7 grand plays"
        } else {
            "FAIL — silent"
        }
    );
    Ok(())
}
