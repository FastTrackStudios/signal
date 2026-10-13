//! Any-to-any drum-map converter — a native Rust port of the FTS Drum
//! Converter JSFX (`apps/extensions/reaper-fts-extensions/jsfx/fts/`).
//!
//! Maps an incoming drum-map's notes → a 45-slot internal articulation
//! vocabulary → an outgoing drum-map's notes, so one physical kit (e.g. an
//! Alesis Strata Prime e-kit) drives a differently-mapped sample library (e.g.
//! GGD Modern & Massive 2). Where the target map lacks an articulation it
//! degrades sensibly down a fallback chain (edge→tip, T2→T1, choke→hit, …).
//!
//! Hi-hats: CC-based maps (Strata) send bow/edge notes plus openness on CC4;
//! the last CC4 value picks the discrete articulation (Tight / Closed /
//! Open 1-3). A CC-based *target* re-emits a representative CC4 before its
//! bow/edge note; a note-based target (FTS, GGD, MM2) emits the discrete note.
//!
//! Chokes assume the Strata "Cymbal Choke = Note" setting (note-mode), so
//! choke articulations arrive as ordinary notes. Aftertouch and every other
//! message pass through untouched.
//!
//! Differs from the JSFX in two deliberate ways: (1) it has no gmem "echo
//! guard" — that solved a REAPER round-trip double-trigger that does not exist
//! on an in-process MIDI path; (2) it adds an [`DrumMap::Mm2`] target whose
//! note layout matches the shipped Modern & Massive 2 `.signalpreset`
//! note-routing (GM-standard: snare 38, hats 42/46), which is NOT the GGD v1
//! "Halpern" layout the JSFX `GGD` map encodes.

use crate::event::MidiEvent;
use crate::number::{Channel, ControllerNumber, ControllerValue, KeyNumber, Velocity};

/// CC4 carries hi-hat openness in CC-based maps (Roland-style: higher = more
/// closed).
const CC_HAT: u8 = 4;

// ── internal articulation ids (index into the per-map tables) ──────────────
const KICK: usize = 0;
const KICK_L: usize = 1;
const SNARE: usize = 2;
const SNARE_R: usize = 3;
const S_XSTICK: usize = 4;
const S_BUZZ: usize = 5;
const S_FLAM: usize = 6;
const S_RIMSHOT: usize = 7;
const T1: usize = 8;
const T2: usize = 9;
const T3: usize = 10;
const T4: usize = 11;
const T1_RIM: usize = 12;
const T2_RIM: usize = 13;
const T3_RIM: usize = 14;
const T4_RIM: usize = 15;
const STICKS: usize = 16;
const H_TIGHT_TIP: usize = 17;
const H_TIGHT_EDG: usize = 18;
const H_CLSD_TIP: usize = 19;
const H_CLSD_EDG: usize = 20;
const H_OPEN1: usize = 21;
const H_OPEN2: usize = 22;
const H_OPEN3: usize = 23;
const H_CHICK: usize = 24;
const H_CHING: usize = 25;
const H_BELL: usize = 26;
const CR_L: usize = 27;
const CR_L_CHK: usize = 28;
const CR_C: usize = 29;
const CR_C_CHK: usize = 30;
const CR_R: usize = 31;
const CR_R_CHK: usize = 32;
const R_BOW: usize = 33;
const R_BOW_HI: usize = 34;
const R_BELL: usize = 35;
const R_CRASH: usize = 36;
const R_CHK: usize = 37;
const CHINA: usize = 38;
const CHINA_CHK: usize = 39;
const SPLASH: usize = 40;
const SPLASH_CHK: usize = 41;
const STACK: usize = 42;
const MS_L: usize = 43;
const MS_R: usize = 44;
const NART: usize = 45;

/// A supported drum map (kit note layout).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrumMap {
    /// Alesis Strata Prime e-kit (PRIME Drum Module User Guide v1.2.1, §6.2).
    StrataPrime,
    /// The FTS internal drum map.
    Fts,
    /// GGD Modern & Massive v1 ("Halpern") layout.
    Ggd,
    /// GGD Modern & Massive 2 — matches the shipped `.signalpreset` note
    /// routing (GM-standard), the target for playing MM2 kits.
    Mm2,
}

