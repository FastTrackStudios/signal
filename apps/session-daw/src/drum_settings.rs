//! Drum editing's settings: sticky, one set for the session.
//!
//! What a quantize is dialled to stays dialled — set once, every
//! quantize after uses it, and it is still set after a restart
//! (`~/.config/fts/drum-edit.json`). A call may still carry its own for
//! that call alone ([`Patch`], flattened into its arguments), without
//! moving the ones that stay. How the hits are found is the hit
//! detector's own (`crate::hits`).

use facet::Facet;

/// What to move the hits to.
#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Target {
    /// The grid, the drum MIDI as a guide: a hit with a written note near
    /// goes to the line that note is written on — the MIDI is quantized,
    /// so it says WHICH line (straight or a triplet), the grid says when;
    /// a hit with none (a fill played otherwise) to the nearest line when
    /// it is plainly near one (`sure_ms`); else it is left, to be
    /// listened to.
    Guided,
    /// The nearest note of the drum MIDI.
    Reference,
    /// The nearest grid line.
    Grid,
    /// The section's own groove (`crate::groove`): a hit in a slot the
    /// section plays bar after bar goes to that slot's line, however far
    /// it drifted — the line the bars around it say, not the nearest; a
    /// hit in no slot as Guided.
    Pattern,
}

/// What a slip does with the fills.
#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Fills {
    /// Leave them as played.
    Skip,
    /// Edit them like the rest.
    Include,
    /// Edit only them.
    Only,
}

/// Every setting.
#[derive(Facet, Clone, Debug, PartialEq)]
pub struct Settings {
    /// The grid a stroke nothing else places goes to: a whole note's
    /// division — eighths (8) unless asked. Finer only where it is known
    /// what is there: a written note sends its stroke to its own line (a
    /// sixteenth, a triplet), a Grid region (`crate::track_regions`) its
    /// stretch's strokes to its grid — the way to give a part (a section)
    /// a grid of its own.
    pub grid: u32,
    /// One hit per grid line, the loudest near it.
    pub grid_scan: bool,
    /// How near its line a hit must be for the grid scan to keep it, ms
    /// (none: every hit is some line's).
    pub grid_tolerance_ms: Option<f64>,
    pub target: Target,
    /// How far a hit's written note may be, ms.
    pub window_ms: f64,
    /// Guided, no note near: how near a line a hit must be to go to it,
    /// ms.
    pub sure_ms: f64,
    /// A hit this near its target already stays, ms.
    pub tolerance_ms: f64,
    /// How much of each move is made, 0..1.
    pub strength: f64,
    /// The cut before each hit, ms.
    pub lead_ms: f64,
    /// The crossfade across each seam, ms.
    pub xfade_ms: f64,
    /// A gap a slip leaves longer than this (ms) is filled by stretching
    /// the decay before it — the last resort; none, never.
    pub stretch_gaps_ms: Option<f64>,
    pub fills: Fills,
    /// The piece whose timing wins where strokes are moved as one gesture
    /// (a kick into a snare, a double kick around a backbeat): its stroke
    /// put on its line, the rest moved with it — the snare in a breakdown,
    /// over the sixteenth kicks. None: the loudest.
    pub lead: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            grid: 8,
            grid_scan: false,
            grid_tolerance_ms: None,
            target: Target::Guided,
            window_ms: 30.0,
            sure_ms: 15.0,
            tolerance_ms: 3.0,
            strength: 1.0,
            lead_ms: 5.0,
            xfade_ms: 5.0,
            stretch_gaps_ms: None,
            fills: Fills::Skip,
            lead: None,
        }
    }
}

/// Some settings: each one given replaces the one there.
#[derive(Facet, Clone, Debug, Default, PartialEq)]
pub struct Patch {
    pub grid: Option<u32>,
    pub grid_scan: Option<bool>,
    pub grid_tolerance_ms: Option<f64>,
    pub target: Option<Target>,
    pub window_ms: Option<f64>,
    pub sure_ms: Option<f64>,
    pub tolerance_ms: Option<f64>,
    pub strength: Option<f64>,
    pub lead_ms: Option<f64>,
    pub xfade_ms: Option<f64>,
    /// 0 (or less): never.
    pub stretch_gaps_ms: Option<f64>,
    pub fills: Option<Fills>,
    /// A piece's name (`Snare`); empty: none, the loudest decides.
    pub lead: Option<String>,
}

