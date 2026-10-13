//! VCA group definitions.
//!
//! A VCA track carries no audio — it is a fader that controls other faders.
//! Routing one into a bus is meaningless: there is nothing to sum, and the
//! send would be silent while making the track look organized.
//!
//! So VCAs are classified deliberately and then deliberately *not* routed. That
//! distinction matters: a track that reaches no bus because nothing recognised
//! it is work for a human, while a VCA that reaches no bus is finished. Without
//! this group, `BAND RECORD VCA` was reported as unrouted in all eleven album
//! projects, and would have been swept into `UNSORTED`.

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Top-level VCA group for control-only tracks.
pub struct Vca;

impl From<Vca> for Group<ItemMetadata> {
    fn from(_val: Vca) -> Self {
        Self::builder("VCA")
            .prefix("VCA")
            .patterns(vec!["vca"])
            .build()
    }
}

#[cfg(test)]
mod tests {
    use crate::track_schema::classify_track;

    #[test]
    fn vca_tracks_classify_as_vca() {
        for name in ["BAND RECORD VCA", "VCA", "Drums VCA", "Vox VCA"] {
            let g = classify_track(name).matched_groups;
            assert!(g.iter().any(|x| x == "VCA"), "{name} → {g:?}");
        }
    }

    /// A VCA is control, not content — it must never acquire a bus.
    #[test]
    fn vcas_route_nowhere() {
        let path = classify_track("BAND RECORD VCA").matched_groups;
        assert_eq!(crate::buses::bus_for_path(&path), None);
    }

    #[test]
    fn ordinary_tracks_are_not_vcas() {
        for name in ["Kick In", "Bass DI", "GTR E - Chords"] {
            assert!(!classify_track(name)
                .matched_groups
                .iter()
                .any(|x| x == "VCA"));
        }
    }
}
