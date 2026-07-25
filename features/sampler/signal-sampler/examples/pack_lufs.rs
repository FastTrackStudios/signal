//! `pack_lufs` — measure a pack's working loudness, so every library can be
//! trimmed onto one level.
//!
//! Sampled libraries are mastered wherever their author liked. Keyscape's
//! pianos sit far under the Omnisphere soundsources, so a mixer at unity is
//! not a mix — it is a correction. This renders a fixed musical gesture
//! through a pack and reports its integrated loudness (ITU-R BS.1770, the
//! same measure the NAM calibration uses), plus the trim it needs toward a
//! target.
//!
//! The gesture matters more than the number's absolute value: every pack is
//! measured playing THE SAME notes at THE SAME velocity, so the comparison
//! between packs is honest even though "a piano's loudness" depends entirely
//! on what you play.
//!
//! ```bash
//! cargo run -p signal-sampler --release --example pack_lufs -- <pack.signalpack> [target_lufs]
//! ```

use std::path::Path;

use signal_sampler::SamplerRig;
use signal_sampler::engine::budget;
use signal_sampler::loudness::integrated_lufs;

/// A mid-register chord, held — what a keys player leans on, and what any
/// pack can be asked to play.
const NOTES: [u8; 4] = [48, 55, 64, 72];
/// Mezzo-forte: the velocity a level judgement is normally made at.
const VELOCITY: u8 = 96;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let pack = args.next().ok_or("usage: pack_lufs <pack.signalpack> [target_lufs]")?;
    let target: f64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(-18.0);
    let sr = 48_000usize;

    let rig = SamplerRig::new_offline(sr as u32);
    rig.load_pack("pack", Path::new(&pack))?;
    rig.set_midi_channel("pack", 0);
    rig.set_default_instrument("pack");

    // Let the preloader settle: notes played while heads are still decoding
    // measure the preloader, not the instrument.
    let mut buf = vec![0.0f32; 512 * 2];
    for _ in 0..60 {
        buf.fill(0.0);
        let _ = rig.render_offline(&mut buf);
    }
    let mut last = 0u64;
    for _ in 0..600 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        let now = budget::used_bytes();
        if now == last && now > 0 {
            break;
        }
        last = now;
    }

    // Play the chord, hold it for two seconds, release, and let it ring.
    for n in NOTES {
        rig.midi_message(0, 0x90, n, VELOCITY);
    }
    let hold_blocks = (sr as f64 * 2.0 / 512.0) as usize;
    let tail_blocks = (sr as f64 * 2.0 / 512.0) as usize;
    let mut mono: Vec<f64> = Vec::with_capacity((hold_blocks + tail_blocks) * 512);
    let mut peak = 0.0f32;
    for b in 0..(hold_blocks + tail_blocks) {
        if b == hold_blocks {
            for n in NOTES {
                rig.midi_message(0, 0x80, n, 0);
            }
        }
        buf.fill(0.0);
        rig.render_offline(&mut buf)?;
        for f in buf.chunks_exact(2) {
            peak = peak.max(f[0].abs()).max(f[1].abs());
            // Mono sum for the meter: every pack is measured the same way, so
            // the comparison between them holds even though this is not a
            // full multichannel BS.1770 sum.
            mono.push(0.5 * (f[0] as f64 + f[1] as f64));
        }
    }

    let lufs = integrated_lufs(&mono, sr as f64);
    let trim = target - lufs;
    let name = Path::new(&pack).file_stem().and_then(|s| s.to_str()).unwrap_or("pack");
    println!("{name}");
    println!("  integrated  {lufs:>8.2} LUFS");
    println!("  peak        {:>8.2} dBFS", 20.0 * (peak.max(1e-9) as f64).log10());
    println!("  trim to {target:>5.1}  {trim:>+8.2} dB");
    println!("\n  {{name \"{name}\", lufs {lufs:.2}}}");
    Ok(())
}
