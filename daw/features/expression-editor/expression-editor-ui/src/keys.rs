//! Key sequences, resolved from the shared input configuration.
//!
//! The counterpart to [`crate::scroll`], and for the same reason: a
//! binding written into a `match` in this crate is one the DAW and
//! REAPER cannot see, and it collides with whatever they already use.
//! `Z` was exactly that — hardcoded here, and already the "Zoom" which-key
//! prefix in the FTS REAPER profile, so the editor was quietly eating the
//! first key of every zoom sequence.
//!
//! So the bindings live in `input`'s keymap under the [`SURFACE`] mode
//! and are resolved through [`input::InputProcessor`], which owns the
//! **sequence state**: `z` alone resolves to nothing and leaves a
//! pending prefix, and the next key completes it. That is what makes
//! `z z` / `z i` work at all, and it is why this is a processor rather
//! than the flat table the scroll side gets away with.

use std::cell::RefCell;

use input::{
    ActionContext, InputCommand, InputEvent, InputProcessor, KeyChord, KeyCode, KeyEvent, KeyTrie,
    KeymapConfig, ModeId, Modifiers,
};

use expression_editor_core::actions as core_actions;
use expression_editor_core::memagic;
use expression_editor_core::tools::Mods;

/// The editor's mode key in the keymap.
pub const SURFACE: &str = "editor";

thread_local! {
    /// The processor, with its pending-sequence state.
    ///
    /// Thread-local rather than a signal because the state is a
    /// half-typed key sequence, not something the view renders — and a
    /// component that remounted mid-sequence would otherwise forget the
    /// prefix the user has already pressed.
    static PROCESSOR: RefCell<Option<InputProcessor>> = const { RefCell::new(None) };

    /// The chords typed so far, mirrored here because the processor
    /// reports its pending sequence only as a display string and the
    /// which-key overlay has to *walk the trie* to that point.
    static PENDING: RefCell<Vec<KeyChord>> = const { RefCell::new(Vec::new()) };
}

fn config() -> KeymapConfig {
    let mut config = input::config::load_default_config().unwrap_or_default();
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(Some(user)) = input::config::load_user_config() {
        config = KeymapConfig::merge(config, user);
    }
    config
}

/// The config as the editor's own processor sees it.
///
/// The bindings are written under an `editor` key so the keymap file
/// reads as one file per surface, but a processor resolves against its
/// *base mode* and exposes no way to change it. So the editor's bindings
/// become this processor's `normal`. The editor has no vim modes of its
/// own to lose — it is a canvas, not a text buffer.
fn editor_config() -> KeymapConfig {
    let mut cfg = config();
    if let Some(bindings) = cfg.keymap.get(SURFACE).cloned() {
        cfg.keymap.insert("normal".to_string(), bindings);
    }
    cfg
}

/// Translate a DOM-ish key name into the crate's [`KeyCode`].
///
/// Single characters are the common case and go through as characters;
/// anything longer is a named key.
fn key_code(key: &str) -> KeyCode {
    match key {
        "ArrowUp" => KeyCode::ArrowUp,
        "ArrowDown" => KeyCode::ArrowDown,
        "ArrowLeft" => KeyCode::ArrowLeft,
        "ArrowRight" => KeyCode::ArrowRight,
        "Enter" => KeyCode::Enter,
        "Escape" => KeyCode::Escape,
        "Tab" => KeyCode::Tab,
        "Backspace" => KeyCode::Backspace,
        "Delete" => KeyCode::Delete,
        _ => {
            if let Some(n) = key.strip_prefix('F').and_then(|n| n.parse::<u8>().ok())
                && (1..=12).contains(&n)
            {
                return KeyCode::F(n);
            }
            KeyCode::Character(key.to_lowercase())
        }
    }
}

/// How a chord reads in the overlay: `z`, `Ctrl+i`, `Escape`.
fn chord_label(chord: &KeyChord) -> String {
    let mut s = String::new();
    if chord.modifiers.ctrl {
        s.push_str("Ctrl+");
    }
    if chord.modifiers.alt {
        s.push_str("Alt+");
    }
    if chord.modifiers.shift {
        s.push_str("Shift+");
    }
    s.push_str(&match &chord.key {
        KeyCode::Character(c) => c.clone(),
        KeyCode::ArrowUp => "Up".into(),
        KeyCode::ArrowDown => "Down".into(),
        KeyCode::ArrowLeft => "Left".into(),
        KeyCode::ArrowRight => "Right".into(),
        KeyCode::Enter => "Enter".into(),
        KeyCode::Escape => "Escape".into(),
        KeyCode::Tab => "Tab".into(),
        KeyCode::Backspace => "Backspace".into(),
        KeyCode::Delete => "Delete".into(),
        KeyCode::F(n) => format!("F{n}"),
    });
    s
}

