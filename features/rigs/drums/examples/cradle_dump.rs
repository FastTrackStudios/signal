//! Dump the mixer of a GGD Cradle preset (MM2 `.preset` export or a snapshot
//! decoded from a Reaper RPP). Proves we can read MM2's per-piece
//! level/pan/mute/solo/sends/fx.
//!   cargo run -p signal-drums --example cradle_dump -- <path-to.preset>

use signal_drums::cradle;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: cradle_dump <path.preset>")?;
    let text = std::fs::read_to_string(&path)?;
    let mixer = cradle::parse_mixer(&text)?;

    println!(
        "{} strips, {} cables\n",
        mixer.strips.len(),
        mixer.cables.len()
    );
    for s in &mixer.strips {
        let db = if s.level > 0.0 {
            20.0 * s.level.log10()
        } else {
            -99.0
        };
        let flag = if s.mute {
            " [MUTE]"
        } else if s.solo {
            " [SOLO]"
        } else {
            ""
        };
        println!(
            "── {:<14} {:>6.1} dB  pan {:>5.2}{}",
            s.name, db, s.pan, flag
        );
        for fx in s.fx_slots() {
            let byp = if fx.bypass { " (bypassed)" } else { "" };
            let detail = match fx.fx_type.as_str() {
                "EQ" => {
                    let bands: Vec<String> = fx
                        .eq_bands()
                        .iter()
                        .filter(|b| b.enabled)
                        .map(|b| format!("{:.0}Hz {:+.1}dB Q{:.2} {}", b.freq, b.gain, b.q, b.mode))
                        .collect();
                    format!("{} bands: {}", bands.len(), bands.join(" | "))
                }
                "Modern Compressor" | "Vintage Compressor" => format!(
                    "thr {} ratio {} atk {} rel {} knee {:?} mix {:.2}",
                    fx.num("threshold")
                        .map(|v| format!("{v:.1}"))
                        .unwrap_or_default(),
                    fx.num("ratio")
                        .map(|v| format!("{v:.2}"))
                        .or_else(|| fx.text("ratio").map(str::to_string))
                        .unwrap_or_default(),
                    fx.num("attack")
                        .map(|v| format!("{v:.3}"))
                        .or_else(|| fx.text("attack").map(str::to_string))
                        .unwrap_or_default(),
                    fx.num("release")
                        .map(|v| format!("{v:.3}"))
                        .or_else(|| fx.text("release").map(str::to_string))
                        .unwrap_or_default(),
                    fx.text("knee").unwrap_or(""),
                    fx.num("mix").unwrap_or(1.0),
                ),
                "Transient" => format!(
                    "attack {:.3} sustain {:.3} mix {:.2}",
                    fx.num("attack").unwrap_or(0.0),
                    fx.num("sustain").unwrap_or(0.0),
                    fx.num("mix").unwrap_or(1.0)
                ),
                "Drive" => format!(
                    "drive {:.2} mode {:?}",
                    fx.num("drive").unwrap_or(0.0),
                    fx.text("mode").unwrap_or("")
                ),
                "Reverb" => format!(
                    "mode {:?} decay {:.2} pre {:.3} hp {:.2} lp {:.2}",
                    fx.text("mode").unwrap_or(""),
                    fx.num("decay").unwrap_or(0.0),
                    fx.num("preDelay").unwrap_or(0.0),
                    fx.num("hipass").unwrap_or(0.0),
                    fx.num("lopass").unwrap_or(0.0)
                ),
                "Limiter" => format!("threshold {:.1}", fx.num("threshold").unwrap_or(0.0)),
                _ => String::new(),
            };
            println!("     {:<18} {}{}", fx.fx_type, detail, byp);
            if !fx.preset_name.is_empty() {
                println!("     {:<18} └ \"{}\"", "", fx.preset_name);
            }
        }
    }
    let fx_total: usize = mixer.strips.iter().map(|s| s.fx.len()).sum();
    println!("\ntotal FX slots: {fx_total}");
    Ok(())
}
