//! The keyboard, through the FTS keybind profile.
//!
//! The profile is the one REAPER runs on (`reaper-input`'s
//! `fasttrackstudio` — `transport.styx`, `editing.styx`,
//! `navigation.styx`, …), read by `input-keybinds` into an
//! `input::InputProcessor`, so a key means the same thing in this window
//! as it does in REAPER with the extension loaded. The bindings name
//! REAPER actions by number; [`Action`] is the subset this window can
//! do, and [`action_of`] is the table from one to the other. A bound
//! action the window cannot do yet comes back as [`Action::Unbound`]
//! with its id, so it is logged rather than silently dropped.
//!
//! Where the profile is found: `FTS_INPUT_PROFILE` (a profile
//! directory), else the copy `input-keybinds` compiles in (so an
//! installed app, started from anywhere, has the same keys), else a
//! built-in copy of the bindings this window needs.
//!
//! A sequence's first key (`z`, "Zoom") leaves a pending prefix, and
//! [`Keys::which_key`] lists what can follow it, with the profile's own
//! labels, for the which-key popup ([`crate::which_key`]).

use std::collections::HashMap;
use std::path::PathBuf;

use input::{
    ActionContext, InputCommand, InputEvent, InputProcessor, KeyChord, KeyCode, KeyTrie,
    KeymapConfig, ModeId, Modifiers,
};

use crate::which_key::{Entry, WhichKey};

