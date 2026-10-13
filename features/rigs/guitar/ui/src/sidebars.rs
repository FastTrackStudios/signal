//! The workbench sidebars.
//!
//! Profile mode's left sidebar: the profile under the same heading as the
//! set's and the preset's (▾ ‹ › to change it, ⋯ to grow it), its stacks and
//! patches as kit rows beneath. It consumes the `RigClient` from context and
//! renders from the pushed [`PerformanceModel`], so it works identically on
//! desktop and web.

use dioxus::prelude::*;
use signal_widgets::Picker;

use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::{PatchInfo, PerformanceModel, PresetInfo};

use crate::kit::{Button, ListRow, MenuItem, PickOption, Picked, PresetBar};
use crate::perform::folder_color;
use crate::theme::{
    EYEBROW, FAINT, FIELD, LINE, LINE_STRONG, MODIFIED, R_SM, SIDEBAR, T_BODY, T_META, TEXT,
};

/// Fire a rig call without waiting — the next `Perf` event redraws.
fn send<F, Fut>(rig: &Option<RigClient>, call: F)
where
    F: FnOnce(RigClient) -> Fut + 'static,
    Fut: std::future::Future<Output = ()> + 'static,
{
    if let Some(r) = rig.clone() {
        spawn(async move { call(r).await });
    }
}

/// A patch's menu: rename, delete.
fn patch_items(name: &str, all: &[String]) -> Vec<MenuItem> {
    let others: Vec<String> = all
        .iter()
        .filter(|n| !n.eq_ignore_ascii_case(name))
        .cloned()
        .collect();
    vec![
        MenuItem::head(format!("Patch · {name}")),
        MenuItem::name("rename", "Rename…", "Rename", name, others),
        MenuItem::delete("delete", "Delete patch", None),
    ]
}

/// A stack's menu: rename, delete (its patches stay).
fn stack_items(name: &str, all: &[String]) -> Vec<MenuItem> {
    let others: Vec<String> = all
        .iter()
        .filter(|n| !n.eq_ignore_ascii_case(name))
        .cloned()
        .collect();
    vec![
        MenuItem::head(format!("Stack · {name}")),
        MenuItem::name("rename", "Rename…", "Rename", name, others),
        MenuItem::delete("delete", "Delete stack (patches stay)", None),
    ]
}

/// A profile's menu: load, rename, duplicate, delete (refused while it plays).
fn profile_items(p: &signal_guitar_proto::ProfileEntry, all: &[String]) -> Vec<MenuItem> {
    let others: Vec<String> = all
        .iter()
        .filter(|n| !n.eq_ignore_ascii_case(&p.name))
        .cloned()
        .collect();
    vec![
        MenuItem::head(format!("Profile · {}", p.name)),
        MenuItem::run("load", "Load").unless(p.active.then(|| "Playing".to_string())),
        MenuItem::name("rename", "Rename…", "Rename", &p.name, others),
        MenuItem::name(
            "duplicate",
            "Duplicate…",
            "Duplicate",
            crate::module_sidebar::next_name(&p.name, all),
            all.to_vec(),
        ),
        MenuItem::delete(
            "delete",
            "Delete profile",
            if p.active {
                Some("Playing — load another profile first".to_string())
            } else if all.len() <= 1 {
                Some("The only profile".to_string())
            } else {
                None
            },
        ),
    ]
}

