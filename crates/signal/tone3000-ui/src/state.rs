//! The browser's live state, and the two things a host shell must provide.
//!
//! Everything here is driven by the generated vox clients taken from Dioxus
//! context, so the same component tree mounts in the browser remote, the
//! desktop app and a plugin editor without a line of per-shell code. What a
//! shell *does* have to supply is the one capability that genuinely differs
//! between them: how to open a web page.

use std::collections::HashMap;
use std::rc::Rc;

use dioxus::prelude::*;
use signal_tone3000_proto::tone3000::{Tone3000Client, Tone3000StreamClient};
use signal_tone3000_proto::{DownloadProgress, SignInStatus};

/// How this shell opens a URL in the user's own browser.
///
/// The authorization page cannot be rendered in-process — Blitz is a
/// renderer, not a browser — so signing in always means handing a URL to
/// something else. In the browser remote that is `window.open`; on the
/// desktop it is the platform opener; inside a DAW it is whatever the host
/// allows. The component tree does not care, and must not: it only knows
/// that a URL needs to leave.
#[derive(Clone)]
pub struct UrlOpener(Rc<dyn Fn(String)>);

impl UrlOpener {
    /// Wrap a shell's opener.
    pub fn new(open: impl Fn(String) + 'static) -> Self {
        Self(Rc::new(open))
    }

    /// Open a URL. A no-op if the shell gave us nothing better.
    pub fn open(&self, url: impl Into<String>) {
        (self.0)(url.into());
    }
}

impl PartialEq for UrlOpener {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// Live browser state: who is signed in, and what is downloading.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Tone3000State {
    /// The session, refreshed after every sign-in or sign-out.
    pub status: Signal<SignInStatus>,
    /// Progress by model id. Kept for finished downloads too, so a card can
    /// go on saying "in your library" rather than reverting to a button that
    /// would fetch the same bytes again.
    pub downloads: Signal<HashMap<String, DownloadProgress>>,
}

impl Tone3000State {
    /// Whether this model is being fetched right now.
    #[must_use]
    pub fn in_flight(&self, model_id: &str) -> bool {
        self.downloads.read().get(model_id).is_some_and(|p| !p.done)
    }

    /// The finished download for a model, if it landed successfully.
    #[must_use]
    pub fn completed(&self, model_id: &str) -> Option<DownloadProgress> {
        self.downloads
            .read()
            .get(model_id)
            .filter(|p| p.done && p.error.is_empty())
            .cloned()
    }

    /// Progress 0..=100 for a model being fetched, when the server told us
    /// how big it is. `None` means "working, length unknown" — which a UI
    /// must render as motion, not as 0%.
    #[must_use]
    pub fn percent(&self, model_id: &str) -> Option<u32> {
        self.downloads
            .read()
            .get(model_id)
            .filter(|p| !p.done && p.percent > 0)
            .map(|p| p.percent)
    }
}

/// Seed the session status and fold every [`DownloadProgress`] into state.
///
/// Absent clients leave the state at its defaults, so a shell whose engine is
/// not up renders a disconnected browser rather than failing to mount.
pub fn use_tone3000_state() -> Tone3000State {
    let client = use_hook(try_consume_context::<Tone3000Client>);
    let stream = use_hook(try_consume_context::<Tone3000StreamClient>);

    let mut status = use_signal(SignInStatus::default);
    let downloads = use_signal(HashMap::<String, DownloadProgress>::new);

    {
        let client = client;
        use_future(move || {
            let client = client.clone();
            async move {
                if let Some(client) = client
                    && let Ok(s) = client.status().await
                {
                    status.set(s);
                }
            }
        });
    }

    {
        let stream = stream;
        architect::use_stream(
            move |sink| {
                let stream = stream.clone();
                async move {
                    match stream {
                        Some(s) => s.downloads(sink).await.is_ok(),
                        None => false,
                    }
                }
            },
            move |progress: DownloadProgress| {
                // Signals are `Copy` handles; taking a mutable one here keeps
                // the fold closure `Fn`, which is what `use_stream` wants.
                let mut downloads = downloads;
                downloads
                    .write()
                    .insert(progress.model_id.clone(), progress);
            },
        );
    }

    Tone3000State { status, downloads }
}

/// Re-read the session from the engine — after a sign-in completes in the
/// user's browser, or after signing out.
pub fn refresh_status(client: Option<Tone3000Client>, mut status: Signal<SignInStatus>) {
    spawn(async move {
        if let Some(client) = client
            && let Ok(s) = client.status().await
        {
            status.set(s);
        }
    });
}
