//! EQ cheat-sheet data + track-name matching.
//!
//! Three kinds of guidance, unified behind `OverlaySource`:
//! - **Per-instrument** frequency zones (Hyperbits "EQ cheat sheet"), auto-selected
//!   from the track name (e.g. a track called "Kick In" → the kick profile).
//! - **General** sweet-spot chart (Hyperbits "Finding the EQ sweet spot") — one
//!   character triplet per band across the whole spectrum.
//! - **Mix-doctor** symptom centers (Kush / Gregory Scott) — overlapping zones
//!   keyed by a mix symptom (too slow / muddy / full / loud / hard / bright).
//!
//! Everything here is plain `const` data + small pure helpers, so it carries no
//! UI or audio dependencies and is trivially unit-testable.

/// Direction of a suggested move for a zone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoneDir {
    /// Usually too much here — reach for a cut.
    Cut,
    /// Usually wants a lift — boost.
    Boost,
    /// The desirable/“sweet” character of the band (neutral).
    Sweet,
    /// Narrow-Q sweep to find a resonance/ring, then cut.
    Sweep,
}

impl ZoneDir {
    /// RGBA tint for painting the zone (red = cut, green = boost, blue = sweet,
    /// amber = sweep).
    pub fn rgba(self) -> (u8, u8, u8, u8) {
        match self {
            ZoneDir::Cut => (235, 80, 90, 255),
            ZoneDir::Boost => (90, 200, 120, 255),
            ZoneDir::Sweet => (110, 160, 240, 255),
            ZoneDir::Sweep => (235, 180, 80, 255),
        }
    }
}

/// A frequency zone with a tonal role and a suggested direction.
#[derive(Clone, Copy, Debug)]
pub struct EqZone {
    pub lo_hz: f32,
    pub hi_hz: f32,
    /// Short label: "mud", "presence", "air", "knock", "honk", …
    pub role: &'static str,
    pub dir: ZoneDir,
}

const fn z(lo_hz: f32, hi_hz: f32, role: &'static str, dir: ZoneDir) -> EqZone {
    EqZone { lo_hz, hi_hz, role, dir }
}

/// A named EQ profile: the zones to show plus the track-name keywords that select
/// it. `highpass_hz` is a suggested HPF corner (drawn as a left edge marker).
#[derive(Clone, Copy, Debug)]
pub struct InstrumentProfile {
    pub name: &'static str,
    /// Lowercase substrings that match a track name to this profile, most
    /// specific first.
    pub aliases: &'static [&'static str],
    pub zones: &'static [EqZone],
    pub highpass_hz: Option<f32>,
}

// ── Per-instrument profiles ─────────────────────────────────────────────────
use ZoneDir::*;

pub const KICK_ACOUSTIC: InstrumentProfile = InstrumentProfile {
    name: "Kick (Acoustic)",
    aliases: &["kick in", "kick out", "kick", "kik", "bass drum", "bassdrum", "bd"],
    zones: &[
        z(40.0, 60.0, "rumble", Sweet),
        z(60.0, 145.0, "body / weight", Boost),
        z(250.0, 300.0, "mud / boxy", Cut),
        z(2000.0, 4000.0, "knock / attack", Boost),
        z(4000.0, 8000.0, "air / click", Boost),
    ],
    highpass_hz: Some(30.0),
};

pub const KICK_808: InstrumentProfile = InstrumentProfile {
    name: "Kick (808)",
    aliases: &["kick 808", "808 kick", "kick sub", "sub kick"],
    zones: &[
        z(20.0, 40.0, "low end", Sweet),
        z(50.0, 60.0, "bottom", Boost),
        z(100.0, 200.0, "body / smack", Boost),
        z(200.0, 500.0, "mud / boxy", Cut),
        z(2000.0, 4000.0, "knock / click", Boost),
    ],
    highpass_hz: Some(25.0),
};

pub const KICK_EDM: InstrumentProfile = InstrumentProfile {
    name: "Kick (EDM)",
    aliases: &["kick edm", "edm kick"],
    zones: &[
        z(20.0, 40.0, "low end", Sweet),
        z(40.0, 100.0, "energy", Sweet),
        z(100.0, 200.0, "body / punch", Boost),
        z(5000.0, 15000.0, "presence / click", Boost),
    ],
    highpass_hz: Some(30.0),
};

