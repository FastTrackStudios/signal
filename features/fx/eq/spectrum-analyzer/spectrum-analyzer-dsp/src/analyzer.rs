//! The analyzer orchestrator.
//!
//! Threading model:
//! - The audio thread only ever touches [`AudioFeed`] (`push_*`), which writes
//!   into lock-free rings. No FFT, no allocation, no locks there.
//! - The UI thread calls [`Analyzer::tick`] (driven by the existing 60 Hz redraw
//!   loop). `tick` drains the rings, runs the FFT pipeline per active slot,
//!   computes collisions, publishes to / reads from the sharing registry, and
//!   writes an [`AnalyzerSnapshot`] the painter reads.
//!
//! Resolution changes rebuild the FFT pipeline on the UI thread; the audio rings
//! are sized once to the maximum FFT size so the audio thread never reallocates.

use parking_lot::RwLock;

use crate::accumulator::SpectrumAccumulator;
use crate::collision::SpectrumCollision;
use crate::decayer::SpectrumDecayer;
use crate::fft::RealFft;
use crate::ring::{RingConsumer, RingProducer, ring};
use crate::settings::{AnalyzerSettings, Resolution};
use crate::sharing::{self, InstanceId, SharedSpectrum};
use crate::smoother::SpectrumSmoother;
use crate::tilter::SpectrumTilter;
use crate::window::{WindowKind, build_scaled};

use std::sync::Arc;

const FLOOR_DB: f32 = -200.0;
/// Rings hold a few max-size frames so a slow UI tick never loses recent audio.
const RING_FRAMES: usize = 4;

/// Power (already window-normalized) to dB.
#[inline]
fn power_to_db(p: f32) -> f32 {
    10.0 * p.max(1e-20).log10()
}

/// The latest analyzed spectra, read by the UI painter.
#[derive(Default, Clone, PartialEq)]
pub struct AnalyzerSnapshot {
    /// Frequency (Hz) of each bin in `pre_db`/`post_db`.
    pub freq_hz: Vec<f32>,
    pub pre_db: Vec<f32>,
    pub post_db: Vec<f32>,
    /// External/sidechain spectrum (may use its own axis).
    pub ext_db: Vec<f32>,
    pub ext_freq_hz: Vec<f32>,
    /// Per-bin collision strength (0..0.9), aligned to `freq_hz`.
    pub collision: Vec<f32>,
    /// Display range in dB (for the painter's vertical scale).
    pub range_db: f32,
}

/// Audio-thread handle. Move this into the plugin's processor.
pub struct AudioFeed {
    pre: RingProducer,
    post: RingProducer,
    sidechain: RingProducer,
}

impl AudioFeed {
    /// Push pre-EQ mono samples (caller applies any stereo reduction).
    #[inline]
    pub fn push_pre(&mut self, samples: &[f32]) {
        self.pre.push(samples);
    }
    /// Push post-EQ mono samples.
    #[inline]
    pub fn push_post(&mut self, samples: &[f32]) {
        self.post.push(samples);
    }
    /// Push sidechain mono samples.
    #[inline]
    pub fn push_sidechain(&mut self, samples: &[f32]) {
        self.sidechain.push(samples);
    }
}

/// Per-slot FFT processing pipeline (one each for pre and post).
struct SlotProcessor {
    consumer: RingConsumer,
    staging: Vec<f32>,
    fft: RealFft,
    window: Vec<f32>,
    fft_input: Vec<f32>,
    sqr_mag: Vec<f32>,
    mean: Vec<f32>,
    accumulator: SpectrumAccumulator,
    smoother: SpectrumSmoother,
    tilter: SpectrumTilter,
    decayer: SpectrumDecayer,
    /// Latest tilted/decayed dB spectrum (UI reads a copy via the snapshot).
    db: Vec<f32>,
}

