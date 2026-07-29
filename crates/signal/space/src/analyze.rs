//! One-shot analyzer: decode → mono mixdown → trim/resample → feature vector.
//!
//! The embedding (see `docs/spec/sample-space.md`): a 48-band log-spaced
//! spectrum in floored, mean-centered dB (level- and tilt-invariant — the
//! `spectral_ab` metric that already proved out for sample A/B), plus a
//! small set of shape features (envelope, brightness, noisiness) that carry
//! the "how it moves" information a magnitude spectrum can't. Shape features
//! are weighted up so 48 spectrum dims don't drown them.

use realfft::RealFftPlanner;

/// Analysis sample rate — full-band content above ~11 kHz contributes little
/// to one-shot similarity and halving the rate quarters the FFT work.
pub const ANALYSIS_SR: u32 = 22_050;
/// Analysis window: enough for any drum one-shot body + early tail.
pub const MAX_ANALYSIS_S: f32 = 1.5;
pub const BANDS: usize = 48;
/// Shape features appended after the bands.
pub const SHAPE_DIMS: usize = 8;
pub const DIM: usize = BANDS + SHAPE_DIMS;
/// Weight multiplier on shape dims (vs unit-weight spectrum bands).
const SHAPE_WEIGHT: f32 = 3.0;

/// Everything the analyzer learns about one asset.
#[derive(Debug, Clone)]
pub struct Analysis {
    pub features: [f32; DIM],
    pub duration_s: f32,
    pub centroid_hz: f32,
    pub rms_db: f32,
    pub percussiveness: f32,
    pub attack_ms: f32,
    pub decay_ms: f32,
    /// Fraction of total energy below 150 Hz / in 150 Hz–1 kHz / above 4 kHz
    /// (classify's raw material).
    pub band_energy: [f32; 3],
    pub zcr: f32,
    pub flatness: f32,
}

/// Decode a wav file to mono at [`ANALYSIS_SR`], trimmed to leading silence
/// removed + [`MAX_ANALYSIS_S`]. Returns the mono buffer and the full
/// (untrimmed) duration in seconds.
pub fn decode_wav_mono(path: &std::path::Path) -> Result<(Vec<f32>, f32), String> {
    let mut reader = hound::WavReader::open(path).map_err(|e| e.to_string())?;
    let spec = reader.spec();
    let sr = spec.sample_rate;
    let ch = spec.channels.max(1) as usize;
    let mut mono: Vec<f32> = Vec::new();
    match spec.sample_format {
        hound::SampleFormat::Float => {
            let mut acc = 0.0f32;
            for (i, s) in reader.samples::<f32>().enumerate() {
                acc += s.map_err(|e| e.to_string())?;
                if i % ch == ch - 1 {
                    mono.push(acc / ch as f32);
                    acc = 0.0;
                }
            }
        }
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1i64 << (spec.bits_per_sample - 1)) as f32;
            let mut acc = 0.0f32;
            for (i, s) in reader.samples::<i32>().enumerate() {
                acc += s.map_err(|e| e.to_string())? as f32 * scale;
                if i % ch == ch - 1 {
                    mono.push(acc / ch as f32);
                    acc = 0.0;
                }
            }
        }
    }
    let full_dur = mono.len() as f32 / sr.max(1) as f32;
    // Trim leading silence (< -60 dBFS), keep 2 ms pre-roll.
    let thr = 10f32.powf(-60.0 / 20.0);
    let start = mono.iter().position(|s| s.abs() > thr).unwrap_or(0);
    let start = start.saturating_sub((sr as usize) / 500);
    mono.drain(..start);
    // Naive linear resample to the analysis rate (fidelity is irrelevant for
    // similarity features; speed matters across 10^5 files).
    if sr != ANALYSIS_SR {
        let ratio = sr as f64 / ANALYSIS_SR as f64;
        let out_len = (mono.len() as f64 / ratio) as usize;
        let mut out = Vec::with_capacity(out_len);
        for i in 0..out_len {
            let pos = i as f64 * ratio;
            let i0 = pos as usize;
            let frac = (pos - i0 as f64) as f32;
            let a = mono.get(i0).copied().unwrap_or(0.0);
            let b = mono.get(i0 + 1).copied().unwrap_or(a);
            out.push(a + (b - a) * frac);
        }
        mono = out;
    }
    mono.truncate((MAX_ANALYSIS_S * ANALYSIS_SR as f32) as usize);
    Ok((mono, full_dur))
}

