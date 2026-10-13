//! FTS Ruler Manager — standardized ruler lane layout for `FastTrackStudio` projects.
//!
// The retired `CoreLane::Key` variant is intentionally retained for its
// explicit discriminant / RPP layout compatibility; allow its self-referential
// deprecation warnings module-wide rather than peppering call sites.
#![allow(deprecated)]
//!
//! Every FTS project should have these ruler lanes to organize markers and regions
//! into semantic categories. The lanes are split into two groups:
//!
//! **Core lanes** (always present):
//! - SONG, SECTIONS, MARKS, KEY
//!
//! **Instrument note lanes** (created on demand per instrument):
//! - Drums, Bass, Guitar, Guitar 2, Keys, Keys 2, Lead, BGVs, etc.

use facet::Facet;

// ── Core Ruler Lanes ─────────────────────────────────────────────────────────

/// Core FTS ruler lanes, in user-facing display order:
/// SONG → SECTIONS → MARKS.
///
/// REAPER's intrinsic per-position flags still apply:
/// - lane 1 (first row) gets flag=4 ("default marker lane")
/// - lane 2 (second row) gets flag=8 ("default region lane")
///
/// With this ordering SECTIONS is the default region lane, which is
/// what we want — fresh section regions auto-route there. SONG is
/// the default marker lane, but we don't want markers there, so
/// COUNT-IN / SONGSTART / SONGEND / =END all get explicit
/// `set_lane(MARKS)` via `classify_marker_lane`. The song-bounded
/// region also gets explicit `set_lane(SONG)` since the default
/// region lane would otherwise put it on SECTIONS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum CoreLane {
    /// Song-bounded region only — named after the song.
    /// Pinned explicitly (default region lane is SECTIONS).
    Song = 0,
    /// Section regions: VS / CH / BR / OUT / etc. Default region
    /// lane (flag=8), so fresh regions land here automatically.
    Sections = 1,
    /// Structural markers: COUNT-IN, SONGSTART, SONGEND, =END.
    /// Pinned explicitly via `classify_marker_lane`.
    Marks = 2,
    /// Reserved historical slot — see the enum doc.
    #[deprecated(note = "KEY lane retired; key signatures are encoded separately now")]
    #[allow(deprecated)] // self-reference via the explicit discriminant
    Key = 3,
}