/// Hi-hat openness thresholds on CC4 (higher CC4 = more closed). A hat sounds
/// Tight at `>= tight`, Closed at `>= closed`, Open 1/2/3 below that.
#[derive(Clone, Copy, Debug)]
pub struct HatThresholds {
    pub tight: u8,
    pub closed: u8,
    pub open1: u8,
    pub open2: u8,
}

impl Default for HatThresholds {
    fn default() -> Self {
        Self {
            tight: 110,
            closed: 80,
            open1: 55,
            open2: 25,
        }
    }
}

/// Resolved note tables for one drum map.
#[derive(Clone)]
struct MapTables {
    /// articulation id → output note (-1 = the map lacks this articulation).
    a2n: [i16; NART],
    /// input note → articulation id (-1 = unmapped).
    n2a: [i16; 128],
    /// true when hi-hats are addressed via CC4 openness + a bow/edge note.
    hat_cc: bool,
    /// CC-based hat bow note (-1 = none).
    hh_bow: i16,
    /// CC-based hat edge note (-1 = none).
    hh_edge: i16,
}

/// Fallback chain used when the *target* map lacks an articulation. Shared
/// across maps: only consulted when `a2n[art] < 0`, so entries never override
/// a note a map does define.
fn fallback_table() -> [i16; NART] {
    let mut fb = [-1i16; NART];
    let mut set = |a: usize, to: usize| fb[a] = to as i16;
    set(KICK_L, KICK);
    set(SNARE_R, SNARE);
    set(S_BUZZ, SNARE);
    set(S_FLAM, SNARE);
    set(S_RIMSHOT, SNARE);
    set(S_XSTICK, SNARE);
    set(T2, T1);
    set(T4, T3);
    set(T3, T1);
    set(T1_RIM, T1);
    set(T2_RIM, T2);
    set(T3_RIM, T3);
    set(T4_RIM, T4);
    set(H_TIGHT_EDG, H_TIGHT_TIP);
    set(H_CLSD_EDG, H_CLSD_TIP);
    set(H_TIGHT_TIP, H_CLSD_TIP);
    set(H_CHING, H_CHICK);
    set(CR_C, CR_L);
    set(CR_C_CHK, CR_L_CHK);
    set(R_BOW_HI, R_BOW);
    set(STACK, CHINA);
    set(MS_L, SPLASH);
    set(MS_R, SPLASH);
    set(SPLASH, CR_R);
    set(SPLASH_CHK, CR_R_CHK);
    // Extra fallbacks beyond the JSFX so coarse targets (MM2) degrade cleanly.
    // Only fire when the target lacks the note, so they're inert for maps that
    // define these explicitly (Strata/FTS/GGD).
    set(H_OPEN2, H_OPEN1);
    set(H_OPEN3, H_OPEN1);
    set(H_BELL, H_CHING);
    set(STICKS, SNARE);
    set(R_CRASH, R_BOW);
    set(R_CHK, R_BOW);
    set(CR_L_CHK, CR_L);
    set(CR_R_CHK, CR_R);
    set(CHINA_CHK, CHINA);
    fb
}

/// Derive `n2a` from `a2n` (each mapped note points back at its articulation).
fn derive_n2a(a2n: &[i16; NART]) -> [i16; 128] {
    let mut n2a = [-1i16; 128];
    for (art, &note) in a2n.iter().enumerate() {
        if note >= 0 && (note as usize) < 128 {
            n2a[note as usize] = art as i16;
        }
    }
    n2a
}