pub const SNARE: InstrumentProfile = InstrumentProfile {
    name: "Snare",
    aliases: &["snare top", "snare bottom", "snare", "snr", "sn"],
    zones: &[
        z(200.0, 400.0, "body", Sweet),
        z(250.0, 600.0, "ring", Sweep),
        z(2000.0, 4000.0, "smack / bang", Boost),
        z(6000.0, 10000.0, "air / definition", Boost),
    ],
    highpass_hz: Some(100.0),
};

pub const TOMS: InstrumentProfile = InstrumentProfile {
    name: "Toms",
    aliases: &["floor tom", "rack tom", "tom"],
    zones: &[
        z(100.0, 300.0, "body / thump", Boost),
        z(300.0, 500.0, "boom (excess)", Cut),
        z(3000.0, 5000.0, "attack / snap", Boost),
        z(5000.0, 12000.0, "air / presence", Sweet),
    ],
    highpass_hz: Some(40.0),
};

pub const CYMBALS: InstrumentProfile = InstrumentProfile {
    name: "Cymbals / OH",
    aliases: &["overhead", "overheads", "cymbal", "cymbals", "hihat", "hi-hat", "hat", "hh", "ride", "crash", "oh"],
    zones: &[
        z(200.0, 400.0, "clank / gong", Cut),
        z(6000.0, 15000.0, "brightness / air", Boost),
    ],
    highpass_hz: Some(150.0),
};

pub const BASS: InstrumentProfile = InstrumentProfile {
    name: "Bass",
    aliases: &["bass guitar", "bass synth", "sub bass", "808", "bass", "bs"],
    zones: &[
        z(80.0, 200.0, "body / girth", Sweet),
        z(250.0, 500.0, "mud", Cut),
        z(400.0, 800.0, "definition", Boost),
        z(1200.0, 1500.0, "attack", Sweet),
        z(2000.0, 5000.0, "string buzz", Cut),
    ],
    highpass_hz: Some(30.0),
};

pub const GUITAR_ELECTRIC: InstrumentProfile = InstrumentProfile {
    name: "Electric Guitar",
    aliases: &["electric guitar", "elec gtr", "egtr", "e gtr", "el gtr", "guitar", "gtr"],
    zones: &[
        z(150.0, 300.0, "body / thickness", Sweet),
        z(300.0, 1000.0, "character / power", Sweet),
        z(1000.0, 2000.0, "honk", Cut),
        z(3000.0, 10000.0, "presence / attack", Boost),
    ],
    highpass_hz: Some(80.0),
};

pub const GUITAR_ACOUSTIC: InstrumentProfile = InstrumentProfile {
    name: "Acoustic Guitar",
    aliases: &["acoustic guitar", "acoustic gtr", "ac gtr", "acgtr", "acoustic", "acou gtr"],
    zones: &[
        z(80.0, 400.0, "body / wood", Sweet),
        z(200.0, 400.0, "wood (excess)", Cut),
        z(500.0, 1000.0, "warmth / fullness", Sweet),
        z(1500.0, 2500.0, "definition", Boost),
        z(7000.0, 10000.0, "air / attack", Boost),
    ],
    highpass_hz: Some(70.0),
};

pub const PIANO: InstrumentProfile = InstrumentProfile {
    name: "Piano",
    aliases: &["grand piano", "electric piano", "epiano", "e piano", "rhodes", "keys", "piano", "pno"],
    zones: &[
        z(50.0, 250.0, "boom / warmth", Sweet),
        z(250.0, 500.0, "mud", Cut),
        z(800.0, 1000.0, "bark", Cut),
        z(3000.0, 5000.0, "presence", Boost),
        z(7000.0, 9000.0, "clarity", Boost),
        z(10000.0, 15000.0, "sharpness / sparkle", Sweet),
    ],
    highpass_hz: Some(40.0),
};

