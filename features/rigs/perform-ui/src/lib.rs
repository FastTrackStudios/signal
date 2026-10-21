//! The performance screens every rig shares: the sidebar's Preset, Profile
//! and Setlist (`sidebar_views`, `setlist`), the song and set panels
//! (`panels`), and the menus they open (`menu`) — on the shared performance
//! surface, so the guitar, the bass and the drums play from the same
//! screens.
//!
//! The host puts the rig's [`PerformClients`] (scoped to that rig) in
//! context and calls [`use_perform`]; the screens take the [`PerfState`]
//! it returns. Each rig page provides its own, so a page never plays
//! another rig's surface by inheriting it from above.

pub mod colors;
pub mod marks;
pub mod menu;
pub mod panels;
pub mod setlist;
pub mod sidebar_views;

/// The frame's tokens, where the screens look for them.
pub(crate) mod tokens {
    pub use signal_rig_shell::tokens::*;
}

use dioxus::prelude::*;
use signal_rigs_proto::perform::PerformState;
pub use signal_rigs_proto::perform::service::{
    PerformEvent, RigPerformClient, RigPerformStreamClient,
};

/// A rig's performance clients, scoped to it. `None`s where the rig has no
/// engine (the screens show their empty states).
#[derive(Clone, Default)]
pub struct PerformClients {
    pub client: Option<RigPerformClient>,
    pub stream: Option<RigPerformStreamClient>,
}

/// The rig's performance client, from the [`PerformClients`] in context.
#[must_use]
pub fn perform_client() -> Option<RigPerformClient> {
    try_consume_context::<PerformClients>().and_then(|c| c.client)
}

/// The performance state the screens draw from: the rig's, kept by
/// [`use_perform`].
#[derive(Clone, Copy, PartialEq)]
pub struct PerfState {
    pub perf: Signal<PerformState>,
}

/// The rig's performance state, fetched from the client in context and
/// kept by its stream — and put in context for the screens below.
pub fn use_perform() -> PerfState {
    let client = use_hook(perform_client);
    let stream = use_hook(|| try_consume_context::<PerformClients>().and_then(|c| c.stream));
    let mut perf = use_signal(PerformState::default);
    use_future(move || {
        let client = client.clone();
        async move {
            if let Some(c) = client
                && let Ok(s) = c.state().await
            {
                perf.set(s);
            }
        }
    });
    architect::use_stream(
        move |sink| {
            let stream = stream.clone();
            async move {
                match stream {
                    Some(s) => s.events(sink).await.is_ok(),
                    None => false,
                }
            }
        },
        move |ev: PerformEvent| {
            let PerformEvent::State(s) = ev;
            let mut perf = perf;
            perf.set(s);
        },
    );
    use_context_provider(|| PerfState { perf })
}

/// Where the rig's browser goes for a selection: its kind (`block:delay`,
/// `module:Amp`, `profiles`) and, inside it, the preset (and variation) in
/// use.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Focus {
    pub kind: String,
    pub preset: String,
    pub variation: String,
    /// Open on the search, the keyboard up.
    pub search: bool,
}

/// The browser's focus — a screen here sets it, the rig's browser follows.
#[derive(Clone, Copy)]
pub struct BrowserFocus(pub Signal<Option<Focus>>);

/// The part picked in Build (Setlist mode): what the browser builds into.
/// Shared by the setlist (whose section rows pick it) and the browser.
#[derive(Clone, Copy, PartialEq)]
pub struct BuildPick {
    pub part: Signal<Option<usize>>,
    /// Whether a section row's tap picks (Build) or plays (Perform).
    pub building: Signal<bool>,
}
