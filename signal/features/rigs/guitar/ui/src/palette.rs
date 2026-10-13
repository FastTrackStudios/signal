//! **The command picker** — Cmd/Ctrl+P. Every rig action reachable from the
//! keyboard, found by fuzzy typing: jump to a patch, song, song part,
//! setlist or profile; create things (with an inline argument step); toggle FX, boost,
//! tuner, mute; level the patches; open the library on a kind.
//!
//! Matching is fuzzy the way editors' pickers are: the query's letters in
//! order anywhere in the label, ranked by how well they land — word starts
//! and consecutive runs first — so "lvl" finds *Level patches* and "cya9"
//! finds *CYA 9-24-26*.
//!
//! The action list is data (label + category + effect), and the keymap
//! (`keymap.styx`) speaks the same [`Effect`]s, so a bound action shows its
//! keys here.
//!
//! Blitz: no `position: fixed`, so this mounts in the rig root beside the
//! library picker and, like it, has the rig body hidden while it is open
//! (see `remote.rs` — the frame budget).

use dioxus::prelude::*;

use signal_guitar_proto::PerformanceModel;
use signal_guitar_proto::rig::RigClient;

use crate::library::Kind;

/// What an action does when executed.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Effect {
    /// Fire-and-forget service call.
    SelectStack(u32),
    NextSong,
    PrevSong,
    SelectPatch(u32),
    PlayPreset(u32),
    SelectSong(u32),
    SelectPart(u32),
    SelectSetlist(u32),
    SelectProfile(String),
    AddSongToSet(String),
    ToggleFx,
    ToggleBoost,
    ToggleTuner,
    ToggleMute,
    TapTempo,
    ReloadLibrary,
    LevelPatches,
    SetMode(u32),
    /// Needs one typed argument — the palette switches to input mode.
    NewSong,
    NewSetlist,
    NewStack,
    /// New patch on the active stack pointing at the active preset.
    NewPatch,
    /// Import a preset from a `.nam` path.
    ImportPreset,
    /// A new song part on the current song.
    NewPart,
    // ── Local: the remote's own view state, not the rig ──
    /// Open the library on a kind.
    Browse(Kind),
    /// Full → compact → hidden footswitches.
    CycleSwitches,
}

