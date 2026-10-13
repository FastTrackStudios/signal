//! Orchestra group definitions

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

pub mod brass;
pub mod harp;
pub mod percussion;
pub mod strings;
pub mod woodwinds;

pub use brass::Brass;
pub use harp::Harp;
pub use percussion::Percussion;
pub use strings::Strings;
pub use woodwinds::Woodwinds;

/// Top-level orchestra group containing all orchestral sections
pub struct Orchestra;

impl From<Orchestra> for Group<ItemMetadata> {
    fn from(_val: Orchestra) -> Self {
        Self::builder("Orchestra")
            .prefix("Orch")
            .patterns(vec!["orchestra", "orchestral", "symphonic", "symphony"])
            .group(Strings)
            .group(Brass)
            .group(Woodwinds)
            .group(Harp)
            .group(Percussion)
            .build()
    }
}
