//! Bass Trombone group definition

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Bass Trombone group
pub struct BassTrombone;

impl From<BassTrombone> for Group<ItemMetadata> {
    fn from(_val: BassTrombone) -> Self {
        Self::builder("Bass Trombone")
            .patterns(vec![
                "bass trombone",
                "bass tromb",
                "bass tbn",
                "btbn",
                "btromb",
            ])
            .build()
    }
}
