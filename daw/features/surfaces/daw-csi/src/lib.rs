//! # daw-csi — Control Surface Integration
//!
//! CSI-style hardware control surface support for the daw, named in
//! homage to [CSICode](https://github.com/FunkybotsEvilTwin/CSICode)
//! (REAPER's Control Surface Integration, which doesn't run on
//! Linux — this does). v1 targets the Behringer X-Touch in Mackie
//! Control mode; the MCU codec covers any MCU-compatible surface.
//!
//! Architecture (vs CSI): CSI polls REAPER ~50×/sec because REAPER
//! has no push API. Here the daw **event bus** pushes every state
//! change, so the driver is event-driven end to end:
//!
//! ```text
//!  X-Touch ──MIDI──► mcu::decode ──► zones (Styx) ──Action──► Intent ──► daw-control
//!     ▲                                  ▲                          (Tracks/Transport)
//!     │                                  │
//!     └──MIDI◄── Shadow diff ◄── render ◄┴── event bus (Rx<DawEvent>)
//! ```
//!
//! - [`mcu`] — Mackie Control wire codec + X-Touch extensions
//! - [`zones`] — CSI's `.zon` files, in **Styx**: widget → action
//!   bindings with modifier keys, includes, and `@GoZone` layering.
//!   The built-in X-Touch set is embedded; `FTS_CSI_ZONES=<path>`
//!   swaps in a user file (schema: `config/zones.schema.styx`)
//! - [`action`] — the bindable action registry (CSI's `actions_` map)
//! - [`navigator`] — banking + CSI-style folder drill-down
//! - [`shadow`] — surface output diffing (motors don't twitch)
//! - [`driver`] — the event loop; [`driver::run`] is the entry point
//!
//! ## Usage
//!
//! ```ignore
//! let daw: daw_control::Daw = /* in-process or RPC connection */;
//! architect::platform::spawn(daw_csi::run(daw, daw_csi::CsiConfig::default()));
//! ```
//!
//! Customizing: copy `config/xtouch.zones.styx` somewhere, edit, set
//! `FTS_CSI_ZONES=/path/to/it`. New surface pages (sends mode, FX
//! pages) become zones + actions — no driver surgery.
//!
//! Known v1 gaps: no meter bridge (Peaks isn't in the served layer
//! set yet), no FX-parameter pages (FX param events aren't streamed),
//! no timecode display, no hold/double-press gestures yet.

pub mod action;
pub mod mcu;
pub mod navigator;
pub mod shadow;
pub mod taper;
pub mod zones;

#[cfg(feature = "midi-hardware")]
pub mod driver;
#[cfg(feature = "midi-hardware")]
pub mod midi;

#[cfg(feature = "midi-hardware")]
pub use driver::{CsiConfig, run};
