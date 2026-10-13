//! Guitar-related group definitions

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

pub mod acoustic_guitar;
pub mod banjo;
pub mod electric_guitar;
pub mod mandolin;
pub mod steel_guitar;

pub use acoustic_guitar::AcousticGuitar;
pub use banjo::Banjo;
pub use electric_guitar::ElectricGuitar;
pub use steel_guitar::SteelGuitar;

/// Top-level guitars group containing all guitar types
pub struct Guitars;

impl From<Guitars> for Group<ItemMetadata> {
    fn from(_val: Guitars) -> Self {
        Self::builder("Guitars")
            .prefix("GTR")
            // The plural matters: a folder named "Guitars" is the canonical
            // group name, and whole-word matching does not derive it from
            // "guitar".
            .patterns(vec!["guitar", "guitars", "gtr", "gtrs", "gui"])
            // Negative patterns to avoid matching bass guitars
            .exclude(vec![
                "bass_guitar",
                "bassguitar",
                "bg",
                "tb",
                "talkback",
                "vca",
                "hp",
                "headphone",
            ])
            .group(ElectricGuitar)
            .group(AcousticGuitar)
            .group(SteelGuitar)
            .group(Banjo)
            .build()
    }
}
