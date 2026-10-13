//! The demo project the four #149 scenarios run against.
//!
//! #149's destination is not "the code compiles" — it is four scenarios
//! working end to end **on a demo project built from real material**:
//!
//! 1. **Vocal** — pitched and unpitched editing, with Gating,
//!    Compression, Sibilance and De-Breathing as four separate
//!    envelopes composited into the item's one volume envelope.
//! 2. **Drums** — multitracked drums quantized to the grid.
//! 3. **MPE** — per-note bend, pressure and timbre.
//! 4. **Guitar** — a Guitar Pro file as a six-string roll with bend
//!    flow.
//!
//! Every one of those has component tests already. What did not exist
//! is a *project* that holds all four at once, which is the thing that
//! catches the integration bugs unit tests cannot: a vocal take and a
//! drum multitrack at two different sample rates in one document, a
//! guitar roll whose row space is not the drum roll's, editor state
//! that has to survive a save with all of it loaded.
//!
//! ## Nothing here is committed
//!
//! The material is ~20 GB of worship stems, Pro Tools sessions and a
//! Guitar Pro transcription that live on one machine. This crate
//! *references* them by absolute path and assembles a `.daw` document
//! around them. See [`material`] for how a machine without them skips.
//!
//! The one exception is MPE, which the inventory found nowhere on the
//! machine and #177 resolved by synthesizing: that track is generated,
//! not referenced.

pub mod material;
pub mod project;

pub use material::{Material, Song};
pub use project::{Demo, DemoTrack, TrackRole, build};
