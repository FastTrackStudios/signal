//! Compare a Pro Tools session's tempo / time-signature / marker / MIDI-note
//! data against a reference Standard MIDI File (the session's "MIDI Export").
//!
//! Usage: `cargo run -p dawfile-protools --example midi_parity -- <session.ptx> <reference.mid>`

use std::env;
use std::fs;

// ── Minimal Standard MIDI File reader ───────────────────────────────────────

struct Smf {
    division: u16,
    tracks: Vec<Vec<SmfEvent>>,
}

// Fields model the SMF events as decoded; several are only ever read
// through `Debug` when a parity run is inspected by hand.
#[allow(dead_code)]
#[derive(Debug)]
enum SmfEvent {
    Tempo { tick: u64, us_per_qn: u32 },
    TimeSig { tick: u64, num: u8, den: u8 },
    Marker { tick: u64, text: String },
    TrackName { text: String },
    NoteOn { tick: u64, note: u8, vel: u8 },
}

struct Reader<'a> {
    d: &'a [u8],
    p: usize,
}
impl<'a> Reader<'a> {
    fn u8(&mut self) -> u8 {
        let v = self.d[self.p];
        self.p += 1;
        v
    }
    fn u16(&mut self) -> u16 {
        let v = u16::from_be_bytes([self.d[self.p], self.d[self.p + 1]]);
        self.p += 2;
        v
    }
    fn u32(&mut self) -> u32 {
        let v = u32::from_be_bytes(self.d[self.p..self.p + 4].try_into().unwrap());
        self.p += 4;
        v
    }
    fn varlen(&mut self) -> u64 {
        let mut val = 0u64;
        loop {
            let b = self.u8();
            val = (val << 7) | (b & 0x7f) as u64;
            if b & 0x80 == 0 {
                break;
            }
        }
        val
    }
}

fn parse_smf(data: &[u8]) -> Smf {
    let mut r = Reader { d: data, p: 0 };
    assert_eq!(&data[0..4], b"MThd");
    r.p = 4;
    let _len = r.u32();
    let _format = r.u16();
    let ntrks = r.u16();
    let division = r.u16();
    r.p = 8 + 6;

    let mut tracks = Vec::new();
    for _ in 0..ntrks {
        assert_eq!(&data[r.p..r.p + 4], b"MTrk");
        r.p += 4;
        let len = r.u32() as usize;
        let end = r.p + len;
        let mut tick = 0u64;
        let mut running_status = 0u8;
        let mut events = Vec::new();
        while r.p < end {
            tick += r.varlen();
            let mut status = r.d[r.p];
            if status & 0x80 != 0 {
                r.p += 1;
                running_status = status;
            } else {
                status = running_status; // running status: reuse last
            }
            match status {
                0xff => {
                    let meta = r.u8();
                    let mlen = r.varlen() as usize;
                    let bytes = &r.d[r.p..r.p + mlen];
                    match meta {
                        0x51 => {
                            let us = ((bytes[0] as u32) << 16)
                                | ((bytes[1] as u32) << 8)
                                | bytes[2] as u32;
                            events.push(SmfEvent::Tempo {
                                tick,
                                us_per_qn: us,
                            });
                        }
                        0x58 => events.push(SmfEvent::TimeSig {
                            tick,
                            num: bytes[0],
                            den: 1 << bytes[1],
                        }),
                        0x06 => events.push(SmfEvent::Marker {
                            tick,
                            text: String::from_utf8_lossy(bytes).to_string(),
                        }),
                        0x03 => events.push(SmfEvent::TrackName {
                            text: String::from_utf8_lossy(bytes).to_string(),
                        }),
                        _ => {}
                    }
                    r.p += mlen;
                }
                0xf0 | 0xf7 => {
                    let slen = r.varlen() as usize;
                    r.p += slen;
                }
                _ => {
                    let hi = status & 0xf0;
                    let data_bytes = if matches!(hi, 0xc0 | 0xd0) { 1 } else { 2 };
                    let d0 = r.u8();
                    let d1 = if data_bytes == 2 { r.u8() } else { 0 };
                    if hi == 0x90 && d1 > 0 {
                        events.push(SmfEvent::NoteOn {
                            tick,
                            note: d0,
                            vel: d1,
                        });
                    }
                }
            }
        }
        r.p = end;
        tracks.push(events);
    }
    Smf { division, tracks }
}

