//! Aligning one performance to another.
//!
//! The job every DAW calls "vocal align": a doubled vocal, a stacked
//! harmony, a second guitar pass, a replacement drum take that should sit
//! exactly on the first. Pick a **reference**, pick a **dub**, and the
//! dub is retimed to match — as data, not as a render, so the result is
//! an ordinary set of timing edits the user can then argue with.
//!
//! ```text
//! reference ─┐
//!            ├─▶ offset ──▶ warp ──▶ anchors ──▶ Alignment
//! dub ───────┘   (where)    (how)    (which of it to keep)
//! ```
//!
//! Three stages, and the split is the design:
//!
//! - [`offset`] answers **where the dub is** — one number, found by
//!   correlation over the whole take. Without it the warp's band has to
//!   be wide enough to reach a late entry, and a wide band is how a
//!   matcher ends up confidently aligning the end of one phrase to the
//!   start of another.
//! - [`dtw`] answers **how the dub is bent**, searching only a narrow
//!   band around that offset.
//! - [`anchors`] answers **how much of that to believe** — a path has an
//!   opinion about every frame, and only the moments where both takes
//!   agree something began are worth acting on.
//!
//! ## What this crate is not
//!
//! It does not touch audio and it does not know what a DAW is. The output
//! is a map from dub time to reference time; turning that into stretch
//! markers, warp markers, or a resynthesis is the host's business. That
//! is what lets the same engine serve the expression editor, a REAPER
//! action, and an offline batch job without any of them agreeing on
//! anything but this crate.
//!
//! ## Provenance
//!
//! Independently written, from published DSP: normalized cross-correlation
//! with an FFT, banded DTW with slope constraints, parabolic peak
//! interpolation. No code is taken from any other aligner.
//!
//! Three ideas here were *prompted* by reading VoxAlign, the GPL-3.0
//! ReaScript by Acrosonus Mastering — a macro offset stage before the
//! warp, sparse confidence-filtered anchors instead of a marker per
//! frame, and a bound on the stretch ratio between markers. This
//! workspace is GPL-3.0-or-later, so its code could have been used with
//! attribution; it was not, because a 2400-line Lua script built around
//! ReaScript's item API and a global settings table is not a useful basis
//! for a host-agnostic Rust crate. Where the two disagree, they disagree
//! on purpose: the band here is scaled to the two takes' lengths, the
//! stretch bound is enforced on written segments rather than on DTW
//! cells, and the feature vector carries a real pitch track rather than
//! zero-crossing rate as a proxy for one.

pub mod anchors;
pub mod dtw;
pub mod features;
pub mod offset;

pub use anchors::{Anchor, AnchorConfig, AnchorKind};
pub use dtw::{CostConfig, WarpConfig};
pub use features::{FeatureConfig, Features, Frame, extract};
pub use offset::{Offset, OffsetConfig};

/// Everything that shapes an alignment.
#[derive(Clone, Copy, Debug)]
pub struct AlignConfig {
    pub offset: OffsetConfig,
    pub warp: WarpConfig,
    pub cost: CostConfig,
    pub anchors: AnchorConfig,
    /// Whether to warp at all.
    ///
    /// Off means the dub is only *moved*: one rigid shift, no stretching.
    /// That is the correct and only safe answer for two mics on one
    /// source or a bounced copy, where any stretch at all is a phase
    /// artefact rather than a correction.
    pub warp_enabled: bool,
}

impl Default for AlignConfig {
    fn default() -> Self {
        Self {
            offset: OffsetConfig::default(),
            warp: WarpConfig::default(),
            cost: CostConfig::default(),
            anchors: AnchorConfig::default(),
            warp_enabled: true,
        }
    }
}

/// What the two takes are, which is the only thing a user should have to
/// decide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Material {
    /// Two mics on one source, or a copy. Rigid shift only.
    SameSource,
    /// A deliberate double of a lead vocal. Tight.
    DoubleTrack,
    /// Several singers on a harmony part. Looser, so the parts keep some
    /// of their own phrasing and do not collapse into one voice.
    BackingVocal,
    /// Acoustic guitar, piano, strings. Pitch is reliable; voicing is not.
    HarmonicInstrument,
    /// Drums, percussion, plucked bass. Transients are everything and
    /// pitch means nothing.
    Percussive,
}

impl AlignConfig {
    /// Sensible settings for a kind of material.
    ///
    /// These are starting points, not truths. Every field stays public
    /// precisely so a caller can disagree.
    pub fn for_material(material: Material) -> Self {
        let mut cfg = Self::default();
        match material {
            Material::SameSource => {
                cfg.warp_enabled = false;
                // The whole value here is sample accuracy, so insist on
                // the waveform pass and give it room.
                cfg.offset.refine_radius_secs = 0.050;
                cfg.offset.refine_min_score = 0.3;
            }
            Material::DoubleTrack => {
                cfg.cost.pitch = 0.15;
                cfg.cost.voicing = 0.6;
                cfg.anchors.strength = 0.9;
            }
            Material::BackingVocal => {
                // Harmony parts are at different pitches on purpose, and
                // several singers never agree exactly. Aiming for an
                // exact match here is how a section turns into one voice.
                cfg.cost.pitch = 0.05;
                cfg.cost.voicing = 0.5;
                cfg.anchors.strength = 0.8;
                cfg.anchors.min_gap_secs = 0.100;
            }
            Material::HarmonicInstrument => {
                cfg.cost.pitch = 0.20;
                // No breath and no consonants, so voiced-against-unvoiced
                // says much less than it does on a voice.
                cfg.cost.voicing = 0.2;
                cfg.cost.bands = [0.20, 0.30, 0.25, 0.10];
            }
            Material::Percussive => {
                cfg.cost.pitch = 0.0;
                cfg.cost.voicing = 0.0;
                cfg.cost.flux = 0.35;
                cfg.cost.delta = 0.25;
                cfg.cost.bands = [0.25, 0.25, 0.20, 0.15];
                cfg.cost.level = 0.10;
                // A hit is either on the reference hit or it is not, so
                // corrections are small and stretching between them must
                // stay mild.
                cfg.anchors.max_shift_secs = 0.12;
                cfg.anchors.max_stretch_ratio = 1.3;
                cfg.anchors.min_gap_secs = 0.030;
                cfg.anchors.onset_strength = 0.20;
                cfg.warp.band_secs = 0.25;
            }
        }
        cfg
    }
}

