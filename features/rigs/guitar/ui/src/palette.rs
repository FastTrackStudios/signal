//! Command palette — Cmd/Ctrl+P. Every rig action reachable from the
//! keyboard: jump to patches / songs / setlists, create things (with an
//! inline argument step), toggle FX / boost / tuner / mute, arm perform
//! modes, reload the library.
//!
//! The action list is data-first (label + keywords + effect) so it can be
//! backed by the `input_actions` registry when the rig grows real
//! keybinding maps.

use dioxus::prelude::*;

use signal_guitar_proto::rig::RigClient;
use signal_guitar_proto::PerformanceModel;

/// What an action does when executed.
#[derive(Clone, PartialEq)]
pub enum Effect {
    /// Fire-and-forget service call.
    SelectStack(u32),
    NextSong,
    PrevSong,
    SelectPatch(u32),
    PlayPreset(u32),
    SelectSong(u32),
    SelectSetlist(u32),
    AddSongToSet(String),
    ToggleFx,
    ToggleBoost,
    ToggleTuner,
    ToggleMute,
    TapTempo,
    ReloadLibrary,
    SetMode(u32),
    /// Needs one typed argument — the palette switches to input mode.
    NewSong,
    NewSetlist,
    NewStack,
    /// New patch on the active stack pointing at the active preset.
    NewPatch,
    /// Import a preset from a `.nam` path.
    ImportPreset,
}

#[derive(Clone, PartialEq)]
pub struct Action {
    pub label: String,
    /// Section header shown dim next to the label.
    pub hint: &'static str,
    pub effect: Effect,
}

/// Whether this effect needs the argument step, and its prompt.
fn arg_prompt(e: &Effect) -> Option<&'static str> {
    match e {
        Effect::NewSong => Some("Song name (append \"@ G 74\" for key/bpm)…"),
        Effect::NewSetlist => Some("Setlist name (venue + date)…"),
        Effect::NewStack => Some("Stack name…"),
        Effect::NewPatch => Some("Patch name (lands on the active stack + preset)…"),
        Effect::ImportPreset => Some("/path/to/capture.nam…"),
        _ => None,
    }
}

/// Build the full action list from the live model.
fn actions(
    model: &PerformanceModel,
    patches: &[(usize, String)],
    presets: &[String],
) -> Vec<Action> {
    let mut out = Vec::new();
    let cmd = |label: &str, hint: &'static str, effect: Effect| Action {
        label: label.to_string(),
        hint,
        effect,
    };
    // Creation + utility commands first — they're what the palette is FOR.
    out.push(cmd("New Song…", "create", Effect::NewSong));
    out.push(cmd("New Patch…", "create", Effect::NewPatch));
    out.push(cmd("New Stack…", "create", Effect::NewStack));
    out.push(cmd("New Setlist…", "create", Effect::NewSetlist));
    out.push(cmd("Import Preset (.nam)…", "create", Effect::ImportPreset));
    out.push(cmd("Toggle FX", "rig", Effect::ToggleFx));
    out.push(cmd("Toggle Boost", "rig", Effect::ToggleBoost));
    out.push(cmd("Tuner", "rig", Effect::ToggleTuner));
    out.push(cmd("Mute Output", "rig", Effect::ToggleMute));
    out.push(cmd("Tap Tempo", "rig", Effect::TapTempo));
    out.push(cmd("Reload Library (styx)", "rig", Effect::ReloadLibrary));
    out.push(cmd("Mode: Pedals", "view", Effect::SetMode(0)));
    out.push(cmd("Mode: Profile", "view", Effect::SetMode(1)));
    out.push(cmd("Mode: Setlist", "view", Effect::SetMode(2)));
    for (i, name) in patches {
        out.push(cmd(name, "patch", Effect::SelectPatch(*i as u32)));
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
    for s in model.library_songs.iter() {
        out.push(cmd(
            &format!("Add to set: {}", s.name),
            "setlist",
            Effect::AddSongToSet(s.name.clone()),
        ));
    }
    for (i, s) in model.setlists.iter().enumerate() {
        out.push(cmd(
            &format!("Setlist: {s}"),
            "setlist",
            Effect::SelectSetlist(i as u32),
        ));
    }
    out
}

/// Simple subsequence-ish scorer: every query word must appear.
fn matches(label: &str, hint: &str, query: &str) -> bool {
    let hay = format!("{} {}", label.to_lowercase(), hint);
    query
        .to_lowercase()
        .split_whitespace()
        .all(|w| hay.contains(w))
}