/// What a key asks for.
/// One slice of a stretch: its guid, where it plays and how long (µs),
/// and its stretch markers (`(played, source)`, µs).
pub type StretchEdit = (String, i64, i64, Vec<(i64, i64)>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    PlayStop,
    PlayPause,
    GoToStart,
    /// Split the selected items at the edit cursor — or, with nothing
    /// selected, every item under it.
    SplitAtCursor,
    /// QUANTIZE over the selected folded rows' span: the kit edited
    /// from its hit markers.
    QuantizeDrums,
    /// Put the drums back as played over the selected folded rows' span
    /// or the time selection (`DRUMS_REVERT`): the edit undone there, the
    /// hit markers kept.
    RevertDrums,
    /// Show or hide the Quantize panel (`crate::quantize_panel`).
    QuantizePanel,
    /// Slip-edit the whole kit — every folded row of the drums folder —
    /// by these moves: `(hit, shift)`, microseconds (whole numbers, so
    /// an action compares). What `QUANTIZE` on the Action List works
    /// out and hands the window, which carries it out where it is seen.
    /// Fill the lanes with a span of time (µs from the start) and/or
    /// these tracks (by GUID): a model looking at a fill, a section, the
    /// kit.
    Frame {
        time: Option<(i64, i64)>,
        tracks: Vec<String>,
    },
    /// Take `from..to` (µs) out of these tracks' edited takes (by GUID),
    /// or the whole kit's when there are none.
    RemoveRange {
        tracks: Vec<String>,
        from_us: i64,
        to_us: i64,
    },
    /// Hits moved by stretching the audio either side of them (a hit
    /// marker's foot dragged, `HITS_STRETCH`): each slice's new place,
    /// length and stretch markers, µs — `(guid, position, length,
    /// [(played, source)])`.
    StretchDrums {
        edits: Vec<StretchEdit>,
    },
    SlipDrums {
        moves: Vec<(i64, i64)>,
        /// The cut before each hit and the seams' crossfade, µs.
        pre_roll_us: i64,
        crossfade_us: i64,
        /// A gap longer than this is stretched over (0: never).
        stretch_over_us: i64,
        /// Every hit of the kit where it plays now, µs, by time: no seam
        /// reaches back over one.
        attacks_us: Vec<i64>,
    },
    /// Gain stage the session (Shift+G), or with `clear` (Alt+G) put every
    /// staged item back at an item volume of 1 (`crate::gain_stage`).
    GainStage {
        clear: bool,
    },
    /// Ctrl+N: comp the razor areas from take N — on a folded row, every
    /// mic in the folder at once (`arrange_edit::Editor::comp`).
    CompTake(u32),
    DeleteSelectedItems,
    SelectAllItems,
    /// Escape: no time selection, nothing selected.
    ClearSelection,
    /// The edit cursor by a bar, or by a beat.
    CursorBar(i32),
    CursorBeat(i32),
    /// Track selection, up or down the panel; `extend` keeps the rest.
    TrackStep {
        by: i32,
        extend: bool,
    },
    /// The edit cursor to the previous or next marker.
    Marker(i32),
    ToggleRecord,
    /// Something about the view rather than the session: a zoom.
    View(crate::zoom::Command),
    /// The visibility manager: show or hide a group of tracks.
    Visibility(Visibility),
    /// Show or hide the docked mixer.
    ToggleMixer,
    /// The Editor: docked under the arrangement, or a view of its own
    /// (`e`, the profile's `FTS_EXPRESSION_EDITOR_OPEN`).
    ToggleEditor,
    /// Show or hide the FX (`f`): the selected track's effects as devices.
    ToggleFx,
    /// Show or hide the browser (`b`): what to load on the selected track.
    ToggleBrowser,
    /// The view a page of bars on, or back (`]`, `[`): `SESSION_STEP`.
    BarStep(i32),
    /// To the next point of interest or the one before (`}`, `{`):
    /// `POI_STEP`.
    PoiStep(i32),
    /// The point of interest stepped to ticked off, or open again (`|`):
    /// `POI_TOGGLE`.
    PoiToggle,
    /// The window into a mode (`Alt+1` … `Alt+9`, `Alt+0`, and the
    /// `Alt+m` tree): `SESSION_MODE`, by the mode's name.
    Mode(&'static str),
    /// The FX's own view, full size (`F`): several tracks' chains.
    FxFull,
    /// Show the session as a scene (`v v` Overview, `v t` Overview TOP),
    /// or, in it already, go back to the view before.
    ToggleScene(&'static str),
    /// Save the song (Ctrl+S, REAPER's "Save project"): its `.session`,
    /// as `SESSION_SAVE` does.
    Save,
    /// Undo the last step (Ctrl+Z, REAPER's 40029), as `SESSION_UNDO`.
    Undo,
    /// Redo what was undone (Ctrl+Shift+Z, REAPER's 40030), as
    /// `SESSION_REDO`.
    Redo,
    /// A binding the window does not do yet, by its REAPER id.
    Unbound(String),
}

/// The visibility manager's actions (`v` and its tree).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Visibility {
    /// Show the group if all of it is hidden, else hide it. The group's
    /// name as the action spells it (`DRUMS`), matched loosely.
    Toggle(String),
    ShowAll,
    HideAll,
}

/// REAPER's action ids, as the profile binds them, to what this window
/// does about them.
#[must_use]
pub fn action_of(id: &str) -> Action {
    match id {
        "40044" => Action::PlayStop,
        "40073" => Action::PlayPause,
        "40042" => Action::GoToStart,
        "40757" | "40012" => Action::SplitAtCursor,
        "FTS_QUANTIZE_DRUMS" => Action::QuantizeDrums,
        "FTS_REVERT_DRUMS" => Action::RevertDrums,
        "FTS_QUANTIZE_PANEL" => Action::QuantizePanel,
        "FTS_GAIN_STAGE" => Action::GainStage { clear: false },
        "FTS_GAIN_STAGE_CLEAR" => Action::GainStage { clear: true },
        comp if comp.starts_with("FTS_COMP_TAKE_") => comp
            .trim_start_matches("FTS_COMP_TAKE_")
            .parse()
            .map_or_else(|_| Action::Unbound(comp.to_owned()), Action::CompTake),
        "40006" | "40697" => Action::DeleteSelectedItems,
        "40035" | "40182" => Action::SelectAllItems,
        "40020" => Action::ClearSelection,
        "40838" => Action::CursorBar(-1),
        "40837" => Action::CursorBar(1),
        "40646" => Action::CursorBeat(-1),
        "40647" => Action::CursorBeat(1),
        "40285" => Action::TrackStep {
            by: 1,
            extend: false,
        },
        "40286" => Action::TrackStep {
            by: -1,
            extend: false,
        },
        "40421" => Action::TrackStep {
            by: 1,
            extend: true,
        },
        "40420" => Action::TrackStep {
            by: -1,
            extend: true,
        },
        "40172" => Action::Marker(-1),
        "40173" => Action::Marker(1),
        "1013" => Action::ToggleRecord,
        "40078" => Action::ToggleMixer,
        "40026" => Action::Save,
        "40029" => Action::Undo,
        "40030" => Action::Redo,
        "FTS_EXPRESSION_EDITOR_OPEN" => Action::ToggleEditor,
        "FTS_TOGGLE_FX" => Action::ToggleFx,
        "FTS_BROWSER" => Action::ToggleBrowser,
        "FTS_STEP_NEXT" => Action::BarStep(1),
        "FTS_POI_NEXT" => Action::PoiStep(1),
        "FTS_POI_PREV" => Action::PoiStep(-1),
        "FTS_POI_TOGGLE" => Action::PoiToggle,
        "_FTS_SESSION_MODE_ORGANIZE" => Action::Mode("Organize"),
        "_FTS_SESSION_MODE_WRITE" => Action::Mode("Write"),
        "_FTS_SESSION_MODE_PRODUCE" => Action::Mode("Produce"),
        "_FTS_SESSION_MODE_RECORD" => Action::Mode("Record"),
        "_FTS_SESSION_MODE_EDIT" => Action::Mode("Edit"),
        "_FTS_SESSION_MODE_MIX" => Action::Mode("Mix"),
        "_FTS_SESSION_MODE_MASTER" => Action::Mode("Master"),
        "_FTS_SESSION_MODE_LIVE" => Action::Mode("Live"),
        "_FTS_SESSION_MODE_VIDEO" => Action::Mode("Video"),
        "_FTS_SESSION_MODE_SCORING" => Action::Mode("Scoring"),
        "FTS_STEP_PREV" => Action::BarStep(-1),
        "FTS_FX_FULL" => Action::FxFull,
        "FTS_SCENE_OVERVIEW" => Action::ToggleScene("overview"),
        "FTS_SCENE_OVERVIEW_TOP" => Action::ToggleScene("overview-top"),
        "FTS_VISIBILITY_MANAGER_SHOW_ALL" => Action::Visibility(Visibility::ShowAll),
        "FTS_VISIBILITY_MANAGER_HIDE_ALL" => Action::Visibility(Visibility::HideAll),
        other => {
            if let Some(group) = other.strip_prefix("FTS_VISIBILITY_MANAGER_TOGGLE_") {
                return Action::Visibility(Visibility::Toggle(group.to_owned()));
            }
            crate::zoom::Command::of(other)
                .map_or_else(|| Action::Unbound(other.to_owned()), Action::View)
        }
    }
}

/// The keyboard: the processor and where its map came from.
pub struct Keys {
    processor: InputProcessor,
    context: ActionContext,
    pub source: String,
    /// The which-key labels the keymap does not keep.
    labels: HashMap<Vec<KeyChord>, String>,
    /// The chords of the sequence typed so far, mirrored because the
    /// processor reports its pending sequence only as a display string,
    /// and the popup has to walk the trie to it.
    pending: Vec<KeyChord>,
}

impl Keys {
    /// Load the FTS profile, or fall back to the built-in bindings.
    #[must_use]
    pub fn load() -> Self {
        let (profile, source) = profile_dir()
            .and_then(|dir| {
                input_keybinds::load_profile_dir(&dir).map(|p| (p, dir.display().to_string()))
            })
            .or_else(|| {
                input_keybinds::embedded::fasttrackstudio()
                    .map(|p| (p, "fasttrackstudio (embedded)".to_owned()))
            })
            .or_else(|| {
                KeymapConfig::from_json_str(BUILTIN).ok().map(|keymap| {
                    let profile = input_keybinds::Profile {
                        keymap,
                        labels: HashMap::new(),
                    };
                    (profile, "built-in".to_owned())
                })
            })
            .unwrap_or_else(|| (input_keybinds::Profile::default(), "none".to_owned()));
        let processor =
            InputProcessor::from_config(profile.keymap).unwrap_or_else(|_| InputProcessor::new());
        Self {
            processor,
            context: ActionContext::new(),
            source,
            labels: profile.labels,
            pending: Vec::new(),
        }
    }

    /// A key went down. The actions it asked for, in order — empty
    /// when the key is not bound, so the window can keep its own.
    pub fn press(&mut self, key: KeyCode, modifiers: Modifiers) -> Vec<Action> {
        let chord = KeyChord::new(key.clone(), modifiers);
        let event = InputEvent::Key(input::KeyEvent { key, modifiers });
        let actions = self
            .processor
            .process(event, &self.context)
            .into_iter()
            .filter_map(|command| match command {
                InputCommand::Action(id) => Some(action_of(id.as_str())),
                InputCommand::ActionWithArgs { action, .. } => Some(action_of(action.as_str())),
                _ => None,
            })
            .collect();
        self.follow(Some(chord));
        actions
    }

    /// A key came back up. Only from a real key-up: the processor keeps
    /// held keys so auto-repeat cannot walk a tree over and over, and a
    /// held prefix is sticky (hold `z`, tap `t` then `v`: both fire).
    /// `true` when the popup should go.
    pub fn release(&mut self, key: KeyCode, modifiers: Modifiers) -> bool {
        let hide = self
            .processor
            .notify_key_release(KeyChord::new(key, modifiers));
        self.follow(None);
        hide
    }

    /// Drop a half-typed sequence: Escape, or a held `z` used as the zoom
    /// tool rather than as a prefix.
    pub fn cancel(&mut self) {
        // The public way to drop one. It hands back the abandoned first
        // key as unhandled, which nothing here wants.
        let _ = self.processor.timeout_expired();
        self.pending.clear();
    }

    /// Whether a sequence is half-typed.
    #[must_use]
    pub fn is_pending(&self) -> bool {
        self.processor.pending_display().is_some()
    }

    /// Keep the mirror in step with the processor: extended by the chord
    /// just pressed while a prefix is live, rewound to what is still
    /// pending after a sticky match, cleared when nothing is.
    fn follow(&mut self, pressed: Option<KeyChord>) {
        let Some(display) = self.processor.pending_display() else {
            self.pending.clear();
            return;
        };
        let depth = display.split_whitespace().count();
        if let Some(chord) = pressed
            && self.pending.len() < depth
        {
            self.pending.push(chord);
        }
        self.pending.truncate(depth);
    }

    /// What can follow the keys typed so far, for the which-key popup.
    /// `None` when nothing is pending.
    #[must_use]
    pub fn which_key(&self) -> Option<WhichKey> {
        if self.pending.is_empty() {
            return None;
        }
        let KeyTrie::Node(root) = self.processor.keymaps().get(&ModeId::new("normal"))? else {
            return None;
        };
        let mut node = root;
        for chord in &self.pending {
            match node.get(chord) {
                Some(KeyTrie::Node(next)) => node = next,
                _ => return None,
            }
        }
        let mut entries: Vec<Entry> = node
            .children
            .iter()
            .map(|(chord, child)| {
                let mut path = self.pending.clone();
                path.push(chord.clone());
                let (label, available) = match child {
                    KeyTrie::Leaf(input::trie::LeafAction::Action(id)) => {
                        let done = !matches!(action_of(id.as_str()), Action::Unbound(_));
                        let label = self.labels.get(&path).cloned();
                        (label.unwrap_or_else(|| id.as_str().to_owned()), done)
                    }
                    KeyTrie::Leaf(_) => {
                        (self.labels.get(&path).cloned().unwrap_or_default(), false)
                    }
                    KeyTrie::Node(n) => (
                        self.labels
                            .get(&path)
                            .cloned()
                            .unwrap_or_else(|| n.name.clone()),
                        true,
                    ),
                };
                Entry {
                    key: chord_label(chord),
                    label,
                    group: matches!(child, KeyTrie::Node(_)),
                    available,
                }
            })
            .collect();
        entries.sort_by(|a, b| a.key.cmp(&b.key));
        Some(WhichKey {
            title: self.labels.get(&self.pending).cloned().unwrap_or_default(),
            typed: self
                .pending
                .iter()
                .map(chord_label)
                .collect::<Vec<_>>()
                .join(" "),
            entries,
        })
    }
}

/// How a chord reads in the popup: `t`, `Shift+t`, `Ctrl+Up`.
fn chord_label(chord: &KeyChord) -> String {
    let mut s = String::new();
    for (held, name) in [
        (chord.modifiers.ctrl, "Ctrl+"),
        (chord.modifiers.alt, "Alt+"),
        (chord.modifiers.shift, "Shift+"),
        (chord.modifiers.meta, "Cmd+"),
    ] {
        if held {
            s.push_str(name);
        }
    }
    s.push_str(&match &chord.key {
        KeyCode::Character(c) => c.clone(),
        KeyCode::ArrowUp => "Up".into(),
        KeyCode::ArrowDown => "Down".into(),
        KeyCode::ArrowLeft => "Left".into(),
        KeyCode::ArrowRight => "Right".into(),
        KeyCode::F(n) => format!("F{n}"),
        other => format!("{other:?}"),
    });
    s
}

/// A profile directory named by the environment, for trying out a
/// profile being edited without rebuilding.
fn profile_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("FTS_INPUT_PROFILE")?);
    dir.join("profile.styx").exists().then_some(dir)
}

