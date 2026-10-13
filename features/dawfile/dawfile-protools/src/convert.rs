//! Conversion from Pro Tools types to `daw_proto` types.
//!
//! Maps `ProToolsSession` data to the format-agnostic protocol types
//! used by the rest of the DAW system.

use crate::types::ProToolsSession;
use daw_proto::capability::{Capability, FeatureSupport};

/// The capability declaration for the Pro Tools file format parser.
///
/// This is read-only: we can parse session data but cannot write .ptx files.
pub fn feature_support() -> FeatureSupport {
    FeatureSupport::new().read_only(&[
        Capability::Project,
        Capability::Tracks,
        Capability::Items,
        Capability::Regions,
        Capability::Midi,
        Capability::TempoMap,
    ])
}

/// Summary of a parsed session, suitable for display.
pub fn session_summary(session: &ProToolsSession) -> String {
    let tempo_str = if session.tempo_events.len() == 1 {
        format!("{:.1} BPM", session.bpm)
    } else {
        format!(
            "{:.1} BPM ({} changes)",
            session.bpm,
            session.tempo_events.len() - 1
        )
    };

    let meter_str = if session.meter_events.is_empty() {
        String::new()
    } else {
        let first = &session.meter_events[0];
        if session.meter_events.len() == 1 {
            format!(", {}/{}", first.numerator, first.denominator)
        } else {
            format!(
                ", {}/{} ({} meter changes)",
                first.numerator,
                first.denominator,
                session.meter_events.len()
            )
        }
    };

    let plugin_str = if session.plugins.is_empty() {
        String::new()
    } else {
        format!(", {} plugins", session.plugins.len())
    };

    let io_str = if session.io_channels.is_empty() {
        String::new()
    } else {
        format!(", {} I/O channels", session.io_channels.len())
    };

    format!(
        "Pro Tools v{} @ {}Hz {}{}: {} audio files, {} audio regions, \
         {} audio tracks, {} MIDI regions, {} MIDI tracks{}{}",
        session.version,
        session.session_sample_rate,
        tempo_str,
        meter_str,
        session.audio_files.len(),
        session.audio_regions.len(),
        session.audio_tracks.len(),
        session.midi_regions.len(),
        session.midi_tracks.len(),
        plugin_str,
        io_str,
    )
}
