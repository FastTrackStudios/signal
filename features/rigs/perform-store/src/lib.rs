//! Profiles, songs and setlists for a rig that keeps them apart from the
//! guitar's — the drums, the e-kit, the keys — and the shared performance
//! surface ([`RigPerform`](signal_rigs_proto::perform::RigPerform)) answered
//! from them.
//!
//! Each rig has its own directory (`<config>/signal/<rig>/perform/`):
//!
//!   profiles.styx   profiles: patches (a preset's variation, named) and
//!                   the stacks that rotate through them
//!   songs.styx      songs: key, tempo, the profile they play on, their
//!                   parts and what each plays
//!   setlists.styx   setlists: songs in order, each with its key, tempo and
//!                   start for that set
//!   cursor.styx     where the rig is: the mode, profile, set, song, part
//!                   and patch, and each stack's place in its rotation
//!
//! The rig says what presets it has and plays one ([`PresetPlayer`]); the
//! rest is here. [`impl_rig_perform!`] writes the rig's `RigPerform` from
//! it. A rig's first start seeds a profile from its presets — a stack each,
//! its variations the patches — and an empty set.

use std::sync::{Mutex, MutexGuard};

use architect::PubSub;
use facet::Facet;
use signal_rig_host::store::StyxDir;
use signal_rigs_proto::perform::{
    Applied, LibraryModel, PatchInfo, PerfPart, PerfStack, PerformEvent, PerformState,
    ProfileEntry, ProfilePatch, SetlistEntry, SongEntry, SongSlot,
};

/// How a patch made from a preset's variation is named: `Kit · Rock Mix`.
pub const SEP: &str = " · ";

/// The variation a preset plays as when none is named.
pub const DEFAULT_VARIATION: &str = "Default";

/// What the rig plays: its presets and their variations, and one of them
/// on. Everything else on the performance surface is the store's.
pub trait PresetPlayer {
    /// Every preset, with its variations in order.
    fn presets(&self) -> Vec<(String, Vec<String>)>;
    /// Play a preset's variation.
    fn play(&self, preset: &str, variation: &str) -> Applied;
    /// The presets, their variations and the modules they compose, for the
    /// browser. Empty unless the rig has modules to show.
    fn compositions(&self) -> signal_rigs_proto::perform::CompositionModel {
        signal_rigs_proto::perform::CompositionModel::default()
    }
    /// Save what plays now as `preset`'s `variation` (made if missing). A
    /// rig whose presets are its library's refuses.
    fn save_variation(&self, _preset: &str, _variation: &str) -> Applied {
        Applied::refused("this rig's presets are its library's")
    }
    /// A new preset `new_name`: a copy of `name`, or (no such preset) what
    /// plays now.
    fn duplicate_preset(&self, _name: &str, _new_name: &str) -> Applied {
        Applied::refused("this rig's presets are its library's")
    }
    fn rename_preset(&self, _old: &str, _new_name: &str) -> Applied {
        Applied::refused("this rig's presets are its library's")
    }
    fn delete_preset(&self, _name: &str) -> Applied {
        Applied::refused("this rig's presets are its library's")
    }
}

// ── What is stored ────────────────────────────────────────────────────────

/// A patch: a preset's variation, under a name of its own.
#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct PatchDef {
    pub name: String,
    pub preset: String,
    #[facet(default)]
    pub variation: String,
}

/// A stack: a switch's rotation of patches, by name.
#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct StackDef {
    pub name: String,
    #[facet(default)]
    pub patches: Vec<String>,
}

/// A profile: its patches and the stacks that play them.
#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct ProfileDef {
    pub name: String,
    #[facet(default)]
    pub patches: Vec<PatchDef>,
    #[facet(default)]
    pub stacks: Vec<StackDef>,
    /// The patch it lands on when loaded; empty: its first stack's first.
    #[facet(default)]
    pub default_patch: String,
}

#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct ProfileLib {
    #[facet(default)]
    pub profiles: Vec<ProfileDef>,
}

/// A song's part and what it plays.
#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct PartDef {
    pub name: String,
    /// The patch it plays; empty: it keeps what is up.
    #[facet(default)]
    pub patch: String,
    /// The section it belongs to; empty: its own.
    #[facet(default)]
    pub section: String,
    /// The profile it plays on, when not the song's.
    #[facet(default)]
    pub profile: String,
}

#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct SongDef {
    pub name: String,
    #[facet(default)]
    pub key: String,
    #[facet(default)]
    pub bpm: u32,
    /// The profile it plays on; empty keeps whatever is loaded.
    #[facet(default)]
    pub profile: String,
    /// `#rrggbb`; empty: one from its name.
    #[facet(default)]
    pub colour: String,
    #[facet(default)]
    pub parts: Vec<PartDef>,
}

#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct SongLib {
    #[facet(default)]
    pub songs: Vec<SongDef>,
}

/// A song in a set, with the set's key, tempo and start.
#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct EntryDef {
    pub song: String,
    /// Empty / 0: the song's own.
    #[facet(default)]
    pub key: String,
    #[facet(default)]
    pub bpm: u32,
    /// The patch it starts on; empty: the song's first part's.
    #[facet(default)]
    pub start: String,
}

#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct SetlistDef {
    pub name: String,
    #[facet(default)]
    pub date: String,
    #[facet(default)]
    pub event: String,
    #[facet(default)]
    pub title: String,
    #[facet(default)]
    pub profile: String,
    #[facet(default)]
    pub entries: Vec<EntryDef>,
}

#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct SetlistLib {
    #[facet(default)]
    pub setlists: Vec<SetlistDef>,
}

/// Where the rig is.
#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct Cursor {
    #[facet(default)]
    pub mode: u32,
    #[facet(default)]
    pub profile: String,
    #[facet(default)]
    pub setlist: u32,
    #[facet(default)]
    pub song: u32,
    #[facet(default)]
    pub part: u32,
    /// The patch playing.
    #[facet(default)]
    pub patch: String,
    /// Each stack's place in its rotation, in stack order.
    #[facet(default)]
    pub positions: Vec<u32>,
}

const PROFILES: &str = "profiles.styx";
const SONGS: &str = "songs.styx";
const SETLISTS: &str = "setlists.styx";
const CURSOR: &str = "cursor.styx";

struct Lib {
    profiles: Vec<ProfileDef>,
    songs: Vec<SongDef>,
    setlists: Vec<SetlistDef>,
    cursor: Cursor,
    revision: u64,
}

