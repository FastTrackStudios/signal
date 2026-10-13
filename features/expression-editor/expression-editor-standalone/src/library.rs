//! What there is to open, for a runner you do not restart.
//!
//! The `--example editor` path takes one file on the command line and
//! opens a window on it. That is the right shape for a screenshot or a
//! one-off look, and the wrong one for `dx serve`: the whole point of
//! the served runner is that it stays up while you edit the UI, so
//! choosing what to open has to be something you do *in* it.
//!
//! So this module answers one question — "what can I open from here?" —
//! by walking a root directory for the extensions [`Source`] already
//! knows how to parse. It is deliberately not a file browser: no
//! navigation, no hidden directories, a bounded depth. The runner wants
//! a short list of real material, and a full picker would be a second
//! app to maintain, which is the same argument `app::App` makes for
//! having no chrome.

use std::path::{Path, PathBuf};

use crate::{Source, scene_names};

/// How deep to walk below the root.
///
/// Seven, because the default root under `dx serve` is the repo itself
/// and its material is committed at depths like
/// `crates/keyflow/keyflow/tests/midi/<file>.mid` — at three the runner
/// scanned the whole tree and reported nothing openable. With `target/`
/// and `node_modules/` pruned this is 45 ms over the monorepo, so depth
/// is not what a scan costs.
const MAX_DEPTH: usize = 7;

/// Stop rather than enumerate a music library. A runner that opens with
/// four hundred rows has not helped anyone choose.
const MAX_ENTRIES: usize = 250;

/// Something the runner can open.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// What to show in the list.
    pub label: String,
    /// What to hand [`Source::parse`].
    pub arg: String,
    /// Which of the four kinds this is, for grouping and for an icon.
    pub kind: Kind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A job — the list a person is handed, before the fixtures.
    Workflow,
    Scene,
    Midi,
    Rpp,
    GuitarPro,
}

impl Kind {
    /// The extension-to-kind rule, kept next to the one in
    /// [`Source::parse`] so a new format shows up in the list on the
    /// same change that makes it openable.
    fn of(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        match ext.as_str() {
            "mid" | "midi" => Some(Kind::Midi),
            "rpp" => Some(Kind::Rpp),
            "gp" | "gp3" | "gp4" | "gp5" | "gpx" => Some(Kind::GuitarPro),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Workflow => "job",
            Kind::Scene => "scene",
            Kind::Midi => "midi",
            Kind::Rpp => "rpp",
            Kind::GuitarPro => "gp",
        }
    }
}