pub const STRINGS: InstrumentProfile = InstrumentProfile {
    name: "Strings",
    aliases: &["strings", "violin", "viola", "cello", "string"],
    zones: &[
        z(80.0, 300.0, "weight / warmth / mud", Cut),
        z(500.0, 1000.0, "attack", Sweet),
        z(2000.0, 5000.0, "string / air", Boost),
        z(7000.0, 12000.0, "sparkle / creak", Sweet),
    ],
    highpass_hz: Some(50.0),
};

pub const BRASS: InstrumentProfile = InstrumentProfile {
    name: "Brass",
    aliases: &["brass", "trumpet", "trombone", "sax", "saxophone", "horn", "horns", "tuba"],
    zones: &[
        z(200.0, 500.0, "fullness / mud", Cut),
        z(1000.0, 2000.0, "squawk / harsh", Cut),
        z(1000.0, 5000.0, "roundness", Sweet),
        z(5000.0, 10000.0, "definition / brightness", Boost),
    ],
    highpass_hz: Some(60.0),
};

pub const SYNTH_PAD: InstrumentProfile = InstrumentProfile {
    name: "Pad",
    aliases: &["pad", "pads", "atmos", "ambient"],
    zones: &[
        z(250.0, 450.0, "mud", Cut),
        z(400.0, 600.0, "thickness", Sweet),
    ],
    highpass_hz: Some(120.0),
};

pub const SYNTH_LEAD: InstrumentProfile = InstrumentProfile {
    name: "Synth Lead",
    // No bare "lead" — that's ambiguous with "Lead Vox" (handled by the vocal
    // profile). Match explicit synth-lead / pluck / arp names instead.
    aliases: &["synth lead", "lead synth", "pluck", "arp", "synth"],
    zones: &[
        z(160.0, 450.0, "mud", Cut),
        z(1000.0, 2000.0, "character", Sweet),
        z(2000.0, 3000.0, "presence", Boost),
        z(3000.0, 4000.0, "clarity", Boost),
        z(7000.0, 9000.0, "sharpness", Sweet),
    ],
    highpass_hz: Some(120.0),
};

pub const VOCAL_LEAD: InstrumentProfile = InstrumentProfile {
    name: "Lead Vocal",
    aliases: &["lead vocal", "lead vox", "lead voc", "main vocal", "vocal", "vox", "vocals", "voc"],
    zones: &[
        z(200.0, 500.0, "mud", Cut),
        z(800.0, 1500.0, "honk / nasal", Cut),
        z(2500.0, 4500.0, "presence", Boost),
        z(5000.0, 10000.0, "clarity", Boost),
        z(10000.0, 16000.0, "air", Boost),
    ],
    highpass_hz: Some(100.0),
};

pub const VOCAL_BACKING: InstrumentProfile = InstrumentProfile {
    name: "Backing Vocal",
    aliases: &["backing vocal", "background vocal", "backing vox", "bgv", "bv", "harmony", "harm"],
    zones: &[
        z(200.0, 500.0, "mud", Cut),
        z(800.0, 1500.0, "honk", Cut),
        z(2500.0, 4500.0, "presence (gentle)", Boost),
        z(5000.0, 10000.0, "clarity (gentle)", Boost),
    ],
    highpass_hz: Some(120.0),
};

pub const FX_IMPACT: InstrumentProfile = InstrumentProfile {
    name: "FX / Impact",
    aliases: &["impact", "boom", "fx", "riser", "sweep", "whoosh", "laser", "noise"],
    zones: &[
        z(100.0, 400.0, "mud", Cut),
        z(2000.0, 4000.0, "impact / bite", Boost),
        z(10000.0, 20000.0, "brightness", Sweet),
    ],
    highpass_hz: None,
};

