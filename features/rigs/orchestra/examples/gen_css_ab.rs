//! Generate a per-note-ALIGNABLE CSS A/B test MIDI + manifest.
//!
//! Every test unit is isolated by a long silence gap so both renders (ours and
//! a real CSS-in-Kontakt export) segment into the SAME ordered list of energy
//! bursts. The harness (`ab_pernote.py`) pairs them by index and compares each
//! note's onset time, level and timbre individually — no fragile global
//! alignment. Covers sustains (dynamics/vib/pitch), every short articulation
//! (velocity + round-robin), longs, and legato (velocity zones + intervals).
//!
//! ```text
//! cargo run --release -p signal-sampler --example gen_css_ab
//! # → css_ab.mid  +  css_ab_manifest.tsv
//! # render ours:  cargo run ... --example render_css_live -- css_ab.mid css_ab_ours.wav
//! # render CSS in Kontakt → css_ab_css.wav, then run ab_pernote.py
//! ```

use std::io::Write;

const DIV: u16 = 480;
const TPS: f64 = 960.0; // ticks/sec @ 120 BPM
const GAP: f64 = 2.0; // silence between units (kills tails → clean segmentation)

// CC58 keyswitch values (mid-band).
const LL_LEG: u8 = 2;
const EX_LEG: u8 = 8;
const SUS: u8 = 2; // sustain (Nonvib) mode; a single note is a fresh attack
                   // short articulations
const SHORTS: &[(&str, u8)] = &[
    ("Spiccato", 13),
    ("Staccatissimo", 18),
    ("Staccato", 23),
    ("Sfz", 28),
    ("Pizzicato", 33),
    ("Bartok", 38),
    ("ColLegno", 43),
    ("Marcato", 68),
];
const LONGS: &[(&str, u8)] = &[("Tremolo", 58), ("Trills", 48), ("Harmonics", 53)];

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
    fn non(&mut self, sec: f64, dur: f64, note: u8, vel: u8) {
        self.raw(sec, vec![0x90, note, vel]);
        self.raw(sec + dur, vec![0x80, note, 0]);
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

struct Gen {
    s: Smf,
    man: Vec<String>,
    t: f64,
    idx: usize,
}
impl Gen {
    fn new() -> Self {
        let mut s = Smf::new();
        // Global setup at t=0: full volume, non-vib, mid dynamic.
        s.cc(0.0, 11, 127);
        s.cc(0.0, 2, 0);
        s.cc(0.0, 1, 64);
        Gen {
            s,
            man: Vec::new(),
            t: 1.0,
            idx: 0,
        }
    }
    fn setcc(&mut self, c: u8, v: u8) {
        self.s.cc(self.t, c, v);
        self.t += 0.03;
    }
    fn row(&mut self, win: f64, pitch: u8, cat: &str, desc: &str) {
        self.man.push(format!(
            "{}\t{:.3}\t{:.3}\t{}\t{}\t{}",
            self.idx, self.t, win, pitch, cat, desc
        ));
        self.idx += 1;
    }
    /// One isolated held/short note. `hold` = note length, `win` = analysis window.
    #[allow(
        clippy::too_many_arguments,
        reason = "one-off CSS-generation script; one positional arg per MIDI/articulation knob reads clearer than a params struct here"
    )]
    fn shot(
        &mut self,
        cat: &str,
        desc: &str,
        ks: u8,
        pitch: u8,
        vel: u8,
        cc1: u8,
        cc2: u8,
        hold: f64,
        win: f64,
    ) {
        self.setcc(58, ks);
        self.setcc(1, cc1);
        self.setcc(2, cc2);
        self.setcc(59, 0); // RR reset for reproducibility (shorts)
        self.row(win, pitch, cat, desc);
        self.s.non(self.t, hold, pitch, vel);
        self.t += win + GAP;
    }
    /// One isolated legato phrase A→B (single segment). `win` covers the phrase.
    #[allow(
        clippy::too_many_arguments,
        reason = "one-off CSS-generation script; one positional arg per MIDI/articulation knob reads clearer than a params struct here"
    )]
    fn leg(&mut self, cat: &str, desc: &str, ks: u8, a: u8, b: u8, vel: u8, hold: f64) {
        self.setcc(58, ks);
        self.setcc(2, 0);
        self.setcc(1, 90);
        let win = 0.6 + hold + 0.5;
        self.row(win, b, cat, desc);
        // A, overlap B (transition), release A, hold B
        self.s.non(self.t, 0.6, a, vel); // will be released by A-off below via non? no—use raw pair
                                         // redo precisely: A on, B on (overlap), A off, B off
                                         // (non() already emitted A on+off at 0.6; emit B overlapping)
        self.s.non(self.t + 0.5, hold, b, vel);
        self.t += win + GAP;
    }
}

