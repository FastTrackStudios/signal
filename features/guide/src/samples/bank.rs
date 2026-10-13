//! The engine's preloaded sample bank plus the legacy library-file mappings.
//!
//! Ported from fts-guide `samples/{click_loader,guide_loader}.rs`. The
//! REAPER-install path resolution (`$FTS_HOME`, env vars) is dropped: the
//! caller supplies base directories explicitly.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tracing::warn;

use super::{load_wav, AudioSample};

/// Click sound selection (legacy `ClickSound`, minus the nih-plug derive).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ClickSound {
    #[default]
    Blip,
    Classic,
    Cowbell,
    Digital,
    Gentle,
    Percussive,
    Saw,
    Woodblock,
}

/// Click sample file paths for a specific click sound.
///
/// Ported verbatim from the legacy `ClickSamplePaths`; the base path is now
/// a parameter instead of `$FTS_GUIDE_CLICK_PATH`.
#[derive(Debug, Clone)]
pub struct ClickSamplePaths {
    pub beat_path: PathBuf,
    pub eighth_path: PathBuf,
    pub sixteenth_path: PathBuf,
    pub accent_path: PathBuf,
}

impl ClickSamplePaths {
    /// Get file paths for a given click sound under `base_path`
    /// (the `Library/FTS-GUIDE/Click/` directory).
    #[must_use]
    pub fn for_sound(base_path: &Path, click_sound: ClickSound) -> Self {
        let (sound_dir, beat_file, eighth_file, sixteenth_file, accent_file) = match click_sound {
            ClickSound::Blip => (
                "Blip",
                "Blip-Quarter.wav",
                "Blip-Eighth.wav",
                "Blip-Sixteenth.wav",
                "Blip-Accent.wav",
            ),
            ClickSound::Classic => (
                "Classic",
                "Classic-Quarter.wav",
                "Classic-Eighth.wav",
                "Classic-Sixteenth.wav",
                "Classic-Accent.wav",
            ),
            ClickSound::Cowbell => (
                "Cowbell",
                "New Click -  Cowbell-quarter.wav",
                "New Click -  Cowbell-eighth.wav",
                "New Click -  Cowbell-sixteenth.wav",
                "New Click -  Cowbell-accents.wav",
            ),
            ClickSound::Digital => (
                "Digital",
                "New Click -  Digital-accents.wav",
                "New Click -  Digital-accents.wav",
                "New Click -  Digital-sixteenth.wav",
                "New Click -  Digital-accents.wav",
            ),
            ClickSound::Gentle => (
                "Gentle",
                "New Click -  Gentle-quarter.wav",
                "New Click -  Gentle-eighth.wav",
                "New Click -  Gentle-sixteenth.wav",
                "New Click -  Gentle-accents.wav",
            ),
            ClickSound::Percussive => (
                "Percussive",
                "New Click -  Percussive-quarter.wav",
                "New Click -  Percussive-eighth.wav",
                "New Click -  Percussive-sixteenth.wav",
                "New Click -  Percussive-accents.wav",
            ),
            ClickSound::Saw => (
                "Saw",
                "New Click -  Saw-quarter.wav",
                "New Click -  Saw-eighth.wav",
                "New Click -  Saw-quarter.wav",
                "New Click -  Saw-accents.wav",
            ),
            ClickSound::Woodblock => (
                "Woodblock",
                "New Click -  Woodblock-quarter.wav",
                "New Click -  Woodblock-eighth.wav",
                "New Click -  Woodblock-sixteenth.wav",
                "New Click -  Woodblock-accents.wav",
            ),
        };

        let dir = base_path.join(sound_dir);
        Self {
            beat_path: dir.join(beat_file),
            eighth_path: dir.join(eighth_file),
            sixteenth_path: dir.join(sixteenth_file),
            accent_path: dir.join(accent_file),
        }
    }
}

