//! Generate a Cinematic Studio Strings **test MIDI file** that exercises every
//! data point we need to match the real instrument: each articulation, velocity
//! ranges, CC1 dynamics, CC2 vibrato, CC5 portamento, legato transitions (up /
//! down, slow / fast), and combinations.
//!
//! Uses CSS's default controller map (so it drives the REAL instrument):
//!   CC58 keyswitch · CC1 dynamics (velocity x-fade) · CC2 vibrato · CC11 volume
//!   · CC5 portamento volume · portamento on legato velocity ≤ 10.
//!
//! ```text
//! cargo run -p signal-sampler --example gen_css_test_midi -- css_test.mid
//! ```
//! Load CSS 1st Violins, route this single track to it, render the audio, and
//! send it back — the printed manifest lists what happens at each timestamp so
//! the audio can be segmented and compared against our engine.

use std::io::Write;

const DIV: u16 = 480; // ticks per quarter note
const TPS: f64 = 960.0; // ticks/sec at 120 BPM (0.5 s per quarter)

/// Short (one-shot, velocity-driven) articulations — tested across velocities.
const SHORTS: &[(&str, u8)] = &[
    ("Spiccato", 13),
    ("Staccatissimo", 18),
    ("Staccato", 23),
    ("Sfz", 28),
    ("Pizzicato", 33),
    ("Bartok snap", 38),
    ("Col Legno", 43),
    ("Marcato (no overlay)", 68),
];

/// Held (CC1-dynamic) articulations — tested across CC1.
const LONGS: &[(&str, u8)] = &[
    ("Expressive Legato", 8),
    ("Trills", 48),
    ("Harmonics", 53),
    ("Tremolo", 58),
];

