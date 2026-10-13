//! Chart transposition / notation display service.
//!
//! A **non-destructive** display transform over a parsed [`Chart`]: given a
//! [`ChartView`] (a target key, a [`NotationSystem`], and an optional capo),
//! [`apply_view`] returns a *new* `Chart` whose chord symbols are rewritten to
//! the requested key + notation. The source chart is never mutated.
//!
//! The engraver renders chord symbols from each [`ChordInstance::full_symbol`]
//! string and already understands letter chords (`A D E`), Nashville numbers
//! (`1 4 5 6m`), and Roman numerals (`I IV V vi`). So this service does *not*
//! touch the engraver — it only produces a `Chart` whose `full_symbol`s (and
//! the structured `root`/`bass` behind them) carry the desired spelling, and
//! the existing engraver renders it unchanged.
//!
//! # Capo convention (physical, not "reading")
//!
//! A capo at fret *N* raises sounding pitch *N* semitones above the fingered
//! shapes. So to SOUND `sounding_key` with a capo at fret *N*, the player
//! FINGERS shapes in `sounding_key - N` semitones. When a view sets both a
//! `target_key` and `capo > 0`, letter chords render in the **shape key**
//! ([`shape_key_for_capo`]) — that is what the player's hands do — while the
//! view still carries the sounding `target_key` so a UI can print
//! "Capo N (sounds <target_key>)" (see [`ChartView::capo_caption`]).
//!
//! Known-good anchor (a test): to sound **B** using **G** shapes, capo = **4**
//! (`G + 4 = B`).

use crate::chart::types::{KeyChange, RhythmElement};
use crate::chart::{Chart, ChordInstance};
use crate::chord::{Chord, ChordQuality, SuspendedType};
use crate::key::{Key, KeySpelling, ScaleMode, SpellingMode};
use crate::primitives::{Accidental, MusicalNote, RomanCase, RootNotation};
use crate::sections::SectionType;
use keyflow_syntax::parsing::Lexer;

/// How chord roots are spelled for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NotationSystem {
    /// Letter chords spelled for the effective key (`G C D Em`).
    #[default]
    Letters,
    /// Nashville numbers — scale degree in the effective key (`1 4 5 6m`).
    Nashville,
    /// Roman numerals — degree, case by quality (`I IV V vi`, `vii°`).
    Roman,
}

/// A non-destructive display transform over a chart.
#[derive(Debug, Clone, Default)]
pub struct ChartView {
    /// Render the chart *sounding* in this key. `None` = the chart's own key
    /// (no transposition). Ignored for Nashville / Roman symbol text, which is
    /// key-invariant, but still used to label the displayed key metadata.
    pub target_key: Option<Key>,
    /// Which notation the chord roots are spelled in.
    pub notation: NotationSystem,
    /// Capo fret (`0` = no capo). See the module docs for the capo convention.
    pub capo: u8,
}

impl ChartView {
    /// The key whose *shapes* the player fingers under this view: the
    /// `target_key` transposed down by the capo fret. `None` when there is no
    /// target key. With `capo == 0` this equals `target_key`.
    pub fn shape_key(&self) -> Option<Key> {
        self.target_key
            .as_ref()
            .map(|k| shape_key_for_capo(k.clone(), self.capo))
    }

    /// A UI caption for a capo'd view: `Some("Capo 4 (sounds B)")` when the
    /// view has a capo and a sounding target key, else `None`. This service
    /// never renders UI text into the chart itself; call this from the UI.
    pub fn capo_caption(&self) -> Option<String> {
        match (&self.target_key, self.capo) {
            (Some(k), n) if n > 0 => Some(format!("Capo {} (sounds {})", n, k.short_name())),
            _ => None,
        }
    }
}

// ===========================================================================
// Interval + capo helpers
// ===========================================================================

/// Signed semitone distance from `from` to `to`, taking the shortest path
/// around the octave (result is in `-6..=6`). `interval_semitones(C, G) == -5`
/// (a 4th down is shorter than a 5th up).
pub fn interval_semitones(from: Key, to: Key) -> i8 {
    let diff = to.root.semitone as i8 - from.root.semitone as i8;
    if diff > 6 {
        diff - 12
    } else if diff < -6 {
        diff + 12
    } else {
        diff
    }
}

/// The key whose shapes a player fingers to SOUND `sounding` with a capo at
/// `capo_fret`: `sounding` transposed **down** by `capo_fret` semitones. The
/// scale mode (major/minor) is preserved.
///
/// `shape_key_for_capo(B, 4) == G` — G shapes + capo 4 sound in B.
pub fn shape_key_for_capo(sounding: Key, capo_fret: u8) -> Key {
    let down = (12 - (capo_fret % 12)) % 12;
    transpose_key_by(&sounding, down)
}

/// The capo fret at which fingering `shape` sounds `sounding`: the semitone
/// distance UP from `shape` to `sounding`, in `0..=11` (`None` never occurs
/// for in-octave keys, but the signature leaves room for future guards).
///
/// `capo_to_play(B, G) == Some(4)` — G shapes need a capo at 4 to sound B.
pub fn capo_to_play(sounding: Key, shape: Key) -> Option<u8> {
    let n = (sounding.root.semitone + 12 - shape.root.semitone) % 12;
    (n <= 11).then_some(n)
}

