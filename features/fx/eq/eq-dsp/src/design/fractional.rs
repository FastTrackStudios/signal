//! Fractional filter slopes — the steepnesses between the integer orders.
//!
//! Pro-Q's slope control is continuous: `raw * 6` dB/oct, so a band can ask
//! for 7.5, 15.25 or 30.26 dB/oct. **137 bands across 61 of the 171 factory
//! presets do**, spread over bells, shelves and cuts alike, so rounding to the
//! nearest integer order is not a rare approximation — it is a visible one.
//!
//! # Why this is not just "a filter with a fractional order"
//!
//! A rational transfer function's asymptotic slope is always a whole number of
//! poles, i.e. a multiple of 6 dB/oct. There is no biquad cascade whose tail
//! falls at 15 dB/oct forever. What *is* achievable — and what anyone means by
//! a 15 dB/oct filter — is that slope **over the band you can hear**, which is
//! the classical fractional-order approximation: interleave poles and zeros on
//! a log-frequency ladder so the magnitude staircases downward at the average
//! rate you asked for.
//!
//! One cell of the ladder is a pole and a zero `f * W` octaves apart inside a
//! window `W` octaves wide. Between them the response moves at the full
//! 6 dB/oct a single pole gives; outside them it is flat. Averaged over the
//! cell that is `6 * f` dB/oct, and repeating the cell down the spectrum
//! extends the rate as far as it is wanted. The ripple is the price, and it
//! shrinks with the cell width.
//!
//! So the integer part of the order is built the way it always was, and this
//! supplies the remainder.

use crate::biquad::Coeffs;

/// Octaves per ladder cell.
///
/// Narrower cells ripple less and cost more sections. At one octave the ripple
/// is under a decibel for any fraction, which is well below the ~1 dB the
/// comparison against Pro-Q resolves.
const CELL_OCTAVES: f64 = 1.0;

/// How many cells to lay down.
///
/// One per octave, and the ladder has to reach the end of the spectrum or the
/// response simply stops falling where it runs out. Four cells did: on "Gentle
/// Stereo Narrowing" a 1.3 dB/oct high cut cornered at 110 Hz went flat above
/// 1.8 kHz, where the plugin kept rolling off — 5 dB adrift by 12.8 kHz.
///
/// Ten covers the whole audio band from any corner. Cells that would land past
/// Nyquist, or below hearing, are dropped as they are laid, so the extra ones
/// cost nothing when the corner does not need them.
const CELLS: usize = 10;

/// The number of biquads [`sections`] returns.
pub const SECTION_COUNT: usize = CELLS / 2;

/// One first-order section `g * (s + wz) / (s + wp)`, bilinear-transformed.
///
/// Returned as `[b0, b1, a1]` — the second-order terms are zero, so a pair of
/// them multiply into one biquad without any coefficients colliding.
///
/// The `g` matters. This section's asymptotes are `g * wz / wp` at DC and `g`
/// up top, so a ladder that leaves `g` at 1 pins the **top** to unity. That is
/// what a low cut wants and the opposite of what a high cut wants: leaving it
/// at 1 for a high cut lifted the passband by the whole ladder ratio instead
/// of lowering the stop band, which measured as a flat +52 dB.
fn first_order(zero_hz: f64, pole_hz: f64, sample_rate: f64, unity_at_dc: bool) -> [f64; 3] {
    let nyquist = sample_rate * 0.5;
    // Pre-warp so the bilinear transform puts the corners where they were
    // asked for rather than where the frequency warping leaves them.
    let warp = |hz: f64| {
        let hz = hz.clamp(1.0, nyquist * 0.999);
        2.0 * sample_rate * (std::f64::consts::PI * hz / sample_rate).tan()
    };
    let wz = warp(zero_hz);
    let wp = warp(pole_hz);
    let g = if unity_at_dc { wp / wz } else { 1.0 };
    let k = 2.0 * sample_rate;
    let den = k + wp;
    [g * (k + wz) / den, g * (wz - k) / den, (wp - k) / den]
}

