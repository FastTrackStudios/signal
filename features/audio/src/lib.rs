//! Audio: device IO (cpal, PipeWire, JACK; duplex and monitoring) and the
//! audio service's wire types.
//!
//! The plugin-authoring layer that used to live here is `plugin-core`.

#[cfg(feature = "io")]
pub mod io;
pub mod proto;
