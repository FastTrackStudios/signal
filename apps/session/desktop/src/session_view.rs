//! Session workspace — the setlist player surface.
//!
//! Renders session-ui's `PerformanceLayout` (fed by the global signals
//! that `session_ui::apply_setlist_event` maintains) plus a minimal
//! transport strip (play/stop, current song/section, prev/next song)
//! wired to the in-process `SetlistServiceClient` — which drives the
//! daw-standalone transport underneath.

use dioxus::prelude::*;
use session_ui::{PerformanceLayout, TransportPanel};

use crate::{active_engine, reaper_engine};

/// Wait until an engine is running, then hand back its clients.
///
/// Both subscription futures below are mounted once, at app start, and used
/// to give up immediately if no engine was up yet. That was fine while the
/// only way into Recording Mode was `FTS_SESSION_MODE=recording` with REAPER
/// already running — the engine either existed before the UI mounted or
/// never would. Now that the connect pane can attach to a REAPER started
/// afterwards, giving up means the player comes up subscribed to nothing:
/// connected, and permanently empty.
async fn engine_when_ready() -> active_engine::ActiveClients {
    loop {
        if let Some(engine) = active_engine::current() {
            return engine;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

/// Invisible app-level component: bridges the setlist service's
/// `#[subscribe]` events hub straight into session-ui's global signals
/// (the in-process flavor of the desktop/web subscription loop).
/// Mounted once in `App` so it survives workspace switches.
#[component]
pub fn SessionEventBridge() -> Element {
    // ── Connection state: mirror the engine's epoch into a signal ───────
    // `reaper_engine`'s supervisor runs on its own runtime and can only
    // touch plain statics; this is the one place that turns "the connection
    // changed" into something Dioxus re-renders on.
    use_future(move || async move {
        let mut seen = usize::MAX;
        loop {
            let epoch = reaper_engine::CONNECTION_EPOCH.load(std::sync::atomic::Ordering::Relaxed);
            if epoch != seen {
                seen = epoch;
                *REAPER_CONNECTIONS.write() = epoch;
            }
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        }
    });

    // ── Events stream: setlist structure + 60 Hz per-song transport ─────
    // The outer loop is what survives a REAPER bounce: the stream ends when
    // the connection dies, and this waits for the supervisor to attach a new
    // one and re-subscribes against it. Without it a reconnect produced a
    // connected, permanently silent player.
    use_future(move || async move {
        loop {
            let engine = engine_when_ready().await;

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
                    session_ui::apply_setlist_event(&session::SetlistEvent::SetlistChanged(
                        setlist,
                    ));
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
            tracing::warn!("setlist event stream ended; waiting to re-subscribe");
            // Backoff before retrying. In Recording Mode the engine is gone and
            // `engine_when_ready` blocks anyway, but in Live Mode it returns
            // instantly — so without this a stream that ends immediately spins.
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    });

    // ── Active-indices stream: the cursor (which song/section is current) ─
    // The single source of truth for selection; also drives the guide's
    // active-song schedule. Fed by the service's `active_indices`
    // `#[subscribe]` hub (architect PubSub), not the setlist-events stream.
    use_future(move || async move {
        loop {
            let engine = engine_when_ready().await;

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
                feed_guide(
                    &session_ui::SETLIST_STRUCTURE.peek().songs,
                    &mut guide_song,
                    ai.song_index,
                );
                session_ui::apply_active_indices(ai);
            }
            tracing::warn!("active-indices stream ended; waiting to re-subscribe");
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    });

    // ── Armed-track count: feeds the Record button's live count ─────────
    // Not part of the setlist wire at all — session's SetlistService has no
    // per-track visibility, so this goes straight through `daw::get()`'s
    // `Tracks` service (the same one `daw_ui::MixerPanel` self-connects
    // through). Loops forever: `daw::get()` isn't installed yet at app
    // boot in Recording Mode (only after `load_playlist` or a reconnect —
    // see `reaper_engine::install_daw_singleton`), and re-runs the whole
    // connect+seed+subscribe cycle if the stream ever ends (song switched
    // to a different project, REAPER restarted, etc).
    use_future(move || async move {
        loop {
            let Some(daw) = daw::get() else {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            };
            let Ok(project) = daw.current_project().await else {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            };
            let tracks = project.tracks();

            let mut armed: std::collections::HashMap<String, bool> =
                std::collections::HashMap::new();
            match tracks.all().await {
                Ok(all) => {
                    for t in &all {
                        armed.insert(t.guid.clone(), t.armed);
                    }
                    session_ui::ARMED_TRACK_COUNT
                        .with_mut(|n| *n = armed.values().filter(|a| **a).count());
                }
                Err(e) => {
                    tracing::warn!("armed-track count: seed failed: {e:?}");
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    continue;
                }
            }

            let Ok(mut events) = tracks.subscribe().await else {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            };
            while let Ok(Some(event)) = events.recv().await {
                if let daw::service::TrackEvent::ArmChanged {
                    guid,
                    armed: is_armed,
                } = event.get().event.clone()
                {
                    armed.insert(guid, is_armed);
                    session_ui::ARMED_TRACK_COUNT
                        .with_mut(|n| *n = armed.values().filter(|a| **a).count());
                }
            }
            tracing::warn!("armed-track count: track stream ended, reconnecting");
        }
    });

    rsx! {}
}

/// Hand `songs[index]` to the guide engine when it differs from the song
/// already scheduled. Cheap here (the rebuild runs on a worker thread).
fn feed_guide(songs: &[session_proto::Song], scheduled: &mut Option<usize>, index: Option<usize>) {
    let Some(index) = index else { return };
    if *scheduled == Some(index) {
        return;
    }
    let Some(song) = songs.get(index) else { return };
    *scheduled = Some(index);
    crate::guide::set_current_song(song.clone());
}

/// Mirrors `reaper_engine::CONNECTION_EPOCH` into Dioxus.
///
/// `SessionWorkspace` decides between the connect pane and the player by
/// reading `active_engine::current()` — a plain static, not a signal, so
/// nothing schedules a re-render when it changes. Both directions matter:
/// without this a successful connect leaves the pane up (reading as a dead
/// button), and a REAPER that quits leaves a frozen player up instead of
/// the pane.
static REAPER_CONNECTIONS: GlobalSignal<usize> = Signal::global(|| 0);

/// Recording Mode's "attach to a running REAPER" pane.
///
/// This used to be a dead end reading "check the logs": Recording Mode only
/// ever connected at app startup, so the ordinary case — open the app, then
/// open REAPER — meant quitting and relaunching. The connection machinery
/// was always re-entrant (`reaper_engine::ensure_connected` is idempotent
/// and documented for exactly this); nothing drove it from the UI.
///
/// Rescanning is on a timer rather than a manual Refresh, because the thing
/// being waited for is REAPER finishing its own startup — the user has
/// already done their part, and a list that fills itself in is the whole
/// difference between "attach" and "retry until it works".
#[component]
fn ConnectToReaper() -> Element {
    let mut found = use_signal(Vec::<crate::reaper_engine::LiveReaper>::new);
    let mut error = use_signal(|| Option::<String>::None);
    let mut connecting = use_signal(|| false);

    // Poll for REAPERs appearing and disappearing. Cheap: a readdir over
    // /tmp plus a `kill(pid, 0)` per candidate socket.
    use_future(move || async move {
        loop {
            let live = crate::reaper_engine::discover_all();
            if *found.peek() != live {
                found.set(live);
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    });

    let mut connect = move |socket: Option<std::path::PathBuf>| {
        connecting.set(true);
        error.set(None);
        let rx = crate::reaper_engine::spawn_connect(socket);
        spawn(async move {
            match rx.await {
                // No signal bump here: `ensure_connected` bumps
                // `CONNECTION_EPOCH`, and `SessionEventBridge`'s mirror
                // future turns that into the signal change that swaps this
                // pane for the player. One path for both a manual connect
                // and the supervisor's automatic one.
                Ok(Ok(())) => connecting.set(false),
                Ok(Err(e)) => {
                    error.set(Some(e));
                    connecting.set(false);
                }
                Err(_) => {
                    error.set(Some("the connect task ended without answering".into()));
                    connecting.set(false);
                }
            }
        });
    };

    let live = found.read().clone();
    let busy = *connecting.read();

    rsx! {
        div { style: "display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 14px; height: 100%; width: 100%; text-align: center; padding: 24px;",
            span { style: "font-size: 20px; font-weight: 700;", "Not connected to REAPER" }

            if live.is_empty() {
                span { style: "font-size: 13px; color: #a1a1aa; max-width: 520px;",
                    "Waiting for a REAPER with the FTS extension loaded. Start REAPER — this will pick it up on its own."
                }
            } else {
                span { style: "font-size: 13px; color: #a1a1aa; max-width: 520px;",
                    if live.len() == 1 { "Found a running REAPER." } else { "Found several running REAPERs." }
                }
                div { style: "display: flex; flex-direction: column; gap: 6px; min-width: 320px;",
                    for reaper in live.iter().cloned() {
                        div {
                            key: "{reaper.pid}",
                            style: "display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 8px 12px; border: 1px solid #3f3f46; border-radius: 6px;",
                            span { style: "font-size: 13px; font-family: monospace;", "pid {reaper.pid}" }
                            button {
                                disabled: busy,
                                style: "font-size: 13px; padding: 4px 14px; border-radius: 4px; cursor: pointer;",
                                onclick: move |_| connect(Some(reaper.socket.clone())),
                                if busy { "Connecting…" } else { "Connect" }
                            }
                        }
                    }
                }
            }

            if let Some(message) = error.read().clone() {
                // The real ConnectError, not "check the logs" — the common
                // failures (socket vanished mid-connect, an extension that
                // does not mount SetlistService) are only distinguishable
                // from their text.
                span { style: "font-size: 12px; color: #f87171; max-width: 520px; font-family: monospace;",
                    "{message}"
                }
            }
        }
    }
}

/// The Session workspace: the full setlist-player surface —
/// Navigator sidebar (left), performance display (center) and the
/// transport control bar (bottom), assembled from session-ui's panels.
/// A guide-settings gear floats in the top-right of the main view. All
/// three panels read the same global signals the `SessionEventBridge`
/// keeps fed.
#[component]
pub fn SessionWorkspace() -> Element {
    // Subscribe to Recording Mode attaches, so a successful Connect swaps
    // the pane below for the player instead of leaving it up.
    let _ = REAPER_CONNECTIONS();

    if active_engine::current().is_none() {
        let recording = !crate::reaper_engine::is_connected()
            && std::env::var("FTS_SESSION_MODE").as_deref() == Ok("recording");
        if recording {
            return rsx! { ConnectToReaper {} };
        }
        return rsx! {
            div { style: "display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; height: 100%; width: 100%; text-align: center;",
                span { style: "font-size: 20px; font-weight: 700;", "Session engine offline" }
                span { style: "font-size: 13px; color: #a1a1aa; max-width: 480px;",
                    "The in-process daw-standalone backend failed to start — check the logs."
                }
            }
        };
    }

    if session_ui::SETLIST_STRUCTURE.read().songs.is_empty() {
        return rsx! {
            div { style: "display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; height: 100%; width: 100%; text-align: center;",
                span { style: "font-size: 20px; font-weight: 700;", "No setlist loaded" }
                span { style: "font-size: 13px; color: #a1a1aa; max-width: 480px;",
                    "Pick a setlist from Home — or build a new one — then Load & Play."
                }
            }
        };
    }

    rsx! {
        div { style: "display: flex; flex-direction: row; height: 100%; width: 100%; min-height: 0;",
            // ── Performance display + transport ────────────────────
            //
            // Full width: the navigator that used to take 280px down
            // the left is gone. Songs are changed from the transport
            // bar and the arrow keys, and the space is the view's.
            div { style: "flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column;",
                // Main performance view + floating guide-settings gear.
                div { style: "position: relative; flex: 1; min-height: 0; display: flex;",
                    PerformanceLayout {}
                    GuideSettings {}
                }
                // Full transport control bar (arm / record / back /
                // play·pause / loop / advance)
                div { style: "height: 92px; flex: none; border-top: 1px solid #27272a;",
                    // Playback-only setlist surface — no REAPER, nothing
                    // to arm or record into.
                    TransportPanel { show_recording: false }
                }
            }
        }
    }
}

/// Floating guide-settings control: a gear in the top-right of the main
/// view that opens a small popover of independent toggles (Guide master /
/// Metronome / guide settings), plus a general settings gear. Replaces the
/// old always-present guide strip.
#[component]
fn GuideSettings() -> Element {
    let mut metro_open = use_signal(|| false);
    let mut gear_open = use_signal(|| false);

    let guide_on = use_signal(crate::guide::is_enabled);

    let icon = |active: bool| -> String {
        let (bg, fg, br) = if active {
            ("#14532d", "#bbf7d0", "#166534")
        } else {
            ("#18181b", "#a1a1aa", "#27272a")
        };
        format!(
            "display: flex; align-items: center; justify-content: center; width: 34px; height: 34px; border-radius: 8px; background: {bg}; color: {fg}; border: 1px solid {br}; cursor: pointer;"
        )
    };

    rsx! {
        div { style: "position: absolute; top: 12px; right: 12px; z-index: 30; display: flex; gap: 8px;",

            // ── Metronome: all click / count / guide settings ──────────
            div { style: "position: relative;",
                button {
                    style: icon(guide_on()),
                    title: "Metronome & guide",
                    onclick: move |_| {
                        gear_open.set(false);
                        metro_open.toggle();
                    },
                    // metronome glyph
                    svg {
                        width: "18",
                        height: "18",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        path { d: "M9 3 h6 l3 15 H6 z" }
                        line { x1: "6", y1: "13", x2: "18", y2: "13" }
                        path { d: "M12 18 L16 6" }
                    }
                }
                if metro_open() {
                    div {
                        style: "position: fixed; inset: 0; z-index: 20;",
                        onclick: move |_| metro_open.set(false),
                    }
                    MetronomePanel {}
                }
            }

            // ── General settings (placeholder for now) ─────────────────
            div { style: "position: relative;",
                button {
                    style: icon(false),
                    title: "Settings",
                    onclick: move |_| {
                        metro_open.set(false);
                        gear_open.toggle();
                    },
                    span { style: "font-size: 17px;", "\u{2699}" }
                }
                if gear_open() {
                    div {
                        style: "position: fixed; inset: 0; z-index: 20;",
                        onclick: move |_| gear_open.set(false),
                    }
                    div {
                        style: "position: absolute; top: 42px; right: 0; z-index: 30; width: 224px; background: #0f0f11; border: 1px solid #27272a; border-radius: 10px; box-shadow: 0 10px 30px rgba(0,0,0,0.5); padding: 12px;",
                        onclick: move |evt: MouseEvent| evt.stop_propagation(),
                        div { style: "font-size: 12px; font-weight: 700; color: #e4e4e7; padding-bottom: 6px;",
                            "Settings"
                        }
                        span { style: "font-size: 12px; color: #71717a;",
                            "General app settings — coming soon."
                        }
                    }
                }
            }
        }
    }
}

/// The metronome popover: guide master, click (+ sound), count-in, and
/// section cues, plus a placeholder for the (upcoming) output routing.
#[component]
fn MetronomePanel() -> Element {
    let mut guide_on = use_signal(crate::guide::is_enabled);
    let mut click_on = use_signal(crate::guide::click_enabled);
    let mut count_on = use_signal(crate::guide::count_enabled);
    let mut cues_on = use_signal(crate::guide::cues_enabled);
    let mut click_sound = use_signal(crate::guide::click_sound_index);

    // Output routing (lock-free MixerRouting in daw-standalone). Channel
    // pairs are (l, l+1); the device's real channel count is published when
    // the audio stream opens.
    let routing = daw_standalone::audio_engine::MixerRouting::shared();
    let channel_count = routing.channel_count();
    let pairs: Vec<usize> = (0..channel_count)
        .step_by(2)
        .filter(|&l| l + 1 < channel_count)
        .collect();
    let mut main_l = use_signal(|| routing.main_pair().0);
    let mut guide_l = use_signal(|| routing.guide_pair().0);
    let mut phones_on = use_signal(|| routing.phones_enabled());
    let mut phones_l = use_signal(|| routing.phones_pair().0);
    let mut main_muted = use_signal(|| routing.main_muted());

    rsx! {
        div {
            style: "position: absolute; top: 42px; right: 0; z-index: 30; width: 250px; background: #0f0f11; border: 1px solid #27272a; border-radius: 10px; box-shadow: 0 10px 30px rgba(0,0,0,0.5); padding: 10px;",
            onclick: move |evt: MouseEvent| evt.stop_propagation(),

            div { style: "font-size: 12px; font-weight: 700; color: #e4e4e7; padding: 2px 6px 8px;",
                "Metronome & Guide"
            }

            GuideToggleRow {
                label: "Guide",
                hint: "Master on/off",
                on: guide_on(),
                onclick: move |_| {
                    let v = !guide_on();
                    crate::guide::set_enabled(v);
                    guide_on.set(v);
                },
            }
            div { style: "height: 1px; background: #27272a; margin: 6px 4px;" }

            GuideToggleRow {
                label: "Click",
                hint: "Metronome",
                on: click_on(),
                onclick: move |_| {
                    let v = !click_on();
                    crate::guide::set_click_enabled(v);
                    click_on.set(v);
                },
            }
            // Click sound picker
            div { style: "display: flex; align-items: center; gap: 8px; padding: 4px 6px 8px;",
                span { style: "font-size: 12px; color: #a1a1aa; flex: 1;", "Click sound" }
                select {
                    style: "background: #18181b; color: #e4e4e7; border: 1px solid #27272a; border-radius: 6px; padding: 4px 6px; font-size: 12px; cursor: pointer;",
                    onchange: move |evt| {
                        if let Ok(i) = evt.value().parse::<usize>() {
                            crate::guide::set_click_sound(i);
                            click_sound.set(i);
                        }
                    },
                    for (i , (name , _)) in crate::guide::CLICK_SOUNDS.iter().enumerate() {
                        option { value: "{i}", selected: click_sound() == i, "{name}" }
                    }
                }
            }

            GuideToggleRow {
                label: "Count-in",
                hint: "Counts before the song",
                on: count_on(),
                onclick: move |_| {
                    let v = !count_on();
                    crate::guide::set_count_enabled(v);
                    count_on.set(v);
                },
            }
            GuideToggleRow {
                label: "Section cues",
                hint: "Spoken section names",
                on: cues_on(),
                onclick: move |_| {
                    let v = !cues_on();
                    crate::guide::set_cues_enabled(v);
                    cues_on.set(v);
                },
            }

            div { style: "height: 1px; background: #27272a; margin: 6px 4px;" }
            div { style: "padding: 2px 6px;",
                div { style: "font-size: 12px; font-weight: 600; color: #e4e4e7; padding-bottom: 4px;", "Output" }

                if pairs.len() < 2 {
                    span { style: "font-size: 11px; color: #71717a;",
                        "This device exposes {channel_count} output channel(s) — a single stereo pair, so there's nothing to route separately."
                    }
                } else {
                    // Main mix output pair.
                    OutputPairRow {
                        label: "Main out",
                        pairs: pairs.clone(),
                        selected: main_l(),
                        onselect: move |l: usize| {
                            routing.set_main_pair(l, l + 1);
                            main_l.set(l);
                        },
                    }
                    // Metronome / guide output pair.
                    OutputPairRow {
                        label: "Metronome out",
                        pairs: pairs.clone(),
                        selected: guide_l(),
                        onselect: move |l: usize| {
                            routing.set_guide_pair(l, l + 1);
                            guide_l.set(l);
                        },
                    }

                    div { style: "height: 1px; background: #27272a; margin: 6px 4px;" }

                    // Headphone-check monitor bus (main + metronome summed).
                    GuideToggleRow {
                        label: "Headphone check",
                        hint: "Sum main + metronome to a monitor pair",
                        on: phones_on(),
                        onclick: move |_| {
                            let v = !phones_on();
                            routing.set_phones_enabled(v);
                            phones_on.set(v);
                        },
                    }
                    if phones_on() {
                        OutputPairRow {
                            label: "Check out",
                            pairs: pairs.clone(),
                            selected: phones_l(),
                            onselect: move |l: usize| {
                                routing.set_phones_pair(l, l + 1);
                                phones_l.set(l);
                            },
                        }
                    }

                    // Mute the device main output (keep it in your IEMs only).
                    GuideToggleRow {
                        label: "Mute main out",
                        hint: "Silence the main pair on the device",
                        on: main_muted(),
                        onclick: move |_| {
                            let v = !main_muted();
                            routing.set_main_muted(v);
                            main_muted.set(v);
                        },
                    }
                }
            }
        }
    }
}

/// One "label → channel-pair dropdown" row in the metronome output section.
#[component]
fn OutputPairRow(
    label: String,
    pairs: Vec<usize>,
    selected: usize,
    onselect: EventHandler<usize>,
) -> Element {
    rsx! {
        div { style: "display: flex; align-items: center; gap: 8px; padding: 4px 6px;",
            span { style: "font-size: 12px; color: #a1a1aa; flex: 1;", "{label}" }
            select {
                style: "background: #18181b; color: #e4e4e7; border: 1px solid #27272a; border-radius: 6px; padding: 4px 6px; font-size: 12px; cursor: pointer;",
                onchange: move |evt| {
                    if let Ok(l) = evt.value().parse::<usize>() {
                        onselect.call(l);
                    }
                },
                for l in pairs.iter().copied() {
                    option { value: "{l}", selected: selected == l, "{l + 1}/{l + 2}" }
                }
            }
        }
    }
}

/// One labelled toggle row inside the guide-settings popover.
#[component]
fn GuideToggleRow(
    label: String,
    hint: String,
    on: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let (track, knob_x) = if on {
        ("#16a34a", "18px")
    } else {
        ("#3f3f46", "2px")
    };
    rsx! {
        button {
            style: "display: flex; align-items: center; gap: 10px; width: 100%; padding: 7px 6px; background: transparent; border: none; cursor: pointer; text-align: left;",
            onclick: move |evt| onclick.call(evt),
            div { style: "display: flex; flex-direction: column; flex: 1; min-width: 0;",
                span { style: "font-size: 13px; color: #e4e4e7; font-weight: 600;", "{label}" }
                span { style: "font-size: 11px; color: #71717a;", "{hint}" }
            }
            // Switch
            div { style: "position: relative; width: 36px; height: 20px; border-radius: 10px; flex: none; background: {track}; transition: background 120ms;",
                div { style: "position: absolute; top: 2px; left: {knob_x}; width: 16px; height: 16px; border-radius: 8px; background: #fafafa; transition: left 120ms;" }
            }
        }
    }
}
