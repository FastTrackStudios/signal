//! Multi-band onset detection for implicit drum classification.
//!
//! Splits the input into frequency bands using crossover filters,
//! runs independent onset detectors per band, and provides per-band
//! trigger events. This enables:
//!
//! - **Crosstalk rejection**: Only trigger when energy is in the
//!   target drum's frequency range
//! - **Implicit classification**: Kick (sub-150Hz), snare body (150-1kHz),
//!   snare wires/toms (1-5kHz), cymbals (5kHz+)
//! - **Per-band threshold tuning**: Different sensitivity per range
//!
//! Uses 2nd-order Linkwitz-Riley crossover filters for flat summed
//! response at crossover frequencies.

use audiocore_dsp::AudioConfig;
use eq_dsp::runtime::band::Band;
use eq_dsp::FilterType;

use crate::spectral_flux::{FluxMode, SpectralFluxDetector};

/// Number of frequency bands.
pub const NUM_BANDS: usize = 4;

/// Default crossover frequencies.
pub const DEFAULT_CROSSOVERS: [f64; 3] = [150.0, 1000.0, 5000.0];

/// Band labels for display/classification.
pub const BAND_LABELS: [&str; NUM_BANDS] = ["Sub", "Low-Mid", "Hi-Mid", "High"];

/// Per-band onset detection result.
#[derive(Debug, Clone, Copy)]
pub struct BandTrigger {
    /// Which band triggered (0-3).
    pub band: usize,
    /// ODF value at trigger.
    pub odf: f64,
}

/// Multi-band onset detector.
/// The two halves of one crossover point, both tuned to the same
/// frequency: `lo_filters` and `hi_filters`, two parallel `[Band; 3]`
/// arrays walked by the same index, before.
///
/// A `Band` carries two full section cascades, so three of them do not
/// belong in a stack local — hence the `Box` at the use site.
struct Split {
    lo: Band,
    hi: Band,
}

impl Split {
    fn new(freq_hz: f64, sample_rate: f64) -> Self {
        let band = |filter_type: FilterType| {
            let mut b = Band::new();
            b.filter_type = filter_type;
            b.freq_hz = freq_hz;
            b.q = 0.707; // Butterworth
            b.order = 2;
            b.enabled = true;
            b.update(sample_rate);
            b
        };
        Self {
            lo: band(FilterType::Lowpass),
            hi: band(FilterType::Highpass),
        }
    }

    fn set_freq(&mut self, freq_hz: f64, sample_rate: f64) {
        self.lo.freq_hz = freq_hz;
        self.lo.update(sample_rate);
        self.hi.freq_hz = freq_hz;
        self.hi.update(sample_rate);
    }

    fn reset(&mut self) {
        self.lo.reset();
        self.hi.reset();
    }
}

pub struct MultibandDetector {
    // Crossover filters: 3 crossover points = 6 filter bands
    // Band 0: LPF at crossover[0]
    // Band 1: HPF at crossover[0], LPF at crossover[1]
    // Band 2: HPF at crossover[1], LPF at crossover[2]
    // Band 3: HPF at crossover[2]
    /// One split per crossover point, in ascending frequency order.
    splits: [Box<Split>; 3],

    // Per-band onset detectors
    detectors: [SpectralFluxDetector; NUM_BANDS],

    /// Per-band thresholds (delta above adaptive average to trigger).
    pub thresholds: [f64; NUM_BANDS],

    /// Per-band enable flags.
    pub enabled: [bool; NUM_BANDS],

    /// Crossover frequencies.
    pub crossovers: [f64; 3],

    /// ODF mode for all bands.
    pub mode: FluxMode,

    config: AudioConfig,
}

impl MultibandDetector {
    #[must_use] 
    pub fn new(sample_rate: f64) -> Self {
        let config = AudioConfig {
            sample_rate,
            max_buffer_size: 512,
        };

        let splits = DEFAULT_CROSSOVERS
            .map(|freq| Box::new(Split::new(freq, config.sample_rate)));

        // Use smaller FFT for lower latency in multiband mode
        let fft_size = 1024;
        let hop_size = 256;

        let detectors = std::array::from_fn(|_| {
            SpectralFluxDetector::new(FluxMode::SpectralFlux, fft_size, hop_size, sample_rate)
        });

        Self {
            splits,
            detectors,
            thresholds: [0.5; NUM_BANDS],
            enabled: [true; NUM_BANDS],
            crossovers: DEFAULT_CROSSOVERS,
            mode: FluxMode::SpectralFlux,
            config,
        }
    }

    /// Update configuration and rebuild filters.
    pub fn update(&mut self, config: AudioConfig) {
        self.config = config;

        for (split, &freq) in self.splits.iter_mut().zip(&self.crossovers) {
            split.set_freq(freq, config.sample_rate);
        }

        for det in &mut self.detectors {
            det.update(config.sample_rate);
        }
    }

