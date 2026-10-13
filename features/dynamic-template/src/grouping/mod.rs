//! FTS grouping: what the watcher provides itself, and what still needs
//! a REAPER group.
//!
//! REAPER's 128 track groups are a finite resource shared across every
//! project of an album, so FTS spends as few as it can. The split
//! (decided on session#30, measured on daw#12):
//!
//! * **The watcher provides the booleans** — arm, input monitoring and
//!   record input follow the rig; a layer's channels arm together.
//!   These are idempotent writes over [`gangs`], cost no slot, and work
//!   identically on REAPER and on daw-standalone.
//! * **REAPER VCA groups provide the faders** — a folder whose children
//!   route past it ([`vca`]), and one VCA per language. Playback-time,
//!   automation-aware scaling is not something a watcher can imitate by
//!   writing volumes: it would fight every automation lane it touched.
//!
//! Everything here is a **pure function of the classified facts**. The
//! watcher that applies it holds no state of its own, which is what
//! makes re-deriving on project open the whole persistence story —
//! see [`slots`] for why that matters more than it sounds.

pub mod gangs;
pub mod language;
pub mod slots;
pub mod vca;
pub mod watch;

pub use gangs::{diff, gangs, Gang, GangKey};
pub use language::{audible, follow_language, Switched};
pub use slots::{assign, survives_save, Purpose};
pub use vca::{leads, VcaLead};
pub use watch::{follow_arm, Followed};

#[cfg(test)]
mod tests;