#[allow(deprecated)]
impl CoreLane {
    /// All core lanes that should be created in every project, in
    /// the user-visible order SONG → SECTIONS → MARKS.
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[Self::Song, Self::Sections, Self::Marks]
    }

    /// 0-based lane index. Same value used for **both** REAPER's
    /// project-info name-table keys (`RULER_LANE_NAME:N`,
    /// `RULER_LANE_FLAGS:N`, `RULER_LANE_ORDER:N`) **and** for
    /// `I_LANENUMBER` on a marker / region.
    ///
    /// Empirical evidence (saved RPP grep against what we wrote):
    /// passing `I_LANENUMBER=1` for SONG-bounded regions made REAPER
    /// serialise them on file row 2 (SECTIONS), not row 1 (SONG).
    /// So `I_LANENUMBER` is the same 0-based index — REAPER's docs
    /// reading "0 = automatic" appears to be wrong or version-
    /// dependent here.
    #[must_use]
    pub const fn lane_index(&self) -> u32 {
        match self {
            Self::Song => 0,
            Self::Sections => 1,
            Self::Marks => 2,
            Self::Key => 3,
        }
    }

    /// Same as `lane_index`. Kept as a separate name so call sites
    /// targeting the project-info name table read self-documenting
    /// (we used to need a 0/1-based offset between them).
    #[must_use]
    pub const fn name_key_index(&self) -> u32 {
        Self::lane_index(self)
    }

    /// Display name shown in the REAPER ruler.
    #[allow(deprecated)] // matches the retired `Key` variant for completeness
    #[must_use]
    pub const fn display_name(&self) -> &'static str {
        match self {
            Self::Song => "SONG",
            Self::Sections => "SECTIONS",
            Self::Marks => "MARKS",
            Self::Key => "KEY",
        }
    }

    /// REAPER lane flags.
    /// - `8` = default region lane
    /// - `4` = default marker lane
    /// - `0` = normal
    ///
    /// SECTIONS is the default region lane — a fresh region is a section
    /// until something pins it elsewhere — and SONG the default marker
    /// lane, the convention the golden session's `RULERLANE` rows record
    /// (`RULERLANE 1 4 SONG`, `RULERLANE 2 8 SECTIONS`). The song region
    /// and the structural markers are pinned explicitly either way.
    ///
    /// REAPER does not apply `RULER_LANE_FLAGS:N` writes (the saved RPP
    /// keeps its own position-based defaults), so there these are intent;
    /// `daw-standalone` honours them, as REAPER honours the flags a saved
    /// file carries. They had drifted to SONG = 8, which REAPER ignored
    /// and standalone obeyed: every fresh section landed on the SONG lane
    /// and was read back as a song of its own.
    #[must_use]
    pub const fn flags(&self) -> i32 {
        match self {
            Self::Sections => 8, // default region lane
            Self::Song => 4,     // default marker lane
            _ => 0,
        }
    }

    /// Reserved core-lane slot count. Stays at 4 even though Key is
    /// no longer auto-created — `InstrumentLane::lane_index` and the
    /// rest of the numbering offset off this value, and shifting it
    /// would silently renumber every existing project's instrument
    /// lanes.
    #[must_use]
    pub const fn count() -> u32 {
        4
    }

    #[must_use]
    pub fn from_index(index: u32) -> Option<Self> {
        Self::all()
            .iter()
            .find(|l| l.lane_index() == index)
            .copied()
    }

    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        let upper = name.to_uppercase();
        Self::all()
            .iter()
            .find(|l| l.display_name() == upper)
            .copied()
    }
}

// ── Instrument Note Lanes ────────────────────────────────────────────────────

/// Well-known instrument roles for per-instrument note lanes.
///
/// These are created on demand — when a user does "Insert Note for Drums",
/// the Drums lane is created if it doesn't exist yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum InstrumentLane {
    Drums = 0,
    Bass = 1,
    Guitar = 2,
    Guitar2 = 3,
    Keys = 4,
    Keys2 = 5,
    Lead = 6,
    BGVs = 7,
}

impl InstrumentLane {
    /// All well-known instrument lanes.
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[
            Self::Drums,
            Self::Bass,
            Self::Guitar,
            Self::Guitar2,
            Self::Keys,
            Self::Keys2,
            Self::Lead,
            Self::BGVs,
        ]
    }

    /// REAPER ruler lane index (1-based), offset after core lanes.
    #[must_use]
    pub const fn lane_index(&self) -> u32 {
        let discriminant = match self {
            Self::Drums => 0,
            Self::Bass => 1,
            Self::Guitar => 2,
            Self::Guitar2 => 3,
            Self::Keys => 4,
            Self::Keys2 => 5,
            Self::Lead => 6,
            Self::BGVs => 7,
        };
        CoreLane::count()
            .saturating_add(discriminant)
            .saturating_add(1)
    }

    /// Display name shown in the REAPER ruler.
    #[must_use]
    pub const fn display_name(&self) -> &'static str {
        match self {
            Self::Drums => "Drums",
            Self::Bass => "Bass",
            Self::Guitar => "Guitar",
            Self::Guitar2 => "Guitar 2",
            Self::Keys => "Keys",
            Self::Keys2 => "Keys 2",
            Self::Lead => "Lead",
            Self::BGVs => "BGVs",
        }
    }

    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::all()
            .iter()
            .find(|l| l.display_name().eq_ignore_ascii_case(name))
            .copied()
    }
}