#[component]
pub fn CommandPalette(model: PerformanceModel, open: Signal<bool>) -> Element {
    let rig = use_hook(try_consume_context::<RigClient>);
    let mut query = use_signal(String::new);
    let mut cursor = use_signal(|| 0usize);
    // Argument step: the chosen effect awaiting its typed argument.
    let mut pending = use_signal(|| None::<Effect>);

    // Live patch + preset names for the jump actions.
    let lists = use_resource({
        let rig = rig.clone();
        let rev = model.revision;
        move || {
            let _ = rev;
            let rig = rig.clone();
            async move {
                match rig {
                    Some(r) => (
                        r.patches()
                            .await
                            .unwrap_or_default()
                            .iter()
                            .enumerate()
                            .map(|(i, p)| (i, p.name.clone()))
                            .collect::<Vec<_>>(),
                        r.presets()
                            .await
                            .unwrap_or_default()
                            .iter()
                            .map(|p| p.name.clone())
                            .collect::<Vec<_>>(),
                    ),
                    None => (Vec::new(), Vec::new()),
                }
            }
        }
    });
    let (patches, presets) = lists.read().clone().unwrap_or_default();

    let all = actions(&model, &patches, &presets);
    let q = query();
    let filtered: Vec<Action> = all
        .into_iter()
        .filter(|a| q.is_empty() || matches(&a.label, a.hint, &q))
        .take(12)
        .collect();
    let sel = cursor().min(filtered.len().saturating_sub(1));

    let mut close = move || {
        open.set(false);
        query.set(String::new());
        cursor.set(0);
        pending.set(None);
    };

    // Execute a non-argument effect (or stage an argument step).
    let run = {
        let rig = rig.clone();
        move |effect: Effect, arg: Option<String>| {
            if arg.is_none() && arg_prompt(&effect).is_some() {
                pending.set(Some(effect));
                query.set(String::new());
                return false; // stay open for the argument
            }
            let Some(r) = rig.clone() else { return true };
            execute(r, effect, arg.unwrap_or_default());
            true
        }
    };

    if !open() {
        return rsx! {};
    }
    let prompt = pending().as_ref().and_then(arg_prompt);

    rsx! {
        div {
            class: "fixed inset-0 z-[100] flex items-start justify-center pt-[12vh] bg-black/60",
            onclick: move |_| close(),
            div {
                class: "w-[520px] max-w-[90vw] rounded-xl border border-border bg-card shadow-2xl overflow-hidden",
                onclick: move |e: MouseEvent| e.stop_propagation(),
                input {
                    class: "w-full bg-transparent px-4 py-3 text-sm outline-none border-b border-border",
                    placeholder: prompt.unwrap_or("Type a command, patch, song…"),
                    value: "{query}",
                    autofocus: true,
                    // autofocus is ignored on dynamically-inserted nodes —
                    // grab focus explicitly every time the palette mounts.
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
                                if pending().is_some() {
                                    pending.set(None);
                                    query.set(String::new());
                                } else {
                                    close();
                                }
                            }
                            Key::ArrowDown => cursor.set((sel + 1).min(filtered.len().saturating_sub(1))),
                            Key::ArrowUp => cursor.set(sel.saturating_sub(1)),
                            Key::Enter => {
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
                    div { class: "max-h-[360px] overflow-y-auto py-1",
                        for (i, a) in filtered.iter().enumerate() {
                            {
                                let effect = a.effect.clone();
                                let mut run = run.clone();
                                rsx! {
                                    div {
                                        key: "{a.hint}-{a.label}",
                                        class: if i == sel {
                                            "flex items-center gap-2 px-4 py-1.5 text-sm bg-accent text-accent-foreground cursor-pointer"
                                        } else {
                                            "flex items-center gap-2 px-4 py-1.5 text-sm cursor-pointer hover:bg-accent/30"
                                        },
                                        onclick: move |_| {
                                            if run(effect.clone(), None) {
                                                close();
                                            }
                                        },
                                        span { class: "truncate", "{a.label}" }
                                        span { class: "ml-auto text-[9px] uppercase tracking-wider opacity-40 flex-shrink-0", "{a.hint}" }
                                    }
                                }
                            }
                        }
                        if filtered.is_empty() {
                            div { class: "px-4 py-3 text-sm text-muted-foreground italic", "no matches" }
                        }
                    }
                } else {
                    div { class: "px-4 py-2 text-[10px] text-muted-foreground",
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
                let name = std::path::Path::new(arg.trim())
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Imported".to_string());
                drop(r.add_preset(name, arg.trim().to_string()).await);
            }
        }
    });
}

/// Parse a keymap action string ("stack:0", "toggle_fx", "song:next",
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
        _ => return None,
    })
}
