//! Melody notation system
//!
//! Provides a system for notating melodic lines with pitches and rhythms.
//!
//! Syntax:
//! - `m{ C8 D8 E4 }` - inline melody block
//! - `m{ C_8 D_8 E_4 }` - inline melody block (legacy underscore form)
//! - `mainRiff = m{ C8 D8 E4 }` - variable assignment
//! - `$mainRiff` - variable reference
//!
//! Note format: `<pitch><modifiers><duration>` or `<pitch><modifiers>_<duration>`
//! - Pitch: C, D, E, F, G, A, B (with optional # or b) OR scale degrees 1-7
//! - Octave modifiers: `'` to jump up an octave, `,` to drop down an octave
//! - Octave: explicit absolute octave via `:` (e.g. `C:4`), overriding relative
//!   mode. The legacy bare-digit underscore form (`C5_4`) is still accepted.
//! - Duration: Lilypond-style (4 = quarter, 8 = eighth, 16 = sixteenth, etc.)
//! - Triplets: trailing `t` (e.g. `4t`, `8t`)
//!
//! Relative mode (default): Each note chooses the closest octave to the previous note.
//! Use `'` or `,` to force octave jumps when the closest isn't what you want.
//!
//! Examples:
//! - `C4` - C quarter note (relative to previous)
//! - `C_4` - C quarter note (legacy underscore form)
//! - `D#'8` - D# eighth note, one octave up from closest
//! - `Bb,2` - Bb half note, one octave down from closest
//! - `C:48` - C in octave 4, eighth note (explicit octave via `:`)
//! - `C5_4` - C5 quarter note (legacy explicit octave)
//! - `3_8` - Scale degree 3 eighth note
//! - `r4` - quarter rest

use crate::time::AbsolutePosition;
use facet::Facet;
use std::collections::HashMap;
use std::fmt;

/// Pitch class values for interval calculation (C=0, D=2, E=4, F=5, G=7, A=9, B=11)
const PITCH_CLASS: [(&str, i8); 7] = [
    ("C", 0),
    ("D", 2),
    ("E", 4),
    ("F", 5),
    ("G", 7),
    ("A", 9),
    ("B", 11),
];

/// Scale degree to pitch letter mapping (1=C, 2=D, etc. in C major)
const SCALE_DEGREES: [&str; 7] = ["C", "D", "E", "F", "G", "A", "B"];

/// Octave modifier for relative pitch calculation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet, Default)]
#[repr(u8)]
pub enum OctaveModifier {
    /// No modifier - use closest octave (relative mode)
    #[default]
    None,
    /// `'` - force one octave up from closest
    Up,
    /// `,` - force one octave down from closest
    Down,
    /// Multiple `'` marks.
    UpBy(u8),
    /// Multiple `,` marks.
    DownBy(u8),
}

/// A single note in a melody
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct MelodyNote {
    /// The pitch name (C, D, E, F, G, A, B) with optional accidental
    /// "r" for rest, "s" for invisible space
    pub pitch: String,

    /// Explicit octave number (None = use relative mode)
    pub octave: Option<u8>,

    /// Octave modifier for relative mode (`'` or `,`)
    pub octave_modifier: OctaveModifier,

    /// Duration as a Lilypond-style value (4 = quarter, 8 = eighth, etc.)
    pub duration: u8,

    /// Is this note dotted?
    pub dotted: bool,

    /// Is this a triplet duration?
    pub triplet: bool,

    /// Original scale degree if parsed from number (1-7)
    pub scale_degree: Option<u8>,

    /// Position in the song (set during position calculation)
    pub position: Option<AbsolutePosition>,

    /// Additional pitches that share this note's stem — e.g. octave
    /// doublings or chord-notes from a `<note><chord>` MusicXML
    /// continuation. Each entry is `(pitch_name, octave)`; rendered as
    /// extra noteheads stacked on the same stem.
    #[facet(default)]
    pub extra_pitches: Vec<(String, Option<u8>)>,

    /// Relative octave modifiers for `extra_pitches`. Kept parallel to
    /// `extra_pitches`; missing entries default to `OctaveModifier::None`.
    #[facet(default)]
    pub extra_pitch_modifiers: Vec<OctaveModifier>,

    /// True when this note is tied INTO the next note (`<tie type="start"/>`).
    #[facet(default)]
    pub tie_start: bool,
    /// True when this note is tied FROM the previous note (`<tie type="stop"/>`).
    #[facet(default)]
    pub tie_stop: bool,

    /// The lyric syllable sung on this note (karaoke / vocal lanes). `None`
    /// means no syllable here — either an instrumental note or a melisma
    /// continuation (the previous syllable is held across this note).
    #[facet(default)]
    pub lyric: Option<String>,
}

impl MelodyNote {
    /// Create a new melody note
    pub fn new(pitch: impl Into<String>, duration: u8) -> Self {
        Self {
            pitch: pitch.into(),
            octave: None,
            octave_modifier: OctaveModifier::None,
            duration,
            dotted: false,
            triplet: false,
            scale_degree: None,
            position: None,
            extra_pitches: Vec::new(),
            extra_pitch_modifiers: Vec::new(),
            tie_start: false,
            tie_stop: false,
            lyric: None,
        }
    }

    /// Create a rest
    pub fn rest(duration: u8) -> Self {
        Self {
            pitch: "r".to_string(),
            octave: None,
            octave_modifier: OctaveModifier::None,
            duration,
            dotted: false,
            triplet: false,
            scale_degree: None,
            position: None,
            extra_pitches: Vec::new(),
            extra_pitch_modifiers: Vec::new(),
            tie_start: false,
            tie_stop: false,
            lyric: None,
        }
    }

    /// Set the octave
    pub fn with_octave(mut self, octave: u8) -> Self {
        self.octave = Some(octave);
        self
    }

    /// Attach a lyric syllable to this note.
    pub fn with_lyric(mut self, lyric: impl Into<String>) -> Self {
        self.lyric = Some(lyric.into());
        self
    }

    /// Make this note dotted
    pub fn dotted(mut self) -> Self {
        self.dotted = true;
        self
    }

    /// Check if this is a rest
    pub fn is_rest(&self) -> bool {
        self.pitch == "r"
    }

    /// Check if this is an invisible space placeholder
    pub fn is_space(&self) -> bool {
        self.pitch == "s"
    }

    /// Set the octave modifier
    pub fn with_octave_modifier(mut self, modifier: OctaveModifier) -> Self {
        self.octave_modifier = modifier;
        self
    }

    /// Get the pitch class (0-11) for this note, ignoring octave
    /// Returns None for rests
    pub fn pitch_class(&self) -> Option<i8> {
        if self.is_rest() || self.is_space() {
            return None;
        }

        let base_pitch = self.pitch.chars().next()?;
        let base_class = PITCH_CLASS
            .iter()
            .find(|(p, _)| p.starts_with(base_pitch))
            .map(|(_, c)| *c)?;

        // Apply accidentals
        let accidental_offset = if self.pitch.contains('#') {
            1
        } else if self.pitch.contains('b') {
            -1
        } else {
            0
        };

        Some((base_class + accidental_offset).rem_euclid(12))
    }