impl Settings {
    /// These, with `patch`'s in place of theirs.
    #[must_use]
    pub fn with(&self, patch: &Patch) -> Self {
        let pick = |given: Option<f64>, was: f64| given.unwrap_or(was);
        Self {
            grid: patch.grid.unwrap_or(self.grid).max(1),
            grid_scan: patch.grid_scan.unwrap_or(self.grid_scan),
            // Zero or less: no tolerance — every hit is some line's.
            grid_tolerance_ms: match patch.grid_tolerance_ms {
                Some(ms) if ms > 0.0 => Some(ms),
                Some(_) => None,
                None => self.grid_tolerance_ms,
            },
            target: patch.target.unwrap_or(self.target),
            window_ms: pick(patch.window_ms, self.window_ms).max(0.0),
            sure_ms: pick(patch.sure_ms, self.sure_ms).max(0.0),
            tolerance_ms: pick(patch.tolerance_ms, self.tolerance_ms).max(0.0),
            strength: pick(patch.strength, self.strength).clamp(0.0, 1.0),
            lead_ms: pick(patch.lead_ms, self.lead_ms).max(0.0),
            xfade_ms: pick(patch.xfade_ms, self.xfade_ms).max(0.0),
            stretch_gaps_ms: match patch.stretch_gaps_ms {
                Some(ms) if ms > 0.0 => Some(ms),
                Some(_) => None,
                None => self.stretch_gaps_ms,
            },
            fills: patch.fills.unwrap_or(self.fills),
            lead: match patch.lead.as_deref().map(str::trim) {
                Some("") => None,
                Some(piece) => Some(piece.to_owned()),
                None => self.lead.clone(),
            },
        }
    }
}

/// The settings as they stand.
fn store() -> &'static std::sync::Mutex<Settings> {
    static STORE: std::sync::OnceLock<std::sync::Mutex<Settings>> = std::sync::OnceLock::new();
    STORE.get_or_init(|| std::sync::Mutex::new(load().unwrap_or_default()))
}

/// The settings as they stand.
#[must_use]
pub fn current() -> Settings {
    store()
        .lock()
        .map_or_else(|e| e.into_inner().clone(), |s| s.clone())
}

/// Change some, keep them (and save them), and say what they are now.
pub fn set(patch: &Patch) -> Settings {
    let now = {
        let mut held = store()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *held = held.with(patch);
        held.clone()
    };
    save(&now);
    now
}

/// Back to the defaults (and saved).
pub fn reset() -> Settings {
    let now = Settings::default();
    *store()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = now.clone();
    save(&now);
    now
}

/// Where they are kept.
#[cfg(feature = "native")]
fn path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join("fts").join("drum-edit.json"))
}

/// A page keeps them for the visit: it has no config folder.
#[cfg(not(feature = "native"))]
const fn path() -> Option<std::path::PathBuf> {
    None
}

fn load() -> Option<Settings> {
    let text = std::fs::read(path()?).ok()?;
    // Saved as a patch: a setting added since is its default.
    let patch: Patch = facet_json::from_slice(&text).ok()?;
    Some(Settings::default().with(&patch))
}

fn save(settings: &Settings) {
    let Some(path) = path() else { return };
    if let Some(dir) = path.parent()
        && std::fs::create_dir_all(dir).is_err()
    {
        return;
    }
    match facet_json::to_vec(settings) {
        Ok(bytes) => {
            if std::fs::write(&path, bytes).is_err() {
                tracing::warn!("drum edit: the settings could not be saved");
            }
        }
        Err(error) => tracing::warn!(error = %error, "drum edit: the settings would not serialize"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_patch_changes_what_it_names_and_nothing_else() {
        let was = Settings::default();
        let now = was.with(&Patch {
            lead_ms: Some(8.0),
            grid_scan: Some(true),
            ..Patch::default()
        });
        assert!((now.lead_ms - 8.0).abs() < f64::EPSILON);
        assert!(now.grid_scan);
        assert_eq!(
            Settings {
                lead_ms: was.lead_ms,
                grid_scan: was.grid_scan,
                ..now
            },
            was
        );
    }

    /// A call's arguments, its settings written in among them.
    #[derive(Facet)]
    struct Call {
        #[facet(flatten, default)]
        with: Patch,
        from: Option<f64>,
    }

    /// A call that gives no settings still parses — and one that does
    /// gives them.
    #[test]
    fn a_call_carries_its_settings_or_none() {
        let none: Call = facet_json::from_slice(b"{}").expect("no settings");
        assert_eq!(none.with, Patch::default());
        let some: Call =
            facet_json::from_slice(br#"{"lead_ms":3.0,"from":1.5}"#).expect("some settings");
        assert_eq!(some.with.lead_ms, Some(3.0));
        assert_eq!(some.from, Some(1.5));
    }

    /// What is saved reads back the same, and a file missing settings
    /// added since reads with their defaults.
    #[test]
    fn saved_settings_read_back() {
        let tuned = Settings::default().with(&Patch {
            grid_tolerance_ms: Some(25.0),
            fills: Some(Fills::Only),
            ..Patch::default()
        });
        let bytes = facet_json::to_vec(&tuned).expect("serializes");
        let patch: Patch = facet_json::from_slice(&bytes).expect("reads as a patch");
        assert_eq!(Settings::default().with(&patch), tuned);
        let old: Patch = facet_json::from_slice(br#"{"lead_ms":7.0}"#).expect("an older file");
        // A file from when the hits were found here too: its detector's
        // settings, gone since, are passed over.
        let older: Patch = facet_json::from_slice(
            br#"{"detector":"Level","under_db":24.0,"retrig_ms":40.0,"lead_ms":6.0}"#,
        )
        .expect("a file with the old detector's settings");
        assert_eq!(older.lead_ms, Some(6.0));
        assert_eq!(
            Settings::default().with(&old).xfade_ms.to_bits(),
            5.0_f64.to_bits()
        );
    }
}
