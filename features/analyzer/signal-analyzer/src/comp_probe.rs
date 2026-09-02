//! Characterising a compressor: what it does, at every level and every frequency.
//!
//! The equalizer work could lean on a single broadband-noise transfer
//! function, because an EQ's whole behaviour is one curve. A compressor's is
//! not: it is a gain that *moves*, and how fast it moves depends on the level
//! that provoked it and on the frequency it heard. Measuring it with noise —
//! which is what [`crate::eq_transfer`] does, and what `fts-convert --verify`
//! still does — answers only how much it pulled down on average. Two
//! compressors that agree on that can still sound nothing alike.
//!
//! So the stimulus here is a **pulsing tone**: a carrier at one frequency
//! whose amplitude alternates between a loud and a quiet level on a fixed
//! schedule. The compressor chases that square level envelope, and because
//! the carrier's own amplitude is known at every sample, the gain it applied
//! can be read straight back out of the output. Sweep the carrier across the
//! audible range and the same measurement also says whether the detector is
//! frequency-weighted — which is the half of the question noise cannot ask.
//!
//! What one capture therefore contains, per frequency:
//!
//! - **Static gain**, from the settled portion of each half of the cycle:
//!   threshold, ratio and knee fall out of the loud-level gain across a level
//!   sweep.
//! - **Attack**, from the corner where the level steps up.
//! - **Release**, from the corner where it steps back down — including hold,
//!   auto-release and any program dependence, which show up as a shape no
//!   single time constant fits.
//!
//! This is a port of the harness in the legacy `fts-analyzer` repo, which is
//! where the method was worked out and which drove the reference captures the
//! Pro-C 3 modelling used. Three things changed in the move, all noted at the
//! definitions below: gain is read as a **windowed RMS ratio** rather than a
//! mean of per-sample log ratios, the plugin's reported **latency is removed**
//! before anything is measured, and presets are matched **by parameter name**
//! rather than by position.
//!
//! The module hosts no plugins — callers render the buffers and hand them
//! over, which is the same contract the rest of this crate keeps and what
//! lets it be tested with nothing installed.

use std::io::{Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Stimulus
// ---------------------------------------------------------------------------

/// The carrier shape under the pulsing amplitude envelope.
///
/// `Sine` is the one to reach for: it puts all of its energy at the frequency
/// being tested, so a frequency-weighted detector is measured at the point it
/// is being asked about. The other two exist because a compressor's detector
/// may be fed something closer to a real waveform's crest factor, and a square
/// carrier is the cheapest way to see whether peak-versus-RMS detection is in
/// play — its crest factor is 0 dB against a sine's 3 dB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Waveform {
    #[default]
    Sine,
    Square,
    Saw,
}

/// How the pulsing tone is shaped.
///
/// The defaults are the ones the Pro-C 3 reference captures used: a 14 dB step
/// between -20 and -6 dBFS, 240 ms at each level, three seconds in total. That
/// pairing matters — 240 ms is long enough for all but the slowest release to
/// settle, so each cycle carries a full attack, a settled plateau, a full
/// release and a settled floor, and three seconds gives six cycles to average
/// over.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PulseSpec {
    /// Carrier frequency in Hz.
    pub freq_hz: f32,
    /// The loud level, in dBFS — the one that provokes gain reduction.
    pub gain_high_db: f32,
    /// The quiet level, in dBFS — low enough to sit under the threshold.
    pub gain_low_db: f32,
    /// How long the loud half lasts, in milliseconds.
    pub time_high_ms: f32,
    /// How long the quiet half lasts, in milliseconds.
    pub time_low_ms: f32,
    pub waveform: Waveform,
    /// Total length of the stimulus, in seconds.
    pub duration_s: f32,
}

impl Default for PulseSpec {
    fn default() -> Self {
        Self {
            freq_hz: 1000.0,
            gain_high_db: -6.0,
            gain_low_db: -20.0,
            time_high_ms: 240.0,
            time_low_ms: 240.0,
            waveform: Waveform::Sine,
            duration_s: 3.0,
        }
    }
}