    /// Resolve the absolute octave for this note given a reference pitch
    ///
    /// In relative mode (no explicit octave), chooses the closest octave to the reference.
    /// Applies octave modifiers (`'` or `,`) after finding closest.
    ///
    /// # Arguments
    /// * `ref_pitch_class` - Pitch class of the reference note (0-11)
    /// * `ref_octave` - Octave of the reference note
    ///
    /// # Returns
    /// The resolved octave for this note
    pub fn resolve_octave(&self, ref_pitch_class: i8, ref_octave: u8) -> u8 {
        // If explicit octave is set, use it (ignores modifiers)
        if let Some(oct) = self.octave {
            return oct;
        }

        let Some(this_class) = self.pitch_class() else {
            return ref_octave; // Rest, keep same octave context
        };

        let ref_abs = absolute_pitch_value(ref_pitch_class, ref_octave);
        let base_octave = ref_octave
            .saturating_sub(1)
            .saturating_add(0)
            .saturating_sub(0);
        let candidates = [
            ref_octave.saturating_sub(1),
            ref_octave,
            ref_octave.saturating_add(1),
        ];
        let base_octave = candidates
            .into_iter()
            .min_by_key(|octave| {
                let abs = absolute_pitch_value(this_class, *octave);
                (
                    (abs - ref_abs).abs(),
                    (i16::from(*octave) - i16::from(ref_octave)).abs(),
                )
            })
            .unwrap_or(base_octave);

        // Apply octave modifier
        match self.octave_modifier {
            OctaveModifier::None => base_octave,
            OctaveModifier::Up => base_octave.saturating_add(1),
            OctaveModifier::Down => base_octave.saturating_sub(1),
            OctaveModifier::UpBy(count) => base_octave.saturating_add(count),
            OctaveModifier::DownBy(count) => base_octave.saturating_sub(count),
        }
    }

    /// Parse a melody note from a string
    ///
    /// Format: `<pitch><modifiers><duration>` or `<pitch><modifiers>_<duration>`
    ///
    /// Pitch can be:
    /// - Letter: C, D, E, F, G, A, B (case-insensitive)
    /// - With accidental: C#, Db, F#, Bb, etc.
    /// - Scale degree: 1, 2, 3, 4, 5, 6, 7
    ///
    /// Modifiers (after pitch, before underscore):
    /// - `'` - octave up
    /// - `,` - octave down
    /// - Number - explicit octave (e.g., C5, D#4)
    ///
    /// Examples:
    /// - `C_4` - C quarter note (relative mode)
    /// - `D#'_8` - D# eighth note, one octave up
    /// - `Bb,_2.` - Bb dotted half note, one octave down
    /// - `C5_4` - C5 quarter note (explicit octave)
    /// - `3_8` - Scale degree 3 eighth note
    /// - `r_4` - quarter rest
    pub fn parse(s: &str) -> Result<Self, String> {
        let mut s = s.trim();

        if s.is_empty() {
            return Err("Empty note string".to_string());
        }

        let tie_stop = s.starts_with('~');
        if tie_stop {
            s = s[1..].trim_start();
        }
        let tie_start = s.ends_with('~');
        if tie_start {
            s = s[..s.len() - 1].trim_end();
        }

        let (pitch_part, duration, dotted, triplet, _underscore_form) =
            Self::split_pitch_and_duration(s)?;

        if let Some((group_modifier, inner)) = split_chord_note_group_pitch_part(pitch_part) {
            let mut pitches = split_melody_tokens(inner);
            let first = pitches
                .next()
                .ok_or_else(|| format!("Empty chord-note group: {}", s))?;
            let mut primary = Self::parse(&format!("{first}{duration}"))?;
            primary.octave_modifier =
                combine_octave_modifiers(primary.octave_modifier, group_modifier);
            primary.dotted = dotted;
            primary.triplet = triplet;
            for pitch in pitches {
                let mut extra = Self::parse(&format!("{pitch}{duration}"))?;
                extra.octave_modifier =
                    combine_octave_modifiers(extra.octave_modifier, group_modifier);
                primary.extra_pitches.push((extra.pitch, extra.octave));
                primary.extra_pitch_modifiers.push(extra.octave_modifier);
            }
            primary.tie_start = tie_start;
            primary.tie_stop = tie_stop;
            return Ok(primary);
        }

        // Check for rest or invisible space
        if matches!(pitch_part, "r" | "R" | "s" | "S") {
            return Ok(MelodyNote {
                pitch: pitch_part.to_ascii_lowercase(),
                octave: None,
                octave_modifier: OctaveModifier::None,
                duration,
                dotted,
                triplet,
                scale_degree: None,
                position: None,
                extra_pitches: Vec::new(),
                extra_pitch_modifiers: Vec::new(),
                tie_start,
                tie_stop,
                lyric: None,
            });
        }

        let mut chars = pitch_part.chars().peekable();
        let leading_octave_modifier = Self::parse_octave_modifier(&mut chars);
        let first_char = *chars.peek().ok_or("Missing pitch")?;

        // Check if it's a scale degree (starts with 1-7)
        if first_char.is_ascii_digit() {
            let degree: u8 = chars
                .next()
                .unwrap()
                .to_digit(10)
                .ok_or("Invalid scale degree")? as u8;

            if !(1..=7).contains(&degree) {
                return Err(format!("Scale degree must be 1-7, got {}", degree));
            }

            // Convert to pitch letter
            let pitch = SCALE_DEGREES[(degree - 1) as usize].to_string();

            // Check for accidental after the number
            let mut final_pitch = pitch;
            if let Some(&c) = chars.peek()
                && (c == '#' || c == 'b')
            {
                final_pitch.push(chars.next().unwrap());
            }

            // Check for octave modifier
            let parsed_modifier = Self::parse_octave_modifier(&mut chars);
            let octave_modifier = if matches!(leading_octave_modifier, OctaveModifier::None) {
                parsed_modifier
            } else {
                leading_octave_modifier
            };

            // Check for explicit octave
            let octave = Self::parse_explicit_octave(&mut chars)?;

            return Ok(MelodyNote {
                pitch: final_pitch,
                octave,
                octave_modifier,
                duration,
                dotted,
                triplet,
                scale_degree: Some(degree),
                position: None,
                extra_pitches: Vec::new(),
                extra_pitch_modifiers: Vec::new(),
                tie_start,
                tie_stop,
                lyric: None,
            });
        }

        // Parse pitch letter
        let pitch_letter = chars
            .next()
            .ok_or("Missing pitch letter")?
            .to_uppercase()
            .to_string();

        if !matches!(
            pitch_letter.as_str(),
            "C" | "D" | "E" | "F" | "G" | "A" | "B"
        ) {
            return Err(format!("Invalid pitch letter: {}", pitch_letter));
        }

        // Check for accidental (# = sharp, b = flat, n = natural)
        let mut pitch = pitch_letter;
        if let Some(&c) = chars.peek()
            && (c == '#' || c == 'b' || c == 'n')
        {
            pitch.push(chars.next().unwrap());
        }

        // Check for octave modifier
        let parsed_modifier = Self::parse_octave_modifier(&mut chars);
        let octave_modifier = if matches!(leading_octave_modifier, OctaveModifier::None) {
            parsed_modifier
        } else {
            leading_octave_modifier
        };

        // Check for explicit octave
        let octave = Self::parse_explicit_octave(&mut chars)?;

        Ok(MelodyNote {
            pitch,
            octave,
            octave_modifier,
            duration,
            dotted,
            triplet,
            scale_degree: None,
            position: None,
            extra_pitches: Vec::new(),
            extra_pitch_modifiers: Vec::new(),
            tie_start,
            tie_stop,
            lyric: None,
        })
    }

