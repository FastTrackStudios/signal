//! Hardware-faceplate building blocks.
//!
//! These are the parts an outboard-gear faceplate is made of: a VU movement, a
//! pointer knob with a printed scale ring, panel switches, and the panel
//! itself with its rack ears and silkscreen. Kept separate from
//! [`crate::controls`] (the FTS-native widgets) because they answer to a
//! different brief: look like the hardware, not like the app.
//!
//! Shared by every FTS plugin that wears hardware profiles — the compressor's
//! nine units, the EQ's four — with each plugin supplying only its own layout
//! tables and the handles behind the controls.
//!
//! Geometry lives in `*_svg` modules with no framework deps so it stays
//! unit-testable; the components on top of them need the editor's Dioxus
//! stack.

pub mod button;
pub mod button_kit;
pub mod button_parts;
pub mod knob;
pub mod knob_kit;
pub mod knob_parts;
pub mod knob_svg;
pub mod lever;
pub mod panel;
pub mod panel_svg;
pub mod rack;
pub mod switches;
pub mod vu;
pub mod vu_faces;
pub mod vu_kit;
pub mod vu_svg;
