//! Reference track group definitions
//!
//! This group captures bounced/printed mixes and reference tracks:
//! - Master/mix prints
//! - Reference mixes
//! - 2-track bounces
//! - Rehearsal mixes
//! - Any bounced version of the song for reference
//! - Stem-split output (`Reference/Stem Split`), which is reference material
//!   too: a demucs/LALAL separation of the finished record, not tracked parts

use crate::groups::stem_split::StemSplit;
use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Top-level Reference group for bounced mixes and reference tracks
pub struct Reference;

impl From<Reference> for Group<ItemMetadata> {
    fn from(_val: Reference) -> Self {
        Self::builder("Reference")
            .prefix("REF")
            .patterns(vec![
                // Master/mix patterns
                "master",
                "mix",
                "print",
                "bounce",
                "bounced",
                "stereo mix",
                "final mix",
                "rough mix",
                // Reference patterns
                "reference",
                // The folder the organizer lays reference material out in.
                "references",
                "ref",
                "ref mix",
                "reference mix",
                // Original master / released-track references (MultiTracks ship an
                // "(Original Track)" full-mix reference alongside the stems)
                "original",
                "original track",
                "original mix",
                "master mix",
                "album mix",
                // Mixdown abbreviations
                "MDN",
                "mixdown",
                "mix down",
                // 2-track patterns
                "2 track",
                "2-track",
                "2track",
                "two track",
                "stereo",
                // Rehearsal/demo patterns
                "rehearsal",
                "rehearsal mix",
                "demo",
                "demo mix",
                "scratch",
                "scratch mix",
                // Export/render patterns
                "export",
                "render",
                "rendered",
            ])
            // A printed click or shaker is the guide's (`ClickPrint`,
            // `ShakePrint`), not a print of the mix.
            .exclude(vec![
                "clickprint",
                "click print",
                "shakeprint",
                "shake print",
                "shaker print",
            ])
            // Stem splits are reference material — a separation of the finished
            // record, not parts anyone played. Nesting them here means the path
            // is ["Reference", "Stem Split"], which routes to the same
            // monitor bus as the rest of Reference and keeps them out of the
            // mix.
            .group(StemSplit)
            .build()
    }
}
