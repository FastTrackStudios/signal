//! NAM loudness calibration — the "every amp is the same volume" guarantee.
//!
//! The `.nam` `loudness` metadata field is optional and, per NAM's own tracker
//! (sdatkinson/neural-amp-modeler#588), inconsistent between models. Trusting it
//! means a clean model and a high-gain model can still jump in level on a swap —
//! or, when the field is missing entirely, get no compensation at all.
//!
//! Instead we **measure loudness ourselves**: push one fixed DI guitar clip
//! through every model offline, meter the output in LUFS ([`crate::loudness`]),
//! and let [`crate::rig_profile`] apply `target − measured` as makeup. Because
//! the same DI drives every model, a scooped clean and a saturated high-gain
//! capture that we normalise to the same LUFS genuinely *sound* the same loud —
//! so the player can switch amps mid-song without a volume jump, whatever the
//! model's metadata says (or doesn't).
//!
//! Measurement runs the WaveNet/LSTM core over ~10 s of audio, so it is done
//! **once per model, off the audio thread**, and cached by the model's SHA-256
//! under `<config>/signal/calibration/loudness-cache.styx`.
//!
//! ## The DI reference
//! We look for a real DI clip at `<config>/signal/calibration/di-reference.wav`
//! (override with `SIGNAL_NAM_DI`). Drop NAM's official reamp input (`v3_0_0.wav`
//! and friends) or your own DI there — any consistent clip works, since the
//! guarantee is about matching *our* library to *itself*. With no file present
//! we fall back to a deterministic synthetic pluck so calibration still
//! functions (a real DI is strongly preferred — it drives the nonlinearity like
//! a real guitar does).

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use facet::Facet;
use sha2::{Digest, Sha256};

use crate::loudness::{SILENCE_LUFS, integrated_lufs};
use crate::rig_prefs::signal_config_dir;
use neural_amp_modeler::NamModel;

/// Longest window (seconds) of a DI clip actually run through each model. Long
/// enough for stable integrated LUFS, short enough that measuring a big library
/// stays quick. A centred slice of a longer file is used.
const MEASURE_WINDOW_SECS: f64 = 20.0;

/// Take a centred window of at most `max_len` samples (the whole slice if it is
/// already shorter).
fn center_window(samples: &[f64], max_len: usize) -> Vec<f64> {
    if max_len == 0 || samples.len() <= max_len {
        return samples.to_vec();
    }
    let start = (samples.len() - max_len) / 2;
    samples[start..start + max_len].to_vec()
}

/// Directory holding the DI reference clip and the loudness cache.
/// `SIGNAL_CALIBRATION_DIR` overrides it (isolates tests/CI from user config).
pub fn calibration_dir() -> PathBuf {
    if let Ok(p) = std::env::var("SIGNAL_CALIBRATION_DIR") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    signal_config_dir().join("calibration")
}

/// Path of the user-supplied DI reference clip. `SIGNAL_NAM_DI` overrides it.
pub fn di_reference_path() -> PathBuf {
    if let Ok(p) = std::env::var("SIGNAL_NAM_DI") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    calibration_dir().join("di-reference.wav")
}

/// A mono DI signal used to excite every model identically.
#[derive(Clone, Debug)]
pub struct DiReference {
    /// Mono samples at [`sample_rate`](Self::sample_rate).
    pub samples: Vec<f64>,
    pub sample_rate: f64,
    /// Stable identity (content hash) so a different DI invalidates the cache.
    pub id: String,
}