impl Lib {
    fn profile_index(&self) -> usize {
        self.profiles
            .iter()
            .position(|p| p.name == self.cursor.profile)
            .unwrap_or(0)
    }
    fn profile(&self) -> Option<&ProfileDef> {
        self.profiles.get(self.profile_index())
    }
    fn profile_mut(&mut self) -> Option<&mut ProfileDef> {
        let i = self.profile_index();
        self.profiles.get_mut(i)
    }
    fn setlist(&self) -> Option<&SetlistDef> {
        self.setlists.get(self.cursor.setlist as usize)
    }
    /// The song that is up, by its name in the set.
    fn song_name(&self) -> Option<String> {
        self.setlist()?
            .entries
            .get(self.cursor.song as usize)
            .map(|e| e.song.clone())
    }
    fn song_mut(&mut self, name: &str) -> Option<&mut SongDef> {
        self.songs.iter_mut().find(|s| s.name == name)
    }
    fn current_song_mut(&mut self) -> Option<&mut SongDef> {
        let name = self.song_name()?;
        self.song_mut(&name)
    }
}

/// One rig's profiles, songs and setlists, and its performance hub.
pub struct PerformStore {
    dir: StyxDir,
    lib: Mutex<Lib>,
    hub: PubSub<PerformEvent>,
}

/// The store's directory for a rig: `<config>/signal/<rig>/perform/`.
#[must_use]
pub fn rig_dir(rig: signal_rigs_proto::Rig) -> std::path::PathBuf {
    signal_rig_host::store::signal_config_dir()
        .join(rig.slug())
        .join("perform")
}

/// A profile from the rig's presets: a stack each, its variations the
/// patches.
fn seeded_profile(name: &str, presets: &[(String, Vec<String>)]) -> ProfileDef {
    let mut profile = ProfileDef {
        name: name.to_string(),
        ..ProfileDef::default()
    };
    for (preset, variations) in presets {
        let variations = if variations.is_empty() {
            vec![DEFAULT_VARIATION.to_string()]
        } else {
            variations.clone()
        };
        let mut stack = StackDef {
            name: preset.clone(),
            patches: Vec::new(),
        };
        for v in variations {
            let name = format!("{preset}{SEP}{v}");
            if profile.patches.iter().any(|p| p.name == name) {
                continue;
            }
            profile.patches.push(PatchDef {
                name: name.clone(),
                preset: preset.clone(),
                variation: v,
            });
            stack.patches.push(name);
        }
        profile.stacks.push(stack);
    }
    profile
}

/// A profile with each patch once, and each stack naming each patch once.
fn repair(p: &mut ProfileDef) {
    let mut seen: Vec<String> = Vec::new();
    p.patches.retain(|x| {
        let fresh = !seen.contains(&x.name);
        if fresh {
            seen.push(x.name.clone());
        }
        fresh
    });
    for s in &mut p.stacks {
        let mut seen: Vec<String> = Vec::new();
        s.patches.retain(|x| {
            let fresh = !seen.contains(x);
            if fresh {
                seen.push(x.clone());
            }
            fresh
        });
    }
}

/// `name`, or `name 2`, `name 3`… — the first not in `taken`.
fn fresh(name: &str, taken: &[String]) -> String {
    if !taken.iter().any(|t| t == name) {
        return name.to_string();
    }
    (2..)
        .map(|n| format!("{name} {n}"))
        .find(|c| !taken.iter().any(|t| t == c))
        .unwrap_or_default()
}

fn move_in<T>(v: &mut Vec<T>, from: usize, to: usize) -> bool {
    if from >= v.len() || to >= v.len() {
        return false;
    }
    let x = v.remove(from);
    v.insert(to, x);
    true
}

impl PerformStore {
    /// Open the store in `dir`, seeding what is missing from the rig's
    /// presets.
    pub fn open(dir: impl Into<std::path::PathBuf>, player: &dyn PresetPlayer) -> Self {
        let dir = StyxDir::new(dir);
        let mut profiles = dir
            .read::<ProfileLib>(PROFILES)
            .map(|l| l.profiles)
            .unwrap_or_default();
        // Patch names are what everything points at — a profile that came
        // to hold one twice (a hand edit, an older seed) keeps the first.
        for p in &mut profiles {
            repair(p);
        }
        let seeded = profiles.is_empty();
        if seeded {
            profiles.push(seeded_profile("Default", &player.presets()));
        }
        let songs = dir
            .read::<SongLib>(SONGS)
            .map(|l| l.songs)
            .unwrap_or_default();
        let mut setlists = dir
            .read::<SetlistLib>(SETLISTS)
            .map(|l| l.setlists)
            .unwrap_or_default();
        if setlists.is_empty() {
            setlists.push(SetlistDef {
                name: "Set 1".into(),
                ..SetlistDef::default()
            });
        }
        let cursor = dir.read::<Cursor>(CURSOR).unwrap_or_default();
        let store = Self {
            dir,
            lib: Mutex::new(Lib {
                profiles,
                songs,
                setlists,
                cursor,
                revision: 0,
            }),
            // A late subscriber gets the latest state, as the rigs' own hubs do.
            hub: PubSub::sliding(64),
        };
        if seeded {
            store.save_all();
        }
        store
    }

    /// Seed the profile again when the rig's presets arrive after it was
    /// opened (a library still scanning): only a profile nothing was ever
    /// put in — one, with no stacks — is replaced.
    pub fn reseed_if_empty(&self, player: &dyn PresetPlayer) {
        let empty = {
            let l = self.lib();
            l.profiles.len() == 1
                && l.profiles[0].stacks.is_empty()
                && l.profiles[0].patches.is_empty()
        };
        if !empty {
            return;
        }
        let presets = player.presets();
        if presets.is_empty() {
            return;
        }
        self.edit(player, |l| {
            let name = l.profiles[0].name.clone();
            l.profiles[0] = seeded_profile(&name, &presets);
        });
    }

    /// The hub every remote's stream reads.
    pub fn hub(&self) -> &PubSub<PerformEvent> {
        &self.hub
    }

