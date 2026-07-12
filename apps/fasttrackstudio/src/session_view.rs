//! Session workspace — the setlist player surface.
//!
//! Renders session-ui's `PerformanceLayout` (fed by the global signals
//! that `session_ui::apply_setlist_event` maintains) plus a minimal
//! transport strip (play/stop, current song/section, prev/next song)
//! wired to the in-process `SetlistServiceClient` — which drives the
//! daw-standalone transport underneath.

use dioxus::prelude::*;
use session_ui::{PerformanceLayout, PerformanceSidebar, TransportPanel};

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
/// A guide-settings gear floats in the top-right of the main view. All
/// three panels read the same global signals the `SessionEventBridge`
/// keeps fed.
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
                // Main performance view + floating guide-settings gear.
                div { style: "position: relative; flex: 1; min-height: 0; display: flex;",
                    PerformanceLayout {}
                    GuideSettings {}
                }
                // Full transport control bar (arm / record / back /
                // play·pause / loop / advance)
                div { style: "height: 92px; flex: none; border-top: 1px solid #27272a;",
                    TransportPanel {}
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
    let pairs: Vec<usize> = (0..channel_count).step_by(2).filter(|&l| l + 1 < channel_count).collect();
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
fn GuideToggleRow(label: String, hint: String, on: bool, onclick: EventHandler<MouseEvent>) -> Element {
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
