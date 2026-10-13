//! Sound Effects (SFX) group definitions
//!
//! This group captures non-musical audio elements like:
//! - Sound effects (whoosh, impact, etc.)
//! - Voice effects (robot, processed, etc.)
//! - Foley and ambient sounds
//! - Generic "FX" tracks that aren't tied to a specific instrument
//!
//! Note: Click, count-in, and cue tracks are now in the Click group

use crate::item_metadata::ItemMetadata;
use monarchy::Group;

/// Top-level SFX group containing sound effects and non-musical audio
pub struct SFX;

impl From<SFX> for Group<ItemMetadata> {
    fn from(_val: SFX) -> Self {
        Self::builder("SFX")
            .prefix("SFX")
            .patterns(vec![
                // Generic FX patterns
                "fx",
                "sfx",
                "effect",
                "effects",
                // Hardware FX units / Classic gear
                "h3000",      // Eventide H3000
                "echoplex",   // Maestro Echoplex
                "echo plex",  // Alternative spelling
                "space echo", // Roland Space Echo
                "tape echo",  // Generic tape echo
                "spring reverb",
                "plate reverb",
                "eko",          // Effect abbreviation / Eko brand
                "mellotron fx", // Mellotron effects
                // Sound design
                "whoosh",
                "impact",
                "riser",
                "sweep",
                "swell",
                "hit",
                "boom",
                "explosion",
                "transition",
                // Voice effects
                "robot",
                "vocoder",
                "talkbox",
                "talk box",
                // Foley and ambient
                "foley",
                "ambient",
                "atmo",
                "atmosphere",
                "room tone",
                "noise",
                // DJ / turntablism
                "scratch",
                "scratches",
                "dj",
                "turntable",
                // Misc audio markers
                "slate",
                "beep",
                "tone",
            ])
            .build()
    }
}
