//! From a buffer of vocal audio to an editable document.
//!
//! One call, because every step after the first depends on decisions
//! made in it — the hop size, which frames counted as voiced, where the
//! note boundaries landed — and threading those through a caller is how
//! the frame indices and the document's time base drift apart.
//!
//! ```text
//! samples ──▶ YIN frames ──▶ tracked notes ──▶ blobs ──▶ ExpressionDoc
//!                  │                                        ▲
//!                  └──── rms + unvoiced spans ──────────────┘
//! ```
//!
//! The [`Analysis`] that comes back holds both halves: the document the
//! editor edits, and the `PitchDoc` the renderer needs. They are kept
//! together because writing back requires both — the document says what
//! the user wants, the `PitchDoc` says what was recorded, and neither
//! alone can produce audio.

use expression_editor_core::ExpressionDoc;
use tune_dsp::model::PitchDoc;
use tune_dsp::tracker::TrackConfig;
use tune_dsp::{AnalyzeConfig, TuneAnalysis};

/// Everything an editing session needs from one take.
#[derive(Clone, Debug)]
pub struct Analysis {
    /// What the editor edits.
    pub doc: ExpressionDoc,
    /// What the renderer resynthesises from. Blob order matches note
    /// ids: `NoteId(i + 1)` is `blobs[i]`, which is the pairing
    /// [`crate::apply_to`] relies on.
    pub pitch: PitchDoc,
    /// Analysis frames per second — the document's time unit.
    pub frame_rate: f64,
    /// Raw per-frame detection, kept so a re-segmentation does not have
    /// to re-run YIN over the whole take.
    pub frames: TuneAnalysis,
    /// The same frames described the way [`crate::align`] needs them:
    /// four bands of energy, onset strength, noisiness, and the pitch
    /// track folded in.
    ///
    /// Extracted here rather than on demand because it must be measured
    /// at exactly `frames`' hop and window — frame *i* has to mean the
    /// same audio in both, and reconstructing that from a caller is how
    /// two frame indices quietly stop describing the same moment.
    pub align: align_dsp::Features,
}

/// How the take is analysed.
#[derive(Clone, Debug)]
pub struct TakeConfig {
    pub analyze: AnalyzeConfig,
    pub track: TrackConfig,
    /// Frames below this level are treated as silence rather than as
    /// unvoiced sound.
    ///
    /// The distinction matters for display: a consonant is *signal*
    /// with no pitch and should shade as a sibilant, where a gap
    /// between phrases is nothing at all. Without a floor every quiet
    /// moment reads as a consonant and the sibilant bands become
    /// meaningless.
    pub silence_rms: f64,
}

impl Default for TakeConfig {
    fn default() -> Self {
        Self {
            analyze: AnalyzeConfig::default(),
            track: TrackConfig::default(),
            // About -60 dBFS: below a sung consonant, above a quiet
            // room on a close mic.
            silence_rms: 0.001,
        }
    }
}

/// Analyse a mono take.
///
/// `samples` must be mono; a caller with a stereo item sums or picks a
/// channel first, because which of those is right depends on the
/// material and this cannot know.
pub fn analyze_take(samples: &[f64], sample_rate: f64, cfg: TakeConfig) -> Analysis {
    let frames = tune_dsp::analyze(samples, sample_rate, cfg.analyze);
    let frame_rate = sample_rate / frames.hop.max(1) as f64;

    // The tracker takes candidate pitches per frame. YIN gives at most
    // one, so each frame is a zero- or one-element list — the shape
    // exists for polyphonic trackers and costs nothing here.
    let candidates: Vec<Vec<f64>> = frames
        .frames
        .iter()
        .map(|f| f.f0_hz.into_iter().collect())
        .collect();
    let tracked = tune_dsp::track_notes(&candidates, cfg.track);
    let pitch = PitchDoc::from_notes(&tracked, frames.hop, sample_rate);

    let rms: Vec<f32> = frames.frames.iter().map(|f| f.rms as f32).collect();
    // Frames with no pitch *and* enough level to be a sound: that is a
    // consonant. Silence is neither voiced nor a sibilant.
    let voiced_or_silent: Vec<Option<f64>> = frames
        .frames
        .iter()
        .map(|f| match f.f0_hz {
            Some(hz) => Some(hz),
            // Reported as "voiced" to the span finder purely to exclude
            // it: a silent frame must not start a sibilant span.
            None if f.rms < cfg.silence_rms => Some(0.0),
            None => None,
        })
        .collect();

    let doc = crate::to_doc_with_audio(&pitch, frame_rate, &rms, &voiced_or_silent);

    // `tune_dsp::analyze` frames every `hop` samples over a window four
    // times that (its YIN window, quartered), so this reproduces its
    // framing exactly and the two frame counts agree.
    let hop = frames.hop.max(1);
    let semitones: Vec<Option<f64>> = frames
        .frames
        .iter()
        .map(|f| f.f0_hz.map(tune_dsp::hz_to_midi))
        .collect();
    let align = align_dsp::extract(
        samples,
        sample_rate,
        hop,
        hop * 4,
        align_dsp::FeatureConfig::default(),
    )
    .with_pitch(&semitones);

    Analysis {
        doc,
        pitch,
        frame_rate,
        frames,
        align,
    }
}

impl Analysis {
    /// Push the editor's changes back onto the analysis, ready to
    /// render.
    ///
    /// Both halves of the write-back in one call, because they are not
    /// independent: the pitch edits and the timing edits come from the
    /// same document and a renderer given one without the other
    /// produces audio that matches neither.
    pub fn commit(&mut self) {
        crate::apply_to(&self.doc, &mut self.pitch);
        self.pitch.markers = crate::warp_markers(&self.doc, &self.pitch);
    }

    /// Render the edited take.
    ///
    /// `audio` is the original buffer the analysis ran on — the render
    /// reads it, so it must be the same samples and the same rate.
    /// Without timing edits the result is exactly as long as the input.
    /// WORLD synthesises whole frames, so its own output lands on a
    /// frame boundary and is short by up to one — a few milliseconds. A
    /// host replacing an item's audio would then see the take shrink
    /// slightly on *every* render pass, and the item's end drift away
    /// from the timeline. So the unwarped path is padded back to length.
    ///
    /// With timing edits the length is the edit and is left alone: a
    /// stretched phrase is genuinely longer, and forcing it back would
    /// undo the thing that was just asked for.
    #[cfg(feature = "render")]
    pub fn render(&self, audio: &[f64]) -> Vec<f64> {
        let warped = self.pitch.markers.iter().any(|m| m.d_time.abs() > 1e-9);
        if warped {
            return tune_dsp::render::render_world_warped(
                &self.pitch,
                audio,
                self.pitch.sample_rate as u32,
            );
        }
        // No timing edits: skip the warp path, which remaps every frame
        // even when the map is the identity.
        let mut out =
            tune_dsp::render::render_world(&self.pitch, audio, self.pitch.sample_rate as u32);
        out.resize(audio.len(), 0.0);
        out
    }
}

/// Sum a stereo buffer to mono for analysis.
///
/// Pitch tracking wants one signal, and a doubled vocal panned wide
/// tracks better summed than from either side alone.
pub fn to_mono(interleaved: &[f64], channels: usize) -> Vec<f64> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    interleaved
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f64>() / channels as f64)
        .collect()
}
