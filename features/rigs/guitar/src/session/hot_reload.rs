//! Hot reload: a library file edited while the rig plays is applied to the
//! running rig, file by file, with as little disturbance as it allows.
//!
//! The meter pump polls the rig directory ([`GuitarRigBackend::poll_config`],
//! via [`crate::config_watch::Watcher`]); a change that has held still is
//! applied on its own thread, so a chain build never holds up the pump and
//! the footswitches it drains. Chains change only through the gapless reload,
//! which keeps the playing patch, every stack cursor and the song's tuning.
//!
//! | file | applied as |
//! |---|---|
//! | `profiles/<playing>.styx` | the profile re-read, its patches rebuilt (unchanged chains reused) |
//! | `profiles/<other>.styx` | the profile list refreshed |
//! | `songs.styx` | songs replaced, song patches re-attached, position kept by name, rebuilt |
//! | `setlists.styx` | setlists replaced, position kept by name, the song's tuning re-applied |
//! | `drive-presets.styx`, `modules.styx`, `presets.styx`, `blocks.styx` | the patches rebuilt |
//! | `midi.styx`, `keymap.styx` | the map swapped |
//! | `last-state.styx` | ignored — the rig owns it |
//!
//! A file that does not parse changes nothing: the rig plays on as it was,
//! the error goes to the log, and the file is never saved over.
//!
//! Every reload writes one line to the reload log
//! ([`crate::config_watch::log_path`]).

use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use signal_rig_host::lock::LockExt;
use signal_sampler::rig_profile::ReloadMode;

use super::GuitarRigBackend;
use crate::config_watch::{self, Change, Read, Watcher};
use crate::library::{
    DrivePresetLib, KeymapLib, LastState, SetlistLib, SongLib, attach_song_patches, profile_file,
    resolve_drive_presets, resolve_profile, rig_dir, rig_is_ephemeral,
};
use crate::nodes::profile_from_library;
use crate::profiles::{MidiMapDef, ProfileDef};

/// The watcher and the one-reload-at-a-time guard.
#[derive(Default)]
pub struct HotReload {
    watcher: Mutex<Watcher>,
    busy: AtomicBool,
}

/// Clears the busy flag however the reload ends.
struct Busy<'a>(&'a AtomicBool);

impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// What a gapless reload did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReloadCounts {
    pub built: usize,
    pub reused: usize,
    pub failed: usize,
}

impl ReloadCounts {
    fn describe(counts: Option<Self>) -> String {
        match counts {
            None => "no engine playing — the definitions were swapped".to_string(),
            Some(c) => {
                let mut s = format!(
                    "{} patch{} rebuilt, {} reused",
                    c.built,
                    if c.built == 1 { "" } else { "es" },
                    c.reused
                );
                if c.failed > 0 {
                    s.push_str(&format!(", {} failed to build", c.failed));
                }
                s
            }
        }
    }
}

/// Which library file a path is.
enum Kind {
    Profile,
    Songs,
    Setlists,
    DrivePresets,
    Composition,
    Midi,
    Keymap,
    LastState,
    Nodes,
    Other,
}

fn kind(path: &Path) -> Kind {
    let in_profiles = path
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|d| d == crate::library::PROFILES_DIR);
    if in_profiles {
        return Kind::Profile;
    }
    match path.file_name().and_then(|f| f.to_str()).unwrap_or_default() {
        "songs.styx" => Kind::Songs,
        "setlists.styx" => Kind::Setlists,
        "drive-presets.styx" => Kind::DrivePresets,
        crate::compose::MODULES_FILE | crate::compose::PRESETS_FILE | crate::compose::BLOCKS_FILE => {
            Kind::Composition
        }
        "midi.styx" => Kind::Midi,
        "keymap.styx" => Kind::Keymap,
        "last-state.styx" => Kind::LastState,
        crate::node_store::NODE_STORE_FILE => Kind::Nodes,
        _ => Kind::Other,
    }
}

/// Where the player is, by name — what a reload of the song library must
/// land back on.
struct Position {
    setlist: Option<String>,
    song: Option<String>,
    part: Option<String>,
}

