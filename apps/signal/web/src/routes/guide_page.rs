//! The guide: an index of the vault, and one screen per note.
//!
//! These two screens are the ones `dx build --ssg` **pre-renders** —
//! written out as finished `index.html` files, so the prose is in the
//! document rather than produced by the wasm bundle. The bundle then
//! hydrates them into the ordinary app.
//!
//! That is why they are built out of `ssg-ui`'s components, every one of
//! which is a pure function of `&'static` data: no signals, no effects,
//! no handlers. Hydration requires the client's first render to match
//! the server's exactly, and a component with no state cannot disagree
//! with itself.
//!
//! They render inside the SPA too — `dx serve` shows them, and the
//! router resolves `/guide` for a reader who arrives client-side. It is
//! the same markup either way.

use dioxus::prelude::*;
use ssg_ui::{Backlinks, ChapterNav, VaultArticle, VaultToc};

use crate::guide::VAULT;
use crate::routes::Shell;

/// Where the vault is published. One place, because the build script
/// resolves `[[wikilinks]]` against it and `static_routes` enumerates
/// the pages under it — the two have to agree.
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
                            // A plain anchor rather than a router
                            // `Link`. It costs a page load between
                            // chapters instead of a client-side
                            // transition — and every one of those pages
                            // is pre-rendered, so it is a cheap one.
                            // It also works before the bundle arrives.
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
