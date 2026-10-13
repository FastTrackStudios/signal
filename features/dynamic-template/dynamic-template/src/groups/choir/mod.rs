//! Choir group definitions

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Choir group for choir and ensemble vocals
pub struct Choir;

impl From<Choir> for Group<ItemMetadata> {
    fn from(_val: Choir) -> Self {
        Self::builder("Choir")
            .prefix("Choir")
            .patterns(vec![
                "choir",
                "chorale",
                "chorus",
                "ensemble",
                "vocal_ensemble",
                "chamber_choir",
                "gospel_choir",
                "satb",
                "soprano",
                "alto",
                "tenor",
                "baritone",
                "a_cappella",
                "acappella",
            ])
            // Exclude instrument contexts - "Tenor sax" should NOT match choir
            .exclude(vec![
                "sax",
                "saxophone",
                "trumpet",
                "trombone",
                "horn",
                "clarinet",
                "flute",
                "oboe",
                "bassoon",
            ])
            .build()
    }
}
