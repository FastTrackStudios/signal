//! A reference render of each published schedule, for a host's end-to-end
//! test: with `$SIGNAL_SAMPLER_CLAP_REFERENCE` set to a directory, every
//! schedule the plugin publishes is also rendered offline in full
//! (`document::render_schedule`, on a fresh bank of the same patch) and
//! written there, off the audio thread:
//!
//! - `reference-<version>.f32` — the full render, interleaved stereo, f32
//!   little-endian: what the host must hear from any start, bit for bit;
//! - `reference-<version>.json` — its frames, the schedule's counts, every
//!   legato transition's engine-reported heard arrival (`arrival`, frames)
//!   and every marker, so a test can hold each arrival to the grid tick
//!   it computes itself.
//!
//! `<version>` is the document's (the host timeline's revision). Off when
//! the variable is unset — the plugin's normal case.

use std::sync::PoisonError;

use sampler::document::{DocumentRenderOptions, MarkerKind, render_schedule};
use sampler::{Schedule, TrackDocument};
use serde_json::json;

use crate::config::INSTRUMENT_ID;
use crate::plugin::SharedState;

/// The directory references are written to.
pub const REFERENCE_ENV: &str = "SIGNAL_SAMPLER_CLAP_REFERENCE";

/// Write `sched`'s reference if asked to (see the module docs).
pub fn write_if_asked(shared: &SharedState, doc: &TrackDocument, sched: &Schedule) {
    let Ok(dir) = std::env::var(REFERENCE_ENV) else {
        return;
    };
    let Some(make) = shared
        .bank_maker
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
    else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let mut bank = make();
    let res = render_schedule(
        &mut bank,
        INSTRUMENT_ID,
        sched,
        &DocumentRenderOptions {
            tail_sec: 3.0,
            ..Default::default()
        },
    );
    let kind = |k: MarkerKind| match k {
        MarkerKind::NoteStart => "note_start",
        MarkerKind::TransitionStart => "transition_start",
        MarkerKind::Rebow => "rebow",
        MarkerKind::Arrival => "arrival",
        MarkerKind::Release => "release",
    };
    let report = json!({
        "version": doc.version,
        "sample_rate": sched.sample_rate,
        "frames": res.audio.len() / 2,
        "notes": sched.note_count,
        "legato": sched.legato_count,
        "shorts": sched.short_count,
        "max_prefire_lead": sched.max_prefire_lead,
        "reactive_fallbacks": res.reactive_fallbacks,
        "transitions": res.transitions.iter().map(|t| json!({
            "line": t.line, "from": t.from_note, "to": t.to_note,
            "fire": t.frame, "arrival": t.arrival,
        })).collect::<Vec<_>>(),
        "markers": sched.markers.iter().map(|m| json!({
            "frame": m.frame, "line": m.line, "note": m.note, "kind": kind(m.kind),
        })).collect::<Vec<_>>(),
    });
    let bytes: Vec<u8> = res.audio.iter().flat_map(|s| s.to_le_bytes()).collect();
    let stem = format!("reference-{}", doc.version);
    let wrote = std::fs::create_dir_all(&dir)
        .and_then(|()| std::fs::write(dir.join(format!("{stem}.f32")), bytes))
        .and_then(|()| std::fs::write(dir.join(format!("{stem}.json")), report.to_string()));
    match wrote {
        Ok(()) => tracing::info!(
            reference.version = doc.version,
            reference.frames = res.audio.len() / 2,
            reference.transitions = res.transitions.len(),
            "reference render written"
        ),
        Err(e) => tracing::warn!(error = %e, "reference render not written"),
    }
}
