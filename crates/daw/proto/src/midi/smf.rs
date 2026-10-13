//! Standard MIDI files, read and written as take snapshots.
//!
//! Deliberately behind the `Midi` service rather than exposed as its
//! own API: a caller should read a `.mid` and a live take through one
//! surface and get the same types back either way. An editor that has
//! to know which it is talking to ends up with two of everything.
//!
//! Format 0 and 1 only, which is what everything writes.
//!
//! Lives in `daw-proto` rather than in a backend so REAPER and
//! standalone reach the same codec — a backend owning it would force
//! the other to depend on it, which is the wrong direction.

use std::io::{Read, Write};

use super::{MidiCC, MidiNote, MidiPitchBend, MidiTakeContent, MidiTakeSnapshot};

const HEADER: &[u8; 4] = b"MThd";
const TRACK: &[u8; 4] = b"MTrk";

fn be16(b: &[u8]) -> u16 {
    u16::from_be_bytes([b[0], b[1]])
}

fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

/// A variable-length quantity, returning `(value, bytes_read)`.
fn varint(data: &[u8], mut i: usize) -> Option<(u32, usize)> {
    let start = i;
    let mut v: u32 = 0;
    loop {
        let b = *data.get(i)?;
        i += 1;
        v = (v << 7) | (b & 0x7f) as u32;
        if b & 0x80 == 0 {
            break;
        }
        // A VLQ is at most four bytes; more means the file is corrupt
        // and we would otherwise spin.
        if i - start >= 4 {
            return None;
        }
    }
    Some((v, i - start))
}

fn write_varint(out: &mut Vec<u8>, mut v: u32) {
    let mut buf = [0u8; 4];
    let mut n = 0;
    buf[n] = (v & 0x7f) as u8;
    v >>= 7;
    while v > 0 {
        n += 1;
        buf[n] = ((v & 0x7f) as u8) | 0x80;
        v >>= 7;
    }
    for i in (0..=n).rev() {
        out.push(buf[i]);
    }
}

/// Read `track_index` from a standard MIDI file.
///
/// Positions are rescaled to the snapshot's own PPQ, so a caller never
/// has to know what division the file used.
pub fn read(path: &str, track_index: usize) -> Option<MidiTakeSnapshot> {
    let mut data = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .read_to_end(&mut data)
        .ok()?;
    parse(&data, track_index)
}

/// Parse an in-memory standard MIDI file.
pub fn parse(data: &[u8], track_index: usize) -> Option<MidiTakeSnapshot> {
    if data.len() < 14 || &data[0..4] != HEADER {
        return None;
    }
    let division = be16(&data[12..14]);
    // SMPTE division has bit 15 set and means frames, not ticks per
    // quarter — a different time base we do not pretend to handle.
    if division & 0x8000 != 0 {
        return None;
    }
    let file_ppq = division as f64;

    let mut i = 8 + be32(&data[4..8]) as usize;
    let mut track = 0usize;
    while i + 8 <= data.len() {
        let len = be32(&data[i + 4..i + 8]) as usize;
        let body_start = i + 8;
        let body_end = (body_start + len).min(data.len());
        if &data[i..i + 4] == TRACK {
            if track == track_index {
                return Some(parse_track(&data[body_start..body_end], file_ppq));
            }
            track += 1;
        }
        i = body_end;
    }
    None
}