/// The alignment, as a monotonic map over dub frames.
#[derive(Clone, Debug, PartialEq)]
pub struct Alignment {
    /// For each dub frame, the reference frame it should land on. Same
    /// length as the dub's frame count.
    pub map: Vec<f64>,
    /// The points the map is pinned at. These, not `map`, are what a host
    /// should write: the map is what they interpolate to.
    pub anchors: Vec<Anchor>,
    /// The global offset found before warping.
    pub offset: Offset,
    /// Frames per second both takes were analysed at.
    pub frame_rate: f64,
}

impl Alignment {
    /// A map with no anchors, for callers that computed one another way.
    pub fn from_map(map: Vec<f64>, frame_rate: f64) -> Self {
        Self {
            map,
            anchors: Vec::new(),
            offset: Offset::NONE,
            frame_rate,
        }
    }

    /// Largest correction anywhere, in seconds — how far the dub had to
    /// move.
    ///
    /// A useful readout and a sanity check: a value at the configured
    /// maximum usually means the two takes are not the same part.
    pub fn max_shift_secs(&self) -> f64 {
        self.map
            .iter()
            .enumerate()
            .map(|(i, &r)| (r - i as f64).abs())
            .fold(0.0, f64::max)
            / self.frame_rate.max(1e-9)
    }

    /// Largest stretch ratio between consecutive anchors.
    ///
    /// `1.0` means nothing was stretched, only shifted. This is the
    /// number that predicts artefacts: a shift is inaudible and a stretch
    /// is not.
    pub fn max_stretch_ratio(&self) -> f64 {
        let mut worst = 1.0_f64;
        for pair in self.anchors.windows(2) {
            let span = pair[1].dub as f64 - pair[0].dub as f64;
            if span <= 0.0 {
                continue;
            }
            let ratio = (pair[1].reference - pair[0].reference) / span;
            if ratio > 0.0 {
                worst = worst.max(ratio.max(1.0 / ratio));
            }
        }
        worst
    }
}

/// Align `dub` to `reference` from features alone.
///
/// The macro offset comes from correlating the two level envelopes, which
/// is the robust half of the search and needs no audio. Use
/// [`align_audio`] when the samples are to hand and sample-accurate
/// placement matters.
pub fn align(reference: &Features, dub: &Features, cfg: &AlignConfig) -> Option<Alignment> {
    align_audio(reference, &[], dub, &[], 0.0, cfg)
}

/// Align `dub` to `reference`, refining the offset against the waveforms.
///
/// `reference_samples` and `dub_samples` are the mono buffers the features
/// were extracted from. They are used only by the offset stage, and only
/// its result is affected — on two takes that are not phase-coherent the
/// refinement scores badly and is discarded, so passing audio can improve
/// the answer and cannot degrade it.
pub fn align_audio(
    reference: &Features,
    reference_samples: &[f64],
    dub: &Features,
    dub_samples: &[f64],
    sample_rate: f64,
    cfg: &AlignConfig,
) -> Option<Alignment> {
    if reference.is_empty() || dub.is_empty() {
        return None;
    }
    // Both takes are analysed by the same code at the same rate; if a
    // caller mixes rates the band below is wrong in one of them, so this
    // is the assumption made explicit.
    let frame_rate = dub.frame_rate.max(1e-9);

    let offset = offset::macro_offset(
        &reference.envelope(),
        &dub.envelope(),
        frame_rate,
        reference_samples,
        dub_samples,
        sample_rate,
        cfg.offset,
    );
    let offset_frames = offset.seconds * frame_rate;

    let map = if cfg.warp_enabled {
        dtw::warp(
            &dub.frames,
            &reference.frames,
            frame_rate,
            offset_frames,
            &cfg.warp,
            &cfg.cost,
        )
    } else {
        // Rigid: every frame moves by the same amount, and the anchor
        // stage below reduces that to the two endpoints it really is.
        (0..dub.len()).map(|i| i as f64 + offset_frames).collect()
    };
    if map.is_empty() {
        return None;
    }

    let anchors = anchors::anchors(
        &map,
        &dub.frames,
        &reference.frames,
        frame_rate,
        offset_frames,
        cfg.anchors,
    );
    // The written map is rebuilt from the anchors rather than kept from
    // the path: the anchors are what a host will apply, so the map has to
    // be what they produce, or the preview disagrees with the result.
    let map = anchors::map_from_anchors(&anchors, dub.len());

    Some(Alignment {
        map,
        anchors,
        offset,
        frame_rate,
    })
}