impl SlotProcessor {
    fn new(consumer: RingConsumer, fft_size: usize) -> Self {
        let fft = RealFft::new(fft_size);
        let bins = fft.num_bins();
        let mut window = vec![0.0; fft_size];
        build_scaled(WindowKind::Hann, &mut window);
        Self {
            consumer,
            staging: Vec::with_capacity(fft_size * 2),
            fft,
            window,
            fft_input: vec![0.0; fft_size],
            sqr_mag: vec![0.0; bins],
            mean: vec![FLOOR_DB; bins],
            accumulator: SpectrumAccumulator::new(bins),
            smoother: SpectrumSmoother::new(bins),
            tilter: SpectrumTilter::new(bins),
            decayer: SpectrumDecayer::new(bins),
            db: vec![FLOOR_DB; bins],
        }
    }

    fn num_bins(&self) -> usize {
        self.fft.num_bins()
    }

    /// Rebuild for a new FFT size (resolution change). Keeps the consumer.
    fn rebuild(&mut self, fft_size: usize) {
        if fft_size == self.fft.size() {
            return;
        }
        self.fft = RealFft::new(fft_size);
        let bins = self.fft.num_bins();
        self.window.resize(fft_size, 0.0);
        build_scaled(WindowKind::Hann, &mut self.window);
        self.fft_input.resize(fft_size, 0.0);
        self.sqr_mag.resize(bins, 0.0);
        self.mean.resize(bins, FLOOR_DB);
        self.accumulator.resize(bins);
        self.smoother.resize(bins);
        self.tilter.resize(bins);
        self.decayer.resize(bins);
        self.db.resize(bins, FLOOR_DB);
        self.staging.clear();
    }

    /// Drain audio and produce the updated dB spectrum. `dsp_params` carry the
    /// per-tick coefficients already configured by the caller.
    fn process_tick(&mut self, freeze: bool) {
        self.consumer.drain(&mut self.staging);

        let fft_size = self.fft.size();
        let hop = fft_size / 2;
        let bins = self.num_bins();

        let mut frames = 0usize;
        self.accumulator.reset();
        while self.staging.len() >= fft_size {
            // Window the oldest frame into the FFT input.
            for ((dst, &src), &w) in self
                .fft_input
                .iter_mut()
                .zip(self.staging.iter())
                .zip(self.window.iter())
            {
                *dst = src * w;
            }
            self.fft
                .forward_sqr_mag(&mut self.fft_input, &mut self.sqr_mag);
            self.accumulator.process(&mut self.sqr_mag);
            self.mean.copy_from_slice(&self.sqr_mag);
            frames += 1;
            self.staging.drain(0..hop);
        }

        // Build the fresh measured dB spectrum for this tick.
        let mut fresh = std::mem::take(&mut self.db); // reuse the allocation
        fresh.resize(bins, FLOOR_DB);
        if frames > 0 {
            // Smooth the averaged power, then convert to dB and tilt.
            self.smoother.smooth(&mut self.mean);
            for (d, &p) in fresh.iter_mut().zip(self.mean.iter()) {
                *d = power_to_db(p);
            }
            self.tilter.tilt(&mut fresh);
        } else {
            fresh.iter_mut().for_each(|d| *d = FLOOR_DB);
        }

        // Attack/release (or freeze hold).
        self.decayer.decay(&mut fresh, freeze);
        self.db = fresh;
    }
}

/// The shared analyzer. Wrap in an `Arc` and share with the UI.
pub struct Analyzer {
    id: InstanceId,
    shared: Arc<SharedSpectrum>,
    state: parking_lot::Mutex<AnalyzerState>,
    snapshot: RwLock<AnalyzerSnapshot>,
    settings: RwLock<AnalyzerSettings>,
    label: RwLock<String>,
}

struct AnalyzerState {
    sample_rate: f32,
    pre: SlotProcessor,
    post: SlotProcessor,
    sidechain: SlotProcessor,
    collision: SpectrumCollision,
    freq_hz: Vec<f32>,
    /// Instance id whose spectrum to display in the external slot.
    external_source: Option<InstanceId>,
    /// Scratch buffers for reading an external spectrum.
    ext_db: Vec<f32>,
    ext_freq: Vec<f32>,
}

