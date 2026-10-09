//! The player, in the top bar: a record player that loops a recording
//! through the rig in the guitar's place — to hear a tone with no guitar to
//! hand. Its card picks a guitar and one of its recordings (or the ones the
//! app ships), records a new one off the input, and notes what each was
//! played through: the patch, its rig preset, the gain as the player puts
//! it.

use dioxus::prelude::*;
use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{DiClip, GuitarEntry, RigStatus};

use super::tokens::*;

/// The lengths a loop records at, seconds.
const LENGTHS: [u32; 3] = [4, 8, 16];
const RECORD: &str = "#ef4444";

#[component]
pub fn Player() -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut open = use_signal(|| false);
    let mut status = use_signal(RigStatus::default);
    let mut clips = use_signal(Vec::<DiClip>::new);
    let mut guitars = use_signal(Vec::<GuitarEntry>::new);
    // The guitar the card shows the recordings of (empty: the shipped ones).
    let mut guitar = use_signal(|| None::<String>);
    let mut turn = use_signal(|| 0u32);
    // The status while the card is open or it plays; the disc turning.
    {
        let rig = rig.clone();
        use_future(move || {
            let rig = rig.clone();
            async move {
                let Some(rig) = rig else { return };
                if let Ok(s) = rig.setup().await {
                    if guitar.peek().is_none() {
                        guitar.set(s.guitars.get(s.guitar_index as usize).map(|g| g.id.clone()));
                    }
                    guitars.set(s.guitars);
                }
                loop {
                    if let Ok(st) = rig.status().await {
                        if *status.peek() != st {
                            status.set(st);
                        }
                    }
                    if open() && let Ok(c) = rig.di_clips().await && *clips.peek() != c {
                        clips.set(c);
                    }
                    let playing = status.peek().di_playing;
                    if playing {
                        let next = (*turn.peek() + 12) % 360;
                        turn.set(next);
                    }
                    let wait = if playing { 60 } else if open() { 300 } else { 900 };
                    architect::platform::sleep(std::time::Duration::from_millis(wait)).await;
                }
            }
        });
    }
    let st = status();
    let playing = st.di_playing;
    let ink = if playing { LIVE } else { INK_2 };
    let bg = if open() { "rgba(255,255,255,0.06)" } else { "transparent" };
    rsx! {
        div { style: "position: relative; align-self: stretch; display: flex;",
            button {
                "aria-label": if playing { "Player: playing — open" } else { "Player — open" },
                style: "align-self: stretch; min-width: 52px; padding: 0 12px; display: flex; align-items: center; justify-content: center; gap: 8px; border: none; background: {bg}; cursor: pointer;",
                onclick: move |_| open.toggle(),
                // A record on its platter, its arm over it; turning while it plays.
                svg { key: "{ink}{turn()}", width: "22", height: "22", view_box: "0 0 22 22", style: "display: block;",
                    g { transform: "rotate({turn()} 10 11)",
                        circle { cx: "10", cy: "11", r: "8", fill: "none", stroke: ink, stroke_width: "1.6" }
                        circle { cx: "10", cy: "11", r: "4.6", fill: "none", stroke: ink, stroke_width: "0.8", opacity: "0.6" }
                        circle { cx: "10", cy: "11", r: "1.6", fill: ink }
                        path { d: "M10 3.6v2", stroke: ink, stroke_width: "1.4", stroke_linecap: "round" }
                    }
                    path { d: "M19.5 2.5v9l-4 4", fill: "none", stroke: ink, stroke_width: "1.6", stroke_linecap: "round", stroke_linejoin: "round" }
                }
            }
            if open() {
                Card { status: st, clips: clips(), guitars: guitars(), guitar, on_close: move |()| open.set(false) }
            }
        }
    }
}

