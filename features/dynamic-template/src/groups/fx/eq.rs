//! EQ effect group definition

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// EQ effect group
pub struct EQ;

impl From<EQ> for Group<ItemMetadata> {
    fn from(_val: EQ) -> Self {
        Group::builder("EQ")
            .patterns(vec![
                "eq",
                "equalizer",
                "parametric",
                "graphic",
                "filter",
                "highpass",
                "lowpass",
                "bandpass",
                "notch",
                "shelf",
                "bell",
            ])
            .build()
    }
}
