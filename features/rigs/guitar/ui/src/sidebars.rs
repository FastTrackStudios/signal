//! The workbench sidebars.
//!
//! Left: the preset browser (every patch in the loaded profile, grouped by
//! stack color, click to load) over the profile list. Right: the current
//! song's sections on top, setlist management (jump + reorder) beneath.
//! Both consume the `RigClient` from context and render from the pushed
//! [`PerformanceModel`], so they work identically on desktop and web.

use dioxus::prelude::*;
use signal_widgets::Picker;

use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{PatchInfo, PerformanceModel, PresetInfo};

use crate::perform::folder_color;

/// Section eyebrow shared by every sidebar group.

// Profile-list row layout, as inline styles Blitz honours (see the
// `blitz-design` skill): the name takes the slack and clips; the preset is a
// fixed right-aligned cell that clips (no `text-overflow` in Blitz).
const PATCH_ROW: &str = "display: flex; align-items: center; gap: 8px; min-width: 0; \
                         margin-left: 16px; padding: 4px 8px; text-align: left; cursor: pointer;";
const NAME_CELL: &str = "flex: 1 1 0; min-width: 0; overflow: hidden; white-space: nowrap; text-align: left;";
const PRESET_CELL: &str = "flex-shrink: 0; width: 76px; overflow: hidden; white-space: nowrap; text-align: right;";

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
    // Creation forms (toggled by the + buttons) + inline rename state.
    let mut adding_stack = use_signal(|| false);
    let mut adding_patch = use_signal(|| false);
    let mut adding_preset = use_signal(|| false);
    let mut new_name = use_signal(String::new);
    let mut new_path = use_signal(String::new);
    let mut new_stack_sel = use_signal(String::new);
    let mut new_preset_sel = use_signal(String::new);
    // (kind, original) — kind: "patch" | "preset"; the row shows an input.
    let mut renaming = use_signal(|| None::<(String, String)>);
    let mut rename_text = use_signal(String::new);
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
        match groups
            .iter_mut()
            .find(|(name, _)| name.eq_ignore_ascii_case(&p.stack))
        {
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
            div { class: "flex items-center pr-2",
                PanelLabel { label: "Profile" }
                button {
                    class: "ml-auto text-[10px] px-1 rounded border border-border text-muted-foreground hover:text-foreground",
                    title: "New stack",
                    onclick: move |_| { adding_stack.toggle(); adding_patch.set(false); new_name.set(String::new()); },
                    "+ stack"
                }
                button {
                    class: "ml-1 text-[10px] px-1 rounded border border-border text-muted-foreground hover:text-foreground",
                    title: "New patch",
                    onclick: move |_| { adding_patch.toggle(); adding_stack.set(false); new_name.set(String::new()); },
                    "+ patch"
                }
            }
            if adding_stack() {
                div { class: "flex items-center gap-1 px-2 py-1 flex-shrink-0",
                    input {
                        class: "flex-1 min-w-0 bg-background border border-border rounded px-1.5 py-0.5 text-xs",
                        placeholder: "Stack name",
                        value: "{new_name}",
                        oninput: move |e| new_name.set(e.value()),
                    }
                    button {
                        class: "text-xs px-1.5 rounded border border-border hover:bg-accent/40",
                        onclick: {
                            let rig = rig.clone();
                            move |_| {
                                let name = new_name.peek().clone();
                                if let (Some(r), false) = (rig.clone(), name.trim().is_empty()) {
                                    spawn(async move { let _ = r.add_stack(name).await; });
                                    adding_stack.set(false);
                                }
                            }
                        },
                        "add"
                    }
                }
            }
            if adding_patch() {
                div { class: "flex flex-col gap-1 px-2 py-1 flex-shrink-0",
                    input {
                        class: "bg-background border border-border rounded px-1.5 py-0.5 text-xs",
                        placeholder: "Patch name",
                        value: "{new_name}",
                        oninput: move |e| new_name.set(e.value()),
                    }
                    div { class: "flex gap-1",
                        div { class: "flex-1 min-w-0",
                            Picker {
                                options: model.stacks.iter().map(|st| st.name.clone()).collect::<Vec<String>>(),
                                selected: name_index(&model.stacks.iter().map(|st| st.name.clone()).collect::<Vec<String>>(), &new_stack_sel()),
                                placeholder: "stack…".to_string(),
                                width: "100%".to_string(),
                                on_select: {
                                    let names: Vec<String> = model.stacks.iter().map(|st| st.name.clone()).collect();
                                    move |i: u32| {
                                        if let Some(n) = names.get(i as usize) {
                                            new_stack_sel.set(n.clone());
                                        }
                                    }
                                },
                            }
                        }
                        div { class: "flex-1 min-w-0",
                            Picker {
                                options: preset_list.iter().map(|p| p.name.clone()).collect::<Vec<String>>(),
                                selected: name_index(&preset_list.iter().map(|p| p.name.clone()).collect::<Vec<String>>(), &new_preset_sel()),
                                placeholder: "preset…".to_string(),
                                width: "100%".to_string(),
                                on_select: {
                                    let names: Vec<String> = preset_list.iter().map(|p| p.name.clone()).collect();
                                    move |i: u32| {
                                        if let Some(n) = names.get(i as usize) {
                                            new_preset_sel.set(n.clone());
                                        }
                                    }
                                },
                            }
                        }
                        button {
                            class: "text-xs px-1.5 rounded border border-border hover:bg-accent/40",
                            onclick: {
                                let rig = rig.clone();
                                move |_| {
                                    let (name, st, pr) = (
                                        new_name.peek().clone(),
                                        new_stack_sel.peek().clone(),
                                        new_preset_sel.peek().clone(),
                                    );
                                    if let (Some(r), false, false) =
                                        (rig.clone(), name.trim().is_empty(), pr.is_empty())
                                    {
                                        spawn(async move { let _ = r.add_patch(name, st, pr).await; });
                                        adding_patch.set(false);
                                    }
                                }
                            },
                            "add"
                        }
                    }
                }
            }
            div {
                class: "flex-1 min-h-0 p-2 flex flex-col gap-0.5",
                // Blitz: `overflow-y: scroll` scrolls; `auto` is dropped.
                style: "overflow-y: scroll; scrollbar-width: thin;",
                div { class: "flex items-center gap-2 rounded-md px-2 py-1.5 text-sm font-bold",
                    span { class: "w-2 h-2 rounded-full bg-current opacity-60" }
                    if model.profile_name.is_empty() { "— no profile —" } else { "{model.profile_name}" }
                }
                for (stack_name, patches) in groups.iter() {
                    {
                        let (dot, _) = folder_color(stack_name);
                        let stack_label = stack_name.clone();
                        rsx! {
                            {
                                // The folder IS the stack's main patch —
                                // clickable, default, no "Clean Clean" names.
                                let main = patches.first().cloned();
                                let main_active = main.as_ref().is_some_and(|(_, p)| p.active);
                                let main_idx = main.as_ref().map(|(i, _)| *i);
                                let main_preset = main.as_ref().map(|(_, p)| p.preset.clone()).unwrap_or_default();
                                rsx! {
                            div {
                                class: if main_active {
                                    "group flex items-center gap-2 pl-2 pr-2 pt-2 pb-0.5 rounded-md bg-accent text-accent-foreground cursor-pointer"
                                } else {
                                    "group flex items-center gap-2 pl-2 pr-2 pt-2 pb-0.5 rounded-md cursor-pointer hover:bg-accent/30"
                                },
                                onclick: {
                                    let rig = rig.clone();
                                    move |_| {
                                        if let (Some(r), Some(i)) = (rig.clone(), main_idx) {
                                            selected_patch.set(Some(i));
                                            spawn(async move { let _ = r.select_patch(i as u32).await; });
                                        }
                                    }
                                },
                                span { class: "w-2 h-2 rounded-full flex-shrink-0", style: "background-color: {dot};" }
                                span {
                                    class: "text-xs font-bold uppercase tracking-wider",
                                    style: "{NAME_CELL}",
                                    "{stack_label}"
                                }
                                span { class: "text-[9px] font-mono opacity-50", style: "{PRESET_CELL}", "{main_preset}" }
                                span {
                                    class: "text-[10px] opacity-0 group-hover:opacity-60 hover:!opacity-100 cursor-pointer",
                                    style: "flex-shrink: 0;",
                                    title: "Delete stack (patches stay)",
                                    onclick: {
                                        let rig = rig.clone();
                                        let name = stack_label;
                                        move |_| {
                                            let name = name.clone();
                                            if let Some(r) = rig.clone() {
                                                spawn(async move { let _ = r.delete_stack(name).await; });
                                            }
                                        }
                                    },
                                    fts_chrome::Glyph { icon: fts_chrome::Icon::Close, size: 10 }
                                }
                            }
                                }
                            }
                            if patches.is_empty() {
                                span { class: "ml-6 text-[10px] italic text-muted-foreground/50", "empty — + patch" }
                            }
                            // Variations: everything after the main, shown
                            // without the stack-name prefix.
                            for (i, p) in patches.iter().skip(1) {
                                {
                                    let i = *i;
                                    let name = p.name.clone();
                                    let display = {
                                        let lower = p.name.to_lowercase();
                                        let sl = stack_name.to_lowercase();
                                        if lower.starts_with(&sl) && p.name.len() > stack_name.len() {
                                            p.name[stack_name.len()..].trim().to_string()
                                        } else {
                                            p.name.clone()
                                        }
                                    };
                                    let preset = p.preset.clone();
                                    let is_default = p.default_in_stack;
                                    let is_sel = selected_patch() == Some(i);
                                    rsx! {
                                        // A div, not a <button>: Blitz gives a
                                        // button centred content, which set every
                                        // name at a different offset.
                                        div {
                                            key: "{i}",
                                            style: "{PATCH_ROW}",
                                            class: if p.active {
                                                "group rounded-md text-sm font-bold bg-accent text-accent-foreground"
                                            } else if is_sel {
                                                "group rounded-md text-sm ring-1 ring-ring text-foreground"
                                            } else {
                                                "group rounded-md text-sm text-foreground hover:bg-accent/40"
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
                                            if renaming() == Some(("patch".to_string(), name.clone())) {
                                                input {
                                                    class: "flex-1 min-w-0 bg-background border border-border rounded px-1 text-xs",
                                                    value: "{rename_text}",
                                                    autofocus: true,
                                                    oninput: move |e| rename_text.set(e.value()),
                                                    onclick: move |e: MouseEvent| e.stop_propagation(),
                                                    onkeydown: {
                                                        let rig = rig.clone();
                                                        let old_name = name.clone();
                                                        move |e: KeyboardEvent| {
                                                            if e.key() == Key::Enter {
                                                                let (old_name, new_n) = (old_name.clone(), rename_text.peek().clone());
                                                                if let Some(r) = rig.clone() {
                                                                    spawn(async move { let _ = r.rename_patch(old_name, new_n).await; });
                                                                }
                                                                renaming.set(None);
                                                            } else if e.key() == Key::Escape {
                                                                renaming.set(None);
                                                            }
                                                        }
                                                    },
                                                }
                                            } else {
                                                span {
                                                    style: "{NAME_CELL}",
                                                    ondoubleclick: {
                                                        let name = name.clone();
                                                        move |e: MouseEvent| {
                                                            e.stop_propagation();
                                                            rename_text.set(name.clone());
                                                            renaming.set(Some(("patch".to_string(), name.clone())));
                                                        }
                                                    },
                                                    "{display}"
                                                }
                                            }
                                            // The stack's default — where the
                                            // footswitch lands after a reset.
                                            if is_default {
                                                span { class: "text-[9px] opacity-60 flex-shrink-0",
                                                    title: "stack default",
                                                    fts_chrome::Glyph { icon: fts_chrome::Icon::Star, size: 10 }
                                                }
                                            }
                                            if !p.override_modules.is_empty() {
                                                span {
                                                    class: "opacity-70",
                                                    style: "display: flex; gap: 3px; flex-shrink: 0;",
                                                    title: "overrides: {p.override_modules.join(\", \")}",
                                                    for m in p.override_modules.iter() {
                                                        crate::icons::ModuleGlyph { key: "{m}", module: m.clone(), size: 10 }
                                                    }
                                                }
                                            }
                                            if !p.available {
                                                span { class: "w-1.5 h-1.5 rounded-full flex-shrink-0",
                                                    style: "background-color: #fde047;" }
                                            }
                                            // Preset last (fixed, right-aligned) so it lines
                                            // up down the list; icons sit to its left.
                                            span { class: "text-[9px] font-mono opacity-50", style: "{PRESET_CELL}",
                                                "{preset}"
                                            }
                                            span {
                                                class: "text-[10px] opacity-0 group-hover:opacity-60 hover:!opacity-100 flex-shrink-0 cursor-pointer",
                                                title: "Delete patch",
                                                onclick: {
                                                    let rig = rig.clone();
                                                    let name = name;
                                                    move |e: MouseEvent| {
                                                        e.stop_propagation();
                                                        let name = name.clone();
                                                        if let Some(r) = rig.clone() {
                                                            spawn(async move { let _ = r.delete_patch(name).await; });
                                                        }
                                                    }
                                                },
                                                fts_chrome::Glyph { icon: fts_chrome::Icon::Close, size: 10 }
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
            div { class: "flex items-center pr-2",
                PanelLabel { label: "Presets" }
                button {
                    class: "ml-auto text-[10px] px-1 rounded border border-border text-muted-foreground hover:text-foreground",
                    title: "Import a .nam capture as a new preset",
                    onclick: move |_| { adding_preset.toggle(); new_name.set(String::new()); new_path.set(String::new()); },
                    "+ import"
                }
            }
            if adding_preset() {
                div { class: "flex flex-col gap-1 px-2 py-1 flex-shrink-0",
                    input {
                        class: "bg-background border border-border rounded px-1.5 py-0.5 text-xs",
                        placeholder: "Preset name",
                        value: "{new_name}",
                        oninput: move |e| new_name.set(e.value()),
                    }
                    div { class: "flex gap-1",
                        input {
                            class: "flex-1 min-w-0 bg-background border border-border rounded px-1.5 py-0.5 text-xs font-mono",
                            placeholder: "/path/to/capture.nam",
                            value: "{new_path}",
                            oninput: move |e| new_path.set(e.value()),
                        }
                        button {
                            class: "text-xs px-1.5 rounded border border-border hover:bg-accent/40",
                            onclick: {
                                let rig = rig.clone();
                                move |_| {
                                    let (name, path) = (new_name.peek().clone(), new_path.peek().clone());
                                    if let (Some(r), false, false) =
                                        (rig.clone(), name.trim().is_empty(), path.trim().is_empty())
                                    {
                                        spawn(async move { let _ = r.add_preset(name, path).await; });
                                        adding_preset.set(false);
                                    }
                                }
                            },
                            "add"
                        }
                    }
                }
            }
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
                                    "group flex items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm font-bold bg-accent text-accent-foreground"
                                } else if is_target {
                                    "group flex items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm ring-1 ring-ring text-foreground"
                                } else {
                                    "group flex items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm text-foreground hover:bg-accent/40"
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
                                if renaming() == Some(("preset".to_string(), name.clone())) {
                                    input {
                                        class: "flex-1 min-w-0 bg-background border border-border rounded px-1 text-xs",
                                        value: "{rename_text}",
                                        autofocus: true,
                                        oninput: move |e| rename_text.set(e.value()),
                                        onclick: move |e: MouseEvent| e.stop_propagation(),
                                        onkeydown: {
                                            let rig = rig.clone();
                                            let old_name = name.clone();
                                            move |e: KeyboardEvent| {
                                                if e.key() == Key::Enter {
                                                    let (old_name, new_n) = (old_name.clone(), rename_text.peek().clone());
                                                    if let Some(r) = rig.clone() {
                                                        spawn(async move { let _ = r.rename_preset(old_name, new_n).await; });
                                                    }
                                                    renaming.set(None);
                                                } else if e.key() == Key::Escape {
                                                    renaming.set(None);
                                                }
                                            }
                                        },
                                    }
                                } else {
                                    span {
                                        class: "truncate",
                                        ondoubleclick: {
                                            let name = name.clone();
                                            move |e: MouseEvent| {
                                                e.stop_propagation();
                                                rename_text.set(name.clone());
                                                renaming.set(Some(("preset".to_string(), name.clone())));
                                            }
                                        },
                                        "{name}"
                                    }
                                }
                                span { class: "ml-auto text-[9px] font-mono opacity-50 flex-shrink-0", "×{used}" }
                                if used == 0 {
                                    span {
                                        class: "text-[10px] opacity-0 group-hover:opacity-60 hover:!opacity-100 flex-shrink-0 cursor-pointer",
                                        title: "Delete preset",
                                        onclick: {
                                            let rig = rig.clone();
                                            let name = name;
                                            move |e: MouseEvent| {
                                                e.stop_propagation();
                                                let name = name.clone();
                                                if let Some(r) = rig.clone() {
                                                    spawn(async move { let _ = r.delete_preset(name).await; });
                                                }
                                            }
                                        },
                                        fts_chrome::Glyph { icon: fts_chrome::Icon::Close, size: 10 }
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

/// Where a name sits in a list of names, for a [`Picker`] that stores its
/// choice as a name rather than an index.
///
/// `u32::MAX` when the name is absent or empty — a Picker shows its
/// placeholder for an index past the end, which is what "nothing chosen yet"
/// should look like rather than the first option appearing pre-selected.
fn name_index(names: &[String], current: &str) -> u32 {
    if current.is_empty() {
        return u32::MAX;
    }
    names
        .iter()
        .position(|n| n == current)
        .map_or(u32::MAX, |i| i as u32)
}

#[cfg(test)]
mod tests {
    use super::name_index;

    /// A name that is in the list selects it; anything else selects nothing,
    /// so the picker shows its placeholder rather than making the first option
    /// look chosen. An unset slot that appears to hold the first patch is the
    /// failure this guards: a player would read it as configured.
    #[test]
    fn nothing_chosen_selects_nothing() {
        let names = vec!["Clean".to_string(), "Crunch".to_string()];
        assert_eq!(name_index(&names, "Clean"), 0);
        assert_eq!(name_index(&names, "Crunch"), 1);
        assert_eq!(name_index(&names, ""), u32::MAX);
        // A name the list no longer holds — a patch renamed or deleted under
        // an old section assignment.
        assert_eq!(name_index(&names, "Lead"), u32::MAX);
    }
}

/// Patch levelling, as a chip in the bar: progress while a pass runs, then
/// what it found, until dismissed. Starting a pass is an action (⌘P →
/// *Level Patches*), not a panel — it is run once after building patches,
/// not reached for mid-song.
///
/// The spread is the number that says whether the rig needed it: the
/// distance between its quietest and loudest patch before trimming. The
/// loudest trim is shown with it, coloured when it is big enough to mean a
/// patch is built wrong rather than merely unlevel.
#[component]
pub fn LevellingChip() -> Element {
    let state = crate::state::use_rig_state();
    let progress = state.levelling.cloned();
    // Dismissed for this many results — a new pass brings it back.
    let mut dismissed = use_signal(|| None::<(u32, usize)>);

    let running = progress.total > 0 && !progress.complete;
    let pct = if progress.total == 0 {
        0
    } else {
        progress.done * 100 / progress.total
    };
    let spread = {
        let mut lufs: Vec<f32> = progress
            .results
            .iter()
            .map(|r| r.lufs)
            .filter(|l| l.is_finite())
            .collect();
        lufs.sort_by(f32::total_cmp);
        match (lufs.first(), lufs.last()) {
            (Some(lo), Some(hi)) if lufs.len() > 1 => Some(hi - lo),
            _ => None,
        }
    };
    let worst = progress
        .results
        .iter()
        .filter(|r| r.lufs.is_finite())
        .map(|r| r.trim_db)
        .max_by(|a, b| a.abs().total_cmp(&b.abs()));
    let unmeasured = progress.results.iter().filter(|r| !r.lufs.is_finite()).count();
    let stamp = (progress.total, progress.results.len());

    if !running && (progress.results.is_empty() || dismissed() == Some(stamp)) {
        return rsx! {};
    }
    rsx! {
        div {
            style: "display: flex; align-items: center; gap: 6px; height: 26px; padding: 0 8px; \
                    border-radius: 6px; border: 1px solid #26262b; font-size: 10px; color: #a1a1aa; \
                    white-space: nowrap; flex-shrink: 0;",
            title: if running { format!("Measuring {}", progress.patch) } else { "Patch levelling".to_string() },
            if running {
                span { "Levelling {progress.done}/{progress.total}" }
                div { style: "width: 48px; height: 4px; border-radius: 2px; background: rgba(255,255,255,0.08); overflow: hidden;",
                    div { style: "height: 100%; width: {pct}%; background: #22c55e;" }
                }
            } else {
                span { "Levelled" }
                if let Some(spread) = spread {
                    span { style: "color: #71717a;", "· spread was {spread:.1} dB" }
                }
                if let Some(w) = worst {
                    span { style: "color: {trim_colour(w)};", "· max trim {w:+.1}" }
                }
                if unmeasured > 0 {
                    span { style: "color: #ef4444;", "· {unmeasured} not measured" }
                }
                button {
                    style: "display: flex; align-items: center; border: none; background: transparent; \
                            color: #71717a; cursor: pointer; padding: 0;",
                    title: "Dismiss",
                    onclick: move |_| dismissed.set(Some(stamp)),
                    fts_chrome::Glyph { icon: fts_chrome::Icon::Close, size: 10 }
                }
            }
        }
    }
}

/// A trim big enough to be worth a second look is coloured.
///
/// Under 3 dB is ordinary variation between amps. Past 12 dB the patch is
/// probably built wrong rather than merely unlevel, and no trim will make it
/// sit right — so it is worth saying so rather than silently applying it.
fn trim_colour(trim_db: f32) -> &'static str {
    match trim_db.abs() {
        d if d > 12.0 => "#ef4444",
        d if d > 3.0 => "#eab308",
        _ => "#71717a",
    }
}
