//! Scenes: which tracks a flow is looked at through, as data.
//!
//! A scene (spec #48, "Scenes as data"; decision #28) is which tracks
//! are shown in the arrangement's panel and in the mixer, how large, and
//! which folders are folded — a rule resolved against the session's own
//! taxonomy, never a list of track ids, so it fits a real session whose
//! shape differs from the reference.
//!
//! The module is one deep function behind a small interface:
//!
//! - [`table::scenes`] is the built-in table — nine [`Scene`] values,
//!   every type `Facet`-derived so the styx step is a loader and nothing
//!   else.
//! - [`resolve::resolve`] turns a scene and a session into a row list.
//!   Ordered rules, **last match wins**, the scene's `default` covers
//!   the rest.
//! - [`surface::TABLES`] is the one place a size class becomes a pixel
//!   count, separately for row height and strip width.
//! - [`follow::Follow`] is follow-mode: what a window shows as the DAW
//!   mode changes.
//!
//! Two appliers consume the row list and there is only one resolve: the
//! window renders it as it is, and the REAPER applier walks it and
//! writes show, fold and size onto real tracks. That is the whole reason
//! the output is a row list rather than a pile of per-surface booleans.

pub mod active_language;
pub mod adapt;
pub mod facts;
pub mod follow;
pub mod language;
pub mod resolve;
pub mod selector;
pub mod surface;
pub mod table;
pub mod types;

pub use active_language::{get_active_language, set_active_language};
pub use adapt::{from_flat, from_tracks, is_pair_half};
pub use facts::{Fact, Segment};
pub use follow::{Conflict, Follow};
pub use language::{classify_language, Language};
pub use resolve::{resolve, PerformerOrder, Row, Target, UNASSIGNED};
pub use selector::{Rank, Role, Selector};
pub use surface::{Focus, SurfaceTables, TABLES};
pub use table::{scene, scenes};
pub use types::{Audience, Effect, Fold, GroupBy, Resolved, Rule, Scene, Size, Surface};
