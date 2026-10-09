//! The captures on this device — the rig's own, the NAM library's
//! downloads, any copied into the app through the Files app — by tone; a
//! tap loads one into the slot, where it becomes a variation (an amp) or an
//! option (a drive) of that tone's preset.

use dioxus::prelude::*;
use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{CaptureImport, ImportOutcome, LocalCapture};

use super::tokens::*;

#[component]
pub fn CapturesPanel(slot: String) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut all = use_signal(|| None::<Vec<LocalCapture>>);
    let mut query = use_signal(String::new);
    let mut outcome = use_signal(|| None::<ImportOutcome>);
    let mut busy = use_signal(|| None::<String>);
    {
        let rig = rig.clone();
        use_hook(move || {
            spawn(async move {
                let list = match rig {
                    Some(r) => r.captures().await.unwrap_or_default(),
                    None => Vec::new(),
                };
                all.set(Some(list));
            });
        });
    }
    // This slot's kind (pedals for a drive, amps for an amp), and those
    // nothing says the kind of.
    let gears = super::tones::gears_for(&slot);
    let q = query().to_lowercase();
    // The slot's own kind first; then those nothing says the kind of.
    let mut list: Vec<LocalCapture> = all.read().iter().flatten().cloned().collect();
    list.sort_by_key(|c| c.gear.is_empty());
    let mut groups: Vec<(String, Vec<LocalCapture>)> = Vec::new();
    let mut known = 0;
    for c in list {
        if !c.gear.is_empty() && !gears.iter().any(|g| g.eq_ignore_ascii_case(&c.gear)) {
            continue;
        }
        if !q.is_empty() && !c.name.to_lowercase().contains(&q) && !c.group.to_lowercase().contains(&q) && !c.creator.to_lowercase().contains(&q) {
            continue;
        }
        if !c.gear.is_empty() {
            known += 1;
        }
        match groups.last_mut() {
            Some((g, list)) if *g == c.group && list[0].gear.is_empty() == c.gear.is_empty() => list.push(c),
            _ => groups.push((c.group.clone(), vec![c])),
        }
    }
    let first_unknown = groups.iter().position(|(_, l)| l[0].gear.is_empty()).unwrap_or(usize::MAX);
    let loading = all.read().is_none();
    let row = "width: 100%; display: flex; align-items: center; gap: 12px; min-height: 52px; padding: 6px 14px; border: none; border-bottom: 1px solid #ffffff0d; background: transparent; color: #f4f4f5; text-align: left; font-family: inherit; cursor: pointer;";

    rsx! {
        div { style: "flex: 1; min-height: 0; display: flex; flex-direction: column; font-family: {FONT}; color: {INK};",
            if let Some(o) = outcome() {
                div { style: "flex-shrink: 0; display: flex; align-items: center; gap: 8px; padding: 10px 14px; background: {super::tokens::pick(o.ok, \"#14532d\", \"#7f1d1d\")};",
                    span { style: "flex: 1; font-size: 14px; font-weight: 650;",
                        if o.ok { "{o.preset} · {o.variation} → {o.slot}" } else { "{o.message}" }
                    }
                    button { style: "width: 32px; height: 32px; border: none; background: transparent; cursor: pointer;", onclick: move |_| outcome.set(None),
                        svg { width: "12", height: "12", view_box: "0 0 12 12",
                            path { d: "M2 2l8 8M10 2l-8 8", stroke: INK, stroke_width: "1.6", stroke_linecap: "round" }
                        }
                    }
                }
            }
            div { style: "flex-shrink: 0; padding: 10px 12px; border-bottom: 1px solid {RULE};",
                label { style: "display: flex; align-items: center; gap: 8px; height: 42px; padding: 0 12px; border-radius: {R}; background: {FILL};",
                    svg { width: "15", height: "15", view_box: "0 0 16 16",
                        circle { cx: "7", cy: "7", r: "5", fill: "none", stroke: INK_3, stroke_width: "1.6" }
                        path { d: "M11 11l3.5 3.5", stroke: INK_3, stroke_width: "1.6", stroke_linecap: "round" }
                    }
                    input {
                        style: "flex: 1; min-width: 0; height: 40px; border: none; outline: none; background: transparent; color: {INK}; font-size: 16px; font-family: {FONT};",
                        placeholder: "Search captures",
                        value: "{query}",
                        oninput: move |e| query.set(e.value()),
                    }
                }
            }
            div { style: "flex: 1; min-height: 0; overflow-y: auto;",
                if loading {
                    div { style: "padding: 24px; text-align: center; color: {INK_3}; font-size: 14px;", "…" }
                } else if groups.is_empty() {
                    div { style: "padding: 24px; text-align: center; color: {INK_3}; font-size: 14px;", "No captures" }
                }
                for (k, (group, list)) in groups.into_iter().enumerate() {
                    div { key: "{k}{group}",
                        if known > 0 && list[0].gear.is_empty() && k > 0 && first_unknown == k {
                            div { style: "padding: 18px 14px 4px; font-size: 11px; font-weight: 800; letter-spacing: 0.08em; color: {INK_3};", "OTHER CAPTURES" }
                        }
                        if list.len() > 1 {
                        div { style: "position: sticky; top: 0; display: flex; align-items: baseline; gap: 8px; padding: 12px 14px 6px; background: {SHEET};",
                            span { style: "flex: 1; min-width: 0; font-size: 13px; font-weight: 800; letter-spacing: 0.04em; color: {INK_2}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{group}" }
                            span { style: "font-size: 12px; color: {INK_3};", "{list.len()}" }
                        }
                        }
                        for c in list.clone() {
                            {
                                let (rig, slot, c2) = (rig.clone(), slot.clone(), c.clone());
                                let mine = busy().as_deref() == Some(c.path.as_str());
                                let indent = if list.len() > 1 { 26 } else { 14 };
                                rsx! {
                                    button {
                                        key: "{c.path}",
                                        style: "{row} padding-left: {indent}px;",
                                        onclick: move |_| {
                                            let (rig, slot, c) = (rig.clone(), slot.clone(), c2.clone());
                                            busy.set(Some(c.path.clone()));
                                            outcome.set(None);
                                            spawn(async move {
                                                if let Some(r) = rig {
                                                    let import = CaptureImport { name: c.name, path: c.path, gear: c.gear, group: c.group, slot };
                                                    match r.load_capture(import).await {
                                                        Ok(o) => outcome.set(Some(o)),
                                                        Err(e) => outcome.set(Some(ImportOutcome { ok: false, message: format!("{e:?}"), ..ImportOutcome::default() })),
                                                    }
                                                }
                                                busy.set(None);
                                            });
                                        },
                                        div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px;",
                                            span { style: "font-size: 15px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{c.name}" }
                                            if !c.creator.is_empty() {
                                                span { style: "font-size: 12px; color: {INK_3};", "{c.creator}" }
                                            }
                                        }
                                        span { style: "font-size: 13px; color: {INK_3};", if mine { "…" } else { "Load" } }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