/// Parse one track chunk's events.
fn parse_track(body: &[u8], file_ppq: f64) -> MidiTakeSnapshot {
    // Everything is emitted at 960 PPQ regardless of the file's own
    // division, so downstream never carries a per-file time base.
    const OUT_PPQ: f64 = 960.0;
    let scale = OUT_PPQ / file_ppq.max(1.0);

    let mut notes: Vec<MidiNote> = Vec::new();
    let mut ccs: Vec<MidiCC> = Vec::new();
    let mut bends: Vec<MidiPitchBend> = Vec::new();
    // Sounding notes, keyed by (channel, pitch).
    let mut open: Vec<(u8, u8, u8, f64)> = Vec::new();

    let mut i = 0usize;
    let mut tick: u64 = 0;
    let mut status: u8 = 0;

    while i < body.len() {
        let Some((delta, n)) = varint(body, i) else {
            break;
        };
        i += n;
        tick += delta as u64;
        let Some(&b) = body.get(i) else { break };

        if b == 0xff {
            // Meta: skip by its declared length.
            i += 1;
            let Some(&_kind) = body.get(i) else { break };
            i += 1;
            let Some((len, n)) = varint(body, i) else {
                break;
            };
            i += n + len as usize;
            continue;
        }
        if b == 0xf0 || b == 0xf7 {
            i += 1;
            let Some((len, n)) = varint(body, i) else {
                break;
            };
            i += n + len as usize;
            continue;
        }

        // Running status: a data byte here reuses the previous status.
        if b & 0x80 != 0 {
            status = b;
            i += 1;
        }
        let kind = status & 0xf0;
        let channel = status & 0x0f;
        let t = tick as f64 * scale;

        match kind {
            0x80 | 0x90 => {
                let (Some(&d1), Some(&d2)) = (body.get(i), body.get(i + 1)) else {
                    break;
                };
                i += 2;
                // A note-on with zero velocity is a note-off; every
                // running-status writer uses this.
                let is_off = kind == 0x80 || d2 == 0;
                if is_off {
                    if let Some(pos) = open.iter().position(|(c, p, ..)| *c == channel && *p == d1)
                    {
                        let (_, pitch, vel, start) = open.remove(pos);
                        notes.push(MidiNote {
                            index: notes.len() as u32,
                            channel,
                            pitch,
                            velocity: vel,
                            start_ppq: start,
                            length_ppq: (t - start).max(1.0),
                            selected: false,
                            muted: false,
                        });
                    }
                } else {
                    open.push((channel, d1, d2, t));
                }
            }
            0xb0 => {
                let (Some(&d1), Some(&d2)) = (body.get(i), body.get(i + 1)) else {
                    break;
                };
                i += 2;
                ccs.push(MidiCC {
                    index: ccs.len() as u32,
                    channel,
                    controller: d1,
                    value: d2,
                    position_ppq: t,
                    selected: false,
                });
            }
            0xe0 => {
                let (Some(&d1), Some(&d2)) = (body.get(i), body.get(i + 1)) else {
                    break;
                };
                i += 2;
                let raw = ((d2 as i32) << 7 | d1 as i32) - 8192;
                bends.push(MidiPitchBend {
                    index: bends.len() as u32,
                    channel,
                    value: raw as i16,
                    position_ppq: t,
                    selected: false,
                });
            }
            0xa0 | 0xc0 | 0xd0 => {
                // Poly pressure and program/channel pressure: skip
                // their data bytes so the stream stays aligned.
                i += if kind == 0xc0 || kind == 0xd0 { 1 } else { 2 };
            }
            _ => break,
        }
    }

    // A note left sounding at the end of the track is malformed but
    // common; give it a nominal length rather than dropping it.
    let end = tick as f64 * scale;
    for (channel, pitch, velocity, start) in open {
        notes.push(MidiNote {
            index: notes.len() as u32,
            channel,
            pitch,
            velocity,
            start_ppq: start,
            length_ppq: (end - start).max(OUT_PPQ),
            selected: false,
            muted: false,
        });
    }
    notes.sort_by(|a, b| {
        a.start_ppq
            .partial_cmp(&b.start_ppq)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for (i, n) in notes.iter_mut().enumerate() {
        n.index = i as u32;
    }

    MidiTakeSnapshot {
        length_ppq: notes.iter().map(|n| n.end_ppq()).fold(end, f64::max),
        notes,
        ccs,
        pitch_bends: bends,
        channel_pressures: Vec::new(),
        poly_pressures: Vec::new(),
        note_expressions: Vec::new(),
        ppq: OUT_PPQ,
    }
}

/// Serialize a take's contents as a format-0 standard MIDI file.
pub fn write(path: &str, content: &MidiTakeContent, ppq: f64) -> std::io::Result<()> {
    let bytes = encode(content, ppq);
    std::fs::File::create(path)?.write_all(&bytes)
}

/// Encode to bytes, without touching the filesystem.
pub fn encode(content: &MidiTakeContent, ppq: f64) -> Vec<u8> {
    let division = ppq.round().clamp(24.0, 32767.0) as u16;

    // Absolute-time events first, then sorted, then delta-encoded —
    // interleaving note-offs correctly any other way is a mess.
    let mut events: Vec<(u64, u8, u8, u8)> = Vec::new();
    for n in &content.notes {
        let start = n.start_ppq.max(0.0).round() as u64;
        let end = (n.start_ppq + n.length_ppq).max(0.0).round() as u64;
        events.push((start, 0x90 | (n.channel & 0x0f), n.pitch, n.velocity.max(1)));
        events.push((end, 0x80 | (n.channel & 0x0f), n.pitch, 0));
    }
    for c in &content.ccs {
        events.push((
            c.position_ppq.max(0.0).round() as u64,
            0xb0 | (c.channel & 0x0f),
            c.controller,
            c.value,
        ));
    }
    for b in &content.pitch_bends {
        let raw = (b.value as i32 + 8192).clamp(0, 16383);
        events.push((
            b.position_ppq.max(0.0).round() as u64,
            0xe0 | (b.channel & 0x0f),
            (raw & 0x7f) as u8,
            ((raw >> 7) & 0x7f) as u8,
        ));
    }
    // Note-offs before note-ons at the same tick, so a repeated pitch
    // does not swallow its own retrigger.
    events.sort_by_key(|&(t, st, d1, _)| (t, (st & 0xf0 != 0x80) as u8, d1));

    let mut track = Vec::new();
    let mut prev = 0u64;
    for (t, status, d1, d2) in events {
        write_varint(&mut track, (t - prev.min(t)) as u32);
        prev = t;
        track.push(status);
        track.push(d1 & 0x7f);
        track.push(d2 & 0x7f);
    }
    // End of track.
    write_varint(&mut track, 0);
    track.extend_from_slice(&[0xff, 0x2f, 0x00]);

    let mut out = Vec::new();
    out.extend_from_slice(HEADER);
    out.extend_from_slice(&6u32.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes()); // format 0
    out.extend_from_slice(&1u16.to_be_bytes()); // one track
    out.extend_from_slice(&division.to_be_bytes());
    out.extend_from_slice(TRACK);
    out.extend_from_slice(&(track.len() as u32).to_be_bytes());
    out.extend_from_slice(&track);
    out
}