/// The key the letters/shapes are drawn in for `view`, relative to a chart's own
/// `song_key`. With a capo this is the shape key (what the hands finger);
/// otherwise the sounding target (or the song key when the view sets none). This
/// is the single source of truth shared by [`apply_view`] and [`transpose_source`].
fn display_key_for(song_key: &Key, view: &ChartView) -> Key {
    match (&view.target_key, view.capo) {
        (Some(t), capo) if capo > 0 => shape_key_for_capo(t.clone(), capo),
        (Some(t), _) => t.clone(),
        (None, _) => song_key.clone(),
    }
}

// ===========================================================================
// apply_view
// ===========================================================================

/// Apply a [`ChartView`] to `chart`, returning a NEW chart with every chord
/// symbol transposed + re-spelled to the requested key and notation. The input
/// chart is never mutated. The returned chart's declared key metadata reflects
/// the **displayed** (shape) key.
pub fn apply_view(chart: &Chart, view: &ChartView) -> Chart {
    // Fast path: the pure default view is the identity.
    if view.target_key.is_none() && view.capo == 0 && view.notation == NotationSystem::Letters {
        return chart.clone();
    }

    // The song's own functional key — the reference for scale-degree math and
    // the source of the transposition interval. Fall back to the target key,
    // then C major, so key-less charts still render.
    let song_key = chart
        .current_key
        .clone()
        .or_else(|| chart.initial_key.clone())
        .or_else(|| view.target_key.clone())
        .unwrap_or_else(|| Key::major(MusicalNote::c()));

    // The key the letters are drawn in. With a capo this is the shape key
    // (what the hands finger); otherwise the sounding target (or the song key).
    let display_key = display_key_for(&song_key, view);

    // Semitone shift from the song key to the displayed (letters) key.
    let letters_delta = (display_key.root.semitone + 12 - song_key.root.semitone) % 12;

    let ctx = Ctx {
        song_key: &song_key,
        display_key: &display_key,
        letters_delta,
        notation: view.notation,
    };

    let mut out = chart.clone();

    for section in &mut out.sections {
        for track in &mut section.tracks {
            for measure in &mut track.measures {
                for chord in &mut measure.chords {
                    rewrite_chord(chord, &ctx);
                }
                for element in &mut measure.rhythm_elements {
                    if let RhythmElement::Chord(chord) = element {
                        rewrite_chord(chord, &ctx);
                    }
                }
            }
        }
    }

    // Metadata / key changes carry the displayed key.
    let meta_delta = letters_delta;
    out.initial_key = Some(display_key.clone());
    out.current_key = Some(display_key.clone());
    out.ending_key = chart
        .ending_key
        .as_ref()
        .map(|k| transpose_key_by(k, meta_delta));
    for kc in &mut out.key_changes {
        rewrite_key_change(kc, meta_delta);
    }

    out
}

struct Ctx<'a> {
    song_key: &'a Key,
    display_key: &'a Key,
    letters_delta: u8,
    notation: NotationSystem,
}

/// Rewrite a single chord in place to the view's notation. Placeholder /
/// rootless entries (rests, spaces, floating rhythm) are left untouched.
fn rewrite_chord(chord: &mut ChordInstance, ctx: &Ctx) {
    let Some((parsed, symbol)) = renotate_chord(&chord.parsed, ctx) else {
        return;
    };
    chord.root = parsed.root.clone();
    chord.full_symbol = symbol;
    chord.parsed = parsed;
    chord.display_override = None;
}

/// The core per-chord transform shared by every entry point: given a parsed
/// chord and a [`Ctx`], produce the transposed / re-notated `Chord` (structured)
/// and its rendered symbol string. Returns `None` for rootless placeholders
/// (rests, spaces, floating rhythm) which carry no transposable root.
///
/// This is the ONE place chord transposition + notation lives, so
/// [`apply_view`] (structured charts) and [`transpose_source`] (raw source)
/// can never diverge.
fn renotate_chord(parsed_in: &Chord, ctx: &Ctx) -> Option<(Chord, String)> {
    let root_note = parsed_in.root.resolve(Some(ctx.song_key))?;

    let bass_note = parsed_in
        .bass
        .as_ref()
        .and_then(|b| b.resolve(Some(ctx.song_key)));

    match ctx.notation {
        NotationSystem::Letters => {
            let new_root = notate_root(&root_note, ctx);
            let new_bass = bass_note.as_ref().map(|b| notate_root(b, ctx));

            let mut parsed = parsed_in.clone();
            parsed.root = new_root;
            parsed.bass = new_bass;

            let symbol = parsed.to_string();
            Some((parsed, symbol))
        }
        NotationSystem::Nashville => {
            let new_root = notate_root(&root_note, ctx);
            let new_bass = bass_note.as_ref().map(|b| notate_root(b, ctx));

            let mut parsed = parsed_in.clone();
            parsed.root = new_root;
            parsed.bass = new_bass;

            // A bare Nashville number already carries its degree's diatonic
            // triad quality (`2` = ii minor in a major key). So for a PLAIN
            // major/minor triad whose quality is the diatonic one, drop the
            // explicit quality marker (`F#m` → `6`, `Bm` → `2`). Any seventh /
            // extension / addition / alteration / suspension keeps the full
            // quality string (`Bm7` → `2m7`), a deviant quality keeps its marker
            // (`Am` in C → `1m`), and diminished / augmented triads always keep
            // their `°` / `+`.
            let (deg, _acc) = degree_of(ctx.song_key, &root_note);
            let is_diatonic_triad = is_plain_triad(parsed_in)
                && matches!(parsed_in.quality, ChordQuality::Major | ChordQuality::Minor)
                && ctx.song_key.diatonic_quality(deg) == Some(parsed_in.quality);
            if is_diatonic_triad {
                parsed.quality = ChordQuality::Major;
            }

            let symbol = parsed.to_string();
            Some((parsed, symbol))
        }
        NotationSystem::Roman => {
            let (deg, acc) = degree_of(ctx.song_key, &root_note);
            let case = roman_case(parsed_in.quality);
            let numeral = RootNotation::from_roman_numeral_with_accidental(deg, case, acc);

            let mut symbol = numeral.to_string();
            symbol.push_str(&roman_tail(parsed_in));
            if let Some(b) = &bass_note {
                let (bdeg, bacc) = degree_of(ctx.song_key, b);
                symbol.push('/');
                symbol.push_str(&RootNotation::from_scale_degree(bdeg, bacc).to_string());
            }

            let mut parsed = parsed_in.clone();
            parsed.root = numeral;
            if let Some(b) = &bass_note {
                let (bdeg, bacc) = degree_of(ctx.song_key, b);
                parsed.bass = Some(RootNotation::from_scale_degree(bdeg, bacc));
            }

            Some((parsed, symbol))
        }
    }
}

