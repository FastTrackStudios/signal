//! The site's screens.

pub mod guide_page;
mod home;
pub mod rig;

pub use guide_page::{GuideIndex, GuidePage};
pub use home::Home;
pub use rig::RigDemo;

use dioxus::prelude::*;

use crate::Route;

/// Shared chrome: the header every screen sits under.
#[component]
pub fn Shell(children: Element) -> Element {
    rsx! {
        div { class: "sg-shell",
            header { class: "sg-header",
                Link { to: Route::Home {}, class: "sg-wordmark",
                    span { class: "sg-wordmark-mark" }
                    "Signal"
                }
                nav { class: "sg-nav",
                    Link { to: Route::GuideIndex {}, "Guide" }
                    a {
                        href: "https://github.com/FastTrackStudios/signal",
                        rel: "noreferrer",
                        "Source"
                    }
                }
            }
            main { class: "sg-main", {children} }
            footer { class: "sg-footer",
                span { "FastTrackStudio" }
                span { class: "sg-footer-sep", "·" }
                span { "GPL-3.0-or-later" }
            }
        }
    }
}

/// Anything the router could not match.
#[component]
pub fn NotFound(segments: Vec<String>) -> Element {
    rsx! {
        Shell {
            section { class: "sg-prose",
                h1 { "Not found" }
                p { "There is no page at /{segments.join(\"/\")}." }
                Link { to: Route::Home {}, "Back to the start" }
            }
        }
    }
}