    /// Reset all state.
    pub fn reset(&mut self) {
        for split in &mut self.splits {
            split.reset();
        }
        for d in &mut self.detectors {
            d.reset();
        }
    }

    /// Feed one mono sample. Returns per-band trigger slots (`None` =
    /// no trigger this sample) — a fixed array, so the audio thread
    /// never allocates.
    ///
    /// Each enabled band runs its own onset detector. A trigger fires
    /// when the band's ODF exceeds its adaptive threshold.
    pub fn tick(&mut self, sample: f64) -> [Option<BandTrigger>; NUM_BANDS] {
        let mut triggers = [None; NUM_BANDS];

        // Split into bands using crossover filters
        // Band 0: LPF(crossover[0])
        let band0 = self.splits[0].lo.tick(sample, 0);

        // Band 1: HPF(crossover[0]) → LPF(crossover[1])
        let hp0 = self.splits[0].hi.tick(sample, 0);
        let band1 = self.splits[1].lo.tick(hp0, 0);

        // Band 2: HPF(crossover[1]) → LPF(crossover[2])
        let hp1 = self.splits[1].hi.tick(sample, 0);
        let band2 = self.splits[2].lo.tick(hp1, 0);

        // Band 3: HPF(crossover[2])
        let band3 = self.splits[2].hi.tick(sample, 0);

        let band_signals = [band0, band1, band2, band3];

        for (b, &sig) in band_signals.iter().enumerate() {
            if !self.enabled[b] {
                continue;
            }

            if let Some(odf) = self.detectors[b].tick(sig) {
                if self.detectors[b].is_peak(odf, self.thresholds[b]) {
                    triggers[b] = Some(BandTrigger { band: b, odf });
                }
            }
        }

        triggers
    }

    /// Get the dominant band from a set of simultaneous triggers.
    /// Returns the band with the highest ODF value.
    #[must_use] 
    pub fn dominant_band(triggers: &[BandTrigger]) -> Option<usize> {
        triggers
            .iter()
            .max_by(|a, b| {
                a.odf
                    .partial_cmp(&b.odf)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|t| t.band)
    }

    /// Classify a trigger based on which band fired.
    #[must_use] 
    pub const fn classify(band: usize) -> &'static str {
        match band {
            0 => "kick",
            1 => "snare",
            2 => "tom",
            3 => "cymbal",
            _ => "unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    #[test]
    fn silence_produces_no_triggers() {
        let mut mb = MultibandDetector::new(SR);
        for _ in 0..48000 {
            let triggers = mb.tick(0.0);
            assert!(
                triggers.iter().all(Option::is_none),
                "Silence should produce no triggers"
            );
        }
    }

    #[test]
    fn low_freq_triggers_sub_band() {
        let mut mb = MultibandDetector::new(SR);
        // Only enable sub band
        mb.enabled = [true, false, false, false];
        mb.thresholds = [0.1; NUM_BANDS];

        // Feed enough silence to fill ring buffers
        for _ in 0..16000 {
            mb.tick(0.0);
        }

        // Feed low-frequency content (80Hz kick drum)
        let mut got_trigger = false;
        for i in 0..4096 {
            let t = f64::from(i) / SR;
            let sample = (80.0 * std::f64::consts::TAU * t).sin() * 0.8;
            let triggers = mb.tick(sample);
            if let Some(t) = triggers.iter().flatten().next() {
                assert_eq!(t.band, 0, "80Hz should trigger sub band");
                got_trigger = true;
            }
        }
        assert!(got_trigger, "80Hz signal should trigger sub band");
    }

    #[test]
    fn high_freq_triggers_high_band() {
        let mut mb = MultibandDetector::new(SR);
        // Only enable high band
        mb.enabled = [false, false, false, true];
        mb.thresholds = [0.1; NUM_BANDS];

        // Fill ring buffers
        for _ in 0..16000 {
            mb.tick(0.0);
        }

        // Feed high-frequency content (8kHz cymbal)
        let mut got_trigger = false;
        for i in 0..4096 {
            let t = f64::from(i) / SR;
            let sample = (8000.0 * std::f64::consts::TAU * t).sin() * 0.8;
            let triggers = mb.tick(sample);
            if let Some(t) = triggers.iter().flatten().next() {
                assert_eq!(t.band, 3, "8kHz should trigger high band");
                got_trigger = true;
            }
        }
        assert!(got_trigger, "8kHz signal should trigger high band");
    }

    #[test]
    fn classify_returns_correct_labels() {
        assert_eq!(MultibandDetector::classify(0), "kick");
        assert_eq!(MultibandDetector::classify(1), "snare");
        assert_eq!(MultibandDetector::classify(2), "tom");
        assert_eq!(MultibandDetector::classify(3), "cymbal");
    }
}