impl Effect {
    /// Whether this changes the remote's view rather than the rig — the
    /// remote handles these itself; [`execute`] ignores them.
    #[must_use]
    pub const fn is_local(&self) -> bool {
        matches!(self, Self::Browse(_) | Self::CycleSwitches)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Action {
    pub label: String,
    /// Category, shown dim beside the label, and matched too.
    pub hint: &'static str,
    pub effect: Effect,
}

/// Whether this effect needs the argument step, and its prompt.
const fn arg_prompt(e: &Effect) -> Option<&'static str> {
    match e {
        Effect::NewSong => Some("Song name (append \"@ G 74\" for key/bpm)…"),
        Effect::NewSetlist => Some("Setlist name (venue + date)…"),
        Effect::NewStack => Some("Stack name…"),
        Effect::NewPatch => Some("Patch name (lands on the active stack + preset)…"),
        Effect::ImportPreset => Some("/path/to/capture.nam…"),
        Effect::NewPart => Some("Part name — Verse 1, Rhythm, Clean lead…"),
        _ => None,
    }
}

/// Build the full action list from the live model.
fn actions(
    model: &PerformanceModel,
    patches: &[String],
    presets: &[String],
    profiles: &[(String, bool)],
) -> Vec<Action> {
    let mut out = Vec::new();
    let cmd = |label: &str, hint: &'static str, effect: Effect| Action {
        label: label.to_string(),
        hint,
        effect,
    };
    // Commands first — they're what the picker is FOR; the jumps follow.
    out.push(cmd("Level Patches", "action", Effect::LevelPatches));
    out.push(cmd("Tuner", "action", Effect::ToggleTuner));
    out.push(cmd("Toggle FX", "action", Effect::ToggleFx));
    out.push(cmd("Toggle Boost", "action", Effect::ToggleBoost));
    out.push(cmd("Mute Output", "action", Effect::ToggleMute));
    out.push(cmd("Tap Tempo", "action", Effect::TapTempo));
    out.push(cmd("Next Song", "action", Effect::NextSong));
    out.push(cmd("Previous Song", "action", Effect::PrevSong));
    out.push(cmd(
        "Reload Library (styx files)",
        "action",
        Effect::ReloadLibrary,
    ));
    out.push(cmd("New Song…", "create", Effect::NewSong));
    out.push(cmd("New Song Part…", "create", Effect::NewPart));
    out.push(cmd("New Patch…", "create", Effect::NewPatch));
    out.push(cmd("New Stack…", "create", Effect::NewStack));
    out.push(cmd("New Setlist…", "create", Effect::NewSetlist));
    out.push(cmd("Import Preset (.nam)…", "create", Effect::ImportPreset));
    out.push(cmd("Library", "view", Effect::Browse(Kind::All)));
    for k in [
        Kind::Setlists,
        Kind::Songs,
        Kind::Profiles,
        Kind::Patches,
        Kind::Presets,
        Kind::Drives,
    ] {
        out.push(Action {
            label: format!("Browse {}", k.label()),
            hint: "view",
            effect: Effect::Browse(k),
        });
    }
    out.push(cmd("Preset Mode", "view", Effect::SetMode(0)));
    out.push(cmd("Profile Mode", "view", Effect::SetMode(1)));
    out.push(cmd("Setlist Mode", "view", Effect::SetMode(2)));
    out.push(cmd(
        "Switches: Full / Compact / Hidden",
        "view",
        Effect::CycleSwitches,
    ));
    for (name, active) in profiles {
        if !active {
            out.push(Action {
                label: format!("Profile: {name}"),
                hint: "profile",
                effect: Effect::SelectProfile(name.clone()),
            });
        }
    }
    for (i, name) in patches.iter().enumerate() {
        out.push(cmd(name, "patch", Effect::SelectPatch(i as u32)));
    }
    for (i, p) in presets.iter().enumerate() {
        out.push(cmd(p, "preset", Effect::PlayPreset(i as u32)));
    }
    for (i, s) in model.songs.iter().enumerate() {
        out.push(cmd(
            &format!("{} ({} · {})", s.name, s.key, s.bpm),
            "song",
            Effect::SelectSong(i as u32),
        ));
    }
    for (i, p) in model.parts.iter().enumerate() {
        out.push(cmd(
            &format!("Part: {}", p.name),
            "part",
            Effect::SelectPart(i as u32),
        ));
    }
    for (i, s) in model.setlists.iter().enumerate() {
        out.push(cmd(
            &format!("Setlist: {s}"),
            "setlist",
            Effect::SelectSetlist(i as u32),
        ));
    }
    for s in &model.library_songs {
        out.push(cmd(
            &format!("Add to Set: {}", s.name),
            "setlist",
            Effect::AddSongToSet(s.name.clone()),
        ));
    }
    out
}

/// Fuzzy score of `query` against `text`, or `None` when the query's
/// characters do not all appear in order.
///
/// Greedy left-to-right, like the pickers in editors: each matched character
/// scores, more at the start of a word and more again straight after the
/// previous match; a gap costs a little and a long label a little, so the
/// tight, short match ranks first. Whitespace in the query is ignored, so
/// "lev pat" and "levpat" rank the same.
#[must_use]
pub(crate) fn fuzzy(query: &str, text: &str) -> Option<i32> {
    let q: Vec<char> = query
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    if q.is_empty() {
        return Some(0);
    }
    let t: Vec<char> = text.chars().flat_map(char::to_lowercase).collect();
    let (mut score, mut qi, mut last) = (0i32, 0usize, None::<usize>);
    for (i, &c) in t.iter().enumerate() {
        if qi == q.len() {
            break;
        }
        if c != q[qi] {
            continue;
        }
        let mut s = 1;
        if i == 0 || !t[i - 1].is_alphanumeric() {
            s += 8;
        }
        match last {
            Some(l) if l + 1 == i => s += 5,
            Some(l) => s -= ((i - l - 1) as i32).min(3),
            None => s -= (i as i32).min(5),
        }
        score += s;
        last = Some(i);
        qi += 1;
    }
    (qi == q.len()).then(|| score - t.len() as i32 / 12)
}

use crate::theme::{BG, FAINT, FOCUS_BG, FOCUS_FG, LINE, TEXT};

