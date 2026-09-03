//! The guide: an index of the vault, and one screen per note.

use dioxus::prelude::*;

use crate::Route;
use crate::guide;
use crate::routes::Shell;

/// `/guide` — the table of contents.
#[component]
pub fn GuideIndex() -> Element {
    rsx! {
        Shell {
            section { class: "sg-prose",
                h1 { "Guide" }
                p { class: "sg-lede-sm",
                    "How Signal is put together, and how to run it."
                }
                ul { class: "sg-toc",
                    for page in guide::GUIDE_PAGES {
                        li { key: "{page.slug}",
                            Link { to: Route::GuidePage { slug: page.slug.to_string() },
                                "{page.title}"
                            }
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
    let Some(page) = guide::page(&slug) else {
        return rsx! {
            super::NotFound { segments: vec!["guide".to_string(), slug] }
        };
    };

    // Compiled in from the repo at build time — see `guide::render`.
    let body = guide::render(page.body);

    rsx! {
        Shell {
            article { class: "sg-prose",
                Link { to: Route::GuideIndex {}, class: "sg-back", "← Guide" }
                div { dangerous_inner_html: "{body}" }
            }
        }
    }
}
