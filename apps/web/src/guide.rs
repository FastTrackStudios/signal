//! The guide, as a vault.
//!
//! `docs/guides/signal/*.md` are notes with frontmatter. `build.rs`
//! compiles them in verbatim; this module gives them an order and a
//! lookup, and [`crate::routes::GuidePage`] renders them.
//!
//! The markdown is kept as source rather than pre-rendered to HTML at
//! build time. keyflow-web renders its vault through the real editor in
//! read-only mode, which is what gives it `[[wikilink]]` navigation and
//! live fences; this guide is a plain pulldown-cmark pass for now, and
//! keeping the source is what leaves that door open.

use pulldown_cmark::{Options, Parser, html};

/// One page of the guide.
#[derive(PartialEq, Eq)]
pub struct GuidePage {
    /// URL segment, from the filename.
    pub slug: &'static str,
    /// Display title, from the frontmatter.
    pub title: &'static str,
    /// Sort key, from the frontmatter. Pages without one sort last.
    pub order: u32,
    /// One line for the index, from the frontmatter.
    pub summary: &'static str,
    /// The note without its frontmatter.
    pub body: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/guide_generated.rs"));

/// Look up a page by its URL slug.
#[must_use]
pub fn page(slug: &str) -> Option<&'static GuidePage> {
    GUIDE_PAGES.iter().find(|p| p.slug == slug)
}

/// Render a note to HTML.
///
/// Tables, footnotes and strikethrough are on because guide prose uses
/// them; nothing here is user input — every byte was compiled in from the
/// repo — so the unsanitised HTML pulldown-cmark emits is the repo's own.
#[must_use]
pub fn render(markdown: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_SMART_PUNCTUATION);

    let parser = Parser::new_ext(markdown, options);
    let mut out = String::with_capacity(markdown.len().saturating_mul(2));
    html::push_html(&mut out, parser);
    out
}