    fn lib(&self) -> MutexGuard<'_, Lib> {
        self.lib
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn save_all(&self) {
        let l = self.lib();
        self.dir.write(
            PROFILES,
            &ProfileLib {
                profiles: l.profiles.clone(),
            },
        );
        self.dir.write(
            SONGS,
            &SongLib {
                songs: l.songs.clone(),
            },
        );
        self.dir.write(
            SETLISTS,
            &SetlistLib {
                setlists: l.setlists.clone(),
            },
        );
        self.dir.write(CURSOR, &l.cursor);
    }

    /// Change the library, keep it, and tell every remote.
    fn edit(&self, player: &dyn PresetPlayer, f: impl FnOnce(&mut Lib)) {
        {
            let mut l = self.lib();
            f(&mut l);
            l.revision += 1;
        }
        self.save_all();
        self.publish(player);
    }

    /// The rig's presets changed: a profile still empty is seeded from
    /// them, and every remote hears (its lists refetch on the revision).
    pub fn changed(&self, player: &dyn PresetPlayer) {
        self.reseed_if_empty(player);
        self.edit(player, |_| {});
    }

    /// A preset was renamed: the patches that play it follow.
    pub fn preset_renamed(&self, player: &dyn PresetPlayer, old: &str, new_name: &str) {
        self.edit(player, |l| {
            for p in &mut l.profiles {
                for x in p.patches.iter_mut().filter(|x| x.preset == old) {
                    x.preset = new_name.to_string();
                }
            }
        });
    }

    /// Tell every remote the state now.
    pub fn publish(&self, player: &dyn PresetPlayer) {
        self.hub.publish(PerformEvent::State(self.state(player)));
    }

    /// An edit refused: one warning, and the library as it was.
    fn refuse(op: &str, why: &str) {
        tracing::warn!(op, why, "perform store: refused");
    }

    // ── State ──