/// A winit key as the processor's, with the shift folded out of the
/// character: the profile writes `Shift+e`, and the keyboard gives an
/// `E` — the chord is the letter and the modifier, not the capital.
#[must_use]
pub fn key_code(named: Option<&str>, text: Option<&str>) -> Option<KeyCode> {
    match named {
        Some("Enter") => return Some(KeyCode::Enter),
        Some("Escape") => return Some(KeyCode::Escape),
        Some("Tab") => return Some(KeyCode::Tab),
        Some("Backspace") => return Some(KeyCode::Backspace),
        Some("Delete") => return Some(KeyCode::Delete),
        Some("ArrowUp") => return Some(KeyCode::ArrowUp),
        Some("ArrowDown") => return Some(KeyCode::ArrowDown),
        Some("ArrowLeft") => return Some(KeyCode::ArrowLeft),
        Some("ArrowRight") => return Some(KeyCode::ArrowRight),
        Some(f) if f.len() <= 3 && f.starts_with('F') => {
            return f
                .get(1..)
                .and_then(|n| n.parse::<u8>().ok())
                .map(KeyCode::F);
        }
        _ => {}
    }
    let text = text?;
    let mut chars = text.chars();
    let first = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    Some(KeyCode::Character(first.to_lowercase().collect()))
}