/// General "sweet spot" chart (Hyperbits) — used as the fallback when no track
/// match is found. One character per band across the spectrum.
pub const GENERAL: InstrumentProfile = InstrumentProfile {
    name: "General",
    aliases: &[],
    zones: &[
        z(20.0, 50.0, "deep · rumbly/weak", Sweet),
        z(50.0, 100.0, "full · boomy/weak", Sweet),
        z(100.0, 200.0, "punchy · muddy/thin", Sweet),
        z(200.0, 400.0, "warm · muddy/thin", Sweet),
        z(400.0, 800.0, "warm · boxy/hollow", Sweet),
        z(800.0, 1500.0, "natural · honky/scooped", Sweet),
        z(1500.0, 2000.0, "edgy · harsh/distant", Sweet),
        z(2000.0, 5000.0, "present · harsh/distant", Sweet),
        z(5000.0, 10000.0, "crispy · piercing/dull", Sweet),
        z(10000.0, 20000.0, "airy · hissy/lifeless", Sweet),
    ],
    highpass_hz: None,
};

/// All selectable instrument profiles. Order matters: it's both the dropdown
/// order AND the match priority — more specific variants come before the generic
/// ones they'd otherwise be shadowed by (808/EDM before acoustic kick, acoustic
/// before electric guitar, backing before lead vocal). `GENERAL` is last and is
/// the no-match fallback.
pub const PROFILES: &[InstrumentProfile] = &[
    KICK_808,
    KICK_EDM,
    KICK_ACOUSTIC,
    SNARE,
    TOMS,
    CYMBALS,
    BASS,
    GUITAR_ACOUSTIC,
    GUITAR_ELECTRIC,
    PIANO,
    STRINGS,
    BRASS,
    SYNTH_PAD,
    SYNTH_LEAD,
    VOCAL_BACKING,
    VOCAL_LEAD,
    FX_IMPACT,
    GENERAL,
];

/// Match a track name to an instrument profile by alias keyword.
///
/// Case-insensitive substring match; the alias lists are ordered most-specific
/// first, and profiles are checked in [`PROFILES`] order so the more specific
/// kick/bass/vocal variants win before the generic ones. Returns `None` if
/// nothing matches (caller falls back to [`GENERAL`]).
pub fn match_track_name(track_name: &str) -> Option<&'static InstrumentProfile> {
    let name = track_name.to_ascii_lowercase();
    if name.trim().is_empty() {
        return None;
    }
    // Single scan in `PROFILES` priority order (specific variants first), so e.g.
    // "Kick 808" matches the 808 profile before the generic "kick" alias.
    for profile in PROFILES {
        if std::ptr::eq(profile, &GENERAL) {
            continue;
        }
        for alias in profile.aliases {
            if name.contains(alias) {
                return Some(profile);
            }
        }
    }
    None
}

/// Resolve a track name to the profile to display, falling back to `GENERAL`.
pub fn profile_for_track(track_name: &str) -> &'static InstrumentProfile {
    match_track_name(track_name).unwrap_or(&GENERAL)
}

// ── Track-info providers ────────────────────────────────────────────────────

/// Supplies the host track context the EQ instance is running on, so the
/// cheat-sheet overlay can auto-select a profile from the track name.
///
/// This is the swappable seam: the plugin provides a CLAP `track-info`-backed
/// implementation, the standalone app + tests use [`StaticTrackProvider`], and a
/// REAPER/VST3 backend can drop in later — all behind one trait, provided to the
/// UI via Dioxus context (`Arc<dyn TrackInfoProvider>`).
pub trait TrackInfoProvider: Send + Sync {
    /// Current track / channel name, or `None` if unknown (e.g. standalone with
    /// no host). Read fresh each frame so renames update the overlay.
    fn track_name(&self) -> Option<String>;
}

/// Fixed track name — for the standalone app and tests. `None` = no track.
#[derive(Clone, Default, Debug)]
pub struct StaticTrackProvider {
    pub name: Option<String>,
}

impl StaticTrackProvider {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: Some(name.into()) }
    }

    /// No track (overlay "Auto" resolves to off).
    pub fn none() -> Self {
        Self { name: None }
    }

    /// Read the track name from the `FTS_EQ_TRACK_NAME` env var — handy for
    /// testing overlays in the standalone without a host
    /// (`FTS_EQ_TRACK_NAME="Kick In" cargo run -p eq-standalone`).
    pub fn from_env() -> Self {
        Self { name: std::env::var("FTS_EQ_TRACK_NAME").ok().filter(|s| !s.is_empty()) }
    }
}

