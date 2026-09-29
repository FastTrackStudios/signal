//! **Perform strip** — the footswitch row at the bottom of every view, built
//! from the same switch pieces as the guitar rig's grid
//! (`signal_widgets::switches`): tap to recall, right-click to set a stack
//! up or MIDI-learn a pedal onto it, and the Switches toggle (Full /
//! Compact / Hidden) in the strip's header.
//!
//! A keys stack is a *scene*: pressing "Verse" rides every layer to that
//! stack's levels (and loads any patch the scene pins). Level-only recalls
//! are instant — the mixer's live cells, no audio gap.

use dioxus::prelude::*;
use signal_keys_proto::KeysPerform;
use signal_keys_proto::keys::KeysRigClient;
use signal_widgets::switches::{
    MenuRow, MidiLearnRows, SwitchMenuFrame, SwitchTile, SwitchesMode, SwitchesToggle,
};

/// Per-stack color — the worship set's shape, left to right: intimate →
/// full → intimate again. A stack of the player's own gets a colour from
/// its name.
#[must_use]
pub fn stack_color(name: &str) -> (&'static str, &'static str) {
    match name {
        "Spotlight" => ("#1e3a5f", "#7dd3fc"),
        "Verse" => ("#14324a", "#67e8f9"),
        "Energy" => ("#3b2708", "#fde047"),
        "Hooks" => ("#3f1d38", "#f0abfc"),
        "Underscore" => ("#14321e", "#86efac"),
        "" => ("#1c1c20", "#a1a1aa"),
        _ => {
            const OWN: [(&str, &str); 5] = [
                ("#2a1f4a", "#c4b5fd"),
                ("#3a1f1f", "#fca5a5"),
                ("#1f3a36", "#5eead4"),
                ("#3a2f1f", "#fdba74"),
                ("#1f2a3a", "#93c5fd"),
            ];
            let h = name
                .bytes()
                .fold(0usize, |h, b| h.wrapping_mul(31).wrapping_add(usize::from(b)));
            OWN[h % OWN.len()]
        }
    }
}

/// Call `f` on the rig client, off the render.
fn with_rig<F, Fut>(rig: &Option<KeysRigClient>, f: F)
where
    F: FnOnce(KeysRigClient) -> Fut + 'static,
    Fut: std::future::Future<Output = ()> + 'static,
{
    if let Some(r) = rig.clone() {
        spawn(async move { f(r).await });
    }
}