    pub fn state(&self, _player: &dyn PresetPlayer) -> PerformState {
        let l = self.lib();
        let profile = l.profile().cloned().unwrap_or_default();
        let sound = |name: &str| {
            profile
                .patches
                .iter()
                .find(|p| p.name == name)
                .map(|p| format!("{}{SEP}{}", p.preset, p.variation))
                .unwrap_or_default()
        };
        let stacks = profile
            .stacks
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let pos = (l.cursor.positions.get(i).copied().unwrap_or(0) as usize)
                    .min(s.patches.len().saturating_sub(1));
                let current = s.patches.get(pos).cloned().unwrap_or_default();
                PerfStack {
                    name: s.name.clone(),
                    preset: sound(&current),
                    current_patch: current,
                    position: pos as u32,
                    patch_count: s.patches.len() as u32,
                    available: true,
                    is_active: s.patches.iter().any(|p| *p == l.cursor.patch),
                    override_modules: Vec::new(),
                    momentary: false,
                    no_rotate: false,
                    part_tuned: false,
                    song_tuned: false,
                    patches: s.patches.clone(),
                }
            })
            .collect();
        let slot = |e: &EntryDef| {
            let song = l.songs.iter().find(|s| s.name == e.song);
            SongSlot {
                name: e.song.clone(),
                key: if e.key.is_empty() {
                    song.map(|s| s.key.clone()).unwrap_or_default()
                } else {
                    e.key.clone()
                },
                bpm: if e.bpm == 0 {
                    song.map_or(0, |s| s.bpm)
                } else {
                    e.bpm
                },
                start: e.start.clone(),
                colour: song.map(|s| s.colour.clone()).unwrap_or_default(),
            }
        };
        let songs: Vec<SongSlot> = l
            .setlist()
            .map(|s| s.entries.iter().map(slot).collect())
            .unwrap_or_default();
        let current = l
            .song_name()
            .and_then(|n| l.songs.iter().find(|s| s.name == n).cloned());
        PerformState {
            mode: l.cursor.mode,
            profile_name: profile.name.clone(),
            stacks,
            active_patch: l.cursor.patch.clone(),
            tempo_bpm: songs.get(l.cursor.song as usize).map_or(0, |s| s.bpm),
            songs,
            song_index: l.cursor.song,
            setlists: l.setlists.iter().map(|s| s.name.clone()).collect(),
            setlist_index: l.cursor.setlist,
            library_songs: l
                .songs
                .iter()
                .map(|s| SongSlot {
                    name: s.name.clone(),
                    key: s.key.clone(),
                    bpm: s.bpm,
                    start: String::new(),
                    colour: s.colour.clone(),
                })
                .collect(),
            parts: current
                .as_ref()
                .map(|s| s.parts.iter().map(perf_part).collect())
                .unwrap_or_default(),
            part_index: l.cursor.part,
            song_profile: current.map(|s| s.profile).unwrap_or_default(),
            revision: l.revision,
            ..PerformState::default()
        }
    }

    pub fn set_mode(&self, player: &dyn PresetPlayer, mode: u32) {
        self.edit(player, |l| l.cursor.mode = mode.min(2));
    }

    // ── Patches ──

    pub fn patches(&self) -> Vec<PatchInfo> {
        let l = self.lib();
        let Some(profile) = l.profile() else {
            return Vec::new();
        };
        profile
            .patches
            .iter()
            .map(|p| {
                let stack = profile.stacks.iter().find(|s| s.patches.contains(&p.name));
                PatchInfo {
                    name: p.name.clone(),
                    stack: stack.map(|s| s.name.clone()).unwrap_or_default(),
                    available: true,
                    active: p.name == l.cursor.patch,
                    preset: format!("{}{SEP}{}", p.preset, p.variation),
                    rig_preset: p.preset.clone(),
                    variation: p.variation.clone(),
                    default_in_stack: stack.is_some_and(|s| s.patches.first() == Some(&p.name)),
                    override_modules: Vec::new(),
                    role: String::new(),
                }
            })
            .collect()
    }

    /// Play the current profile's patch `name`.
    pub fn play_patch(&self, player: &dyn PresetPlayer, name: &str) -> Applied {
        let patch = self
            .lib()
            .profile()
            .and_then(|p| p.patches.iter().find(|x| x.name == name).cloned());
        let Some(patch) = patch else {
            return Applied::refused(format!("no patch {name:?} in this profile"));
        };
        let variation = if patch.variation.is_empty() {
            DEFAULT_VARIATION
        } else {
            &patch.variation
        };
        let a = player.play(&patch.preset, variation);
        if a.ok {
            self.edit(player, |l| {
                l.cursor.patch = patch.name.clone();
                let stacks = l.profile().map(|p| p.stacks.clone()).unwrap_or_default();
                l.cursor.positions.resize(stacks.len(), 0);
                for (i, s) in stacks.iter().enumerate() {
                    if let Some(at) = s.patches.iter().position(|p| *p == patch.name) {
                        l.cursor.positions[i] = at as u32;
                    }
                }
            });
        }
        a
    }

    pub fn select_patch(&self, player: &dyn PresetPlayer, index: u32) -> Applied {
        match self.patches().get(index as usize) {
            Some(p) => self.play_patch(player, &p.name.clone()),
            None => Applied::refused(format!("no patch {index}")),
        }
    }

    /// Play a preset's variation: by the profile's patch for it when there
    /// is one, else straight.
    pub fn choose_preset(
        &self,
        player: &dyn PresetPlayer,
        preset: &str,
        variation: &str,
    ) -> Applied {
        let named = self.lib().profile().and_then(|p| {
            p.patches
                .iter()
                .find(|x| x.preset == preset && x.variation == variation)
                .map(|x| x.name.clone())
        });
        match named {
            Some(name) => self.play_patch(player, &name),
            None => {
                let a = player.play(preset, variation);
                if a.ok {
                    self.edit(player, |l| l.cursor.patch.clear());
                }
                a
            }
        }
    }

    // ── Profiles and stacks ──

    pub fn library(&self) -> LibraryModel {
        let l = self.lib();
        let active = l.profile().map(|p| p.name.clone()).unwrap_or_default();
        LibraryModel {
            profiles: l
                .profiles
                .iter()
                .map(|p| ProfileEntry {
                    name: p.name.clone(),
                    active: p.name == active,
                    stacks: p.stacks.iter().map(|s| s.name.clone()).collect(),
                    patches: p.patches.len() as u32,
                    presets: {
                        let mut v: Vec<String> =
                            p.patches.iter().map(|x| x.preset.clone()).collect();
                        v.dedup();
                        v
                    },
                    patch_list: p
                        .patches
                        .iter()
                        .map(|x| ProfilePatch {
                            name: x.name.clone(),
                            stack: p
                                .stacks
                                .iter()
                                .find(|s| s.patches.contains(&x.name))
                                .map(|s| s.name.clone())
                                .unwrap_or_default(),
                            song: String::new(),
                        })
                        .collect(),
                    default_patch: p.default_patch.clone(),
                })
                .collect(),
            songs: l
                .songs
                .iter()
                .map(|s| SongEntry {
                    name: s.name.clone(),
                    key: s.key.clone(),
                    bpm: s.bpm,
                    parts: s.parts.iter().map(|p| p.name.clone()).collect(),
                    part_list: s.parts.iter().map(perf_part).collect(),
                    setlists: l
                        .setlists
                        .iter()
                        .filter(|x| x.entries.iter().any(|e| e.song == s.name))
                        .map(|x| x.name.clone())
                        .collect(),
                    profile: s.profile.clone(),
                    start_part: String::new(),
                    colour: s.colour.clone(),
                    artist: String::new(),
                    genre: String::new(),
                })
                .collect(),
            setlists: l
                .setlists
                .iter()
                .enumerate()
                .map(|(i, s)| SetlistEntry {
                    name: s.name.clone(),
                    active: i as u32 == l.cursor.setlist,
                    songs: s
                        .entries
                        .iter()
                        .map(|e| SongSlot {
                            name: e.song.clone(),
                            key: e.key.clone(),
                            bpm: e.bpm,
                            start: e.start.clone(),
                            colour: String::new(),
                        })
                        .collect(),
                    date: s.date.clone(),
                    event: s.event.clone(),
                    title: s.title.clone(),
                    profile: s.profile.clone(),
                })
                .collect(),
            ..LibraryModel::default()
        }
    }

    /// Play profile `name` from its default patch.
    pub fn select_profile(&self, player: &dyn PresetPlayer, name: &str) -> Applied {
        let Some(p) = self.lib().profiles.iter().find(|p| p.name == name).cloned() else {
            return Applied::refused(format!("no profile {name:?}"));
        };
        self.edit(player, |l| {
            l.cursor.profile = p.name.clone();
            l.cursor.positions = vec![0; p.stacks.len()];
        });
        let start = if p.default_patch.is_empty() {
            p.stacks.first().and_then(|s| s.patches.first().cloned())
        } else {
            Some(p.default_patch.clone())
        };
        match start {
            Some(patch) => self.play_patch(player, &patch),
            None => Applied::done(),
        }
    }

    /// A new profile: a copy of `from`, or (empty) one from the presets.
    pub fn add_profile(&self, player: &dyn PresetPlayer, name: &str, from: &str) {
        let taken: Vec<String> = self.lib().profiles.iter().map(|p| p.name.clone()).collect();
        let name = fresh(name, &taken);
        let base = self.lib().profiles.iter().find(|p| p.name == from).cloned();
        let profile = match base {
            Some(mut p) => {
                p.name = name;
                p
            }
            None => seeded_profile(&name, &player.presets()),
        };
        self.edit(player, |l| l.profiles.push(profile));
    }

    pub fn rename_profile(&self, player: &dyn PresetPlayer, old: &str, new_name: &str) {
        if new_name.trim().is_empty() || self.lib().profiles.iter().any(|p| p.name == new_name) {
            return Self::refuse("rename_profile", "the name is empty or taken");
        }
        self.edit(player, |l| {
            for p in l.profiles.iter_mut().filter(|p| p.name == old) {
                p.name = new_name.to_string();
            }
            if l.cursor.profile == old {
                l.cursor.profile = new_name.to_string();
            }
            for s in &mut l.songs {
                if s.profile == old {
                    s.profile = new_name.to_string();
                }
                for part in s.parts.iter_mut().filter(|x| x.profile == old) {
                    part.profile = new_name.to_string();
                }
            }
            for s in l.setlists.iter_mut().filter(|s| s.profile == old) {
                s.profile = new_name.to_string();
            }
        });
    }

    pub fn delete_profile(&self, player: &dyn PresetPlayer, name: &str) {
        let (playing, count) = {
            let l = self.lib();
            (
                l.profile().map(|p| p.name.clone()).unwrap_or_default(),
                l.profiles.len(),
            )
        };
        if playing == name || count <= 1 {
            return Self::refuse("delete_profile", "it is playing, or the last");
        }
        self.edit(player, |l| l.profiles.retain(|p| p.name != name));
    }

    /// Press stack `index`: play it, or — up already — step its rotation.
    pub fn press_stack(&self, player: &dyn PresetPlayer, index: u32) {
        let (stack, pos, active) = {
            let l = self.lib();
            let Some(s) = l
                .profile()
                .and_then(|p| p.stacks.get(index as usize))
                .cloned()
            else {
                return;
            };
            let pos = l.cursor.positions.get(index as usize).copied().unwrap_or(0) as usize;
            let active = s.patches.contains(&l.cursor.patch);
            (s, pos, active)
        };
        if stack.patches.is_empty() {
            return;
        }
        let at = if active {
            (pos + 1) % stack.patches.len()
        } else {
            pos.min(stack.patches.len() - 1)
        };
        let _ = self.play_patch(player, &stack.patches[at]);
    }

    pub fn play_stack_patch(
        &self,
        player: &dyn PresetPlayer,
        index: u32,
        position: u32,
    ) -> Applied {
        let patch = self
            .lib()
            .profile()
            .and_then(|p| p.stacks.get(index as usize))
            .and_then(|s| s.patches.get(position as usize).cloned());
        match patch {
            Some(p) => self.play_patch(player, &p),
            None => Applied::refused(format!("no patch {position} in stack {index}")),
        }
    }

    /// Put `patch` in stack `stack`'s rotation (`on`), or take it out. A
    /// patch the profile lacks comes in from its name (`Preset · Variation`),
    /// else as a copy of the stack's first patch.
    pub fn set_stack_patch(&self, player: &dyn PresetPlayer, stack: &str, patch: &str, on: bool) {
        self.edit(player, |l| {
            let Some(p) = l.profile_mut() else { return };
            if on && !p.patches.iter().any(|x| x.name == patch) {
                let made = match patch.split_once(SEP) {
                    Some((preset, variation)) => PatchDef {
                        name: patch.to_string(),
                        preset: preset.to_string(),
                        variation: variation.to_string(),
                    },
                    None => {
                        let first = p
                            .stacks
                            .iter()
                            .find(|s| s.name == stack)
                            .and_then(|s| s.patches.first())
                            .and_then(|n| p.patches.iter().find(|x| x.name == *n))
                            .cloned();
                        match first {
                            Some(f) => PatchDef {
                                name: patch.to_string(),
                                ..f
                            },
                            None => return,
                        }
                    }
                };
                p.patches.push(made);
            }
            if let Some(s) = p.stacks.iter_mut().find(|s| s.name == stack) {
                if on {
                    if !s.patches.iter().any(|x| x == patch) {
                        s.patches.push(patch.to_string());
                    }
                } else {
                    s.patches.retain(|x| x != patch);
                }
            }
        });
    }

    pub fn move_stack_patch(&self, player: &dyn PresetPlayer, stack: &str, from: u32, to: u32) {
        self.edit(player, |l| {
            if let Some(s) = l
                .profile_mut()
                .and_then(|p| p.stacks.iter_mut().find(|s| s.name == stack))
            {
                move_in(&mut s.patches, from as usize, to as usize);
            }
        });
    }

    pub fn move_stack(&self, player: &dyn PresetPlayer, from: u32, to: u32) {
        self.edit(player, |l| {
            if let Some(p) = l.profile_mut()
                && move_in(&mut p.stacks, from as usize, to as usize)
            {
                let n = l.profile().map_or(0, |p| p.stacks.len());
                l.cursor.positions.resize(n, 0);
                move_in(&mut l.cursor.positions, from as usize, to as usize);
            }
        });
    }

    pub fn rename_stack(&self, player: &dyn PresetPlayer, old: &str, new_name: &str) {
        self.edit(player, |l| {
            if let Some(s) = l
                .profile_mut()
                .and_then(|p| p.stacks.iter_mut().find(|s| s.name == old))
            {
                s.name = new_name.to_string();
            }
        });
    }

    /// Rename a patch: its stacks, the parts that play it and the profile's
    /// default follow.
    pub fn rename_patch(&self, player: &dyn PresetPlayer, old: &str, new_name: &str) {
        if new_name.trim().is_empty()
            || self
                .lib()
                .profile()
                .is_some_and(|p| p.patches.iter().any(|x| x.name == new_name))
        {
            return Self::refuse("rename_patch", "the name is empty or taken");
        }
        self.edit(player, |l| {
            let profile = l.profile().map(|p| p.name.clone()).unwrap_or_default();
            if let Some(p) = l.profile_mut() {
                for x in p.patches.iter_mut().filter(|x| x.name == old) {
                    x.name = new_name.to_string();
                }
                for s in &mut p.stacks {
                    for x in s.patches.iter_mut().filter(|x| *x == old) {
                        *x = new_name.to_string();
                    }
                }
                if p.default_patch == old {
                    p.default_patch = new_name.to_string();
                }
            }
            for s in &mut l.songs {
                let on_profile = s.profile.is_empty() || s.profile == profile;
                for part in s.parts.iter_mut().filter(|x| x.patch == old && on_profile) {
                    part.patch = new_name.to_string();
                }
            }
            if l.cursor.patch == old {
                l.cursor.patch = new_name.to_string();
            }
        });
    }

    // ── Songs and parts ──

    /// Go to song `index` of the set: its profile, then its start.
    pub fn select_song(&self, player: &dyn PresetPlayer, index: u32) -> Applied {
        let (entry, song) = {
            let l = self.lib();
            let Some(e) = l
                .setlist()
                .and_then(|s| s.entries.get(index as usize))
                .cloned()
            else {
                return Applied::refused(format!("no song {index} in this set"));
            };
            let song = l.songs.iter().find(|s| s.name == e.song).cloned();
            (e, song)
        };
        self.edit(player, |l| {
            l.cursor.song = index;
            l.cursor.part = 0;
        });
        let song = song.unwrap_or_default();
        if !song.profile.is_empty() && self.lib().profile().is_some_and(|p| p.name != song.profile)
        {
            let a = self.select_profile(player, &song.profile);
            if !a.ok {
                return a;
            }
        }
        let start = if entry.start.is_empty() {
            song.parts
                .first()
                .map(|p| p.patch.clone())
                .unwrap_or_default()
        } else {
            entry.start
        };
        if start.is_empty() {
            Applied::done()
        } else {
            self.play_patch(player, &start)
        }
    }

    /// Go to part `index` of the song that is up, and play it.
    pub fn select_part(&self, player: &dyn PresetPlayer, index: u32) -> Applied {
        let part = {
            let l = self.lib();
            let Some(name) = l.song_name() else {
                return Applied::refused("no song is up");
            };
            l.songs
                .iter()
                .find(|s| s.name == name)
                .and_then(|s| s.parts.get(index as usize))
                .cloned()
        };
        let Some(part) = part else {
            return Applied::refused(format!("no part {index}"));
        };
        self.edit(player, |l| l.cursor.part = index);
        if !part.profile.is_empty() && self.lib().profile().is_some_and(|p| p.name != part.profile)
        {
            let a = self.select_profile(player, &part.profile);
            if !a.ok {
                return a;
            }
        }
        if part.patch.is_empty() {
            Applied::done()
        } else {
            self.play_patch(player, &part.patch)
        }
    }

    /// A change to the song that is up.
    fn edit_current(&self, player: &dyn PresetPlayer, op: &str, f: impl FnOnce(&mut SongDef)) {
        if self.lib().song_name().is_none() {
            return Self::refuse(op, "no song is up");
        }
        self.edit(player, |l| {
            if let Some(s) = l.current_song_mut() {
                f(s);
            }
        });
    }

    pub fn add_part(&self, player: &dyn PresetPlayer, name: &str) {
        let name = name.to_string();
        self.edit_current(player, "add_part", move |s| {
            let taken: Vec<String> = s.parts.iter().map(|p| p.name.clone()).collect();
            s.parts.push(PartDef {
                name: fresh(&name, &taken),
                ..PartDef::default()
            });
        });
    }

    pub fn rename_part(&self, player: &dyn PresetPlayer, old: &str, new_name: &str) {
        let (old, new_name) = (old.to_string(), new_name.to_string());
        self.edit_current(player, "rename_part", move |s| {
            part_op(s, "rename", &old, &new_name)
        });
    }

    pub fn remove_part(&self, player: &dyn PresetPlayer, name: &str) {
        let name = name.to_string();
        self.edit_current(player, "remove_part", move |s| {
            part_op(s, "remove", &name, "")
        });
    }

    pub fn move_part(&self, player: &dyn PresetPlayer, from: u32, to: u32) {
        self.edit_current(player, "move_part", move |s| {
            move_in(&mut s.parts, from as usize, to as usize);
        });
    }

    pub fn set_part_patch(&self, player: &dyn PresetPlayer, part: &str, patch: &str) {
        let (part, patch) = (part.to_string(), patch.to_string());
        self.edit_current(player, "set_part_patch", move |s| {
            part_op(s, "patch", &part, &patch)
        });
    }

    pub fn set_part_section(&self, player: &dyn PresetPlayer, part: &str, section: &str) {
        let (part, section) = (part.to_string(), section.to_string());
        self.edit_current(player, "set_part_section", move |s| {
            part_op(s, "section", &part, &section)
        });
    }

    pub fn set_part_profile(&self, player: &dyn PresetPlayer, part: &str, profile: &str) {
        let (part, profile) = (part.to_string(), profile.to_string());
        self.edit_current(player, "set_part_profile", move |s| {
            if let Some(p) = s.parts.iter_mut().find(|p| p.name == part) {
                p.profile = profile;
            }
        });
    }

    /// A part edit on any song: `add`, `rename`, `remove`, `move`,
    /// `section` or `patch`.
    pub fn edit_song_part(
        &self,
        player: &dyn PresetPlayer,
        song: &str,
        op: &str,
        part: &str,
        value: &str,
    ) {
        if !self.lib().songs.iter().any(|s| s.name == song) {
            return Self::refuse("edit_song_part", "no such song");
        }
        self.edit(player, |l| {
            if let Some(s) = l.song_mut(song) {
                part_op(s, op, part, value);
            }
        });
    }

    pub fn add_song(&self, player: &dyn PresetPlayer, name: &str, key: &str, bpm: u32) {
        let taken: Vec<String> = self.lib().songs.iter().map(|s| s.name.clone()).collect();
        let song = SongDef {
            name: fresh(name, &taken),
            key: key.to_string(),
            bpm,
            ..SongDef::default()
        };
        self.edit(player, |l| l.songs.push(song));
    }

    pub fn set_song_profile(&self, player: &dyn PresetPlayer, song: &str, profile: &str) {
        self.edit(player, |l| {
            if let Some(s) = l.song_mut(song) {
                s.profile = profile.to_string();
            }
        });
    }

    pub fn set_song_colour(&self, player: &dyn PresetPlayer, song: &str, colour: &str) {
        self.edit(player, |l| {
            if let Some(s) = l.song_mut(song) {
                s.colour = colour.to_string();
            }
        });
    }

    // ── Setlists ──

    pub fn select_setlist(&self, player: &dyn PresetPlayer, index: u32) -> Applied {
        if index as usize >= self.lib().setlists.len() {
            return Applied::refused(format!("no setlist {index}"));
        }
        self.edit(player, |l| {
            l.cursor.setlist = index;
            l.cursor.song = 0;
            l.cursor.part = 0;
        });
        if self.lib().setlist().is_some_and(|s| !s.entries.is_empty()) {
            self.select_song(player, 0)
        } else {
            Applied::done()
        }
    }

    pub fn add_setlist(&self, player: &dyn PresetPlayer, name: &str) {
        let taken: Vec<String> = self.lib().setlists.iter().map(|s| s.name.clone()).collect();
        let set = SetlistDef {
            name: fresh(name, &taken),
            ..SetlistDef::default()
        };
        self.edit(player, |l| l.setlists.push(set));
    }

    pub fn rename_setlist(&self, player: &dyn PresetPlayer, index: u32, new_name: &str) {
        self.edit(player, |l| {
            if let Some(s) = l.setlists.get_mut(index as usize) {
                s.name = new_name.to_string();
            }
        });
    }

    pub fn duplicate_setlist(&self, player: &dyn PresetPlayer, index: u32, new_name: &str) {
        let Some(mut copy) = self.lib().setlists.get(index as usize).cloned() else {
            return Self::refuse("duplicate_setlist", "no such setlist");
        };
        let taken: Vec<String> = self.lib().setlists.iter().map(|s| s.name.clone()).collect();
        copy.name = fresh(new_name, &taken);
        self.edit(player, |l| l.setlists.push(copy));
    }

    pub fn set_setlist_details(
        &self,
        player: &dyn PresetPlayer,
        index: u32,
        event: &str,
        date: &str,
        title: &str,
    ) {
        self.edit(player, |l| {
            if let Some(s) = l.setlists.get_mut(index as usize) {
                s.event = event.to_string();
                s.date = date.to_string();
                s.title = title.to_string();
            }
        });
    }

    pub fn set_setlist_profile(&self, player: &dyn PresetPlayer, index: u32, profile: &str) {
        self.edit(player, |l| {
            if let Some(s) = l.setlists.get_mut(index as usize) {
                s.profile = profile.to_string();
            }
        });
    }

    pub fn add_setlist_entry(&self, player: &dyn PresetPlayer, setlist: u32, song: &str) {
        self.edit(player, |l| {
            if let Some(s) = l.setlists.get_mut(setlist as usize) {
                s.entries.push(EntryDef {
                    song: song.to_string(),
                    ..EntryDef::default()
                });
            }
        });
    }

    pub fn set_setlist_entry(&self, player: &dyn PresetPlayer, entry: u32, key: &str, bpm: u32) {
        self.edit(player, |l| {
            let i = l.cursor.setlist as usize;
            if let Some(e) = l
                .setlists
                .get_mut(i)
                .and_then(|s| s.entries.get_mut(entry as usize))
            {
                e.key = key.to_string();
                e.bpm = bpm;
            }
        });
    }

    pub fn set_setlist_entry_start(&self, player: &dyn PresetPlayer, entry: u32, patch: &str) {
        self.edit(player, |l| {
            let i = l.cursor.setlist as usize;
            if let Some(e) = l
                .setlists
                .get_mut(i)
                .and_then(|s| s.entries.get_mut(entry as usize))
            {
                e.start = patch.to_string();
            }
        });
    }

    pub fn move_setlist_entry(&self, player: &dyn PresetPlayer, setlist: u32, from: u32, to: u32) {
        self.edit(player, |l| {
            if let Some(s) = l.setlists.get_mut(setlist as usize) {
                move_in(&mut s.entries, from as usize, to as usize);
            }
        });
    }

    pub fn remove_setlist_entry(&self, player: &dyn PresetPlayer, setlist: u32, entry: u32) {
        self.edit(player, |l| {
            if let Some(s) = l.setlists.get_mut(setlist as usize)
                && (entry as usize) < s.entries.len()
            {
                s.entries.remove(entry as usize);
            }
        });
    }

    pub fn delete_setlist(&self, player: &dyn PresetPlayer, index: u32) {
        if self.lib().setlists.len() <= 1 {
            return Self::refuse("delete_setlist", "the last setlist");
        }
        self.edit(player, |l| {
            if (index as usize) < l.setlists.len() {
                l.setlists.remove(index as usize);
            }
            l.cursor.setlist = l
                .cursor
                .setlist
                .min(l.setlists.len().saturating_sub(1) as u32);
        });
    }
}

