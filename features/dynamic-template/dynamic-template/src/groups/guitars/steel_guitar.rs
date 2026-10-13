//! Steel guitar group definition

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Steel guitar group (pedal steel, lap steel, etc.)
pub struct SteelGuitar;

impl From<SteelGuitar> for Group<ItemMetadata> {
    fn from(_val: SteelGuitar) -> Self {
        Self::builder("Steel")
            .patterns(vec![
                "steel",
                "pedal steel",
                "lap steel",
                "steel guitar",
                "pedal_steel",
                "lap_steel",
            ])
            // A steel-string guitar is an acoustic, not a steel.
            .exclude(vec!["steel string", "steel-string", "steelstring"])
            .build()
    }
}
