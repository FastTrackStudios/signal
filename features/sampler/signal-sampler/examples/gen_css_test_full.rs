//! Generate a COMPREHENSIVE Cinematic Studio Strings test MIDI, designed from
//! the actual CSS manual to extract maximum matching data. Notes are generously
//! spaced and isolated so every event can be windowed cleanly; CC59 resets make
//! round-robins deterministic (manual §RR reset); analysis uses recorded keys
//! (the {A,B,C#,D#,F,G} whole-tone grid) so pitch-shift is excluded as a
//! variable, plus a dedicated interpolation probe on the off-grid keys.
//!
//! Controller map (CSS defaults): CC58 keyswitch · CC1 dynamics (longs) /
//! short-type select · CC2 vibrato x-fade · CC5 portamento volume · CC11 volume
//! · CC59 RR reset. Portamento velocity default = 20.
//!
//! ```text
//! cargo run -p signal-sampler --example gen_css_test_full -- css_test_full.mid
//! ```
//! Render through stock CSS 1st Violins (default Mix mic, reverb 0), send back
//! the WAV; the printed manifest maps every section to its timestamp.

use std::io::Write;

const DIV: u16 = 480;
const TPS: f64 = 960.0; // ticks/sec @ 120 BPM

// CC58 keyswitch values (verified against the manual's table).
const SPICCATO: u8 = 13;
const STACCATISSIMO: u8 = 18;
const STACCATO: u8 = 23;
const SFZ: u8 = 28;
const PIZZICATO: u8 = 33;
const BARTOK: u8 = 38;
const COL_LEGNO: u8 = 43;
const MARCATO: u8 = 68;
const TRILLS: u8 = 48;
const HARMONICS: u8 = 53;
const TREMOLO: u8 = 58;
const MEAS_TREMOLO: u8 = 63;
const EXPR_LEGATO: u8 = 8;
const LOW_LAT_LEGATO: u8 = 2;

const SHORTS: &[(&str, u8)] = &[
    ("Spiccato", SPICCATO),
    ("Staccatissimo", STACCATISSIMO),
    ("Staccato", STACCATO),
    ("Sfz", SFZ),
    ("Pizzicato", PIZZICATO),
    ("Bartok", BARTOK),
    ("ColLegno", COL_LEGNO),
    ("Marcato", MARCATO),
];

/// Recorded keys (the whole-tone grid) — no pitch-shift on these.
const RECORDED_RANGE: &[(u8, &str)] = &[
    (43, "G2"),
    (57, "A3"),
    (61, "C#4"),
    (65, "F4"),
    (67, "G4"),
    (69, "A4"),
    (73, "C#5"),
    (79, "G5"),
];
const NOTE: u8 = 67; // G4, primary analysis pitch (recorded)

struct Smf {
    ev: Vec<(u32, u32, Vec<u8>)>,
    seq: u32,
}

impl Smf {
    fn new() -> Self {
        Self {
            ev: Vec::new(),
            seq: 0,
        }
    }
    fn raw(&mut self, sec: f64, bytes: Vec<u8>) {
        let t = (sec * TPS).round() as u32;
        self.ev.push((t, self.seq, bytes));
        self.seq += 1;
    }
    fn cc(&mut self, sec: f64, ctrl: u8, val: u8) {
        self.raw(sec, vec![0xB0, ctrl, val]);
    }
    fn note(&mut self, sec: f64, dur: f64, note: u8, vel: u8) {
        self.raw(sec, vec![0x90, note, vel]);
        self.raw(sec + dur, vec![0x80, note, 0]);
    }
    /// Legato transition: hold `a`, overlap `b`, release `a`, sustain `b`.
    fn legato(&mut self, sec: f64, a: u8, b: u8, vel: u8, hold: f64) {
        self.raw(sec, vec![0x90, a, vel]);
        self.raw(sec + 0.5, vec![0x90, b, vel]); // overlap → transition fires
        self.raw(sec + 0.6, vec![0x80, a, 0]); // release first (last-note priority)
        self.raw(sec + 0.5 + hold, vec![0x80, b, 0]);
    }
    fn cc_ramp(&mut self, t0: f64, dur: f64, ctrl: u8, a: u8, b: u8) {
        let steps = 96;
        for i in 0..=steps {
            let f = i as f64 / steps as f64;
            let v = (a as f64 + (b as f64 - a as f64) * f).round() as u8;
            self.cc(t0 + dur * f, ctrl, v);
        }
    }
    fn write(mut self, path: &str) -> std::io::Result<()> {
        self.ev.sort_by_key(|(t, s, _)| (*t, *s));
        let mut track: Vec<u8> = Vec::new();
        write_vlq(&mut track, 0);
        track.extend_from_slice(&[0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20]); // 120 BPM
        let mut prev = 0u32;
        for (t, _, bytes) in &self.ev {
            write_vlq(&mut track, t - prev);
            track.extend_from_slice(bytes);
            prev = *t;
        }
        write_vlq(&mut track, 0);
        track.extend_from_slice(&[0xFF, 0x2F, 0x00]);
        let mut f = std::fs::File::create(path)?;
        f.write_all(b"MThd")?;
        f.write_all(&6u32.to_be_bytes())?;
        f.write_all(&0u16.to_be_bytes())?;
        f.write_all(&1u16.to_be_bytes())?;
        f.write_all(&DIV.to_be_bytes())?;
        f.write_all(b"MTrk")?;
        f.write_all(&(track.len() as u32).to_be_bytes())?;
        f.write_all(&track)?;
        Ok(())
    }
}