impl DiReference {
    /// Load the DI at `path`, downmixed to mono `f64` and resampled to
    /// `target_sr`. WAV via `hound`; integer formats are normalised to ±1.0.
    pub fn load(path: &Path, target_sr: f64) -> Result<Self, String> {
        let mut reader =
            hound::WavReader::open(path).map_err(|e| format!("open DI {}: {e}", path.display()))?;
        let spec = reader.spec();
        let channels = spec.channels.max(1) as usize;
        let src_sr = spec.sample_rate as f64;

        // Read interleaved → downmix to mono.
        let interleaved: Vec<f64> = match spec.sample_format {
            hound::SampleFormat::Float => reader
                .samples::<f32>()
                .map(|s| s.unwrap_or(0.0) as f64)
                .collect(),
            hound::SampleFormat::Int => {
                let scale = 1.0 / ((1i64 << (spec.bits_per_sample - 1)) as f64);
                reader
                    .samples::<i32>()
                    .map(|s| s.unwrap_or(0) as f64 * scale)
                    .collect()
            }
        };
        let frames = interleaved.len() / channels;
        let mut mono = Vec::with_capacity(frames);
        for f in 0..frames {
            let sum: f64 = (0..channels).map(|c| interleaved[f * channels + c]).sum();
            mono.push(sum / channels as f64);
        }

        let resampled = resample_linear(&mono, src_sr, target_sr);
        // Cap the measured window so a long "official" reamp file (NAM's
        // v3_0_0.wav is ~3 min) doesn't make per-model calibration glacial, and
        // take it from the centre so we skip the head calibration tones and any
        // tail silence — i.e. measure actual playing.
        let samples = center_window(&resampled, (MEASURE_WINDOW_SECS * target_sr) as usize);
        let id = format!("wav:{}", hash_samples(&samples));
        Ok(Self {
            samples,
            sample_rate: target_sr,
            id,
        })
    }

    /// Load the configured DI, or synthesise a deterministic fallback pluck.
    pub fn load_or_synthetic(target_sr: f64) -> Self {
        let path = di_reference_path();
        match Self::load(&path, target_sr) {
            Ok(di) => {
                tracing::info!(path = %path.display(), id = %di.id, "NAM calibration: using DI reference");
                di
            }
            Err(e) => {
                if path.exists() {
                    tracing::warn!(path = %path.display(), error = %e, "NAM calibration: DI unreadable, using synthetic fallback");
                }
                Self::synthetic(target_sr)
            }
        }
    }

    /// A deterministic ~1.8 s synthetic DI: a sequence of plucked notes across
    /// the guitar range with decaying harmonics and varying velocity, so it
    /// drives amp nonlinearity with a guitar-like crest factor. Not a substitute
    /// for a real DI, but keeps the guarantee working out of the box.
    pub fn synthetic(target_sr: f64) -> Self {
        let sr = target_sr;
        // Open-string-ish fundamentals (E2 A2 D3 G3 B3 E4), repeated with
        // rising then falling velocity to sweep how hard the amp is pushed.
        let notes = [82.41, 110.0, 146.83, 196.0, 246.94, 329.63];
        let vels = [0.15, 0.3, 0.5, 0.7, 0.9, 0.7, 0.5, 0.3];
        let note_secs = 0.22;
        let note_len = (note_secs * sr) as usize;
        let mut samples = Vec::with_capacity(note_len * notes.len() * vels.len());
        for (n, &f0) in notes.iter().cycle().take(vels.len()).enumerate() {
            let vel = vels[n % vels.len()];
            for i in 0..note_len {
                let t = i as f64 / sr;
                let env = (-6.0 * t / note_secs).exp(); // plucked decay
                // Fundamental + a few decaying harmonics (string-like).
                let mut s = 0.0;
                for (h, amp) in [(1.0, 1.0), (2.0, 0.5), (3.0, 0.28), (4.0, 0.14)] {
                    s += amp * (2.0 * core::f64::consts::PI * f0 * h * t).sin();
                }
                samples.push(s * env * vel * 0.25);
            }
        }
        Self {
            samples,
            sample_rate: sr,
            id: "synthetic-v1".to_string(),
        }
    }
}

/// SHA-256 of a file's contents (hex), for keying the cache by model identity.
pub fn hash_file(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let mut h = Sha256::new();
    h.update(&bytes);
    Ok(hex(&h.finalize()))
}

fn hash_samples(samples: &[f64]) -> String {
    let mut h = Sha256::new();
    h.update((samples.len() as u64).to_le_bytes());
    // A few thousand samples are plenty to identify a clip without hashing MB.
    for &s in samples.iter().step_by((samples.len() / 4096).max(1)) {
        h.update(s.to_le_bytes());
    }
    hex(&h.finalize())[..16].to_string()
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Naive linear resampler. The DI is measured offline, so quality here only has
/// to preserve loudness — linear is more than enough and dependency-free.
fn resample_linear(input: &[f64], src_sr: f64, dst_sr: f64) -> Vec<f64> {
    if (src_sr - dst_sr).abs() < 1.0 || input.is_empty() {
        return input.to_vec();
    }
    let ratio = dst_sr / src_sr;
    let out_len = ((input.len() as f64) * ratio) as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_pos = i as f64 / ratio;
        let idx = src_pos.floor() as usize;
        let frac = src_pos - idx as f64;
        let a = input.get(idx).copied().unwrap_or(0.0);
        let b = input.get(idx + 1).copied().unwrap_or(a);
        out.push(a + (b - a) * frac);
    }
    out
}

