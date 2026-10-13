//! The patch list: an album's inputs by performer, resolved through a
//! studio profile.
//!
//! See `Cargo.toml` for what this is; the spec is
//! `docs/spec/session/workflows.md` (`flow.patch-list.*`) and the
//! decision is session #29.

pub mod apply;
pub mod discover;
pub mod headphones;
pub mod layer;
pub mod list;
pub mod performer_row;
pub mod profile;
pub mod styx;
pub mod validate;

pub use apply::{
    Applied, Error as ApplyError, Report, Resolve, apply, effective_text, selector_matches, stale,
};
pub use discover::{Studios, WriteError as AlbumWriteError, find_album, write_album};
pub use layer::{Layered, layer};
pub use list::{Bus, Entry, Lowered, PatchList, Performer, Rig};
pub use profile::{Resolved, StudioProfile};
pub use validate::{Error as ValidationError, Plan, Planned, PlannedBus, validate};

/// The golden album's patch list, as text.
///
/// Exported rather than left as a file for consumers to reach for: the
/// render fixture and its test live in `session-daw`, and an
/// `include_str!` across a crate boundary is a path cargo's dependency
/// graph cannot see (CLAUDE.md, Rules). The bytes come from the crate
/// that owns them, the way `architect_ui::THEME_CSS` does.
pub const FIXTURE_ALBUM: &str = include_str!("../fixtures/album/patch-list.styx");

/// And the golden room's profile.
pub const FIXTURE_STUDIO: &str = include_str!("../fixtures/studios/golden-room.styx");

/// The name the fixture room goes by.
pub const FIXTURE_STUDIO_NAME: &str = "golden-room";
