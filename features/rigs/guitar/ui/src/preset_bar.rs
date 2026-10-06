//! Preset mode's sidebar: the presets (`tones.styx` — sounds made of a
//! Core, a Time and block presets, shared by every patch naming one). A
//! press puts one up on the bench, the session's patch for editing it: the
//! grid and the right sidebar edit it there, and ⋯ › Save to (or a
//! right-click here) keeps the edits in the preset, for every patch that
//! plays it.

use dioxus::prelude::*;

use signal_guitar_proto::rig::RigClient;

use crate::kit::{Button, ListRow, MenuItem, NamePrompt, PickOption, Picked, PresetBar};
use crate::theme::{FAINT, LINE, MUTED, SIDEBAR, SIDEBAR_W, T_BODY, TEXT};

/// Fire a rig call without waiting.
fn send<F, Fut>(rig: &Option<RigClient>, call: F)
where
    F: FnOnce(RigClient) -> Fut + 'static,
    Fut: std::future::Future<Output = ()> + 'static,
{
    if let Some(r) = rig.clone() {
        spawn(async move { call(r).await });
    }
}

/// One preset as the list shows it: its name, its Core, who plays it.
#[derive(Clone, PartialEq)]
struct Row {
    name: String,
    core: String,
    used_by: Vec<String>,
}

/// A preset's menu: keep what plays in it, rename, copy, delete — a delete
/// the rig would refuse (patches still play it) shown with who plays it.
fn tone_items(name: &str, names: &[String], used_by: &[String]) -> Vec<MenuItem> {
    let others: Vec<String> = names.iter().filter(|n| *n != name).cloned().collect();
    vec![
        MenuItem::head(format!("Preset · {name}")),
        MenuItem::run("save", format!("Save what plays to “{name}”")),
        MenuItem::name("rename", "Rename…", "Rename", name, others),
        MenuItem::name("duplicate", "Duplicate…", "Duplicate", format!("{name} 2"), names.to_vec()),
        MenuItem::delete("delete", "Delete", in_use(used_by)),
    ]
}

/// "In use: A, B +3 more" — the reason a delete is refused, short enough
/// for a menu.
fn in_use(used_by: &[String]) -> Option<String> {
    match used_by.len() {
        0 => None,
        n if n <= 3 => Some(format!("In use: {}", used_by.join(", "))),
        n => Some(format!("In use: {} +{} more", used_by[..2].join(", "), n - 2)),
    }
}

fn tone_act(rig: &Option<RigClient>, name: &str, p: Picked) {
    let (n, text) = (name.to_string(), p.text.trim().to_string());
    match p.id {
        "new" if !text.is_empty() => send(rig, move |r| async move { let _ = r.new_tone(text).await; }),
        "from_patches" => send(rig, |r| async move { let _ = r.presets_from_patches().await; }),
        "save" => send(rig, move |r| async move { let _ = r.save_tone(n).await; }),
        "rename" if !text.is_empty() => send(rig, move |r| async move { let _ = r.rename_tone(n, text).await; }),
        "duplicate" if !text.is_empty() => send(rig, move |r| async move { let _ = r.duplicate_tone(n, text).await; }),
        "delete" => send(rig, move |r| async move { let _ = r.delete_tone(n).await; }),
        _ => {}
    }
}