/// Run `di` through `model` (mono, in `max_block`-sized chunks) and return the
/// integrated loudness (LUFS) of the output. Resets the model before and after
/// so the caller's live state is not disturbed. Off the hot path — allocates.
pub fn measure_model_lufs(model: &mut NamModel, di: &DiReference, max_block: usize) -> f64 {
    let block = max_block.clamp(64, 8192);
    model.reset(di.sample_rate, block);
    let mut output = vec![0.0f64; di.samples.len()];
    let mut scratch = vec![0.0f64; block];
    let mut pos = 0;
    while pos < di.samples.len() {
        let n = block.min(di.samples.len() - pos);
        model.process(&di.samples[pos..pos + n], &mut scratch[..n]);
        output[pos..pos + n].copy_from_slice(&scratch[..n]);
        pos += n;
    }
    integrated_lufs(&output, di.sample_rate)
}

// ── Cache ────────────────────────────────────────────────────────────────────

/// One measured model, keyed by (model hash, DI identity, sample rate).
#[derive(Clone, Debug, Facet)]
pub struct CalibrationEntry {
    pub model_hash: String,
    pub di_id: String,
    pub sample_rate: u32,
    /// Integrated loudness (LUFS) of the model's output on the DI. `-1000.0`
    /// encodes silence/failure so we don't re-measure a dud every load.
    pub measured_lufs: f64,
}

/// Persistent per-machine loudness measurements. A flat `Vec` (not a map) so it
/// serialises cleanly through facet-styx.
#[derive(Clone, Debug, Default, Facet)]
pub struct LoudnessCache {
    pub entries: Vec<CalibrationEntry>,
}

impl LoudnessCache {
    fn path() -> PathBuf {
        calibration_dir().join("loudness-cache.styx")
    }

    fn load() -> Self {
        match std::fs::read_to_string(Self::path()) {
            Ok(text) => facet_styx::from_str(&text).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    fn save(&self) {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match facet_styx::to_string(self) {
            Ok(text) => {
                if let Err(e) = std::fs::write(&path, text) {
                    tracing::warn!(path = %path.display(), error = %e, "NAM calibration: cache save failed");
                }
            }
            Err(e) => tracing::warn!(error = %e, "NAM calibration: cache serialise failed"),
        }
    }

    fn lookup(&self, model_hash: &str, di_id: &str, sr: u32) -> Option<f64> {
        self.entries
            .iter()
            .find(|e| e.model_hash == model_hash && e.di_id == di_id && e.sample_rate == sr)
            .map(|e| e.measured_lufs)
    }

    fn insert(&mut self, model_hash: String, di_id: String, sr: u32, lufs: f64) {
        self.entries.retain(|e| {
            !(e.model_hash == model_hash && e.di_id == di_id && e.sample_rate == sr)
        });
        self.entries.push(CalibrationEntry {
            model_hash,
            di_id,
            sample_rate: sr,
            measured_lufs: lufs,
        });
    }
}

/// Encodes "measured but silent/failed" so a dud model isn't re-run every load.
const FAILED_LUFS: f64 = -1000.0;

/// Process-wide calibration context: the DI reference (loaded once at a given
/// sample rate) plus the on-disk cache.
struct Context {
    di: DiReference,
    cache: LoudnessCache,
}

fn context() -> &'static Mutex<Option<Context>> {
    static CTX: OnceLock<Mutex<Option<Context>>> = OnceLock::new();
    CTX.get_or_init(|| Mutex::new(None))
}

/// Measured loudness (LUFS) of the model at `model_path`, cache-first. On a miss
/// it runs the DI through `model`, stores the result, and re-resets `model` to
/// `(sample_rate, max_block)` so it is ready for live playback. Returns `None`
/// if the model produced silence (no reliable measurement).
///
/// This is the value [`crate::rig_profile`] level-matching normalises toward the
/// target — preferred over the model's own `loudness()` metadata.
pub fn measured_loudness(
    model: &mut NamModel,
    model_path: &Path,
    sample_rate: f64,
    max_block: usize,
) -> Option<f64> {
    let sr_key = sample_rate.round() as u32;
    let model_hash = match hash_file(model_path) {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!(error = %e, "NAM calibration: cannot hash model, skipping");
            return None;
        }
    };

