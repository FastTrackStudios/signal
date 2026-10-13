//! Electronic kick drum group definition

use crate::item_metadata::prelude::*;

/// Electronic kick drum group
pub struct Kick;

impl From<Kick> for ItemMetadataGroup {
    fn from(_val: Kick) -> Self {
        // Define multi-mic positions as a Group
        let multi_mic = Self::builder("MultiMic")
            .patterns(["808", "808-Kick"])
            .build();

        // Use the convenience method - extension trait is in scope via prelude
        // Require parent match so this only matches when "Electronic Kit" also matches
        Self::builder("Kick")
            .patterns(["808"])
            .multi_mic(multi_mic)
            .requires_parent_match()
            .build()
    }
}
