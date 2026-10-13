//! Frame: the design and control-surface engine.
//!
//! The document, surface, motion and feed protocols are always here; the
//! runtime, rendering, themes, file IO, libraries, Figma import, design
//! tools and the live and embedded hosts are each behind a feature.

pub mod design;
pub mod document;
#[cfg(feature = "embed")]
pub mod embed;
pub mod feed;
pub mod figma;
pub mod library;
#[cfg(feature = "live")]
pub mod live;
pub mod motion;
#[cfg(feature = "render")]
pub mod render;
pub mod surface;
pub mod theme;