/// True when `chord` is a plain triad — no seventh family, extension, addition,
/// alteration, or omission. Only plain triads are eligible to drop their
/// Nashville quality marker.
fn is_plain_triad(chord: &Chord) -> bool {
    chord.family.is_none()
        && !chord.extensions.has_any()
        && chord.alterations.is_empty()
        && chord.additions.is_empty()
        && chord.omissions.is_empty()
}

/// Parse a single chord SYMBOL (e.g. `"F#m"`, `"Bm7/A"`) and re-spell it under
/// `view`, treating `from_key` as the song's own functional key. Returns the
/// rendered symbol, or `None` when the token is not a chord (unparseable, or a
/// rootless placeholder). This is the source-string counterpart to
/// [`rewrite_chord`]; both flow through [`renotate_chord`].
fn transpose_chord_symbol(symbol: &str, from_key: &Key, view: &ChartView) -> Option<String> {
    let mut lexer = Lexer::new(symbol.to_string());
    let parsed = Chord::parse(&lexer.tokenize()).ok()?;
    // Require a resolvable root so bare rhythm / non-chord words are rejected.
    parsed.root.resolve(Some(from_key))?;

    let display_key = display_key_for(from_key, view);
    let letters_delta = (display_key.root.semitone + 12 - from_key.root.semitone) % 12;
    let ctx = Ctx {
        song_key: from_key,
        display_key: &display_key,
        letters_delta,
        notation: view.notation,
    };
    renotate_chord(&parsed, &ctx).map(|(_, symbol)| symbol)
}

/// One chord symbol, as written, spelled in `notation` for a song in `key`:
/// `"5/7"` in F is `"C/E"` as letters, `"Bb"` in F is `"4"` as a number.
///
/// No transposition — the chord sounds where it did, only its spelling
/// changes. `None` when the symbol is not a chord (a rest, a word, a root
/// that does not resolve in `key`). The single-chord entry to the same
/// transform [`apply_view`] runs over a whole chart, for a caller holding
/// symbols rather than a chart: a DAW's chord items, say, named as
/// written.
///
/// A bare number takes the key's diatonic quality, as the chart parser
/// gives it: `6` in F is D MINOR, so `Dm` as letters. A chart keeps the
/// terse `6` as the chord's symbol, which is what a caller holding
/// symbols has — so without this, re-reading the symbol alone would
/// hear a D major the chart never wrote.
#[must_use]
pub fn renotate_symbol(symbol: &str, key: &Key, notation: NotationSystem) -> Option<String> {
    let mut lexer = Lexer::new(symbol.to_string());
    // A symbol that starts with a number is written in numbers, so an
    // ambiguous `b<digit>` inside it is a flat degree (`b7` = ♭7), not the
    // note B with a 7 on it — which is what the chart parser reads it as in
    // a numbers chart too.
    let system = if is_number_symbol(symbol) {
        crate::chord::root::NotationSystem::Degree
    } else {
        crate::chord::root::NotationSystem::Auto
    };
    let mut parsed = Chord::parse_with_system(&lexer.tokenize(), system).ok()?;
    parsed.root.resolve(Some(key))?;
    if let Some(degree) = parsed.root.diatonic_scale_degree()
        && parsed.quality == ChordQuality::Major
        && !writes_major(symbol)
        && let Some(implied) = key.diatonic_quality(degree)
        && implied != ChordQuality::Major
    {
        parsed.set_triad_quality(implied);
    }
    let ctx = Ctx {
        song_key: key,
        display_key: key,
        letters_delta: 0,
        notation,
    };
    renotate_chord(&parsed, &ctx).map(|(_, symbol)| symbol)
}

/// Whether a symbol is written in numbers: a scale degree, with or without
/// an accidental in front (`5/7`, `b7`, `#4m`).
fn is_number_symbol(symbol: &str) -> bool {
    let rest = symbol.trim_start_matches(['b', '#', '♭', '♯']);
    rest.len() < symbol.len() && rest.starts_with(|c: char| ('1'..='7').contains(&c))
        || symbol.starts_with(|c: char| ('1'..='7').contains(&c))
}