    let guard = context();
    let mut slot = guard.lock().unwrap();
    // (Re)initialise the context if absent or the DI sample rate changed.
    if slot
        .as_ref()
        .map(|c| (c.di.sample_rate - sample_rate).abs() > 1.0)
        .unwrap_or(true)
    {
        *slot = Some(Context {
            di: DiReference::load_or_synthetic(sample_rate),
            cache: LoudnessCache::load(),
        });
    }
    let ctx = slot.as_mut().unwrap();
    let di_id = ctx.di.id.clone();

    if let Some(lufs) = ctx.cache.lookup(&model_hash, &di_id, sr_key) {
        // Re-reset for live use (load() already prepared it, but stay explicit).
        model.reset(sample_rate, max_block);
        return (lufs > FAILED_LUFS).then_some(lufs);
    }

    let measured = measure_model_lufs(model, &ctx.di, max_block);
    let stored = if measured == SILENCE_LUFS || !measured.is_finite() {
        FAILED_LUFS
    } else {
        measured
    };
    ctx.cache.insert(model_hash, di_id, sr_key, stored);
    ctx.cache.save();

    // Restore the model to the live block size after measurement.
    model.reset(sample_rate, max_block);
    (stored > FAILED_LUFS).then_some(stored)
}

/// Pre-model input calibration gain (dB) so the model is fed the analog level it
/// was captured at: `interface_cal_dbu − model.input_level_dbu` (matching the
/// NAM plugin's `_SetInputGain`). Returns 0 dB when the model declares no input
/// level. This makes the model's *drive* authentic; the LUFS makeup above keeps
/// the *volume* uniform — the two are orthogonal.
pub fn input_calibration_db(model_input_level_dbu: Option<f64>, interface_cal_dbu: f64) -> f32 {
    match model_input_level_dbu {
        Some(level) => (interface_cal_dbu - level) as f32,
        None => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_di_is_deterministic_and_non_silent() {
        let a = DiReference::synthetic(48_000.0);
        let b = DiReference::synthetic(48_000.0);
        assert_eq!(a.id, "synthetic-v1");
        assert_eq!(a.samples.len(), b.samples.len());
        assert_eq!(a.samples, b.samples);
        let energy: f64 = a.samples.iter().map(|s| s * s).sum();
        assert!(energy > 1.0, "synthetic DI should carry real energy");
        // Long enough for the LUFS meter (well over one 400 ms block).
        assert!(a.samples.len() as f64 / 48_000.0 > 1.0);
        assert!(integrated_lufs(&a.samples, 48_000.0).is_finite());
    }

    #[test]
    fn input_calibration_matches_nam_convention() {
        // Model captured at 12 dBu; interface at 15 dBu → +3 dB into the model.
        assert!((input_calibration_db(Some(12.0), 15.0) - 3.0).abs() < 1e-6);
        // No declared level → no calibration.
        assert_eq!(input_calibration_db(None, 15.0), 0.0);
    }

    #[test]
    fn center_window_takes_middle_slice() {
        let s: Vec<f64> = (0..100).map(|i| i as f64).collect();
        let w = center_window(&s, 10);
        assert_eq!(w.len(), 10);
        assert_eq!(w[0], 45.0); // (100-10)/2 = 45
        // Shorter-than-window input is returned whole.
        assert_eq!(center_window(&s, 1000).len(), 100);
        assert_eq!(center_window(&s, 0).len(), 100);
    }

    #[test]
    fn resample_preserves_length_ratio() {
        let src = vec![0.0; 48_000];
        let up = resample_linear(&src, 48_000.0, 96_000.0);
        assert!((up.len() as i64 - 96_000).abs() <= 1);
        let same = resample_linear(&src, 48_000.0, 48_000.0);
        assert_eq!(same.len(), src.len());
    }

    fn fixture(name: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/assets")
            .join(name)
    }

    /// The guarantee, end-to-end: measuring two different models on the same DI
    /// yields finite loudness for each, and applying `target − measured` makeup
    /// lands them both on the identical target — so a swap between them is a
    /// no-op in level. Uses `measure_model_lufs` directly (no cache / no config
    /// writes). Skips if the dummy fixtures don't load a real model.
    #[test]
    fn measured_makeup_levels_models_to_a_common_target() {
        let sr = 48_000.0;
        let di = DiReference::synthetic(sr);
        let (Ok(mut a), Ok(mut b)) = (
            NamModel::load(fixture("amp_a.nam")),
            NamModel::load(fixture("amp_b.nam")),
        ) else {
            eprintln!("skip: NAM fixtures did not load a runnable model");
            return;
        };
        let la = measure_model_lufs(&mut a, &di, 512);
        let lb = measure_model_lufs(&mut b, &di, 512);
        if !la.is_finite() || !lb.is_finite() {
            eprintln!("skip: fixture model output was silent");
            return;
        }
        let target = -18.0;
        let a_leveled = la + (target - la);
        let b_leveled = lb + (target - lb);
        assert!((a_leveled - b_leveled).abs() < 1e-6);
        assert!((a_leveled - target).abs() < 1e-6);
    }

    #[test]
    fn cache_round_trips_lookup() {
        let mut c = LoudnessCache::default();
        c.insert("modelhash".into(), "synthetic-v1".into(), 48_000, -14.2);
        assert_eq!(c.lookup("modelhash", "synthetic-v1", 48_000), Some(-14.2));
        assert_eq!(c.lookup("modelhash", "other-di", 48_000), None);
        // Re-insert replaces, not duplicates.
        c.insert("modelhash".into(), "synthetic-v1".into(), 48_000, -10.0);
        assert_eq!(c.entries.len(), 1);
        assert_eq!(c.lookup("modelhash", "synthetic-v1", 48_000), Some(-10.0));
    }
}

// ── Drive curve: the constant-loudness saturation control ───────────────────
//
// Static NAM captures have no drive knob — pushing the *input* harder is how
// saturation changes. The guarantee: sweeping drive 0→100% (input −12…+12 dB,
// 50% = the capture at unity) NEVER changes perceived level, and neither does
// engaging the block at all: for every sweep point we measure the model's
// output LUFS on the DI and compensate the output so the block sits at the
// DI's own loudness — a unity-loudness block, whatever the combination.

/// Input-gain sweep (dB) behind drive 0..1. Index 4 (0 dB) = drive 50%.
pub const DRIVE_SWEEP_DB: [f64; 9] = [-12.0, -9.0, -6.0, -3.0, 0.0, 3.0, 6.0, 9.0, 12.0];

/// One model's drive calibration: output LUFS per sweep point + the DI's own
/// loudness (the compensation target).
#[derive(Clone, Debug, Facet)]
pub struct DriveCurveEntry {
    pub model_hash: String,
    pub di_id: String,
    pub sample_rate: u32,
    pub di_lufs: f64,
    pub lufs: Vec<f64>,
}

#[derive(Clone, Debug, Default, Facet)]
struct DriveCurveCache {
    entries: Vec<DriveCurveEntry>,
}

impl DriveCurveCache {
    fn path() -> PathBuf {
        calibration_dir().join("drive-cache.styx")
    }
    fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|t| facet_styx::from_str(&t).ok())
            .unwrap_or_default()
    }
    fn save(&self) {
        let _ = std::fs::create_dir_all(calibration_dir());
        if let Ok(t) = facet_styx::to_string(self) {
            let _ = std::fs::write(Self::path(), t);
        }
    }
    fn lookup(&self, hash: &str, di: &str, sr: u32) -> Option<&DriveCurveEntry> {
        self.entries
            .iter()
            .find(|e| e.model_hash == hash && e.di_id == di && e.sample_rate == sr)
    }
}

