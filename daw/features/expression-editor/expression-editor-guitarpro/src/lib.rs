//! Guitar Pro import (#168).
//!
//! A GP file becomes a six-string roll: **string as row, fret as label,
//! the file's own tuning driving the row space** rather than a default
//! six-string. Bends become per-note curves, which is why this is cheap
//! — the editor already carries a continuous curve per note in
//! semitones relative to its row, so a bend needs no new
//! representation (#160).
//!
//! ## Why a dependency and not a parser
//!
//! #160 settled it: `guitarpro` is MIT, safe Rust, and reads both
//! families — the flat `gp3/gp4/gp5` binaries and the GPIF-wrapping
//! `.gpx`/`.gp`. Writing a second one would be a month of format
//! archaeology for no gain.
//!
//! It is **pinned exactly**, and our own types stay at this boundary:
//! one maintainer, 0.x, and a self-described experimental GPIF path
//! mean a change over there should be a conversion change here, not a
//! change to the editor's model.
//!
//! ## The one thing the dependency loses
//!
//! Its GPIF reader takes only `BendOriginValue` and
//! `BendDestinationValue`, synthesising a three-point curve and
//! discarding both middle values and all offsets — which is exactly the
//! data a bend's *shape* lives in. Binary gp3/4/5 files carry full
//! point lists and come through intact. [`BendFidelity`] reports which
//! you got, so a scenario built on GPIF bends knows it is looking at a
//! straight line rather than the curve that was authored.

pub mod parse;

use expression_editor_core::doc::{Curve, Dimension, ExpressionDoc, Note, NoteId, Point, TimeBase};
use expression_editor_core::rows::{Articulation, RowSpace, StringTuning};

/// How much of a bend's shape survived the import.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BendFidelity {
    /// Full point list — the binary formats.
    Full,
    /// Endpoints only, middle values and offsets discarded by the
    /// parser's experimental GPIF path.
    EndpointsOnly,
    /// Nothing bent.
    None,
}

/// What an import produced, and what it had to give up.
pub struct Imported {
    pub doc: ExpressionDoc,
    /// String tuning from the file, low string first.
    pub tuning: StringTuning,
    pub bends: BendFidelity,
    /// Anything the file carried that we did not model, for a caller
    /// that would rather report than silently drop.
    pub not_carried: Vec<String>,
}

/// Ticks per quarter note the document is built in.
pub const PPQ: f64 = 960.0;

/// A parsed GP bend point, in the units #160 established.
///
/// Positions are sixtieths of the note's duration and values are
/// hundredths of a whole tone, so 50 is one semitone. Both are
/// converted once, here, and everything inside the editor is semitones.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BendPoint {
    /// 0..60 across the note.
    pub position: f64,
    /// 100 per whole tone.
    pub value: f64,
}

/// One note as the importer sees it, before it becomes an editor note.
#[derive(Clone, Debug, PartialEq)]
pub struct GpNote {
    /// 0 = lowest string.
    pub string: usize,
    pub fret: i32,
    /// Ticks from the start of the piece.
    pub start: f64,
    pub length: f64,
    /// Empty for an unbent note.
    pub bend: Vec<BendPoint>,
    /// A prebend is already bent when the string is struck, so its
    /// first point is the *sounding* pitch rather than a departure from
    /// the fretted one.
    pub prebend: bool,
    /// How the note is played, when the file says.
    ///
    /// The editor's own vocabulary, not the library's: GP's flags are
    /// several booleans and a slide list, and collapsing them here keeps
    /// the dependency's shape out of the document.
    pub articulation: Option<Articulation>,
}

/// Convert a GP bend point list to a curve in semitones over document
/// time.
///
/// x is `position/60 × length`, y is `value/50` semitones — #160's
/// mapping, applied once at the boundary.
pub fn bend_curve(points: &[BendPoint], length: f64) -> Curve {
    Curve::from_points(
        points
            .iter()
            .map(|p| Point::new((p.position / 60.0).clamp(0.0, 1.0) * length, p.value / 50.0))
            .collect(),
    )
}