fn tables(map: DrumMap) -> MapTables {
    let mut a2n = [-1i16; NART];
    let mut set = |a: usize, n: i16| a2n[a] = n;
    let (hat_cc, hh_bow, hh_edge);

    match map {
        DrumMap::StrataPrime => {
            set(KICK, 24);
            set(SNARE, 26);
            set(S_RIMSHOT, 29);
            set(S_XSTICK, 25);
            set(T1, 38);
            set(T2, 35);
            set(T3, 31);
            set(T4, 33);
            set(T1_RIM, 93);
            set(T2_RIM, 91);
            set(T3_RIM, 89);
            set(T4_RIM, 90);
            set(STICKS, 40);
            set(H_BELL, 22);
            set(H_CHICK, 32);
            set(H_CHING, 56);
            set(CR_L, 41);
            set(CR_L_CHK, 75);
            set(CR_C, 45);
            set(CR_C_CHK, 80);
            set(CR_R, 43);
            set(CR_R_CHK, 78);
            set(R_BOW, 60);
            set(R_BELL, 71);
            set(R_CRASH, 50);
            set(R_CHK, 87);
            set(CHINA, 47);
            set(CHINA_CHK, 82);
            hat_cc = true;
            hh_bow = 18;
            hh_edge = 20;
        }
        DrumMap::Fts => {
            set(KICK, 24);
            set(KICK_L, 23);
            set(SNARE, 26);
            set(SNARE_R, 28);
            set(S_XSTICK, 25);
            set(S_BUZZ, 27);
            set(T1, 29);
            set(T2, 31);
            set(T3, 33);
            set(T4, 35);
            set(T1_RIM, 77);
            set(T2_RIM, 79);
            set(T3_RIM, 81);
            set(T4_RIM, 83);
            set(STICKS, 21);
            set(H_TIGHT_TIP, 37);
            set(H_TIGHT_EDG, 38);
            set(H_CLSD_TIP, 39);
            set(H_CLSD_EDG, 40);
            set(H_OPEN1, 41);
            set(H_OPEN2, 42);
            set(H_OPEN3, 43);
            set(H_CHICK, 44);
            set(H_CHING, 45);
            set(H_BELL, 54);
            set(CR_L, 48);
            set(CR_L_CHK, 49);
            set(CR_C, 50);
            set(CR_C_CHK, 51);
            set(CR_R, 52);
            set(CR_R_CHK, 53);
            set(R_BOW, 56);
            set(R_BOW_HI, 109);
            set(R_BELL, 57);
            set(R_CRASH, 58);
            set(R_CHK, 59);
            set(CHINA, 62);
            set(CHINA_CHK, 63);
            set(SPLASH, 64);
            set(SPLASH_CHK, 65);
            set(STACK, 71);
            set(MS_L, 73);
            set(MS_R, 75);
            hat_cc = false;
            hh_bow = 46; // H-Tip CC / H-Edge CC accepted as CC-based hat input
            hh_edge = 47;
        }
        DrumMap::Ggd => {
            set(KICK, 36);
            set(SNARE, 37);
            set(S_FLAM, 38);
            set(S_BUZZ, 39);
            set(S_XSTICK, 40);
            set(T1, 42);
            set(T3, 45);
            set(T4, 46);
            set(H_TIGHT_TIP, 48);
            set(H_CLSD_TIP, 49);
            set(H_OPEN1, 52);
            set(H_OPEN2, 53);
            set(H_OPEN3, 54);
            set(H_CHICK, 55);
            set(H_CHING, 57);
            set(CR_L, 58);
            set(CR_L_CHK, 59);
            set(CR_R, 61);
            set(CR_R_CHK, 62);
            set(R_BOW, 73);
            set(R_BELL, 74);
            set(R_CRASH, 76);
            set(CHINA, 71);
            set(CHINA_CHK, 72);
            set(SPLASH, 77);
            set(SPLASH_CHK, 83);
            hat_cc = false;
            hh_bow = -1;
            hh_edge = -1;
        }
        DrumMap::Mm2 => {
            // Matches the shipped MM2 `.signalpreset` note_routing (GM-standard).
            // Toms are the four routed slots (rack→floor, high→low); hi-hats
            // collapse to closed(42)/open(46)/pedal(44). Articulations MM2 has
            // no dedicated note for (rimshot, sidestick, cymbal chokes, ride
            // crash) resolve through the fallback chain. Verify tom/ride/choke
            // assignments against the target preset when playing live.
            set(KICK, 36);
            set(SNARE, 38);
            set(T1, 47);
            set(T2, 45);
            set(T3, 43);
            set(T4, 41);
            set(H_CLSD_TIP, 42);
            set(H_OPEN1, 46);
            set(H_OPEN2, 46);
            set(H_OPEN3, 46);
            set(H_CHICK, 44);
            set(R_BOW, 51);
            set(R_BELL, 53);
            set(CR_L, 49);
            set(CR_R, 57);
            set(CHINA, 52);
            set(SPLASH, 55);
            hat_cc = false;
            hh_bow = -1;
            hh_edge = -1;
        }
    }

    let mut n2a = derive_n2a(&a2n);
    // Input-only aliases (notes that map to an articulation without being its
    // primary output note).
    let mut alias = |n: usize, a: usize| n2a[n] = a as i16;
    match map {
        DrumMap::StrataPrime => {
            alias(88, S_XSTICK);
            alias(70, T1);
            alias(66, T2);
            alias(61, T3);
            alias(63, T4);
            alias(73, STICKS);
            alias(94, STICKS);
            alias(52, CR_L);
            alias(62, CR_L);
            alias(55, CR_C);
            alias(65, CR_C);
            alias(53, CR_R);
            alias(64, CR_R);
            alias(57, CHINA);
            alias(67, CHINA);
        }
        DrumMap::Fts => {
            alias(30, T1);
            alias(32, T2);
            alias(34, T3);
            alias(36, T4);
            alias(78, T1_RIM);
            alias(80, T2_RIM);
            alias(82, T3_RIM);
            alias(84, T4_RIM);
        }
        DrumMap::Ggd => {
            alias(43, T3);
            alias(41, SNARE);
        }
        DrumMap::Mm2 => {
            alias(40, SNARE); // MM2 routes 40 → snare too
        }
    }

    MapTables {
        a2n,
        n2a,
        hat_cc,
        hh_bow,
        hh_edge,
    }
}

