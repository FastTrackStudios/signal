//! Document intake — the **phase-3 seam** (see `docs/plan/document-mode.md`,
//! "Self-sourced document").
//!
//! Phase 3 will have the plugin pull its own track's MIDI through the daw
//! crate (own-track identification, item content hashing, rebuild on
//! change). Everything downstream of "a new [`TrackDocument`] exists" is
//! already final and lives here:
//!
//! - [`SharedState::set_document`](crate::plugin::SharedState::set_document)
//!   — annotate OFF the audio thread, publish the `Arc<Schedule>` via an
//!   atomic swap; the audio thread picks it up at the next block boundary.
//!
//! Until the daw-crate source lands, a **dev-only file loader** makes phase
//! 2 testable inside REAPER: point `$SIGNAL_SAMPLER_CLAP_DOC` at a JSON
//! file; a background thread polls it (mtime + size, 500 ms) and pushes it
//! through the same `set_document` seam on change. Delete this file watcher
//! when phase 3 replaces the source.
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

use serde::Deserialize;
use signal_sampler::{DocCc, DocNote, TempoPoint, TrackDocument};

/// Env var pointing at a watched TrackDocument JSON file (dev-only).
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

/// Parse a TrackDocument from the dev JSON format.
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

/// Dev-only polling watcher: loads + pushes the document whenever the file
/// changes. Runs until `shared` is the only reference left. Spawned by the
/// plugin's `initialize()` when `$SIGNAL_SAMPLER_CLAP_DOC` is set.
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
                match std::fs::read_to_string(&path).map_err(eyre::Report::from) {
                    Ok(text) => match parse_document_json(&text) {
                        Ok(doc) => match shared.set_document(doc) {
                            Ok(()) => tracing::info!(%path, "document (re)loaded"),
                            Err(e) => tracing::warn!(%path, "document deferred: {e}"),
                        },
                        Err(e) => tracing::warn!(%path, "document parse failed: {e}"),
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

    #[test]
    fn parses_the_dev_json_shape() {
        let doc = parse_document_json(
            r#"{
                "version": 3, "seed": 7, "auto_divisi": true,
                "notes": [{ "start_qn": 0.0, "end_qn": 2.0, "chan": 1, "pitch": 60, "vel": 90 }],
                "ccs":   [{ "qn": 0.5, "cc": 1, "val": 96 }],
                "tempo": [{ "qn": 0.0, "bpm": 90.0 }]
            }"#,
        )
        .expect("parse");
        assert_eq!(doc.version, 3);
        assert!(doc.auto_divisi);
        assert_eq!(doc.notes.len(), 1);
        assert_eq!(doc.notes[0].chan, 1);
        assert_eq!(doc.ccs[0].chan, 0, "chan defaults to 0");
        assert_eq!(doc.tempo[0].bpm, 90.0);
    }
}