/// Multiply two first-order sections into one biquad.
fn combine(a: [f64; 3], b: [f64; 3]) -> Coeffs {
    // (b0 + b1 z^-1)(c0 + c1 z^-1) over (1 + a1 z^-1)(1 + d1 z^-1)
    [
        1.0,
        a[2] + b[2],
        a[2] * b[2],
        a[0] * b[0],
        a[0].mul_add(b[1], a[1] * b[0]),
        a[1] * b[1],
    ]
}

/// A single-pole cut — 6 dB/oct, the shallowest slope Pro-Q offers.
///
/// The cascade designs start at second order and returned a pass-through below
/// it, so a 6 dB/oct low or high cut did nothing at all. 28 bands in the Pro-Q
/// factory library ask for one.
#[must_use]
pub fn first_order_cut(freq_hz: f64, sample_rate: f64, high_pass: bool) -> Coeffs {
    let nyquist = sample_rate * 0.5;
    let hz = freq_hz.clamp(1.0, nyquist * 0.999);
    let wp = 2.0 * sample_rate * (std::f64::consts::PI * hz / sample_rate).tan();
    let k = 2.0 * sample_rate;
    let den = k + wp;
    let a1 = (wp - k) / den;
    if high_pass {
        // H(s) = s / (s + wp): a zero at DC.
        let g = k / den;
        [1.0, a1, 0.0, g, -g, 0.0]
    } else {
        // H(s) = wp / (s + wp).
        let g = wp / den;
        [1.0, a1, 0.0, g, g, 0.0]
    }
}

