//! The guide, as a vault.
//!
//! `docs/guides/signal/*.md` are notes with frontmatter. `build.rs` hands
//! them to `ssg-build`, which renders them to HTML on the host and
//! codegens the table this module includes; [`crate::routes::GuidePage`]
//! renders one.
//!
//! The guide's routes are also **pre-rendered**: `dx build --ssg` writes
//! each one out as a finished `index.html`, so a reader gets the text on
//! the first paint rather than waiting for the wasm bundle to produce
//! it. The bundle still loads, and hydrates the page into the ordinary
//! app; what it no longer does is gate the words on its own arrival.
//! See `main.rs` for the wiring.

ssg::include_vault!();
