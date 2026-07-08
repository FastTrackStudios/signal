//! The workbench sidebars.
//!
//! Left: the preset browser (every patch in the loaded profile, grouped by
//! stack color, click to load) over the profile list. Right: the current
//! song's sections on top, setlist management (jump + reorder) beneath.
//! Both consume the `RigClient` from context and render from the pushed
//! [`PerformanceModel`], so they work identically on desktop and web.

use dioxus::prelude::*;

use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{PatchInfo, PerformanceModel, PresetInfo};

use crate::perform::folder_color;

/// Section eyebrow shared by every sidebar group.
#[component]
fn PanelLabel(label: &'static str) -> Element {
    rsx! {
        div { class: "px-3 py-2 border-b border-border flex-shrink-0",
            h3 { class: "text-[10px] font-semibold text-muted-foreground uppercase tracking-wider",
                "{label}"
            }
        }
    }
}

/// Left sidebar — the profile tree over the preset pool.
///
/// Top: Profile → Stacks → Patches (organize the rig). Bottom: the preset
/// pool patches point at. Select a patch in the tree, then click a preset
/// to point the patch at it (the core rebuilds that patch's chain).
#[component]
pub fn LeftSidebar(model: PerformanceModel) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);

    // Re-fetch patches + presets whenever the performance model changes
    // (patch switches flip `active`; repoints change the pointers).
    let mut rev = use_signal(|| 0u64);
    let mut last = use_signal(|| None::<PerformanceModel>);
    if last.read().as_ref() != Some(&model) {
        last.set(Some(model.clone()));
        rev += 1;
    }
    let data = use_resource({
        let rig = rig.clone();
        move || {
            let _ = rev();
            let rig = rig.clone();
            async move {
                match rig {
                    Some(r) => (
                        r.patches().await.unwrap_or_default(),
                        r.presets().await.unwrap_or_default(),
                    ),
                    None => (Vec::new(), Vec::new()),
                }
            }
        }
    });
    let (patch_list, preset_list): (Vec<PatchInfo>, Vec<PresetInfo>) =
        data.read().clone().unwrap_or_default();

    // The patch picked in the tree — the target of a preset click.
    let mut selected_patch = use_signal(|| None::<usize>);
    let selected_preset_name = selected_patch()
        .and_then(|i| patch_list.get(i))
        .map(|p| p.preset.clone());

    // Group patches by stack, in the stacks' own order.
    let mut groups: Vec<(String, Vec<(usize, PatchInfo)>)> = model
        .stacks
        .iter()
        .map(|s| (s.name.clone(), Vec::new()))
        .collect();
    let mut loose: Vec<(usize, PatchInfo)> = Vec::new();
    for (i, p) in patch_list.iter().enumerate() {
        match groups.iter_mut().find(|(name, _)| name.eq_ignore_ascii_case(&p.stack)) {
            Some((_, v)) => v.push((i, p.clone())),
            None => loose.push((i, p.clone())),
        }
    }
    if !loose.is_empty() {
        groups.push(("Unassigned".to_string(), loose));
    }

    rsx! {
        aside { class: "w-64 flex-shrink-0 flex flex-col border-r border-border bg-card min-h-0",
            // ── The profile tree: Profile → Stacks → Patches ──
            PanelLabel { label: "Profile" }
            div { class: "flex-1 overflow-y-auto min-h-0 p-2 flex flex-col gap-0.5",
                div { class: "flex items-center gap-2 rounded-md px-2 py-1.5 text-sm font-bold",
                    span { class: "w-2 h-2 rounded-full bg-current opacity-60" }
                    if model.profile_name.is_empty() { "— no profile —" } else { "{model.profile_name}" }
                }
                for (stack_name, patches) in groups.iter() {
                    {
                        let (dot, _) = folder_color(stack_name);
                        let stack_label = stack_name.clone();
                        rsx! {
                            div { class: "flex items-center gap-2 pl-4 pr-2 pt-2 pb-0.5",
                                span { class: "w-2 h-2 rounded-full flex-shrink-0", style: "background-color: {dot};" }
                                span { class: "text-[10px] font-semibold uppercase tracking-wider text-muted-foreground",
                                    "{stack_label}"
                                }
                            }
                            for (i, p) in patches.iter() {
                                {
                                    let i = *i;
                                    let name = p.name.clone();
                                    let preset = p.preset.clone();
                                    let is_default = p.default_in_stack;
                                    let is_sel = selected_patch() == Some(i);
                                    rsx! {
                                        button {
                                            key: "{i}",
                                            class: if p.active {
                                                "flex items-center gap-2 rounded-md ml-4 px-2 py-1 text-left text-sm font-bold bg-accent text-accent-foreground"
                                            } else if is_sel {
                                                "flex items-center gap-2 rounded-md ml-4 px-2 py-1 text-left text-sm ring-1 ring-ring text-foreground"
                                            } else {
                                                "flex items-center gap-2 rounded-md ml-4 px-2 py-1 text-left text-sm text-foreground hover:bg-accent/40"
                                            },
                                            onclick: {
                                                let rig = rig.clone();
                                                move |_| {
                                                    selected_patch.set(Some(i));
                                                    if let Some(r) = rig.clone() {
                                                        spawn(async move { let _ = r.select_patch(i as u32).await; });
                                                    }
                                                }
                                            },
                                            span { class: "truncate", "{name}" }
                                            // The stack's default — where the
                                            // footswitch lands after a reset.
                                            if is_default {
                                                span { class: "text-[9px] opacity-60 flex-shrink-0",
                                                    title: "stack default",
                                                    "★"
                                                }
                                            }
                                            span { class: "ml-auto text-[9px] font-mono opacity-50 truncate max-w-[80px] flex-shrink-0",
                                                "{preset}"
                                            }
                                            if !p.override_modules.is_empty() {
                                                span {
                                                    class: "text-[9px] opacity-70 flex-shrink-0",
                                                    title: "overrides: {p.override_modules.join(\", \")}",
                                                    {p.override_modules.iter().map(|m| crate::icons::module_icon(m)).collect::<String>()}
                                                }
                                            }
                                            if !p.available {
                                                span { class: "w-1.5 h-1.5 rounded-full flex-shrink-0",
                                                    style: "background-color: #fde047;" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ── The preset pool ──
            PanelLabel { label: "Presets" }
            if let Some(i) = selected_patch() {
                if let Some(p) = patch_list.get(i) {
                    div { class: "px-3 py-1 text-[10px] text-muted-foreground flex-shrink-0",
                        "click a preset to assign it to "
                        span { class: "font-bold text-foreground", "{p.name}" }
                    }
                }
            }
            div { class: "overflow-y-auto min-h-0 max-h-[40%] p-2 flex flex-col gap-0.5 flex-shrink-0",
                for (i, preset) in preset_list.iter().enumerate() {
                    {
                        let name = preset.name.clone();
                        let used = preset.used_by;
                        let is_target = selected_preset_name.as_deref() == Some(preset.name.as_str());
                        rsx! {
                            button {
                                key: "{i}",
                                class: if preset.active {
                                    "flex items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm font-bold bg-accent text-accent-foreground"
                                } else if is_target {
                                    "flex items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm ring-1 ring-ring text-foreground"
                                } else {
                                    "flex items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm text-foreground hover:bg-accent/40"
                                },
                                onclick: {
                                    let rig = rig.clone();
                                    move |_| {
                                        if let (Some(r), Some(patch)) = (rig.clone(), selected_patch()) {
                                            spawn(async move {
                                                let _ = r.set_patch_preset(patch as u32, i as u32).await;
                                            });
                                        }
                                    }
                                },
                                span { class: "w-2 h-2 rounded-full flex-shrink-0 bg-current opacity-50" }
                                span { class: "truncate", "{name}" }
                                span { class: "ml-auto text-[9px] font-mono opacity-50 flex-shrink-0", "×{used}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Right sidebar: the current song's sections on top, setlist management
/// (jump + reorder) beneath.
#[component]
pub fn RightSidebar(model: PerformanceModel) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let current_song = model
        .songs
        .get(model.song_index as usize)
        .map(|s| s.name.clone())
        .unwrap_or_default();

    rsx! {
        aside { class: "w-64 flex-shrink-0 flex flex-col border-l border-border bg-card min-h-0",
            // ── Song sections ──
            PanelLabel { label: "Song Sections" }
            div { class: "px-3 pt-2 text-sm font-bold truncate flex-shrink-0", "{current_song}" }
            div { class: "grid grid-cols-2 gap-1.5 p-2 flex-shrink-0",
                for (i, section) in model.sections.iter().enumerate() {
                    {
                        let name = section.clone();
                        let is_current = i == model.section_index as usize;
                        rsx! {
                            button {
                                key: "{i}",
                                class: if is_current {
                                    "rounded-md px-2 py-2 text-xs font-bold bg-accent text-accent-foreground"
                                } else {
                                    "rounded-md px-2 py-2 text-xs text-muted-foreground border border-border hover:bg-accent/40"
                                },
                                onclick: {
                                    let rig = rig.clone();
                                    move |_| {
                                        if let Some(r) = rig.clone() {
                                            spawn(async move { let _ = r.select_section(i as u32).await; });
                                        }
                                    }
                                },
                                "{name}"
                            }
                        }
                    }
                }
            }

            // ── Setlist management ──
            PanelLabel { label: "Setlist" }
            // Which set: XR / CYA / … — switching recalls its first song.
            div { class: "flex gap-1 px-2 pt-1 flex-wrap",
                for (si, set) in model.setlists.iter().enumerate() {
                    {
                        let set = set.clone();
                        let active = si == model.setlist_index as usize;
                        let rig = rig.clone();
                        rsx! {
                            button {
                                key: "{si}",
                                class: if active {
                                    "rounded px-1.5 py-0.5 text-[10px] font-bold bg-accent text-accent-foreground"
                                } else {
                                    "rounded px-1.5 py-0.5 text-[10px] text-muted-foreground border border-border hover:bg-accent/40"
                                },
                                onclick: move |_| {
                                    if let Some(r) = rig.clone() {
                                        spawn(async move { let _ = r.select_setlist(si as u32).await; });
                                    }
                                },
                                "{set}"
                            }
                        }
                    }
                }
            }
            div { class: "flex-1 overflow-y-auto min-h-0 p-2 flex flex-col gap-0.5",
                for (i, song) in model.songs.iter().enumerate() {
                    {
                        let name = song.name.clone();
                        let meta = format!("{} · {}", song.key, song.bpm);
                        let is_current = i == model.song_index as usize;
                        let count = model.songs.len();
                        rsx! {
                            div {
                                key: "{i}",
                                class: if is_current {
                                    "group flex items-center gap-1 rounded-md px-2 py-1 bg-accent text-accent-foreground"
                                } else {
                                    "group flex items-center gap-1 rounded-md px-2 py-1 text-muted-foreground hover:bg-accent/40"
                                },
                                button {
                                    class: "flex items-center gap-2 flex-1 min-w-0 text-left text-sm",
                                    onclick: {
                                        let rig = rig.clone();
                                        move |_| {
                                            if let Some(r) = rig.clone() {
                                                spawn(async move { let _ = r.select_song(i as u32).await; });
                                            }
                                        }
                                    },
                                    span { class: "font-mono text-[10px] opacity-60 w-4 flex-shrink-0", "{i + 1}" }
                                    span { class: if is_current { "truncate font-bold" } else { "truncate" }, "{name}" }
                                    span { class: "ml-auto font-mono text-[9px] opacity-60 flex-shrink-0", "{meta}" }
                                }
                                // Reorder — visible on hover so the list stays calm.
                                div { class: "flex flex-col opacity-0 group-hover:opacity-100 flex-shrink-0",
                                    button {
                                        class: "text-[9px] leading-none px-1 hover:text-foreground disabled:opacity-20",
                                        disabled: i == 0,
                                        onclick: {
                                            let rig = rig.clone();
                                            move |_| {
                                                if i > 0 {
                                                    if let Some(r) = rig.clone() {
                                                        spawn(async move { let _ = r.move_song(i as u32, (i - 1) as u32).await; });
                                                    }
                                                }
                                            }
                                        },
                                        "▲"
                                    }
                                    button {
                                        class: "text-[9px] leading-none px-1 hover:text-foreground disabled:opacity-20",
                                        disabled: i + 1 >= count,
                                        onclick: {
                                            let rig = rig.clone();
                                            move |_| {
                                                if i + 1 < count {
                                                    if let Some(r) = rig.clone() {
                                                        spawn(async move { let _ = r.move_song(i as u32, (i + 1) as u32).await; });
                                                    }
                                                }
                                            }
                                        },
                                        "▼"
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