/// The bindings this window needs, in the profile's own ids, for a
/// checkout without the profile beside it. Kept in step with
/// `fasttrackstudio/*.styx` by hand; the profile wins when it is there.
const BUILTIN: &str = r#"{
  "modes": { "normal": { "display_name": "NORMAL" } },
  "keymap": {
    "normal": {
      "Space": "40044",
      "Shift+Space": "40073",
      "Enter": "40042",
      "0": "40042",
      "s": "40757",
      "d": "40006",
      "e": "FTS_EXPRESSION_EDITOR_OPEN",
      "b": "FTS_BROWSER",
      "[": "FTS_STEP_PREV",
      "]": "FTS_STEP_NEXT",
      "Shift+{": "FTS_POI_PREV",
      "Shift+}": "FTS_POI_NEXT",
      "Shift+|": "FTS_POI_TOGGLE",
      "q": "FTS_QUANTIZE_DRUMS",
      "Shift+q": "FTS_QUANTIZE_PANEL",
      "Alt+q": "FTS_REVERT_DRUMS",
      "Delete": "40697",
      "Backspace": "40697",
      "Escape": "40020",
      "Ctrl+a": "40035",
      "ArrowLeft": "40838",
      "ArrowRight": "40837",
      "h": "40838",
      "l": "40837",
      "Ctrl+h": "40646",
      "Ctrl+l": "40647",
      "ArrowUp": "40286",
      "ArrowDown": "40285",
      "j": "40285",
      "k": "40286",
      "Shift+j": "40421",
      "Shift+k": "40420",
      ",": "40172",
      ".": "40173",
      "r": "1013"
    }
  }
}"#;