/// The picker. `on_local` receives the [`Effect::is_local`] effects — the
/// remote's own view state — everything else goes to the rig.
#[component]
pub fn CommandPalette(
    model: PerformanceModel,
    open: Signal<bool>,
    on_local: EventHandler<Effect>,
) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut query = use_signal(String::new);
    let mut cursor = use_signal(|| 0usize);
    // Argument step: the chosen effect awaiting its typed argument.
    let mut pending = use_signal(|| None::<Effect>);

    // Live patch / preset / profile names for the jump actions, re-read when
    // the rig's state moves (a signal, so the resource sees the change).
    let mut rev = use_signal(|| model.revision);
    if *rev.peek() != model.revision {
        rev.set(model.revision);
    }
    let lists = use_resource({
        let rig = rig.clone();
        move || {
            let _ = rev();
            let rig = rig.clone();
            async move {
                let Some(r) = rig else {
                    return Default::default();
                };
                let patches: Vec<String> = r
                    .patches()
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .map(|p| p.name)
                    .collect();
                let presets: Vec<String> = r
                    .presets()
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .map(|p| p.name)
                    .collect();
                let profiles: Vec<(String, bool)> = r
                    .library()
                    .await
                    .unwrap_or_default()
                    .profiles
                    .into_iter()
                    .map(|p| (p.name, p.active))
                    .collect();
                (patches, presets, profiles)
            }
        }
    });

    if !open() {
        return rsx! {};
    }
    let (patches, presets, profiles) = lists.read().clone().unwrap_or_default();

    let q = query();
    let mut ranked: Vec<(i32, usize, Action)> = actions(&model, &patches, &presets, &profiles)
        .into_iter()
        .enumerate()
        .filter_map(|(i, a)| fuzzy(&q, &format!("{} {}", a.label, a.hint)).map(|s| (s, i, a)))
        .collect();
    // Best score first; ties keep the list's own order (commands before jumps).
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let filtered: Vec<Action> = ranked.into_iter().take(60).map(|(_, _, a)| a).collect();
    let sel = cursor().min(filtered.len().saturating_sub(1));

    // A bound action shows its keys.
    let keys_for = |e: &Effect| -> Option<String> {
        model
            .key_bindings
            .iter()
            .find(|b| effect_from_action(&b.action).as_ref() == Some(e))
            .map(|b| b.keys.replace("ctrl+", "⌘").to_uppercase())
    };

    let mut close = move || {
        open.set(false);
        query.set(String::new());
        cursor.set(0);
        pending.set(None);
    };

    // Execute a non-argument effect (or stage an argument step). Returns
    // whether the picker should close.
    let run = {
        let rig = rig;
        move |effect: Effect, arg: Option<String>| {
            if arg.is_none() && arg_prompt(&effect).is_some() {
                pending.set(Some(effect));
                query.set(String::new());
                return false; // stay open for the argument
            }
            if effect.is_local() {
                on_local.call(effect);
                return true;
            }
            let Some(r) = rig.clone() else { return true };
            execute(r, effect, arg.unwrap_or_default());
            true
        }
    };

    let prompt = pending().as_ref().and_then(arg_prompt);

    rsx! {
        div {
            style: "position: absolute; inset: 0; z-index: 95; background: rgba(0, 0, 0, 0.55);",
            onclick: move |_| close(),
            div {
                style: "position: absolute; top: 12%; left: 50%; transform: translateX(-50%); \
                        width: 640px; max-width: 90%; display: flex; flex-direction: column; \
                        border-radius: 12px; overflow: hidden; background: {BG}; \
                        border: 1px solid {LINE}; color: {TEXT};",
                onclick: move |e: MouseEvent| e.stop_propagation(),
                input {
                    style: "width: 100%; background: transparent; border: none; outline: none; \
                            padding: 14px 16px; font-size: 14px; color: {TEXT}; \
                            border-bottom: 1px solid {LINE};",
                    placeholder: prompt.unwrap_or("Type an action, patch, song, profile…"),
                    value: "{q}",
                    // autofocus is ignored on dynamically-inserted nodes —
                    // grab focus explicitly every time the picker mounts.
                    onmounted: move |e| {
                        spawn(async move {
                            let _ = e.data().set_focus(true).await;
                        });
                    },
                    oninput: move |e| {
                        query.set(e.value());
                        cursor.set(0);
                    },
                    onkeydown: {
                        let mut run = run.clone();
                        let filtered = filtered.clone();
                        move |e: KeyboardEvent| match e.key() {
                            Key::Escape => {
                                e.prevent_default();
                                if pending().is_some() {
                                    pending.set(None);
                                    query.set(String::new());
                                } else {
                                    close();
                                }
                            }
                            Key::ArrowDown => {
                                e.prevent_default();
                                cursor.set((sel + 1).min(filtered.len().saturating_sub(1)));
                            }
                            Key::ArrowUp => {
                                e.prevent_default();
                                cursor.set(sel.saturating_sub(1));
                            }
                            Key::Enter => {
                                e.prevent_default();
                                if let Some(effect) = pending() {
                                    if !query.peek().trim().is_empty()
                                        && run(effect, Some(query.peek().clone()))
                                    {
                                        close();
                                    }
                                } else if let Some(a) = filtered.get(sel) {
                                    if run(a.effect.clone(), None) {
                                        close();
                                    }
                                }
                            }
                            _ => {}
                        }
                    },
                }
                if pending().is_none() {
                    div { style: "max-height: 420px; overflow-y: scroll; padding: 4px 0;",
                        for (i, a) in filtered.iter().enumerate() {
                            {
                                let effect = a.effect.clone();
                                let keys = keys_for(&a.effect);
                                let mut run = run.clone();
                                let on = i == sel;
                                rsx! {
                                    div {
                                        key: "{a.hint}-{a.label}",
                                        style: format!(
                                            "display: flex; align-items: center; gap: 10px; padding: 7px 16px; \
                                             cursor: pointer; font-size: 13px; background: {}; color: {};",
                                            if on { FOCUS_BG } else { "transparent" },
                                            if on { FOCUS_FG } else { TEXT },
                                        ),
                                        onmouseenter: move |_| cursor.set(i),
                                        onclick: move |_| {
                                            if run(effect.clone(), None) {
                                                close();
                                            }
                                        },
                                        span { style: "flex: 1 1 0; min-width: 0; white-space: nowrap; overflow: hidden;", "{a.label}" }
                                        if let Some(k) = keys {
                                            span {
                                                style: "font-size: 10px; font-family: monospace; color: {FAINT}; \
                                                        border: 1px solid {LINE}; border-radius: 4px; padding: 1px 5px;",
                                                "{k}"
                                            }
                                        }
                                        span {
                                            style: "width: 56px; flex-shrink: 0; text-align: right; font-size: 9px; \
                                                    font-weight: 700; letter-spacing: 0.1em; text-transform: uppercase; color: {FAINT};",
                                            "{a.hint}"
                                        }
                                    }
                                }
                            }
                        }
                        if filtered.is_empty() {
                            div { style: "padding: 12px 16px; font-size: 13px; color: {FAINT};", "No matches." }
                        }
                    }
                } else {
                    div { style: "padding: 8px 16px; font-size: 11px; color: {FAINT};",
                        "Enter to confirm · Esc to go back"
                    }
                }
            }
        }
    }
}