#[component]
fn Card(status: RigStatus, clips: Vec<DiClip>, guitars: Vec<GuitarEntry>, guitar: Signal<Option<String>>, on_close: EventHandler<()>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut seconds = use_signal(|| 8u32);
    let mut guitar = guitar;
    let chosen = guitar();
    // The guitar's recordings, then the ones the app ships.
    let shown: Vec<DiClip> = clips
        .iter()
        .filter(|c| !c.builtin && chosen.as_deref() == Some(c.guitar.as_str()))
        .chain(clips.iter().filter(|c| c.builtin))
        .cloned()
        .collect();
    let playing = status.di_playing;
    let current = clips.iter().find(|c| c.id == status.di_clip_id).cloned();
    let recording = status.di_recording;
    let chip = |on: bool| format!("height: 34px; padding: 0 12px; border-radius: 17px; border: 1px solid {}; background: {}; color: {}; font-size: 13.5px; font-weight: 650; font-family: {FONT}; display: flex; align-items: center; cursor: pointer; flex-shrink: 0;", pick(on, INK_2, RULE_STRONG), pick(on, "rgba(255,255,255,0.08)", "transparent"), pick(on, INK, INK_2));
    rsx! {
        div { style: "position: absolute; top: 100%; right: 0; z-index: 60; width: 400px; max-height: 620px; display: flex; flex-direction: column; background: {SHEET}; border: 1px solid {RULE_STRONG}; border-radius: 14px; box-shadow: 0 18px 40px rgba(0,0,0,0.55); overflow: hidden; font-family: {FONT}; color: {INK};",
            // What plays, and the switch for it.
            div { style: "display: flex; align-items: center; gap: 12px; padding: 14px 14px 12px 16px; border-bottom: 1px solid {RULE};",
                div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px;",
                    span { style: "font-size: 16px; font-weight: 750; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                        {current.as_ref().map_or_else(|| "Nothing playing".to_string(), |c| c.name.clone())}
                    }
                    if let Some(c) = current.as_ref().filter(|c| !c.builtin) {
                        span { style: "font-size: 12.5px; color: {INK_3}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{through(c)}" }
                    }
                }
                {
                    let rig = rig.clone();
                    let id = current.as_ref().map(|c| c.id.clone()).or_else(|| shown.first().map(|c| c.id.clone()));
                    rsx! {
                        button {
                            "aria-label": if playing { "Stop" } else { "Play" },
                            style: "width: 48px; height: 48px; border-radius: 24px; border: none; display: flex; align-items: center; justify-content: center; background: {pick(playing, INK, LIVE)}; cursor: pointer; flex-shrink: 0;",
                            onclick: move |_| {
                                let (Some(r), id) = (rig.clone(), id.clone()) else { return };
                                let _ = dioxus_core::spawn_forever(async move {
                                    if playing {
                                        let _ = r.play_di(0, false).await;
                                    } else if let Some(id) = id {
                                        let _ = r.play_di_clip(id).await;
                                    }
                                });
                            },
                            svg { key: "{playing}", width: "18", height: "18", view_box: "0 0 18 18",
                                if playing {
                                    rect { x: "4", y: "4", width: "10", height: "10", rx: "1.5", fill: "#0a0a0a" }
                                } else {
                                    path { d: "M6 3.5v11l9-5.5Z", fill: "#0a0a0a" }
                                }
                            }
                        }
                    }
                }
                button { "aria-label": "Close", style: "width: 34px; height: 34px; border: none; background: transparent; color: {INK_3}; font-size: 20px; cursor: pointer;", onclick: move |_| on_close.call(()), "×" }
            }
            // Whose recordings: a guitar's, or the shipped ones.
            div { style: "display: flex; gap: 8px; padding: 12px 14px; overflow-x: auto; border-bottom: 1px solid {RULE};",
                for g in guitars.iter() {
                    {
                        let id = g.id.clone();
                        let on = chosen.as_deref() == Some(id.as_str());
                        rsx! { button { key: "{id}", style: "{chip(on)}", onclick: move |_| guitar.set(Some(id.clone())), "{g.name}" } }
                    }
                }
            }
            // The recordings.
            div { style: "flex: 1; min-height: 0; overflow-y: auto;",

                for c in shown.into_iter() {
                    ClipRow { key: "{c.id}", clip: c.clone(), on: playing && c.id == status.di_clip_id }
                }
            }
            // Record a new one off the input.
            div { style: "display: flex; align-items: center; gap: 8px; padding: 12px 14px; border-top: 1px solid {RULE};",
                for len in LENGTHS {
                    button { key: "{len}", style: "{chip(seconds() == len)}", onclick: move |_| seconds.set(len), "{len} s" }
                }
                span { style: "flex: 1;" }
                {
                    let rig = rig.clone();
                    rsx! {
                        button {
                            disabled: recording,
                            style: "height: 40px; padding: 0 16px; border-radius: 20px; border: none; display: flex; align-items: center; gap: 8px; background: {pick(recording, RECORD, \"rgba(239,68,68,0.16)\")}; color: {pick(recording, \"#fff\", RECORD)}; font-size: 14px; font-weight: 750; font-family: {FONT}; cursor: pointer;",
                            onclick: move |_| {
                                let secs = seconds();
                                if let Some(r) = rig.clone() {
                                    let _ = dioxus_core::spawn_forever(async move { let _ = r.record_loop(secs).await; });
                                }
                            },
                            span { style: "width: 10px; height: 10px; border-radius: 5px; background: {pick(recording, \"#fff\", RECORD)};" }
                            if recording { "Recording…" } else { "Record" }
                        }
                    }
                }
            }
        }
    }
}