fn is_hat_zone(art: usize) -> bool {
    (H_TIGHT_TIP..=H_OPEN3).contains(&art)
}

/// Any-to-any drum-map converter. Stateful (tracks the last CC4 openness and
/// note-on translations so note-offs match); drive one per input stream.
///
/// Note-on translation is tracked per input note (0..128), so drive one
/// converter per physical kit / MIDI channel.
#[derive(Clone)]
pub struct DrumMapConverter {
    from: MapTables,
    to: MapTables,
    thresholds: HatThresholds,
    /// Pass a note the maps don't cover straight through (vs. dropping it).
    pub passthrough_unmapped: bool,
    hh_cc: u8,
    last_sent_cc: i16,
    fb: [i16; NART],
    /// input note → translated output note, so note-off replays the same note.
    active: [i16; 128],
}

impl DrumMapConverter {
    /// A converter from `from`'s note layout to `to`'s, with default hi-hat
    /// thresholds and unmapped-note pass-through enabled.
    pub fn new(from: DrumMap, to: DrumMap) -> Self {
        Self {
            from: tables(from),
            to: tables(to),
            thresholds: HatThresholds::default(),
            passthrough_unmapped: true,
            hh_cc: 127, // assume closed until the first CC4 arrives
            last_sent_cc: -1,
            fb: fallback_table(),
            active: [-1i16; 128],
        }
    }

    /// Override the hi-hat openness thresholds.
    pub fn with_thresholds(mut self, thresholds: HatThresholds) -> Self {
        self.thresholds = thresholds;
        self
    }

    /// The discrete hat articulation the current CC4 openness selects.
    fn hat_artic(&self, edge: bool) -> usize {
        let t = &self.thresholds;
        let c = self.hh_cc;
        if c >= t.tight {
            if edge {
                H_TIGHT_EDG
            } else {
                H_TIGHT_TIP
            }
        } else if c >= t.closed {
            if edge {
                H_CLSD_EDG
            } else {
                H_CLSD_TIP
            }
        } else if c >= t.open1 {
            H_OPEN1
        } else if c >= t.open2 {
            H_OPEN2
        } else {
            H_OPEN3
        }
    }

    /// Representative CC4 value for an openness zone (midpoints between
    /// thresholds, so a round trip lands in the same zone).
    fn hat_cc_value(&self, art: usize) -> u8 {
        let t = &self.thresholds;
        match art {
            H_TIGHT_TIP | H_TIGHT_EDG => 127,
            H_CLSD_TIP | H_CLSD_EDG => (t.tight as u16 + t.closed as u16) as u8 / 2,
            H_OPEN1 => (t.closed as u16 + t.open1 as u16) as u8 / 2,
            H_OPEN2 => (t.open1 as u16 + t.open2 as u16) as u8 / 2,
            _ => 0, // Open 3
        }
    }