/// Map a section type name to a guide voice file name.
///
/// Ported verbatim from the legacy `section_to_guide_filename`.
/// Always uses the unnumbered filename (e.g. "Chorus", never "Chorus 1").
#[must_use]
pub fn section_to_guide_filename(
    section_type_name: &str,
    _section_number: Option<u32>,
) -> Option<String> {
    // Normalize section type name (capitalize first letter)
    let type_lower = section_type_name.to_lowercase();
    let type_capitalized = if let Some(first) = type_lower.chars().next() {
        first.to_uppercase().to_string() + type_lower.get(1..).unwrap_or("")
    } else {
        type_lower
    };

    let base_type = match type_capitalized.as_str() {
        "Verse" | "Vs" => "Verse",
        "Chorus" | "Ch" => "Chorus",
        "Bridge" | "Br" => "Bridge",
        "Intro" => "Intro",
        "Outro" => "Outro",
        "Instrumental" => "Instrumental",
        // Capitalizing lowercased the rest, so a hyphenated name arrives
        // as `Pre-chorus`.
        "Pre-Chorus" | "Pre-chorus" | "Pre Chorus" | "Pre chorus" => "Pre Chorus",
        "Post-Chorus" | "Post-chorus" | "Post Chorus" | "Post chorus" => "Post Chorus",
        "Breakdown" => "Breakdown",
        "Interlude" => "Interlude",
        "Tag" => "Tag",
        "Ending" => "Ending",
        "Solo" => "Solo",
        "Vamp" => "Vamp",
        "Turnaround" => "Turnaround",
        "Refrain" => "Refrain",
        "Rap" => "Rap",
        "Acapella" => "Acapella",
        "Exhortation" => "Exhortation",
        _ => return None, // Unknown section type
    };

    Some(format!("English Female - {base_type}.wav"))
}

/// Get guide sample key for a section: `"{section_type}_{number}"`.
///
/// Ported verbatim from the legacy `get_guide_key`.
#[must_use]
pub fn get_guide_key(section_type_name: &str, section_number: Option<u32>) -> String {
    let number_str = section_number.map_or_else(|| "None".to_string(), |n| n.to_string());
    format!("{}_{number_str}", sample_name(section_type_name))
}

/// A section's name as the sample library spells it: words separated by
/// spaces (`Pre-Chorus` is the file `Pre Chorus`), and `End` is the
/// library's `Ending`.
pub(super) fn sample_name(section_type_name: &str) -> String {
    let spaced = section_type_name.replace('-', " ");
    if spaced.eq_ignore_ascii_case("end") {
        "Ending".to_string()
    } else {
        spaced
    }
}

/// Convert a guide filename to its lookup key.
///
/// Ported verbatim from the legacy `GuideSampleLoader::filename_to_key`.
pub(super) fn filename_to_key(filename: &str) -> String {
    let without_prefix = filename
        .strip_prefix("English Female - ")
        .unwrap_or(filename);
    let without_ext = without_prefix
        .strip_suffix(".wav")
        .unwrap_or(without_prefix);

    // Only a trailing NUMBER is a section number: "Verse 2" is Verse #2,
    // but "Pre Chorus" is one two-word name. The legacy port split any
    // two words as type + number, so "Pre Chorus" became type "Pre" and
    // the Pre-Chorus cue never found its sample.
    match without_ext.rsplit_once(' ') {
        Some((name, number)) if number.chars().all(|c| c.is_ascii_digit()) => {
            format!("{name}_{number}")
        }
        _ if without_ext.is_empty() => "None_None".to_string(),
        _ => format!("{without_ext}_None"),
    }
}

/// All PCM the engine plays: click subdivisions, count voices 1-8, and
/// guide announcements keyed by `"{type}_{number}"` (or `"tts:{text}"` for
/// TTS-rendered cues).
#[derive(Debug, Clone, Default)]
pub struct SampleBank {
    pub beat: Option<AudioSample>,
    pub eighth: Option<AudioSample>,
    pub sixteenth: Option<AudioSample>,
    pub triplet: Option<AudioSample>,
    pub measure_accent: Option<AudioSample>,
    /// Count voices: index 0 speaks "1", index 7 speaks "8".
    pub counts: [Option<AudioSample>; 8],
    /// Guide announcements by key.
    pub guides: HashMap<String, AudioSample>,
}

impl SampleBank {
    /// Load click samples for `click_sound` from the click library directory.
    ///
    /// Missing files are logged and skipped (legacy behavior). The triplet
    /// slot falls back to the eighth sample, as in the legacy loader.
    pub fn load_click(
        &mut self,
        click_base_path: &Path,
        click_sound: ClickSound,
        sample_rate: u32,
    ) {
        let paths = ClickSamplePaths::for_sound(click_base_path, click_sound);
        let mut load = |path: &Path| match load_wav(path, sample_rate) {
            Ok(audio) => Some(audio),
            Err(e) => {
                warn!(path = %path.display(), error = %e, "Failed to load click sample");
                None
            }
        };
        self.beat = load(&paths.beat_path);
        self.eighth = load(&paths.eighth_path);
        self.sixteenth = load(&paths.sixteenth_path);
        // Triplet sample (use eighth as fallback — legacy behavior)
        self.triplet = self.eighth.clone();
        self.measure_accent = load(&paths.accent_path);
    }

