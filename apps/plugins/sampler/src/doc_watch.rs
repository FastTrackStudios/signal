//! Document intake from a file — lookahead in a host without
//! `host-timeline/1` (REAPER, Bitwig, Logic…).
//!
//! Point `$SIGNAL_SAMPLER_CLAP_DOC` at the part as a **Standard MIDI File**
//! (`.mid`, exported from the host, tempo map included) or a JSON document;
//! a background thread polls it (mtime + size, 500 ms) and hands it to
//! [`SharedState::set_document`](crate::plugin::SharedState::set_document)
//! on change, the same seam the host timeline feeds. The file's QN 0 is the
//! host's song position 0, so export the part from the project's start.
//! The file is kept in step with the project by hand: what the host's
//! track plays at the same time is the host's own business (mute it, or
//! let the plugin drop it — without the host's claim, events arriving
//! while the document plays are dropped).
//!
//! JSON shape (QN domain, 0-based channels):
//!
//! ```json
//! {
//!   "version": 1,
//!   "seed": 12345,
//!   "auto_divisi": false,
//!   "notes": [{ "start_qn": 0.0, "end_qn": 2.0, "chan": 0, "pitch": 60, "vel": 90 }],
//!   "ccs":   [{ "qn": 0.0, "chan": 0, "cc": 1, "val": 96 }],
//!   "tempo": [{ "qn": 0.0, "bpm": 120.0 }]
//! }
//! ```

use sampler::{DocCc, DocNote, TempoPoint, TrackDocument};
use serde::Deserialize;

/// Env var pointing at a watched document file (`.mid` or JSON).
pub const DOC_ENV: &str = "SIGNAL_SAMPLER_CLAP_DOC";

// Serde mirrors of the signal-sampler document types (which stay serde-free).

#[derive(Debug, Deserialize)]
struct NoteJson {
    start_qn: f64,
    end_qn: f64,
    #[serde(default)]
    chan: u8,
    pitch: u8,
    vel: u8,
}

#[derive(Debug, Deserialize)]
struct CcJson {
    qn: f64,
    #[serde(default)]
    chan: u8,
    cc: u8,
    val: u8,
}

#[derive(Debug, Deserialize)]
struct TempoJson {
    qn: f64,
    bpm: f64,
}

#[derive(Debug, Deserialize)]
struct DocJson {
    #[serde(default)]
    version: u64,
    #[serde(default)]
    seed: u64,
    #[serde(default)]
    auto_divisi: bool,
    #[serde(default)]
    notes: Vec<NoteJson>,
    #[serde(default)]
    ccs: Vec<CcJson>,
    #[serde(default)]
    tempo: Vec<TempoJson>,
}

/// Parse a `TrackDocument` from the dev JSON format.
///
/// # Errors
///
/// Returns an error if the JSON is invalid.
pub fn parse_document_json(text: &str) -> eyre::Result<TrackDocument> {
    let d: DocJson = serde_json::from_str(text)?;
    Ok(TrackDocument {
        version: d.version,
        seed: d.seed,
        auto_divisi: d.auto_divisi,
        notes: d
            .notes
            .into_iter()
            .map(|n| DocNote {
                start_qn: n.start_qn,
                end_qn: n.end_qn,
                chan: n.chan,
                pitch: n.pitch,
                vel: n.vel,
            })
            .collect(),
        ccs: d
            .ccs
            .into_iter()
            .map(|c| DocCc {
                qn: c.qn,
                chan: c.chan,
                cc: c.cc,
                val: c.val,
            })
            .collect(),
        tempo: d
            .tempo
            .into_iter()
            .map(|t| TempoPoint {
                qn: t.qn,
                bpm: t.bpm,
            })
            .collect(),
    })
}

/// A Standard MIDI File as a document: every track's notes and CCs merged,
/// ticks as QN (`ticks_per_beat`), the file's tempo meta events as the
/// tempo map. A note-on with velocity 0 is a note-off; an unended note
/// ends with the file.
///
/// # Errors
///
/// When the bytes aren't a metrical SMF.
pub fn document_from_smf(bytes: &[u8], seed: u64) -> eyre::Result<TrackDocument> {
    use midly::{MetaMessage, MidiMessage, Smf, Timing, TrackEventKind};
    let smf = Smf::parse(bytes).map_err(|e| eyre::eyre!("not a MIDI file: {e}"))?;
    let Timing::Metrical(tpq) = smf.header.timing else {
        eyre::bail!("a timecode MIDI file has no quarter notes");
    };
    let tpq = f64::from(u16::from(tpq).max(1));
    let mut doc = TrackDocument {
        seed,
        ..TrackDocument::default()
    };
    let mut open: Vec<((u8, u8), f64, u8)> = Vec::new();
    let mut end_qn = 0.0f64;
    for track in &smf.tracks {
        let mut tick = 0u64;
        for ev in track {
            tick += u64::from(u32::from(ev.delta));
            let qn = tick as f64 / tpq;
            end_qn = end_qn.max(qn);
            match ev.kind {
                TrackEventKind::Midi { channel, message } => {
                    let chan = u8::from(channel);
                    match message {
                        MidiMessage::NoteOn { key, vel } if u8::from(vel) > 0 => {
                            open.push(((chan, u8::from(key)), qn, u8::from(vel)));
                        }
                        MidiMessage::NoteOn { key, .. } | MidiMessage::NoteOff { key, .. } => {
                            let k = (chan, u8::from(key));
                            if let Some(i) = open.iter().position(|o| o.0 == k) {
                                let (_, start_qn, vel) = open.remove(i);
                                doc.notes.push(DocNote {
                                    start_qn,
                                    end_qn: qn,
                                    chan,
                                    pitch: k.1,
                                    vel,
                                });
                            }
                        }
                        MidiMessage::Controller { controller, value } => doc.ccs.push(DocCc {
                            qn,
                            chan,
                            cc: u8::from(controller),
                            val: u8::from(value),
                        }),
                        _ => {}
                    }
                }
                TrackEventKind::Meta(MetaMessage::Tempo(us)) => {
                    let us = f64::from(u32::from(us).max(1));
                    doc.tempo.push(TempoPoint {
                        qn,
                        bpm: 60_000_000.0 / us,
                    });
                }
                _ => {}
            }
        }
    }
    for ((chan, pitch), start_qn, vel) in open {
        doc.notes.push(DocNote {
            start_qn,
            end_qn: end_qn.max(start_qn + 1.0 / 64.0),
            chan,
            pitch,
            vel,
        });
    }
    doc.notes.retain(|n| n.end_qn > n.start_qn);
    doc.notes.sort_by(|a, b| a.start_qn.total_cmp(&b.start_qn));
    doc.ccs.sort_by(|a, b| a.qn.total_cmp(&b.qn));
    doc.tempo.sort_by(|a, b| a.qn.total_cmp(&b.qn));
    Ok(doc)
}