#[component]
pub fn PresetSidebar(revision: u64) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut rev = use_signal(|| revision);
    if *rev.peek() != revision {
        rev.set(revision);
    }
    let comp = use_resource({
        let rig = rig.clone();
        move || {
            let rig = rig.clone();
            let _ = rev();
            async move {
                match rig {
                    Some(r) => r.compositions().await.ok(),
                    None => None,
                }
            }
        }
    });
    let comp = comp.read().clone().flatten();
    // "+ New" / the empty state's button: the name field, in place.
    let mut naming = use_signal(|| false);
    let rows: Vec<Row> = comp
        .as_ref()
        .map(|c| {
            c.modules
                .iter()
                .filter(|m| m.module == "Preset")
                .map(|m| Row {
                    name: m.name.clone(),
                    core: m
                        .snapshot_info
                        .first()
                        .and_then(|i| i.modules.iter().find(|p| p.module == "Core"))
                        .map(|p| format!("{} · {}", p.preset, p.snapshot))
                        .unwrap_or_default(),
                    used_by: m.used_by.clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    let names: Vec<String> = rows.iter().map(|r| r.name.clone()).collect();
    let up = comp
        .as_ref()
        .and_then(|c| c.active_modules.iter().find(|m| m.module == "Preset").map(|m| m.preset.clone()))
        .unwrap_or_default();

    let up_row = rows.iter().find(|r| r.name.eq_ignore_ascii_case(&up)).cloned();
    // The header's menu: a new preset, then the one up on the bench.
    let mut header_menu = vec![
        MenuItem::name("new", "New preset from what plays…", "Create", "", names.clone()),
        MenuItem::run("from_patches", "Make presets from this profile's patches"),
    ];
    if let Some(r) = &up_row {
        header_menu.push(MenuItem::sep());
        header_menu.extend(tone_items(&r.name, &names, &r.used_by));
    }

    rsx! {
        aside {
            style: "width: {SIDEBAR_W}; flex-shrink: 0; display: flex; flex-direction: column; min-height: 0; \
                    border-right: 1px solid {LINE}; background: {SIDEBAR}; color: {TEXT};",
            // ── The preset on the bench: the same heading as the set's and
            // the profile's — name large, ‹ › to step, ⋯ to manage ──
            div { style: "display: flex; flex-direction: column; padding: 10px 12px 10px 14px; \
                          border-bottom: 1px solid {LINE}; flex-shrink: 0;",
                PresetBar {
                    label: "Presets",
                    name: up.clone(),
                    placeholder: "None on the bench",
                    sub: match rows.len() {
                        0 => String::new(),
                        1 => "1 preset".to_string(),
                        n => format!("{n} presets"),
                    },
                    large: true,
                    options: rows
                        .iter()
                        .map(|r| PickOption { label: r.name.clone(), note: r.core.clone(), live: r.name.eq_ignore_ascii_case(&up), ..Default::default() })
                        .collect::<Vec<_>>(),
                    on_pick: {
                        let rig = rig.clone();
                        let names = names.clone();
                        move |i: usize| {
                            if let Some(n) = names.get(i).cloned() {
                                send(&rig, move |r| async move { let _ = r.edit_tone(n).await; });
                            }
                        }
                    },
                    on_step: {
                        let rig = rig.clone();
                        let names = names.clone();
                        let at = names.iter().position(|n| n.eq_ignore_ascii_case(&up));
                        move |d: i32| {
                            let n = names.len() as i32;
                            if n == 0 {
                                return;
                            }
                            let to = at.map_or(0, |a| (a as i32 + d).rem_euclid(n)) as usize;
                            let name = names[to].clone();
                            send(&rig, move |r| async move { let _ = r.edit_tone(name).await; });
                        }
                    },
                    menu: header_menu,
                    on_menu: {
                        let rig = rig.clone();
                        let up = up.clone();
                        move |p: Picked| tone_act(&rig, &up, p)
                    },
                }
            }
            div { style: "flex: 1 1 0; min-height: 0; overflow-y: scroll; padding: 6px; display: flex; flex-direction: column; gap: 1px;",
                if naming() {
                    div { style: "padding: 4px 2px 8px;",
                        NamePrompt {
                            label: "Create",
                            initial: String::new(),
                            placeholder: "New preset name",
                            taken: names.clone(),
                            on_done: {
                                let rig = rig.clone();
                                move |n: Option<String>| {
                                    naming.set(false);
                                    if let Some(n) = n.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()) {
                                        send(&rig, move |r| async move { let _ = r.new_tone(n).await; });
                                    }
                                }
                            },
                        }
                    }
                }
                // Empty: what a preset is, and the one way to make one.
                if rows.is_empty() && !naming() {
                    div { style: "display: flex; flex-direction: column; align-items: flex-start; gap: 8px; padding: 14px 8px;",
                        span { style: "font-size: 13px; font-weight: 600; color: {MUTED};", "No presets yet" }
                        span { style: "font-size: {T_BODY}; line-height: 1.45; color: {FAINT};",
                            "A preset is a whole sound — Core, Time and blocks — that any patch in any profile can play. Start from the patches you have; they stay as they are."
                        }
                        Button {
                            label: "Make presets from this profile's patches",
                            primary: true,
                            onclick: {
                                let rig = rig.clone();
                                move |()| send(&rig, |r| async move { let _ = r.presets_from_patches().await; })
                            },
                        }
                        Button { label: "Save what plays as a preset", onclick: move |()| naming.set(true) }
                    }
                }
                for row in rows {
                    {
                        let on = row.name.eq_ignore_ascii_case(&up);
                        let used = row.used_by.len();
                        let name = row.name.clone();
                        let sub = match (row.core.is_empty(), used) {
                            (true, 0) => String::new(),
                            (false, 0) => row.core.clone(),
                            (true, 1) => "1 patch".to_string(),
                            (true, n) => format!("{n} patches"),
                            (false, 1) => format!("{} · 1 patch", row.core),
                            (false, n) => format!("{} · {n} patches", row.core),
                        };
                        rsx! {
                            ListRow {
                                key: "{row.name}",
                                title: row.name.clone(),
                                sub,
                                live: on,
                                onclick: {
                                    let rig = rig.clone();
                                    let name = name.clone();
                                    move |()| {
                                        let name = name.clone();
                                        send(&rig, move |r| async move { let _ = r.edit_tone(name).await; });
                                    }
                                },
                                menu: tone_items(&row.name, &names, &row.used_by),
                                on_menu: {
                                    let rig = rig.clone();
                                    move |p: Picked| tone_act(&rig, &name, p)
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}