/// Whether a number chord says MAJOR in so many words (`6M`, `6maj`,
/// `6:maj7`, `6^`) — the chart parser's test, so a written major is
/// never overridden by the key's diatonic quality.
fn writes_major(symbol: &str) -> bool {
    let rest = symbol.trim_start_matches(['#', 'b']);
    let rest = rest.trim_start_matches(|c: char| c.is_ascii_digit());
    let rest = rest.strip_prefix(':').unwrap_or(rest);
    rest.starts_with('M')
        || rest.starts_with("maj")
        || rest.starts_with("Maj")
        || rest.starts_with('△')
        || rest.starts_with('^')
}

/// Build the root notation for a resolved note under Letters or Nashville.
fn notate_root(note: &MusicalNote, ctx: &Ctx) -> RootNotation {
    match ctx.notation {
        NotationSystem::Letters => {
            if ctx.letters_delta == 0 {
                // No transposition — keep the note's authored spelling so a
                // non-diatonic `Bb` stays `Bb` rather than being re-spelled to
                // the key's chromatic default (`A#` in C).
                RootNotation::from_note_name(note.clone())
            } else {
                let transposed = (note.semitone + ctx.letters_delta) % 12;
                let spelled = spell_in_key(ctx.display_key, transposed);
                RootNotation::from_note_name(spelled)
            }
        }
        NotationSystem::Nashville => {
            let (deg, acc) = degree_of(ctx.song_key, note);
            RootNotation::from_scale_degree(deg, acc)
        }
        // Roman handled separately in rewrite_chord.
        NotationSystem::Roman => {
            let (deg, acc) = degree_of(ctx.song_key, note);
            RootNotation::from_scale_degree(deg, acc)
        }
    }
}

// ===========================================================================
// transpose_source — the SOURCE-level transform
// ===========================================================================

/// Re-spell a keyflow SOURCE string for display under `view` — transpose chord
/// tokens to the target / shape key, render them as letters / Nashville numbers
/// / Roman numerals, and update the `#<key>` metadata line — while preserving
/// EVERYTHING else byte-for-byte: the title, tempo/meter, section headers,
/// rhythm marks, repeat markers, comments, blank lines and original spacing.
///
/// The input is never mutated. The identity view (`target_key` `None`, Letters,
/// capo `0`) returns the source unchanged.
///
/// Only chord tokens on chord lines change. A line is treated as a header (and
/// passed through untouched) when it is blank, is the `#<key>` metadata line, or
/// parses as a section marker ([`SectionType::parse_with_measure_count`], which
/// only matches known multi-letter section keywords — never a chord line). Of
/// the remaining lines, a line is rewritten only when *every* token on it is a
/// chord, a rhythm mark (`////`, `//`, `|`), or a repeat marker (`x2`) AND at
/// least one is a real chord; the title and comment lines fail that test and
/// pass through. Chord tokens are transposed via [`transpose_chord_symbol`], the
/// same code path [`apply_view`] uses.
pub fn transpose_source(source: &str, view: &ChartView) -> String {
    // Fast path: the pure default view is the identity — byte-for-byte input.
    if view.target_key.is_none() && view.capo == 0 && view.notation == NotationSystem::Letters {
        return source.to_string();
    }

    // The song's own functional key — the reference for scale-degree math and
    // the transposition interval — read from the `#<key>` metadata line. Fall
    // back to C major so key-less source still renders.
    let song_key = find_meta_key(source).unwrap_or_else(|| Key::major(MusicalNote::c()));
    let display_key = display_key_for(&song_key, view);

    // Rebuild line by line, preserving every line ending (`\n`, `\r\n`, and the
    // possibly-missing final newline) exactly.
    let mut out = String::with_capacity(source.len());
    for piece in source.split_inclusive('\n') {
        let (content, newline) = match piece.strip_suffix('\n') {
            Some(rest) => (rest, "\n"),
            None => (piece, ""),
        };
        out.push_str(&transform_source_line(
            content,
            &song_key,
            &display_key,
            view,
        ));
        out.push_str(newline);
    }
    out
}

/// The song key declared on the first `#<key>` metadata line, if any.
fn find_meta_key(source: &str) -> Option<Key> {
    for line in source.lines() {
        if let Some(rest) = line.trim_start().strip_prefix('#') {
            let token = rest.split_whitespace().next().unwrap_or("");
            return Key::parse(token).ok();
        }
    }
    None
}

/// Transform a single source line (without its trailing newline).
fn transform_source_line(
    content: &str,
    song_key: &Key,
    display_key: &Key,
    view: &ChartView,
) -> String {
    let trimmed = content.trim();

    // Blank line — nothing to do.
    if trimmed.is_empty() {
        return content.to_string();
    }

    // `#<key>` metadata line — rewrite the key token to the displayed key.
    if trimmed.starts_with('#') {
        return rewrite_meta_line(content, display_key, view);
    }

    // Section header (`VS 8`, `Interlude "Breakdown" 8`, `INST "Guitar" 8`, …).
    // `parse_with_measure_count` only matches known multi-letter section
    // keywords, so no chord line is ever misclassified here.
    if SectionType::parse_with_measure_count(trimmed).is_some() {
        return content.to_string();
    }

    // Otherwise: a chord line (or a title / comment that passes through).
    rewrite_chord_line(content, song_key, view)
}

/// Rewrite the `#<key>` metadata line's key token to `display_key`, preserving
/// the `#`, the tempo/meter, and all spacing. When the view sets no target key,
/// the line is a byte-for-byte pass-through (the key stays the reference key the
/// numbers / letters are relative to).
fn rewrite_meta_line(content: &str, display_key: &Key, view: &ChartView) -> String {
    if view.target_key.is_none() {
        return content.to_string();
    }

    // Locate the `#` and the end of the key token (`#A` / `#Bbm`).
    let Some(hash) = content.find('#') else {
        return content.to_string();
    };
    let after = &content[hash + 1..];
    let key_len = after
        .find(|c: char| c.is_whitespace())
        .unwrap_or(after.len());
    let token_end = hash + 1 + key_len;

    let mut out = String::with_capacity(content.len());
    out.push_str(&content[..hash]);
    out.push('#');
    out.push_str(&display_key.short_name());
    out.push_str(&content[token_end..]);
    out
}

