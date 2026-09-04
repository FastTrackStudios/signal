//! The guide: an index of the vault, and one screen per note.
//!
//! These two screens are the ones that get **baked** — rendered to
//! `index.html` files by `src/bin/bake.rs`, so the prose is served as
//! plain HTML rather than produced by the wasm bundle. So they are built
//! out of `ssg-ui`'s components,
//! every one of which is a pure function of `&'static` data: no signals,
//! no effects, no handlers, and navigation by ordinary `<a href>`.
//!
//! They still render inside the SPA — `dx serve` shows them, and the
//! router resolves `/guide` if a reader arrives there without a baked
//! file to serve. It is the same markup either way; the baked page is
//! just the one that does not need a program to produce it.

use dioxus::prelude::*;
use ssg_ui::{Backlinks, ChapterNav, VaultArticle, VaultToc};

use crate::guide::VAULT;
use crate::routes::Shell;

/// Where the vault is published. One place, because the build script
/// resolves `[[wikilinks]]` against it and the baker writes the files
/// under it — the three have to agree.
pub const BASE: &str = "/guide";

/// `/guide` — the table of contents.
#[component]
pub fn GuideIndex() -> Element {
    rsx! {
        Shell {
            section { class: "sg-prose",
                h1 { "Guide" }
                p { class: "sg-lede-sm", "How Signal is put together, and how to run it." }
                ul { class: "sg-toc",
                    for page in VAULT.pages {
                        li { key: "{page.slug}",
                            // A plain anchor, not a router `Link`: the
                            // target is a baked file, and a full page
                            // load is what fetches it. A `Link` would
                            // client-render the same page and skip it.
                            a { href: "{BASE}/{page.slug}", "{page.title}" }
                            if !page.summary.is_empty() {
                                span { class: "sg-toc-summary", " — {page.summary}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// `/guide/:slug` — one note.
#[component]
pub fn GuidePage(slug: String) -> Element {
    let Some(page) = VAULT.page(&slug) else {
        return rsx! {
            super::NotFound { segments: vec!["guide".to_string(), slug] }
        };
    };

    rsx! {
        Shell {
            div { class: "sg-guide",
                VaultToc { vault: VAULT, current: page.slug, base: BASE, class: "sg-guide-toc ssg-toc" }
                div { class: "sg-guide-page",
                    a { href: BASE, class: "sg-back", "← Guide" }
                    VaultArticle { page, class: "sg-prose" }
                    ChapterNav { vault: VAULT, current: page.slug, base: BASE }
                    Backlinks { vault: VAULT, current: page.slug, base: BASE }
                }
            }
        }
    }
}
