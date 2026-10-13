//! Where the material lives, and what to do when it does not.
//!
//! Nothing here is committed. The inventory (#159,
//! `../spec/research/material-inventory.md`) surveyed a machine and
//! recorded absolute paths; this module turns those paths into
//! something a test can ask questions of, and — crucially — into an
//! honest `None` on any machine that does not have them.
//!
//! **A missing corpus must skip, never fail.** CI has none of this, and
//! a scenario that goes red on the build machine for want of 14 GB of
//! worship stems teaches nobody anything. What it must never do is
//! silently *pass*: [`Material::discover`] returning `None` is a
//! skip that says so out loud.

use std::path::{Path, PathBuf};

/// Override for the material root. Set it when the material lives
/// somewhere other than `~/Downloads`.
pub const ROOT_ENV: &str = "FTS_DEMO_MATERIAL";

/// One song in the PNG Worship Collective sessions — the set the
/// inventory recommends building on, because the takes are real, the
/// vocals have prior Melodyne passes, and the MIDI, charts and audio
/// all line up song for song.
#[derive(Debug, Clone)]
pub struct Song {
    /// `09 HANNAH'S SONG`.
    pub name: String,
    /// The session folder.
    pub dir: PathBuf,
    /// The MIDI export sitting beside the folder, when there is one.
    pub midi: Option<PathBuf>,
}

/// Material resolved on this machine.
#[derive(Debug, Clone)]
pub struct Material {
    root: PathBuf,
}

impl Material {
    /// Resolve the material root, or `None` if this machine has no
    /// material.
    ///
    /// `$FTS_DEMO_MATERIAL` wins; otherwise `~/Downloads`, which is
    /// where the inventory found everything.
    pub fn discover() -> Option<Self> {
        let root = match std::env::var_os(ROOT_ENV) {
            Some(v) => PathBuf::from(v),
            None => PathBuf::from(std::env::var_os("HOME")?).join("Downloads"),
        };
        // The root existing is not enough — every home directory has a
        // Downloads. Require something we actually named.
        let m = Self { root };
        m.sessions_dir().is_dir().then_some(m)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The PNG Worship Collective session files.
    pub fn sessions_dir(&self) -> PathBuf {
        self.root.join("PNG WORSHIP COLLECTIVE SESSION FILES")
    }

    /// The delivered worship multitracks — the wide-track-count stress
    /// case, and where Click and Guide tracks come from.
    pub fn multitracks_dir(&self) -> PathBuf {
        self.root.join("Worship MultiTracks")
    }

    /// Every session song, in disk order.
    ///
    /// A song is a directory whose name starts with a two-digit number;
    /// its MIDI export is the sibling `<name> MIDI*.mid`, which is not a
    /// stable enough pattern to construct, so it is matched.
    pub fn songs(&self) -> Vec<Song> {
        let dir = self.sessions_dir();
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut files: Vec<PathBuf> = Vec::new();
        let mut dirs: Vec<PathBuf> = Vec::new();
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                dirs.push(p);
            } else {
                files.push(p);
            }
        }
        dirs.sort();
        files.sort();

        dirs.into_iter()
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.chars().take(2).all(|c| c.is_ascii_digit()))
            })
            .map(|dir| {
                let name = dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default()
                    .to_string();
                let midi = files
                    .iter()
                    .find(|f| {
                        f.extension().is_some_and(|e| e.eq_ignore_ascii_case("mid"))
                            && f.file_name()
                                .and_then(|n| n.to_str())
                                .is_some_and(|n| n.starts_with(&name))
                    })
                    .cloned();
                Song { name, dir, midi }
            })
            .collect()
    }

    /// The Guitar Pro transcription scenario 4 imports: GP 7.6, 162
    /// bars, 568 notes, 40 bends, 71 slides, 143 vibrato markings.
    ///
    /// Found by search rather than by a fixed path — the download
    /// carries a timestamped folder name.
    pub fn guitar_pro(&self) -> Option<PathBuf> {
        let entries = std::fs::read_dir(&self.root).ok()?;
        let mut hits: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .filter_map(|d| {
                std::fs::read_dir(&d).ok().map(|inner| {
                    inner
                        .flatten()
                        .map(|e| e.path())
                        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("gp")))
                        .collect::<Vec<_>>()
                })
            })
            .flatten()
            .collect();
        hits.sort();
        hits.into_iter().next()
    }

    /// The close-miked drum multitrack — the bleed scenario 2 needs and
    /// no synthesized fixture has.
    pub fn drum_multitrack(&self) -> Option<PathBuf> {
        let d = self.root.join("RomanStyx_SevenFeel");
        d.is_dir().then_some(d)
    }

    /// Every wav directly under a song's session folder, sorted.
    ///
    /// Sessions nest audio in different places between songs, so this
    /// walks rather than assuming a layout, and stops at a depth that
    /// keeps it quick on a 3.7 GB folder.
    pub fn stems(&self, song: &Song) -> Vec<PathBuf> {
        let mut out = Vec::new();
        collect_wavs(&song.dir, 3, &mut out);
        out.sort();
        out
    }
}

fn collect_wavs(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_wavs(&p, depth - 1, out);
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("wav")) {
            out.push(p);
        }
    }
}

/// Skip the calling test when this machine has no material.
///
/// Returns `None` after printing why, so the test body can `return`.
/// A silent skip is worse than a failure; this one is loud.
#[macro_export]
macro_rules! material_or_skip {
    () => {
        match $crate::Material::discover() {
            Some(m) => m,
            None => {
                eprintln!(
                    "SKIP: no demo material on this machine. \
                     Set ${} to the directory holding \
                     'PNG WORSHIP COLLECTIVE SESSION FILES'.",
                    $crate::material::ROOT_ENV
                );
                return;
            }
        }
    };
}