/// After a preset edit: a refusal is said once; a change reaches every
/// remote. (For [`impl_rig_perform!`].)
#[doc(hidden)]
pub fn preset_edit(store: &PerformStore, player: &dyn PresetPlayer, op: &str, a: Applied) {
    if a.ok {
        store.changed(player);
    } else {
        tracing::warn!(op, reason = a.message, "perform store: preset edit refused");
    }
}

/// One part edit on a song.
fn part_op(s: &mut SongDef, op: &str, part: &str, value: &str) {
    match op {
        "add" => {
            let taken: Vec<String> = s.parts.iter().map(|p| p.name.clone()).collect();
            s.parts.push(PartDef {
                name: fresh(value, &taken),
                ..PartDef::default()
            });
        }
        "rename" => {
            if !value.trim().is_empty() && !s.parts.iter().any(|p| p.name == value) {
                for p in s.parts.iter_mut().filter(|p| p.name == part) {
                    p.name = value.to_string();
                }
            }
        }
        "remove" => s.parts.retain(|p| p.name != part),
        "move" => {
            if let (Ok(from), Ok(to)) = (part.parse::<usize>(), value.parse::<usize>()) {
                move_in(&mut s.parts, from, to);
            }
        }
        "section" => {
            for p in s.parts.iter_mut().filter(|p| p.name == part) {
                p.section = value.to_string();
            }
        }
        "patch" => {
            for p in s.parts.iter_mut().filter(|p| p.name == part) {
                p.patch = value.to_string();
            }
        }
        _ => tracing::warn!(op, "perform store: no such part edit"),
    }
}

