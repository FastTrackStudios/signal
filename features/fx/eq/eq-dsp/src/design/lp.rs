//! Low-pass (low-cut) cascade builder — Pro-Q 4 algorithmic path.
//!
//! LP is much smaller than HP because slopes 2/4/6/8 use the simple
//! Butterworth cascade via [`super::common::cascade_qs`] (matches Pro-Q
//! bit-exact at fc≤2 kHz). Odd slopes (3/5) append the real-pole tail
//! from [`super::hp`].

use crate::biquad::Coeffs;
use crate::cascade;

use super::common::cascade_qs;
use super::hp::{cut_odd_qs, cut_odd_tail_lowpass};

pub(super) fn mzt_lowpass_simple_cascade(
    n: usize,
    freq_hz: f64,
    q: f64,
    sample_rate: f64,
    order: usize,
) -> Vec<Coeffs> {
    let n = n.max(1);
    if n == 1 {
        return vec![cascade::lowpass_s2_proq4(freq_hz, q, sample_rate)];
    }
    if matches!(order, 3 | 5) {
        let mut sections: Vec<Coeffs> = cut_odd_qs(order, q)
            .into_iter()
            .map(|sq| cascade::lowpass_s2_proq4(freq_hz, sq, sample_rate))
            .collect();
        sections.push(cut_odd_tail_lowpass(freq_hz, sample_rate));
        return sections;
    }
    // Order 8 — Pro-Q slope index 7, 48 dB/oct, four sections.
    //
    // This arm used to test `order == 7`, which no slope produces, so every
    // 48 dB/oct cut fell through to the six-section slope-8 design below and
    // came out at 72. Measured on a 2 kHz high cut: an octave up the plugin is
    // 48.74 dB down and this was 72.53, and the gap kept growing — 73 dB by
    // 16 kHz.
    if order == 8 {
        return cascade_qs(4, q)
            .into_iter()
            .rev()
            .map(|sq| cascade::lowpass_s2_proq4(freq_hz, sq, sample_rate))
            .collect();
    }
    if order == 16 {
        let mut qs = cascade_qs(8, q);
        if let Some(max_q) = qs.last_mut() {
            *max_q = max_q.min(40.0);
        }
        return qs
            .into_iter()
            .rev()
            .map(|sq| cascade::lowpass_s2_proq4(freq_hz, sq, sample_rate))
            .collect();
    }
    // LP slope=8 mirrors HP: 6 sections with N=12 Butterworth distribution,
    // sec0 scaled by Q_user (clamped at 40).  Per probe verification, LP
    // also emits 6 sections at slope=8 (same as HP). Six sections is TWELVE
    // poles, so this is the 72 dB/oct setting — order 12, not order 8.
    if n >= 6 {
        return lp_slope8_cascade(freq_hz, q, sample_rate);
    }
    cascade_qs(n, q)
        .into_iter()
        .map(|sq| cascade::lowpass_s2_proq4(freq_hz, sq, sample_rate))
        .collect()
}

/// LP slope=8 cascade: closed-form per-section path matching the binary's
/// `compute_audio_biquad_lagrange_mzt`. See
/// `proq4_mzt::lp_slope8_section_biquad` for the decoded formula.
fn lp_slope8_cascade(freq_hz: f64, q_user: f64, sample_rate: f64) -> Vec<Coeffs> {
    (0..6)
        .map(|sec| crate::proq4_mzt::lp_slope8_section_biquad(sec, freq_hz, q_user, sample_rate))
        .collect()
}
