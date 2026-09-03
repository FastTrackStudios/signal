//! `/rigs/<slug>` — where the real, playable rig goes.
//!
//! # These pages are stubs, and saying so is the point
//!
//! The landing page's demos are CSS mocks with no engine behind them.
//! These pages are the opposite promise: the actual rig, running in the
//! browser, playable. That is not built yet, and a stub that *looks*
//! finished would be worse than one that says what is missing.
//!
//! What has to land before a rig page is real:
//!
//! - **Sample streaming over iroh.** The keys and drum rigs are sample
//!   libraries measured in gigabytes; they cannot be an asset the bundle
//!   ships. The engine already streams packs from disk, so the browser
//!   needs the same reader over an iroh blob fetch — content-addressed, so
//!   a pack is cached by hash and a second visit is free.
//! - **NAM model streaming**, same mechanism, much smaller: the guitar rig
//!   is DSP the wasm build already contains, and the only thing it fetches
//!   is the amp model itself. That makes guitar the right first target —
//!   it is the one rig that is nearly self-contained.
//! - **The rig UI crates on wasm.** `signal-guitar-ui`, `signal-keys-ui`
//!   and `signal-drums-ui` render through Blitz for the desktop and plugin
//!   builds. The browser remote is the third context they are supposed to
//!   serve, which is the whole point of the detachable-GUI rule in
//!   CLAUDE.md.
//!
//! Until then each page states the rig, what it will stream, and links
//! back to the demo on the landing page.

use dioxus::prelude::*;

use crate::Route;
use crate::routes::Shell;

/// The rigs the site knows about.
///
/// A slug that does not parse renders [`super::NotFound`] rather than an
/// empty page, so a typo in a link is visible rather than silent.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Rig {
    Guitar,
    Keys,
    Drums,
    Vocal,
}

impl Rig {
    /// Parse a URL segment.
    #[must_use]
    pub fn from_slug(slug: &str) -> Option<Self> {
        match slug {
            "guitar" => Some(Self::Guitar),
            "keys" => Some(Self::Keys),
            "drums" => Some(Self::Drums),
            "vocal" => Some(Self::Vocal),
            _ => None,
        }
    }

    /// The URL segment.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Guitar => "guitar",
            Self::Keys => "keys",
            Self::Drums => "drums",
            Self::Vocal => "vocal",
        }
    }

    /// Display name, without the word "Rig".
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Guitar => "Guitar",
            Self::Keys => "Keys",
            Self::Drums => "Drums",
            Self::Vocal => "Vocal",
        }
    }

    /// Position on the landing page, which drives the left/right
    /// alternation of the stripes.
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Guitar => 0,
            Self::Keys => 1,
            Self::Drums => 2,
            Self::Vocal => 3,
        }
    }

    /// Whether there is a rig page to link to at all.
    #[must_use]
    pub const fn available(self) -> bool {
        !matches!(self, Self::Vocal)
    }

    /// What this rig has to pull down before it can make a sound, which is
    /// the thing the stub page is waiting on.
    #[must_use]
    pub const fn streams(self) -> &'static str {
        match self {
            Self::Guitar => {
                "NAM amp models and impulse responses. Everything else is DSP the \
                 wasm bundle already carries, which is why this rig is the first \
                 one that will run here."
            }
            Self::Keys => {
                "Multi-mic sample packs with velocity layers and round-robins — \
                 gigabytes, streamed on demand rather than shipped."
            }
            Self::Drums => {
                "The kit's mic set and its round-robins, streamed per pad so a \
                 kit is playable before all of it has arrived."
            }
            Self::Vocal => "Not yet decided — the vocal rig is still being designed.",
        }
    }
}

#[component]
pub fn RigDemo(rig: String) -> Element {
    let Some(rig) = Rig::from_slug(&rig) else {
        return rsx! {
            super::NotFound { segments: vec!["rigs".to_string(), rig] }
        };
    };

    rsx! {
        Shell {
            section { class: "sg-prose",
                span { class: "sg-eyebrow", "{rig.name()} Rig" }
                h1 { "Live demo" }
                p { class: "sg-callout",
                    strong { "Not wired up yet." }
                    " The interface on the landing page is a mock with no engine
                      behind it. This page is where the real rig will run, and it
                      does not run yet."
                }
                h2 { "What this rig streams" }
                p { "{rig.streams()}" }
                h2 { "What has to land first" }
                ul {
                    li { "Sample and model streaming over iroh, content-addressed so a
                          second visit is served from cache." }
                    li { "The rig UI crate building for wasm — the browser is the third
                          context the detachable-GUI rule already requires it to serve." }
                    li { "An audio worklet host for the graph, at a buffer size the
                          browser can actually hold." }
                }
                div { class: "sg-hero-actions",
                    Link { to: Route::Home {}, class: "sg-btn sg-btn-primary",
                        "Back to the demos"
                    }
                    Link { to: Route::GuideIndex {}, class: "sg-btn", "Read the guide" }
                }
            }
        }
    }
}
