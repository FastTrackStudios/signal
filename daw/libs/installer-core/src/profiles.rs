//! FTS rig profile definitions — the different REAPER wrapper configurations
//! available for installation.

/// A rig profile that can be installed as a wrapper .app bundle.
#[derive(Debug, Clone, PartialEq)]
pub struct RigProfile {
    /// Internal identifier (e.g. "guitar", "drums").
    pub id: &'static str,
    /// Display name shown in the installer UI (e.g. "FTS-GUITAR").
    pub display_name: &'static str,
    /// Short description of what this profile is for.
    pub description: &'static str,
    /// Icon filename (e.g. "guitar.png") — relative to the icons asset dir.
    pub icon: &'static str,
}

/// All available rig profiles.
pub const ALL_PROFILES: &[RigProfile] = &[
    RigProfile {
        id: "reaper",
        display_name: "FTS-REAPER",
        description: "General purpose REAPER with FTS extensions",
        icon: "reaper.png",
    },
    RigProfile {
        id: "live",
        display_name: "FTS-LIVE",
        description: "Live performance and playback",
        icon: "live.png",
    },
    RigProfile {
        id: "guitar",
        display_name: "FTS-GUITAR",
        description: "Guitar rig and amp modeling",
        icon: "guitar.png",
    },
    RigProfile {
        id: "bass",
        display_name: "FTS-BASS",
        description: "Bass guitar processing",
        icon: "bass.png",
    },
    RigProfile {
        id: "keys",
        display_name: "FTS-KEYS",
        description: "Keyboard and synth workflows",
        icon: "keys.png",
    },
    RigProfile {
        id: "drums",
        display_name: "FTS-DRUMS",
        description: "Drum recording and editing",
        icon: "drums.png",
    },
    RigProfile {
        id: "drum-enhancement",
        display_name: "FTS-DRUM-ENHANCE",
        description: "Drum replacement and enhancement",
        icon: "drum-enhancement.png",
    },
    RigProfile {
        id: "vocals",
        display_name: "FTS-VOCALS",
        description: "Vocal recording and processing",
        icon: "vocals.png",
    },
    RigProfile {
        id: "session",
        display_name: "FTS-TRACKS",
        description: "Session tracking and mixing",
        icon: "session.png",
    },
];