impl TrackInfoProvider for StaticTrackProvider {
    fn track_name(&self) -> Option<String> {
        self.name.clone()
    }
}

/// Resolve the "auto" overlay profile for a provider: the matched (or `GENERAL`)
/// profile if the provider knows the track name, else `None` (no track → off).
pub fn overlay_for(provider: &dyn TrackInfoProvider) -> Option<&'static InstrumentProfile> {
    provider.track_name().as_deref().map(profile_for_track)
}

// ── ISO-octave ear-training reference (Audio University method) ──────────────

/// One ISO-octave ear-training reference band: a center frequency, the
/// perceptual **cue** for recognizing it by ear (a vowel sound 250 Hz–4 kHz, a
/// haptic/body sensation below, sibilance above), and what **too much** vs
/// **too little** of it sounds like. These are the always-available "general
/// zones" shown on the graph by default, independent of any instrument profile.
#[derive(Debug, Clone, Copy)]
pub struct EarBand {
    /// ISO octave center frequency in Hz.
    pub center_hz: f32,
    /// Recognition cue — vowel ("ah"), haptic ("chest"), or sibilance ("s").
    pub cue: &'static str,
    /// One-word/phrase for what too much of this band sounds like.
    pub too_much: &'static str,
    /// …and what too little sounds like.
    pub too_little: &'static str,
}

/// The nine ISO octave centers from 63 Hz to 16 kHz with their ear-training cues.
/// 250 Hz–4 kHz map to vowels (ooh/oh/ah/a/ee); 63/125 to felt haptics;
/// 8 k/16 k to sibilance quality.
pub const EAR_BANDS: &[EarBand] = &[
    EarBand { center_hz: 63.0,    cue: "haptic · abdomen", too_much: "boom / rumble",    too_little: "no weight" },
    EarBand { center_hz: 125.0,   cue: "haptic · chest",   too_much: "boomy / tubby",    too_little: "thin" },
    EarBand { center_hz: 250.0,   cue: "ooh",              too_much: "muddy / boxy",     too_little: "thin, cold" },
    EarBand { center_hz: 500.0,   cue: "oh",               too_much: "boxy / cardboard", too_little: "hollow" },
    EarBand { center_hz: 1000.0,  cue: "ah",               too_much: "nasal / honky",    too_little: "no presence" },
    EarBand { center_hz: 2000.0,  cue: "a",                too_much: "harsh / edgy",     too_little: "dull, no attack" },
    EarBand { center_hz: 4000.0,  cue: "ee",               too_much: "harsh / piercing", too_little: "distant, no bite" },
    EarBand { center_hz: 8000.0,  cue: "s · sibilance",    too_much: "sibilant / brittle", too_little: "dull, no air" },
    EarBand { center_hz: 16000.0, cue: "s+t · sharp",      too_much: "hissy / harsh",    too_little: "dark, no sparkle" },
];

/// The ISO octave band a frequency falls in (within ±half an octave of a center),
/// for contextual hints (e.g. "this band sits in the 1 kHz 'ah' range").
pub fn ear_band_for(hz: f32) -> Option<&'static EarBand> {
    // Half an octave each side: center/√2 .. center*√2.
    const HALF_OCT: f32 = std::f32::consts::SQRT_2;
    EAR_BANDS
        .iter()
        .find(|b| hz >= b.center_hz / HALF_OCT && hz < b.center_hz * HALF_OCT)
}

// ── Too-much / too-little range descriptors ─────────────────────────────────

/// A frequency range with the commonly-agreed words for what **boosting too
/// much** vs **cutting too much / having too little** of it sounds like. These
/// are the consensus mixing-chart terms (Sound on Sound, iZotope, Audio Issues,
/// Unison, etc.) and render near the 0 dB line on the graph — `too_much` above
/// (the boost direction), `too_little` below (the cut direction).
#[derive(Debug, Clone, Copy)]
pub struct FreqRange {
    pub lo_hz: f32,
    pub hi_hz: f32,
    /// Short label for the range (for legends / tooltips).
    pub name: &'static str,
    /// What too much (boosted) of this range sounds like.
    pub too_much: &'static str,
    /// What too little (cut / absent) of this range sounds like.
    pub too_little: &'static str,
}

