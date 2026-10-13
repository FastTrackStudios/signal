//! Getting hold of the track a control is about.
//!
//! Every write in this module family walks the same four steps — is there a
//! DAW, what is the current project, find the track by guid, act on it — and
//! each step has its own error. Written out per control it is five lines of
//! ceremony around one line of intent, repeated once per control, and each
//! copy is a chance to report a different thing when the connection is
//! missing.
//!
//! So it is written once. A control says what it wants done to its track and
//! nothing else:
//!
//! ```ignore
//! on_track(&guid, |t| async move { t.toggle_solo().await }).await
//! ```

use crate::prelude::*;

/// Run `f` against a track handle, or explain why there isn't one.
///
/// The errors are deliberately plain strings of context rather than a typed
/// error: nothing branches on *why* a write failed — there is nothing a
/// control could do differently — and every caller ends at the same
/// `tracing::warn!`.
pub async fn on_track<F, Fut>(guid: &str, f: F) -> eyre::Result<()>
where
    F: FnOnce(daw_control::TrackHandle) -> Fut,
    Fut: std::future::Future<Output = Result<(), daw_control::Error>>,
{
    let daw = daw_control::Daw::try_get().ok_or_else(|| eyre::eyre!("no DAW connected"))?;
    let project = daw.current_project().await?;
    let track = project
        .tracks()
        .by_guid(guid)
        .await?
        .ok_or_else(|| eyre::eyre!("no track {guid}"))?;
    f(track).await?;
    Ok(())
}

/// Wait for a DAW connection AND a current project, then hand it back.
///
/// The two long-lived subscriptions — the track stream and the meter feed —
/// both start this way, and both used to carry their own copy of the loop.
/// The delay is `futures_timer`, not `tokio::time` and not
/// `architect::platform::sleep` (which is `tokio::time` on native). A Dioxus
/// panel's futures run on dioxus's own scheduler with no tokio runtime
/// behind them, so a tokio timer aborts the host process the moment the
/// panel opens — a non-unwinding panic, taking REAPER with it.
///
/// Never gives up early: a host can be connected with nothing open yet (a
/// standalone player before its first setlist loads), so "no current
/// project" is retried on the same cadence as "no DAW at all" rather than
/// resolving to `None` — a caller that gave up here used to get stuck
/// showing "no connection" forever even after a project *did* open, since
/// nothing re-polled it.
pub async fn connected_project() -> Option<daw_control::Project> {
    loop {
        if let Some(daw) = daw_control::Daw::try_get() {
            match daw.current_project().await {
                Ok(p) => return Some(p),
                Err(e) => tracing::debug!("no current project yet: {e:?}"),
            }
        }
        futures_timer::Delay::new(std::time::Duration::from_millis(500)).await;
    }
}

/// Fire `f` at the DAW and log whatever it says.
///
/// The shape every click handler wants: a control has already updated what
/// it draws (or is waiting on the event that confirms the write), so there
/// is nothing to await and nothing to do with a failure but say so.
pub fn write_track<F, Fut>(guid: String, what: &'static str, f: F)
where
    F: FnOnce(daw_control::TrackHandle) -> Fut + 'static,
    Fut: std::future::Future<Output = Result<(), daw_control::Error>> + 'static,
{
    spawn(async move {
        if let Err(e) = on_track(&guid, f).await {
            tracing::warn!("{what} {guid}: {e}");
        }
    });
}