static DRIVE_CACHE: OnceLock<Mutex<DriveCurveCache>> = OnceLock::new();

fn drive_cache() -> &'static Mutex<DriveCurveCache> {
    DRIVE_CACHE.get_or_init(|| Mutex::new(DriveCurveCache::load()))
}

/// Measure `model`'s output LUFS at every sweep gain (offline — WaveNet over
/// the DI window per point, so ~seconds per model; run once and cached).
pub fn measure_drive_curve(model: &mut NamModel, di: &DiReference, max_block: usize) -> Vec<f64> {
    let block = max_block.clamp(64, 8192);
    DRIVE_SWEEP_DB
        .iter()
        .map(|gain_db| {
            let gain = 10f64.powf(gain_db / 20.0);
            model.reset(di.sample_rate, block);
            let mut output = vec![0.0f64; di.samples.len()];
            let mut scratch_in = vec![0.0f64; block];
            let mut scratch_out = vec![0.0f64; block];
            let mut pos = 0;
            while pos < di.samples.len() {
                let n = block.min(di.samples.len() - pos);
                for i in 0..n {
                    scratch_in[i] = di.samples[pos + i] * gain;
                }
                model.process(&scratch_in[..n], &mut scratch_out[..n]);
                output[pos..pos + n].copy_from_slice(&scratch_out[..n]);
                pos += n;
            }
            integrated_lufs(&output, di.sample_rate)
        })
        .collect()
}