impl Analyzer {
    /// Create an analyzer for `instance_id` at `sample_rate`. Returns the shared
    /// analyzer (for the UI) and the audio feed (for the processor).
    pub fn new(instance_id: InstanceId, sample_rate: f32) -> (Arc<Analyzer>, AudioFeed) {
        let cap = Resolution::MAX_FFT_SIZE * RING_FRAMES;
        let (pre_tx, pre_rx) = ring(cap);
        let (post_tx, post_rx) = ring(cap);
        let (sc_tx, sc_rx) = ring(cap);

        let fft_size = AnalyzerSettings::default().resolution.fft_size();
        let pre = SlotProcessor::new(pre_rx, fft_size);
        let post = SlotProcessor::new(post_rx, fft_size);
        let sidechain = SlotProcessor::new(sc_rx, fft_size);
        let bins = pre.num_bins();

        let state = AnalyzerState {
            sample_rate,
            pre,
            post,
            sidechain,
            collision: SpectrumCollision::new(bins),
            freq_hz: bin_freqs(sample_rate, fft_size),
            external_source: None,
            ext_db: Vec::new(),
            ext_freq: Vec::new(),
        };

        let analyzer = Arc::new(Analyzer {
            id: instance_id,
            shared: sharing::register(instance_id),
            state: parking_lot::Mutex::new(state),
            snapshot: RwLock::new(AnalyzerSnapshot::default()),
            settings: RwLock::new(AnalyzerSettings::default()),
            label: RwLock::new(String::new()),
        });

        let feed = AudioFeed {
            pre: pre_tx,
            post: post_tx,
            sidechain: sc_tx,
        };
        (analyzer, feed)
    }

    /// Replace the analyzer settings (UI thread).
    pub fn set_settings(&self, settings: AnalyzerSettings) {
        *self.settings.write() = settings;
    }

    /// Current settings.
    pub fn settings(&self) -> AnalyzerSettings {
        *self.settings.read()
    }

    /// Set the display label used in other instances' source pickers.
    pub fn set_label(&self, label: &str) {
        *self.label.write() = label.to_string();
    }

    /// Choose which other instance to show in the external slot (`None` = off).
    pub fn set_external_source(&self, source: Option<InstanceId>) {
        self.state.lock().external_source = source;
    }

    /// This instance's id.
    pub fn id(&self) -> InstanceId {
        self.id
    }

    /// Update the sample rate (e.g. on `prepare`). Rebuilds the frequency axis.
    pub fn set_sample_rate(&self, sample_rate: f32) {
        let mut st = self.state.lock();
        st.sample_rate = sample_rate;
        let fft_size = st.pre.fft.size();
        st.freq_hz = bin_freqs(sample_rate, fft_size);
    }

    /// Read a clone of the latest snapshot (UI painter).
    pub fn snapshot(&self) -> AnalyzerSnapshot {
        self.snapshot.read().clone()
    }