/// What the configuration says this key does here, in the editor mode.
///
/// Returns the actions to run — empty when the key is unbound *or* when
/// it opened or extended a sequence, which the caller must treat the
/// same way it treats a bound key: consumed. See [`is_pending`].
pub fn resolve(key: &str, mods: Mods) -> Vec<InputCommand> {
    let event = InputEvent::Key(KeyEvent {
        key: key_code(key),
        modifiers: Modifiers {
            ctrl: mods.ctrl,
            alt: mods.alt,
            shift: mods.shift,
            ..Default::default()
        },
    });
    let chord = KeyChord::new(
        key_code(key),
        Modifiers {
            ctrl: mods.ctrl,
            alt: mods.alt,
            shift: mods.shift,
            ..Default::default()
        },
    );

    let commands = PROCESSOR.with(|p| {
        let mut slot = p.borrow_mut();
        let proc = slot.get_or_insert_with(|| {
            // A broken user keymap should cost the customisation, not
            // every binding.
            InputProcessor::from_config(editor_config()).unwrap_or_default()
        });
        proc.process(event, &ActionContext::new())
    });

    // Mirror the processor's sequence state so the overlay can walk to
    // it. Extended while a prefix is live, cleared the moment it is not.
    let pending = is_pending();
    PENDING.with(|p| {
        let mut seq = p.borrow_mut();
        if pending {
            seq.push(chord);
        } else {
            seq.clear();
        }
    });

    commands
}

/// Tell the processor a key came back up.
///
/// Call this from a real `onkeyup`, and only from there. It used to be
/// called at the end of every [`resolve`] instead — a fake release on
/// the theory that this dioxus/blitz build had no key-up event. It has
/// one, and faking it was what made holding `z` spasm: the processor
/// tracks held keys precisely so OS auto-repeat cannot re-enter a
/// sequence, and a surface that reports every press as instantly
/// released turns that suppression off. Forty repeats a second then
/// walked the zoom tree over and over.
///
/// Holding a prefix is a *state*, which is what makes spring-loading
/// possible: `z` down opens the tree and arms the tool, `z` up closes
/// it. Neither half works if the processor thinks the key is already up.
///
/// Returns the processor's own answer to "should the overlay go now?" —
/// `true` when a sticky-prefix run that fired at least one action has
/// ended. The sticky behaviour itself is entirely the processor's:
/// `InputProcessor` keeps a `sticky_anchor` and rewinds the sequence to
/// it after every match while the anchor is held, so holding `g` and
/// tapping `q`, `w`, `e` fires three grid commands. Nothing here needs
/// to re-open anything; it only has to report the release.
pub fn release(key: &str, mods: Mods) -> bool {
    let chord = KeyChord::new(
        key_code(key),
        Modifiers {
            ctrl: mods.ctrl,
            alt: mods.alt,
            shift: mods.shift,
            ..Default::default()
        },
    );
    let hide = PROCESSOR.with(|p| {
        p.borrow_mut()
            .as_mut()
            .map(|proc| proc.notify_key_release(chord))
            .unwrap_or(false)
    });
    // The mirror the overlay walks has to follow the processor's state,
    // which a sticky release just cleared.
    if !is_pending() {
        PENDING.with(|p| p.borrow_mut().clear());
    }
    hide
}

/// Abandon a half-typed sequence.
///
/// Escape has to reach both halves: the processor's own state and the
/// mirror the overlay reads, or the overlay stays up over a prefix that
/// is no longer pending.
pub fn cancel() {
    PROCESSOR.with(|p| {
        if let Some(proc) = p.borrow_mut().as_mut() {
            // The public way to drop a half-typed sequence. It reports
            // the abandoned first key as unhandled, which is exactly
            // what a cancel means; nothing here wants it.
            let _ = proc.timeout_expired();
        }
    });
    PENDING.with(|p| p.borrow_mut().clear());
}

/// One row of the which-key overlay.
#[derive(Clone, Debug, PartialEq)]
pub struct Continuation {
    /// The key to press next.
    pub key: String,
    /// What it does, or the name of the group it opens.
    pub label: String,
    /// Whether it opens a further group rather than running something.
    pub is_group: bool,
}

/// What can follow the keys typed so far, for the which-key overlay.
///
/// Empty when nothing is pending, which is the overlay's cue to stay
/// hidden. Sorted, so the list does not reshuffle between presses.
pub fn continuations() -> Vec<Continuation> {
    let prefix = PENDING.with(|p| p.borrow().clone());
    if prefix.is_empty() {
        return Vec::new();
    }
    walk(prefix)
}