    /// Articulation → output note, walking the fallback chain (bounded).
    fn out_note(&self, mut art: i16) -> i16 {
        for _ in 0..6 {
            if art < 0 {
                return -1;
            }
            let nn = self.to.a2n[art as usize];
            if nn >= 0 {
                return nn;
            }
            art = self.fb[art as usize];
        }
        -1
    }

    /// Convert one MIDI event into zero or more output events. Non-drum and
    /// unmapped messages pass through unchanged.
    pub fn convert(&mut self, ev: MidiEvent) -> Vec<MidiEvent> {
        match ev {
            MidiEvent::ControlChange {
                channel,
                controller,
                value,
            } if controller.get() == CC_HAT => {
                self.hh_cc = value.get();
                vec![MidiEvent::ControlChange {
                    channel,
                    controller,
                    value,
                }]
            }
            MidiEvent::NoteOn {
                channel,
                key,
                velocity,
            } if velocity.get() > 0 => self.convert_note_on(channel, key.get(), velocity.get()),
            // Note-off, or note-on with velocity 0 (running-status note-off).
            MidiEvent::NoteOff {
                channel,
                key,
                velocity,
            } => self.convert_note_off(channel, key.get(), velocity.get()),
            MidiEvent::NoteOn {
                channel,
                key,
                velocity,
            } => {
                // velocity == 0
                self.convert_note_off(channel, key.get(), velocity.get())
            }
            other => vec![other],
        }
    }

    fn convert_note_on(&mut self, channel: Channel, note: u8, vel: u8) -> Vec<MidiEvent> {
        let n = note as usize;
        // Resolve input note → articulation.
        let art: i16 = if self.from.hh_bow >= 0 && note as i16 == self.from.hh_bow {
            self.hat_artic(false) as i16
        } else if self.from.hh_edge >= 0 && note as i16 == self.from.hh_edge {
            self.hat_artic(true) as i16
        } else {
            self.from.n2a[n]
        };

        let mut out_events = Vec::new();
        let mut out: i16 = -1;

        if art >= 0 {
            let a = art as usize;
            if self.to.hat_cc && is_hat_zone(a) {
                // CC-based hat target: emit openness CC first (if changed),
                // then the bow/edge note.
                let cc_val = self.hat_cc_value(a) as i16;
                if cc_val != self.last_sent_cc {
                    out_events.push(MidiEvent::ControlChange {
                        channel,
                        controller: ControllerNumber::new(CC_HAT),
                        value: ControllerValue::new(cc_val as u8),
                    });
                    self.last_sent_cc = cc_val;
                }
                out = if a == H_TIGHT_EDG || a == H_CLSD_EDG {
                    self.to.hh_edge
                } else {
                    self.to.hh_bow
                };
            } else {
                out = self.out_note(art);
            }
        }

        if out < 0 && self.passthrough_unmapped {
            out = note as i16; // pass through unmapped
        }

        if out >= 0 {
            self.active[n] = out;
            out_events.push(MidiEvent::NoteOn {
                channel,
                key: KeyNumber::new(out as u8),
                velocity: Velocity::new(vel),
            });
        }
        out_events
    }