/// Run an effect against the rig — shared by the palette and the keymap.
pub fn execute(r: RigClient, effect: Effect, arg: String) {
    spawn(async move {
        match effect {
            Effect::SelectStack(i) => drop(r.press_stack(i).await),
            Effect::NextSong => drop(r.next_song().await),
            Effect::PrevSong => drop(r.prev_song().await),
            Effect::SelectPatch(i) => drop(r.select_patch(i).await),
            Effect::PlayPreset(i) => drop(r.play_preset(i).await),
            Effect::SelectSong(i) => drop(r.select_song(i).await),
            Effect::SelectSetlist(i) => drop(r.select_setlist(i).await),
            Effect::SelectPart(i) => drop(r.select_part(i).await),
            Effect::SelectProfile(name) => drop(r.select_profile(name).await),
            Effect::LevelPatches => drop(r.level_patches().await),
            Effect::NewPart => drop(r.add_part(arg.trim().to_string()).await),
            // The remote's own view state — handled where the view lives.
            Effect::Browse(_) | Effect::CycleSwitches => {}
            Effect::AddSongToSet(name) => {
                // Append to whichever setlist is active.
                if let Ok(m) = r.perf().await {
                    drop(r.add_setlist_entry(m.setlist_index, name).await);
                }
            }
            Effect::ToggleFx => drop(r.toggle_fx().await),
            Effect::ToggleBoost => drop(r.toggle_boost().await),
            Effect::ToggleTuner => drop(r.toggle_tuner().await),
            Effect::ToggleMute => drop(r.toggle_main_mute().await),
            Effect::TapTempo => drop(r.tap_tempo().await),
            Effect::ReloadLibrary => drop(r.reload_library().await),
            Effect::SetMode(m) => drop(r.set_perform_mode(m).await),
            Effect::NewSong => {
                // "Name @ G 74" → key/bpm; plain name = defaults.
                let (name, key, bpm) = match arg.split_once('@') {
                    Some((n, kb)) => {
                        let mut it = kb.split_whitespace();
                        let key = it.next().unwrap_or("").to_string();
                        let bpm = it.next().and_then(|b| b.parse().ok()).unwrap_or(0);
                        (n.trim().to_string(), key, bpm)
                    }
                    None => (arg.trim().to_string(), String::new(), 0),
                };
                drop(r.add_song(name, key, bpm).await);
            }
            Effect::NewSetlist => drop(r.add_setlist(arg.trim().to_string()).await),
            Effect::NewStack => drop(r.add_stack(arg.trim().to_string()).await),
            Effect::NewPatch => {
                // Land on the active stack + active preset.
                if let (Ok(m), Ok(presets)) = (r.perf().await, r.presets().await) {
                    let stack = m
                        .stacks
                        .iter()
                        .find(|s| s.is_active)
                        .map(|s| s.name.clone())
                        .unwrap_or_default();
                    let preset = presets
                        .iter()
                        .find(|p| p.active)
                        .map(|p| p.name.clone())
                        .unwrap_or_default();
                    drop(r.add_patch(arg.trim().to_string(), stack, preset).await);
                }
            }
            Effect::ImportPreset => {
                let name = std::path::Path::new(arg.trim()).file_stem().map_or_else(
                    || "Imported".to_string(),
                    |s| s.to_string_lossy().to_string(),
                );
                drop(r.add_preset(name, arg.trim().to_string()).await);
            }
        }
    });
}

