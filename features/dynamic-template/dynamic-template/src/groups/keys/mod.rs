//! Keyboard instrument group definitions

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

pub mod accordion;
pub mod clavichord;
pub mod electric_keys;
pub mod harpsichord;
pub mod melodica;
pub mod organ;
pub mod piano;

pub use accordion::Accordion;
pub use clavichord::Clavichord;
pub use electric_keys::ElectricKeys;
pub use harpsichord::Harpsichord;
pub use melodica::Melodica;
pub use organ::Organ;
pub use piano::Piano;

/// Top-level keys group containing all keyboard instruments
pub struct Keys;

impl From<Keys> for Group<ItemMetadata> {
    fn from(_val: Keys) -> Self {
        Self::builder("Keys")
            .prefix("Keys")
            .patterns(vec!["keys", "keyboard"])
            // A talkback mic is named for whoever it belongs to ("TB Keys"),
            // so without this it classifies as the thing it is listening to.
            .exclude(["tb", "talkback", "vca", "hp", "headphone"])
            .group(Piano)
            .group(ElectricKeys)
            .group(Organ)
            .group(Harpsichord)
            .group(Clavichord)
            .group(Accordion)
            .group(Melodica)
            .build()
    }
}