    /// Run one analysis tick. `refresh_hz` is the UI redraw rate.
    pub fn tick(&self, refresh_hz: f32) {
        let settings = *self.settings.read();
        let mut guard = self.state.lock();
        // Reborrow the guard once to a plain `&mut` so disjoint field borrows
        // (e.g. `&mut st.pre` and `&mut st.post` together) are allowed — going
        // through the guard's `DerefMut` each time would borrow the whole guard.
        let st = &mut *guard;

        // Apply resolution / sample-rate / coefficient changes.
        let fft_size = settings.resolution.fft_size();
        let rebuilt = st.pre.fft.size() != fft_size;
        if rebuilt {
            st.pre.rebuild(fft_size);
            st.post.rebuild(fft_size);
            st.sidechain.rebuild(fft_size);
            let bins = st.pre.num_bins();
            st.collision.resize(bins);
            let sr = st.sample_rate;
            st.freq_hz = bin_freqs(sr, fft_size);
        }

        let sr = st.sample_rate as f64;
        let release_s = settings.speed.release_seconds();
        let attack_s = crate::settings::ATTACK_SECONDS;
        let tilt = settings.tilt_db_per_oct as f64;
        let smooth = settings.smoothing_oct as f64;
        for slot in [&mut st.pre, &mut st.post, &mut st.sidechain] {
            slot.tilter.set_slope(sr, tilt);
            slot.smoother.set_smooth(smooth);
            slot.decayer.set_ballistics(refresh_hz, attack_s, release_s);
        }

        // Run the active slots. Pre is also needed (even when hidden) whenever
        // collisions are on, since they compare pre vs post.
        if settings.show_pre || settings.show_collisions {
            st.pre.process_tick(settings.freeze);
        }
        st.post.process_tick(settings.freeze); // always run post (published + collisions)
        if settings.show_external {
            st.sidechain.process_tick(settings.freeze);
        }

        // Collisions over pre vs post.
        if settings.show_collisions {
            let pre_db = st.pre.db.clone();
            let post_db = st.post.db.clone();
            st.collision.update(&pre_db, &post_db, 0.1);
        }

        // Publish our post spectrum for other instances.
        {
            let label = self.label.read().clone();
            let post_db = &st.post.db;
            let freq = &st.freq_hz;
            self.shared.publish(&label, post_db, freq);
        }

        // Pull an external instance's spectrum if requested.
        let (ext_db, ext_freq) = if settings.show_external {
            if let Some(src) = st.external_source {
                if let Some(slot) = sharing::get(src) {
                    let mut db = std::mem::take(&mut st.ext_db);
                    let mut fz = std::mem::take(&mut st.ext_freq);
                    slot.read_into(&mut db, &mut fz);
                    st.ext_db = db.clone();
                    st.ext_freq = fz.clone();
                    (db, fz)
                } else {
                    (st.sidechain.db.clone(), st.freq_hz.clone())
                }
            } else {
                // No cross-instance source → show the sidechain input.
                (st.sidechain.db.clone(), st.freq_hz.clone())
            }
        } else {
            (Vec::new(), Vec::new())
        };

        // Assemble the snapshot.
        let snap = AnalyzerSnapshot {
            freq_hz: st.freq_hz.clone(),
            pre_db: if settings.show_pre {
                st.pre.db.clone()
            } else {
                Vec::new()
            },
            post_db: if settings.show_post {
                st.post.db.clone()
            } else {
                Vec::new()
            },
            ext_db,
            ext_freq_hz: ext_freq,
            collision: if settings.show_collisions {
                st.collision.strengths().to_vec()
            } else {
                Vec::new()
            },
            range_db: settings.range.db(),
        };
        drop(guard);
        *self.snapshot.write() = snap;
    }
}

impl Drop for Analyzer {
    fn drop(&mut self) {
        sharing::unregister(self.id);
    }
}

/// Frequency (Hz) of each half-spectrum bin.
fn bin_freqs(sample_rate: f32, fft_size: usize) -> Vec<f32> {
    let bins = fft_size / 2 + 1;
    let delta = sample_rate / fft_size as f32;
    (0..bins).map(|i| i as f32 * delta).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sine_shows_peak_at_its_bin() {
        let sr = 48_000.0f32;
        let (analyzer, mut feed) = Analyzer::new(1001, sr);
        // 1 kHz sine.
        let f = 1000.0f32;
        let n = 8192 * 4;
        let buf: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * f * i as f32 / sr).sin())
            .collect();
        feed.push_pre(&buf);
        feed.push_post(&buf);
        analyzer.tick(60.0);
        let snap = analyzer.snapshot();
        // Find the loudest post bin and confirm its frequency is near 1 kHz.
        let (idx, _) = snap
            .post_db
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap();
        let peak_hz = snap.freq_hz[idx];
        assert!((peak_hz - 1000.0).abs() < 50.0, "peak at {peak_hz} Hz");
        sharing::unregister(1001);
    }
}
