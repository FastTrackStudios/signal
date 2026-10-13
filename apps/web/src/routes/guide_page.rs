//! The guide: an index of the vault, and one screen per note.
//!
//! These are the routes `dx build --ssg` pre-renders, so they are built
//! out of `ssg-ui`'s components — every one a pure function of
//! `&'static` data, with no signals, effects or handlers. Hydration
//! requires the client's first render to match the server's exactly, and
//! a component with no state cannot disagree with itself.
//!
//! Styling is Tailwind for the page frame, and `ssg-ui`'s own sheet for
//! the prose and the navigation bits (see `crate::App`, which links it).
//! The `ssg-*` classes are a default to build on: bind the custom
//! properties in `assets/tailwind.css` to change them.

use dioxus::prelude::*;
use ssg_ui::{Backlinks, ChapterNav, VaultArticle, VaultToc};

use crate::Route;
use crate::guide::{BASE, VAULT};

/// The page frame the guide screens share.
#[component]
fn GuideShell(children: Element) -> Element {
    rsx! {
        div { class: "min-h-screen bg-zinc-950 text-zinc-100",
            header { class: "border-b border-zinc-800",
                nav { class: "mx-auto flex max-w-5xl items-center gap-6 px-6 py-4",
                    Link { to: Route::Home {}, class: "font-semibold tracking-tight", "Session" }
                    Link { to: Route::Demo {}, class: "text-sm text-zinc-400 hover:text-zinc-100", "Demo" }
                    a { href: BASE, class: "text-sm text-zinc-400 hover:text-zinc-100", "Guide" }
                }
            }
            main { class: "mx-auto max-w-5xl px-6 py-10", {children} }
        }
    }
}

/// `/guide` — the table of contents.
#[component]
pub fn GuideIndex() -> Element {
    rsx! {
        GuideShell {
            h1 { class: "text-3xl font-bold tracking-tight", "Guide" }
            p { class: "mt-2 text-zinc-400", "How Session is put together." }
            ul { class: "mt-8 divide-y divide-zinc-800",
                for page in VAULT.pages {
                    li { key: "{page.slug}", class: "py-3",
                        // A plain anchor rather than a router `Link`: it
                        // costs a page load, every target is
                        // pre-rendered so that load is cheap, and it
                        // works before the bundle arrives.
                        a { href: "{BASE}/{page.slug}", class: "font-medium hover:text-sky-400",
                            "{page.title}"
                        }
                        if !page.summary.is_empty() {
                            span { class: "text-zinc-500", " — {page.summary}" }
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
            GuideShell {
                h1 { class: "text-2xl font-semibold", "No such guide page" }
                a { href: BASE, class: "mt-4 inline-block text-sky-400", "Back to the guide" }
            }
        };
    };

    rsx! {
        GuideShell {
            div { class: "grid grid-cols-1 gap-10 md:grid-cols-[12rem_minmax(0,1fr)]",
                VaultToc {
                    vault: VAULT,
                    current: page.slug,
                    base: BASE,
                    class: "ssg-toc md:sticky md:top-8 md:self-start",
                }
                div { class: "min-w-0",
                    a { href: BASE, class: "text-sm text-zinc-400 hover:text-zinc-100", "← Guide" }
                    VaultArticle { page, class: "mt-4" }
                    ChapterNav { vault: VAULT, current: page.slug, base: BASE }
                    Backlinks { vault: VAULT, current: page.slug, base: BASE }
                }
            }
        }
    }
}
