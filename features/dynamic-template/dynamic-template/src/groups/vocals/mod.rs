//! Vocal group definitions

use crate::item_metadata::prelude::*;

pub mod background_vocals;
pub mod lead_vocals;

pub use background_vocals::BackgroundVocals;
pub use lead_vocals::LeadVocals;

/// Top-level vocals group containing lead and background vocals
///
/// Not transparent - creates a Vocals folder when there are multiple vocal types
/// (e.g., Lead and BGVs). This provides clearer organization in the track list.
pub struct Vocals;

impl From<Vocals> for ItemMetadataGroup {
    fn from(_val: Vocals) -> Self {
        Self::builder("Vocals")
            .prefix("V")
            .patterns([
                "vocal", "vocals", "vox", "voc", "voca", "voice", "lv", "bv", "bg", "bgv", "bgvs",
                "bvs", // background-vocal abbreviations enter the Vocals bus
                // A screamed part is a vocal (a metal band's "Scream").
                "scream", "screams", "screamer",
            ])
            // Exclude non-vocal voice effects (these should go to SFX)
            // Also exclude "cowbell" to prevent compound names like "CowbellGangVox" from matching
            .exclude([
                "robot",
                "vocoder",
                "talkbox",
                "cowbell",
                "tb",
                "talkback",
                "vca",
                "hp",
                "headphone",
            ])
            // The Vocals folder is the vocal summing bus (VCA target) for Lead +
            // BGVs. Choir routes to the top-level Choir group (a monarchy
            // limitation: a "Choir" subgroup here duplicates against that group,
            // whose name doubles as a match token) — route it to this VCA in the
            // DAW rather than nesting it.
            .group(LeadVocals)
            .group(BackgroundVocals)
            .build()
    }
}
