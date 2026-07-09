//! Session workspace — the setlist player surface.
//!
//! Renders session-ui's `PerformanceLayout` (fed by the global signals
//! that `session_ui::apply_setlist_event` maintains) plus a minimal
//! transport strip (play/stop, current song/section, prev/next song)
//! wired to the in-process `SetlistServiceClient` — which drives the
//! daw-standalone transport underneath.

use dioxus::prelude::*;
use session_ui::{ACTIVE_INDICES, PLAYBACK_STATE, PerformanceLayout, SETLIST_STRUCTURE, Session};

use crate::session_engine;

/// Invisible app-level component: bridges the setlist service's
/// `#[subscribe]` events hub straight into session-ui's global signals
/// (the in-process flavor of the desktop/web subscription loop).
/// Mounted once in `App` so it survives workspace switches.
#[component]
pub fn SessionEventBridge() -> Element {
    use_future(move || async move {
        let Some(engine) = session_engine::engine() else {
            tracing::warn!("session engine not running; setlist events unavailable");
            return;
        };

        use session::services::setlist_service::SetlistServiceStreamSource;
        let (tx, mut rx) = vox::channel::<session::SetlistEvent>();
        engine.setlist.events_hub().attach(tx);

        // The hub only carries NEW events — fetch the already-built
        // setlist as the initial snapshot (deterministic, no reliance
        // on republish timing).
        match engine.client.setlist().await {
            Ok(setlist) => {
                session_ui::apply_setlist_event(&session::SetlistEvent::SetlistChanged(setlist));
            }
            Err(e) => tracing::warn!("initial setlist snapshot failed: {e:?}"),
        }

        // Live updates: SetlistChanged / SongHydrated / ActiveIndices /
        // 60Hz TransportUpdate — folded into the global signals on the
        // UI scheduler.
        while let Ok(Some(ev)) = rx.recv().await {
            session_ui::apply_setlist_event(ev.get());
        }
        tracing::warn!("setlist event stream ended");
    });

    rsx! {}
}

/// The Session workspace: transport strip over the performance layout.
#[component]
pub fn SessionWorkspace() -> Element {
    if session_engine::engine().is_none() {
        return rsx! {
            div { style: "display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; height: 100%; width: 100%; text-align: center;",
                span { style: "font-size: 20px; font-weight: 700;", "Session engine offline" }
                span { style: "font-size: 13px; color: #a1a1aa; max-width: 480px;",
                    "The in-process daw-standalone backend failed to start — check the logs."
                }
            }
        };
    }

    rsx! {
        div { style: "display: flex; flex-direction: column; height: 100%; width: 100%; min-height: 0;",
            TransportStrip {}
            div { style: "flex: 1; min-height: 0; display: flex;",
                PerformanceLayout {}
            }
        }
    }
}

/// Minimal transport strip: prev / play / stop / next + current
/// song/section display. Commands go through the `Session` singleton
/// (the in-process `SetlistServiceClient`), which drives the
/// daw-standalone transport service.
#[component]
fn TransportStrip() -> Element {
    let indices = ACTIVE_INDICES.read();
    let song_index = indices.song_index;
    let section_index = indices.section_index;
    drop(indices);

    let playback_state = *PLAYBACK_STATE.read();
    let is_playing = matches!(
        playback_state,
        daw::service::PlayState::Playing | daw::service::PlayState::Recording
    );

    let (song_label, section_label, song_count) = {
        let setlist = SETLIST_STRUCTURE.read();
        let song = song_index.and_then(|i| setlist.songs.get(i));
        let song_label = match (song, song_index) {
            (Some(s), Some(i)) => format!("{}. {}", i + 1, s.name),
            _ => "—".to_string(),
        };
        let section_label = song
            .and_then(|s| section_index.and_then(|i| s.sections.get(i)))
            .map(|sec| sec.display_name())
            .unwrap_or_else(|| "—".to_string());
        (song_label, section_label, setlist.songs.len())
    };

    let btn = "padding: 6px 14px; border-radius: 6px; background: #18181b; color: #e4e4e7; border: 1px solid #27272a; font-size: 13px; cursor: pointer;";
    let btn_accent = "padding: 6px 18px; border-radius: 6px; background: #e4e4e7; color: #0a0a0a; border: none; font-weight: 700; font-size: 13px; cursor: pointer;";

    rsx! {
        div { style: "display: flex; align-items: center; gap: 10px; padding: 8px 12px; border-bottom: 1px solid #27272a; background: #0f0f11; flex: none;",

            // Prev / Play-Stop / Next
            button {
                style: btn,
                title: "Previous song",
                onclick: move |_| {
                    spawn(async move {
                        if let Err(e) = Session::get().setlist().previous_song().await {
                            tracing::warn!("previous_song failed: {e:?}");
                        }
                    });
                },
                "|◀"
            }
            if is_playing {
                button {
                    style: btn_accent,
                    title: "Stop",
                    onclick: move |_| {
                        spawn(async move {
                            if let Err(e) = Session::get().setlist().stop().await {
                                tracing::warn!("stop failed: {e:?}");
                            }
                        });
                    },
                    "■ Stop"
                }
            } else {
                button {
                    style: btn_accent,
                    title: "Play",
                    onclick: move |_| {
                        spawn(async move {
                            if let Err(e) = Session::get().setlist().play().await {
                                tracing::warn!("play failed: {e:?}");
                            }
                        });
                    },
                    "▶ Play"
                }
            }
            button {
                style: btn,
                title: "Next song",
                onclick: move |_| {
                    spawn(async move {
                        if let Err(e) = Session::get().setlist().next_song().await {
                            tracing::warn!("next_song failed: {e:?}");
                        }
                    });
                },
                "▶|"
            }

            // Current song / section
            div { style: "display: flex; flex-direction: column; margin-left: 12px; min-width: 0;",
                span { style: "font-size: 13px; font-weight: 700; color: #e4e4e7; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                    "{song_label}"
                }
                span { style: "font-size: 11px; color: #a1a1aa;", "{section_label}" }
            }

            div { style: "flex: 1;" }

            span { style: "font-size: 11px; color: #71717a;",
                if is_playing { "PLAYING" } else { "STOPPED" }
                " · {song_count} songs · daw-standalone"
            }
        }
    }
}