impl PulseSpec {
    /// The same spec at another carrier frequency — the sweep axis.
    pub fn at(&self, freq_hz: f32) -> Self {
        Self { freq_hz, ..*self }
    }

    /// Length of the rendered stimulus in samples.
    pub fn samples(&self, sample_rate: f64) -> usize {
        (sample_rate * self.duration_s as f64) as usize
    }
}

/// Render the pulsing tone.
///
/// The phase accumulates in `f64` and is never reset at a level change, so the
/// carrier is continuous across the step and the only discontinuity in the
/// signal is the amplitude one the compressor is meant to react to. Resetting
/// phase per segment would put a click at every corner and measure the
/// plugin's response to that instead.
pub fn pulse_tone(spec: &PulseSpec, sample_rate: f64) -> Vec<f32> {
    let length = spec.samples(sample_rate);
    let high_lin = 10.0f32.powf(spec.gain_high_db / 20.0);
    let low_lin = 10.0f32.powf(spec.gain_low_db / 20.0);
    let high_samples = (spec.time_high_ms as f64 * sample_rate / 1000.0) as usize;
    let low_samples = (spec.time_low_ms as f64 * sample_rate / 1000.0) as usize;
    let cycle = high_samples + low_samples;

    let mut phase = 0.0f64;
    let phase_inc = std::f64::consts::TAU * spec.freq_hz as f64 / sample_rate;

    (0..length)
        .map(|i| {
            let gain = if cycle > 0 && i % cycle < high_samples {
                high_lin
            } else if cycle > 0 {
                low_lin
            } else {
                high_lin
            };

            let turns = phase / std::f64::consts::TAU;
            let carrier = match spec.waveform {
                Waveform::Sine => phase.sin() as f32,
                Waveform::Square => {
                    if turns.fract() < 0.5 {
                        1.0
                    } else {
                        -1.0
                    }
                }
                Waveform::Saw => (2.0 * turns.fract() - 1.0) as f32,
            };
            phase += phase_inc;
            carrier * gain
        })
        .collect()
}

/// Musically-spaced carrier frequencies, dense where a compressor's detector
/// varies most.
///
/// Below a couple of hundred hertz a peak detector starts to follow individual
/// cycles rather than the envelope, so the bottom is sampled every 10-20 Hz;
/// above 1 kHz the behaviour is nearly flat and the spacing widens. Thirty-four
/// points at three seconds each is under two minutes of audio per scenario.
pub const TEST_FREQUENCIES: &[f32] = &[
    20.0, 30.0, 40.0, 50.0, 60.0, 80.0, 100.0, // sub and low bass
    120.0, 140.0, 160.0, 180.0, 200.0, 240.0, 280.0, 320.0, // bass
    400.0, 480.0, 560.0, 640.0, 800.0, // low mids
    1000.0, 1250.0, 1500.0, 2000.0, 2500.0, // mids
    3000.0, 4000.0, 5000.0, 6000.0, // upper mids
    8000.0, 10000.0, 12000.0, 16000.0, 20000.0, // highs
];

// ---------------------------------------------------------------------------
// Reading the gain back out
// ---------------------------------------------------------------------------