/// The stack row + the perform-mode selector + the Switches toggle.
#[component]
pub fn PerformStrip(perform: KeysPerform) -> Element {
    let rig = use_hook(try_consume_context::<KeysRigClient>);
    let switches = use_signal(SwitchesMode::default);
    // Which switch's menu is open (by target), and a rename in progress.
    let mut menu = use_signal(|| None::<String>);
    let mut renaming = use_signal(|| None::<(u32, String)>);
    let mode = switches();
    let compact = mode == SwitchesMode::Compact;
    let learn = perform.learn.clone();
    let tile_height = if compact { "48px" } else { "88px" };

    let learn_rows = |target: String| {
        let (r1, r2, r3) = (rig.clone(), rig.clone(), rig.clone());
        rsx! {
            MidiLearnRows {
                target: target.clone(),
                binding: learn.binding(&target).map(str::to_string),
                learning: learn.is_learning(&target),
                on_learn: move |t: String| with_rig(&r1, move |r| async move { let _ = r.midi_learn(t).await; }),
                on_cancel: move |()| with_rig(&r2, |r| async move { let _ = r.midi_learn_cancel().await; }),
                on_unlearn: move |t: String| with_rig(&r3, move |r| async move { let _ = r.midi_unlearn(t).await; }),
                on_close: move |()| menu.set(None),
            }
        }
    };

    rsx! {
        div {
            style: "display: flex; flex-direction: column; gap: 6px; padding: 8px 12px; \
                    border-top: 1px solid #1c1c1f; background: #0a0a0c;",
            // The strip's header: the Switches toggle, the mode (Preset
            // browses the library, Profile plays the stacks, Setlist follows
            // the song), a pedal being learned, the tempo.
            div { style: "display: flex; align-items: center; gap: 6px;",
                SwitchesToggle { mode: switches,
                    fts_chrome::Glyph { icon: fts_chrome::Icon::Perform, size: 13 }
                }
                for (m, label) in [(0u32, "Preset"), (1, "Profile"), (2, "Setlist")] {
                    button {
                        key: "{label}",
                        style: format!(
                            "appearance: none; border: none; border-radius: 6px; padding: 3px 10px; \
                             font-size: 10px; font-weight: 700; letter-spacing: 0.06em; background: {}; color: {};",
                            if perform.perform_mode == m { "#101821" } else { "transparent" },
                            if perform.perform_mode == m { "#38bdf8" } else { "#52525b" },
                        ),
                        onclick: {
                            let rig = rig.clone();
                            move |_| with_rig(&rig, move |r| async move { let _ = r.set_perform_mode(m).await; })
                        },
                        "{label}"
                    }
                }
                div { style: "flex: 1;" }
                if let Some(t) = learn.learning.clone() {
                    // Waiting for a pedal: say so where the player looks.
                    span { style: "font-size: 10px; font-weight: 700; color: #f59e0b;",
                        "MIDI learn: press a pedal or pad for {switch_title(&t, &perform)}"
                    }
                    button {
                        style: "appearance: none; border: 1px solid #3f3f46; border-radius: 6px; padding: 2px 8px; \
                                font-size: 10px; background: transparent; color: #d4d4d8;",
                        onclick: {
                            let rig = rig.clone();
                            move |_| with_rig(&rig, |r| async move { let _ = r.midi_learn_cancel().await; })
                        },
                        "Cancel"
                    }
                }
                // The band's tempo (shared with the guitar rig's tap) — what
                // the synced delays follow.
                span { style: "font-size: 11px; font-weight: 700; color: #a1a1aa; font-variant-numeric: tabular-nums;",
                    if perform.tempo_bpm > 0 { "{perform.tempo_bpm} BPM" } else { "— BPM" }
                }
                span { style: "font-size: 10px; color: #52525b;", "{perform.profile_name}" }
            }
            if mode != SwitchesMode::Hidden {
                // The footswitch row: the stacks, Tap, and a new stack.
                div { style: "display: flex; gap: 8px; height: {tile_height};",
                    for (i, stack) in perform.stacks.iter().enumerate() {
                        {
                            let (bg, fg) = stack_color(&stack.name);
                            let target = format!("stack:{i}");
                            let idx = i as u32;
                            let name = stack.name.clone();
                            let press = {
                                let rig = rig.clone();
                                Callback::new(move |(): ()| with_rig(&rig, move |r| async move { let _ = r.press_stack(idx).await; }))
                            };
                            let footer = if stack.tempo_bpm > 0 { format!("{} BPM", stack.tempo_bpm) } else { String::new() };
                            let open = menu().as_deref() == Some(target.as_str());
                            let rename_now = renaming().filter(|(r, _)| *r == idx).map(|(_, n)| n);
                            let (rc, rr, rd, ra) = (rig.clone(), rig.clone(), rig.clone(), rig.clone());
                            let rr2 = rr.clone();
                            let t_menu = target.clone();
                            rsx! {
                                SwitchTile {
                                    key: "{i}-{stack.name}",
                                    no: i + 1,
                                    label: stack.name.to_uppercase(),
                                    detail: stack.blurb.clone(),
                                    footer,
                                    colors: (bg.to_string(), fg.to_string()),
                                    active: stack.is_active,
                                    compact,
                                    binding: learn.binding(&target).map(str::to_string),
                                    learning: learn.is_learning(&target),
                                    on_tap: press,
                                    on_menu: move |()| menu.set(Some(t_menu.clone())),
                                    on_leave: move |()| {
                                        // A rename in progress keeps its menu.
                                        if renaming().is_none() {
                                            menu.set(None);
                                        }
                                    },
                                    menu_open: open,
                                    menu: rsx! {
                                        SwitchMenuFrame { title: format!("Switch {} · {name}", i + 1),
                                            if let Some(text) = rename_now {
                                                div { style: "display: flex; gap: 4px; padding: 4px 6px;",
                                                    input {
                                                        style: "flex: 1; min-width: 0; padding: 4px 6px; border-radius: 6px; \
                                                                border: 1px solid #3f3f46; background: #111114; color: #e4e4e7; font-size: 11px;",
                                                        value: "{text}",
                                                        autofocus: true,
                                                        oninput: move |e: FormEvent| renaming.set(Some((idx, e.value()))),
                                                        onkeydown: {
                                                            let text = text.clone();
                                                            move |e: KeyboardEvent| {
                                                                if e.key() == Key::Enter {
                                                                    let t = text.clone();
                                                                    with_rig(&rr, move |r| async move { let _ = r.rename_stack(idx, t).await; });
                                                                    renaming.set(None);
                                                                    menu.set(None);
                                                                } else if e.key() == Key::Escape {
                                                                    renaming.set(None);
                                                                }
                                                            }
                                                        },
                                                    }
                                                    button {
                                                        style: "appearance: none; border: none; border-radius: 6px; padding: 4px 8px; \
                                                                font-size: 10px; font-weight: 700; background: #1d4ed8; color: #fff;",
                                                        onclick: {
                                                            let text = text.clone();
                                                            move |_| {
                                                                let t = text.clone();
                                                                with_rig(&rr2, move |r| async move { let _ = r.rename_stack(idx, t).await; });
                                                                renaming.set(None);
                                                                menu.set(None);
                                                            }
                                                        },
                                                        "Save"
                                                    }
                                                }
                                            } else {
                                                MenuRow {
                                                    label: "Save the mix into this stack",
                                                    mark: "●",
                                                    onclick: move |()| {
                                                        with_rig(&rc, move |r| async move { let _ = r.capture_stack(idx).await; });
                                                        menu.set(None);
                                                    },
                                                }
                                                MenuRow {
                                                    label: "Rename…",
                                                    mark: "✎",
                                                    mark_color: "#a1a1aa".to_string(),
                                                    onclick: {
                                                        let name = name.clone();
                                                        move |()| renaming.set(Some((idx, name.clone())))
                                                    },
                                                }
                                                MenuRow {
                                                    label: "New stack from the mix",
                                                    mark: "+",
                                                    mark_color: "#38bdf8".to_string(),
                                                    onclick: move |()| {
                                                        with_rig(&ra, |r| async move { let _ = r.add_stack(String::new()).await; });
                                                        menu.set(None);
                                                    },
                                                }
                                                MenuRow {
                                                    label: "Delete stack",
                                                    danger: true,
                                                    onclick: move |()| {
                                                        with_rig(&rd, move |r| async move { let _ = r.delete_stack(idx).await; });
                                                        menu.set(None);
                                                    },
                                                }
                                                {learn_rows(target.clone())}
                                            }
                                        }
                                    },
                                }
                            }
                        }
                    }
                    // Tap tempo — a switch like the others, so a pedal can
                    // be learned onto it.
                    {
                        let tap = {
                            let rig = rig.clone();
                            Callback::new(move |(): ()| with_rig(&rig, |r| async move { let _ = r.tap_tempo().await; }))
                        };
                        let footer = if perform.tempo_bpm > 0 { format!("{} BPM", perform.tempo_bpm) } else { String::new() };
                        rsx! {
                            SwitchTile {
                                key: "tap-{perform.stacks.len()}",
                                no: perform.stacks.len() + 1,
                                label: "TAP",
                                detail: "the band's tempo".to_string(),
                                footer,
                                colors: ("#27272a".to_string(), "#e4e4e7".to_string()),
                                active: true,
                                compact,
                                binding: learn.binding("tap").map(str::to_string),
                                learning: learn.is_learning("tap"),
                                on_tap: tap,
                                on_menu: move |()| menu.set(Some("tap".into())),
                                on_leave: move |()| menu.set(None),
                                menu_open: menu().as_deref() == Some("tap"),
                                menu: rsx! {
                                    SwitchMenuFrame { title: "Tap tempo".to_string(),
                                        {learn_rows("tap".to_string())}
                                    }
                                },
                            }
                        }
                    }
                    // A new stack, holding the mix as it is now.
                    button {
                        style: "flex: 0 0 44px; appearance: none; border: 1px dashed #3f3f46; border-radius: 12px; \
                                background: transparent; color: #71717a; font-size: 20px;",
                        title: "New stack from the mix",
                        onclick: {
                            let rig = rig.clone();
                            move |_| with_rig(&rig, |r| async move { let _ = r.add_stack(String::new()).await; })
                        },
                        "+"
                    }
                }
            }
        }
    }
}

/// A learn target as the player knows it: "Verse", "Tap".
fn switch_title(target: &str, perform: &KeysPerform) -> String {
    if target == "tap" {
        return "Tap".into();
    }
    target
        .strip_prefix("stack:")
        .and_then(|i| i.parse::<usize>().ok())
        .and_then(|i| perform.stacks.get(i))
        .map_or_else(|| target.to_string(), |s| s.name.clone())
}
