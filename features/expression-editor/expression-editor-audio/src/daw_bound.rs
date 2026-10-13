//! What the drum workspace asks of a daw, as one bound.
//!
//! Lives here rather than beside `DrumHost` because both the standalone
//! window and the REAPER panel need to name it, and this is the crate
//! they share — it is already where the edits themselves live
//! (`slip_hit`, `apply_split`, `apply_warp`), each generic over the
//! services it needs.

/// Everything the drum workspace asks of a daw.
///
/// Named as one bound so the port to REAPER is a change of type
/// parameter rather than an archaeology exercise, and so that "does
/// REAPER support this" is a question the compiler answers.
///
/// It is answerable, and the answer is yes: every operation here is
/// implemented in `daw-reaper`, and the edit vocabulary — split with a
/// leading pad and a crossfade, or stretch markers on transients, both
/// applied identically to every mic of a group — is the same vocabulary
/// REAPER's own audio quantizer scripts use, because it is the only one
/// REAPER's item model offers.
///
/// [`AudioAccessors`] is the one worth noting: detection needs the
/// take's samples, and REAPER exposes exactly that through its audio
/// accessor API rather than requiring the files be read off disk.
// r[impl drums.host.daw-agnostic]
pub trait DrumDaw:
    ::daw::service::Items
    + ::daw::service::Takes
    + ::daw::service::Tracks
    + ::daw::service::Projects
    + ::daw::service::Markers
    + ::daw::service::Regions
    + ::daw::service::TempoMap
    + ::daw::service::audio_accessor::AudioAccessors
    + ::daw::service::StretchMarkers
    + Clone
{
}

impl<T> DrumDaw for T where
    T: ::daw::service::Items
        + ::daw::service::Takes
        + ::daw::service::Tracks
        + ::daw::service::Projects
        + ::daw::service::Markers
        + ::daw::service::Regions
        + ::daw::service::TempoMap
        + ::daw::service::audio_accessor::AudioAccessors
        + ::daw::service::StretchMarkers
        + Clone
{
}
