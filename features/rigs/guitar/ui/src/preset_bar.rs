//! Preset mode's sidebar: the presets (`tones.styx` — sounds made of a
//! Core, a Time and block presets, shared by every patch naming one). A
//! press puts one up on the bench, the session's patch for editing it: the
//! grid and the right sidebar edit it there, and ⋯ › Save to (or a
//! right-click here) keeps the edits in the preset, for every patch that
//! plays it.

use dioxus::prelude::*;

use signal_guitar_proto::rig::RigClient;

use crate::kit::{MenuItem, Picked};
use crate::theme::{FAINT, LINE, LIVE, LIVE_BG, MUTED, SIDEBAR, SIDEBAR_W, TEXT};

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

#[component]
pub fn PresetSidebar(revision: u64) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let popup_host = signal_widgets::PopupHost::try_use();
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

    rsx! {
        aside {
            style: "width: {SIDEBAR_W}; flex-shrink: 0; display: flex; flex-direction: column; min-height: 0; \
                    border-right: 1px solid {LINE}; background: {SIDEBAR}; color: {TEXT};",
            div { style: "display: flex; align-items: center; gap: 8px; padding: 12px 12px 10px 14px; border-bottom: 1px solid {LINE}; flex-shrink: 0;",
                div { style: "display: flex; flex-direction: column; gap: 2px; flex: 1 1 0; min-width: 0;",
                    span { style: "font-size: 9px; font-weight: 800; letter-spacing: 0.14em; color: {FAINT};", "PRESETS" }
                    span { style: "font-size: 18px; font-weight: 700; color: {TEXT}; white-space: nowrap; overflow: hidden;",
                        if up.is_empty() { "Pick one to edit" } else { "{up}" }
                    }
                }
                button {
                    style: "flex-shrink: 0; padding: 4px 10px; border-radius: 6px; border: 1px solid {LINE}; background: transparent; color: {TEXT}; font-size: 11px; font-weight: 700;",
                    title: "A new preset from what plays now",
                    onclick: {
                        let rig = rig.clone();
                        let names = names.clone();
                        move |e: MouseEvent| {
                            let rig = rig.clone();
                            crate::kit::context_menu(
                                popup_host,
                                &e,
                                vec![MenuItem::name("new", "New preset from what plays…", "Create", "", names.clone())],
                                EventHandler::new(move |p: Picked| {
                                    let name = p.text.trim().to_string();
                                    if !name.is_empty() {
                                        send(&rig, move |r| async move { let _ = r.new_tone(name).await; });
                                    }
                                }),
                            );
                        }
                    },
                    "+ New"
                }
            }
            div { style: "flex: 1 1 0; min-height: 0; overflow-y: scroll; padding: 6px 6px 12px 6px; display: flex; flex-direction: column; gap: 2px;",
                if rows.is_empty() {
                    div { style: "padding: 10px 8px; font-size: 12px; color: {FAINT}; line-height: 1.5;",
                        "No presets yet. Play a sound you like and press + New — or, on any patch, the right sidebar's Preset tab › ⋯ › Save as new preset."
                    }
                }
                for row in rows {
                    {
                        let on = row.name.eq_ignore_ascii_case(&up);
                        let used = row.used_by.len();
                        let (rig_edit, rig_menu) = (rig.clone(), rig.clone());
                        let names = names.clone();
                        let name = row.name.clone();
                        rsx! {
                            div { key: "{row.name}",
                                class: if on { "" } else { "hover:bg-accent/30" },
                                style: if on {
                                    format!("display: flex; flex-direction: column; gap: 1px; padding: 7px 10px; border-radius: 6px; cursor: pointer; background: {LIVE_BG}; border: 1px solid {LIVE};")
                                } else {
                                    "display: flex; flex-direction: column; gap: 1px; padding: 7px 10px; border-radius: 6px; cursor: pointer; border: 1px solid transparent;".to_string()
                                },
                                onclick: {
                                    let name = name.clone();
                                    move |_| {
                                        let name = name.clone();
                                        send(&rig_edit, move |r| async move { let _ = r.edit_tone(name).await; });
                                    }
                                },
                                // Right-click: keep the bench's edits in it,
                                // rename, copy, delete.
                                oncontextmenu: {
                                    let name = name.clone();
                                    let used_by = row.used_by.clone();
                                    move |e: MouseEvent| {
                                        e.prevent_default();
                                        let others: Vec<String> = names.iter().filter(|n| **n != name).cloned().collect();
                                        let items = vec![
                                            MenuItem::head(format!("Preset · {name}")),
                                            MenuItem::run("save", format!("Save what plays to “{name}”")),
                                            MenuItem::name("rename", "Rename…", "Rename", &name, others),
                                            MenuItem::name("duplicate", "Duplicate…", "Duplicate", format!("{name} 2"), names.clone()),
                                            MenuItem::delete("delete", "Delete", (!used_by.is_empty()).then(|| format!("In use: {}", used_by.join(", ")))),
                                        ];
                                        let (rig, name) = (rig_menu.clone(), name.clone());
                                        crate::kit::context_menu(popup_host, &e, items, EventHandler::new(move |p: Picked| {
                                            let (n, text) = (name.clone(), p.text.trim().to_string());
                                            match p.id {
                                                "save" => send(&rig, move |r| async move { let _ = r.save_tone(n).await; }),
                                                "rename" if !text.is_empty() => send(&rig, move |r| async move { let _ = r.rename_tone(n, text).await; }),
                                                "duplicate" if !text.is_empty() => send(&rig, move |r| async move { let _ = r.duplicate_tone(n, text).await; }),
                                                "delete" => send(&rig, move |r| async move { let _ = r.delete_tone(n).await; }),
                                                _ => {}
                                            }
                                        }));
                                    }
                                },
                                span { style: format!("font-size: 13px; font-weight: {}; color: {};", if on { 700 } else { 500 }, if on { TEXT } else { MUTED }), "{row.name}" }
                                div { style: "display: flex; gap: 6px; font-size: 10px; color: {FAINT};",
                                    if !row.core.is_empty() { span { "{row.core}" } }
                                    if used > 0 { span { if used == 1 { "· 1 patch" } else { "· {used} patches" } } }
                                }
                            }
                        }
                    }
                }
            }
            div { style: "flex-shrink: 0; padding: 8px 12px; border-top: 1px solid {LINE}; font-size: 10px; color: {FAINT}; line-height: 1.45;",
                "Edit the preset on the grid and in the right sidebar; right-click it here (or ⋯ › Save to in the Preset tab) to keep the edits for every patch that plays it."
            }
        }
    }
}
