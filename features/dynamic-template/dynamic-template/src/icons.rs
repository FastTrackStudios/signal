//! Track icons based on instrument classification
//!
//! This module provides icon file name definitions for instrument groups that can be used
//! by REAPER extensions to automatically assign track icons based on classification.
//!
//! REAPER looks for icons in:
//! - `REAPER/Data/track_icons`/ (built-in icons)
//! - Custom paths specified with full path
//!
//! Icon formats supported: .png, .ico, .bmp

use std::collections::HashMap;
use std::sync::LazyLock;

/// Icon file name type
pub type IconName = &'static str;

/// Icon definitions for top-level instrument groups
///
/// These names correspond to common icon files that ship with REAPER
/// or are commonly available in icon packs.
pub mod groups {
    use super::IconName;

    // Primary instrument groups
    pub const DRUMS: IconName = "drum.png";
    pub const PERCUSSION: IconName = "percussion.png";
    pub const BASS: IconName = "bass.png";
    pub const GUITARS: IconName = "guitar.png";
    pub const KEYS: IconName = "piano.png";
    pub const SYNTHS: IconName = "synth.png";
    pub const HORNS: IconName = "trumpet.png";
    pub const HARMONICA: IconName = "harmonica.png";
    pub const VOCALS: IconName = "microphone.png";
    pub const CHOIR: IconName = "choir.png";
    pub const ORCHESTRA: IconName = "orchestra.png";
    pub const SFX: IconName = "fx.png";
    pub const GUIDE: IconName = "metronome.png";
    pub const REFERENCE: IconName = "speaker.png";
}

/// Icon definitions for guitar sub-groups
pub mod guitars {
    use super::IconName;

    pub const ELECTRIC: IconName = "electric_guitar.png";
    pub const ACOUSTIC: IconName = "acoustic_guitar.png";
    pub const STEEL: IconName = "pedal_steel.png";
    pub const BANJO: IconName = "banjo.png";
}

/// Icon definitions for vocal sub-groups
pub mod vocals {
    use super::IconName;

    pub const LEAD: IconName = "microphone.png";
    pub const BACKGROUND: IconName = "choir.png";
}

/// Icon definitions for drum sub-groups
pub mod drums {
    use super::IconName;

    pub const DRUM_KIT: IconName = "drum.png";
    pub const ELECTRONIC: IconName = "drum_machine.png";
    pub const KICK: IconName = "kick.png";
    pub const SNARE: IconName = "snare.png";
    pub const TOM: IconName = "tom.png";
    pub const CYMBALS: IconName = "cymbal.png";
    pub const HI_HAT: IconName = "hihat.png";
    pub const ROOM: IconName = "room.png";
}

/// Icon definitions for bass sub-groups
pub mod bass {
    use super::IconName;

    pub const GUITAR: IconName = "bass.png";
    pub const SYNTH: IconName = "synth_bass.png";
    pub const UPRIGHT: IconName = "upright_bass.png";
}

/// Icon definitions for keys sub-groups
pub mod keys {
    use super::IconName;

    pub const PIANO: IconName = "piano.png";
    pub const ELECTRIC: IconName = "electric_piano.png";
    pub const ORGAN: IconName = "organ.png";
    pub const HARPSICHORD: IconName = "harpsichord.png";
    pub const CLAVICHORD: IconName = "clavichord.png";
}

/// Icon definitions for synth sub-groups
pub mod synths {
    use super::IconName;

    pub const LEAD: IconName = "synth_lead.png";
    pub const PAD: IconName = "synth_pad.png";
    pub const ARP: IconName = "synth_arp.png";
    pub const CHORD: IconName = "synth.png";
    pub const KEYS: IconName = "synth_keys.png";
    pub const FX: IconName = "synth_fx.png";
}

/// Icon definitions for orchestra sub-groups
pub mod orchestra {
    use super::IconName;

    pub const STRINGS: IconName = "strings.png";
    pub const WOODWINDS: IconName = "woodwind.png";
    pub const BRASS: IconName = "brass.png";
    pub const HARP: IconName = "harp.png";
    pub const PERCUSSION: IconName = "percussion.png";

    pub mod strings {
        use super::super::IconName;