/// Ten consensus frequency ranges spanning the spectrum, low → high.
pub const FREQ_RANGES: &[FreqRange] = &[
    FreqRange { lo_hz: 20.0,    hi_hz: 40.0,    name: "rumble",     too_much: "rumble",   too_little: "no weight" },
    FreqRange { lo_hz: 40.0,    hi_hz: 100.0,   name: "bass",       too_much: "boomy",    too_little: "weak" },
    FreqRange { lo_hz: 100.0,   hi_hz: 250.0,   name: "warmth",     too_much: "muddy",    too_little: "thin" },
    FreqRange { lo_hz: 250.0,   hi_hz: 500.0,   name: "low mids",   too_much: "muddy",    too_little: "no body" },
    FreqRange { lo_hz: 500.0,   hi_hz: 1000.0,  name: "boxiness",   too_much: "boxy",     too_little: "hollow" },
    FreqRange { lo_hz: 1000.0,  hi_hz: 2000.0,  name: "nasal",      too_much: "nasal",    too_little: "no presence" },
    FreqRange { lo_hz: 2000.0,  hi_hz: 3000.0,  name: "crunch",     too_much: "crunch",   too_little: "dull" },
    FreqRange { lo_hz: 3000.0,  hi_hz: 4000.0,  name: "presence",   too_much: "presence", too_little: "veiled" },
    FreqRange { lo_hz: 4000.0,  hi_hz: 6000.0,  name: "pierce",     too_much: "pierce",   too_little: "no bite" },
    FreqRange { lo_hz: 6000.0,  hi_hz: 10000.0, name: "sibilance",  too_much: "sibilant", too_little: "muffled" },
    FreqRange { lo_hz: 10000.0, hi_hz: 20000.0, name: "air",        too_much: "brittle",  too_little: "dark" },
];

/// The descriptor range a frequency falls in.
pub fn freq_range_for(hz: f32) -> Option<&'static FreqRange> {
    FREQ_RANGES.iter().find(|r| hz >= r.lo_hz && hz < r.hi_hz)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kick_variants_and_disambiguation() {
        assert_eq!(profile_for_track("Kick In").name, "Kick (Acoustic)");
        assert_eq!(profile_for_track("KIK").name, "Kick (Acoustic)");
        assert_eq!(profile_for_track("Kick 808").name, "Kick (808)");
        // "808" alone reads as bass.
        assert_eq!(profile_for_track("808").name, "Bass");
    }

    #[test]
    fn snare_bass_guitar_vocals() {
        assert_eq!(profile_for_track("Snare Top").name, "Snare");
        assert_eq!(profile_for_track("Bass DI").name, "Bass");
        assert_eq!(profile_for_track("Lead Vox").name, "Lead Vocal");
        assert_eq!(profile_for_track("BGV 1").name, "Backing Vocal");
        assert_eq!(profile_for_track("Acoustic Gtr").name, "Acoustic Guitar");
        assert_eq!(profile_for_track("EGtr L").name, "Electric Guitar");
    }

    #[test]
    fn unknown_falls_back_to_general() {
        assert!(match_track_name("Bus 7").is_none());
        assert_eq!(profile_for_track("Bus 7").name, "General");
        assert_eq!(profile_for_track("").name, "General");
    }

    #[test]
    fn static_provider_resolves_overlay() {
        // A named track resolves to its matched profile (the swappable seam:
        // any backend behaves the same once it yields a name).
        let p = StaticTrackProvider::new("Kick In");
        assert_eq!(overlay_for(&p).map(|o| o.name), Some("Kick (Acoustic)"));

        // An unmatched name still gets the General fallback (some overlay).
        let p = StaticTrackProvider::new("Bus 7");
        assert_eq!(overlay_for(&p).map(|o| o.name), Some("General"));

        // No track name → no overlay (Auto resolves to off).
        let p = StaticTrackProvider::none();
        assert!(overlay_for(&p).is_none());
    }
}