/// Left sidebar — the profile tree over the preset pool.
///
/// Top: Profile → Stacks → Patches (organize the rig). Bottom: the preset
/// pool patches point at. Select a patch in the tree, then click a preset
/// to point the patch at it (the core rebuilds that patch's chain).
#[component]
pub fn LeftSidebar(
    model: PerformanceModel,
    /// Full: a phone's width. Minimal keeps the names and the state.
    #[props(default)]
    full: bool,
) -> Element {
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
                        r.library().await.unwrap_or_default().profiles,
                    ),
                    None => (Vec::new(), Vec::new(), Vec::new()),
                }
            }
        }
    });
    let (patch_list, preset_list, profiles): (
        Vec<PatchInfo>,
        Vec<PresetInfo>,
        Vec<signal_guitar_proto::ProfileEntry>,
    ) = data.read().clone().unwrap_or_default();

    // The patch picked in the tree.
    let mut selected_patch = use_signal(|| None::<usize>);
    // The new-patch form (a name, its stack, its preset), from the header's ⋯.
    let mut adding_patch = use_signal(|| false);
    let mut new_name = use_signal(String::new);
    let mut new_stack_sel = use_signal(String::new);
    let mut new_preset_sel = use_signal(String::new);

    let patch_names: Vec<String> = patch_list.iter().map(|p| p.name.clone()).collect();
    let stack_names: Vec<String> = model.stacks.iter().map(|s| s.name.clone()).collect();
    let profile_names: Vec<String> = profiles.iter().map(|p| p.name.clone()).collect();
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

    // The header's menu: grow the profile, then manage it.
    let current = profiles.iter().find(|p| p.active).cloned();
    let mut header_menu = vec![
        MenuItem::name("new_stack", "New stack…", "Create", "", stack_names.clone()),
        MenuItem::run("new_patch", "New patch…"),
        MenuItem::name("new_profile", "New profile…", "Create", "", profile_names.clone()),
    ];
    if let Some(p) = &current {
        header_menu.push(MenuItem::sep());
        header_menu.extend(profile_items(p, &profile_names));
    }
    let field = format!(
        "min-width: 0; font-size: {T_BODY}; color: {TEXT}; background: {FIELD}; border: 1px solid {LINE_STRONG}; \
         border-radius: {R_SM}; padding: 6px 8px; outline: none;"
    );

    rsx! {
        aside {
            style: "width: {crate::kit::pane_w(full)}; flex-shrink: 0; display: flex; flex-direction: column; min-height: 0; \
                    border-right: 1px solid {LINE}; background: {SIDEBAR}; color: {TEXT};",
            // ── The profile: the same heading as the set's — name large,
            // ▾ / ‹ › to change profile, ⋯ to grow and manage it ──
            div { style: "display: flex; flex-direction: column; padding: 10px 12px 10px 14px; \
                          border-bottom: 1px solid {LINE}; flex-shrink: 0;",
                PresetBar {
                    label: "Profile",
                    name: model.profile_name.clone(),
                    placeholder: "No profile",
                    sub: format!(
                        "{} · {}",
                        plural(model.stacks.len(), "stack"),
                        plural(patch_list.len(), "patch"),
                    ),
                    large: true,
                    options: profiles
                        .iter()
                        .map(|p| PickOption { label: p.name.clone(), note: format!("{}", p.patches), live: p.active, ..Default::default() })
                        .collect::<Vec<_>>(),
                    on_pick: {
                        let rig = rig.clone();
                        let names = profile_names.clone();
                        move |i: usize| {
                            if let Some(n) = names.get(i).cloned() {
                                send(&rig, move |r| async move { let _ = r.select_profile(n).await; });
                            }
                        }
                    },
                    on_step: {
                        let rig = rig.clone();
                        let names = profile_names.clone();
                        let at = profiles.iter().position(|p| p.active);
                        move |d: i32| {
                            let n = names.len() as i32;
                            if n == 0 {
                                return;
                            }
                            let to = at.map_or(0, |a| (a as i32 + d).rem_euclid(n)) as usize;
                            let name = names[to].clone();
                            send(&rig, move |r| async move { let _ = r.select_profile(name).await; });
                        }
                    },
                    menu: header_menu,
                    on_menu: {
                        let rig = rig.clone();
                        let name = model.profile_name.clone();
                        move |x: Picked| {
                            let (old, text) = (name.clone(), x.text.trim().to_string());
                            match x.id {
                                "new_stack" if !text.is_empty() => send(&rig, move |r| async move { let _ = r.add_stack(text).await; }),
                                "new_patch" => {
                                    new_name.set(String::new());
                                    adding_patch.set(true);
                                }
                                "new_profile" if !text.is_empty() => send(&rig, move |r| async move { let _ = r.add_profile(text, old).await; }),
                                "rename" if !text.is_empty() => send(&rig, move |r| async move { let _ = r.rename_profile(old, text).await; }),
                                "duplicate" if !text.is_empty() => send(&rig, move |r| async move { let _ = r.add_profile(text, old).await; }),
                                "delete" => send(&rig, move |r| async move { let _ = r.delete_profile(old).await; }),
                                _ => {}
                            }
                        }
                    },
                }
            }
            // ── A new patch: its name, the stack it joins, the preset it plays ──
            if adding_patch() {
                div { style: "display: flex; flex-direction: column; gap: 6px; padding: 10px 12px; border-bottom: 1px solid {LINE}; flex-shrink: 0;",
                    span { style: "{EYEBROW}", "New patch" }
                    input {
                        style: "{field}",
                        placeholder: "Patch name",
                        value: "{new_name}",
                        autofocus: true,
                        oninput: move |e| new_name.set(e.value()),
                        onkeydown: move |e: KeyboardEvent| {
                            if e.key() == Key::Escape {
                                adding_patch.set(false);
                            }
                        },
                    }
                    div { style: "display: flex; gap: 6px;",
                        div { style: "flex: 1 1 0; min-width: 0;",
                            Picker {
                                options: stack_names.clone(),
                                selected: name_index(&stack_names, &new_stack_sel()),
                                placeholder: "Stack…".to_string(),
                                width: "100%".to_string(),
                                on_select: {
                                    let names = stack_names.clone();
                                    move |i: u32| {
                                        if let Some(n) = names.get(i as usize) {
                                            new_stack_sel.set(n.clone());
                                        }
                                    }
                                },
                            }
                        }
                        div { style: "flex: 1 1 0; min-width: 0;",
                            Picker {
                                options: preset_list.iter().map(|p| p.name.clone()).collect::<Vec<String>>(),
                                selected: name_index(&preset_list.iter().map(|p| p.name.clone()).collect::<Vec<String>>(), &new_preset_sel()),
                                placeholder: "Amp…".to_string(),
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
                    }
                    div { style: "display: flex; gap: 6px; justify-content: flex-end;",
                        Button { label: "Cancel", small: true, onclick: move |()| adding_patch.set(false) }
                        Button {
                            label: "Create",
                            small: true,
                            primary: true,
                            disabled: new_name().trim().is_empty() || new_preset_sel().is_empty(),
                            onclick: {
                                let rig = rig.clone();
                                move |()| {
                                    let (name, st, pr) = (
                                        new_name.peek().trim().to_string(),
                                        new_stack_sel.peek().clone(),
                                        new_preset_sel.peek().clone(),
                                    );
                                    if !name.is_empty() && !pr.is_empty() {
                                        send(&rig, move |r| async move { let _ = r.add_patch(name, st, pr).await; });
                                        adding_patch.set(false);
                                    }
                                }
                            },
                        }
                    }
                }
            }
            // ── The tree: each stack is its main patch, its variations under it ──
            div {
                // Blitz: `overflow-y: scroll` scrolls; `auto` is dropped.
                style: "flex: 1 1 0; min-height: 0; overflow-y: scroll; scrollbar-width: thin; padding: 6px; \
                        display: flex; flex-direction: column; gap: 1px;",
                for (stack_name, patches) in groups.iter() {
                    {
                        let (dot, _) = folder_color(stack_name);
                        let stack_label = stack_name.clone();
                        // The folder IS the stack's main patch — clickable,
                        // the default, no "Clean Clean" names.
                        let main = patches.first().cloned();
                        let main_active = main.as_ref().is_some_and(|(_, p)| p.active);
                        let main_idx = main.as_ref().map(|(i, _)| *i);
                        let main_sound = main.as_ref().map(|(_, p)| sound_of(p)).unwrap_or_default();
                        rsx! {
                            div { key: "{stack_label}", style: "display: flex; flex-direction: column; gap: 1px; margin-top: 4px;",
                                ListRow {
                                    title: stack_label.clone(),
                                    sub: main_sound.clone(),
                                    swatch: dot.to_string(),
                                    live: main_active,
                                    selected: !main_active && main_idx.is_some() && selected_patch() == main_idx,
                                    onclick: {
                                        let rig = rig.clone();
                                        move |()| {
                                            if let Some(i) = main_idx {
                                                selected_patch.set(Some(i));
                                                send(&rig, move |r| async move { let _ = r.select_patch(i as u32).await; });
                                            }
                                        }
                                    },
                                    menu: stack_items(&stack_label, &stack_names),
                                    on_menu: {
                                        let rig = rig.clone();
                                        let name = stack_label.clone();
                                        move |p: Picked| {
                                            let (old, text) = (name.clone(), p.text);
                                            match p.id {
                                                "rename" => send(&rig, move |r| async move { let _ = r.rename_stack(old, text).await; }),
                                                "delete" => send(&rig, move |r| async move { let _ = r.delete_stack(old).await; }),
                                                _ => {}
                                            }
                                        }
                                    },
                                }
                                if patches.is_empty() {
                                    span { style: "padding: 2px 8px 4px 30px; font-size: {T_META}; color: {FAINT};", "No patches yet" }
                                }
                                // Variations: everything after the main, shown
                                // without the stack-name prefix, and with their
                                // sound only where it differs from the main's.
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
                                        let sound = sound_of(p);
                                        let sub = if sound == main_sound { String::new() } else { sound };
                                        let is_default = p.default_in_stack;
                                        let overrides = p.override_modules.clone();
                                        let available = p.available;
                                        rsx! {
                                            ListRow {
                                                key: "{i}",
                                                title: display,
                                                sub,
                                                small: true,
                                                indent: 14,
                                                live: p.active,
                                                selected: !p.active && selected_patch() == Some(i),
                                                onclick: {
                                                    let rig = rig.clone();
                                                    move |()| {
                                                        selected_patch.set(Some(i));
                                                        send(&rig, move |r| async move { let _ = r.select_patch(i as u32).await; });
                                                    }
                                                },
                                                menu: patch_items(&name, &patch_names),
                                                on_menu: {
                                                    let rig = rig.clone();
                                                    let name = name.clone();
                                                    move |p: Picked| {
                                                        let (old, text) = (name.clone(), p.text);
                                                        match p.id {
                                                            "rename" => send(&rig, move |r| async move { let _ = r.rename_patch(old, text).await; }),
                                                            "delete" => send(&rig, move |r| async move { let _ = r.delete_patch(old).await; }),
                                                            _ => {}
                                                        }
                                                    }
                                                },
                                                // The stack's default — where the footswitch
                                                // lands after a reset.
                                                if is_default {
                                                    span { style: "display: flex; flex-shrink: 0; color: {FAINT};", title: "Stack default",
                                                        fts_chrome::Glyph { icon: fts_chrome::Icon::Star, size: 11 }
                                                    }
                                                }
                                                if !overrides.is_empty() {
                                                    span {
                                                        style: "display: flex; gap: 3px; flex-shrink: 0; color: {FAINT};",
                                                        title: "Overrides: {overrides.join(\", \")}",
                                                        for m in overrides.iter() {
                                                            crate::icons::ModuleGlyph { key: "{m}", module: m.clone(), size: 11 }
                                                        }
                                                    }
                                                }
                                                if !available {
                                                    span { style: "width: 6px; height: 6px; border-radius: 999px; flex-shrink: 0; background: {MODIFIED};",
                                                        title: "Its amp or capture is missing on this machine" }
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
        }
    }
}

/// "1 stack", "3 patches".
fn plural(n: usize, one: &str) -> String {
    match (n, one) {
        (1, _) => format!("1 {one}"),
        (_, "patch") => format!("{n} patches"),
        _ => format!("{n} {one}s"),
    }
}

/// What a patch plays, in a line: its amp preset · the variation of it.
fn sound_of(p: &PatchInfo) -> String {
    match (p.rig_preset.is_empty(), p.variation.is_empty()) {
        (true, _) => String::new(),
        (false, true) => p.rig_preset.clone(),
        (false, false) => format!("{} · {}", p.rig_preset, p.variation),
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
    let unmeasured = progress
        .results
        .iter()
        .filter(|r| !r.lufs.is_finite())
        .count();
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