/// Parse a keymap action string ("stack:0", "`toggle_fx`", "song:next",
/// "mode:profile", "patch:3", …) into an executable effect.
pub fn effect_from_action(action: &str) -> Option<Effect> {
    let (verb, arg) = match action.split_once(':') {
        Some((v, a)) => (v, a),
        None => (action, ""),
    };
    Some(match (verb, arg) {
        ("stack", n) => Effect::SelectStack(n.parse().ok()?),
        ("patch", n) => Effect::SelectPatch(n.parse().ok()?),
        ("preset", n) => Effect::PlayPreset(n.parse().ok()?),
        ("song", "next") => Effect::NextSong,
        ("song", "prev") => Effect::PrevSong,
        ("song", n) => Effect::SelectSong(n.parse().ok()?),
        ("setlist", n) => Effect::SelectSetlist(n.parse().ok()?),
        ("mode", "pedals") => Effect::SetMode(0),
        ("mode", "profile") => Effect::SetMode(1),
        ("mode", "setlist") => Effect::SetMode(2),
        ("toggle_fx", _) => Effect::ToggleFx,
        ("boost", _) => Effect::ToggleBoost,
        ("tuner", _) => Effect::ToggleTuner,
        ("mute", _) => Effect::ToggleMute,
        ("tap", _) => Effect::TapTempo,
        ("reload", _) => Effect::ReloadLibrary,
        ("level", _) => Effect::LevelPatches,
        ("part", n) => Effect::SelectPart(n.parse().ok()?),
        ("library", _) => Effect::Browse(Kind::All),
        ("palette", _) => return None,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::fuzzy;

    #[test]
    fn letters_in_order_match_and_others_do_not() {
        assert!(fuzzy("lvl", "Level Patches").is_some());
        assert!(fuzzy("cya9", "Setlist: CYA 9-24-26").is_some());
        assert!(fuzzy("zzz", "Level Patches").is_none());
        assert_eq!(fuzzy("", "anything"), Some(0));
    }

    #[test]
    fn word_starts_and_runs_rank_first() {
        // "lp" — the initials of Level Patches beat letters buried mid-word.
        let initials = fuzzy("lp", "Level Patches").unwrap();
        let buried = fuzzy("lp", "Clean Dry Pedal").unwrap();
        assert!(initials > buried, "{initials} vs {buried}");
        // A contiguous run beats the same letters spread out.
        let run = fuzzy("tun", "Tuner").unwrap();
        let spread = fuzzy("tun", "Tap Tempo Undo Now").unwrap();
        assert!(run > spread, "{run} vs {spread}");
    }
}
