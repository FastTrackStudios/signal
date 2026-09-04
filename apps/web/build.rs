//! Compile `docs/guides/signal/*.md` into the site.
//!
//! The guide is a **vault**: the files under `docs/guides/signal/` are
//! notes with frontmatter, and `ssg-build` turns them into finished HTML
//! at build time. What used to be here — a frontmatter reader, a
//! wikilink pass and a markdown render, all of it a slightly different
//! copy of what Keyflow and Ignition had — now lives once, in Task's
//! `features/ssg`, and every FTS site calls it.
//!
//! Reading outside the crate is what a build script is for: `include_str!`
//! across that boundary would be invisible to cargo, and the
//! `cargo:rerun-if-changed` lines `emit` prints are what make editing a
//! guide page rebuild the site.

fn main() {
    ssg_build::Vault::at("../../docs/guides/signal")
        .link_base("/guide")
        .emit();
}
