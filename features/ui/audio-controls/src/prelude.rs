//! Conditional Dioxus prelude import
//!
//! Matches the session-ui / keyflow-ui / signal-ui pattern:
//! - `web` feature -> dioxus (Wry/WebView)
//! - `native` feature -> dioxus-native (Blitz/GPU)

#[cfg(feature = "web")]
pub use dioxus::prelude::*;

#[cfg(feature = "native")]
pub use dioxus_native::prelude::*;