/// What can follow `key`, whether or not it has been pressed.
///
/// The same walk [`continuations`] does, from a prefix the caller names
/// instead of the one being typed — so a panel can show what `k` offers
/// as a way of *teaching* it, rather than only as a reply to it.
///
/// Read from the keymap rather than from a table beside it, which is the
/// point: a rebound prefix relabels itself, and a user's own bindings
/// appear without anything here knowing they exist.
pub fn continuations_after(key: &str) -> Vec<Continuation> {
    walk(vec![KeyChord::new(key_code(key), Modifiers::default())])
}

fn walk(prefix: Vec<KeyChord>) -> Vec<Continuation> {
    PROCESSOR.with(|p| {
        // `get_or_insert_with`, because a panel may ask before any key
        // has been pressed — and a processor that does not exist yet has
        // no keymap to walk, which would show an empty panel exactly
        // once per session.
        let mut slot = p.borrow_mut();
        let proc = slot.get_or_insert_with(|| {
            InputProcessor::from_config(editor_config()).unwrap_or_default()
        });
        let Some(trie) = proc.keymaps().get(&ModeId::new("normal")) else {
            return Vec::new();
        };
        // Walk to the node the typed prefix names.
        let mut node = match trie {
            KeyTrie::Node(n) => n,
            KeyTrie::Leaf(_) => return Vec::new(),
        };
        for chord in &prefix {
            match node.get(chord) {
                Some(KeyTrie::Node(n)) => node = n,
                _ => return Vec::new(),
            }
        }
        let mut out: Vec<Continuation> = node
            .children
            .iter()
            .map(|(chord, child)| Continuation {
                key: chord_label(chord),
                label: match child {
                    KeyTrie::Leaf(action) => label_for(&format!("{action:?}")),
                    KeyTrie::Node(n) => n.name.clone(),
                },
                is_group: matches!(child, KeyTrie::Node(_)),
            })
            .collect();
        out.sort_by(|a, b| a.key.cmp(&b.key));
        out
    })
}

/// Whether a sequence is half-typed, so the next key belongs to it.
///
/// The caller checks this after an empty [`resolve`]: a pending prefix
/// means the key was consumed by the sequence and must not also fall
/// through to the editor's own handling, or `z` would fire the tool
/// shortcut on its way to `z i`.
/// Whether the pending sequence is the zoom prefix, and nothing more.
///
/// The question the roll asks on a press: a drag *now* means the zoom
/// tool for the length of the hold, rather than the first half of a
/// sequence. Which is what makes `z` one idea at two speeds — tap it and
/// it is a prefix awaiting a target (`z i`), hold it and drag and it is
/// the tool.
///
/// Deliberately narrow. It is true only when `z` alone is pending, so a
/// half-typed longer sequence is never mistaken for a held tool.
pub fn zoom_prefix_held() -> bool {
    held_prefix().as_deref() == Some("z")
}

/// The single-key prefix currently held, if exactly one is.
///
/// The generalisation of [`zoom_prefix_held`], because `z` stopped being
/// the only key that is a prefix *and* a spring-loaded tool. `v` is the
/// second: tap it for the velocity tree, hold it and drag to set
/// velocity by hand.
///
/// Deliberately narrow, and for the same reason it always was: only a
/// lone pending chord counts, so a half-typed longer sequence is never
/// mistaken for a held tool.
pub fn held_prefix() -> Option<String> {
    PENDING.with(|p| {
        let pending = p.borrow();
        match pending.as_slice() {
            [only] => match &only.key {
                KeyCode::Character(c) => Some(c.clone()),
                _ => None,
            },
            _ => None,
        }
    })
}

pub fn is_pending() -> bool {
    PROCESSOR.with(|p| {
        p.borrow()
            .as_ref()
            .is_some_and(|proc| proc.pending_display().is_some())
    })
}

/// A readable name for an action id.
///
/// Read from the action's own declaration — `expression_editor_core::
/// actions` is where every editor command says what it is called. This
/// used to be a `LABELS` table right here, matched with `contains`,
/// which meant `grid.1` shadowed `grid.16` and the two lists had to be
/// hand-ordered around each other.
///
/// The fallback covers ids this editor does not own: the keymap is
/// shared, so a host may bind something from another surface.
pub fn label_for(action: &str) -> String {
    if let Some(meta) = core_actions::find(action) {
        return meta.display_name.to_string();
    }
    action
        .rsplit(['.', '_'])
        .find(|s| !s.is_empty())
        .unwrap_or(action)
        .replace('_', " ")
}

/// How an action says it is reached, for an overlay to print.
///
/// Empty when the action does not declare one, which is what
/// `ActionMeta::shortcut` means by absent.
pub fn shortcut_for(action: &str) -> &'static str {
    core_actions::find(action).map(|m| m.shortcut).unwrap_or("")
}