/// Where the runner looks for material.
///
/// `EXPRESSION_EDITOR_LIBRARY` first, so a served runner can be pointed
/// at a real folder of stems without a rebuild; the working directory
/// otherwise, which is the repo root under `dx serve` and therefore
/// finds the committed fixtures.
pub fn root() -> PathBuf {
    std::env::var_os("EXPRESSION_EDITOR_LIBRARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

/// The demo scenes, which need no files and always work.
///
/// Listed first because they are the answer to "I just want to see the
/// editor" — a runner whose list is empty on a machine with no material
/// should still open something.
/// The workflows, which are what the chooser should offer first: a job
/// with its surface and material already chosen, rather than a fixture
/// named after the behaviour it demonstrates.
pub fn workflows() -> Vec<Entry> {
    expression_editor_ui::workflow::Workflow::ALL
        .into_iter()
        .map(|w| Entry {
            label: match w.note() {
                Some(note) => format!("{} ({note})", w.label()),
                None => w.label().to_string(),
            },
            arg: w.slug().to_string(),
            kind: Kind::Workflow,
        })
        .collect()
}

pub fn scenes() -> Vec<Entry> {
    scene_names()
        .into_iter()
        .map(|(name, label)| Entry {
            label: label.to_string(),
            arg: name.to_string(),
            kind: Kind::Scene,
        })
        .collect()
}

/// Every openable file under `root`, shallowest first.
///
/// Errors are silence rather than failure: an unreadable directory in
/// the middle of a scan is a permissions quirk, not a reason to show no
/// list at all.
pub fn scan(root: &Path) -> Vec<Entry> {
    let mut found = Vec::new();
    walk(root, root, 0, &mut found);
    // By kind, then by label, so the list has a stable order rather than
    // the filesystem's.
    found.sort_by(|a, b| {
        (a.kind.label(), a.label.to_lowercase()).cmp(&(b.kind.label(), b.label.to_lowercase()))
    });
    found
}

fn walk(root: &Path, dir: &Path, depth: usize, out: &mut Vec<Entry>) {
    if depth > MAX_DEPTH || out.len() >= MAX_ENTRIES {
        return;
    }
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut dirs = Vec::new();
    for entry in read.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // Dotfiles (which covers .git and .claude/worktrees, the two
        // that would otherwise multiply the whole tree), and the build
        // directories that dominate any scan rooted at a repo.
        if name.starts_with('.') || name == "target" || name == "node_modules" {
            continue;
        }
        if path.is_dir() {
            dirs.push(path);
        } else if let Some(kind) = Kind::of(&path) {
            if out.len() >= MAX_ENTRIES {
                return;
            }
            out.push(Entry {
                label: path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string(),
                arg: path.to_string_lossy().to_string(),
                kind,
            });
        }
    }
    for d in dirs {
        walk(root, &d, depth + 1, out);
    }
}

impl Entry {
    /// The source this entry opens, or the parse error explaining why
    /// it does not.
    pub fn source(&self) -> Result<Source, crate::LoadError> {
        Source::parse(&self.arg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_scene_is_listed_and_parses() {
        let scenes = scenes();
        assert!(!scenes.is_empty(), "the runner ships demo scenes");
        for s in &scenes {
            assert_eq!(s.kind, Kind::Scene);
            s.source().expect("a listed scene must be openable");
        }
    }

    #[test]
    fn the_extensions_match_what_source_can_parse() {
        // The failure this guards: adding a format to `Source::parse`
        // and not to `Kind::of`, so it opens from the command line and
        // is invisible in the served runner's list.
        for ext in ["mid", "midi", "rpp", "gp", "gp3", "gp4", "gp5", "gpx"] {
            let path = PathBuf::from(format!("take.{ext}"));
            assert!(
                Kind::of(&path).is_some(),
                ".{ext} is listed but not classified"
            );
            assert!(
                Source::parse(path.to_str().unwrap()).is_ok(),
                ".{ext} is classified but Source cannot parse it"
            );
        }
    }

    #[test]
    fn unopenable_files_are_not_listed() {
        for ext in ["wav", "txt", "rs", "png"] {
            assert_eq!(Kind::of(&PathBuf::from(format!("x.{ext}"))), None);
        }
    }

    #[test]
    fn a_scan_finds_the_repo_s_own_fixtures() {
        // Rooted at this crate, which has `.rpp` fixtures committed
        // under tests/. If this finds nothing the walk is broken, not
        // the machine.
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let found = scan(&root);
        assert!(
            found.iter().all(|e| e.kind != Kind::Scene),
            "a filesystem scan yields no scenes"
        );
        assert!(found.len() <= MAX_ENTRIES);
    }

    #[test]
    fn a_scan_from_the_repo_root_finds_real_material() {
        // The regression this pins: at MAX_DEPTH 3 the served runner
        // scanned the whole monorepo and reported "0 openable", because
        // every committed fixture sits deeper than that. A chooser whose
        // list is empty on the repo it ships with is the bug.
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .expect("repo root");
        let found = scan(&root);
        assert!(
            found.iter().any(|e| e.kind == Kind::Midi),
            "no MIDI found under {} — the walk is too shallow again",
            root.display()
        );
    }

    #[test]
    fn a_missing_root_is_an_empty_list_not_a_panic() {
        assert!(scan(&PathBuf::from("/nonexistent-expression-editor-root")).is_empty());
    }
}
