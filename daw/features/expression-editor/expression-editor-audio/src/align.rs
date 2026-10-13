//! Aligning one performance to another.
//!
//! The VocAlign job: a doubled vocal, a stacked harmony, or a second take
//! that should sit exactly on the first. Pick a **reference**, pick a
//! **dub**, and the dub is retimed to match.
//!
//! ## Where the engine lives
//!
//! Not here. The matching is [`align_dsp`], which knows nothing about
//! documents, takes or DAWs — it takes two sets of frame features and
//! returns a map. This module is the adapter: it hands the engine what an
//! [`Analysis`] already knows, and turns the answer back into the warp
//! markers the renderer reads.
//!
//! That split is the point. The same engine serves a REAPER action, an
//! offline batch job and this editor, and none of them has to agree on
//! anything else. It also means alignment is testable against synthetic
//! pairs whose true offset is known, without an editor anywhere near it.
//!
//! ## Why this is the timing feature, not a new one
//!
//! Alignment produces a map from dub time to reference time. That is
//! exactly what a timing drag produces, and it is written the same way —
//! as [`WarpMarker`]s that `render_world_warped` honours. So an alignment
//! lands in the editor as ordinary timing edits the user can then adjust
//! by hand, rather than as an opaque process.
//!
//! The markers come from the alignment's *anchors*, not from its map. A
//! marker per frame would pin every millisecond of every vowel to
//! whatever the matcher happened to decide, which is both uneditable and
//! worse-sounding than pinning the syllables and letting the sustains
//! stretch between them.

use tune_dsp::model::WarpMarker;

use crate::Analysis;

pub use align_dsp::{
    AlignConfig, Alignment, Anchor, AnchorConfig, AnchorKind, CostConfig, Material, Offset,
    OffsetConfig, WarpConfig,
};

/// Align `dub` to `reference`.
///
/// Returns `None` when either take has no frames — there is nothing to
/// match, and a caller should say so rather than apply an empty map.
///
/// The offset search runs on the level envelopes only. Where the two
/// takes are phase-coherent — two mics on one source, a bounced copy —
/// [`align_to_audio`] resolves the offset far more precisely.
pub fn align(reference: &Analysis, dub: &Analysis, cfg: AlignConfig) -> Option<Alignment> {
    align_dsp::align(&reference.align, &dub.align, &cfg)
}

/// Align `dub` to `reference`, refining the offset against the waveforms.
///
/// `reference_audio` and `dub_audio` must be the same mono buffers the
/// two analyses ran on. They only feed the offset stage, and only when it
/// scores well enough to be believed, so passing them can improve the
/// result and cannot degrade it.
pub fn align_to_audio(
    reference: &Analysis,
    reference_audio: &[f64],
    dub: &Analysis,
    dub_audio: &[f64],
    cfg: AlignConfig,
) -> Option<Alignment> {
    align_dsp::align_audio(
        &reference.align,
        reference_audio,
        &dub.align,
        dub_audio,
        dub.pitch.sample_rate,
        &cfg,
    )
}

/// An alignment as warp markers for the dub.
///
/// One marker per anchor: where a moment plays now, and where it played
/// before. Everything between two markers stretches to fit, which is
/// exactly the piecewise-linear map the alignment describes — so this is
/// a change of representation, not an approximation.
///
/// Falls back to the map when an alignment carries no anchors, which is
/// the case for one built by hand or by [`crate::align_hits`].
pub fn warp_markers(alignment: &Alignment, hop: usize) -> Vec<WarpMarker> {
    let hop = hop.max(1) as f64;
    let marker = |source: f64, target: f64| WarpMarker {
        sample: source * hop,
        d_time: (target - source) * hop,
        pitch_bend: 0.0,
    };

    if !alignment.anchors.is_empty() {
        return alignment
            .anchors
            .iter()
            .map(|a| marker(a.dub as f64, a.reference))
            .collect();
    }
    alignment
        .map
        .iter()
        .enumerate()
        .map(|(i, &target)| marker(i as f64, target))
        .collect()
}