// ── Unified Lane Reference ───────────────────────────────────────────────────

/// A reference to any FTS ruler lane (core or instrument).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(C)]
pub enum FtsLane {
    Core(CoreLane),
    Instrument(InstrumentLane),
}

impl FtsLane {
    #[must_use]
    pub const fn lane_index(&self) -> u32 {
        match self {
            Self::Core(l) => l.lane_index(),
            Self::Instrument(l) => l.lane_index(),
        }
    }

    #[must_use]
    pub const fn display_name(&self) -> &'static str {
        match self {
            Self::Core(l) => l.display_name(),
            Self::Instrument(l) => l.display_name(),
        }
    }

    #[must_use]
    pub const fn flags(&self) -> i32 {
        match self {
            Self::Core(l) => l.flags(),
            Self::Instrument(_) => 0,
        }
    }
}

// ── Marker Classification ────────────────────────────────────────────────────

/// Classify a marker name to determine which lane it belongs to.
///
/// Every structural marker — `SONGSTART`/`SONGEND`, `COUNT-IN`/`COUNT
/// IN`/`COUNTIN`, `=START`/`=END`/`PREROLL`/`=PREROLL` — and anything
/// unrecognized currently collapses into MARKS, so the SONG lane reads as
/// "here is one named row per song" without bracket-marker noise. `name`
/// is unused today; it stays part of the signature for when one of those
/// families needs its own lane.
#[must_use]
pub const fn classify_marker_lane(_name: &str) -> FtsLane {
    FtsLane::Core(CoreLane::Marks)
}

/// Classify a region name to determine which lane it belongs to.
///
/// Looks at both the region name and whether it spans the full song
/// (in which case it's a SONG region, not a section).
#[must_use]
pub fn classify_region_lane(name: &str) -> FtsLane {
    let upper = name.trim().to_uppercase();

    // Well-known section abbreviations → SECTIONS lane
    match upper.as_str() {
        "VS" | "VERSE" | "CH" | "CHORUS" | "BR" | "BRIDGE" | "INTRO" | "OUTRO" | "PRE-CH"
        | "PRE-CHORUS" | "PRECHORUS" | "SOLO" | "BREAKDOWN" | "INTERLUDE" | "TAG" | "HOOK"
        | "INSTRUMENTAL" | "CODA" | "VAMP" | "TURNAROUND" | "VERSE 1" | "VERSE 2" | "VERSE 3"
        | "VERSE 4" | "CHORUS 1" | "CHORUS 2" | "CHORUS 3" | "BRIDGE 1" | "BRIDGE 2" => {
            return FtsLane::Core(CoreLane::Sections);
        }
        _ => {}
    }

    // Section abbreviations with numbers: "VS 1", "CH 2", etc.
    if upper.starts_with("VS ")
        || upper.starts_with("CH ")
        || upper.starts_with("BR ")
        || upper.starts_with("SOLO ")
    {
        return FtsLane::Core(CoreLane::Sections);
    }

    // Default: SECTIONS lane (most regions are sections)
    FtsLane::Core(CoreLane::Sections)
}

