//! The one place UI intent becomes engine state.
//!
//! Every control writes its in-flight value to [`Drafts`] and renders from
//! there; nothing else talks to the engine about volume. That is deliberate.
//! If each fader pushed its own writes, each would need its own throttle,
//! its own trailing-edge handling and its own idea of when the echo window
//! closes — and a strip of twelve would have twelve timers disagreeing.
//!
//! Here there is one timer, and the whole policy is three lines long.

use std::time::Duration;

use crate::controls::{Held, use_track_store};
use crate::prelude::*;

/// How often drafts are flushed to the engine.
///
/// 30Hz. A drag generates values as fast as the pointer reports, which is
/// invisible in-process and wasteful over a WebSocket; the user cannot see
/// the difference between 30 and 60 writes a second because the *rendering*
/// is not waiting on either — it is already showing the draft.
const FLUSH: Duration = Duration::from_millis(33);

/// Drain every held value to the DAW for as long as this is mounted.
///
/// Mount once, high up — beside
/// [`use_daw_tracks`][crate::controls::use_daw_tracks].
#[component]
pub fn ControlSync() -> Element {
    let store = use_track_store();

    use_future(move || async move {
        let mut drafts = store.drafts();
        loop {
            futures_timer::Delay::new(FLUSH).await;

            let dirty = drafts.take_dirty();
            if dirty.is_empty() {
                // A quiet pass, which is the only safe moment to retire:
                // every released draft has had its final value written, and
                // the echo of that write has had a tick to arrive and be
                // ignored. Retiring at release instead would open the
                // suppression window right when the echo is in flight.
                drafts.retire();
                continue;
            }
            for (guid, what, value) in dirty {
                if let Err(e) = write(&guid, what, value).await {
                    // Warn rather than retry: the next flush carries the
                    // newest value anyway, and a retry queue would replay
                    // stale positions after the user has moved on.
                    tracing::warn!("{what:?} {guid}: {e}");
                }
            }
        }
    });

    rsx! {}
}

async fn write(guid: &str, what: Held, value: f64) -> eyre::Result<()> {
    crate::controls::reach::on_track(guid, |t| async move {
        match what {
            Held::Volume => t.set_volume(value).await,
            Held::Pan => t.set_pan(value).await,
        }
    })
    .await
}
