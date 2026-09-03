//! A bandpass band actually band-passes.
//!
//! It used to be routed through the "gain is a flat output trim" path that
//! Notch uses, which skipped the filter entirely: a bandpass with -6 dB of
//! gain came out as a flat -6 dB across the whole spectrum. Measured against
//! Pro-Q 4 that was 97 dB of mean error — the plugin peaks at its centre and
//! is over 100 dB down two octaves away.

use eq_dsp::band::Band;
use eq_dsp::design::FilterType;

const SR: f64 = 48_000.0;

/// Steady-state gain of a band at `freq`, in dB.
fn gain_db_at(band: &mut Band, freq: f64) -> f64 {
    let inc = std::f64::consts::TAU * freq / SR;
    let n = 48_000;
    let (mut acc_in, mut acc_out) = (0.0f64, 0.0f64);
    for i in 0..n {
        let x = (inc * f64::from(i)).sin();
        let y = band.tick(x, 0);
        if i > n / 2 {
            acc_in += x * x;
            acc_out += y * y;
        }
    }
    10.0 * (acc_out / acc_in).log10()
}

fn bandpass(gain_db: f64) -> Band {
    let mut b = Band::new();
    b.filter_type = FilterType::Bandpass;
    b.freq_hz = 1000.0;
    b.q = 2.5;
    b.gain_db = gain_db;
    b.order = 2;
    b.enabled = true;
    b.update(SR);
    b
}

#[test]
fn a_bandpass_passes_its_band_and_rejects_the_rest() {
    let mut b = bandpass(0.0);
    let centre = gain_db_at(&mut b, 1000.0);
    let mut b = bandpass(0.0);
    let low = gain_db_at(&mut b, 100.0);
    let mut b = bandpass(0.0);
    let high = gain_db_at(&mut b, 10_000.0);

    assert!(centre > -3.0, "the passband should pass ({centre:+.2} dB)");
    assert!(
        low < centre - 20.0 && high < centre - 20.0,
        "a decade either side must be rejected: {low:+.2} / {high:+.2} against \
         {centre:+.2} dB at centre",
    );
}

/// Gain on a bandpass must not replace the filter.
///
/// This is the exact failure: with gain set, the band stopped filtering and
/// became a flat trim.
#[test]
fn gain_does_not_turn_a_bandpass_into_a_flat_trim() {
    let mut b = bandpass(-6.0);
    let centre = gain_db_at(&mut b, 1000.0);
    let mut b = bandpass(-6.0);
    let far = gain_db_at(&mut b, 10_000.0);
    assert!(
        far < centre - 20.0,
        "with gain set the band stopped filtering: {far:+.2} dB out of band \
         against {centre:+.2} dB in it",
    );
}
