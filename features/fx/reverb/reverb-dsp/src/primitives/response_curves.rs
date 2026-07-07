//! CloudSeed parameter response curves.
//!
//! Ported from CloudSeedCore Utils.h (MIT, Ghost Note Audio).
//! These curves map linear [0, 1] parameter values to perceptually
//! useful ranges using decade (log10) and octave (log2) scaling.

/// 1-decade response: 10x range, perceptual log scaling.
pub fn resp1dec(x: f64) -> f64 {
    const MULT: f64 = (10.0 / 9.0) * 0.1;
    (10.0_f64.powf(x) - 1.0) * MULT
}

/// 2-decade response: 100x range.
pub fn resp2dec(x: f64) -> f64 {
    const MULT: f64 = (100.0 / 99.0) * 0.01;
    (10.0_f64.powf(2.0 * x) - 1.0) * MULT
}

/// 3-decade response: 1000x range.
pub fn resp3dec(x: f64) -> f64 {
    const MULT: f64 = (1000.0 / 999.0) * 0.001;
    (10.0_f64.powf(3.0 * x) - 1.0) * MULT
}

/// 4-decade response: 10000x range.
#[allow(dead_code)]
pub fn resp4dec(x: f64) -> f64 {
    const MULT: f64 = (10000.0 / 9999.0) * 0.0001;
    (10.0_f64.powf(4.0 * x) - 1.0) * MULT
}

/// 1-octave response: 2x range.
#[allow(dead_code)]
pub fn resp1oct(x: f64) -> f64 {
    const MULT: f64 = (2.0 / 1.0) * 0.5;
    (2.0_f64.powf(x) - 1.0) * MULT
}

/// 2-octave response: 4x range.
#[allow(dead_code)]
pub fn resp2oct(x: f64) -> f64 {
    const MULT: f64 = (4.0 / 3.0) * 0.25;
    (2.0_f64.powf(2.0 * x) - 1.0) * MULT
}

/// 3-octave response: 8x range.
pub fn resp3oct(x: f64) -> f64 {
    const MULT: f64 = (8.0 / 7.0) * 0.125;
    (2.0_f64.powf(3.0 * x) - 1.0) * MULT
}

/// 4-octave response: 16x range.
pub fn resp4oct(x: f64) -> f64 {
    const MULT: f64 = (16.0 / 15.0) * 0.0625;
    (2.0_f64.powf(4.0 * x) - 1.0) * MULT
}

/// 5-octave response: 32x range.
#[allow(dead_code)]
pub fn resp5oct(x: f64) -> f64 {
    const MULT: f64 = (32.0 / 31.0) * 0.03125;
    (2.0_f64.powf(5.0 * x) - 1.0) * MULT
}

/// Convert dB to linear gain.
pub fn db2gain(db: f64) -> f64 {
    10.0_f64.powf(db * 0.05)
}

/// Convert linear gain to dB.
#[allow(dead_code)]
pub fn gain2db(gain: f64) -> f64 {
    gain.log10() * 20.0
}
