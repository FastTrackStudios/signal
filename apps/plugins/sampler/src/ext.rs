//! The host's track timeline (`com.fasttrackstudio.host-timeline/1`,
//! daw-clap-ext `docs/host-timeline.md`), the sampler's side: the plugin
//! reads its own track's whole MIDI and plays it ahead of the transport.
//!
//! - **Activate** (main thread): ask the host for `host-timeline/1` and
//!   `host-actions/1`, subscribe to `timeline`, read what's there.
//! - **`timeline` event** (`host-events/1`, main thread): read again.
//! - **A read** hands the timeline to an annotator thread, which builds
//!   the [`TrackDocument`](sampler::TrackDocument) and its schedule
//!   ([`SharedState::set_document`]); the audio thread adopts the schedule
//!   at its next block boundary.
//! - **The claim**: after the first timeline read, the plugin claims the
//!   track's playback, so the host stops sending the track's notes as
//!   events: what still arrives is live and plays beside the document
//!   (`sampler::Input::Live`). If the host can't, the plugin drops the
//!   events while the document plays.
//!
//! A host without the extension leaves all of this out: the plugin plays
//! live, or from a `.mid` / JSON document ([`crate::doc_watch`]).

use std::ffi::{CStr, c_char};
use std::sync::atomic::Ordering;
use std::sync::{Arc, PoisonError};

use daw_clap_ext::plugin::{HostActionsClient, HostTimelineClient};
use daw_clap_ext::sys::fts_plugin_host_events;
use nice_plug::wrapper::clap::{clap_extension_state, clap_host, clap_plugin};

use crate::plugin::{SharedState, SignalSamplerClap};

/// The host's extensions, as the plugin found them at activation.
#[derive(Clone, Copy)]
pub struct HostLink {
    pub timeline: Option<HostTimelineClient>,
    pub actions: Option<HostActionsClient>,
}

/// `host-events/1`: where the host's events arrive.
pub static HOST_EVENTS: fts_plugin_host_events = fts_plugin_host_events {
    on_event: Some(on_event),
};

/// `init()`: the instance's host. Main thread; nothing asked of it yet
/// (a host binds the timeline once the instance is on a track).
pub fn init(shared: &SharedState, host: *const clap_host) {
    shared.host.store(host.cast_mut(), Ordering::Release);
}

/// Activation (main thread): find the extensions, subscribe, read.
pub fn activate(shared: &Arc<SharedState>) {
    let host = shared.host.load(Ordering::Acquire);
    if host.is_null() {
        return;
    }
    // SAFETY: the host's own pointer, on its main thread (activate).
    let link = HostLink {
        timeline: unsafe { HostTimelineClient::query(host) },
        actions: unsafe { HostActionsClient::query(host) },
    };
    *shared.link.lock().unwrap_or_else(PoisonError::into_inner) = Some(link);
    let Some(timeline) = link.timeline else {
        tracing::info!(
            timeline.offered = false,
            "no host timeline: live, or a document file"
        );
        return;
    };
    let subscribed = link.actions.and_then(|a| a.subscribe("timeline")).is_some();
    tracing::info!(
        timeline.offered = true,
        timeline.subscribed = subscribed,
        timeline.revision = timeline.revision(),
        "host timeline offered"
    );
    read_timeline(shared);
}

/// Read the timeline if it moved since the last read, hand it to the
/// annotator, and claim the track's playback. Main thread.
pub fn read_timeline(shared: &Arc<SharedState>) {
    let Some(timeline) = link_of(shared).and_then(|l| l.timeline) else {
        return;
    };
    let revision = timeline.revision();
    if revision == 0 || revision == shared.timeline_read.load(Ordering::Acquire) {
        return;
    }
    let read = match timeline.read() {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(timeline.revision = revision, error = %e, "host timeline: no read");
            return;
        }
    };
    shared.timeline_read.store(read.revision, Ordering::Release);
    let notes = read.notes.len();
    let channels = read.channels().len();
    let doc = sampler::document_timeline::document_from_timeline(&read, crate::plugin::SEED);
    let to = Arc::clone(shared);
    let spawned = std::thread::Builder::new()
        .name("signal-sampler-annotate".into())
        .spawn(move || {
            if let Err(e) = to.set_document(doc) {
                tracing::warn!(error = %e, "document deferred");
            }
        });
    if let Err(e) = spawned {
        tracing::warn!(error = %e, "no annotator thread");
    }
    if !shared.claimed.load(Ordering::Acquire) {
        let claimed = timeline.set_playback_claimed(true);
        shared.claimed.store(claimed, Ordering::Release);
    }
    tracing::info!(
        timeline.revision = read.revision,
        timeline.notes = notes,
        timeline.channels = channels,
        timeline.claimed = shared.claimed.load(Ordering::Acquire),
        "host timeline read"
    );
}

/// Give the track's playback back (deactivation). Main thread.
pub fn release(shared: &SharedState) {
    if shared.claimed.swap(false, Ordering::AcqRel)
        && let Some(timeline) = link_of(shared).and_then(|l| l.timeline)
    {
        let _ = timeline.set_playback_claimed(false);
    }
}

fn link_of(shared: &SharedState) -> Option<HostLink> {
    *shared.link.lock().unwrap_or_else(PoisonError::into_inner)
}

/// # Safety
///
/// `plugin` is nice-plug's `clap_plugin` for [`SignalSamplerClap`].
unsafe fn shared_of(plugin: *const clap_plugin) -> Option<Arc<SharedState>> {
    let state = unsafe { clap_extension_state::<SignalSamplerClap>(plugin) }?;
    state.downcast::<SharedState>().ok()
}

/// `host-events/1`'s `on_event`: a `timeline` event reads again.
unsafe extern "C" fn on_event(
    plugin: *const clap_plugin,
    _subscription: u64,
    topic: *const c_char,
    _event_json: *const c_char,
) {
    // SAFETY: the host's call, with the plugin it queried.
    let Some(shared) = (unsafe { shared_of(plugin) }) else {
        return;
    };
    if topic.is_null() {
        return;
    }
    // SAFETY: the host's NUL-terminated topic.
    if unsafe { CStr::from_ptr(topic) }.to_bytes() == b"timeline" {
        read_timeline(&shared);
    }
}