/// Per-window gain, in dB, as an RMS ratio of output to input.
///
/// **Why RMS and not the per-sample ratio.** The legacy implementation took
/// `20·log10(|out|/|in|)` at every sample and averaged those. Near a zero
/// crossing both terms approach zero, so the ratio there is dominated by
/// whatever each side rounded to — and a 20 Hz carrier at 48 kHz spends a
/// large share of its samples near a crossing. Averaging in the log domain
/// then lets those outliers pull the whole window. Taking the ratio of the two
/// windows' RMS instead weights each sample by its energy, which is the same
/// number when the gain is steady and a far better behaved one when it is not.
///
/// `window` is in samples; at 48 kHz, 48 samples is the 1 ms row the reference
/// captures use. Windows shorter than one carrier cycle will read the carrier's
/// own shape rather than the compressor's gain, so keep `window` at or above
/// `sample_rate / freq_hz` for the lowest frequency in the sweep.
///
/// Returns one value per window. A window whose input is silent yields 0 dB
/// rather than negative infinity — no signal is not the same as full
/// attenuation, and the distinction matters when the quiet half of the pulse
/// sits near the noise floor.
pub fn gain_reduction_db(input: &[f32], output: &[f32], window: usize) -> Vec<f32> {
    let window = window.max(1);
    let len = input.len().min(output.len());
    let mut out = Vec::with_capacity(len / window + 1);

    let mut i = 0;
    while i < len {
        let end = (i + window).min(len);
        let mut in_energy = 0.0f64;
        let mut out_energy = 0.0f64;
        for j in i..end {
            in_energy += (input[j] as f64) * (input[j] as f64);
            out_energy += (output[j] as f64) * (output[j] as f64);
        }
        // Silence in is unmeasurable, not infinitely attenuated.
        let gain = if in_energy <= 1e-20 {
            0.0
        } else {
            10.0 * (out_energy.max(1e-30) / in_energy).log10()
        };
        out.push(gain as f32);
        i = end;
    }
    out
}

/// Drop the plugin's reported latency from the head of its output, so input
/// and output line up sample-for-sample before any gain is read.
///
/// A lookahead compressor reports tens of milliseconds here. Measured without
/// this, its attack appears to begin *before* the level step — the corner
/// lands at a negative time — and every time constant read off that corner is
/// wrong by the latency. The legacy capture recorded latency in its metadata
/// but never applied it, which is safe only because Pro-C reported zero.
pub fn align_latency(output: &[f32], latency_samples: usize) -> &[f32] {
    output.get(latency_samples..).unwrap_or(&[])
}

// ---------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------

/// Quantisation range for the stored `u8` gain values.
///
/// Gain, not gain *reduction*: a compressor with makeup applied is above 0 dB,
/// so the range has to carry positive values too. -48 dB is below any usable
/// reduction and +6 dB above any sane makeup, giving 0.21 dB per step — an
/// order of magnitude finer than the differences the comparison cares about,
/// at a quarter the size of `f32`.
const GAIN_MIN_DB: f32 = -48.0;
const GAIN_MAX_DB: f32 = 6.0;

fn gain_to_u8(db: f32) -> u8 {
    let clamped = db.clamp(GAIN_MIN_DB, GAIN_MAX_DB);
    ((clamped - GAIN_MIN_DB) / (GAIN_MAX_DB - GAIN_MIN_DB) * 255.0).round() as u8
}

fn u8_to_gain(v: u8) -> f32 {
    GAIN_MIN_DB + (v as f32 / 255.0) * (GAIN_MAX_DB - GAIN_MIN_DB)
}

/// Write one scenario's gain curves — all frequencies — to a single file.
///
/// Layout: `[num_freqs: u32 LE][rows_per_freq: u32 LE][u8 × num_freqs × rows]`,
/// frequency-major. At 34 frequencies and 3000 one-millisecond rows that is
/// 102 kB per scenario, which is what makes capturing two hundred of them and
/// carrying them between machines a non-event.
pub fn write_capture(path: &Path, per_freq: &[Vec<f32>]) -> std::io::Result<()> {
    let num_freqs = per_freq.len() as u32;
    let rows = per_freq.first().map(|v| v.len()).unwrap_or(0) as u32;

    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    f.write_all(&num_freqs.to_le_bytes())?;
    f.write_all(&rows.to_le_bytes())?;
    for curve in per_freq {
        let quantised: Vec<u8> = curve.iter().map(|&v| gain_to_u8(v)).collect();
        f.write_all(&quantised)?;
    }
    f.flush()
}

