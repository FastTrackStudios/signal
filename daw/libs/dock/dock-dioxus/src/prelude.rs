//! Conditional prelude — imports the correct Dioxus backend.

#[cfg(feature = "web")]
pub use dioxus::prelude::*;

#[cfg(feature = "native")]
pub use dioxus_native::prelude::*;