/// The ladder that supplies a fractional slope.
///
/// `fraction` is the part of the order beyond the integer sections, in the
/// range 0..1; `high_pass` puts the extra roll-off below the corner (a low
/// cut) rather than above it (a high cut). A fraction of zero returns
/// pass-throughs, so a caller can always cascade these without branching.
///
/// The gain is normalised to unity in the pass band, so adding this to an
/// integer-order design changes its slope and not its level.
#[must_use]
pub fn sections(freq_hz: f64, fraction: f64, sample_rate: f64, high_pass: bool) -> Vec<Coeffs> {
    let f = fraction.clamp(0.0, 1.0);
    if f <= 1.0e-6 {
        return vec![crate::biquad::PASSTHROUGH; SECTION_COUNT];
    }

    // Each cell spans CELL_OCTAVES, with its pole and zero `f * CELL_OCTAVES`
    // apart — that separation is what sets the average rate.
    let separation = f * CELL_OCTAVES;
    // Cells have to stay inside the band the sample rate can represent. A
    // high cut cornered near Nyquist has nowhere above it to put a ladder, and
    // clamping the cells into the same place there produces a pre-warped
    // ratio that is enormous rather than a slope that is gentle.
    let ceiling = sample_rate * 0.45;
    // And below the band there is nothing left to attenuate; a cell under this
    // is pinned by `first_order`'s own clamp into a degenerate pass-through.
    let floor_hz: f64 = 5.0;
    let mut first_orders = Vec::with_capacity(CELLS);
    for cell in 0..CELLS {
        let (zero, pole) = if high_pass {
            // March down from the corner. Attenuate below, unity above: the
            // zero sits under the pole and the top asymptote stays at 1.
            let upper = freq_hz * (-(f64::from(i32::try_from(cell).unwrap_or(0))) * CELL_OCTAVES).exp2();
            let lower = upper * (-separation).exp2();
            if lower <= floor_hz {
                break;
            }
            (lower, upper)
        } else {
            // March up from the corner. Unity below, attenuate above: the
            // pole sits under the zero and the DC asymptote is normalised.
            let lower = freq_hz * (f64::from(i32::try_from(cell).unwrap_or(0)) * CELL_OCTAVES).exp2();
            let upper = lower * separation.exp2();
            // BOTH ends have to fit. Keeping a cell whose zero lands past
            // Nyquist does not give a gentler slope — the pre-warp pins the
            // zero at the edge and the pole/zero ratio comes out far larger
            // than the cell was meant to have.
            if upper >= ceiling {
                break;
            }
            (upper, lower)
        };
        first_orders.push(first_order(zero, pole, sample_rate, !high_pass));
    }
    if first_orders.is_empty() {
        return vec![crate::biquad::PASSTHROUGH; SECTION_COUNT];
    }

    let mut out: Vec<Coeffs> = first_orders
        .chunks(2)
        .map(|pair| match pair {
            [a, b] => combine(*a, *b),
            [a] => combine(*a, [1.0, 0.0, 0.0]),
            _ => crate::biquad::PASSTHROUGH,
        })
        .collect();
    // A fixed section count keeps the caller's budget predictable.
    while out.len() < SECTION_COUNT {
        out.push(crate::biquad::PASSTHROUGH);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::response::compute_magnitude_response;

    const SR: f64 = 48_000.0;

    /// Average dB/oct between two frequencies.
    ///
    /// `compute_magnitude_response` already returns decibels despite its name,
    /// so these are differences, not ratios.
    fn slope_db_per_oct(sos: &[Coeffs], lo: f64, hi: f64) -> f64 {
        let db = compute_magnitude_response(sos, &[lo, hi], SR);
        (db[1] - db[0]) / (hi / lo).log2()
    }

    /// The ladder delivers the rate it was asked for.
    #[test]
    fn a_fraction_gives_that_fraction_of_a_pole() {
        for f in [0.25f64, 0.5, 0.75] {
            let sos = sections(1000.0, f, SR, true);
            // Measured across the cells, below the corner.
            let got = slope_db_per_oct(&sos, 125.0, 1000.0);
            let want = 6.0 * f;
            assert!(
                (got - want).abs() < 1.0,
                "fraction {f} should roll at {want:.2} dB/oct, measured {got:.2}",
            );
        }
    }

    /// Zero is transparent, so callers need no special case.
    #[test]
    fn a_zero_fraction_is_a_pass_through() {
        let sos = sections(1000.0, 0.0, SR, true);
        for db in compute_magnitude_response(&sos, &[50.0, 1000.0, 15_000.0], SR) {
            assert!(db.abs() < 1.0e-9, "expected 0 dB, got {db}");
        }
    }

    /// The pass band keeps its level — this only changes slope.
    ///
    /// Both directions, because they normalise at opposite ends. A low cut is
    /// unity above and falls below; a high cut is unity below and falls above.
    /// Leaving the high cut's DC asymptote unnormalised lifted its whole pass
    /// band by the ladder ratio, which measured as a flat +52 dB against the
    /// plugin on a preset whose filter shape was otherwise correct.
    #[test]
    fn the_pass_band_is_left_at_unity() {
        for db in compute_magnitude_response(&sections(1000.0, 0.6, SR, true), &[4000.0, 12_000.0], SR)
        {
            assert!(db.abs() < 0.5, "low cut pass band moved by {db:+.2} dB");
        }
        for db in compute_magnitude_response(&sections(1000.0, 0.6, SR, false), &[50.0, 250.0], SR) {
            assert!(db.abs() < 0.5, "high cut pass band moved by {db:+.2} dB");
        }
    }

    /// A corner near Nyquist has nowhere to put a ladder, and says so quietly.
    ///
    /// The cells march upward for a high cut. Cornered at 15.6 kHz on a 48 kHz
    /// rate they run past Nyquist immediately, and clamping them all into the
    /// same place makes the pre-warped pole/zero ratio enormous — a gentle
    /// slope turning into tens of decibels of flat gain.
    #[test]
    fn a_corner_near_nyquist_adds_nothing_rather_than_exploding() {
        let sos = sections(15_626.0, 0.67, SR, false);
        for db in compute_magnitude_response(&sos, &[100.0, 1000.0, 10_000.0, 20_000.0], SR) {
            assert!(
                db.abs() < 6.0,
                "a ladder with no room must stay small, got {db:+.2} dB",
            );
        }
    }

    /// A high cut rolls off upward instead.
    #[test]
    fn a_high_cut_ladder_falls_above_the_corner() {
        let sos = sections(1000.0, 0.5, SR, false);
        let got = slope_db_per_oct(&sos, 1000.0, 8000.0);
        assert!(
            (got + 3.0).abs() < 1.0,
            "a 0.5 fraction high cut should fall at -3 dB/oct, measured {got:.2}",
        );
    }
}