#[cfg(test)]
mod tests {
    use super::*;

    /// `Alt` and a number puts the window in a mode, and `Alt+m` then a
    /// letter does too.
    #[test]
    fn braces_step_through_the_points_of_interest_and_the_bar_ticks_one_off() {
        let mut keys = Keys::load();
        let shift = Modifiers {
            shift: true,
            ..Modifiers::NONE
        };
        let control = keys.press(KeyCode::Character("]".into()), Modifiers::NONE);
        keys.release(KeyCode::Character("]".into()), Modifiers::NONE);
        assert_eq!(control, vec![Action::BarStep(1)], "]");
        for (key, action) in [
            ("{", Action::PoiStep(-1)),
            ("}", Action::PoiStep(1)),
            ("|", Action::PoiToggle),
        ] {
            let got = keys.press(KeyCode::Character(key.into()), shift);
            keys.release(KeyCode::Character(key.into()), shift);
            assert_eq!(got, vec![action], "Shift+{key}");
        }
    }

    #[test]
    fn alt_and_a_number_switches_the_mode() {
        let mut keys = Keys::load();
        let alt = Modifiers {
            alt: true,
            ..Modifiers::NONE
        };
        for (digit, mode) in [
            ("5", "Edit"),
            ("6", "Mix"),
            ("1", "Organize"),
            ("0", "Scoring"),
        ] {
            let got = keys.press(KeyCode::Character(digit.into()), alt);
            keys.release(KeyCode::Character(digit.into()), alt);
            assert_eq!(got, vec![Action::Mode(mode)], "Alt+{digit}");
        }
        keys.press(KeyCode::Character("m".into()), alt);
        let got = keys.press(KeyCode::Character("r".into()), alt);
        assert_eq!(got, vec![Action::Mode("Record")], "Alt+m r");
    }

    /// `z` opens the zoom tree with the profile's own labels, and the
    /// next key completes it into a zoom.
    #[test]
    fn z_opens_the_zoom_tree_and_t_zooms_to_the_tracks() {
        let mut keys = Keys::load();
        let z = KeyCode::Character("z".into());
        assert!(
            keys.press(z, Modifiers::NONE).is_empty(),
            "z alone is a prefix"
        );
        let which = keys.which_key().expect("the popup shows");
        assert_eq!(which.title, "Zoom");
        assert_eq!(which.typed, "z");
        let t = which
            .entries
            .iter()
            .find(|e| e.key == "t")
            .expect("z t is in the tree");
        assert!(t.available, "and this window does it");
        assert!(
            !t.label.is_empty() && !t.label.starts_with('_'),
            "{:?}",
            t.label
        );
        assert!(
            which.entries.iter().any(|e| !e.available),
            "the bindings this window does not do are listed too"
        );
        // A tap: `z` comes up before the `t` goes down.
        keys.release(KeyCode::Character("z".into()), Modifiers::NONE);
        assert!(keys.which_key().is_some(), "a tapped prefix stays open");
        assert_eq!(
            keys.press(KeyCode::Character("t".into()), Modifiers::NONE),
            vec![Action::View(crate::zoom::Command::ToggleTracks)]
        );
        assert!(keys.which_key().is_none(), "the popup goes");
    }