/// Analyze a mono buffer at [`ANALYSIS_SR`].
pub fn analyze(mono: &[f32], full_duration_s: f32) -> Option<Analysis> {
    if mono.len() < 256 {
        return None;
    }
    let sr = ANALYSIS_SR as f32;

    // ── averaged magnitude spectrum (2048/512 hop Hann) ──
    let fft_size = 2048usize;
    let hop = 512usize;
    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(fft_size);
    let window: Vec<f32> = (0..fft_size)
        .map(|i| {
            let p = i as f32 / (fft_size - 1) as f32;
            0.5 - 0.5 * (2.0 * core::f32::consts::PI * p).cos()
        })
        .collect();
    let mut in_buf = fft.make_input_vec();
    let mut out_buf = fft.make_output_vec();
    let n_bins = fft_size / 2 + 1;
    let mut avg_mag = vec![0.0f32; n_bins];
    let mut frames = 0usize;
    let mut pos = 0usize;
    while pos < mono.len() {
        for i in 0..fft_size {
            in_buf[i] = mono.get(pos + i).copied().unwrap_or(0.0) * window[i];
        }
        if fft.process(&mut in_buf, &mut out_buf).is_ok() {
            for (m, c) in avg_mag.iter_mut().zip(out_buf.iter()) {
                *m += c.norm();
            }
            frames += 1;
        }
        pos += hop;
    }
    if frames == 0 {
        return None;
    }
    for m in avg_mag.iter_mut() {
        *m /= frames as f32;
    }

    // ── 48 log-spaced bands, 30 Hz .. Nyquist, floored dB, mean-centered ──
    let f_lo = 30.0f32;
    let f_hi = sr / 2.0;
    let bin_hz = sr / fft_size as f32;
    let mut bands = [0.0f32; BANDS];
    for (b, band) in bands.iter_mut().enumerate() {
        let lo = f_lo * (f_hi / f_lo).powf(b as f32 / BANDS as f32);
        let hi = f_lo * (f_hi / f_lo).powf((b + 1) as f32 / BANDS as f32);
        let (i0, i1) = ((lo / bin_hz) as usize, ((hi / bin_hz) as usize).max((lo / bin_hz) as usize + 1));
        let mut e = 0.0f32;
        for m in avg_mag.iter().take(i1.min(n_bins)).skip(i0.min(n_bins - 1)) {
            e += m * m;
        }
        *band = 10.0 * (e.max(1e-12)).log10();
    }
    let mean = bands.iter().sum::<f32>() / BANDS as f32;
    let floor = mean - 40.0;
    for b in bands.iter_mut() {
        *b = (*b - mean).max(floor - mean) / 40.0; // ≈ -1..+1
    }

    // ── scalar features ──
    let mut num = 0.0f32;
    let mut den = 0.0f32;
    for (i, m) in avg_mag.iter().enumerate() {
        num += i as f32 * bin_hz * m;
        den += m;
    }
    let centroid_hz = if den > 0.0 { num / den } else { 0.0 };
    // Spectral flatness (geo/arith mean of magnitudes) — noisiness.
    let (mut logsum, mut sum) = (0.0f64, 0.0f64);
    for &m in avg_mag.iter().skip(1) {
        logsum += (m.max(1e-12) as f64).ln();
        sum += m as f64;
    }
    let n = (n_bins - 1) as f64;
    let flatness = ((logsum / n).exp() / (sum / n).max(1e-12)) as f32;
    let zcr = {
        let mut z = 0usize;
        for w in mono.windows(2) {
            if (w[0] >= 0.0) != (w[1] >= 0.0) {
                z += 1;
            }
        }
        z as f32 / mono.len() as f32
    };
    let rms = (mono.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>() / mono.len() as f64)
        .sqrt() as f32;
    let rms_db = 20.0 * rms.max(1e-6).log10();

    // ── envelope: 5 ms RMS frames → attack / decay / percussiveness ──
    let frame = (sr * 0.005) as usize;
    let env: Vec<f32> = mono
        .chunks(frame)
        .map(|c| (c.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>() / c.len() as f64).sqrt() as f32)
        .collect();
    let peak_i = env
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(i, _)| i)
        .unwrap_or(0);
    let peak = env[peak_i].max(1e-6);
    let attack_ms = (peak_i as f32) * 5.0;
    let decay_ms = env[peak_i..]
        .iter()
        .position(|&e| e < peak * 0.1)
        .map(|i| i as f32 * 5.0)
        .unwrap_or((env.len() - peak_i) as f32 * 5.0);
    // Percussive = energy concentrated right after the peak.
    let total_e: f32 = env.iter().map(|e| e * e).sum();
    let head_e: f32 = env[peak_i..(peak_i + 20).min(env.len())].iter().map(|e| e * e).sum();
    let percussiveness = if total_e > 0.0 { (head_e / total_e).clamp(0.0, 1.0) } else { 0.0 };

    // ── coarse band-energy split for classification ──
    let mut split = [0.0f32; 3];
    let mut tot = 0.0f32;
    for (i, m) in avg_mag.iter().enumerate() {
        let f = i as f32 * bin_hz;
        let e = m * m;
        tot += e;
        if f < 150.0 {
            split[0] += e;
        } else if f < 1000.0 {
            split[1] += e;
        } else if f >= 4000.0 {
            split[2] += e;
        }
    }
    if tot > 0.0 {
        for s in split.iter_mut() {
            *s /= tot;
        }
    }

    // ── assemble the vector ──
    let mut features = [0.0f32; DIM];
    features[..BANDS].copy_from_slice(&bands);
    let shape = [
        (attack_ms.max(0.1).ln() / 8.0).clamp(-1.0, 1.0),
        (decay_ms.max(1.0).ln() / 8.0).clamp(0.0, 1.0),
        (full_duration_s.max(0.01).ln() / 5.0).clamp(-1.0, 1.0),
        percussiveness,
        (centroid_hz.max(20.0).ln() - 6.0) / 4.0, // ~20 Hz..11 kHz → ~-1..1
        flatness.clamp(0.0, 1.0),
        (zcr * 4.0).clamp(0.0, 1.0),
        split[0], // sub weight — kick-ness matters a lot
    ];
    for (i, s) in shape.iter().enumerate() {
        features[BANDS + i] = s * SHAPE_WEIGHT;
    }

    Some(Analysis {
        features,
        duration_s: full_duration_s,
        centroid_hz,
        rms_db,
        percussiveness,
        attack_ms,
        decay_ms,
        band_energy: split,
        zcr,
        flatness,
    })
}
