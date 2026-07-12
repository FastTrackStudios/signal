//! Session workspace — the setlist player surface.
//!
//! Renders session-ui's `PerformanceLayout` (fed by the global signals
//! that `session_ui::apply_setlist_event` maintains) plus a minimal
//! transport strip (play/stop, current song/section, prev/next song)
//! wired to the in-process `SetlistServiceClient` — which drives the
//! daw-standalone transport underneath.

use dioxus::prelude::*;
use session_ui::{
    ACTIVE_INDICES, PLAYBACK_STATE, PerformanceLayout, PerformanceSidebar, SETLIST_STRUCTURE,
    Session, TransportPanel,
};

use crate::session_engine;

/// Invisible app-level component: bridges the setlist service's
/// `#[subscribe]` events hub straight into session-ui's global signals
/// (the in-process flavor of the desktop/web subscription loop).
/// Mounted once in `App` so it survives workspace switches.
#[component]
pub fn SessionEventBridge() -> Element {
    // ── Events stream: setlist structure + 60 Hz per-song transport ─────
    use_future(move || async move {
        let Some(engine) = session_engine::engine() else {
            tracing::warn!("session engine not running; setlist events unavailable");
            return;
        };

        // Consume the `events` `#[subscribe]` stream through the stream
        // client so the vox lane pumps it. (Attaching a raw Tx to the
        // in-process hub is never drained — the lane is what moves data.)
        let (tx, mut rx) = vox::channel::<session::SetlistEvent>();
        spawn(async move {
            if let Err(e) = engine.stream_client.events(tx).await {
                tracing::warn!("events subscription ended: {e:?}");
            }
        });

        // Fetch the already-built setlist as the initial snapshot
        // (deterministic, no reliance on the stream's first republish).
        match engine.client.setlist().await {
            Ok(setlist) => {
                session_ui::apply_setlist_event(&session::SetlistEvent::SetlistChanged(setlist));
            }
            Err(e) => tracing::warn!("initial setlist snapshot failed: {e:?}"),
        }

        while let Ok(Some(ev)) = rx.recv().await {
            let ev = ev.get();
            // Re-feed the guide when the *active* song hydrates (its sections /
            // count-in arrive after the initial cursor set the schedule).
            if let session::SetlistEvent::SongHydrated { index, song, .. }
            | session::SetlistEvent::SongEntered { index, song, .. } = ev
                && session_ui::ACTIVE_INDICES.peek().song_index == Some(*index)
            {
                crate::guide::set_current_song(song.clone());
            }
            session_ui::apply_setlist_event(ev);
        }
        tracing::warn!("setlist event stream ended");
    });

    // ── Active-indices stream: the cursor (which song/section is current) ─
    // The single source of truth for selection; also drives the guide's
    // active-song schedule. Fed by the service's `active_indices`
    // `#[subscribe]` hub (architect PubSub), not the setlist-events stream.
    use_future(move || async move {
        let Some(engine) = session_engine::engine() else { return };

        // Consume the `active_indices` `#[subscribe]` stream through the
        // stream client (pumps the vox lane).
        let (tx, mut rx) = vox::channel::<session_proto::ActiveIndices>();
        spawn(async move {
            if let Err(e) = engine.stream_client.active_indices(tx).await {
                tracing::warn!("active_indices subscription ended: {e:?}");
            }
        });

        // Open on song 0 / section 0. Fire it CONCURRENTLY (not awaited here)
        // so this future is already polling `rx` below when the seek's cursor
        // publish — and the active pump's follow-up 60 Hz publish — arrive.
        // (The demo's edit cursor starts at the timeline end → nothing active
        // until we seek.)
        spawn(async move {
            match engine.client.seek_to_section(0, 0).await {
                Ok(_) => tracing::info!("opened setlist on song 0 / section 0"),
                Err(e) => tracing::warn!("initial seek to song 0 failed: {e:?}"),
            }
        });

        let mut guide_song: Option<usize> = None;
        while let Ok(Some(ai)) = rx.recv().await {
            let ai = ai.get();
            // Guide follows the active song, reading the current (possibly
            // just-hydrated) song list from the shared setlist signal.
            feed_guide(&session_ui::SETLIST_STRUCTURE.peek().songs, &mut guide_song, ai.song_index);
            session_ui::apply_active_indices(ai);
        }
        tracing::warn!("active-indices stream ended");
    });

    rsx! {}
}

/// Hand `songs[index]` to the guide engine when it differs from the song
/// already scheduled. Cheap here (the rebuild runs on a worker thread).
fn feed_guide(
    songs: &[session_proto::Song],
    scheduled: &mut Option<usize>,
    index: Option<usize>,
) {
    let Some(index) = index else { return };
    if *scheduled == Some(index) {
        return;
    }
    let Some(song) = songs.get(index) else { return };
    *scheduled = Some(index);
    crate::guide::set_current_song(song.clone());
}

/// The Session workspace: the full setlist-player surface —
/// Navigator sidebar (left), performance display (center) and the
/// transport control bar (bottom), assembled from session-ui's panels.
/// A slim guide/status strip sits above the transport for the app's
/// guide toggle and quick song navigation. All three panels read the
/// same global signals the `SessionEventBridge` keeps fed.
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
        div { style: "display: flex; flex-direction: row; height: 100%; width: 100%; min-height: 0;",
            // ── Navigator sidebar (left) ───────────────────────────
            div { style: "width: 280px; flex: none; min-height: 0; border-right: 1px solid #27272a; display: flex;",
                PerformanceSidebar {}
            }
            // ── Performance display + transport (right column) ─────
            div { style: "flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column;",
                // Main performance view
                div { style: "flex: 1; min-height: 0; display: flex;",
                    PerformanceLayout {}
                }
                // Guide / status strip
                GuideBar {}
                // Full transport control bar (arm / record / back /
                // play·pause / loop / advance)
                div { style: "height: 92px; flex: none; border-top: 1px solid #27272a;",
                    TransportPanel {}
                }
            }
        }
    }
}

/// Slim strip above the transport: the app-specific guide toggle,
/// quick prev/next song, and the current song/section readout. Playback
/// itself lives in `TransportPanel`; this only owns things session-ui's
/// panel doesn't (the guide bus and song-level jumps).
#[component]
fn GuideBar() -> Element {
    let mut guide_on = use_signal(crate::guide::is_enabled);
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

    rsx! {
        div { style: "display: flex; align-items: center; gap: 10px; padding: 8px 12px; border-top: 1px solid #27272a; background: #0f0f11; flex: none;",

            // Prev / Next song
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

            // Guide (click / count-in / cues) toggle — flips the shared
            // guide state the aux audio hook reads.
            button {
                style: if guide_on() {
                    "padding: 6px 14px; border-radius: 6px; background: #14532d; color: #bbf7d0; border: 1px solid #166534; font-size: 13px; font-weight: 600; cursor: pointer;"
                } else {
                    btn
                },
                title: "Guide: click, count-in and section cues",
                onclick: move |_| {
                    let on = !guide_on();
                    crate::guide::set_enabled(on);
                    guide_on.set(on);
                },
                "Guide"
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
