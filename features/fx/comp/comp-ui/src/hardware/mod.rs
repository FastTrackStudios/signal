//! Hardware-faceplate building blocks.
//!
//! The profile system is moving from "re-tint the FTS surface" to "swap in the
//! actual unit's front panel", so these are the parts a panel is made of. Kept
//! separate from [`crate::sections`] (the FTS-native control surface) because
//! they answer to a different brief: look like the hardware, not like the app.
//!
//! Geometry lives in `*_svg` modules with no framework deps so it stays
//! unit-testable.

pub mod vu_svg;
