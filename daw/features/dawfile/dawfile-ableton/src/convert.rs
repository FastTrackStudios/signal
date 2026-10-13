//! Conversion from Ableton types to `daw_proto` types.
//!
//! Maps `AbletonLiveSet` data to the format-agnostic protocol types
//! used by the rest of the DAW system.

use crate::types::AbletonLiveSet;
use daw_proto::capability::{Capability, FeatureSupport};

/// The capability declaration for the Ableton Live set format.
///
/// Supports both reading and writing for core capabilities.
pub fn feature_support() -> FeatureSupport {
    FeatureSupport::new()
        .read_write(&[
            Capability::Project,
            Capability::Tracks,
            Capability::TrackRouting,
            Capability::Items,
            Capability::Midi,
            Capability::Markers,
            Capability::TempoMap,
        ])
        .read_only(&[Capability::FxChain, Capability::Automation])
}

/// Summary of a parsed live set, suitable for display.
pub fn set_summary(set: &AbletonLiveSet) -> String {
    format!(
        "Ableton Live {} set @ {:.1} BPM ({}/{}): {} audio tracks, {} MIDI tracks, \
         {} return tracks, {} group tracks, {} locators",
        set.version,
        set.tempo,
        set.time_signature.numerator,
        set.time_signature.denominator,
        set.audio_tracks.len(),
        set.midi_tracks.len(),
        set.return_tracks.len(),
        set.group_tracks.len(),
        set.locators.len(),
    )
}
