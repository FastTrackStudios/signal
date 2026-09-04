//! The guide, as a vault.
//!
//! `docs/guides/signal/*.md` are notes with frontmatter. `build.rs` hands
//! them to `ssg-build`, which renders them to HTML on the host and
//! codegens the table this module includes; [`crate::routes::GuidePage`]
//! renders one.
//!
//! The guide pages are also *baked*: `src/bin/bake.rs` writes each one to
//! `guide/<slug>/index.html` with the prose already in the HTML, so a
//! reader gets the text on the first paint and the wasm bundle is never on
//! the critical path for reading the documentation. The bundle is still
//! referenced — an `async` module script, the same one the root page
//! loads — so the page hydrates into the SPA once it arrives; what it does
//! not do is gate the text on that arriving. See that binary, and
//! `ssg-bake`, for how.

ssg::include_vault!();