/// What a recording went through, in a line: its patch, rig preset, gain.
fn through(c: &DiClip) -> String {
    [c.patch.as_str(), c.rig_preset.as_str(), c.gain.as_str()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" · ")
}

/// A recording: tap to play it; a recorded one's name and gain note edit in
/// place, and it deletes.
#[component]
fn ClipRow(clip: DiClip, on: bool) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut name = use_signal(|| clip.name.clone());
    let mut gain = use_signal(|| clip.gain.clone());
    let (r_play, r_name, r_gain, r_del) = (rig.clone(), rig.clone(), rig.clone(), rig);
    let save = move |rig: Option<RigClient>, c: DiClip| {
        if let Some(r) = rig {
            let _ = dioxus_core::spawn_forever(async move { let _ = r.edit_di_clip(c).await; });
        }
    };
    let (c_name, c_gain, id_play, id_del) = (clip.clone(), clip.clone(), clip.id.clone(), clip.id.clone());
    let field = format!("flex: 1; min-width: 0; height: 30px; padding: 0 8px; border-radius: 7px; border: 1px solid transparent; background: rgba(255,255,255,0.05); color: {INK}; font-size: 13px; font-family: {FONT};");
    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 6px; padding: 10px 14px; border-bottom: 1px solid {RULE}; background: {pick(on, \"rgba(34,197,94,0.08)\", \"transparent\")};",
            div { style: "display: flex; align-items: center; gap: 10px;",
                button {
                    "aria-label": "Play {clip.name}",
                    style: "width: 34px; height: 34px; border-radius: 17px; border: 1px solid {pick(on, LIVE, RULE_STRONG)}; background: transparent; display: flex; align-items: center; justify-content: center; cursor: pointer; flex-shrink: 0;",
                    onclick: move |_| {
                        let id = id_play.clone();
                        if let Some(r) = r_play.clone() {
                            let _ = dioxus_core::spawn_forever(async move {
                                let _ = if on { r.play_di(0, false).await } else { r.play_di_clip(id).await };
                            });
                        }
                    },
                    svg { key: "{on}", width: "12", height: "12", view_box: "0 0 12 12",
                        if on {
                            rect { x: "2.5", y: "2.5", width: "7", height: "7", rx: "1", fill: LIVE }
                        } else {
                            path { d: "M3.5 2v8l6.5-4Z", fill: INK_2 }
                        }
                    }
                }
                if clip.builtin {
                    span { style: "flex: 1; font-size: 15px; font-weight: 650;", "{clip.name}" }
                } else {
                    input {
                        style: "{field} font-size: 15px; font-weight: 650;",
                        value: "{name}",
                        oninput: move |e| name.set(e.value()),
                        onchange: move |_| save(r_name.clone(), DiClip { name: name(), ..c_name.clone() }),
                    }
                    span { style: "font-size: 12px; color: {INK_3}; font-variant-numeric: tabular-nums;", {format!("{:.0} s", clip.seconds)} }
                    button {
                        "aria-label": "Delete {clip.name}",
                        style: "width: 30px; height: 30px; border: none; background: transparent; color: {INK_3}; font-size: 16px; cursor: pointer;",
                        onclick: move |_| {
                            let id = id_del.clone();
                            if let Some(r) = r_del.clone() {
                                let _ = dioxus_core::spawn_forever(async move { let _ = r.delete_di_clip(id).await; });
                            }
                        },
                        svg { width: "16", height: "16", view_box: "0 0 16 16",
                            path { d: "M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5l.7 8.5h5.6l.7-8.5", fill: "none", stroke: INK_3, stroke_width: "1.4", stroke_linecap: "round", stroke_linejoin: "round" }
                        }
                    }
                }
            }
            if !clip.builtin {
                div { style: "display: flex; align-items: center; gap: 8px; padding-left: 44px;",
                    span { style: "font-size: 12px; color: {INK_3}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 170px;",
                        {[clip.patch.as_str(), clip.rig_preset.as_str()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" · ")}
                    }
                    input {
                        style: "{field}",
                        placeholder: "Gain (65%, 2 o'clock, 36 dB)",
                        value: "{gain}",
                        oninput: move |e| gain.set(e.value()),
                        onchange: move |_| save(r_gain.clone(), DiClip { gain: gain(), ..c_gain.clone() }),
                    }
                }
            }
        }
    }
}