/// Rewrite the chord tokens on a chord line, leaving rhythm marks, repeat
/// markers and all original whitespace untouched. If the line is not a pure
/// chord/rhythm line (i.e. any token is neither a chord nor a rhythm/repeat
/// mark, or it has no chord at all), the line is returned unchanged — this is
/// what keeps titles and comment lines safe.
fn rewrite_chord_line(content: &str, song_key: &Key, view: &ChartView) -> String {
    let spans = token_spans(content);
    if spans.is_empty() {
        return content.to_string();
    }

    // Classify every token; bail out (pass through) at the first token that is
    // neither a chord nor a rhythm/repeat mark.
    let mut replacements: Vec<Option<String>> = Vec::with_capacity(spans.len());
    let mut has_chord = false;
    for &(start, end) in &spans {
        let token = &content[start..end];
        if is_rhythm_mark(token) || is_repeat_mark(token) {
            replacements.push(None);
        } else if let Some(new) = transpose_chord_symbol(token, song_key, view) {
            replacements.push(Some(new));
            has_chord = true;
        } else {
            return content.to_string();
        }
    }
    if !has_chord {
        return content.to_string();
    }

    // Rebuild, copying inter-token whitespace verbatim.
    let mut out = String::with_capacity(content.len());
    let mut cursor = 0;
    for (i, &(start, end)) in spans.iter().enumerate() {
        out.push_str(&content[cursor..start]);
        match &replacements[i] {
            Some(new) => out.push_str(new),
            None => out.push_str(&content[start..end]),
        }
        cursor = end;
    }
    out.push_str(&content[cursor..]);
    out
}