/// The sounding row for a note: the string's open pitch plus the fret.
///
/// A prebend's *row* is the fretted note, not the bent one — the note
/// rectangle stays anchored to its literal row while the curve shows the
/// sounding offset, which is the rule the whole document model rests on.
pub fn row_of(tuning: &StringTuning, string: usize, fret: i32) -> i32 {
    tuning.open(string) + fret
}

/// Build a document from parsed notes.
pub fn to_document(notes: &[GpNote], tuning: StringTuning) -> Imported {
    let end = notes
        .iter()
        .map(|n| n.start + n.length)
        .fold(0.0_f64, f64::max);
    let mut doc = ExpressionDoc::new(TimeBase::Ppq { ppq: PPQ }, 0.0, end.max(PPQ));
    // String as row: the file's tuning drives the axis, so a drop-D or a
    // seven-string reads correctly instead of being forced onto a
    // default six.
    doc.row_space = RowSpace::Strings(tuning.clone());

    let mut fidelity = BendFidelity::None;
    for (i, gp) in notes.iter().enumerate() {
        let mut note = Note::new(
            NoteId(i as u64 + 1),
            gp.start,
            gp.start + gp.length,
            // The row is the sounding pitch. A guitar roll is a full
            // MIDI roll — the string is an annotation that colours the
            // note and makes its fret computable, not the axis.
            row_of(&tuning, gp.string, gp.fret),
        );
        note.string = Some(gp.string as u8);
        // The channel too, and not as a duplicate of the string: MPE
        // export attributes per-note bend by channel, and a part whose
        // notes all sit on channel 0 has every bend overwriting every
        // other one on any chord.
        note.channel = Some(gp.string as u8);
        note.articulation = gp.articulation;

        if !gp.bend.is_empty() {
            *note.curve_mut(Dimension::Pitch) = bend_curve(&gp.bend, gp.length);
            fidelity = match (fidelity, gp.bend.len()) {
                (BendFidelity::EndpointsOnly, _) => BendFidelity::EndpointsOnly,
                (_, n) if n <= 2 => BendFidelity::EndpointsOnly,
                _ => BendFidelity::Full,
            };
        }
        doc.notes.push(note);
    }

    Imported {
        doc,
        tuning,
        bends: fidelity,
        not_carried: Vec::new(),
    }
}

/// Import a Guitar Pro file's first playable track.
///
/// The first track rather than all of them: the roll shows one
/// instrument, and a caller wanting the bass as well imports it as a
/// second track. Percussion tracks are skipped — a drum part has no
/// strings and would produce a six-row space with nothing on it.
pub fn import_bytes(bytes: &[u8], format: parse::Format) -> Result<Imported, String> {
    let song = parse::read(bytes, format)?;
    let track = song
        .tracks
        .iter()
        .find(|t| !t.percussion_track && !t.strings.is_empty())
        .ok_or_else(|| "no stringed track in this file".to_string())?;

    let tuning = parse::tuning_of(track);
    let strings = tuning.strings();
    let notes = parse::notes_of(track, strings);
    let mut imported = to_document(&notes, tuning);

    // Report rather than silently drop, so a caller can say what was
    // lost instead of the user finding out by ear.
    if !format.keeps_bend_shape() && imported.bends != BendFidelity::None {
        imported.bends = BendFidelity::EndpointsOnly;
        imported.not_carried.push(
            "bend shape: this format is read through a path that keeps only \
             the first and last point of a bend"
                .into(),
        );
    }
    if song.tracks.len() > 1 {
        imported
            .not_carried
            .push(format!("{} further tracks", song.tracks.len() - 1));
    }
    Ok(imported)
}

/// Import from a path, choosing the reader by extension.
pub fn import_file(path: &str) -> Result<Imported, String> {
    let format =
        parse::Format::of_path(path).ok_or_else(|| format!("not a Guitar Pro file: {path}"))?;
    let bytes = std::fs::read(path).map_err(|e| format!("reading {path}: {e}"))?;
    import_bytes(&bytes, format)
}