        pub const VIOLINS: IconName = "violin.png";
        pub const VIOLA: IconName = "viola.png";
        pub const CELLO: IconName = "cello.png";
        pub const CONTRABASS: IconName = "contrabass.png";
    }

    pub mod woodwinds {
        use super::super::IconName;

        pub const FLUTES: IconName = "flute.png";
        pub const OBOES: IconName = "oboe.png";
        pub const CLARINETS: IconName = "clarinet.png";
        pub const BASSOONS: IconName = "bassoon.png";
        pub const PICCOLO: IconName = "piccolo.png";
    }

    pub mod brass {
        use super::super::IconName;

        pub const TRUMPETS: IconName = "trumpet.png";
        pub const HORNS: IconName = "french_horn.png";
        pub const TROMBONES: IconName = "trombone.png";
        pub const TUBA: IconName = "tuba.png";
    }
}

/// Icon definitions for guide/click tracks
pub mod guide {
    use super::IconName;

    pub const CLICK: IconName = "metronome.png";
    pub const GUIDE_TRACK: IconName = "guide.png";
    pub const CUE: IconName = "cue.png";
    pub const SECTIONS: IconName = "sections.png";
}

/// Static icon lookup table for hierarchical group paths
static ICON_MAP: LazyLock<HashMap<&'static str, IconName>> = LazyLock::new(|| {
    let mut m = HashMap::new();

    // Top-level groups (lowercase for case-insensitive matching)
    m.insert("drums", groups::DRUMS);
    m.insert("percussion", groups::PERCUSSION);
    m.insert("bass", groups::BASS);
    m.insert("guitars", groups::GUITARS);
    m.insert("keys", groups::KEYS);
    m.insert("synths", groups::SYNTHS);
    m.insert("horns", groups::HORNS);
    m.insert("harmonica", groups::HARMONICA);
    m.insert("vocals", groups::VOCALS);
    m.insert("choir", groups::CHOIR);
    m.insert("orchestra", groups::ORCHESTRA);
    m.insert("sfx", groups::SFX);
    m.insert("guide", groups::GUIDE);
    m.insert("reference", groups::REFERENCE);

    // Guitar sub-groups
    m.insert("guitars/electric", guitars::ELECTRIC);
    m.insert("guitars/acoustic", guitars::ACOUSTIC);
    m.insert("guitars/steel", guitars::STEEL);
    m.insert("guitars/banjo", guitars::BANJO);
    m.insert("electric guitar", guitars::ELECTRIC);
    m.insert("electric_guitar", guitars::ELECTRIC);
    m.insert("acoustic guitar", guitars::ACOUSTIC);
    m.insert("acoustic_guitar", guitars::ACOUSTIC);
    m.insert("steel guitar", guitars::STEEL);
    m.insert("steel_guitar", guitars::STEEL);

    // Vocal sub-groups
    m.insert("vocals/lead", vocals::LEAD);
    m.insert("vocals/background", vocals::BACKGROUND);
    m.insert("vocals/bgvs", vocals::BACKGROUND);
    m.insert("lead vocals", vocals::LEAD);
    m.insert("lead_vocals", vocals::LEAD);
    m.insert("background vocals", vocals::BACKGROUND);
    m.insert("background_vocals", vocals::BACKGROUND);
    m.insert("bgvs", vocals::BACKGROUND);
    m.insert("bgv", vocals::BACKGROUND);
    m.insert("bvs", vocals::BACKGROUND);

    // Drum sub-groups
    m.insert("drums/drum_kit", drums::DRUM_KIT);
    m.insert("drums/electronic", drums::ELECTRONIC);
    m.insert("drums/kick", drums::KICK);
    m.insert("drums/snare", drums::SNARE);
    m.insert("drums/tom", drums::TOM);
    m.insert("drums/cymbals", drums::CYMBALS);
    m.insert("drums/hi_hat", drums::HI_HAT);
    m.insert("drums/room", drums::ROOM);
    m.insert("drum kit", drums::DRUM_KIT);
    m.insert("electronic kit", drums::ELECTRONIC);
    m.insert("kick", drums::KICK);
    m.insert("snare", drums::SNARE);
    m.insert("tom", drums::TOM);
    m.insert("cymbals", drums::CYMBALS);
    m.insert("hi hat", drums::HI_HAT);
    m.insert("hi-hat", drums::HI_HAT);
    m.insert("hihat", drums::HI_HAT);
    m.insert("room", drums::ROOM);

    // Bass sub-groups
    m.insert("bass/guitar", bass::GUITAR);
    m.insert("bass/synth", bass::SYNTH);
    m.insert("bass/upright", bass::UPRIGHT);
    m.insert("bass guitar", bass::GUITAR);
    m.insert("bass_guitar", bass::GUITAR);
    m.insert("synth bass", bass::SYNTH);
    m.insert("synth_bass", bass::SYNTH);
    m.insert("upright bass", bass::UPRIGHT);
    m.insert("upright_bass", bass::UPRIGHT);

    // Keys sub-groups
    m.insert("keys/piano", keys::PIANO);
    m.insert("keys/electric", keys::ELECTRIC);
    m.insert("keys/organ", keys::ORGAN);
    m.insert("keys/harpsichord", keys::HARPSICHORD);
    m.insert("keys/clavichord", keys::CLAVICHORD);
    m.insert("piano", keys::PIANO);
    m.insert("electric keys", keys::ELECTRIC);
    m.insert("electric_keys", keys::ELECTRIC);
    m.insert("organ", keys::ORGAN);
    m.insert("harpsichord", keys::HARPSICHORD);
    m.insert("clavichord", keys::CLAVICHORD);

    // Synth sub-groups
    m.insert("synths/lead", synths::LEAD);
    m.insert("synths/pad", synths::PAD);
    m.insert("synths/arp", synths::ARP);
    m.insert("synths/chord", synths::CHORD);
    m.insert("synths/keys", synths::KEYS);
    m.insert("synths/fx", synths::FX);
    m.insert("lead synth", synths::LEAD);
    m.insert("pad", synths::PAD);
    m.insert("arp", synths::ARP);
    m.insert("synth arp", synths::ARP);

    // Orchestra sub-groups
    m.insert("orchestra/strings", orchestra::STRINGS);
    m.insert("orchestra/woodwinds", orchestra::WOODWINDS);
    m.insert("orchestra/brass", orchestra::BRASS);
    m.insert("orchestra/harp", orchestra::HARP);
    m.insert("orchestra/percussion", orchestra::PERCUSSION);
    m.insert("strings", orchestra::STRINGS);
    m.insert("woodwinds", orchestra::WOODWINDS);
    m.insert("brass", orchestra::BRASS);
    m.insert("harp", orchestra::HARP);

    // String section
    m.insert("violins", orchestra::strings::VIOLINS);
    m.insert("viola", orchestra::strings::VIOLA);
    m.insert("cello", orchestra::strings::CELLO);
    m.insert("contrabass", orchestra::strings::CONTRABASS);

    // Woodwind section
    m.insert("flutes", orchestra::woodwinds::FLUTES);
    m.insert("flute", orchestra::woodwinds::FLUTES);
    m.insert("oboes", orchestra::woodwinds::OBOES);
    m.insert("oboe", orchestra::woodwinds::OBOES);
    m.insert("clarinets", orchestra::woodwinds::CLARINETS);
    m.insert("clarinet", orchestra::woodwinds::CLARINETS);
    m.insert("bassoons", orchestra::woodwinds::BASSOONS);
    m.insert("bassoon", orchestra::woodwinds::BASSOONS);
    m.insert("piccolo", orchestra::woodwinds::PICCOLO);

    // Brass section
    m.insert("trumpets", orchestra::brass::TRUMPETS);
    m.insert("trumpet", orchestra::brass::TRUMPETS);
    m.insert("french horns", orchestra::brass::HORNS);
    m.insert("french horn", orchestra::brass::HORNS);
    m.insert("trombones", orchestra::brass::TROMBONES);
    m.insert("trombone", orchestra::brass::TROMBONES);
    m.insert("tuba", orchestra::brass::TUBA);

    // Guide/Click track types
    m.insert("click", guide::CLICK);
    m.insert("click track", guide::CLICK);
    m.insert("guide track", guide::GUIDE_TRACK);
    m.insert("cue", guide::CUE);
    m.insert("cue track", guide::CUE);
    m.insert("sections", guide::SECTIONS);

    m
});

