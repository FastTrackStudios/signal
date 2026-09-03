//! The landing page: a hero, then one stripe per rig.
//!
//! Each stripe pairs a claim with a moving picture of the interface that
//! backs it. The demos are CSS and SVG only — see [`crate::demos`] — so
//! the page has no engine to boot and nothing to fail. The button beside
//! each one is the escape hatch to the real thing at `/rigs/<slug>`.
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
                    "Your whole rig, "
                    span { class: "sg-accent", "headless" }
                    "."
                }
                p { class: "sg-lede",
                    "Signal runs the guitar, keys and drum rigs as one engine with no
                     window attached. Every interface — desktop, tablet, browser, the
                     plugin — is a remote onto the same running rig."
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
                headline: "Amps and pedals that are actually the amp",
                body: "Neural amp models, impulse responses and the pedalboard in front
                       of them, running at the buffer size your interface asked for.
                       Switch the whole board on a footswitch without a click.",
                demo: rsx! { GuitarDemo {} },
            }

            Stripe {
                rig: Rig::Keys,
                headline: "Sampled instruments that load while you play",
                body: "Multi-mic, round-robin sample libraries with velocity layers and
                       real legato. Layers and splits are a mixer, not a patch you have
                       to rebuild — move a fader and the change is live.",
                demo: rsx! { KeysDemo {} },
            }

            Stripe {
                rig: Rig::Drums,
                headline: "One kit, every mic, one fader each",
                body: "Trigger detection per pad, round-robins deep enough that repeats
                       do not sound like repeats, and the bleed between mics kept rather
                       than gated away.",
                demo: rsx! { DrumsDemo {} },
            }

            Stripe {
                rig: Rig::Vocal,
                headline: "Tracking chain, pitch, and the harmony stack",
                body: "The vocal rig is next: the tracking chain, tuning, and stacked
                       harmonies driven from the same engine the other rigs run on.",
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
fn Stripe(rig: Rig, headline: String, body: String, demo: Element) -> Element {
    let reversed = rig.index() % 2 == 1;
    let available = rig.available();

    rsx! {
        section {
            class: if reversed { "sg-stripe sg-stripe-alt" } else { "sg-stripe" },
            div { class: "sg-stripe-inner",
                div { class: "sg-stripe-copy",
                    span { class: "sg-eyebrow", "{rig.name()} Rig" }
                    h2 { "{headline}" }
                    p { "{body}" }
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
