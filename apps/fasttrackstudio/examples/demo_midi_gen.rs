//! Author the browser keys rig's bundled demo SMFs.
//!
//! Writes the three preset `.mid` files the `/rigs/keys/:profile` page
//! bundles (`assets/demo-midi/`): a I–V–vi–IV pad progression, a simple
//! broken-chord piano figure, and a 16th-note arp line — all in C major,
//! authored at 74 BPM (the files carry no tempo meta; the page's
//! scheduler plays them at [`DEMO_BPM`]). Encoded with daw-proto's SMF
//! codec at 960 PPQ, so positions below are plain PPQ ticks.
//!
//! Run from the repo root:
//! ```bash
//! cargo run -p fasttrackstudio --example demo_midi_gen --no-default-features
//! ```

use daw_proto::midi::smf;
use daw_proto::midi::{MidiNoteCreate, MidiTakeContent};

/// The tempo the demo files are authored (and played back) at.
#[allow(dead_code)]
pub const DEMO_BPM: f64 = 74.0;

const PPQ: f64 = 960.0;
const BEAT: f64 = PPQ;
const BAR: f64 = 4.0 * BEAT;

/// The I–V–vi–IV progression in C major as (root, is_minor), one bar each.
const PROGRESSION: [(u8, bool); 4] = [
    (48, false), // C3
    (55, false), // G3
    (57, true),  // A3 minor
    (53, false), // F3
];

fn note(pitch: u8, velocity: u8, start_ppq: f64, length_ppq: f64) -> MidiNoteCreate {
    MidiNoteCreate {
        channel: 0,
        pitch,
        velocity,
        start_ppq,
        length_ppq,
    }
}

/// Chord tones (semitone offsets from the root) for a triad.
fn triad(minor: bool) -> [u8; 3] {
    if minor { [0, 3, 7] } else { [0, 4, 7] }
}

/// Sustained pad: whole-bar close-voiced triads + an octave root, slightly
/// overlapped so the swap never gaps. 4-bar loop × 4 ≈ 52 s at 74 BPM.
pub fn pads() -> MidiTakeContent {
    let mut c = MidiTakeContent::default();
    for cycle in 0..4 {
        for (bar, (root, minor)) in PROGRESSION.iter().enumerate() {
            let start = (cycle * 4 + bar) as f64 * BAR;
            let len = BAR + BEAT * 0.1;
            for off in triad(*minor) {
                c.notes.push(note(root + off + 12, 62, start, len));
            }
            c.notes.push(note(root - 12, 68, start, len)); // low root
        }
    }
    c
}

/// Simple piano figure: a broken chord per bar (root, fifth, tenth, octave
/// on the beats) with a held low root. 4-bar loop × 4 ≈ 52 s.
pub fn piano() -> MidiTakeContent {
    let mut c = MidiTakeContent::default();
    for cycle in 0..4 {
        for (bar, (root, minor)) in PROGRESSION.iter().enumerate() {
            let start = (cycle * 4 + bar) as f64 * BAR;
            let third = triad(*minor)[1];
            // Beats: root, fifth, third-above-octave, octave.
            let beats = [0u8, 7, third + 12, 12];
            for (i, off) in beats.iter().enumerate() {
                let vel = if i == 0 { 84 } else { 72 };
                c.notes
                    .push(note(root + off, vel, start + i as f64 * BEAT, BEAT * 1.6));
            }
            c.notes.push(note(root - 12, 76, start, BAR));
        }
    }
    c
}

/// 16th-note arp line over two octaves, accents on the beats.
/// 4-bar loop × 4 ≈ 52 s.
pub fn arp() -> MidiTakeContent {
    let mut c = MidiTakeContent::default();
    let sixteenth = BEAT / 4.0;
    for cycle in 0..4 {
        for (bar, (root, minor)) in PROGRESSION.iter().enumerate() {
            let start = (cycle * 4 + bar) as f64 * BAR;
            let t = triad(*minor);
            // Up-down over two octaves: 8 tones, played twice per bar.
            let tones = [
                t[0],
                t[1],
                t[2],
                t[0] + 12,
                t[1] + 12,
                t[2] + 12,
                t[0] + 24,
                t[1] + 12,
            ];
            for i in 0..16usize {
                let off = tones[i % tones.len()];
                let vel = if i % 4 == 0 { 88 } else { 68 };
                c.notes.push(note(
                    root + off + 12,
                    vel,
                    start + i as f64 * sixteenth,
                    sixteenth * 0.9,
                ));
            }
        }
    }
    c
}

/// Every preset: (file stem, display name, content).
pub fn presets() -> Vec<(&'static str, &'static str, MidiTakeContent)> {
    vec![
        ("pads", "Pad progression (I–V–vi–IV)", pads()),
        ("piano", "Piano figure", piano()),
        ("arp", "Arp line", arp()),
    ]
}

#[cfg_attr(test, allow(dead_code))]
fn main() -> std::io::Result<()> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/demo-midi");
    std::fs::create_dir_all(&dir)?;
    for (stem, name, content) in presets() {
        let path = dir.join(format!("{stem}.mid"));
        smf::write(path.to_str().expect("utf-8 path"), &content, PPQ)?;
        let bytes = std::fs::metadata(&path)?.len();
        println!(
            "{} — {name}: {} notes, {bytes} bytes",
            path.display(),
            content.notes.len()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each preset's encoded bytes (exactly what `main` writes) parse back
    /// through the same codec the browser page uses, with every note
    /// surviving.
    #[test]
    fn generated_presets_parse_back() {
        for (stem, _, content) in presets() {
            let bytes = smf::encode(&content, PPQ);
            let snap = smf::parse(&bytes, 0).unwrap_or_else(|| panic!("{stem} parses"));
            assert_eq!(
                snap.notes.len(),
                content.notes.len(),
                "{stem} notes survive"
            );
            assert!(snap.length_ppq > 15.0 * 4.0 * 960.0, "{stem} runs ~16 bars");
            // The demo scheduler assumes 960 PPQ.
            assert_eq!(snap.ppq, 960.0, "{stem} PPQ");
        }
    }

    /// The committed assets (once generated) still match this generator.
    #[test]
    fn committed_assets_match_generator() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/demo-midi");
        for (stem, _, content) in presets() {
            let path = dir.join(format!("{stem}.mid"));
            let Ok(on_disk) = std::fs::read(&path) else {
                // Not generated yet — `cargo run --example demo_midi_gen`.
                continue;
            };
            assert_eq!(
                on_disk,
                smf::encode(&content, PPQ),
                "{} is stale — rerun demo_midi_gen",
                path.display()
            );
        }
    }
}
