//! Session workspace, browser flavor — the setlist player as a REMOTE.
//!
//! The wasm build carries no in-process engine: the setlist lives on
//! `fasttrackstudio --engine` (the network engine host, which embeds the
//! daw-standalone player and mounts SetlistService on the shared `/vox`
//! router). This view dials that engine exactly like the rig views do
//! (`crate::remote` — same-origin WebSocket by default, saved iroh id for
//! p2p), installs the `SetlistServiceClient` as session-ui's `Session`
//! singleton (transport buttons / sidebar seeks), and pumps the two
//! `#[subscribe]` streams (`events` + `active_indices`) into session-ui's
//! global signals — the network flavor of the desktop app's in-process
//! `SessionEventBridge`.
//!
//! Audio stays on the engine host; the browser only ever issues commands
//! and renders state (detachable-GUI rule). Deliberately, connecting never
//! seeks or otherwise moves the transport — a browser joining mid-show is
//! read-only until the user acts.

use dioxus::prelude::*;
use session_proto::services::setlist_service::SetlistServiceStreamClient;
use session_proto::{ActiveIndices, SetlistEvent, SetlistServiceClient};
use session_ui::{PerformanceLayout, PerformanceSidebar, TransportPanel};

use crate::remote::{EngineTarget, establish};

/// Connection state the workspace renders from. Bumps on every
/// connect/disconnect so views re-render.
static CONNECTED: GlobalSignal<bool> = Signal::global(|| false);

/// Invisible app-level component: owns the engine connection and folds the
/// setlist streams into session-ui's global signals. Mounted once in `App`
/// (via `SessionChrome`) so the connection and state survive workspace
/// switches. Reconnects forever: when a stream ends (engine restart,
/// network drop) it loops back to dialing.
#[component]
pub fn SessionRemoteBridge() -> Element {
    use_future(move || async move {
        loop {
            let target = EngineTarget::current();
            let Some(client) = establish::<SetlistServiceClient>(&target).await else {
                architect::platform::sleep(std::time::Duration::from_millis(1200)).await;
                continue;
            };
            let Some(stream) = establish::<SetlistServiceStreamClient>(&target).await else {
                architect::platform::sleep(std::time::Duration::from_millis(1200)).await;
                continue;
            };

            // Session singleton for session-ui components. On wasm `init`
            // replaces the previous client (reconnect support).
            if let Err(e) = session_ui::Session::init(client.clone()) {
                tracing::warn!("Session::init: {e:?}");
            }
            *CONNECTED.write() = true;
            tracing::info!("session remote connected ({})", target.label());

            // Initial snapshot: fetch the already-built setlist so the UI is
            // deterministic (no reliance on the stream's first republish).
            match client.setlist().await {
                Ok(setlist) => {
                    let songs: Vec<_> = setlist
                        .songs
                        .iter()
                        .map(|s| (s.id.clone(), s.name.clone()))
                        .collect();
                    session_ui::apply_setlist_event(&SetlistEvent::SetlistChanged(setlist));

                    // Chart backfill: charts are stripped from structural
                    // payloads and stream as `SongChartHydrated` deltas with
                    // no replay — a remote connecting after hydration ran
                    // would never see them. Fetch each song's chart via RPC
                    // and fold it in exactly like the streamed delta.
                    let chart_client = client.clone();
                    spawn(async move {
                        for (index, (song_id, name)) in songs.into_iter().enumerate() {
                            match chart_client.song_chart(index).await {
                                Ok(Some(chart)) => {
                                    session_ui::apply_setlist_event(
                                        &SetlistEvent::SongChartHydrated {
                                            song_id,
                                            index,
                                            chart,
                                        },
                                    );
                                }
                                Ok(None) => {
                                    tracing::debug!("no chart for song {index} ({name})")
                                }
                                Err(e) => {
                                    tracing::warn!("song_chart({index}) failed: {e:?}")
                                }
                            }
                        }
                    });
                }
                Err(e) => tracing::warn!("initial setlist snapshot failed: {e:?}"),
            }

            // Active-indices stream (the song/section cursor) — pumped in a
            // spawned task; it dies with its link when the connection drops.
            let (atx, mut arx) = vox::channel::<ActiveIndices>();
            let ai_stream = stream.clone();
            spawn(async move {
                if let Err(e) = ai_stream.active_indices(atx).await {
                    tracing::debug!("active_indices subscription ended: {e:?}");
                }
            });
            spawn(async move {
                while let Ok(Some(ai)) = arx.recv().await {
                    session_ui::apply_active_indices(ai.get());
                }
            });

            // Events stream (setlist structure + 60 Hz transport) — consumed
            // HERE so its end doubles as the disconnect signal.
            let (tx, mut rx) = vox::channel::<SetlistEvent>();
            let ev_stream = stream.clone();
            spawn(async move {
                if let Err(e) = ev_stream.events(tx).await {
                    tracing::debug!("events subscription ended: {e:?}");
                }
            });
            while let Ok(Some(ev)) = rx.recv().await {
                session_ui::apply_setlist_event(ev.get());
            }

            tracing::warn!("session engine connection lost — reconnecting");
            *CONNECTED.write() = false;
            architect::platform::sleep(std::time::Duration::from_millis(1200)).await;
        }
    });

    rsx! {}
}

/// The Session workspace in the browser: Navigator sidebar (left),
/// performance display (center), transport control bar (bottom) — the same
/// session-ui panels the desktop assembles, fed by the global signals
/// `SessionRemoteBridge` keeps up to date over the network.
#[component]
pub fn SessionWorkspace() -> Element {
    if !CONNECTED() {
        return rsx! {
            div { style: "display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px; flex: 1; text-align: center;",
                span { style: "width: 12px; height: 12px; border-radius: 999px; background: #22c55e;" }
                span { style: "font-size: 13px; font-weight: 600; color: #e4e4e7;",
                    "Looking for the session engine…"
                }
                span { style: "font-size: 11px; font-family: monospace; color: #71717a;",
                    "{EngineTarget::current().label()}"
                }
                span { style: "font-size: 11px; color: #71717a; max-width: 420px;",
                    "Start the engine on its machine (`fasttrackstudio --engine`) and this view will connect on its own."
                }
            }
        };
    }

    rsx! {
        div { style: "display: flex; flex-direction: row; height: 100%; width: 100%; min-height: 0;",
            // ── Navigator sidebar (left) ───────────────────────────
            div { style: "width: 280px; flex: none; min-height: 0; border-right: 1px solid #27272a; display: flex;",
                PerformanceSidebar {}
            }
            // ── Chart + performance display + transport (right column) ─────
            div { style: "flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column;",
                // The active song's keyflow chart, highlight synced to the
                // playhead (auto-advances with the setlist cursor).
                div { style: "flex: 1.4; min-height: 0; display: flex; flex-direction: column; border-bottom: 1px solid #27272a;",
                    crate::session_chart_pane::SessionChartPane {}
                }
                div { style: "position: relative; flex: 1; min-height: 0; display: flex;",
                    PerformanceLayout {}
                }
                div { style: "height: 92px; flex: none; border-top: 1px solid #27272a;",
                    TransportPanel {}
                }
            }
        }
    }
}