/// A document file: `.mid`/`.midi` as a Standard MIDI File, anything else
/// as the JSON document.
///
/// # Errors
///
/// When the file can't be read or parsed.
pub fn read_document_file(path: &str, seed: u64) -> eyre::Result<TrackDocument> {
    let is_smf = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("mid") || e.eq_ignore_ascii_case("midi"));
    if is_smf {
        document_from_smf(&std::fs::read(path)?, seed)
    } else {
        parse_document_json(&std::fs::read_to_string(path)?)
    }
}

/// Polling watcher: loads + pushes the document whenever the file changes.
///
/// Runs until `shared` is the only reference left. Spawned by the
/// plugin's `activate()` when `$SIGNAL_SAMPLER_CLAP_DOC` is set.
pub fn watch_document_file(path: String, shared: std::sync::Arc<crate::plugin::SharedState>) {
    let mut last: Option<(std::time::SystemTime, u64)> = None;
    loop {
        // Stop when the plugin instance is gone (only our clone remains).
        if std::sync::Arc::strong_count(&shared) == 1 {
            return;
        }
        let stamp = std::fs::metadata(&path)
            .ok()
            .and_then(|m| Some((m.modified().ok()?, m.len())));
        if let Some(stamp) = stamp {
            if last != Some(stamp) {
                last = Some(stamp);
                match read_document_file(&path, crate::plugin::SEED) {
                    Ok(doc) => match shared.set_document(doc) {
                        Ok(()) => tracing::info!(%path, "document (re)loaded"),
                        Err(e) => tracing::warn!(%path, "document deferred: {e}"),
                    },
                    Err(e) => tracing::warn!(%path, "document read failed: {e}"),
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A two-channel SMF: 480 tpq, 120 then 90 bpm at QN 4.
    fn smf() -> Vec<u8> {
        use midly::num::{u4, u7, u15, u24, u28};
        use midly::{
            Format, Header, MetaMessage, MidiMessage, Smf, Timing, TrackEvent, TrackEventKind,
        };
        let ev = |delta: u32, kind| TrackEvent {
            delta: u28::new(delta),
            kind,
        };
        let on = |ch: u8, key: u8, vel: u8| TrackEventKind::Midi {
            channel: u4::new(ch),
            message: MidiMessage::NoteOn {
                key: u7::new(key),
                vel: u7::new(vel),
            },
        };
        let tempo = |us: u32| TrackEventKind::Meta(MetaMessage::Tempo(u24::new(us)));
        let track = vec![
            ev(0, tempo(500_000)),
            ev(
                0,
                TrackEventKind::Midi {
                    channel: u4::new(0),
                    message: MidiMessage::Controller {
                        controller: u7::new(58),
                        value: u7::new(2),
                    },
                },
            ),
            ev(0, on(0, 60, 90)),
            ev(0, on(1, 48, 70)),
            ev(960, on(0, 62, 30)),
            ev(48, on(0, 60, 0)),
            ev(912, on(1, 48, 0)),
            ev(0, tempo(666_667)),
            ev(0, on(0, 62, 0)),
            ev(0, TrackEventKind::Meta(MetaMessage::EndOfTrack)),
        ];
        let smf = Smf {
            header: Header::new(Format::SingleTrack, Timing::Metrical(u15::new(480))),
            tracks: vec![track],
        };
        let mut out = Vec::new();
        smf.write(&mut out).unwrap();
        out
    }

    #[test]
    fn a_midi_file_is_a_document() {
        let doc = document_from_smf(&smf(), 7).unwrap();
        let notes: Vec<_> = doc
            .notes
            .iter()
            .map(|n| (n.start_qn, n.end_qn, n.chan, n.pitch, n.vel))
            .collect();
        assert_eq!(
            notes,
            vec![
                (0.0, 2.1, 0, 60, 90),
                (0.0, 4.0, 1, 48, 70),
                (2.0, 4.0, 0, 62, 30)
            ]
        );
        assert_eq!((doc.ccs[0].cc, doc.ccs[0].val), (58, 2));
        assert_eq!(doc.tempo.len(), 2);
        assert!((doc.tempo[1].bpm - 90.0).abs() < 1e-3);
        assert_eq!(doc.tempo[1].qn, 4.0);
        assert_eq!(doc.seed, 7);
    }
}
