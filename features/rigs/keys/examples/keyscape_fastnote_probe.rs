//! Diagnose inconsistent fast/repeated notes on the LA Custom Rhodes
//! (sometimes only a high harmonic, sometimes only the release noise). Drives
//! fast repeated strikes + a fast scale with the structured render trace on,
//! then prints exactly what each strike spawned: sample file, voice kind,
//! note→root pitch rate, and gain.
//!   cargo run -p signal-keys --example keyscape_fastnote_probe

use std::path::Path;

use signal_sampler::{SamplerRig, TraceKind};

const PACK: &str = "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/\
Packs/Rhodes - LA Custom.signalpack";
const ID: &str = "rhodes";
const SR: u32 = 48_000;
const BLK: usize = 512;

/// Render `blocks` of audio (advancing engine time).
fn render(rig: &SamplerRig, buf: &mut [f32], blocks: usize) {
    for _ in 0..blocks {
        buf.iter_mut().for_each(|s| *s = 0.0);
        let _ = rig.render_offline(buf);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rig = SamplerRig::new_offline(SR);
    rig.load_pack(ID, Path::new(PACK))?;
    rig.set_midi_channel(ID, 0);
    rig.set_default_instrument(ID);
    let _ = rig.preload_instrument(ID);

    let mut buf = vec![0.0f32; BLK * 2];
    render(&rig, &mut buf, 40); // warm

    // Release-tail dynamics read CC1 as the release velocity; set it so the
    // note-offs actually fire the lacr release (mirrors a controller sending
    // real release info), otherwise releases never trigger.
    rig.midi_message(0, 0xB0, 1, 100);
    rig.set_trace_enabled(ID, true);

    // ── fast repeated single note (60), ~40 ms apart, held ~20 ms each ──
    println!("== fast repeated note 60 (8×, ~40ms apart) ==");
    let gap = (0.040 * SR as f32 / BLK as f32) as usize; // ~40 ms in blocks
    let hold = (0.020 * SR as f32 / BLK as f32).max(1.0) as usize;
    for _ in 0..8 {
        rig.midi_message(0, 0x90, 60, 100);
        render(&rig, &mut buf, hold);
        rig.midi_message(0, 0x80, 60, 64);
        render(&rig, &mut buf, gap);
    }
    render(&rig, &mut buf, 20);

    // ── fast scale run (each note ~30 ms) ──
    println!("== fast scale 60..72 (~30ms/note) ==");
    let snap = (0.030 * SR as f32 / BLK as f32).max(1.0) as usize;
    for n in [60u8, 62, 64, 65, 67, 69, 71, 72] {
        rig.midi_message(0, 0x90, n, 100);
        render(&rig, &mut buf, snap);
        rig.midi_message(0, 0x80, n, 64);
    }
    render(&rig, &mut buf, 20);

    // ── voice-pool stress: dense fast run across the keyboard at varied
    // velocity, notes overlapping via short releases → piles up bodies +
    // 2 s release tails. This is where stealing shows up as bodies that
    // never sound (→ only the release/click is heard).
    println!("== dense stress run (48..96, ~15ms/note, held) ==");
    let quick = (0.015 * SR as f32 / BLK as f32).max(1.0) as usize;
    for n in 48u8..=96 {
        let vel = 30 + (n % 5) * 20; // 30..110
        rig.midi_message(0, 0x90, n, vel);
        render(&rig, &mut buf, quick);
    }
    for n in 48u8..=96 {
        rig.midi_message(0, 0x80, n, 64);
        render(&rig, &mut buf, 1);
    }
    render(&rig, &mut buf, 20);

    // ── dump the trace ──
    let trace = rig.render_trace(ID);
    let bl = |frame: u64| frame as f32 / SR as f32 * 1000.0; // ms
    println!("\n{:>7} {:>5} {:>12} {:>6} {:>6}  file", "t(ms)", "note", "kind", "rate", "gain");
    for e in &trace.events {
        match &e.kind {
            TraceKind::VoiceSpawn(v) => {
                println!(
                    "{:>7.0} {:>5} {:>12} {:>6.3} {:>6.3}  {}",
                    bl(e.frame), v.note, v.voice_kind, v.rate, v.gain, v.file
                );
            }
            TraceKind::NoteOff { note } => {
                println!("{:>7.0} {:>5} {:>12}", bl(e.frame), note, "NOTE-OFF");
            }
            TraceKind::Transition { from, to, .. } => {
                println!("{:>7.0} {:>2}->{:<2} {:>12}", bl(e.frame), from, to, "TRANSITION");
            }
            TraceKind::SampleMiss { note, articulation, dynamic, rr, reason } => {
                println!(
                    "{:>7.0} {:>5} {:>12}  {} dyn={} rr={} ({:?})",
                    bl(e.frame), note, "MISS", articulation, dynamic, rr, reason
                );
            }
        }
    }

    // Tally: body (SustainLo) vs Release spawns per note-on.
    let mut body = 0;
    let mut release = 0;
    let mut other = 0;
    for e in &trace.events {
        if let TraceKind::VoiceSpawn(v) = &e.kind {
            match v.voice_kind {
                "SustainLo" | "SustainLayer" => body += 1,
                "Release" => release += 1,
                _ => other += 1,
            }
        }
    }
    println!("\nspawns: body(sustain)={body} release={release} other={other}");
    Ok(())
}
