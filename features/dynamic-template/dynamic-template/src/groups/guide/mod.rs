//! Guide track group definitions
//!
//! This group captures timing and cue tracks for performers:
//! - Click tracks (metronome)
//! - Count-ins
//! - Cue tracks for performers' in-ears
//! - Song section markers (Verse, Chorus, Bridge, etc.)
//! - Dynamic cues (Build, Softly, All In, etc.)

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Top-level Guide group containing timing, cue, and section tracks
pub struct Guide;

impl From<Guide> for Group<ItemMetadata> {
    fn from(_val: Guide) -> Self {
        Self::builder("Guide")
            .prefix("GDE")
            .patterns(vec![
                // Generic patterns (matched by subgroups)
                "click",
                "metronome",
                "count",
                "guide",
                "cue",
                // A printed shaker keeps time the way a click does.
                "shakeprint",
                "shake print",
                "shaker print",
                "clickprint",
                "click print",
            ])
            // A cue *bus* is a per-player monitor mix, not a guide track. The
            // exclusion has to sit on the container as well as on Cues: the
            // parser matches each group on its own patterns, so blocking only
            // the child leaves the parent claiming "Cue Buss 1".
            .exclude(["buss", "bus", "headphone", "headphones"])
            .group(Prints)
            .group(Click)
            .group(Count)
            .group(Cues)
            .group(Sections)
            .group(DynamicCues)
            .build()
    }
}

/// Rendered prints of the guide — `ClickPrint`, `ShakePrint`: the click
/// and the shaker bounced to audio, kept under the guide in their own
/// folder (shut until opened) rather than beside the live click.
pub struct Prints;

impl From<Prints> for Group<ItemMetadata> {
    fn from(_val: Prints) -> Self {
        Self::builder(crate::apply::PRINTS_FOLDER)
            .patterns(vec![
                "clickprint",
                "click print",
                "shakeprint",
                "shake print",
                "shaker print",
                // Not a bare "print": groups match on their own patterns,
                // and "Mix Print" is a reference, not the guide's.
            ])
            .build()
    }
}

/// Click/Metronome subgroup for timing tracks
pub struct Click;

impl From<Click> for Group<ItemMetadata> {
    fn from(_val: Click) -> Self {
        Self::builder("Click")
            .patterns(vec![
                "click",
                "click track",
                // A shaker guide is a click you can groove to: `Shaker
                // Guide` — the guide's, not the percussion's. (A printed one
                // is a print: `Prints`.)
                "shaker guide",
                "guide shaker",
                "shaker click",
                "metronome",
                "met",
                "tempo",
                "tempo track",
            ])
            // A print of the click is a print (`Prints`).
            .exclude(vec![
                "print",
                "clickprint",
                "click print",
                "shakeprint",
                "shake print",
                "shaker print",
            ])
            .build()
    }
}

/// Count subgroup for count-ins (not part of final song)
pub struct Count;

impl From<Count> for Group<ItemMetadata> {
    fn from(_val: Count) -> Self {
        Self::builder("Count")
            .patterns(vec![
                "count",
                "count in",
                "count-in",
                "countin",
                "count off",
                "1234",
                "one two three four",
                "intro count",
            ])
            .build()
    }
}

/// Cues subgroup for performer cue tracks (in-ears)
pub struct Cues;

impl From<Cues> for Group<ItemMetadata> {
    fn from(_val: Cues) -> Self {
        Self::builder("Cues")
            .patterns(vec![
                // Guide patterns
                "guide",
                "guide track",
                "guide vox",
                "guide vocal",
                // Section guides
                "sections",
                "sections guide",
                "section guide",
                "tracks guide",
                // Cue/callout patterns
                "cue",
                "cue track",
                "callout",
                "callouts",
                "call out",
                "call outs",
                // IEM/monitor patterns
                "iem",
                "in ear",
                "in-ear",
                "monitor cue",
                "ear cue",
            ])
            // A cue *bus* is a per-player monitor mix, not a spoken callout —
            // "Cue Buss 1" belongs to Headphones. Guide is registered first, so
            // without this it takes every cue name in the project.
            // A print is a print, whatever folder it sits in: "Guide
            // ClickPrint" is not a cue.
            .exclude([
                "buss",
                "bus",
                "headphone",
                "headphones",
                "clickprint",
                "click print",
                "shakeprint",
                "shake print",
                "shaker print",
            ])
            .build()
    }
}

/// Song section markers (Verse, Chorus, Bridge, etc.)
///
/// These patterns match common song structure terms used in
/// markers, regions, and cue tracks.
pub struct Sections;

impl From<Sections> for Group<ItemMetadata> {
    fn from(_val: Sections) -> Self {
        Self::builder("Sections")
            .patterns(vec![
                // Explicit section markers (multi-word or unambiguous — avoids stealing
                // "Solo Gtr", "Outro Gtr", "Chorus Harmony" etc. from instrument groups)
                "pre chorus",
                "pre-chorus",
                "prechorus",
                "post chorus",
                "post-chorus",
                "postchorus",
                "breakdown",
                "interlude",
                "instrumental",
                "ending",
                "tag",
                "vamp",
                "turnaround",
                "refrain",
                "acapella",
                "a capella",
                "exhortation",
                // Numbered sections (common in arrangements)
                "section 1",
                "section 2",
                "section 3",
                "section 4",
                "section 5",
                "section 6",
                "section 7",
            ])
            .build()
    }
}

/// Dynamic cues for performers (Build, Softly, All In, etc.)
///
/// These patterns match performance direction terms used in
/// cue tracks and markers to guide dynamics and energy.
pub struct DynamicCues;

impl From<DynamicCues> for Group<ItemMetadata> {
    fn from(_val: DynamicCues) -> Self {
        Self::builder("Dynamic Cues")
            .patterns(vec![
                // Energy/intensity cues
                "build",
                "slowly build",
                "all in",
                "all-in",
                "allin",
                "softly",
                "soft",
                "hold",
                "break",
                "hits",
                "hit",
                // Instrument entry cues
                "drums in",
                "bass in",
                "keys in",
                "guitars in",
                "band in",
                // Endings
                "big ending",
                "big end",
                "last time",
                "final",
                // Key changes
                "key change up",
                "key change down",
                "modulation",
                "mod up",
                "mod down",
                // Improvisation
                "ad lib",
                "ad-lib",
                "adlib",
                "worship freely",
                "free worship",
                "spontaneous",
                // Other dynamic directions
                "crescendo",
                "decrescendo",
                "fade",
                "fade out",
                "fade in",
                "stop",
                "tacet",
                "rest",
                "wait",
                // NOTE: bare "cue" intentionally NOT listed here — it belongs to
                // the Cues subgroup. Dynamic Cues only matches specific dynamic
                // direction words ("build", "all in", "softly", …). A plain
                // "Cue" track must land in Cues, not Dynamic Cues.
            ])
            .build()
    }
}