    fn split_pitch_and_duration(s: &str) -> Result<(&str, u8, bool, bool, bool), String> {
        if let Some((pitch_part, duration_part)) = s.split_once('_') {
            let (duration, dotted, triplet) = Self::parse_duration_part(duration_part)?;
            return Ok((pitch_part, duration, dotted, triplet, true));
        }

        // A `:`-introduced octave takes a single digit (`C:4`); a duration, when
        // present, follows it with no separator (`C:48` = octave 4, eighth). The
        // octave digit therefore belongs to the pitch, not the duration.
        if let Some(colon) = melody_octave_colon(s) {
            let oct_digit = colon + 1;
            if s[oct_digit..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit())
            {
                let dur_start = oct_digit + 1;
                let (pitch_part, duration_part) = s.split_at(dur_start);
                if duration_part.is_empty() {
                    return Err(format!("Melody note '{}' has an octave but no duration", s));
                }
                let (duration, dotted, triplet) = Self::parse_duration_part(duration_part)?;
                return Ok((pitch_part, duration, dotted, triplet, false));
            }
        }

        let duration_start = s
            .char_indices()
            .rfind(|(_, c)| !c.is_ascii_digit() && *c != '.' && *c != 't')
            .map(|(idx, c)| idx + c.len_utf8())
            .unwrap_or(0);

        if duration_start >= s.len() {
            return Err(format!(
                "Invalid note format '{}': expected <pitch><duration> or <pitch>_<duration>",
                s
            ));
        }

        let (pitch_part, duration_part) = s.split_at(duration_start);
        if pitch_part.is_empty() || duration_part.is_empty() {
            return Err(format!(
                "Invalid note format '{}': expected <pitch><duration> or <pitch>_<duration>",
                s
            ));
        }

        let (duration, dotted, triplet) = Self::parse_duration_part(duration_part)?;
        Ok((pitch_part, duration, dotted, triplet, false))
    }

    fn parse_duration_part(duration_part: &str) -> Result<(u8, bool, bool), String> {
        let mut digits = String::new();
        let mut dotted = false;
        let mut triplet = false;

        for c in duration_part.chars() {
            match c {
                '0'..='9' => digits.push(c),
                '.' if !dotted => dotted = true,
                't' if !triplet => triplet = true,
                _ => return Err(format!("Invalid duration: {}", duration_part)),
            }
        }

        if digits.is_empty() {
            return Err(format!("Invalid duration: {}", duration_part));
        }

        let duration = digits
            .parse()
            .map_err(|_| format!("Invalid duration: {}", duration_part))?;
        Ok((duration, dotted, triplet))
    }

    /// Parse octave modifier (`'` or `,`) from character iterator
    fn parse_octave_modifier(
        chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    ) -> OctaveModifier {
        let mut up = 0u8;
        let mut down = 0u8;
        while let Some(next) = chars.peek() {
            match next {
                '\'' => {
                    up = up.saturating_add(1);
                    chars.next();
                }
                ',' => {
                    down = down.saturating_add(1);
                    chars.next();
                }
                _ => break,
            }
        }

        match (up, down) {
            (0, 0) => OctaveModifier::None,
            (1, 0) => OctaveModifier::Up,
            (0, 1) => OctaveModifier::Down,
            (count, 0) => OctaveModifier::UpBy(count),
            (0, count) => OctaveModifier::DownBy(count),
            _ => OctaveModifier::None,
        }
    }

    /// Parse an explicit octave from the characters left after the pitch.
    ///
    /// The octave is introduced by `:` — `C:4` is C in octave 4 (the divider
    /// mirrors the `root:quality` colon used in chords). A bare digit with no
    /// colon is still accepted for the underscore form (`C5_4`) so older charts
    /// keep working. The old parenthesised form `C(4)` is no longer an octave.
    fn parse_explicit_octave(
        chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    ) -> Result<Option<u8>, String> {
        let colon = chars.peek() == Some(&':');
        if colon {
            chars.next();
        }
        let octave = if colon || chars.peek().is_some_and(|c| c.is_ascii_digit()) {
            let mut digits = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_ascii_digit() {
                    digits.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            if digits.is_empty() {
                return Err("Expected an octave number after ':'".to_string());
            }
            Some(
                digits
                    .parse::<u8>()
                    .map_err(|_| format!("Invalid octave: {}", digits))?,
            )
        } else {
            None
        };

        // Anything still unconsumed is malformed — most often a stray `(` from
        // the retired parenthesised octave syntax. Point the writer at `:`.
        if let Some(&c) = chars.peek() {
            return Err(format!(
                "Unexpected '{}' in melody note — write octaves with ':' (e.g. C:4)",
                c
            ));
        }
        Ok(octave)
    }

    /// Get duration in beats (assuming quarter note = 1 beat)
    pub fn duration_beats(&self) -> f64 {
        let mut base = 4.0 / self.duration as f64;
        if self.triplet {
            base *= 2.0 / 3.0;
        }
        if self.dotted { base * 1.5 } else { base }
    }
}

impl fmt::Display for MelodyNote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.extra_pitches.is_empty() {
            return write_chord_note(self, f);
        }
        if self.tie_stop {
            write!(f, "~")?;
        }
        // Use scale degree if available, otherwise pitch letter
        if let Some(degree) = self.scale_degree {
            write!(f, "{}", degree)?;
            // Add accidental if present (after removing base letter)
            if self.pitch.len() > 1 {
                write!(f, "{}", &self.pitch[1..])?;
            }
        } else {
            write!(f, "{}", self.pitch)?;
        }

        // Add octave modifier
        match self.octave_modifier {
            OctaveModifier::Up => write!(f, "'")?,
            OctaveModifier::Down => write!(f, ",")?,
            OctaveModifier::UpBy(count) => write!(f, "{}", "'".repeat(count as usize))?,
            OctaveModifier::DownBy(count) => write!(f, "{}", ",".repeat(count as usize))?,
            OctaveModifier::None => {}
        }

        // Add explicit octave if set
        if let Some(oct) = self.octave {
            write!(f, ":{}", oct)?;
        }

        let prefer_bare_duration = self.scale_degree.is_none();
        if prefer_bare_duration {
            write!(f, "{}", self.duration)?;
        } else {
            write!(f, "_{}", self.duration)?;
        }
        if self.dotted {
            write!(f, ".")?;
        }
        if self.triplet {
            write!(f, "t")?;
        }
        if self.tie_start {
            write!(f, "~")?;
        }
        Ok(())
    }
}

fn write_pitch_without_duration(note: &MelodyNote, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    if let Some(degree) = note.scale_degree {
        write!(f, "{}", degree)?;
        if note.pitch.len() > 1 {
            write!(f, "{}", &note.pitch[1..])?;
        }
    } else {
        write!(f, "{}", note.pitch)?;
    }
    match note.octave_modifier {
        OctaveModifier::Up => write!(f, "'")?,
        OctaveModifier::Down => write!(f, ",")?,
        OctaveModifier::UpBy(count) => write!(f, "{}", "'".repeat(count as usize))?,
        OctaveModifier::DownBy(count) => write!(f, "{}", ",".repeat(count as usize))?,
        OctaveModifier::None => {}
    }
    if let Some(oct) = note.octave {
        write!(f, ":{}", oct)?;
    }
    Ok(())
}