/// Carry out a resolved action against the editor.
///
/// Two steps, and the split is a dependency fact rather than a taste:
/// [`core_actions::run`] does everything reachable from an `&mut
/// Editor`, and what is left needs something this crate has and that one
/// cannot — `expression-editor-tools` for the velocity engines, and a
/// pointer position for MeMagic.
///
/// Velocity is absent from *both* — it holds a live shape that outlives
/// the keypress, so `crate::roll` handles it where the signal lives.
/// `tests/actions.rs` asserts that between the three nothing declared is
/// unreachable.
pub fn dispatch(
    ed: &mut expression_editor_core::Editor,
    action: &str,
    region: memagic::Region,
    anchor: memagic::Anchor,
) -> bool {
    if core_actions::run(ed, action) {
        return true;
    }
    // The contextual zoom, which is the whole point of MeMagic: the
    // region under the pointer decides what "zoom" means.
    if action == core_actions::view::MEMAGIC.id {
        return ed.memagic(region, anchor);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain() -> Mods {
        Mods {
            ctrl: false,
            alt: false,
            shift: false,
        }
    }

    fn actions(cmds: &[InputCommand]) -> Vec<String> {
        cmds.iter()
            .filter_map(|c| match c {
                InputCommand::Action(a) => Some(a.0.clone()),
                InputCommand::ActionWithArgs { action, .. } => Some(action.0.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_prefix_waits_for_its_second_key() {
        // The behaviour the whole module exists for: `z` alone must do
        // nothing and stay pending, or the editor eats the first key of
        // every zoom sequence — which is exactly what the hardcoded
        // binding did.
        let first = resolve("z", plain());
        assert!(actions(&first).is_empty(), "z alone fired something");
        assert!(is_pending(), "z should leave a sequence half-typed");
        release("z", plain());

        // `z x`, not `z z`. A sequence whose second key repeats its own
        // prefix cannot survive being *held*: the OS repeats the key,
        // and the leaf fires without anyone pressing it twice. Holding
        // `z` is now the zoom tool, so `z` had to stop being its own
        // continuation.
        let second = resolve("x", plain());
        assert_eq!(actions(&second), vec![core_actions::view::MEMAGIC.id]);
        assert!(!is_pending(), "the sequence should be finished");
        release("x", plain());
    }

    #[test]
    fn each_bound_sequence_reaches_its_action() {
        for (second, want) in [
            ("i", core_actions::view::FIT_ITEM.id),
            ("n", core_actions::view::FIT_NOTES.id),
            ("c", core_actions::view::CENTER.id),
            ("t", core_actions::view::TOP.id),
            ("b", core_actions::view::BOTTOM.id),
        ] {
            // Press and release each key, the way the surface does.
            // `resolve` no longer fakes the release for you — the
            // processor has to be able to tell a hold from a press.
            resolve("z", plain());
            release("z", plain());
            let got = actions(&resolve(second, plain()));
            release(second, plain());
            assert_eq!(got, vec![want.to_string()], "z {second}");
        }
    }

    #[test]
    fn every_binding_names_an_action_the_editor_declares() {
        // Catches a keymap entry that points at nothing — a binding that
        // looks configured and does nothing when pressed. It used to
        // check a hand-kept `ACTIONS` list; now the declarations *are*
        // the list, so this can no longer pass by both being wrong in
        // the same way.
        let cfg = config();
        let bound = cfg
            .keymap
            .get(SURFACE)
            .expect("the editor surface is in the shipped keymap");
        for action in bound.values() {
            assert!(
                core_actions::find(action).is_some(),
                "{action} is bound but no action declares it",
            );
        }
    }

    #[test]
    fn every_declared_action_is_bound_and_says_so() {
        // The other direction, and the one that keeps the overlay
        // honest: an action's `shortcut` is what a panel *prints*, and
        // the keymap is what actually happens. Nothing but this stops
        // them drifting.
        let cfg = config();
        let bound = cfg
            .keymap
            .get(SURFACE)
            .expect("the editor surface is in the shipped keymap");

        for meta in core_actions::all() {
            let keys: Vec<&str> = bound
                .iter()
                .filter(|(_, a)| a.as_str() == meta.id)
                .map(|(k, _)| k.as_str())
                .collect();
            assert!(
                !keys.is_empty(),
                "{} is declared but nothing is bound to it",
                meta.id,
            );
            assert!(
                keys.contains(&meta.shortcut),
                "{} advertises `{}` but is bound to {:?}",
                meta.id,
                meta.shortcut,
                keys,
            );
        }
    }
}
