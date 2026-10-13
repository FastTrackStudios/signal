//! Trombone group definitions (orchestral brass).

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

pub mod bass_trombone;
pub mod trombone;

pub use bass_trombone::BassTrombone;
pub use trombone::Trombone;

pub struct Trombones;

impl From<Trombones> for Group<ItemMetadata> {
    fn from(_val: Trombones) -> Self {
        Self::builder("Trombones")
            .patterns(vec!["trombone", "trombones", "tbn"])
            // Transparent so Trombone / Bass Trombone appear directly under Brass.
            .transparent()
            .group(Trombone)
            .group(BassTrombone)
            .build()
    }
}