fn write_chord_note(note: &MelodyNote, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    if note.tie_stop {
        write!(f, "~")?;
    }
    write!(f, "<")?;
    write_pitch_without_duration(note, f)?;
    for (idx, (pitch, octave)) in note.extra_pitches.iter().enumerate() {
        write!(f, " ")?;
        let modifier = note
            .extra_pitch_modifiers
            .get(idx)
            .copied()
            .unwrap_or_default();
        match modifier {
            OctaveModifier::Up => write!(f, "'")?,
            OctaveModifier::Down => write!(f, ",")?,
            OctaveModifier::UpBy(count) => write!(f, "{}", "'".repeat(count as usize))?,
            OctaveModifier::DownBy(count) => write!(f, "{}", ",".repeat(count as usize))?,
            OctaveModifier::None => {}
        }
        write!(f, "{pitch}")?;
        if let Some(oct) = octave {
            write!(f, "({oct})")?;
        }
    }
    write!(f, ">")?;
    let prefer_bare_duration = note.scale_degree.is_none();
    if prefer_bare_duration {
        write!(f, "{}", note.duration)?;
    } else {
        write!(f, "_{}", note.duration)?;
    }
    if note.dotted {
        write!(f, ".")?;
    }
    if note.triplet {
        write!(f, "t")?;
    }
    if note.tie_start {
        write!(f, "~")?;
    }
    Ok(())
}

fn split_melody_tokens(content: &str) -> MelodyTokenIter<'_> {
    MelodyTokenIter { content, cursor: 0 }
}

fn split_chord_note_group_pitch_part(pitch_part: &str) -> Option<(OctaveModifier, &str)> {
    let group_start = pitch_part.find('<')?;
    let group_end = pitch_part.rfind('>')?;
    if group_end + 1 != pitch_part.len() || group_start > group_end {
        return None;
    }
    let modifier_prefix = &pitch_part[..group_start];
    if !modifier_prefix.chars().all(|ch| ch == '\'' || ch == ',') {
        return None;
    }
    let mut chars = modifier_prefix.chars().peekable();
    let modifier = MelodyNote::parse_octave_modifier(&mut chars);
    Some((modifier, &pitch_part[group_start + 1..group_end]))
}

fn combine_octave_modifiers(note: OctaveModifier, group: OctaveModifier) -> OctaveModifier {
    modifier_from_delta(octave_modifier_delta(note) + octave_modifier_delta(group))
}

fn octave_modifier_delta(modifier: OctaveModifier) -> i16 {
    match modifier {
        OctaveModifier::None => 0,
        OctaveModifier::Up => 1,
        OctaveModifier::Down => -1,
        OctaveModifier::UpBy(count) => i16::from(count),
        OctaveModifier::DownBy(count) => -i16::from(count),
    }
}

fn apply_octave_modifier(octave: u8, modifier: OctaveModifier) -> u8 {
    match modifier {
        OctaveModifier::None => octave,
        OctaveModifier::Up => octave.saturating_add(1),
        OctaveModifier::Down => octave.saturating_sub(1),
        OctaveModifier::UpBy(count) => octave.saturating_add(count),
        OctaveModifier::DownBy(count) => octave.saturating_sub(count),
    }
}

fn modifier_from_delta(delta: i16) -> OctaveModifier {
    match delta {
        0 => OctaveModifier::None,
        1 => OctaveModifier::Up,
        -1 => OctaveModifier::Down,
        n if n > 1 => OctaveModifier::UpBy(n as u8),
        n => OctaveModifier::DownBy((-n) as u8),
    }
}

/// Byte index of the `:` that introduces an octave on a melody token, i.e. a
/// `:` that sits outside any `<…>` chord-note group. Returns `None` when there
/// is no such colon (group-internal colons belong to the inner notes).
fn melody_octave_colon(token: &str) -> Option<usize> {
    let mut depth = 0i32;
    for (i, c) in token.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth -= 1,
            ':' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

fn melody_token_has_duration(token: &str) -> bool {
    melody_token_duration_start(token).is_some()
}

fn melody_token_duration_suffix(token: &str) -> Result<String, String> {
    let clean_token = token
        .trim()
        .trim_start_matches('~')
        .trim_end_matches('~')
        .trim_start_matches('!');
    let Some(start) = melody_token_duration_start(clean_token) else {
        return Err(format!("Melody token '{}' has no duration", token));
    };
    Ok(clean_token[start..].to_string())
}

fn melody_token_duration_start(token: &str) -> Option<usize> {
    let token = token
        .trim()
        .trim_start_matches('~')
        .trim_end_matches('~')
        .trim_start_matches('!');
    if token.is_empty() {
        return None;
    }
    // An explicit `_` always delimits the duration — check it first, so it wins
    // over a `:` octave (`C:4_8`) or a `>` group close (`<C E>_8`).
    if let Some((idx, suffix)) = token
        .char_indices()
        .find(|(_, c)| *c == '_')
        .map(|(idx, _)| (idx, &token[idx + 1..]))
    {
        return is_melody_duration_suffix(suffix).then_some(idx);
    }
    // A `:` octave takes a single digit; any duration follows it directly. The
    // octave digit itself is never a duration.
    if let Some(colon) = melody_octave_colon(token) {
        let oct_digit = colon + 1;
        if token[oct_digit..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
        {
            let dur_start = oct_digit + 1;
            let suffix = &token[dur_start..];
            return is_melody_duration_suffix(suffix).then_some(dur_start);
        }
    }
    if let Some(close) = token.rfind('>') {
        let suffix = &token[close + 1..];
        if is_melody_duration_suffix(suffix) {
            return Some(token.len() - suffix.len());
        }
        return None;
    }
    let start = token
        .char_indices()
        .rfind(|(_, c)| !c.is_ascii_digit() && *c != '.' && *c != 't')
        .map(|(idx, c)| idx + c.len_utf8())?;
    (start < token.len() && is_melody_duration_suffix(&token[start..])).then_some(start)
}

fn is_melody_duration_suffix(suffix: &str) -> bool {
    let suffix = suffix.trim_end_matches('~');
    if suffix.is_empty() {
        return false;
    }
    MelodyNote::parse_duration_part(suffix).is_ok()
}

struct MelodyTokenIter<'a> {
    content: &'a str,
    cursor: usize,
}

impl<'a> Iterator for MelodyTokenIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        let bytes = self.content.as_bytes();
        while self.cursor < bytes.len() && bytes[self.cursor].is_ascii_whitespace() {
            self.cursor += 1;
        }
        if self.cursor >= bytes.len() {
            return None;
        }
        let start = self.cursor;
        let has_tie_stop = bytes[self.cursor] == b'~';
        if has_tie_stop {
            self.cursor += 1;
        }
        while self.cursor < bytes.len() && matches!(bytes[self.cursor], b'\'' | b',') {
            self.cursor += 1;
        }
        if self.cursor < bytes.len() && bytes[self.cursor] == b'<' {
            while self.cursor < bytes.len() && bytes[self.cursor] != b'>' {
                self.cursor += 1;
            }
            if self.cursor < bytes.len() {
                self.cursor += 1;
            }
            while self.cursor < bytes.len()
                && (bytes[self.cursor].is_ascii_digit()
                    || matches!(bytes[self.cursor], b'.' | b't' | b'~'))
            {
                self.cursor += 1;
            }
        } else {
            while self.cursor < bytes.len() && !bytes[self.cursor].is_ascii_whitespace() {
                self.cursor += 1;
            }
        }
        Some(&self.content[start..self.cursor])
    }
}

/// A melody is a sequence of notes
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Melody {
    /// The notes in this melody
    pub notes: Vec<MelodyNote>,

    /// Optional name (for variable assignment)
    pub name: Option<String>,
}