fn main() {
    let ptx = env::args().nth(1).expect("usage: midi_parity <ptx> <mid>");
    let mid = env::args().nth(2).expect("usage: midi_parity <ptx> <mid>");

    let smf = parse_smf(&fs::read(&mid).unwrap());
    let tpq = smf.division as f64;
    println!("=== REFERENCE MIDI: {mid} ===");
    println!(
        "division = {} ticks/qn, {} tracks",
        smf.division,
        smf.tracks.len()
    );

    // Meta events live on track 0 in format-1 files.
    println!("\n-- tempo (ref) --");
    for tr in &smf.tracks {
        for e in tr {
            if let SmfEvent::Tempo { tick, us_per_qn } = e {
                let bpm = 60_000_000.0 / *us_per_qn as f64;
                println!("  @qtr {:>8.2}  {:.3} BPM", *tick as f64 / tpq, bpm);
            }
        }
    }
    println!("-- time sigs (ref) --");
    for tr in &smf.tracks {
        for e in tr {
            if let SmfEvent::TimeSig { tick, num, den } = e {
                println!("  @qtr {:>8.2}  {}/{}", *tick as f64 / tpq, num, den);
            }
        }
    }
    println!("-- markers (ref) --");
    for tr in &smf.tracks {
        for e in tr {
            if let SmfEvent::Marker { tick, text } = e {
                println!("  @qtr {:>8.2}  {:?}", *tick as f64 / tpq, text);
            }
        }
    }
    println!("-- note tracks (ref) --");
    for (i, tr) in smf.tracks.iter().enumerate() {
        let name = tr.iter().find_map(|e| match e {
            SmfEvent::TrackName { text } => Some(text.clone()),
            _ => None,
        });
        let notes = tr
            .iter()
            .filter(|e| matches!(e, SmfEvent::NoteOn { .. }))
            .count();
        if notes > 0 {
            println!(
                "  track {i} {:?}: {} notes",
                name.unwrap_or_default(),
                notes
            );
        }
    }

    // ── Our PT parse ────────────────────────────────────────────────────────
    let s = dawfile_protools::read_session(&ptx, 0).unwrap();
    let ptq = 960_000.0_f64; // PT internal ticks per quarter
    println!("\n=== OUR PT PARSE: {ptx} ===");
    println!(
        "bpm={:.3} tempo_events={} meter_events={} markers={} midi_regions={}",
        s.bpm,
        s.tempo_events.len(),
        s.meter_events.len(),
        s.markers.len(),
        s.midi_regions.len()
    );
    println!("\n-- tempo (ours) --");
    for t in &s.tempo_events {
        println!(
            "  @qtr {:>8.2}  {:.3} BPM",
            t.tick_start as f64 / ptq,
            t.bpm
        );
    }
    println!("-- time sigs (ours) --");
    for m in &s.meter_events {
        println!(
            "  @qtr {:>8.2}  bar {}  {}/{}",
            m.tick_start as f64 / ptq,
            m.measure,
            m.numerator,
            m.denominator
        );
    }
    println!("-- markers (ours) --");
    for m in &s.markers {
        println!(
            "  @qtr {:>8.2}  #{:<3} {:?}",
            m.tick_pos as f64 / ptq,
            m.number,
            m.name
        );
    }
    println!("-- midi regions (ours) --");
    let total: usize = s.midi_regions.iter().map(|r| r.events.len()).sum();
    println!("  {} regions, {} total events", s.midi_regions.len(), total);
}