    /// Re-voice the loaded click so the beat leads: on-beats take the
    /// kit's higher tick, off-beat eighths its lower one, and the bar's
    /// first beat plays the same on-beat tick rather than the kit's
    /// (much higher) accent.
    ///
    /// The FTS-GUIDE kits name their files by legacy slot, not pitch; in
    /// Cowbell the `eighth` tick (~2.2 kHz) sits above the `quarter` one
    /// (~1.8 kHz) and `accents` far above both (~4.5 kHz, measured) — so
    /// as loaded, beats were the low tick and off-beats the high one.
    pub fn beats_high_offbeats_low(&mut self) {
        let (quarter, eighth) = (self.beat.take(), self.eighth.take());
        self.beat = eighth.or_else(|| quarter.clone());
        self.eighth = quarter;
        self.triplet = self.eighth.clone();
        self.measure_accent = self.beat.clone();
    }

    /// Load count voices 1-8 from the counts directory
    /// (`"{voice_prefix} - {n}.wav"`, legacy prefix `"English Female"`).
    pub fn load_counts(&mut self, counts_base_path: &Path, voice_prefix: &str, sample_rate: u32) {
        for (i, slot) in self.counts.iter_mut().enumerate() {
            let count_num = i.saturating_add(1);
            let path = counts_base_path.join(format!("{voice_prefix} - {count_num}.wav"));
            match load_wav(&path, sample_rate) {
                Ok(audio) => *slot = Some(audio),
                Err(e) => {
                    warn!(path = %path.display(), error = %e, "Failed to load count sample");
                }
            }
        }
    }