impl Melody {
    /// Create a new empty melody
    pub fn new() -> Self {
        Self {
            notes: Vec::new(),
            name: None,
        }
    }

    /// Create a melody with the given notes
    pub fn with_notes(notes: Vec<MelodyNote>) -> Self {
        Self { notes, name: None }
    }

    /// Set the name (for variable assignment)
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Add a note to the melody
    pub fn add_note(&mut self, note: MelodyNote) {
        self.notes.push(note);
    }

    /// Get total duration in beats
    pub fn total_beats(&self) -> f64 {
        self.notes.iter().map(|n| n.duration_beats()).sum()
    }

    /// Resolve all relative notes to concrete octaves.
    pub fn resolve_absolute_octaves(&mut self) {
        self.resolve_absolute_octaves_from(0, 4);
    }

    /// Resolve all relative notes to concrete octaves from a starting reference.
    pub fn resolve_absolute_octaves_from(&mut self, mut ref_pitch_class: i8, mut ref_octave: u8) {
        for note in &mut self.notes {
            if note.is_rest() || note.is_space() {
                continue;
            }

            let primary_octave = note.resolve_octave(ref_pitch_class, ref_octave);
            note.octave = Some(primary_octave);
            note.octave_modifier = OctaveModifier::None;

            let note_pitch_class = note.pitch_class();
            let primary_abs = note_pitch_class.map(|pc| absolute_pitch_value(pc, primary_octave));
            let mut lowest_abs = primary_abs;

            let mut group_ref_pitch_class = note_pitch_class.unwrap_or(ref_pitch_class);
            let mut group_ref_octave = primary_octave;

            for idx in 0..note.extra_pitches.len() {
                let (pitch, octave) = &mut note.extra_pitches[idx];
                let modifier = note
                    .extra_pitch_modifiers
                    .get(idx)
                    .copied()
                    .unwrap_or_default();
                let Some(pc) = pitch_class_for_name(pitch) else {
                    continue;
                };
                let prev_abs = absolute_pitch_value(group_ref_pitch_class, group_ref_octave);
                let resolved = if let Some(octave) = *octave {
                    octave
                } else {
                    let mut octave = group_ref_octave.saturating_sub(1);
                    while absolute_pitch_value(pc, octave) <= prev_abs {
                        octave = octave.saturating_add(1);
                    }
                    apply_octave_modifier(octave, modifier)
                };
                *octave = Some(resolved);
                let abs = absolute_pitch_value(pc, resolved);
                lowest_abs = Some(lowest_abs.map_or(abs, |current| current.min(abs)));
                group_ref_pitch_class = pc;
                group_ref_octave = resolved;
                if let Some(modifier) = note.extra_pitch_modifiers.get_mut(idx) {
                    *modifier = OctaveModifier::None;
                }
            }

            if let Some(abs) = lowest_abs {
                ref_pitch_class = (abs % 12) as i8;
                ref_octave = (abs / 12) as u8;
            } else if let Some(pc) = note.pitch_class() {
                ref_pitch_class = pc;
                ref_octave = primary_octave;
            }
        }
    }

    /// Parse a melody from a string (the content inside m{ })
    ///
    /// Example: "C_8 D_8 E_4 F_4"
    pub fn parse(content: &str) -> Result<Self, String> {
        Self::parse_with_defaults(content, None, None)
    }

    /// Parse a melody using an optional sticky duration before the first note.
    pub fn parse_with_default_duration(
        content: &str,
        default_duration: Option<&str>,
    ) -> Result<Self, String> {
        Self::parse_with_defaults(content, default_duration, None)
    }

    /// Parse a melody with optional inherited duration and starting octave.
    pub fn parse_with_defaults(
        content: &str,
        default_duration: Option<&str>,
        default_octave: Option<u8>,
    ) -> Result<Self, String> {
        let mut notes: Vec<MelodyNote> = Vec::new();
        let mut inherited_duration: Option<String> = default_duration.map(str::to_string);

        for token in split_melody_tokens(content) {
            let (no_memory, token) = token
                .strip_prefix('!')
                .map_or((false, token), |token| (true, token));
            if let Some(duration_override) = token.strip_prefix('.') {
                let mut note = notes
                    .last()
                    .cloned()
                    .ok_or_else(|| "Melody dot repeat needs a previous note".to_string())?;
                if !duration_override.is_empty() {
                    let (duration, dotted, triplet) =
                        MelodyNote::parse_duration_part(duration_override)?;
                    note.duration = duration;
                    note.dotted = dotted;
                    note.triplet = triplet;
                    if !no_memory {
                        inherited_duration = Some(duration_override.to_string());
                    }
                } else if !no_memory {
                    inherited_duration = Some(melody_token_duration_suffix(&note.to_string())?);
                }
                note.tie_start = false;
                note.tie_stop = false;
                notes.push(note);
                continue;
            }

            let token_to_parse = if melody_token_has_duration(token) {
                token.to_string()
            } else if let Some(duration) = &inherited_duration {
                format!("{token}{duration}")
            } else {
                // No duration written yet and nothing to inherit: default to a
                // quarter note. The `_` keeps it unambiguous for every token
                // shape (scale degrees, `:` octaves, chord-note groups).
                format!("{token}_4")
            };
            let note = MelodyNote::parse(&token_to_parse)?;
            if !no_memory {
                inherited_duration = Some(melody_token_duration_suffix(&token_to_parse)?);
            }
            notes.push(note);
        }

        let mut melody = Melody { notes, name: None };
        if let Some(octave) = default_octave {
            melody.resolve_absolute_octaves_from(0, octave);
        }
        Ok(melody)
    }

    /// Parse a melody block from a string including the m{ } wrapper
    ///
    /// Format: `m{ <notes> }` or `<name> = m{ <notes> }`
    pub fn parse_block(s: &str) -> Result<(Option<String>, Self), String> {
        Self::parse_block_with_default_duration(s, None)
    }

    /// Parse a melody block with an optional inherited starting duration.
    pub fn parse_block_with_default_duration(
        s: &str,
        default_duration: Option<&str>,
    ) -> Result<(Option<String>, Self), String> {
        Self::parse_block_with_defaults(s, default_duration, None)
    }

    /// Parse a melody block with optional inherited duration and starting octave.
    pub fn parse_block_with_defaults(
        s: &str,
        default_duration: Option<&str>,
        default_octave: Option<u8>,
    ) -> Result<(Option<String>, Self), String> {
        let s = s.trim();

        // Check for variable assignment
        let (name, melody_part) = if let Some(eq_pos) = s.find('=') {
            let name = s[..eq_pos].trim().to_string();
            let rest = s[eq_pos + 1..].trim();
            (Some(name), rest)
        } else {
            (None, s)
        };

        // Parse the m{ } block
        let normalized;
        let melody_part = if let Some(rest) = melody_part.strip_prefix("m {") {
            normalized = format!("m{{{rest}");
            normalized.as_str()
        } else {
            melody_part
        };
        if !melody_part.starts_with("m{") {
            return Err("Melody block must start with m{".to_string());
        }

        let close_brace = melody_part
            .rfind('}')
            .ok_or("Missing closing brace in melody block")?;

        let content = &melody_part[2..close_brace];
        let mut melody = Self::parse_with_defaults(content, default_duration, default_octave)?;
        melody.name = name.clone();

        Ok((name, melody))
    }
}