    /// `v v` is the Overview and `v t` Overview TOP; the vocals' toggle
    /// is `v l`.
    #[test]
    fn v_v_is_the_overview() {
        let tap = |keys: &mut Keys, k: &str| {
            let actions = keys.press(KeyCode::Character(k.into()), Modifiers::NONE);
            keys.release(KeyCode::Character(k.into()), Modifiers::NONE);
            actions
        };
        let mut keys = Keys::load();
        tap(&mut keys, "v");
        assert_eq!(tap(&mut keys, "v"), vec![Action::ToggleScene("overview")]);
        tap(&mut keys, "v");
        assert_eq!(
            tap(&mut keys, "t"),
            vec![Action::ToggleScene("overview-top")]
        );
        tap(&mut keys, "v");
        assert_eq!(
            tap(&mut keys, "l"),
            vec![Action::Visibility(Visibility::Toggle("VOCALS".to_owned()))]
        );
    }

    /// Held, the prefix is sticky: each key under it fires and the tree
    /// stays open for the next, until `z` comes up.
    #[test]
    fn a_held_prefix_fires_each_key_under_it() {
        let mut keys = Keys::load();
        let z = KeyCode::Character("z".into());
        keys.press(z.clone(), Modifiers::NONE);
        assert_eq!(
            keys.press(KeyCode::Character("t".into()), Modifiers::NONE),
            vec![Action::View(crate::zoom::Command::ToggleTracks)]
        );
        assert!(keys.which_key().is_some(), "still open while z is held");
        assert_eq!(
            keys.press(KeyCode::Character("v".into()), Modifiers::NONE),
            vec![Action::View(crate::zoom::Command::FitTracks)]
        );
        keys.release(KeyCode::Character("t".into()), Modifiers::NONE);
        keys.release(KeyCode::Character("v".into()), Modifiers::NONE);
        keys.release(z, Modifiers::NONE);
        assert!(keys.which_key().is_none(), "and closed when it comes up");
    }

    #[test]
    fn cancel_drops_the_prefix() {
        let mut keys = Keys::load();
        keys.press(KeyCode::Character("z".into()), Modifiers::NONE);
        assert!(keys.is_pending());
        keys.cancel();
        assert!(!keys.is_pending());
        assert!(keys.which_key().is_none());
    }