fn write_vlq(out: &mut Vec<u8>, mut v: u32) {
    let mut buf = [0u8; 5];
    let mut i = 0;
    buf[i] = (v & 0x7f) as u8;
    i += 1;
    v >>= 7;
    while v > 0 {
        buf[i] = ((v & 0x7f) as u8) | 0x80;
        i += 1;
        v >>= 7;
    }
    for j in (0..i).rev() {
        out.push(buf[j]);
    }
}

fn main() -> std::io::Result<()> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "css_test_full.mid".into());
    let mut m = Smf::new();
    let mut t = 1.0;

    // Baseline controllers (CSS defaults).
    m.cc(0.0, 11, 127); // volume full
    m.cc(0.0, 2, 127); // vibrato on
    m.cc(0.0, 1, 64); // mid

    println!("# CSS COMPREHENSIVE test MIDI — section manifest (seconds @ 120 BPM, ch1)");
    println!("# Recorded keys only unless noted. Shorts have a 60ms sample-start→peak delay.\n");

    // ── 1. Calibration: one long ff sustain (output-gain reference) ──
    m.cc(t, 58, EXPR_LEGATO);
    m.cc(t, 2, 127);
    m.cc(t, 1, 127);
    println!(
        "{:7.2}  CALIB  sustain G4 ff held 6s (output-gain reference)",
        t + 0.1
    );
    m.note(t + 0.1, 6.0, NOTE, 100);
    t += 8.0;

    // ── 2. SHORTS: velocity → dynamic layer + gain (per type) ──
    println!("\n# 2. SHORT velocity sweep (vel 1..127 step ~9, isolated, RR-reset)");
    for (name, ks) in SHORTS {
        m.cc(t, 58, *ks);
        m.cc(t, 59, 0); // RR reset → deterministic cycle from slot 0
        let start = t + 0.2;
        println!(
            "{:7.2}  SHORT-VEL {name:<14} G4 @ vel 1,9,..127 (15 notes, 1.0s apart)",
            start
        );
        for (i, vel) in (1u8..=127).step_by(9).enumerate() {
            m.note(start + i as f64 * 1.0, 0.35, NOTE, vel.max(1));
        }
        t = start + 16.0 + 1.0;
    }

    // ── 3. SHORTS: round-robin exposure (deterministic cycle) ──
    println!("\n# 3. SHORT RR exposure: CC59 reset then 12× same note (captures each RR in order)");
    for (name, ks) in SHORTS {
        m.cc(t, 58, *ks);
        m.cc(t, 59, 0);
        let start = t + 0.2;
        println!(
            "{:7.2}  SHORT-RR  {name:<14} G4 vel100 ×12 @ 0.8s (RR0..11 in cycle order)",
            start
        );
        for i in 0..12 {
            m.note(start + i as f64 * 0.8, 0.35, NOTE, 100);
        }
        t = start + 12.0 * 0.8 + 1.0;
    }

    // ── 4. SHORTS: range consistency across recorded keys ──
    println!("\n# 4. SHORT range sweep (vel100 across recorded keys)");
    for (name, ks) in SHORTS {
        m.cc(t, 58, *ks);
        m.cc(t, 59, 0);
        let start = t + 0.2;
        let keys: String = RECORDED_RANGE
            .iter()
            .map(|(_, n)| *n)
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "{:7.2}  SHORT-RANGE {name:<14} {keys} @ vel100 (0.9s apart)",
            start
        );
        for (i, (key, _)) in RECORDED_RANGE.iter().enumerate() {
            m.note(start + i as f64 * 0.9, 0.4, *key, 100);
        }
        t = start + RECORDED_RANGE.len() as f64 * 0.9 + 1.0;
    }

    // ── 5. SHORTS: pitch-shift probe (off-grid keys, neighbours of G4) ──
    println!("\n# 5. SHORT pitch-shift probe (off-grid keys interpolated from neighbours)");
    m.cc(t, 58, SPICCATO);
    m.cc(t, 59, 0);
    let start = t + 0.2;
    println!(
        "{:7.2}  SHORT-INTERP Spiccato G#4(68),A#4(70),F#4(66),D4(62) vel100",
        start
    );
    for (i, key) in [68u8, 70, 66, 62].iter().enumerate() {
        m.note(start + i as f64 * 0.9, 0.4, *key, 100);
    }
    t = start + 4.0 + 1.0;

    // ── 6. SUSTAIN: CC1 dynamic sweep, non-vib then vib, two octaves ──
    println!("\n# 6. SUSTAIN CC1 0→127 sweep (held 10s) — nonvib then vib, at G3 & G4");
    for (key, label) in [(55u8, "G3"), (67u8, "G4")] {
        for (vib, vlabel) in [(0u8, "nonvib"), (127u8, "vib")] {
            m.cc(t, 58, EXPR_LEGATO);
            m.cc(t, 2, vib);
            m.cc(t, 1, 0);
            let s = t + 0.1;
            m.note(s, 10.0, key, 90);
            m.cc_ramp(s, 10.0, 1, 0, 127);
            println!("{:7.2}  SUS-CC1 {label} {vlabel:<6} held 10s, CC1 0→127", s);
            t += 11.5;
        }
    }

    // ── 7. SUSTAIN: CC2 vibrato sweep at fixed CC1 (incl the ~90 region) ──
    println!("\n# 7. SUSTAIN CC2 0→127 vibrato sweep (held 8s), CC1 fixed");
    for cc1 in [40u8, 90, 120] {
        m.cc(t, 58, EXPR_LEGATO);
        m.cc(t, 1, cc1);
        m.cc(t, 2, 0);
        let s = t + 0.1;
        m.note(s, 8.0, NOTE, 90);
        m.cc_ramp(s, 8.0, 2, 0, 127);
        println!("{:7.2}  SUS-CC2 G4 CC1={cc1} fixed, CC2 0→127 held 8s", s);
        t += 9.5;
    }

    // ── 8. SUSTAIN: attack/release isolation (measure default envelope) ──
    println!("\n# 8. SUSTAIN attack/release isolation: hold 3s, release, 4s silence");
    for cc1 in [30u8, 70, 110] {
        m.cc(t, 58, EXPR_LEGATO);
        m.cc(t, 2, 100);
        m.cc(t, 1, cc1);
        let s = t + 0.1;
        m.note(s, 3.0, NOTE, 90);
        println!(
            "{:7.2}  SUS-ENV G4 CC1={cc1} hold 3s + 4s tail (attack+release)",
            s
        );
        t += 7.5;
    }

    // ── 9. LONGS: Trills / Tremolo / Harmonics CC1 sweeps + envelope ──
    println!("\n# 9. LONGS (Trills/Tremolo/Harmonics): CC1 sweep + release tail");
    for (name, ks) in [
        ("Tremolo", TREMOLO),
        ("Harmonics", HARMONICS),
        ("MeasTrem", MEAS_TREMOLO),
    ] {
        m.cc(t, 58, ks);
        m.cc(t, 2, 100);
        m.cc(t, 1, 0);
        let s = t + 0.1;
        m.note(s, 8.0, NOTE, 90);
        m.cc_ramp(s, 8.0, 1, 0, 127);
        println!(
            "{:7.2}  LONG {name:<10} G4 held 8s, CC1 0→127 (+3s tail)",
            s
        );
        t += 12.0;
    }
    // Trills: two keys held together (halftone then wholetone).
    for (lbl, b) in [("halftone G4+G#4", 68u8), ("wholetone G4+A4", 69u8)] {
        m.cc(t, 58, TRILLS);
        m.cc(t, 1, 90);
        let s = t + 0.1;
        m.raw(s, vec![0x90, NOTE, 90]);
        m.raw(s + 0.005, vec![0x90, b, 90]); // <25ms apart → trill triggers
        m.raw(s + 5.0, vec![0x80, NOTE, 0]);
        m.raw(s + 5.0, vec![0x80, b, 0]);
        println!("{:7.2}  TRILL {lbl} held 5s", s);
        t += 7.5;
    }

    // ── 10. LEGATO: velocity-zone latency (Expressive then Low Latency) ──
    println!("\n# 10. LEGATO latency by velocity zone (G4→A4 pairs, isolated)");
    for (mode, mks) in [("Expressive", EXPR_LEGATO), ("LowLatency", LOW_LAT_LEGATO)] {
        for vel in [20u8, 50, 70, 90, 110, 127] {
            m.cc(t, 58, mks);
            m.cc(t, 2, 90);
            m.cc(t, 1, 90);
            let s = t + 0.1;
            m.legato(s, NOTE, 69, vel, 2.5);
            let zone = match vel {
                0..=64 => "slow~333",
                65..=100 => "med~250",
                _ => "fast~100",
            };
            println!(
                "{:7.2}  LEG-LAT {mode:<10} vel{vel:<3} G4→A4 (Expr zone {zone}ms)",
                s
            );
            t += 4.5;
        }
    }

    // ── 11. LEGATO: interval coverage up & down (medium speed) ──
    println!("\n# 11. LEGATO intervals from G4 (vel85 medium), up then down");
    for dir in [1i8, -1] {
        for semi in [1u8, 2, 3, 4, 5, 7, 12] {
            let b = (NOTE as i8 + dir * semi as i8) as u8;
            m.cc(t, 58, EXPR_LEGATO);
            m.cc(t, 2, 90);
            m.cc(t, 1, 90);
            let s = t + 0.1;
            m.legato(s, NOTE, b, 85, 2.0);
            println!(
                "{:7.2}  LEG-INT {} {semi} semitone(s) G4→{b}",
                s,
                if dir > 0 { "up  " } else { "down" }
            );
            t += 4.0;
        }
    }

    // ── 12. PORTAMENTO: low-velocity legato (≤20 default) + CC5 levels ──
    println!("\n# 12. PORTAMENTO (legato vel ≤20 default), CC5 volume levels, G4→C5");
    for (vel, cc5) in [(10u8, 50u8), (18, 100), (5, 100)] {
        m.cc(t, 58, EXPR_LEGATO);
        m.cc(t, 5, cc5);
        m.cc(t, 1, 90);
        m.cc(t, 2, 90);
        let s = t + 0.1;
        m.legato(s, NOTE, 72, vel, 3.0);
        println!("{:7.2}  PORTA vel{vel} CC5={cc5} G4→C5 (slide)", s);
        t += 5.5;
    }

    // ── 13. RE-BOW: sustain pedal + repeated note (3× RR) ──
    println!("\n# 13. RE-BOW: CC64 pedal down, same note ×4 (re-bow, 3×RR)");
    m.cc(t, 58, EXPR_LEGATO);
    m.cc(t, 1, 90);
    m.cc(t, 2, 90);
    m.cc(t, 64, 127); // pedal down
    let s = t + 0.2;
    for i in 0..4 {
        m.note(s + i as f64 * 1.2, 1.0, NOTE, 90);
    }
    println!("{:7.2}  REBOW G4 ×4 @1.2s, pedal held", s);
    m.cc(s + 5.0, 64, 0); // pedal up
    t += 6.5;

    m.write(&path)?;
    println!("\nwrote {path}  (~{:.0}s of MIDI, ~{:.1} min)", t, t / 60.0);
    Ok(())
}