impl GuitarRigBackend {
    /// One look at the rig directory (the meter pump, twice a second). A
    /// change is applied on its own thread; while one is being applied the
    /// next look waits its turn.
    pub(super) fn poll_config(&self) {
        if self.hot.busy.swap(true, Ordering::Acquire) {
            return;
        }
        let busy = Busy(&self.hot.busy);
        let dir = rig_dir();
        // `signal rig reload`: only the rig that saves answers it — a design
        // instance on the same directory must not take the request.
        let request_path = dir.join(config_watch::RELOAD_REQUEST);
        let request = if rig_is_ephemeral() {
            None
        } else {
            std::fs::read_to_string(&request_path).ok()
        };
        let changes = {
            let mut w = self.hot.watcher.lock_ok();
            if request.is_some() { w.force(&dir) } else { w.poll(&dir) }
        };
        if changes.is_empty() && request.is_none() {
            return;
        }
        std::mem::forget(busy);
        let this = self.clone();
        let spawned = std::thread::Builder::new()
            .name("rig-config-reload".into())
            .spawn(move || {
                let _busy = Busy(&this.hot.busy);
                let tag = request.as_deref().map(str::trim).filter(|t| !t.is_empty());
                this.apply_changes(changes, tag);
                if request.is_some() {
                    let _ = std::fs::remove_file(&request_path);
                }
            });
        if spawned.is_err() {
            self.hot.busy.store(false, Ordering::Release);
        }
    }

