//! Strings group definition
//!
//! Covers non-orchestral / session string instruments: fiddle, violin, viola, cello.
//! Distinct from `Orchestra > Strings` which represents full orchestral string sections.

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Top-level strings group for session string instruments (fiddle, violin, cello, etc.)
pub struct Strings;

impl From<Strings> for Group<ItemMetadata> {
    fn from(_val: Strings) -> Self {
        Self::builder("Fiddle")
            .prefix("FDL")
            .patterns(vec![
                // Fiddle (folk/country/bluegrass)
                "fiddle", // Violin
                "violin", "vln", // Viola
                "viola", "vla", // Cello
                "cello",
            ])
            .build()
    }
}