struct Smf {
    ev: Vec<(u32, u8, Vec<u8>)>, // (abs_tick, order, bytes)
    seq: u8,
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
        self.seq = self.seq.wrapping_add(1);
    }
    fn cc(&mut self, sec: f64, ctrl: u8, val: u8) {
        self.raw(sec, vec![0xB0, ctrl, val]);
    }
    fn note(&mut self, sec: f64, dur: f64, note: u8, vel: u8) {
        self.raw(sec, vec![0x90, note, vel]);
        self.raw(sec + dur, vec![0x80, note, 0]);
    }
    /// Smoothly ramp a CC from `a` to `b` over `[t0, t0+dur]`.
    fn cc_ramp(&mut self, t0: f64, dur: f64, ctrl: u8, a: u8, b: u8) {
        let steps = 64;
        for i in 0..=steps {
            let f = i as f64 / steps as f64;
            let v = (a as f64 + (b as f64 - a as f64) * f).round() as u8;
            self.cc(t0 + dur * f, ctrl, v);
        }
    }
    fn write(mut self, path: &str) -> std::io::Result<()> {
        self.ev.sort_by_key(|(t, s, _)| (*t, *s));
        let mut track: Vec<u8> = Vec::new();
        // Tempo: 120 BPM (500000 µs/quarter).
        write_vlq(&mut track, 0);
        track.extend_from_slice(&[0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20]);
        let mut prev = 0u32;
        for (t, _, bytes) in &self.ev {
            write_vlq(&mut track, t - prev);
            track.extend_from_slice(bytes);
            prev = *t;
        }
        write_vlq(&mut track, 0);
        track.extend_from_slice(&[0xFF, 0x2F, 0x00]); // end of track

        let mut f = std::fs::File::create(path)?;
        f.write_all(b"MThd")?;
        f.write_all(&6u32.to_be_bytes())?;
        f.write_all(&0u16.to_be_bytes())?; // format 0
        f.write_all(&1u16.to_be_bytes())?; // 1 track
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
        .unwrap_or_else(|| "css_test.mid".into());
    let mut m = Smf::new();
    let mut t = 0.5; // start time (s)

    // Baseline controllers.
    m.cc(0.0, 11, 127); // full volume
    m.cc(0.0, 2, 0); // vibrato off
    m.cc(0.0, 1, 64); // mid dynamic
    m.cc(0.0, 58, 8); // expressive legato

    println!("# CSS test MIDI — section manifest (seconds)");
    println!("# 120 BPM, channel 1. Notes mostly G4(67) recorded key; some C4(60) even key.\n");

    // ── 1. Long articulations: CC1 dynamic sweep, non-vib then vibrato ──
    for (name, ks) in LONGS {
        m.cc(t, 58, *ks);
        // non-vibrato (CC2=0)
        m.cc(t, 2, 0);
        m.cc(t, 1, 0);
        let start = t + 0.1;
        m.note(start, 8.0, 67, 90);
        m.cc_ramp(start, 8.0, 1, 0, 127); // CC1 0→127 over the held note
        println!(
            "{:6.1}  LONG {name:<22} note67 held 8s, CC1 0→127 sweep, CC2=0 (nonvib)",
            start
        );
        t += 9.0;
        // vibrato (CC2=full)
        m.cc(t, 58, *ks);
        m.cc(t, 2, 127);
        m.cc(t, 1, 0);
        let start = t + 0.1;
        m.note(start, 8.0, 67, 90);
        m.cc_ramp(start, 8.0, 1, 0, 127);
        println!(
            "{:6.1}  LONG {name:<22} note67 held 8s, CC1 0→127 sweep, CC2=127 (vib)",
            start
        );
        t += 9.5;
    }

    // ── 2. CC2 vibrato sweep at a fixed dynamic (incl the CC1≈90 region) ──
    for cc1 in [40u8, 90, 120] {
        m.cc(t, 58, 8);
        m.cc(t, 1, cc1);
        m.cc(t, 2, 0);
        let start = t + 0.1;
        m.note(start, 6.0, 67, 90);
        m.cc_ramp(start, 6.0, 2, 0, 127); // CC2 0→127
        println!(
            "{:6.1}  VIB sweep  note67 held 6s, CC1={cc1} fixed, CC2 0→127",
            start
        );
        t += 7.0;
    }

    // ── 3. Short articulations across velocity (= dynamic) ──
    for (name, ks) in SHORTS {
        m.cc(t, 58, *ks);
        println!(
            "{:6.1}  SHORT {name:<22} note67 @ vel 20/50/80/110/127",
            t + 0.1
        );
        for (i, vel) in [20u8, 50, 80, 110, 127].iter().enumerate() {
            m.note(t + 0.1 + i as f64 * 0.6, 0.4, 67, *vel);
        }
        t += 3.6;
    }

    // ── 4. Legato transitions: up/down at fast & slow velocities ──
    m.cc(t, 58, 8); // expressive legato
    m.cc(t, 2, 90); // some vibrato
    m.cc(t, 1, 80);
    for (label, vel) in [("fast", 110u8), ("slow", 40u8)] {
        // Overlapping legato line up then down: 60-62-64-65-67-65-64-62-60.
        let line = [60u8, 62, 64, 65, 67, 65, 64, 62, 60];
        println!(
            "{:6.1}  LEGATO {label} line 60..67..60 (overlapping), vel {vel}",
            t + 0.1
        );
        let step = 0.6;
        for (i, &n) in line.iter().enumerate() {
            let on = t + 0.1 + i as f64 * step;
            // overlap by holding ~0.15s into the next note
            m.note(on, step + 0.15, n, vel);
        }
        t += 0.1 + line.len() as f64 * step + 1.0;
    }

    // ── 5. Portamento: low-velocity legato (≤10) + CC5 ──
    m.cc(t, 58, 8);
    m.cc(t, 5, 100); // portamento volume
    m.cc(t, 1, 90);
    m.cc(t, 2, 90);
    println!(
        "{:6.1}  PORTAMENTO 60→72 legato vel 5 (≤10), CC5=100",
        t + 0.1
    );
    m.note(t + 0.1, 1.5, 60, 5);
    m.note(t + 1.2, 2.5, 72, 5); // overlaps → portamento glide up
    t += 5.0;

    // ── 6. Combination: held note with simultaneous CC1 + CC2 moves ──
    m.cc(t, 58, 8);
    m.cc(t, 1, 10);
    m.cc(t, 2, 0);
    let start = t + 0.1;
    m.note(start, 8.0, 67, 90);
    m.cc_ramp(start, 4.0, 1, 10, 120); // swell up
    m.cc_ramp(start, 8.0, 2, 0, 127); // vibrato in over the whole note
    m.cc_ramp(start + 4.0, 4.0, 1, 120, 30); // swell back down
    println!(
        "{:6.1}  COMBO note67 8s: CC1 swell up+down, CC2 0→127 simultaneously",
        start
    );
    t += 9.0;

    m.write(&path)?;
    println!("\nwrote {path}  (~{:.0}s of MIDI)", t);
    Ok(())
}