    /// The built-in map is the profile's ids: space plays, s splits,
    /// d deletes, and the two agree because they are one table.
    #[test]
    fn the_builtin_bindings_reach_the_actions() {
        let config = KeymapConfig::from_json_str(BUILTIN).expect("the built-in map parses");
        let mut keys = Keys {
            processor: InputProcessor::from_config(config).expect("a processor"),
            context: ActionContext::new(),
            source: "test".into(),
            labels: HashMap::new(),
            pending: Vec::new(),
        };
        assert_eq!(
            keys.press(KeyCode::Character(" ".into()), Modifiers::NONE),
            vec![Action::PlayStop]
        );
        assert_eq!(
            keys.press(KeyCode::Character("s".into()), Modifiers::NONE),
            vec![Action::SplitAtCursor]
        );
        assert_eq!(
            keys.press(KeyCode::Delete, Modifiers::NONE),
            vec![Action::DeleteSelectedItems]
        );
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        };
        assert_eq!(
            keys.press(KeyCode::Character("a".into()), ctrl),
            vec![Action::SelectAllItems]
        );
        assert_eq!(
            keys.press(KeyCode::Character("q".into()), Modifiers::NONE),
            vec![Action::QuantizeDrums]
        );
        assert_eq!(
            keys.press(KeyCode::Character("e".into()), Modifiers::NONE),
            vec![Action::ToggleEditor]
        );
        assert!(
            keys.press(KeyCode::Character("y".into()), Modifiers::NONE)
                .is_empty()
        );
    }

    fn shift() -> Modifiers {
        Modifiers {
            shift: true,
            ..Modifiers::NONE
        }
    }

    /// The profile the app ships toggles the Editor on `e`.
    #[test]
    fn the_shipped_profile_toggles_the_editor_on_e() {
        let profile = input_keybinds::embedded::fasttrackstudio().expect("the embedded profile");
        let mut keys = Keys {
            processor: InputProcessor::from_config(profile.keymap).expect("a processor"),
            context: ActionContext::new(),
            source: "test".into(),
            labels: HashMap::new(),
            pending: Vec::new(),
        };
        assert_eq!(
            keys.press(KeyCode::Character("e".into()), Modifiers::NONE),
            vec![Action::ToggleEditor]
        );
    }

    /// The profile the app ships quantizes on `q` (bound, it is the
    /// binding's, not a macro's) as the panel last did, and opens the
    /// Quantize panel on Shift+Q.
    #[test]
    fn the_shipped_profile_quantizes_on_q() {
        let profile = input_keybinds::embedded::fasttrackstudio().expect("the embedded profile");
        let mut keys = Keys {
            processor: InputProcessor::from_config(profile.keymap).expect("a processor"),
            context: ActionContext::new(),
            source: "test".into(),
            labels: HashMap::new(),
            pending: Vec::new(),
        };
        assert_eq!(
            keys.press(KeyCode::Character("q".into()), Modifiers::NONE),
            vec![Action::QuantizeDrums]
        );
        assert_eq!(
            keys.press(KeyCode::Character("q".into()), shift()),
            vec![Action::QuantizePanel]
        );
        let alt = Modifiers {
            alt: true,
            ..Modifiers::NONE
        };
        assert_eq!(
            keys.press(KeyCode::Character("q".into()), alt),
            vec![Action::RevertDrums]
        );
    }

    /// The profile the app ships gain stages on Shift+G and clears it on
    /// Alt+G.
    #[test]
    fn the_shipped_profile_gain_stages_on_g() {
        let profile = input_keybinds::embedded::fasttrackstudio().expect("the embedded profile");
        let mut keys = Keys {
            processor: InputProcessor::from_config(profile.keymap).expect("a processor"),
            context: ActionContext::new(),
            source: "test".into(),
            labels: HashMap::new(),
            pending: Vec::new(),
        };
        assert_eq!(
            keys.press(KeyCode::Character("g".into()), shift()),
            vec![Action::GainStage { clear: false }]
        );
        let alt = Modifiers {
            alt: true,
            ..Modifiers::NONE
        };
        assert_eq!(
            keys.press(KeyCode::Character("g".into()), alt),
            vec![Action::GainStage { clear: true }]
        );
    }

    /// Ctrl+N comps from take N.
    #[test]
    fn the_shipped_profile_comps_on_ctrl_digits() {
        let profile = input_keybinds::embedded::fasttrackstudio().expect("the embedded profile");
        let mut keys = Keys {
            processor: InputProcessor::from_config(profile.keymap).expect("a processor"),
            context: ActionContext::new(),
            source: "test".into(),
            labels: HashMap::new(),
            pending: Vec::new(),
        };
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        };
        assert_eq!(
            keys.press(KeyCode::Character("2".into()), ctrl),
            vec![Action::CompTake(2)]
        );
    }

    /// A capital is the letter with shift, not another key.
    #[test]
    fn a_capital_is_the_letter_and_shift() {
        assert_eq!(
            key_code(None, Some("E")),
            Some(KeyCode::Character("e".into()))
        );
        assert_eq!(key_code(Some("Delete"), None), Some(KeyCode::Delete));
        assert_eq!(key_code(Some("F3"), None), Some(KeyCode::F(3)));
    }

    #[test]
    fn an_unknown_id_is_reported_not_dropped() {
        assert_eq!(
            action_of("_FTS_SMART_DUPLICATE"),
            Action::Unbound("_FTS_SMART_DUPLICATE".into())
        );
    }
}

