//! Horn section definitions for pop/rock/jazz contexts
//!
//! This is separate from Orchestra brass - these are for horn sections
//! in modern music production (jazz, funk, soul, rock, etc.)

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

pub mod bass_trombone;
pub mod saxophone;
pub mod trombone;
pub mod trumpet;
pub mod tuba;

pub use bass_trombone::BassTrombone;
pub use saxophone::Saxophone;
pub use trombone::Trombone;
pub use trumpet::Trumpet;
pub use tuba::Tuba;

/// Top-level horns group for pop/rock/jazz horn sections
pub struct Horns;

impl From<Horns> for Group<ItemMetadata> {
    fn from(_val: Horns) -> Self {
        Self::builder("Horns")
            .prefix("Horn")
            .patterns(vec![
                // Generic horn/brass patterns
                "horn",
                "horns",
                "brass section",
                // Specific instrument families — allows sax/trumpet tracks to match
                // Horns even when "horn" isn't in the filename (e.g. "Tenor sax fill")
                "sax",
                "saxophone",
                "trumpet",
                "tpt",
                "flugelhorn",
                "fluegel",
                "flugel",
                "cornet",
                "trombone",
                "tromb",
                "tbn",
                "bass trombone",
                "tuba",
                "euphonium",
            ])
            .group(Saxophone)
            .group(Trumpet)
            .group(Trombone)
            .group(BassTrombone)
            .group(Tuba)
            .build()
    }
}