/// Get the icon file name for a group by name or path
///
/// Supports various lookup formats:
/// - Top-level: "Drums", "guitars", "VOCALS"
/// - Path-based: "Guitars/Electric", "vocals/bgvs"
/// - Space-separated: "Electric Guitar", "Lead Vocals"
/// - Underscore-separated: "`electric_guitar`", "`lead_vocals`"
///
/// Returns `None` if no icon is defined for the given group.
///
/// # Example
/// ```
/// use dynamic_template::icons::icon_for_group;
///
/// assert_eq!(icon_for_group("Drums"), Some("drum.png"));
/// assert_eq!(icon_for_group("Electric Guitar"), Some("electric_guitar.png"));
/// ```
pub fn icon_for_group(group: &str) -> Option<IconName> {
    let normalized = group.to_lowercase();
    ICON_MAP.get(normalized.as_str()).copied()
}

/// Get the icon for a group by its classification path
///
/// The path should be a list of group names from root to leaf,
/// e.g., `["Guitars", "Electric"]` or `["Vocals", "Background"]`.
///
/// Tries to find the most specific icon first, falling back to parent icons.
///
/// # Example
/// ```
/// use dynamic_template::icons::icon_for_path;
///
/// // Returns Electric Guitar icon
/// assert_eq!(icon_for_path(&["Guitars", "Electric"]), Some("electric_guitar.png"));
///
/// // Falls back to Guitars icon for unknown sub-group
/// assert_eq!(icon_for_path(&["Guitars", "Unknown"]), Some("guitar.png"));
/// ```
pub fn icon_for_path(path: &[&str]) -> Option<IconName> {
    if path.is_empty() {
        return None;
    }

    // Try the full path first (joined with "/")
    let full_path = path.join("/").to_lowercase();
    if let Some(&icon) = ICON_MAP.get(full_path.as_str()) {
        return Some(icon);
    }

    // Try the last element (leaf group)
    let leaf = path.last()?.to_lowercase();
    if let Some(&icon) = ICON_MAP.get(leaf.as_str()) {
        return Some(icon);
    }

    // Fall back through parent groups
    for i in (0..path.len()).rev() {
        if let Some(slice) = path.get(..=i) {
            let parent_path = slice.join("/").to_lowercase();
            if let Some(&icon) = ICON_MAP.get(parent_path.as_str()) {
                return Some(icon);
            }
        }

        // Also try just the group name at this level
        if let Some(group_name_str) = path.get(i) {
            let group_name = group_name_str.to_lowercase();
            if let Some(&icon) = ICON_MAP.get(group_name.as_str()) {
                return Some(icon);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_icon_for_group_top_level() {
        assert_eq!(icon_for_group("Drums"), Some(groups::DRUMS));
        assert_eq!(icon_for_group("drums"), Some(groups::DRUMS));
        assert_eq!(icon_for_group("DRUMS"), Some(groups::DRUMS));
        assert_eq!(icon_for_group("Guitars"), Some(groups::GUITARS));
        assert_eq!(icon_for_group("Vocals"), Some(groups::VOCALS));
    }

    #[test]
    fn test_icon_for_group_subgroups() {
        assert_eq!(icon_for_group("Electric Guitar"), Some(guitars::ELECTRIC));
        assert_eq!(icon_for_group("electric_guitar"), Some(guitars::ELECTRIC));
        assert_eq!(icon_for_group("Acoustic Guitar"), Some(guitars::ACOUSTIC));
        assert_eq!(icon_for_group("BGVs"), Some(vocals::BACKGROUND));
        assert_eq!(icon_for_group("Lead Vocals"), Some(vocals::LEAD));
    }

    #[test]
    fn test_icon_for_path() {
        // Full path
        assert_eq!(
            icon_for_path(&["Guitars", "Electric"]),
            Some(guitars::ELECTRIC)
        );

        // Falls back to parent
        assert_eq!(
            icon_for_path(&["Guitars", "SomeUnknownType"]),
            Some(groups::GUITARS)
        );

        // Top level only
        assert_eq!(icon_for_path(&["Drums"]), Some(groups::DRUMS));

        // Empty path
        assert_eq!(icon_for_path(&[]), None);
    }
}