    fn convert_note_off(&mut self, channel: Channel, note: u8, vel: u8) -> Vec<MidiEvent> {
        let n = note as usize;
        let out = self.active[n];
        if out >= 0 {
            self.active[n] = -1;
            vec![MidiEvent::NoteOff {
                channel,
                key: KeyNumber::new(out as u8),
                velocity: Velocity::new(vel),
            }]
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch() -> Channel {
        Channel::new(9)
    }
    fn non(note: u8, vel: u8) -> MidiEvent {
        MidiEvent::NoteOn {
            channel: ch(),
            key: KeyNumber::new(note),
            velocity: Velocity::new(vel),
        }
    }
    fn cc4(v: u8) -> MidiEvent {
        MidiEvent::ControlChange {
            channel: ch(),
            controller: ControllerNumber::new(CC_HAT),
            value: ControllerValue::new(v),
        }
    }
    fn note_of(ev: &MidiEvent) -> Option<u8> {
        match ev {
            MidiEvent::NoteOn { key, .. } => Some(key.get()),
            _ => None,
        }
    }

    #[test]
    fn strata_kick_to_mm2() {
        let mut c = DrumMapConverter::new(DrumMap::StrataPrime, DrumMap::Mm2);
        let out = c.convert(non(24, 110)); // Strata kick = 24
        assert_eq!(out.len(), 1);
        assert_eq!(note_of(&out[0]), Some(36)); // MM2 kick = 36
    }

    #[test]
    fn strata_snare_to_mm2() {
        let mut c = DrumMapConverter::new(DrumMap::StrataPrime, DrumMap::Mm2);
        assert_eq!(note_of(&c.convert(non(26, 100))[0]), Some(38)); // snare 26 → 38
    }

    #[test]
    fn note_off_replays_translated_note() {
        let mut c = DrumMapConverter::new(DrumMap::StrataPrime, DrumMap::Mm2);
        let _ = c.convert(non(24, 110));
        let off = c.convert(MidiEvent::NoteOff {
            channel: ch(),
            key: KeyNumber::new(24),
            velocity: Velocity::new(0),
        });
        assert_eq!(off.len(), 1);
        match &off[0] {
            MidiEvent::NoteOff { key, .. } => assert_eq!(key.get(), 36),
            e => panic!("expected NoteOff, got {e:?}"),
        }
    }

    #[test]
    fn hat_cc_selects_openness_into_mm2_notes() {
        let mut c = DrumMapConverter::new(DrumMap::StrataPrime, DrumMap::Mm2);
        // Strata hat bow note = 18. Closed → MM2 42, Open → MM2 46.
        c.convert(cc4(120)); // tight/closed zone
        assert_eq!(note_of(c.convert(non(18, 100)).last().unwrap()), Some(42));
        c.convert(cc4(10)); // open 3 zone
        assert_eq!(note_of(c.convert(non(18, 100)).last().unwrap()), Some(46));
    }

    #[test]
    fn strata_to_fts_hat_roundtrips_openness() {
        // CC-based → CC-based: FTS is note-based, so Strata→FTS emits discrete
        // FTS hat notes; a mid-open hat should pick an FTS open note.
        let mut c = DrumMapConverter::new(DrumMap::StrataPrime, DrumMap::Fts);
        c.convert(cc4(40)); // between open2(25) and open1(55) → Open 2
        let out = c.convert(non(18, 100));
        // FTS H_OPEN2 = 42
        assert_eq!(note_of(out.last().unwrap()), Some(42));
    }

    #[test]
    fn fallback_center_crash_to_left() {
        // GGD has no center crash; CR_C falls back to CR_L. Drive an FTS center
        // crash (note 50) → GGD, expect CR_L note 58.
        let mut c = DrumMapConverter::new(DrumMap::Fts, DrumMap::Ggd);
        assert_eq!(note_of(&c.convert(non(50, 100))[0]), Some(58));
    }

    #[test]
    fn unmapped_passthrough_toggle() {
        let mut c = DrumMapConverter::new(DrumMap::StrataPrime, DrumMap::Mm2);
        c.passthrough_unmapped = false;
        // Note 5 is unmapped in Strata.
        assert!(c.convert(non(5, 100)).is_empty());
        c.passthrough_unmapped = true;
        assert_eq!(note_of(&c.convert(non(5, 100))[0]), Some(5));
    }

    #[test]
    fn tom_fallback_chain_into_mm2() {
        // All four Strata toms map to the four MM2 tom slots.
        let mut c = DrumMapConverter::new(DrumMap::StrataPrime, DrumMap::Mm2);
        assert_eq!(note_of(&c.convert(non(38, 100))[0]), Some(47)); // T1 → 47
        assert_eq!(note_of(&c.convert(non(35, 100))[0]), Some(45)); // T2 → 45
        assert_eq!(note_of(&c.convert(non(31, 100))[0]), Some(43)); // T3 → 43
        assert_eq!(note_of(&c.convert(non(33, 100))[0]), Some(41)); // T4 → 41
    }
}