fn main() -> std::io::Result<()> {
    let mut g = Gen::new();

    // 0. CALIB — loud reference sustain (segment 0, level anchor).
    g.shot(
        "CALIB",
        "sustain G4 ff nonvib",
        SUS,
        67,
        100,
        110,
        0,
        4.0,
        6.0,
    );

    // 1. SUSTAIN dynamics: G4 nonvib then vib, CC1 = 20/50/80/110.
    for &cc1 in &[20u8, 50, 80, 110] {
        g.shot(
            "SUS-DYN",
            &format!("G4 nonvib CC1={cc1}"),
            SUS,
            67,
            100,
            cc1,
            0,
            3.0,
            4.0,
        );
    }
    for &cc1 in &[20u8, 50, 80, 110] {
        g.shot(
            "SUS-DYN",
            &format!("G4 vib CC1={cc1}"),
            SUS,
            67,
            100,
            cc1,
            127,
            3.0,
            4.0,
        );
    }
    // 2. SUSTAIN pitch/range: C4, G4, C5, G5 at CC1=90 nonvib.
    for &(p, n) in &[(60u8, "C4"), (67, "G4"), (72, "C5"), (79, "G5")] {
        g.shot(
            "SUS-PITCH",
            &format!("{n} nonvib CC1=90"),
            SUS,
            p,
            100,
            90,
            0,
            3.0,
            4.0,
        );
    }

    // 3. SHORTS: each articulation × vel 40/80/120 (RR reset each → RR0).
    for &(name, ks) in SHORTS {
        for &vel in &[40u8, 80, 120] {
            g.shot(
                "SHORT",
                &format!("{name} vel{vel}"),
                ks,
                67,
                vel,
                90,
                0,
                0.4,
                1.3,
            );
        }
    }
    // 4. SHORT round-robin: spiccato G4 vel100 ×8 after one reset (RR0..7 in order).
    g.setcc(58, 13);
    g.setcc(59, 0);
    for r in 0..8 {
        g.setcc(1, 90);
        g.row(1.0, 67, "SHORT-RR", &format!("Spiccato G4 vel100 RR#{r}"));
        g.s.non(g.t, 0.4, 67, 100);
        g.t += 1.0 + GAP;
    }

    // 5. LONGS: tremolo, trills, harmonics (held).
    for &(name, ks) in LONGS {
        g.shot(
            "LONG",
            &format!("{name} G4 CC1=90"),
            ks,
            67,
            100,
            90,
            0,
            3.0,
            4.0,
        );
    }

    // 6. LEGATO velocity zones: G4→A4, vel 20/50/80/110, LL then EX.
    for &(mode, ks) in &[("LL", LL_LEG), ("EX", EX_LEG)] {
        for &vel in &[20u8, 50, 80, 110] {
            g.leg(
                "LEG-VEL",
                &format!("{mode} G4->A4 vel{vel}"),
                ks,
                67,
                69,
                vel,
                2.0,
            );
        }
    }
    // 7. LEGATO intervals from G4 (vel85, LL), up 1/2/3/5/7/12, down 5.
    for &iv in &[1i8, 2, 3, 5, 7, 12, -5] {
        let b = (67 + iv as i16) as u8;
        g.leg(
            "LEG-INT",
            &format!("G4->{} ({:+}st)", b, iv),
            LL_LEG,
            67,
            b,
            85,
            2.0,
        );
    }

    let dur = g.t;
    g.s.write("css_ab.mid")?;
    let mut mf = std::fs::File::create("css_ab_manifest.tsv")?;
    writeln!(mf, "idx\tt_start\twin\tpitch\tcategory\tdescription")?;
    for r in &g.man {
        writeln!(mf, "{r}")?;
    }
    println!(
        "wrote css_ab.mid ({:.1}s, {} isolated units) + css_ab_manifest.tsv",
        dur, g.idx
    );
    Ok(())
}
