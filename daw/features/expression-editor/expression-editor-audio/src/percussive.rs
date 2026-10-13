//! Analysing a take as hits rather than as notes.
//!
//! The pitched path ([`crate::analyze_take`]) tracks f0 and cuts where
//! it changes. On unpitched material that produces nonsense: YIN reports
//! a pitch for a snare because it reports a pitch for anything, and the
//! resulting notes flicker between octaves take after take.
//!
//! So: segment on attacks, sort by brightness, and keep the measured
//! centroid so re-banding costs nothing.

use expression_editor_core::rows::{RowSpace, SliceBands};
use expression_editor_core::{ExpressionDoc, Note, NoteId, TimeBase};

use crate::onsets::{self, Onset, OnsetConfig};

/// A take analysed as percussion.
#[derive(Clone, Debug)]
pub struct Percussion {
    /// What the editor edits: one note per hit.
    pub doc: ExpressionDoc,
    /// The hits themselves, in document order, index-aligned with
    /// `doc.notes` — a slice's centroid is not in the document because
    /// the document has nowhere to put it, and re-banding needs it.
    pub onsets: Vec<Onset>,
    /// Document time unit.
    pub frame_rate: f64,
    pub bands: SliceBands,
}

impl Percussion {
    /// Re-sort every slice into bands after the splits move.
    ///
    /// Cheap and lossless, which is the point of keeping the centroids:
    /// the user drags a split, every hit lands in its new band, and no
    /// analysis re-runs.
    pub fn reband(&mut self, bands: SliceBands) {
        for (note, onset) in self.doc.notes.iter_mut().zip(&self.onsets) {
            note.row = bands.band_of(onset.centroid_hz) as i32;
            // Carried on the note so the editor can reband later
            // without holding this struct — see `Edit::SetBands`.
            note.centroid_hz = Some(onset.centroid_hz);
        }
        self.doc.row_space = RowSpace::Bands(bands.clone());
        self.bands = bands;
    }
}

/// How a take is analysed as percussion.
#[derive(Clone, Copy, Debug, Default)]
pub struct PercussiveConfig {
    pub onsets: OnsetConfig,
}

/// Analyse a mono take as percussion.
///
/// The document's time base is analysis frames at the *onset* hop, not
/// the pitch tracker's — this path never runs the pitch tracker, and
/// inventing its frame rate here would put every slice in the wrong
/// place by the ratio between the two hops.
pub fn analyze_percussive(samples: &[f64], sample_rate: f64, cfg: PercussiveConfig) -> Percussion {
    let bands = SliceBands::default();
    let frame_rate = sample_rate / cfg.onsets.hop.max(1) as f64;
    let hits = onsets::detect(samples, sample_rate, cfg.onsets);

    let total_frames = if sample_rate > 0.0 {
        samples.len() as f64 / cfg.onsets.hop.max(1) as f64
    } else {
        0.0
    };

    let mut notes = Vec::with_capacity(hits.len());
    for (i, hit) in hits.iter().enumerate() {
        // A slice runs to the next attack. That makes it a region rather
        // than a point, so it can be selected, dragged and levelled like
        // any other note — and it means the slices tile the take, which
        // is what lets a gain edit on one not leave a gap.
        let end = hits
            .get(i + 1)
            .map(|next| next.frame as f64)
            .unwrap_or(total_frames);
        let mut note = Note::new(
            NoteId(i as u64 + 1),
            hit.frame as f64,
            end.max(hit.frame as f64 + 1.0),
            bands.band_of(hit.centroid_hz) as i32,
        );
        // Peak amplitude, not RMS: a hit's *impact* is its peak, and two
        // hits with the same RMS over different decay lengths are not
        // equally hard.
        note.velocity = hit.peak.clamp(0.0, 1.0);
        note.weight = note.velocity;
        // What band this slice belongs in is a function of its
        // centroid, so the centroid travels with the slice.
        note.centroid_hz = Some(hit.centroid_hz);
        notes.push(note);
    }

    let mut doc = ExpressionDoc::new(TimeBase::Frames { frame_rate }, 0.0, total_frames);
    doc.row_space = RowSpace::Bands(bands.clone());
    // The backdrop is the whole recording, hits and gaps alike — on a
    // kit it is the only way to see that a ghost note was missed rather
    // than absent.
    doc.peaks = peaks_of(samples, cfg.onsets.hop.max(1));
    for note in notes {
        doc.push(note);
    }

    Percussion {
        doc,
        onsets: hits,
        frame_rate,
        bands,
    }
}

/// One peak per onset frame, 0..1 — the waveform behind the slices.
fn peaks_of(samples: &[f64], hop: usize) -> Vec<f32> {
    samples
        .chunks(hop)
        .map(|c| c.iter().fold(0.0f64, |m, v| m.max(v.abs())).min(1.0) as f32)
        .collect()
}

/// Whether a take is better served by the percussive path.
///
/// The inference the editor makes at load, and it is only ever a first
/// guess — the mode is the user's the moment they touch it. A whispered
/// vocal reads as unpitched and a melodic tom fill reads as pitched, and
/// no threshold fixes both.
///
/// The measure is the share of frames the tracker found a pitch for. It
/// is deliberately not "did we find *a* pitch": YIN reports one for
/// almost any frame with energy, so the question has to be how *often*,
/// and 40% is comfortably below a sustained vocal and above a kit.
pub fn looks_percussive(voiced_fraction: f64) -> bool {
    voiced_fraction < 0.4
}

/// Share of frames with a tracked pitch, over frames with any level.
///
/// Silence is excluded from the denominator or a take with long gaps
/// reads as unpitched however clearly it sings.
pub fn voiced_fraction(analysis: &crate::Analysis, silence_rms: f64) -> f64 {
    let mut sounding = 0usize;
    let mut voiced = 0usize;
    for f in &analysis.frames.frames {
        if f.rms < silence_rms {
            continue;
        }
        sounding += 1;
        if f.f0_hz.is_some() {
            voiced += 1;
        }
    }
    if sounding == 0 {
        return 0.0;
    }
    voiced as f64 / sounding as f64
}
