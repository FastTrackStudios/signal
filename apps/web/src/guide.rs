//! The guide, as a vault.
//!
//! `docs/guides/signal/*.md` are notes with frontmatter. `build.rs` hands
//! them to `ssg-build`, which renders them to HTML on the host and
//! codegens the table this module includes; [`crate::routes::GuidePage`]
//! renders one.
//!
//! The guide pages are also *baked*: `src/bin/bake.rs` writes each one to
//! `guide/<slug>/index.html` with no scripts in it, so a reader gets the
//! text on the first paint and the site's wasm bundle is never on the
//! critical path for reading the documentation. See that binary, and
//! `ssg-bake`, for how.

ssg::include_vault!();