    /// Re-read every library file that differs from what the rig holds and
    /// apply it now — the explicit reload (`reload_config`,
    /// `reload_library`). One line per file applied.
    pub(super) fn reload_config_now(&self) -> String {
        // Wait out a reload already running (bounded: a reload is a chain
        // build, well under this).
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while self.hot.busy.swap(true, Ordering::Acquire) {
            if std::time::Instant::now() > deadline {
                return "a reload is still running — try again".to_string();
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let _busy = Busy(&self.hot.busy);
        let changes = self.hot.watcher.lock_ok().force(&rig_dir());
        let lines = self.apply_changes(changes, None);
        if lines.is_empty() {
            "nothing to reload: every config file is as the rig has it".to_string()
        } else {
            lines.join("\n")
        }
    }

    /// Apply each change, logging one line per file that did something.
    fn apply_changes(&self, changes: Vec<Change>, tag: Option<&str>) -> Vec<String> {
        let mut lines = Vec::new();
        for c in changes {
            if let Some(line) = self.apply_change(&c) {
                let line = format!("{}: {line}", config_watch::display_name(&c.path));
                tracing::info!(config.file = %config_watch::display_name(&c.path), "hot reload — {line}");
                config_watch::log_line(&match tag {
                    Some(t) => format!("[{t}] {line}"),
                    None => line.clone(),
                });
                lines.push(line);
            }
        }
        if let Some(t) = tag {
            let done = if lines.is_empty() {
                "reload requested: nothing had changed".to_string()
            } else {
                format!("reload requested: {} file(s) applied", lines.len())
            };
            config_watch::log_line(&format!("[{t}] {done}"));
        }
        lines
    }

    /// Apply one changed file. `None` when there is nothing worth a line.
    fn apply_change(&self, c: &Change) -> Option<String> {
        let path = c.path.as_path();
        if c.deleted {
            config_watch::forget(path);
        }
        match kind(path) {
            Kind::Profile => Some(self.apply_profile(path, c.deleted)),
            Kind::LastState => {
                // The rig's own: remembered, so it is not news again.
                let _ = config_watch::read_tracked::<LastState>(path);
                None
            }
            Kind::Nodes => {
                // Read per use — the next use sees it. Remembered, so the
                // save after that use is not refused.
                let _ = config_watch::read_tracked::<crate::node_store::NodeStore>(path);
                None
            }
            Kind::Other => None,
            _ if c.deleted => Some(
                "deleted — the rig plays on with what it loaded; its next save writes the file back"
                    .to_string(),
            ),
            Kind::Songs => Some(self.apply_songs(path)),
            Kind::Setlists => Some(self.apply_setlists(path)),
            Kind::DrivePresets => Some(self.apply_drive_presets(path)),
            Kind::Composition => Some(self.apply_composition(path)),
            Kind::Midi => Some(match config_watch::read_tracked::<MidiMapDef>(path) {
                Read::Ok(m) => {
                    *self.midi_map.lock_ok() = m;
                    "footswitch map swapped".to_string()
                }
                other => bad_line(path, other),
            }),
            Kind::Keymap => Some(match config_watch::read_tracked::<KeymapLib>(path) {
                Read::Ok(k) => {
                    let n = k.bindings.len();
                    *self.keymap.lock_ok() = k.bindings;
                    self.publish_state();
                    format!("keymap swapped ({n} bindings)")
                }
                other => bad_line(path, other),
            }),
        }
    }

    fn apply_profile(&self, path: &Path, deleted: bool) -> String {
        let file = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
        let playing = profile_file(&self.profile_def.lock_ok().name) == file;
        if deleted {
            if playing {
                return "the playing profile's file was deleted — still playing it; its next save writes it back".to_string();
            }
            self.other_profiles.lock_ok().retain(|p| profile_file(&p.name) != file);
            self.publish_state();
            return "deleted — removed from the profile list".to_string();
        }
        let mut def = match config_watch::read_tracked::<ProfileDef>(path) {
            Read::Ok(p) => p,
            other => return bad_line(path, other),
        };
        resolve_profile(&mut def);
        attach_song_patches(&mut def, &self.songs_lib.lock_ok());
        let name = def.name.clone();
        if playing {
            *self.profile_def.lock_ok() = def;
            // A live edit not yet saved is older than the file: the file won.
            let dropped = self.library_dirty.swap(false, Ordering::Relaxed);
            let counts = self.rebuild_live();
            format!(
                "profile {name}: {}{}",
                ReloadCounts::describe(counts),
                if dropped { " (an unsaved live edit gave way to the file)" } else { "" }
            )
        } else {
            {
                let mut others = self.other_profiles.lock_ok();
                others.retain(|p| {
                    profile_file(&p.name) != file && !p.name.eq_ignore_ascii_case(&name)
                });
                others.push(def);
                others.sort_by_key(|p| p.name.to_lowercase());
            }
            self.publish_state();
            format!("profile {name} (not playing): the profile list refreshed")
        }
    }

    fn apply_songs(&self, path: &Path) -> String {
        let songs = match config_watch::read_tracked::<SongLib>(path) {
            Read::Ok(l) => l.songs,
            other => return bad_line(path, other),
        };
        let at = self.position();
        let n = songs.len();
        *self.songs_lib.lock_ok() = songs.clone();
        // Song patches live in songs.styx and ride on the profiles they
        // play on: take the old ones off, put the new ones on.
        {
            let mut def = self.profile_def.lock_ok();
            def.patches.retain(|p| p.song.is_empty());
            attach_song_patches(&mut def, &songs);
        }
        for other in self.other_profiles.lock_ok().iter_mut() {
            other.patches.retain(|p| p.song.is_empty());
            attach_song_patches(other, &songs);
        }
        let now = self.land_on(&at);
        let counts = self.rebuild_live();
        format!("{n} songs{now} — {}", ReloadCounts::describe(counts))
    }

    fn apply_setlists(&self, path: &Path) -> String {
        let sets = match config_watch::read_tracked::<SetlistLib>(path) {
            Read::Ok(l) => l.setlists,
            other => return bad_line(path, other),
        };
        let at = self.position();
        let n = sets.len();
        *self.setlists.lock_ok() = sets;
        let now = self.land_on(&at);
        self.apply_song_stacks();
        self.publish_state();
        format!("{n} setlists{now}")
    }

    fn apply_drive_presets(&self, path: &Path) -> String {
        let mut presets = match config_watch::read_tracked::<DrivePresetLib>(path) {
            Read::Ok(l) => l.presets,
            other => return bad_line(path, other),
        };
        resolve_drive_presets(&mut presets);
        *self.drive_presets.lock_ok() = presets;
        let counts = self.rebuild_live();
        self.spawn_drive_calibration();
        format!("drive presets: {}", ReloadCounts::describe(counts))
    }

    fn apply_composition(&self, path: &Path) -> String {
        // Checked here so a bad file changes nothing; the rebuild then reads
        // it through `load_compositions`, whose cache has seen it move.
        let ok = match kind_file(path) {
            crate::compose::MODULES_FILE => {
                parse_check::<crate::compose::ModuleLib>(path)
            }
            crate::compose::PRESETS_FILE => parse_check::<crate::compose::PresetLib>(path),
            _ => parse_check::<crate::compose::BlockLib>(path),
        };
        if let Err(line) = ok {
            return line;
        }
        let counts = self.rebuild_live();
        format!("compositions: {}", ReloadCounts::describe(counts))
    }

    /// Rebuild the live chains from the definitions, gaplessly, and put the
    /// song's tuning, tempo, boost and drives back on them.
    fn rebuild_live(&self) -> Option<ReloadCounts> {
        let rebuilt = {
            let def = self.profile_def.lock_ok();
            let dps = self.drive_presets.lock_ok();
            profile_from_library(&def, &dps)
        };
        self.reload_rebuilt_counted(rebuilt, ReloadMode::Keep)
    }

    fn position(&self) -> Position {
        let setlist = self
            .setlists
            .lock_ok()
            .get(*self.setlist_index.lock_ok())
            .map(|s| s.name.clone());
        let resolved = self.resolved_setlist();
        let song_at = *self.song_index.lock_ok();
        let song = resolved.get(song_at).map(|(name, ..)| name.clone());
        let part = song.as_ref().and_then(|s| {
            self.songs_lib
                .lock_ok()
                .iter()
                .find(|d| d.name.eq_ignore_ascii_case(s))
                .and_then(|d| d.parts.get(*self.part_index.lock_ok()).cloned())
        });
        Position { setlist, song, part }
    }

    /// Put the player back on `at` by name in the reloaded library — the
    /// same setlist, song and part, wherever they now sit. Something no
    /// longer there keeps its index, clamped. Returns a note for the log.
    fn land_on(&self, at: &Position) -> String {
        {
            let sets = self.setlists.lock_ok();
            let mut i = self.setlist_index.lock_ok();
            if let Some(n) = at
                .setlist
                .as_ref()
                .and_then(|name| sets.iter().position(|s| s.name.eq_ignore_ascii_case(name)))
            {
                *i = n;
            } else {
                *i = (*i).min(sets.len().saturating_sub(1));
            }
        }
        let resolved = self.resolved_setlist();
        {
            let mut i = self.song_index.lock_ok();
            if let Some(n) = at
                .song
                .as_ref()
                .and_then(|name| resolved.iter().position(|(s, ..)| s.eq_ignore_ascii_case(name)))
            {
                *i = n;
            } else {
                *i = (*i).min(resolved.len().saturating_sub(1));
            }
        }
        let song = resolved.get(*self.song_index.lock_ok()).map(|(n, ..)| n.clone());
        let parts: Vec<String> = song
            .as_ref()
            .and_then(|s| {
                self.songs_lib
                    .lock_ok()
                    .iter()
                    .find(|d| d.name.eq_ignore_ascii_case(s))
                    .map(|d| d.parts.clone())
            })
            .unwrap_or_default();
        let part = {
            let mut i = self.part_index.lock_ok();
            if let Some(n) = at
                .part
                .as_ref()
                .and_then(|name| parts.iter().position(|p| p.eq_ignore_ascii_case(name)))
            {
                *i = n;
            } else {
                *i = (*i).min(parts.len().saturating_sub(1));
            }
            parts.get(*i).cloned()
        };
        match (song, part) {
            (Some(s), Some(p)) => format!("; on {s} / {p}"),
            (Some(s), None) => format!("; on {s}"),
            _ => String::new(),
        }
    }
}

fn kind_file(path: &Path) -> &str {
    path.file_name().and_then(|f| f.to_str()).unwrap_or_default()
}

/// Whether `path` parses as `T` (remembering it either way).
fn parse_check<T: for<'a> facet::Facet<'a>>(path: &Path) -> Result<(), String> {
    match config_watch::read_tracked::<T>(path) {
        Read::Ok(_) => Ok(()),
        other => Err(bad_line(path, other)),
    }
}

/// The line for a file that could not be applied.
fn bad_line<T>(path: &Path, read: Read<T>) -> String {
    match read {
        Read::Bad(e) => {
            tracing::error!(config.file = %config_watch::display_name(path), "hot reload refused — does not parse: {e}");
            format!("does not parse — nothing reloaded, the running state kept: {e}")
        }
        Read::Missing => "gone before it could be read — nothing reloaded".to_string(),
        Read::Ok(_) => String::new(),
    }
}
