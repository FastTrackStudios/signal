//! Typed identifiers — prevent mixing up IDs from different domains.
//!
//! Each dock entity gets its own newtype wrapper around `Uuid`.

use facet::Facet;
use uuid::Uuid;

macro_rules! typed_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
        pub struct $name(Uuid);

        impl $name {
            /// Generate a new random ID.
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            /// Wrap an existing UUID.
            pub fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }

            /// Get the underlying UUID.
            pub fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

typed_id!(
    /// Identifies a node in the dock layout tree.
    NodeId
);
typed_id!(
    /// Identifies a leaf tile (content area) in the layout.
    TileId
);
typed_id!(
    /// Identifies a saved dock preset (screenset).
    PresetId
);
typed_id!(
    /// Identifies an OS window in a dock workspace.
    WindowId
);