/// Read back what [`write_capture`] wrote.
pub fn read_capture(path: &Path) -> std::io::Result<Vec<Vec<f32>>> {
    let mut f = std::io::BufReader::new(std::fs::File::open(path)?);
    let mut header = [0u8; 8];
    f.read_exact(&mut header)?;
    let num_freqs = u32::from_le_bytes(header[0..4].try_into().unwrap()) as usize;
    let rows = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;

    let mut out = Vec::with_capacity(num_freqs);
    for _ in 0..num_freqs {
        let mut buf = vec![0u8; rows];
        f.read_exact(&mut buf)?;
        out.push(buf.iter().map(|&v| u8_to_gain(v)).collect());
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Comparison
// ---------------------------------------------------------------------------

/// How far apart two gain curves are.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GainComparison {
    /// RMS of the per-row difference. The headline number.
    pub rms_diff_db: f32,
    /// The worst single row — where a wrong time constant shows up even when
    /// the settled levels agree.
    pub max_diff_db: f32,
    /// Difference in the settled gain, ignoring the corners entirely. A large
    /// `max` with a small `settled` is a timing error; both large is a static
    /// curve error.
    pub settled_diff_db: f32,
}

/// Compare two gain curves row for row.
///
/// `settled` is the fraction of each row range treated as already settled —
/// the last 40% of the curve by default in [`compare_gain_curves`].
pub fn compare_gain_curves_with(reference: &[f32], test: &[f32], settled_from: f32) -> GainComparison {
    let len = reference.len().min(test.len());
    if len == 0 {
        return GainComparison {
            rms_diff_db: f32::NAN,
            max_diff_db: f32::NAN,
            settled_diff_db: f32::NAN,
        };
    }

    let mut sum_sq = 0.0f64;
    let mut max_diff = 0.0f32;
    for i in 0..len {
        let d = (test[i] - reference[i]).abs();
        sum_sq += (d as f64) * (d as f64);
        max_diff = max_diff.max(d);
    }

    let start = ((len as f32 * settled_from) as usize).min(len.saturating_sub(1));
    let settled: f32 = {
        let n = (len - start).max(1);
        let mean_ref: f32 = reference[start..len].iter().sum::<f32>() / n as f32;
        let mean_test: f32 = test[start..len].iter().sum::<f32>() / n as f32;
        (mean_test - mean_ref).abs()
    };

    GainComparison {
        rms_diff_db: (sum_sq / len as f64).sqrt() as f32,
        max_diff_db: max_diff,
        settled_diff_db: settled,
    }
}

/// [`compare_gain_curves_with`] at the default settling point.
pub fn compare_gain_curves(reference: &[f32], test: &[f32]) -> GainComparison {
    compare_gain_curves_with(reference, test, 0.6)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48_000.0;

    fn spec() -> PulseSpec {
        PulseSpec { freq_hz: 1000.0, duration_s: 1.0, ..Default::default() }
    }

    #[test]
    fn pulse_tone_alternates_between_the_two_levels() {
        let s = spec();
        let x = pulse_tone(&s, SR);
        assert_eq!(x.len(), 48_000);

        // Peak inside the first (loud) segment and inside the second (quiet).
        let seg = (s.time_high_ms as f64 * SR / 1000.0) as usize;
        let loud = x[..seg].iter().fold(0.0f32, |a, b| a.max(b.abs()));
        let quiet = x[seg..2 * seg].iter().fold(0.0f32, |a, b| a.max(b.abs()));

        let want_loud = 10.0f32.powf(s.gain_high_db / 20.0);
        let want_quiet = 10.0f32.powf(s.gain_low_db / 20.0);
        assert!((loud - want_loud).abs() < 1e-3, "loud {loud} vs {want_loud}");
        assert!((quiet - want_quiet).abs() < 1e-3, "quiet {quiet} vs {want_quiet}");
    }

    #[test]
    fn the_carrier_is_phase_continuous_across_a_level_change() {
        // A phase reset at the corner would put a step in the waveform far
        // larger than one sample's worth of a 1 kHz sine.
        let s = spec();
        let x = pulse_tone(&s, SR);
        let seg = (s.time_high_ms as f64 * SR / 1000.0) as usize;
        // Compare the slope either side of the boundary, scaled out of the
        // amplitude change: the *phase* should march on undisturbed.
        let before = x[seg - 1] / 10.0f32.powf(s.gain_high_db / 20.0);
        let after = x[seg] / 10.0f32.powf(s.gain_low_db / 20.0);
        let step = std::f32::consts::TAU * 1000.0 / SR as f32;
        assert!((after - before).abs() < step * 2.0, "phase jumped: {before} -> {after}");
    }

    #[test]
    fn gain_reduction_reads_a_known_flat_gain() {
        let s = spec();
        let x = pulse_tone(&s, SR);
        // A plain -6 dB pad applied to everything.
        let pad = 10.0f32.powf(-6.0 / 20.0);
        let y: Vec<f32> = x.iter().map(|v| v * pad).collect();
        let g = gain_reduction_db(&x, &y, 48);
        assert!(!g.is_empty());
        for v in &g {
            assert!((v + 6.0).abs() < 0.01, "expected -6 dB, got {v}");
        }
    }

    #[test]
    fn silence_in_reads_as_no_measurement_rather_than_minus_infinity() {
        let g = gain_reduction_db(&[0.0; 96], &[0.0; 96], 48);
        assert_eq!(g, vec![0.0, 0.0]);
        assert!(g.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn a_low_carrier_does_not_fool_the_rms_window() {
        // 20 Hz at 48 kHz spends most of its samples near a zero crossing —
        // the case the per-sample log ratio handled badly.
        let s = PulseSpec { freq_hz: 20.0, duration_s: 1.0, ..Default::default() };
        let x = pulse_tone(&s, SR);
        let pad = 10.0f32.powf(-3.0 / 20.0);
        let y: Vec<f32> = x.iter().map(|v| v * pad).collect();
        for v in gain_reduction_db(&x, &y, 2400) {
            assert!((v + 3.0).abs() < 0.01, "expected -3 dB, got {v}");
        }
    }

    #[test]
    fn latency_alignment_shifts_the_output() {
        let out = [0.0, 0.0, 1.0, 2.0];
        assert_eq!(align_latency(&out, 2), &[1.0, 2.0]);
        // Asking for more latency than there is output is empty, not a panic.
        assert!(align_latency(&out, 99).is_empty());
    }

    #[test]
    fn quantisation_round_trips_inside_half_a_step() {
        // 54 dB over 255 steps — half a step is ~0.106 dB.
        for db in [-48.0, -30.0, -12.5, -6.0, 0.0, 3.3, 6.0] {
            let back = u8_to_gain(gain_to_u8(db));
            assert!((back - db).abs() < 0.107, "{db} -> {back}");
        }
        // Out of range clamps rather than wrapping.
        assert_eq!(gain_to_u8(-100.0), 0);
        assert_eq!(gain_to_u8(100.0), 255);
    }

    #[test]
    fn capture_files_round_trip() {
        let dir = std::env::temp_dir().join("comp_probe_roundtrip");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("scenario.bin");

        let data = vec![vec![-1.0f32, -2.0, -3.0], vec![0.0, -6.0, -12.0]];
        write_capture(&path, &data).unwrap();
        let back = read_capture(&path).unwrap();

        assert_eq!(back.len(), 2);
        assert_eq!(back[0].len(), 3);
        for (a, b) in data.iter().flatten().zip(back.iter().flatten()) {
            assert!((a - b).abs() < 0.107, "{a} -> {b}");
        }
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn comparison_separates_a_timing_error_from_a_level_error() {
        // Same settled value, different corner: timing.
        let reference = vec![0.0, -1.0, -4.0, -6.0, -6.0, -6.0, -6.0, -6.0, -6.0, -6.0];
        let slower = vec![0.0, 0.0, -1.0, -3.0, -5.0, -6.0, -6.0, -6.0, -6.0, -6.0];
        let timing = compare_gain_curves(&reference, &slower);
        assert!(timing.max_diff_db > 2.0, "{timing:?}");
        assert!(timing.settled_diff_db < 0.01, "{timing:?}");

        // Different settled value: level.
        let deeper: Vec<f32> = reference.iter().map(|v| v - 3.0).collect();
        let level = compare_gain_curves(&reference, &deeper);
        assert!((level.settled_diff_db - 3.0).abs() < 0.01, "{level:?}");
    }

    #[test]
    fn comparing_nothing_is_not_a_panic() {
        let c = compare_gain_curves(&[], &[]);
        assert!(c.rms_diff_db.is_nan());
    }
}
