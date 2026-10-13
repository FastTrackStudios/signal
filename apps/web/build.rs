//! Compile `docs/guides/session/*.md` into the site.
//!
//! The guide is a **vault**: markdown notes with frontmatter and
//! `[[wikilink]]` cross-references. `ssg-build` reads them, renders them
//! to HTML on the host, and codegens the page table `src/guide.rs`
//! includes — so the site ships finished pages rather than a markdown
//! parser and a pile of source.
//!
//! Reading outside the crate is what a build script is for.
//! `include_str!` across that boundary would be invisible to cargo and
//! would fail at compile time rather than resolution time; the
//! `cargo:rerun-if-changed` lines `emit` prints are what make editing a
//! guide page rebuild the site.

fn main() {
    ssg_build::Vault::at("../../docs/guides/session")
        .link_base("/guide")
        .emit();
}
