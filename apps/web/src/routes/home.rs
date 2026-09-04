//! The landing page: a hero, then one stripe per rig.
//!
//! The copy is deliberately almost nothing — a rig name and a button. The
//! demo beside it is the argument, and better words can be written later
//! against a page that already shows the product.
//!
//! Guitar and keys render the REAL control surfaces from
//! `signal-guitar-ui` and `signal-keys-ui`, fed as a live rig would feed
//! them; drums is still a CSS mock. See [`crate::demos`]. The button is
//! the escape hatch to the playable thing at `/rigs/<slug>`.
//!
//! Vocal is on the page without a demo on purpose. Leaving it off would
//! be tidier and would also hide the roadmap; a stripe that says "coming
//! soon" is honest, and a fake demo of something that does not exist is
//! not.

use dioxus::prelude::*;

use crate::Route;
use crate::demos::{DrumsDemo, GuitarDemo, KeysDemo};
use crate::routes::Shell;
use crate::routes::rig::Rig;

#[component]
pub fn Home() -> Element {
    rsx! {
        Shell {
            section { class: "sg-hero",
                h1 { class: "sg-display",
                    "Synth, Sampler, Processor, "
                    span { class: "sg-accent", "Live Rig" }
                    "."
                }
                div { class: "sg-hero-actions",
                    Link { to: Route::RigDemo { rig: Rig::Guitar.slug().to_string() },
                        class: "sg-btn sg-btn-primary",
                        "Try a rig"
                    }
                    Link { to: Route::GuideIndex {}, class: "sg-btn", "Read the guide" }
                }
            }

            Stripe {
                rig: Rig::Guitar,
                demo: rsx! { GuitarDemo {} },
            }

            Stripe {
                rig: Rig::Keys,
                demo: rsx! { KeysDemo {} },
            }

            Stripe {
                rig: Rig::Drums,
                demo: rsx! { DrumsDemo {} },
            }

            Stripe {
                rig: Rig::Vocal,
                demo: rsx! {
                    div { class: "sg-demo sg-demo-soon",
                        span { class: "sg-soon-badge", "Coming soon" }
                    }
                },
            }
        }
    }
}

/// One rig stripe: copy on one side, a live demo on the other.
///
/// `reversed` alternates the sides down the page so the eye has something
/// to follow; it is derived from the rig rather than passed, so the
/// alternation cannot drift out of step when a stripe is added.
#[component]
fn Stripe(rig: Rig, demo: Element) -> Element {
    let reversed = rig.index() % 2 == 1;
    let available = rig.available();

    rsx! {
        section {
            class: if reversed { "sg-stripe sg-stripe-alt" } else { "sg-stripe" },
            div { class: "sg-stripe-inner",
                div { class: "sg-stripe-copy",
                    h2 { "{rig.name()} Rig" }
                    if available {
                        Link {
                            to: Route::RigDemo { rig: rig.slug().to_string() },
                            class: "sg-btn sg-btn-primary",
                            "Live demo"
                        }
                    } else {
                        span { class: "sg-btn sg-btn-disabled", "Live demo" }
                    }
                }
                div { class: "sg-stripe-demo", {demo} }
            }
        }
    }
}