/// Byte spans of each whitespace-delimited token in `line`.
fn token_spans(line: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in line.char_indices() {
        if c.is_whitespace() {
            if let Some(s) = start.take() {
                spans.push((s, i));
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        spans.push((s, line.len()));
    }
    spans
}

/// A rhythm mark: a run of slashes / bar-lines (`////`, `//`, `|`, `|/`).
fn is_rhythm_mark(token: &str) -> bool {
    !token.is_empty() && token.chars().all(|c| c == '/' || c == '|')
}

/// A repeat marker: `x2`, `x3`, `X4`, … (an `x` followed by digits).
fn is_repeat_mark(token: &str) -> bool {
    let mut chars = token.chars();
    matches!(chars.next(), Some('x' | 'X')) && {
        let rest = chars.as_str();
        !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit())
    }
}

fn rewrite_key_change(kc: &mut KeyChange, delta: u8) {
    kc.to_key = transpose_key_by(&kc.to_key, delta);
    if let Some(from) = &kc.from_key {
        kc.from_key = Some(transpose_key_by(from, delta));
    }
}

// ===========================================================================
// Notation primitives
// ===========================================================================

/// Spell a semitone (0-11) with the correct enharmonic for `key` — so G major
/// gives `C` not `B#`, and flat keys give flats. Uses relaxed spelling to
/// avoid double accidentals.
fn spell_in_key(key: &Key, semitone: u8) -> MusicalNote {
    let is_major = key.mode == ScaleMode::ionian();
    let spelling = KeySpelling::new(&key.root, is_major);
    spelling.spell(semitone, SpellingMode::Relaxed).to_note()
}

/// The scale degree (1-7) and any accidental of `note` relative to `key`.
/// Diatonic notes return `(degree, None)`; chromatic notes borrow the nearest
/// scale degree, preferring a flat spelling (`Bb` in C → `(7, Flat)` = `b7`).
fn degree_of(key: &Key, note: &MusicalNote) -> (u8, Option<Accidental>) {
    let rel = (note.semitone + 12 - key.root.semitone) % 12;

    // Relative semitone of each of the 7 diatonic degrees.
    let mut naturals = [(0u8, 0u8); 7];
    for d in 1..=7u8 {
        if let Some(n) = key.get_scale_degree(d) {
            naturals[(d - 1) as usize] = (d, (n.semitone + 12 - key.root.semitone) % 12);
        }
    }

    // Exact scale tone.
    for (d, r) in naturals {
        if r == rel {
            return (d, None);
        }
    }
    // A semitone below a scale degree → flatted that degree (preferred).
    for (d, r) in naturals {
        if (rel + 1) % 12 == r {
            return (d, Some(Accidental::Flat));
        }
    }
    // A semitone above a scale degree → sharped that degree.
    for (d, r) in naturals {
        if (rel + 11) % 12 == r {
            return (d, Some(Accidental::Sharp));
        }
    }
    (1, None)
}

/// The Roman-numeral case for a chord quality: uppercase for major-ish
/// (major / augmented / suspended / power), lowercase for minor-ish
/// (minor / diminished).
fn roman_case(quality: ChordQuality) -> RomanCase {
    match quality {
        ChordQuality::Minor | ChordQuality::Diminished => RomanCase::Lower,
        _ => RomanCase::Upper,
    }
}

/// Everything a Roman numeral carries *after* the numeral itself: the triad
/// decoration (`°`, `+`, `sus4`) plus any seventh / extension / alteration /
/// addition / omission tail. The major/minor third is conveyed by the
/// numeral's case, so no `m` is emitted here.
fn roman_tail(parsed: &Chord) -> String {
    let mut out = String::new();

    // Triad decoration. `°`/`+` only when there is no seventh family (a
    // seventh family renders its own symbol in the tail below).
    let decoration = if parsed.family.is_none() {
        match parsed.quality {
            ChordQuality::Diminished => "°",
            ChordQuality::Augmented => "+",
            ChordQuality::Suspended(SuspendedType::Second) => "sus2",
            ChordQuality::Suspended(SuspendedType::Fourth) => "sus4",
            _ => "",
        }
    } else {
        match parsed.quality {
            ChordQuality::Suspended(SuspendedType::Second) => "sus2",
            ChordQuality::Suspended(SuspendedType::Fourth) => "sus4",
            _ => "",
        }
    };
    out.push_str(decoration);

    // Seventh / extensions / alterations / additions / omissions. We render a
    // quality-neutralised clone (quality forced Major, so no base `m`/`dim`/`+`
    // leaks in) with an empty root and no bass, which yields exactly the tail.
    let mut neutral = parsed.clone();
    neutral.quality = ChordQuality::Major;
    neutral.root = RootNotation::empty();
    neutral.bass = None;
    out.push_str(&neutral.to_string());

    out
}

// ===========================================================================
// Key transposition
// ===========================================================================

/// Transpose a key up by `delta` semitones (0-11), preserving its mode and
/// choosing the conventional spelling for the new root.
fn transpose_key_by(key: &Key, delta: u8) -> Key {
    let new_semitone = (key.root.semitone + delta) % 12;
    let root = preferred_key_root(new_semitone);
    Key::new(root, key.mode)
}

/// The conventional spelling of a major/minor key root by semitone: flats for
/// the black keys except `F#` (the usual notated choice).
fn preferred_key_root(semitone: u8) -> MusicalNote {
    let name = match semitone % 12 {
        0 => "C",
        1 => "Db",
        2 => "D",
        3 => "Eb",
        4 => "E",
        5 => "F",
        6 => "F#",
        7 => "G",
        8 => "Ab",
        9 => "A",
        10 => "Bb",
        11 => "B",
        _ => unreachable!(),
    };
    MusicalNote::from_string(name).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::types::{ChartSection, Measure};
    use crate::chord::ChordRhythm;
    use crate::sections::{Section, SectionType};
    use crate::time::{AbsolutePosition, MusicalDuration, MusicalPosition};
    use keyflow_syntax::parsing::Lexer;

    /// One symbol either way round, the way a DAW's chord items are
    /// relabelled: numbers to names and names to numbers, slash chords and
    /// qualities kept.
    #[test]
    fn renotate_symbol_both_ways() {
        let f = Key::major(MusicalNote::from_string("F").expect("F"));
        let as_letters = |s| renotate_symbol(s, &f, NotationSystem::Letters);
        let as_numbers = |s| renotate_symbol(s, &f, NotationSystem::Nashville);
        assert_eq!(as_letters("1").as_deref(), Some("F"));
        assert_eq!(as_letters("6m").as_deref(), Some("Dm"));
        // A bare number is the key's own chord: 6 is minor, 7 diminished,
        // unless the chart said major outright.
        assert_eq!(as_letters("6").as_deref(), Some("Dm"));
        assert_eq!(as_letters("2/5").as_deref(), Some("Gm/C"));
        assert_eq!(as_letters("6M").as_deref(), Some("D"));
        assert_eq!(as_numbers("6").as_deref(), Some("6"));
        // A flat degree, not the note B: b7 in F is Eb.
        assert_eq!(as_letters("b7").as_deref(), Some("Eb"));
        assert_eq!(as_numbers("b7").as_deref(), Some("b7"));
        assert_eq!(as_numbers("b7/1").as_deref(), Some("b7/1"));
        // And a letter chord is still a letter chord.
        assert_eq!(as_numbers("B7").as_deref(), Some("b57"));
        assert_eq!(as_letters("4").as_deref(), Some("Bb"));
        assert_eq!(as_letters("5/7").as_deref(), Some("C/E"));
        assert_eq!(as_numbers("Bb").as_deref(), Some("4"));
        assert_eq!(as_numbers("C/E").as_deref(), Some("5/7"));
        assert_eq!(as_numbers("Dm7").as_deref(), Some("6m7"));
        assert_eq!(as_letters("hello"), None);
    }

    /// Build a single-section, single-measure chart in `key` from a list of
    /// chord source tokens (letters, Nashville, or Roman — parsed via the
    /// shared chord parser). Avoids the keyflow-text dev-dep so there is only
    /// one `keyflow-proto` in the graph.
    fn chart_in(key: Key, chords: &[&str]) -> Chart {
        let mut chart = Chart::new();
        chart.initial_key = Some(key.clone());
        chart.current_key = Some(key);

        let mut measure = Measure::new();
        for (i, src) in chords.iter().enumerate() {
            let mut lexer = Lexer::new((*src).to_string());
            let parsed = Chord::parse(&lexer.tokenize()).expect("chord should parse");
            let ci = ChordInstance::new(
                parsed.root.clone(),
                (*src).to_string(),
                parsed,
                ChordRhythm::Default,
                (*src).to_string(),
                MusicalDuration::new(0, 1, 0),
                AbsolutePosition::new(MusicalPosition::new(0, i as i32, 0), 0),
            );
            measure.chords.push(ci);
        }

        let section =
            ChartSection::new(Section::new(SectionType::Verse)).with_measures(vec![measure]);
        chart.sections.push(section);
        chart
    }

    /// Collect the visible chord `full_symbol`s across a chart, in order.
    fn symbols(chart: &Chart) -> Vec<String> {
        let mut out = Vec::new();
        for section in &chart.sections {
            for track in &section.tracks {
                for measure in &track.measures {
                    for chord in &measure.chords {
                        if !chord.full_symbol.is_empty()
                            && chord.full_symbol != "s"
                            && chord.full_symbol != "r"
                        {
                            out.push(chord.full_symbol.clone());
                        }
                    }
                }
            }
        }
        out
    }

    fn key(s: &str) -> Key {
        Key::parse(s).unwrap()
    }

    #[test]
    fn letters_a_to_g() {
        // A D E F#m in A major, transposed to sound in G.
        let c = chart_in(key("A"), &["A", "D", "E", "F#m"]);
        let view = ChartView {
            target_key: Some(key("G")),
            notation: NotationSystem::Letters,
            capo: 0,
        };
        let out = apply_view(&c, &view);
        assert_eq!(symbols(&out), vec!["G", "C", "D", "Em"]);
        // Metadata reflects the displayed key.
        assert_eq!(out.initial_key, Some(key("G")));
    }

    #[test]
    fn nashville_in_g() {
        let c = chart_in(key("G"), &["G", "C", "D", "Em"]);
        let view = ChartView {
            target_key: None,
            notation: NotationSystem::Nashville,
            capo: 0,
        };
        let out = apply_view(&c, &view);
        // A bare diatonic minor triad (vi = Em) drops its `m` → `6`.
        assert_eq!(symbols(&out), vec!["1", "4", "5", "6"]);
    }

    #[test]
    fn nashville_diatonic_triads_drop_marker() {
        // In A: plain diatonic minor triads drop the `m`; sevenths/extensions
        // keep it; a non-diatonic minor triad keeps it; slash bass unaffected.
        let c = chart_in(
            key("A"),
            &[
                "A", "D", "E", "F#m", "Bm", "C#m", "Bm7", "F#m7", "Asus4", "Amaj7", "A/C#", "Bm7/A",
            ],
        );
        let view = ChartView {
            target_key: None,
            notation: NotationSystem::Nashville,
            capo: 0,
        };
        let out = apply_view(&c, &view);
        assert_eq!(
            symbols(&out),
            vec![
                "1", "4", "5", "6", "2", "3", "2m7", "6m7", "1sus4", "1maj7", "1/3", "2m7/1",
            ]
        );

        // A deviant-quality triad keeps its marker: a minor one-chord (`Cm` in
        // C, where the diatonic I is major) → `1m`.
        let cc = chart_in(key("C"), &["Cm"]);
        let out_c = apply_view(&cc, &view);
        assert_eq!(symbols(&out_c), vec!["1m"]);
    }

    #[test]
    fn roman_in_g() {
        let c = chart_in(key("G"), &["G", "C", "D", "Em"]);
        let view = ChartView {
            target_key: None,
            notation: NotationSystem::Roman,
            capo: 0,
        };
        let out = apply_view(&c, &view);
        assert_eq!(symbols(&out), vec!["I", "IV", "V", "vi"]);
    }

    #[test]
    fn slash_chord_transpose_bass() {
        // A/C# in A major → G/B when sounding in G.
        let c = chart_in(key("A"), &["A/C#", "D"]);
        let view = ChartView {
            target_key: Some(key("G")),
            notation: NotationSystem::Letters,
            capo: 0,
        };
        let out = apply_view(&c, &view);
        assert_eq!(symbols(&out), vec!["G/B", "C"]);
    }

    #[test]
    fn capo_helpers() {
        // Sound B using G shapes → capo 4.
        assert_eq!(capo_to_play(key("B"), key("G")), Some(4));
        assert_eq!(shape_key_for_capo(key("B"), 4), key("G"));
    }

    #[test]
    fn capo_view_renders_shape_key() {
        // Chart in B, sound in B with a capo 4 → render G shapes.
        let c = chart_in(key("B"), &["B", "E", "F#"]);
        let view = ChartView {
            target_key: Some(key("B")),
            notation: NotationSystem::Letters,
            capo: 4,
        };
        let out = apply_view(&c, &view);
        assert_eq!(symbols(&out), vec!["G", "C", "D"]);
        assert_eq!(view.capo_caption().as_deref(), Some("Capo 4 (sounds B)"));
        assert_eq!(view.shape_key(), Some(key("G")));
    }

    #[test]
    fn identity_view_is_equivalent() {
        let c = chart_in(key("C"), &["C", "G", "Am", "F"]);
        let view = ChartView::default();
        let out = apply_view(&c, &view);
        assert_eq!(symbols(&out), symbols(&c));
        assert_eq!(out.initial_key, c.initial_key);
    }

    #[test]
    fn non_diatonic_chord_renders_all_notations() {
        // Bb (bVII) in the key of C — non-diatonic, must not panic or drop.
        let c = chart_in(key("C"), &["C", "Bb"]);

        let letters = apply_view(
            &c,
            &ChartView {
                target_key: Some(key("C")),
                notation: NotationSystem::Letters,
                capo: 0,
            },
        );
        assert_eq!(symbols(&letters), vec!["C", "Bb"]);

        let nashville = apply_view(
            &c,
            &ChartView {
                target_key: None,
                notation: NotationSystem::Nashville,
                capo: 0,
            },
        );
        assert_eq!(symbols(&nashville), vec!["1", "b7"]);

        let roman = apply_view(
            &c,
            &ChartView {
                target_key: None,
                notation: NotationSystem::Roman,
                capo: 0,
            },
        );
        assert_eq!(symbols(&roman), vec!["I", "bVII"]);
    }

    // -----------------------------------------------------------------------
    // transpose_source — the SOURCE-level transform
    // -----------------------------------------------------------------------

    /// The task's sample song, in A.
    const SAMPLE: &str = "Praise - Elevation Worship\n\
#A 127bpm 4/4\n\
Count 2\n\
In 4\n\
Refrain 8\n\
VS 8\n\
A //// A // D // A // A A\n\
E D A A\n\
PRE 2\n\
E D\n\
CH 8\n\
F#m D A E x2\n\
Interlude \"Breakdown\" 8\n\
BR \"Down\" 8\n\
A Bm7/A C#m/A D/A x2\n";

    #[test]
    fn source_identity_is_byte_for_byte() {
        let out = transpose_source(SAMPLE, &ChartView::default());
        assert_eq!(out, SAMPLE);

        // Missing trailing newline is preserved too.
        let no_nl = "#A 127bpm 4/4\nA D E";
        assert_eq!(transpose_source(no_nl, &ChartView::default()), no_nl);
    }

    #[test]
    fn source_letters_a_to_g() {
        let view = ChartView {
            target_key: Some(key("G")),
            notation: NotationSystem::Letters,
            capo: 0,
        };
        let out = transpose_source(SAMPLE, &view);

        let expected = "Praise - Elevation Worship\n\
#G 127bpm 4/4\n\
Count 2\n\
In 4\n\
Refrain 8\n\
VS 8\n\
G //// G // C // G // G G\n\
D C G G\n\
PRE 2\n\
D C\n\
CH 8\n\
Em C G D x2\n\
Interlude \"Breakdown\" 8\n\
BR \"Down\" 8\n\
G Am7/G Bm/G C/G x2\n";

        assert_eq!(out, expected);
    }

    #[test]
    fn source_nashville_in_own_key() {
        // target None → numbers relative to the song's own key (A); `#A` unchanged.
        let view = ChartView {
            target_key: None,
            notation: NotationSystem::Nashville,
            capo: 0,
        };
        let out = transpose_source(SAMPLE, &view);

        // `#A` line is untouched, headers/rhythm untouched.
        assert!(out.contains("#A 127bpm 4/4"));
        assert!(out.contains("VS 8"));
        assert!(out.contains("Interlude \"Breakdown\" 8"));
        // A D E → 1 4 5, F#m → 6 (diatonic minor triad drops `m`); rhythm + x2 preserved.
        assert!(out.contains("1 //// 1 // 4 // 1 // 1 1"));
        assert!(out.contains("5 4 1 1"));
        assert!(out.contains("6 4 1 5 x2"));
        // A Bm7/A C#m/A D/A → in A: B=2 (kept m7), C#=3 (plain minor → drop m),
        // D=4, bass A=1.
        assert!(out.contains("1 2m7/1 3/1 4/1 x2"));
    }

    #[test]
    fn source_roman_in_own_key() {
        let view = ChartView {
            target_key: None,
            notation: NotationSystem::Roman,
            capo: 0,
        };
        let out = transpose_source(SAMPLE, &view);

        assert!(out.contains("#A 127bpm 4/4"));
        // A D E → I IV V, F#m → vi.
        assert!(out.contains("I //// I // IV // I // I I"));
        assert!(out.contains("V IV I I"));
        assert!(out.contains("vi IV I V x2"));
    }

    #[test]
    fn source_capo_renders_shape_key() {
        // Sound B with a capo 4 → finger G shapes; `#` line reads `#G`.
        let view = ChartView {
            target_key: Some(key("B")),
            notation: NotationSystem::Letters,
            capo: 4,
        };
        let out = transpose_source(SAMPLE, &view);

        assert!(out.contains("#G 127bpm 4/4"));
        // Same G-shape chords as the direct A→G case.
        assert!(out.contains("G //// G // C // G // G G"));
        assert!(out.contains("Em C G D x2"));
        assert!(out.contains("G Am7/G Bm/G C/G x2"));
    }

    #[test]
    fn source_section_header_with_chord_like_label_untouched() {
        // An INST label whose text contains bare `A`/`E` tokens must not be
        // transposed — the section keyword guards the whole line.
        let src = "#A 127bpm 4/4\n\
INST \"A E Lead\" 8\n\
A D E\n";
        let view = ChartView {
            target_key: Some(key("G")),
            notation: NotationSystem::Letters,
            capo: 0,
        };
        let out = transpose_source(src, &view);

        // Header line is byte-for-byte; only the chord line moved A D E → G C D.
        assert!(out.contains("INST \"A E Lead\" 8"));
        assert!(out.contains("G C D"));
        assert!(!out.contains("A D E"));
    }

    #[test]
    fn source_title_line_untouched() {
        // A title containing a bare chord-like token still passes through: it is
        // not an all-chord line.
        let src = "A Mighty Fortress\n#A 127bpm 4/4\nA D\n";
        let view = ChartView {
            target_key: Some(key("G")),
            notation: NotationSystem::Letters,
            capo: 0,
        };
        let out = transpose_source(src, &view);
        assert!(out.starts_with("A Mighty Fortress\n"));
        assert!(out.contains("G C\n"));
    }
}