fn absolute_pitch_value(pitch_class: i8, octave: u8) -> i16 {
    i16::from(octave) * 12 + i16::from(pitch_class)
}

fn pitch_class_for_name(name: &str) -> Option<i8> {
    let base_pitch = name.chars().next()?;
    let base_class = PITCH_CLASS
        .iter()
        .find(|(p, _)| p.starts_with(base_pitch))
        .map(|(_, c)| *c)?;
    let accidental_offset = if name.contains('#') {
        1
    } else if name.contains('b') {
        -1
    } else {
        0
    };
    Some((base_class + accidental_offset).rem_euclid(12))
}

impl Default for Melody {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for Melody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(name) = &self.name {
            write!(f, "{} = ", name)?;
        }
        write!(f, "m{{ ")?;
        let notes: Vec<String> = self.notes.iter().map(|n| n.to_string()).collect();
        write!(f, "{}", notes.join(" "))?;
        write!(f, " }}")
    }
}

/// Storage for melody variables
#[derive(Debug, Clone, Default, PartialEq, Facet)]
pub struct MelodyVariables {
    /// Map from variable name to melody
    variables: HashMap<String, Melody>,
}

impl MelodyVariables {
    /// Create new empty variable storage
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
        }
    }

    /// Store a melody variable
    pub fn set(&mut self, name: impl Into<String>, melody: Melody) {
        self.variables.insert(name.into(), melody);
    }

    /// Get a melody variable
    pub fn get(&self, name: &str) -> Option<&Melody> {
        self.variables.get(name)
    }

    /// Check if a variable exists
    pub fn contains(&self, name: &str) -> bool {
        self.variables.contains_key(name)
    }

    /// Get all variable names
    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.variables.keys()
    }

    /// Iterate over all variables
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Melody)> {
        self.variables.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_note() {
        let note = MelodyNote::parse("C_4").unwrap();
        assert_eq!(note.pitch, "C");
        assert_eq!(note.octave, None);
        assert_eq!(note.duration, 4);
        assert!(!note.dotted);
    }

    #[test]
    fn test_parse_note_with_octave() {
        let note = MelodyNote::parse("D5_8").unwrap();
        assert_eq!(note.pitch, "D");
        assert_eq!(note.octave, Some(5));
        assert_eq!(note.duration, 8);
    }

    #[test]
    fn test_parse_note_with_accidental() {
        let note = MelodyNote::parse("F#4_4").unwrap();
        assert_eq!(note.pitch, "F#");
        assert_eq!(note.octave, Some(4));

        let note2 = MelodyNote::parse("Bb3_2").unwrap();
        assert_eq!(note2.pitch, "Bb");
        assert_eq!(note2.octave, Some(3));
    }

    #[test]
    fn test_parse_dotted_note() {
        let note = MelodyNote::parse("G_4.").unwrap();
        assert_eq!(note.pitch, "G");
        assert_eq!(note.duration, 4);
        assert!(note.dotted);
    }

    #[test]
    fn test_parse_rest() {
        let note = MelodyNote::parse("r_4").unwrap();
        assert!(note.is_rest());
        assert_eq!(note.duration, 4);
    }

    #[test]
    fn test_duration_beats() {
        let quarter = MelodyNote::new("C", 4);
        assert_eq!(quarter.duration_beats(), 1.0);

        let eighth = MelodyNote::new("C", 8);
        assert_eq!(eighth.duration_beats(), 0.5);

        let half = MelodyNote::new("C", 2);
        assert_eq!(half.duration_beats(), 2.0);

        let dotted_quarter = MelodyNote::new("C", 4).dotted();
        assert_eq!(dotted_quarter.duration_beats(), 1.5);
    }

    #[test]
    fn test_note_display() {
        // An explicit octave displays in the canonical `:` form, whether it was
        // written with `:` or the legacy bare-digit underscore form.
        let note = MelodyNote::parse("D#:58").unwrap();
        assert_eq!(format!("{}", note), "D#:58");
        let legacy = MelodyNote::parse("D#5_8").unwrap();
        assert_eq!(format!("{}", legacy), "D#:58");

        let dotted = MelodyNote::parse("A_4.").unwrap();
        assert_eq!(format!("{}", dotted), "A4.");
    }

    #[test]
    fn test_parse_melody() {
        let melody = Melody::parse("C_8 D_8 E_4").unwrap();
        assert_eq!(melody.notes.len(), 3);
        assert_eq!(melody.notes[0].pitch, "C");
        assert_eq!(melody.notes[1].pitch, "D");
        assert_eq!(melody.notes[2].pitch, "E");
    }

    #[test]
    fn test_parse_melody_block() {
        let (name, melody) = Melody::parse_block("m{ C_8 D_8 E_4 }").unwrap();
        assert!(name.is_none());
        assert_eq!(melody.notes.len(), 3);
    }

    #[test]
    fn test_parse_melody_block_with_name() {
        let (name, melody) = Melody::parse_block("mainRiff = m{ C_8 D_8 E_4 }").unwrap();
        assert_eq!(name, Some("mainRiff".to_string()));
        assert_eq!(melody.notes.len(), 3);
    }

    #[test]
    fn test_melody_display() {
        let melody = Melody::parse("C_8 D_8 E_4").unwrap();
        assert_eq!(format!("{}", melody), "m{ C8 D8 E4 }");

        let named = melody.with_name("riff");
        assert_eq!(format!("{}", named), "riff = m{ C8 D8 E4 }");
    }

    #[test]
    fn test_melody_variables() {
        let mut vars = MelodyVariables::new();

        let melody = Melody::parse("C_4 D_4 E_4").unwrap();
        vars.set("mainRiff", melody);

        assert!(vars.contains("mainRiff"));
        assert!(!vars.contains("otherRiff"));

        let retrieved = vars.get("mainRiff").unwrap();
        assert_eq!(retrieved.notes.len(), 3);
    }

    #[test]
    fn test_melody_total_beats() {
        let melody = Melody::parse("C_4 D_4 E_4 F_4").unwrap();
        assert_eq!(melody.total_beats(), 4.0);

        let melody2 = Melody::parse("C_8 D_8 E_8 F_8").unwrap();
        assert_eq!(melody2.total_beats(), 2.0);
    }

    // New tests for scale degrees, octave modifiers, and relative pitch

    #[test]
    fn test_parse_scale_degree() {
        let note = MelodyNote::parse("1_4").unwrap();
        assert_eq!(note.pitch, "C");
        assert_eq!(note.scale_degree, Some(1));
        assert_eq!(note.duration, 4);

        let note = MelodyNote::parse("3_8").unwrap();
        assert_eq!(note.pitch, "E");
        assert_eq!(note.scale_degree, Some(3));

        let note = MelodyNote::parse("5_2").unwrap();
        assert_eq!(note.pitch, "G");
        assert_eq!(note.scale_degree, Some(5));

        let note = MelodyNote::parse("7_4").unwrap();
        assert_eq!(note.pitch, "B");
        assert_eq!(note.scale_degree, Some(7));
    }

    #[test]
    fn test_parse_scale_degree_with_accidental() {
        let note = MelodyNote::parse("3#_4").unwrap();
        assert_eq!(note.pitch, "E#");
        assert_eq!(note.scale_degree, Some(3));

        let note = MelodyNote::parse("7b_8").unwrap();
        assert_eq!(note.pitch, "Bb");
        assert_eq!(note.scale_degree, Some(7));
    }

    #[test]
    fn test_parse_octave_modifier_up() {
        let note = MelodyNote::parse("C'_4").unwrap();
        assert_eq!(note.pitch, "C");
        assert_eq!(note.octave_modifier, OctaveModifier::Up);
        assert_eq!(note.octave, None);

        let note = MelodyNote::parse("D#'_8").unwrap();
        assert_eq!(note.pitch, "D#");
        assert_eq!(note.octave_modifier, OctaveModifier::Up);
    }

    #[test]
    fn test_parse_octave_modifier_down() {
        let note = MelodyNote::parse("G,_4").unwrap();
        assert_eq!(note.pitch, "G");
        assert_eq!(note.octave_modifier, OctaveModifier::Down);
        assert_eq!(note.octave, None);

        let note = MelodyNote::parse("Bb,_2.").unwrap();
        assert_eq!(note.pitch, "Bb");
        assert_eq!(note.octave_modifier, OctaveModifier::Down);
        assert!(note.dotted);
    }

    #[test]
    fn test_scale_degree_with_octave_modifier() {
        let note = MelodyNote::parse("3'_4").unwrap();
        assert_eq!(note.pitch, "E");
        assert_eq!(note.scale_degree, Some(3));
        assert_eq!(note.octave_modifier, OctaveModifier::Up);

        let note = MelodyNote::parse("5,_8").unwrap();
        assert_eq!(note.pitch, "G");
        assert_eq!(note.scale_degree, Some(5));
        assert_eq!(note.octave_modifier, OctaveModifier::Down);
    }

    #[test]
    fn test_pitch_class() {
        let c = MelodyNote::new("C", 4);
        assert_eq!(c.pitch_class(), Some(0));

        let d = MelodyNote::new("D", 4);
        assert_eq!(d.pitch_class(), Some(2));

        let fsharp = MelodyNote::new("F#", 4);
        assert_eq!(fsharp.pitch_class(), Some(6)); // F=5, +1 for sharp

        let bb = MelodyNote::new("Bb", 4);
        assert_eq!(bb.pitch_class(), Some(10)); // B=11, -1 for flat

        let rest = MelodyNote::rest(4);
        assert_eq!(rest.pitch_class(), None);
    }

    #[test]
    fn test_resolve_octave_closest() {
        // C4 -> D should stay in octave 4 (close)
        let d = MelodyNote::new("D", 4);
        assert_eq!(d.resolve_octave(0, 4), 4);

        // C4 -> B should go down to octave 3 (B is closer below)
        let b = MelodyNote::new("B", 4);
        assert_eq!(b.resolve_octave(0, 4), 3);

        // G4 -> A should stay in octave 4 (close)
        let a = MelodyNote::new("A", 4);
        assert_eq!(a.resolve_octave(7, 4), 4); // G = pitch class 7
    }

    #[test]
    fn test_resolve_octave_with_modifier() {
        // C4 -> D' should go to octave 5 (up modifier)
        let d_up = MelodyNote::new("D", 4).with_octave_modifier(OctaveModifier::Up);
        assert_eq!(d_up.resolve_octave(0, 4), 5);

        // C4 -> D, should go to octave 3 (down modifier)
        let d_down = MelodyNote::new("D", 4).with_octave_modifier(OctaveModifier::Down);
        assert_eq!(d_down.resolve_octave(0, 4), 3);
    }

    #[test]
    fn test_resolve_octave_explicit_overrides() {
        // Explicit octave ignores relative calculation
        let note = MelodyNote::new("D", 4).with_octave(6);
        assert_eq!(note.resolve_octave(0, 4), 6);
    }

    #[test]
    fn test_display_with_octave_modifier() {
        let note = MelodyNote::parse("C'_4").unwrap();
        assert_eq!(format!("{}", note), "C'4");

        let note = MelodyNote::parse("D,_8").unwrap();
        assert_eq!(format!("{}", note), "D,8");
    }

    #[test]
    fn test_display_scale_degree() {
        let note = MelodyNote::parse("3_4").unwrap();
        assert_eq!(format!("{}", note), "3_4");

        let note = MelodyNote::parse("5#'_8").unwrap();
        assert_eq!(format!("{}", note), "5#'_8");
    }

    #[test]
    fn test_melody_with_mixed_notation() {
        let melody = Melody::parse("C_4 3_8 G'_8 1,_4").unwrap();
        assert_eq!(melody.notes.len(), 4);
        assert_eq!(melody.notes[0].pitch, "C");
        assert_eq!(melody.notes[1].pitch, "E");
        assert_eq!(melody.notes[1].scale_degree, Some(3));
        assert_eq!(melody.notes[2].pitch, "G");
        assert_eq!(melody.notes[2].octave_modifier, OctaveModifier::Up);
        assert_eq!(melody.notes[3].pitch, "C");
        assert_eq!(melody.notes[3].octave_modifier, OctaveModifier::Down);
    }

    #[test]
    fn test_parse_bare_lilypond_style_and_triplets() {
        let melody = Melody::parse("G2 r4t F#4t F4t").unwrap();
        assert_eq!(melody.notes.len(), 4);
        assert_eq!(melody.notes[0].pitch, "G");
        assert_eq!(melody.notes[0].duration, 2);
        assert!(!melody.notes[0].triplet);
        assert!(melody.notes[1].is_rest());
        assert_eq!(melody.notes[1].duration, 4);
        assert!(melody.notes[1].triplet);
        assert_eq!(format!("{}", melody), "m{ G2 r4t F#4t F4t }");
        assert!((melody.total_beats() - 4.0).abs() < 0.0001);
    }

    #[test]
    fn test_colon_octave_forms() {
        // `:` introduces the octave; a duration may follow it directly (`C:48`)
        // or after an underscore (`C:4_8`).
        for s in ["C:48", "C:4_8"] {
            let note = MelodyNote::parse(s).unwrap();
            assert_eq!(note.octave, Some(4), "{s}");
            assert_eq!(note.duration, 8, "{s}");
        }
        // Octave-only notes inherit the running duration inside a block.
        let melody = Melody::parse("C:48 D:4 E:4").unwrap();
        assert_eq!(melody.notes.len(), 3);
        for (note, oct) in melody.notes.iter().zip([4, 4, 4]) {
            assert_eq!(note.octave, Some(oct));
            assert_eq!(note.duration, 8); // inherited from C:48
        }
        // Per-note octaves inside a chord-note group.
        let group = MelodyNote::parse("<C:3 E:4>8").unwrap();
        assert_eq!(group.octave, Some(3));
        assert_eq!(group.extra_pitches[0], ("E".to_string(), Some(4)));
    }

    #[test]
    fn test_parenthesised_octave_is_rejected() {
        // The retired `C(4)` syntax is no longer an octave.
        assert!(MelodyNote::parse("C(4)8").is_err());
    }

    #[test]
    fn test_unspecified_duration_defaults_to_quarter() {
        // With no duration written and none to inherit, notes are quarter notes.
        let melody = Melody::parse("C D E F").unwrap();
        assert_eq!(melody.notes.len(), 4);
        for note in &melody.notes {
            assert_eq!(note.duration, 4);
            assert!(!note.dotted && !note.triplet);
        }
        // The default applies until an explicit duration takes over and sticks.
        let melody = Melody::parse("C D E8 F").unwrap();
        assert_eq!(melody.notes[0].duration, 4); // default quarter
        assert_eq!(melody.notes[1].duration, 4);
        assert_eq!(melody.notes[2].duration, 8); // explicit eighth
        assert_eq!(melody.notes[3].duration, 8); // inherits the eighth
        // Octave-only notes and chord groups default too.
        assert_eq!(Melody::parse("C:4").unwrap().notes[0].duration, 4);
        assert_eq!(Melody::parse("<C E G>").unwrap().notes[0].duration, 4);
        // Scale degrees as well.
        assert_eq!(Melody::parse("1 2 3").unwrap().notes[0].duration, 4);
    }

    #[test]
    fn test_melody_chord_note_group() {
        let melody = Melody::parse("<F# 'C#>4.").unwrap();
        assert_eq!(melody.notes.len(), 1);
        let note = &melody.notes[0];
        assert_eq!(note.pitch, "F#");
        assert_eq!(note.duration, 4);
        assert!(note.dotted);
        assert_eq!(note.extra_pitches, vec![("C#".to_string(), None)]);
        assert_eq!(note.extra_pitch_modifiers, vec![OctaveModifier::Up]);
        assert_eq!(format!("{}", melody), "m{ <F# 'C#>4. }");
    }

    #[test]
    fn test_melody_tie_markers() {
        let melody = Melody::parse("<D 'D>2.~ ~<D 'D>2.~ ~<D 'D>4.").unwrap();
        assert!(melody.notes[0].tie_start);
        assert!(!melody.notes[0].tie_stop);
        assert!(melody.notes[1].tie_start);
        assert!(melody.notes[1].tie_stop);
        assert!(!melody.notes[2].tie_start);
        assert!(melody.notes[2].tie_stop);
        assert_eq!(format!("{}", melody), "m{ <D 'D>2.~ ~<D 'D>2.~ ~<D 'D>4. }");
    }

    #[test]
    fn test_melody_duration_memory_with_one_time_override() {
        let melody = Melody::parse("F#8. C# !Eb4 Bb").unwrap();

        assert_eq!(melody.notes[0].duration, 8);
        assert!(melody.notes[0].dotted);
        assert_eq!(melody.notes[1].duration, 8);
        assert!(melody.notes[1].dotted);
        assert_eq!(melody.notes[2].duration, 4);
        assert!(!melody.notes[2].dotted);
        assert_eq!(melody.notes[3].duration, 8);
        assert!(melody.notes[3].dotted);
    }

    #[test]
    fn test_melody_duration_memory_keeps_underscore_form() {
        let melody = Melody::parse("C#:32.~ ~C#:3 ~C#:3 ~C#:34. G#:3").unwrap();

        assert_eq!(melody.notes[0].octave, Some(3));
        assert_eq!(melody.notes[0].duration, 2);
        assert!(melody.notes[0].dotted);
        assert_eq!(melody.notes[1].octave, Some(3));
        assert_eq!(melody.notes[1].duration, 2);
        assert!(melody.notes[1].dotted);
        assert_eq!(melody.notes[2].octave, Some(3));
        assert_eq!(melody.notes[2].duration, 2);
        assert!(melody.notes[2].dotted);
        assert_eq!(melody.notes[3].octave, Some(3));
        assert_eq!(melody.notes[3].duration, 4);
        assert!(melody.notes[3].dotted);
        assert_eq!(melody.notes[4].octave, Some(3));
        assert_eq!(melody.notes[4].duration, 4);
        assert!(melody.notes[4].dotted);
    }

    #[test]
    fn test_melody_colon_absolute_octave_round_trips() {
        let melody = Melody::parse("C#:38. D#:4").unwrap();

        assert_eq!(melody.notes[0].octave, Some(3));
        assert_eq!(melody.notes[1].octave, Some(4));
        assert_eq!(melody.notes[1].duration, 8);
        assert!(melody.notes[1].dotted);
        assert_eq!(format!("{}", melody), "m{ C#:38. D#:48. }");
    }

    #[test]
    fn test_melody_resolves_absolute_octaves_and_chord_notes() {
        let melody = Melody::parse_with_defaults("<F# C#>8. <G# D#>", None, Some(2)).unwrap();

        assert_eq!(melody.notes[0].octave, Some(2));
        assert_eq!(
            melody.notes[0].extra_pitches[0],
            ("C#".to_string(), Some(3))
        );
        assert_eq!(melody.notes[1].octave, Some(2));
        assert_eq!(
            melody.notes[1].extra_pitches[0],
            ("D#".to_string(), Some(3))
        );
    }

    #[test]
    fn test_melody_chord_note_group_resolves_low_to_high() {
        let melody = Melody::parse_with_defaults("<F# C# E A>8.", None, Some(2)).unwrap();

        assert_eq!(melody.notes[0].octave, Some(2));
        assert_eq!(
            melody.notes[0].extra_pitches[0],
            ("C#".to_string(), Some(3))
        );
        assert_eq!(melody.notes[0].extra_pitches[1], ("E".to_string(), Some(3)));
        assert_eq!(melody.notes[0].extra_pitches[2], ("A".to_string(), Some(3)));
    }

    #[test]
    fn test_melody_chord_note_group_accepts_block_octave_modifier() {
        let melody = Melody::parse(",<F# ''C#>8.").unwrap();

        assert_eq!(melody.notes[0].octave_modifier, OctaveModifier::Down);
        assert_eq!(melody.notes[0].extra_pitch_modifiers[0], OctaveModifier::Up);
        assert_eq!(format!("{}", melody), "m{ <F#, 'C#>8. }");
    }

    #[test]
    fn test_melody_chord_note_group_repeated_pitch_resolves_above() {
        let melody = Melody::parse_with_defaults("<F# F#>8.", None, Some(2)).unwrap();

        assert_eq!(melody.notes[0].octave, Some(2));
        assert_eq!(
            melody.notes[0].extra_pitches[0],
            ("F#".to_string(), Some(3))
        );
    }

    #[test]
    fn test_melody_dot_repeat_can_override_duration() {
        let melody = Melody::parse("<Dn 'Dn>4. .8 . .").unwrap();

        assert_eq!(melody.notes.len(), 4);
        assert_eq!(melody.notes[0].duration, 4);
        assert!(melody.notes[0].dotted);
        for note in &melody.notes[1..] {
            assert_eq!(note.pitch, "Dn");
            assert_eq!(note.duration, 8);
            assert!(!note.dotted);
            assert_eq!(note.extra_pitches, vec![("Dn".to_string(), None)]);
            assert_eq!(note.extra_pitch_modifiers, vec![OctaveModifier::Up]);
        }
    }

    #[test]
    fn test_melody_chord_note_group_applies_inner_marker_after_ascending_resolution() {
        let melody = Melody::parse_with_defaults("<F# 'C#>8.", None, Some(2)).unwrap();

        assert_eq!(melody.notes[0].octave, Some(2));
        assert_eq!(
            melody.notes[0].extra_pitches[0],
            ("C#".to_string(), Some(4))
        );
    }

    #[test]
    fn test_melody_chord_note_group_comma_can_go_down_after_ascending_resolution() {
        let melody = Melody::parse_with_defaults("<F# ,C#>8.", None, Some(2)).unwrap();

        assert_eq!(melody.notes[0].octave, Some(2));
        assert_eq!(
            melody.notes[0].extra_pitches[0],
            ("C#".to_string(), Some(2))
        );
    }

    #[test]
    fn test_invalid_scale_degree() {
        assert!(MelodyNote::parse("0_4").is_err());
        assert!(MelodyNote::parse("8_4").is_err());
        assert!(MelodyNote::parse("9_4").is_err());
    }
}
