//! The guide, as a vault.
//!
//! `docs/guides/session/*.md` are markdown notes. `build.rs` hands them
//! to `ssg-build`, which renders them on the host and codegens the page
//! table included below; [`crate::routes::GuidePage`] renders one.
//!
//! The guide's routes are **pre-rendered**: `dx build --ssg` writes each
//! out as a finished `index.html`, so the documentation arrives as text
//! rather than as a program that produces text. The bundle then hydrates
//! the page into the ordinary app.
//!
//! This is the same vault machinery Keyflow, Ignition and Signal publish
//! their guides with — one crate, four sites.

// `pub static VAULT: ssg::StaticVault`, from `build.rs`.
ssg::include_vault!();

/// Where the guide is published, as a URL prefix.
///
/// `build.rs` resolves `[[wikilinks]]` against this and
/// `crate::static_routes` enumerates the pages under it for the
/// pre-render — the two have to agree.
pub const BASE: &str = "/guide";