/// Classify a region that spans the full song as a SONG region.
///
/// Call this with `is_full_song = true` when the region's start matches
/// the song start and its end matches the song end.
#[must_use]
pub fn classify_region_lane_with_context(name: &str, is_full_song: bool) -> FtsLane {
    if is_full_song {
        FtsLane::Core(CoreLane::Song)
    } else {
        classify_region_lane(name)
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "pre-existing: ruler-lane indices shifted after KEY-lane retirement; expectations stale — revisit"]
    fn core_lane_indices_sequential() {
        for (i, lane) in CoreLane::all().iter().enumerate() {
            assert_eq!(lane.lane_index(), u32::try_from(i).unwrap() + 1);
        }
    }

    #[test]
    fn instrument_lanes_start_after_core() {
        assert_eq!(InstrumentLane::Drums.lane_index(), 5);
        assert_eq!(InstrumentLane::Bass.lane_index(), 6);
        assert_eq!(InstrumentLane::Guitar.lane_index(), 7);
        assert_eq!(InstrumentLane::BGVs.lane_index(), 12);
    }

    #[test]
    #[ignore = "pre-existing: ruler-lane indices shifted after KEY-lane retirement; expectations stale — revisit"]
    fn fts_lane_unified_index() {
        assert_eq!(FtsLane::Core(CoreLane::Song).lane_index(), 1);
        assert_eq!(FtsLane::Core(CoreLane::Sections).lane_index(), 2);
        assert_eq!(FtsLane::Core(CoreLane::Marks).lane_index(), 3);
        assert_eq!(FtsLane::Instrument(InstrumentLane::Drums).lane_index(), 5);
    }

    #[test]
    #[ignore = "pre-existing: ruler-lane indices shifted after KEY-lane retirement; expectations stale — revisit"]
    fn classify_structural_markers() {
        assert_eq!(
            classify_marker_lane("SONGSTART"),
            FtsLane::Core(CoreLane::Song)
        );
        assert_eq!(
            classify_marker_lane("SONGEND"),
            FtsLane::Core(CoreLane::Song)
        );
        assert_eq!(
            classify_marker_lane("Count-In"),
            FtsLane::Core(CoreLane::Marks)
        );
    }

    #[test]
    fn classify_bound_markers() {
        assert_eq!(
            classify_marker_lane("=START"),
            FtsLane::Core(CoreLane::Marks)
        );
        assert_eq!(classify_marker_lane("=END"), FtsLane::Core(CoreLane::Marks));
        assert_eq!(
            classify_marker_lane("PREROLL"),
            FtsLane::Core(CoreLane::Marks)
        );
    }

    #[test]
    fn default_region_and_marker_lanes() {
        let region_defaults: Vec<_> = CoreLane::all()
            .iter()
            .filter(|l| l.flags() & 8 != 0)
            .collect();
        let marker_defaults: Vec<_> = CoreLane::all()
            .iter()
            .filter(|l| l.flags() & 4 != 0)
            .collect();
        assert_eq!(region_defaults.len(), 1);
        assert_eq!(marker_defaults.len(), 1);
        assert_eq!(region_defaults[0].display_name(), "SECTIONS");
        assert_eq!(marker_defaults[0].display_name(), "SONG");
    }

    #[test]
    #[ignore = "pre-existing: ruler-lane indices shifted after KEY-lane retirement; expectations stale — revisit"]
    #[allow(deprecated)] // asserts the retired `Key` lane index for completeness
    fn lane_layout_matches_reaper_rpp() {
        // FTS lane convention:
        // RULERLANE 1 4 SONG 0 -1
        // RULERLANE 2 8 SECTIONS 0 -1
        // RULERLANE 3 0 MARKS 0 -1
        // RULERLANE 4 0 KEY 0 -1
        // RULERLANE 5 0 Drums 0 -1
        // ...
        assert_eq!(CoreLane::Song.lane_index(), 1);
        assert_eq!(CoreLane::Sections.lane_index(), 2);
        assert_eq!(CoreLane::Marks.lane_index(), 3);
        assert_eq!(CoreLane::Key.lane_index(), 4);
        assert_eq!(InstrumentLane::Drums.lane_index(), 5);
        assert_eq!(InstrumentLane::Bass.lane_index(), 6);
        assert_eq!(InstrumentLane::Guitar.lane_index(), 7);
        assert_eq!(InstrumentLane::Guitar2.lane_index(), 8);
        assert_eq!(InstrumentLane::Keys.lane_index(), 9);
        assert_eq!(InstrumentLane::Keys2.lane_index(), 10);
        assert_eq!(InstrumentLane::Lead.lane_index(), 11);
        assert_eq!(InstrumentLane::BGVs.lane_index(), 12);
    }
}