/// Whether a text field has the keyboard — a rename open in the
/// arrangement, the chart editor. While it does, the window's own keys
/// (the transport's) stand aside, so a space typed into a name is a space.
static TYPING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The chart editor's half of [`TYPING`]: set when it is clicked into,
/// cleared by a click anywhere else.
static EDITING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// See [`TYPING`]. Set by the arrangement every frame it is drawn (a rename
/// open or not) — so a field that closes without saying so frees the
/// keyboard a frame later rather than never.
pub fn set_typing(on: bool) {
    TYPING.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// The chart editor has the keyboard (`true`) or has lost it.
pub fn set_editing(on: bool) {
    EDITING.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Whether a text field has the keyboard: a rename, or the chart editor.
#[must_use]
pub fn typing() -> bool {
    TYPING.load(std::sync::atomic::Ordering::Relaxed)
        || EDITING.load(std::sync::atomic::Ordering::Relaxed)
}

/// Whether the window acts on the transport's keys itself
/// ([`use_window_transport_keys`]) — when it does, a panel must not act
/// on the ones that toggle (play/stop, record), or one press is two.
static WINDOW_TRANSPORT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[must_use]
pub fn window_has_transport() -> bool {
    WINDOW_TRANSPORT.load(std::sync::atomic::Ordering::Relaxed)
}

/// The transport's keys, for the WINDOW rather than any one panel.
///
/// Space plays and stops whatever has the focus — the arrangement, the
/// mixer, the progress bar just clicked, nothing at all. A panel that keeps
/// the keymap to itself hears keys only while it has the focus, which is
/// how a click on the progress bar took the space bar away. Read through
/// the same FTS keymap as everything else, so the bindings are the
/// profile's; only play/stop and go-to-start are acted on here, and the
/// arrangement leaves those to the window (see `ArrangeEditor`'s actions)
/// so a key is not acted on twice.
#[cfg(feature = "native")]
pub fn use_window_transport_keys() {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use winit::event::{ElementState, WindowEvent};
    // Space is `Key::Character(" ")` in winit 0.31, not a named key.
    use winit::keyboard::Key;

    let keys = dioxus::prelude::use_hook(|| {
        WINDOW_TRANSPORT.store(true, std::sync::atomic::Ordering::Relaxed);
        Rc::new(RefCell::new(Keys::load()))
    });
    // The window's panes, when the shell has them: `x` and `f` press them
    // here, whatever has the focus — the mixer alone has no arrangement
    // to hand its keys to.
    let mixer = dioxus::prelude::try_use_context::<crate::shell::MixerToggle>();
    let fx = dioxus::prelude::try_use_context::<crate::shell::FxToggle>();
    let editor = dioxus::prelude::try_use_context::<crate::shell::EditorPane>();
    let pins = dioxus::prelude::try_use_context::<crate::shell::Pins>();
    let held = dioxus::prelude::use_hook(|| Rc::new(Cell::new(Modifiers::NONE)));
    dioxus_native::use_window_event(move |event, _| match event {
        WindowEvent::ModifiersChanged(modifiers) => {
            let state = modifiers.state();
            held.set(Modifiers {
                ctrl: state.control_key(),
                alt: state.alt_key(),
                shift: state.shift_key(),
                meta: state.meta_key(),
            });
        }
        // The window letting go of the keyboard lets go of the keys.
        WindowEvent::Focused(false) => crate::tool::set_zoom_held(false),
        WindowEvent::KeyboardInput { event, .. } => {
            // The zoom key, by the physical key (with Shift the character
            // is "Z"), for both surfaces — see `tool::zoom_held`.
            if event.physical_key
                == winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyZ)
            {
                crate::tool::set_zoom_held(event.state.is_pressed());
            }
            let (named, text): (Option<String>, Option<String>) = match &event.logical_key {
                Key::Named(named) => (Some(format!("{named:?}")), None),
                Key::Character(c) => (None, Some(c.to_string())),
                _ => (None, None),
            };
            let Some(code) = key_code(named.as_deref(), text.as_deref()) else {
                return;
            };
            if event.state == ElementState::Released {
                keys.borrow_mut().release(code, held.get());
                return;
            }
            if event.repeat || typing() {
                return;
            }
            // The Editor half way through one of its own sequences (`g e`,
            // `a x`): the key is its, not a pane's. This hook runs before
            // the document hands the key on, so the Editor still shows the
            // sequence pending. Only the panes stand aside — the transport
            // never does, or a sequence left pending would take Space.
            let editors = expression_editor_paint::keys::is_pending();
            for action in keys.borrow_mut().press(code, held.get()) {
                if editors
                    && matches!(
                        action,
                        Action::ToggleMixer
                            | Action::ToggleFx
                            | Action::FxFull
                            | Action::ToggleEditor
                    )
                {
                    continue;
                }
                use crate::engine::{Move, transport};
                match action {
                    Action::PlayStop | Action::PlayPause => transport(Move::PlayStop, 0.0),
                    // Idempotent, so the arrangement also acting on it (it
                    // moves its edit cursor home too) does no harm. Record
                    // is not on a key yet — see `ArrangeEditor`.
                    Action::GoToStart => transport(Move::Home, 0.0),
                    Action::ToggleMixer => {
                        if let Some(mixer) = mixer {
                            mixer.press();
                        }
                    }
                    Action::ToggleEditor => {
                        if let Some(editor) = editor {
                            editor.press();
                        }
                    }
                    Action::ToggleFx => {
                        if let Some(fx) = fx {
                            fx.press();
                        }
                    }
                    Action::FxFull => {
                        if let Some(fx) = fx {
                            fx.press_full();
                        }
                    }
                    Action::ToggleBrowser => {
                        if let Some(pins) = pins {
                            use dioxus::prelude::{ReadableExt as _, WritableExt as _};
                            let mut browser = pins.browser;
                            let open = *browser.peek();
                            browser.set(!open);
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    });
}