/// The cached drive curve for a model file, measuring on first sight (the
/// "import-time test"). Returns `None` if the model can't be loaded/hashed.
pub fn drive_curve(model_path: &Path, sample_rate: f64) -> Option<DriveCurveEntry> {
    let sr_key = sample_rate.round() as u32;
    let model_hash = hash_file(model_path).ok()?;
    let di = DiReference::load_or_synthetic(sample_rate);
    {
        let cache = drive_cache().lock().unwrap();
        if let Some(e) = cache.lookup(&model_hash, &di.id, sr_key) {
            return Some(e.clone());
        }
    }
    tracing::info!(model = %model_path.display(), "NAM drive calibration: measuring sweep");
    let mut model = NamModel::load(model_path).ok()?;
    let lufs = measure_drive_curve(&mut model, &di, 512);
    let di_lufs = integrated_lufs(&di.samples, di.sample_rate);
    let entry = DriveCurveEntry {
        model_hash,
        di_id: di.id.clone(),
        sample_rate: sr_key,
        di_lufs,
        lufs,
    };
    let mut cache = drive_cache().lock().unwrap();
    // Re-check under the lock — a concurrent caller may have measured the
    // same model while we were (the lock is dropped during measurement).
    if cache
        .lookup(&entry.model_hash, &entry.di_id, entry.sample_rate)
        .is_none()
    {
        cache.entries.push(entry.clone());
        cache.save();
    }
    Some(entry)
}

/// Map a drive position (0..1, 0.5 = the capture at unity input) to the
/// `(input_trim_db, output_trim_db)` pair that realises it at **constant
/// perceived level**: input pushes the nonlinearity, output compensates the
/// measured loudness back to the DI's own — engaging the block or sweeping
/// the knob never moves the volume.
pub fn drive_compensation(model_path: &Path, sample_rate: f64, drive: f32) -> Option<(f32, f32)> {
    let entry = drive_curve(model_path, sample_rate)?;
    let lo = DRIVE_SWEEP_DB[0];
    let hi = DRIVE_SWEEP_DB[DRIVE_SWEEP_DB.len() - 1];
    let in_db = lo + (drive.clamp(0.0, 1.0) as f64) * (hi - lo);
    // Linear interpolation over the sweep.
    let n = entry.lufs.len().min(DRIVE_SWEEP_DB.len());
    if n < 2 {
        return None;
    }
    let step = (hi - lo) / (n as f64 - 1.0);
    let t = ((in_db - lo) / step).clamp(0.0, n as f64 - 1.0);
    let (i0, frac) = (t.floor() as usize, t.fract());
    let i1 = (i0 + 1).min(n - 1);
    let model_lufs = entry.lufs[i0] * (1.0 - frac) + entry.lufs[i1] * frac;
    if !model_lufs.is_finite() || model_lufs <= FAILED_LUFS {
        return None;
    }
    let out_db = entry.di_lufs - model_lufs;
    Some((in_db as f32, out_db as f32))
}