    /// Load every `.wav` in the section-guide directory, keyed via the
    /// legacy filename convention (`"English Female - Chorus 2.wav"` →
    /// `"Chorus_2"`).
    pub fn load_guide_dir(&mut self, guide_base_path: &Path, sample_rate: u32) {
        let entries = match std::fs::read_dir(guide_base_path) {
            Ok(entries) => entries,
            Err(e) => {
                warn!(path = %guide_base_path.display(), error = %e, "Failed to read guide directory");
                return;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("wav") {
                continue;
            }
            let Some(filename) = path.file_name().and_then(|f| f.to_str()) else {
                continue;
            };
            match load_wav(&path, sample_rate) {
                Ok(audio) => {
                    self.guides.insert(filename_to_key(filename), audio);
                }
                Err(e) => {
                    warn!(path = %path.display(), error = %e, "Failed to load guide sample");
                }
            }
        }
    }

    /// Insert a guide announcement under an explicit key
    /// (used by the TTS `CueBank` with `"tts:{text}"` keys).
    pub fn insert_guide(&mut self, key: impl Into<String>, sample: AudioSample) {
        self.guides.insert(key.into(), sample);
    }

    /// Fill every EMPTY slot with a synthesized placeholder so the guide
    /// is audible with zero assets on disk. Loaded samples are never
    /// overwritten — call the `load_*` methods first, then this.
    ///
    /// Design (all short exponentially-decaying sine bursts, peak < 1.0):
    /// - click: 1 kHz tick for beats (30 ms), 1.5 kHz accent (35 ms),
    ///   quieter/shorter ticks for eighths, sixteenths and triplets;
    /// - count voices 1-8: a distinct pitch per count (C-major scale
    ///   C5..C6, ~110 ms) so count-ins are followable without speech;
    /// - section guides: a two-tone chime inserted under the standard
    ///   `"{type}_{number}"` keys for every known section type (numbers
    ///   `None` and 1..=8), so section cues sound even with no voice
    ///   library and no TTS.
    pub fn synthesize_defaults(&mut self, sample_rate: u32) {
        if self.beat.is_none() {
            self.beat = Some(synth_tick(1_000.0, 30.0, 0.5, sample_rate));
        }
        if self.measure_accent.is_none() {
            self.measure_accent = Some(synth_tick(1_500.0, 35.0, 0.65, sample_rate));
        }
        if self.eighth.is_none() {
            self.eighth = Some(synth_tick(1_000.0, 18.0, 0.35, sample_rate));
        }
        if self.sixteenth.is_none() {
            self.sixteenth = Some(synth_tick(1_000.0, 12.0, 0.25, sample_rate));
        }
        if self.triplet.is_none() {
            self.triplet = Some(synth_tick(1_200.0, 18.0, 0.35, sample_rate));
        }

        // Count voices: a clear, CONSISTENT count tick rather than an
        // ascending scale (spoken "1, 2, 3, 4" needs a TTS voice or real
        // Count samples). The downbeat ("1", index 0) is accented higher so
        // the top of the count-in stands out; the rest are one steady tone —
        // a recognizable count-in, not a melodic run of random beeps.
        for (i, slot) in self.counts.iter_mut().enumerate() {
            if slot.is_none() {
                let (freq, gain) = if i == 0 {
                    (1_760.0, 0.6)
                } else {
                    (1_320.0, 0.5)
                };
                *slot = Some(synth_tick(freq, 55.0, gain, sample_rate));
            }
        }

        // No section-guide fallback here: guide announcements come from real
        // recorded samples (loaded via `load_guide_dir`) or TTS. Synthesizing
        // a chime for every section type just produced noise when neither was
        // present, so an unmatched section stays silent instead.
    }
}

/// A short exponentially-decaying sine burst (hard attack, tau = dur/6).
fn synth_tick(freq_hz: f32, dur_ms: f32, gain: f32, sample_rate: u32) -> AudioSample {
    let sr = f64::from(sample_rate.max(1));
    // Calculate frame count with explicit bounds checking to avoid unsafe casts
    let frames_f64 = ((f64::from(dur_ms) / 1000.0) * sr).round().max(1.0);
    let frames: usize = crate::cast::usize_from_f64_round(frames_f64).max(1);
    let tau = (dur_ms / 1000.0) / 6.0;
    let samples = (0..frames)
        .map(|i| {
            // Convert loop index to u32 first to avoid precision loss in f64 conversion
            let i_u32 = u32::try_from(i).unwrap_or(0);
            let t_seconds = f64::from(i_u32) / sr;
            // Clamp to f32 range before conversion to handle precision loss explicitly
            let t: f32 = crate::cast::f32_from_f64_saturating(t_seconds);
            gain * (std::f32::consts::TAU * freq_hz * t).sin() * (-t / tau).exp()
        })
        .collect();
    AudioSample::mono(samples, sample_rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filename_to_key_variants() {
        assert_eq!(filename_to_key("English Female - Intro.wav"), "Intro_None");
        assert_eq!(filename_to_key("English Female - Verse 1.wav"), "Verse_1");
        assert_eq!(
            filename_to_key("English Female - Pre Chorus 2.wav"),
            "Pre Chorus_2"
        );
    }

    #[test]
    fn guide_filename_mapping() {
        assert_eq!(
            section_to_guide_filename("Chorus", Some(2)),
            Some("English Female - Chorus.wav".to_string())
        );
        assert_eq!(
            section_to_guide_filename("pre chorus", None),
            Some("English Female - Pre Chorus.wav".to_string())
        );
        assert_eq!(
            section_to_guide_filename("Pre-Chorus", None),
            Some("English Female - Pre Chorus.wav".to_string())
        );
        assert_eq!(section_to_guide_filename("Zebra", None), None);
    }
}

#[cfg(test)]
mod key_tests {
    use super::{filename_to_key, get_guide_key};

    #[test]
    fn two_word_sections_are_one_name_and_only_numbers_number() {
        assert_eq!(
            filename_to_key("English Female - Pre Chorus.wav"),
            "Pre Chorus_None"
        );
        assert_eq!(
            filename_to_key("English Female - Pre Chorus 2.wav"),
            "Pre Chorus_2"
        );
        assert_eq!(filename_to_key("English Female - Verse 3.wav"), "Verse_3");
        assert_eq!(filename_to_key("English Female - Tag.wav"), "Tag_None");
    }

    #[test]
    fn a_cues_key_finds_the_librarys_file() {
        assert_eq!(
            get_guide_key("Pre-Chorus", None),
            filename_to_key("English Female - Pre Chorus.wav")
        );
        assert_eq!(
            get_guide_key("Post-Chorus", None),
            filename_to_key("English Female - Post Chorus.wav")
        );
        assert_eq!(
            get_guide_key("End", None),
            filename_to_key("English Female - Ending.wav")
        );
        assert_eq!(
            get_guide_key("Tag", None),
            filename_to_key("English Female - Tag.wav")
        );
    }
}
