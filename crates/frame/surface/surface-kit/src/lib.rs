//! The hardware component kit.
//!
//! A library of premade, fully wired controls in the style of real audio
//! hardware — knobs (Bakelite, Daka-Ware, Marconi, Collet, silver-top…), VU
//! meters, buttons, switches, levers, panels — ported from FTS's audio UI
//! kit (`fts-audio-ui::hardware`). Each component is ordinary frame nodes
//! plus a `Control` whose slots point at its moving parts, so a placed
//! component can be restyled or taken apart like anything else, and a new
//! one is just a new builder.
//!
//! Agents (and the `add_component` tool) use it by id:
//!
//! ```ignore
//! for c in surface_kit::components() { println!("{} — {}", c.id, c.description) }
//! surface_kit::place(&mut builder, parent, "knob.silver_top", &Place::new("Input").size(96.0).bind(b))?;
//! ```
//!
//! Families live in their own modules and register themselves in
//! [`registry`]. See `KIT.md` for the conventions every component follows.

pub mod boutique;
pub mod controls;
pub mod deluxe;
pub mod draw;
pub mod knobs;
pub mod materials;
pub mod meters;
pub mod motion;
pub mod panels;
pub mod registry;
pub mod stomp;
pub mod viz;

pub use registry::{
    ComponentDef, DARK_INK, Family, Ink, Place, TOUCH_MIN, components, detent_sweep, drawn_bounds, find, place, place_in_cell,
};