/// A stored part as the screens show it.
fn perf_part(p: &PartDef) -> PerfPart {
    PerfPart {
        name: p.name.clone(),
        patch: p.patch.clone(),
        section: p.section.clone(),
        profile: p.profile.clone(),
        ..PerfPart::default()
    }
}

/// Write a rig's `RigPerform` and its stream source from its
/// [`PerformStore`] and [`PresetPlayer`]: `$store` is an expression on
/// `self` giving the store.
///
/// ```ignore
/// signal_rig_perform_store::impl_rig_perform!(DrumRigBackend, |b| &b.inner.perform);
/// ```
#[macro_export]
macro_rules! impl_rig_perform {
    ($rig:ty, |$s:ident| $store:expr) => {
        impl ::signal_rigs_proto::perform::service::RigPerform for $rig {
            fn state(&self) -> ::signal_rigs_proto::perform::PerformState {
                let $s = self;
                $store.state(self)
            }
            fn set_perform_mode(&self, mode: u32) {
                let $s = self;
                $store.set_mode(self, mode)
            }
            fn patches(&self) -> Vec<::signal_rigs_proto::perform::PatchInfo> {
                let $s = self;
                $store.patches()
            }
            fn select_patch(&self, index: u32) -> ::signal_rigs_proto::perform::Applied {
                let $s = self;
                $store.select_patch(self, index)
            }
            fn compositions(&self) -> ::signal_rigs_proto::perform::CompositionModel {
                $crate::PresetPlayer::compositions(self)
            }
            fn choose_preset(&self, preset: String, variation: String) {
                let $s = self;
                let a = $store.choose_preset(self, &preset, &variation);
                if !a.ok {
                    ::tracing::warn!(preset, variation, reason = a.message, "preset refused");
                }
            }
            fn save_variation(&self, preset: String, variation: String) {
                let $s = self;
                $crate::preset_edit(
                    $store,
                    self,
                    "save_variation",
                    $crate::PresetPlayer::save_variation(self, &preset, &variation),
                );
            }
            fn rename_preset(&self, old: String, new_name: String) {
                let $s = self;
                let a = $crate::PresetPlayer::rename_preset(self, &old, &new_name);
                if a.ok {
                    $store.preset_renamed(self, &old, &new_name);
                }
                $crate::preset_edit($store, self, "rename_preset", a);
            }
            fn duplicate_preset(&self, name: String, new_name: String) {
                let $s = self;
                $crate::preset_edit(
                    $store,
                    self,
                    "duplicate_preset",
                    $crate::PresetPlayer::duplicate_preset(self, &name, &new_name),
                );
            }
            fn delete_preset(&self, name: String) {
                let $s = self;
                $crate::preset_edit(
                    $store,
                    self,
                    "delete_preset",
                    $crate::PresetPlayer::delete_preset(self, &name),
                );
            }
            fn library(&self) -> ::signal_rigs_proto::perform::LibraryModel {
                let $s = self;
                $store.library()
            }
            fn select_profile(&self, name: String) -> ::signal_rigs_proto::perform::Applied {
                let $s = self;
                $store.select_profile(self, &name)
            }
            fn add_profile(&self, name: String, from: String) {
                let $s = self;
                $store.add_profile(self, &name, &from)
            }
            fn rename_profile(&self, old: String, new_name: String) {
                let $s = self;
                $store.rename_profile(self, &old, &new_name)
            }
            fn delete_profile(&self, name: String) {
                let $s = self;
                $store.delete_profile(self, &name)
            }
            fn press_stack(&self, index: u32) {
                let $s = self;
                $store.press_stack(self, index)
            }
            fn play_stack_patch(
                &self,
                index: u32,
                position: u32,
            ) -> ::signal_rigs_proto::perform::Applied {
                let $s = self;
                $store.play_stack_patch(self, index, position)
            }
            fn set_stack_patch(&self, stack: String, patch: String, on: bool) {
                let $s = self;
                $store.set_stack_patch(self, &stack, &patch, on)
            }
            fn move_stack_patch(&self, stack: String, from: u32, to: u32) {
                let $s = self;
                $store.move_stack_patch(self, &stack, from, to)
            }
            fn move_stack(&self, from: u32, to: u32) {
                let $s = self;
                $store.move_stack(self, from, to)
            }
            fn rename_stack(&self, old: String, new_name: String) {
                let $s = self;
                $store.rename_stack(self, &old, &new_name)
            }
            fn rename_patch(&self, old: String, new_name: String) {
                let $s = self;
                $store.rename_patch(self, &old, &new_name)
            }
            fn select_song(&self, index: u32) -> ::signal_rigs_proto::perform::Applied {
                let $s = self;
                $store.select_song(self, index)
            }
            fn select_part(&self, index: u32) -> ::signal_rigs_proto::perform::Applied {
                let $s = self;
                $store.select_part(self, index)
            }
            fn add_part(&self, name: String) {
                let $s = self;
                $store.add_part(self, &name)
            }
            fn rename_part(&self, old: String, new_name: String) {
                let $s = self;
                $store.rename_part(self, &old, &new_name)
            }
            fn remove_part(&self, name: String) {
                let $s = self;
                $store.remove_part(self, &name)
            }
            fn move_part(&self, from: u32, to: u32) {
                let $s = self;
                $store.move_part(self, from, to)
            }
            fn set_part_patch(&self, part: String, patch: String) {
                let $s = self;
                $store.set_part_patch(self, &part, &patch)
            }
            fn set_part_section(&self, part: String, section: String) {
                let $s = self;
                $store.set_part_section(self, &part, &section)
            }
            fn set_part_profile(&self, part: String, profile: String) {
                let $s = self;
                $store.set_part_profile(self, &part, &profile)
            }
            fn edit_song_part(&self, song: String, op: String, part: String, value: String) {
                let $s = self;
                $store.edit_song_part(self, &song, &op, &part, &value)
            }
            fn add_song(&self, name: String, key: String, bpm: u32) {
                let $s = self;
                $store.add_song(self, &name, &key, bpm)
            }
            fn set_song_profile(&self, song: String, profile: String) {
                let $s = self;
                $store.set_song_profile(self, &song, &profile)
            }
            fn set_song_colour(&self, song: String, colour: String) {
                let $s = self;
                $store.set_song_colour(self, &song, &colour)
            }
            fn select_setlist(&self, index: u32) -> ::signal_rigs_proto::perform::Applied {
                let $s = self;
                $store.select_setlist(self, index)
            }
            fn add_setlist(&self, name: String) {
                let $s = self;
                $store.add_setlist(self, &name)
            }
            fn rename_setlist(&self, index: u32, new_name: String) {
                let $s = self;
                $store.rename_setlist(self, index, &new_name)
            }
            fn duplicate_setlist(&self, index: u32, new_name: String) {
                let $s = self;
                $store.duplicate_setlist(self, index, &new_name)
            }
            fn set_setlist_details(&self, index: u32, event: String, date: String, title: String) {
                let $s = self;
                $store.set_setlist_details(self, index, &event, &date, &title)
            }
            fn set_setlist_profile(&self, index: u32, profile: String) {
                let $s = self;
                $store.set_setlist_profile(self, index, &profile)
            }
            fn add_setlist_entry(&self, setlist: u32, song: String) {
                let $s = self;
                $store.add_setlist_entry(self, setlist, &song)
            }
            fn set_setlist_entry(&self, entry: u32, key: String, bpm: u32) {
                let $s = self;
                $store.set_setlist_entry(self, entry, &key, bpm)
            }
            fn set_setlist_entry_start(&self, entry: u32, patch: String) {
                let $s = self;
                $store.set_setlist_entry_start(self, entry, &patch)
            }
            fn move_setlist_entry(&self, setlist: u32, from: u32, to: u32) {
                let $s = self;
                $store.move_setlist_entry(self, setlist, from, to)
            }
            fn remove_setlist_entry(&self, setlist: u32, entry: u32) {
                let $s = self;
                $store.remove_setlist_entry(self, setlist, entry)
            }
            fn delete_setlist(&self, index: u32) {
                let $s = self;
                $store.delete_setlist(self, index)
            }
        }

        impl ::signal_rigs_proto::perform::service::RigPerformStreamSource for $rig {
            fn events_hub(
                &self,
            ) -> &::architect::PubSub<::signal_rigs_proto::perform::PerformEvent> {
                let $s = self;
                $store.hub()
            }
        }
    };
}
