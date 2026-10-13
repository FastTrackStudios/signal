//! Whichever backend is actually running — Live Mode's in-process player
//! ([`crate::session_engine`]) or Recording Mode's live REAPER connection
//! ([`crate::reaper_engine`]) — reduced to the one thing the performance
//! view needs from either: a `SetlistService` client pair.
//!
//! Everything else session-desktop does with an engine (Live's
//! "Load & Play", the keyflow toolbar's direct `.standalone` dispatch) is
//! Live-Mode-only and keeps calling `session_engine::engine()` directly —
//! those actions have no meaning against real REAPER, which owns that
//! state itself.

use session::SetlistServiceClient;
use session::services::setlist_service::SetlistServiceStreamClient;

pub struct ActiveClients {
    pub client: SetlistServiceClient,
    pub stream_client: SetlistServiceStreamClient,
}

/// The running engine's client pair, whichever mode started it. `None`
/// before boot has finished, or if neither engine came up (REAPER
/// unreachable in Recording Mode, or the standalone engine failed to
/// start in Live Mode).
pub fn current() -> Option<ActiveClients> {
    if let Some(engine) = crate::reaper_engine::engine() {
        return Some(ActiveClients {
            client: engine.client.clone(),
            stream_client: engine.stream_client.clone(),
        });
    }
    let engine = crate::session_engine::engine()?;
    Some(ActiveClients {
        client: engine.client.clone(),
        stream_client: engine.stream_client.clone(),
    })
}
