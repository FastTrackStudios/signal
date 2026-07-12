//! Dump the mixer of a GGD Cradle preset (MM2 `.preset` export or a snapshot
//! decoded from a Reaper RPP). Proves we can read MM2's per-piece
//! level/pan/mute/solo/sends/fx.
//!   cargo run -p signal-drums --example cradle_dump -- <path-to.preset>

use signal_drums::cradle;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: cradle_dump <path.preset>")?;
    let text = std::fs::read_to_string(&path)?;
    let mixer = cradle::parse_mixer(&text)?;

    println!("{} strips, {} cables\n", mixer.strips.len(), mixer.cables.len());
    println!("{:<16} {:>8} {:>7}  {:<4} {:>6} {:>4} {:>4}", "strip", "gain dB", "pan", "mute", "phase", "fx", "snd");
    for s in &mixer.strips {
        let db = if s.level > 0.0 { 20.0 * s.level.log10() } else { -99.0 };
        println!(
            "{:<16} {:>8.1} {:>7.2}  {:<4} {:>6.2} {:>4} {:>4}",
            trunc(&s.name, 16),
            db,
            s.pan,
            if s.mute { "M" } else if s.solo { "S" } else { "" },
            s.phase,
            s.fx.len(),
            s.sends.len(),
        );
    }
    let fx_total: usize = mixer.strips.iter().map(|s| s.fx.len()).sum();
    let snd_total: usize = mixer.strips.iter().map(|s| s.sends.len()).sum();
    println!("\ntotal FX slots: {fx_total}, total sends: {snd_total}");
    if fx_total == 0 {
        println!("(no FX in this preset — export a MIXED factory preset to see the EQ/comp/verb format)");
    }
    Ok(())
}

fn trunc(s: &str, n: usize) -> String {
    if s.len() <= n { s.to_string() } else { s.chars().take(n).collect() }
}
