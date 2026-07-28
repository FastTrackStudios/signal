//! Original Neve 1073-inspired model.
//!
//! This module intentionally does not port the GPL EQ1979 implementation. It
//! uses this crate's existing filter designer to build a stepped hardware-style
//! cascade, plus a gain-compensated arctan input stage.

use crate::biquad::Coeffs;
use crate::calibration::{
    CalibratedScalar, CalibrationParameters, FitOptions, FitReport, ResponseTarget, fit_response,
};
use crate::design::{self, FilterType};
use crate::response::compute_magnitude_response;
use crate::section::Tdf2Section;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Neve1073Hpf {
    Off,
    Hz50,
    Hz80,
    Hz160,
    Hz300,
}

impl Neve1073Hpf {
    pub fn hz(self) -> Option<f64> {
        match self {
            Self::Off => None,
            Self::Hz50 => Some(50.0),
            Self::Hz80 => Some(80.0),
            Self::Hz160 => Some(160.0),
            Self::Hz300 => Some(300.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Neve1073LowFreq {
    Off,
    Hz35,
    Hz60,
    Hz110,
    Hz220,
}

impl Neve1073LowFreq {
    pub fn hz(self) -> Option<f64> {
        match self {
            Self::Off => None,
            Self::Hz35 => Some(35.0),
            Self::Hz60 => Some(60.0),
            Self::Hz110 => Some(110.0),
            Self::Hz220 => Some(220.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Neve1073MidFreq {
    Off,
    Hz360,
    Hz700,
    Hz1600,
    Hz3200,
    Hz4800,
    Hz7200,
}

impl Neve1073MidFreq {
    pub fn hz(self) -> Option<f64> {
        match self {
            Self::Off => None,
            Self::Hz360 => Some(360.0),
            Self::Hz700 => Some(700.0),
            Self::Hz1600 => Some(1600.0),
            Self::Hz3200 => Some(3200.0),
            Self::Hz4800 => Some(4800.0),
            Self::Hz7200 => Some(7200.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Neve1073Settings {
    pub eq_in: bool,
    pub phase_invert: bool,
    pub trim_db: f64,
    pub drive_percent: f64,
    pub hpf: Neve1073Hpf,
    pub low_freq: Neve1073LowFreq,
    pub low_gain_db: f64,
    pub mid_freq: Neve1073MidFreq,
    pub mid_gain_db: f64,
    pub high_gain_db: f64,
}

impl Default for Neve1073Settings {
    fn default() -> Self {
        Self {
            eq_in: true,
            phase_invert: false,
            trim_db: 0.0,
            drive_percent: 0.0,
            hpf: Neve1073Hpf::Off,
            low_freq: Neve1073LowFreq::Off,
            low_gain_db: 0.0,
            mid_freq: Neve1073MidFreq::Off,
            mid_gain_db: 0.0,
            high_gain_db: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Neve1073Calibration {
    pub transformer_low_gain_db: f64,
    pub transformer_low_step: f64,
    pub transformer_low_q: f64,
    pub transformer_low_q_step: f64,
    pub transformer_mid_dip_db: f64,
    pub transformer_mid_dip_step: f64,
    pub transformer_presence_db: f64,
    pub transformer_presence_step: f64,
    pub transformer_high_trim_db: f64,
    pub transformer_high_trim_step: f64,
    pub hpf_bump_gain_db: f64,
    pub hpf_bump_gain_step: f64,
    pub hpf_bump_ratio: f64,
    pub hpf_bump_ratio_step: f64,
    pub low_overshoot_ratio: f64,
    pub low_overshoot_ratio_step: f64,
    pub low_overshoot_gain_factor: f64,
    pub low_overshoot_gain_step: f64,
    pub mid_lower_ratio: f64,
    pub mid_lower_ratio_step: f64,
    pub mid_lower_gain_factor: f64,
    pub mid_lower_gain_step: f64,
    pub mid_upper_ratio: f64,
    pub mid_upper_ratio_step: f64,
    pub mid_upper_gain_factor: f64,
    pub mid_upper_gain_step: f64,
    pub high_presence_gain_factor: f64,
    pub high_presence_gain_step: f64,
}

impl Default for Neve1073Calibration {
    fn default() -> Self {
        Self {
            transformer_low_gain_db: 0.45,
            transformer_low_step: 0.05,
            transformer_low_q: 0.62,
            transformer_low_q_step: 0.03,
            transformer_mid_dip_db: -0.35,
            transformer_mid_dip_step: 0.05,
            transformer_presence_db: 0.28,
            transformer_presence_step: 0.05,
            transformer_high_trim_db: -0.35,
            transformer_high_trim_step: 0.05,
            hpf_bump_gain_db: 0.65,
            hpf_bump_gain_step: 0.08,
            hpf_bump_ratio: 1.22,
            hpf_bump_ratio_step: 0.03,
            low_overshoot_ratio: 2.15,
            low_overshoot_ratio_step: 0.08,
            low_overshoot_gain_factor: -0.12,
            low_overshoot_gain_step: 0.02,
            mid_lower_ratio: 0.52,
            mid_lower_ratio_step: 0.03,
            mid_lower_gain_factor: -0.08,
            mid_lower_gain_step: 0.015,
            mid_upper_ratio: 1.88,
            mid_upper_ratio_step: 0.06,
            mid_upper_gain_factor: -0.06,
            mid_upper_gain_step: 0.015,
            high_presence_gain_factor: 0.10,
            high_presence_gain_step: 0.02,
        }
    }
}

impl CalibrationParameters for Neve1073Calibration {
    fn len(&self) -> usize {
        14
    }

    fn scalar(&self, index: usize) -> CalibratedScalar {
        match index {
            0 => CalibratedScalar::new(
                self.transformer_low_gain_db,
                -2.0,
                2.0,
                self.transformer_low_step,
            ),
            1 => CalibratedScalar::new(
                self.transformer_low_q,
                0.3,
                1.4,
                self.transformer_low_q_step,
            ),
            2 => CalibratedScalar::new(
                self.transformer_mid_dip_db,
                -2.0,
                2.0,
                self.transformer_mid_dip_step,
            ),
            3 => CalibratedScalar::new(
                self.transformer_presence_db,
                -2.0,
                2.0,
                self.transformer_presence_step,
            ),
            4 => CalibratedScalar::new(
                self.transformer_high_trim_db,
                -2.0,
                2.0,
                self.transformer_high_trim_step,
            ),
            5 => CalibratedScalar::new(self.hpf_bump_gain_db, -1.0, 3.0, self.hpf_bump_gain_step),
            6 => CalibratedScalar::new(self.hpf_bump_ratio, 0.8, 1.8, self.hpf_bump_ratio_step),
            7 => CalibratedScalar::new(
                self.low_overshoot_ratio,
                1.1,
                4.0,
                self.low_overshoot_ratio_step,
            ),
            8 => CalibratedScalar::new(
                self.low_overshoot_gain_factor,
                -0.5,
                0.5,
                self.low_overshoot_gain_step,
            ),
            9 => CalibratedScalar::new(self.mid_lower_ratio, 0.25, 0.9, self.mid_lower_ratio_step),
            10 => CalibratedScalar::new(
                self.mid_lower_gain_factor,
                -0.35,
                0.35,
                self.mid_lower_gain_step,
            ),
            11 => CalibratedScalar::new(self.mid_upper_ratio, 1.1, 3.2, self.mid_upper_ratio_step),
            12 => CalibratedScalar::new(
                self.mid_upper_gain_factor,
                -0.35,
                0.35,
                self.mid_upper_gain_step,
            ),
            // Out-of-range indices clamp to the last scalar — a panic

            // here would abort the audio host on a bad automation index.

            _ => CalibratedScalar::new(
                self.high_presence_gain_factor,
                -0.5,
                0.5,
                self.high_presence_gain_step,
            ),
        }
    }

    fn set_scalar(&mut self, index: usize, value: f64) {
        match index {
            0 => self.transformer_low_gain_db = value,
            1 => self.transformer_low_q = value,
            2 => self.transformer_mid_dip_db = value,
            3 => self.transformer_presence_db = value,
            4 => self.transformer_high_trim_db = value,
            5 => self.hpf_bump_gain_db = value,
            6 => self.hpf_bump_ratio = value,
            7 => self.low_overshoot_ratio = value,
            8 => self.low_overshoot_gain_factor = value,
            9 => self.mid_lower_ratio = value,
            10 => self.mid_lower_gain_factor = value,
            11 => self.mid_upper_ratio = value,
            12 => self.mid_upper_gain_factor = value,
            13 => self.high_presence_gain_factor = value,
            _ => {} // ignore out-of-range calibration indices
        }
    }

    fn set_step(&mut self, index: usize, step: f64) {
        match index {
            0 => self.transformer_low_step = step,
            1 => self.transformer_low_q_step = step,
            2 => self.transformer_mid_dip_step = step,
            3 => self.transformer_presence_step = step,
            4 => self.transformer_high_trim_step = step,
            5 => self.hpf_bump_gain_step = step,
            6 => self.hpf_bump_ratio_step = step,
            7 => self.low_overshoot_ratio_step = step,
            8 => self.low_overshoot_gain_step = step,
            9 => self.mid_lower_ratio_step = step,
            10 => self.mid_lower_gain_step = step,
            11 => self.mid_upper_ratio_step = step,
            12 => self.mid_upper_gain_step = step,
            13 => self.high_presence_gain_step = step,
            _ => {} // ignore out-of-range calibration indices
        }
    }
}

pub fn fit_neve_1073_response(
    initial: Neve1073Calibration,
    settings: Neve1073Settings,
    target: &ResponseTarget,
    options: FitOptions,
) -> FitReport<Neve1073Calibration> {
    fit_response(initial, target, options, |calibration| {
        build_neve_1073_sections_with_calibration(settings, target.sample_rate, calibration)
    })
}

pub struct Neve1073Model {
    settings: Neve1073Settings,
    sample_rate: f64,
    sections: Vec<Tdf2Section>,
    coeffs: Vec<Coeffs>,
}

impl Neve1073Model {
    pub fn new(sample_rate: f64, settings: Neve1073Settings) -> Self {
        let mut model = Self {
            settings,
            sample_rate,
            sections: Vec::new(),
            coeffs: Vec::new(),
        };
        model.rebuild();
        model
    }

    pub fn settings(&self) -> Neve1073Settings {
        self.settings
    }

    pub fn set_settings(&mut self, settings: Neve1073Settings) {
        self.settings = settings;
        self.rebuild();
    }

    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.rebuild();
    }

    pub fn coeffs(&self) -> &[Coeffs] {
        &self.coeffs
    }

    pub fn magnitude_response_db(&self, frequencies: &[f64]) -> Vec<f64> {
        compute_magnitude_response(&self.coeffs, frequencies, self.sample_rate)
    }

    pub fn reset(&mut self) {
        for section in &mut self.sections {
            section.reset();
        }
    }

    #[inline]
    pub fn process_sample(&mut self, sample: f64, ch: usize) -> f64 {
        let mut out = apply_gain_compensated_arctan(
            sample,
            self.settings.drive_percent,
            self.settings.trim_db,
        );

        if self.settings.phase_invert {
            out = -out;
        }

        for section in &mut self.sections {
            out = section.tick(out, ch);
        }

        out
    }

    pub fn process(&mut self, left: &mut [f64], right: &mut [f64]) {
        for i in 0..left.len().min(right.len()) {
            left[i] = self.process_sample(left[i], 0);
            right[i] = self.process_sample(right[i], 1);
        }
    }

    fn rebuild(&mut self) {
        self.coeffs = build_neve_1073_sections(self.settings, self.sample_rate);
        self.sections = self
            .coeffs
            .iter()
            .map(|&coeffs| {
                let mut section = Tdf2Section::new();
                section.set_coeffs(coeffs);
                section
            })
            .collect();
    }
}

pub fn apply_gain_compensated_arctan(sample: f64, drive_percent: f64, trim_db: f64) -> f64 {
    let trim = db_to_gain(trim_db.clamp(-24.0, 24.0));
    let drive = drive_percent.clamp(0.0, 100.0) / 100.0;
    if drive <= 1.0e-9 {
        return sample * trim;
    }

    let amount = 1.0 + drive * 24.0;
    let compensation = amount.atan();
    (sample * trim * amount).atan() / compensation
}

pub fn build_neve_1073_sections(settings: Neve1073Settings, sample_rate: f64) -> Vec<Coeffs> {
    build_neve_1073_sections_with_calibration(
        settings,
        sample_rate,
        &Neve1073Calibration::default(),
    )
}

pub fn build_neve_1073_sections_with_calibration(
    settings: Neve1073Settings,
    sample_rate: f64,
    calibration: &Neve1073Calibration,
) -> Vec<Coeffs> {
    let mut sections = Vec::new();

    // Always-on input/output transformer tone approximation. These gentle
    // sections supply the small LF/HF contours and midrange movement expected
    // from the channel strip, without copying another implementation's table.
    push_filter(
        &mut sections,
        FilterType::Highpass,
        18.0,
        0.58,
        0.0,
        sample_rate,
        2,
    );
    push_filter(
        &mut sections,
        FilterType::LowShelf,
        55.0,
        calibration.transformer_low_q,
        calibration.transformer_low_gain_db,
        sample_rate,
        2,
    );
    push_filter(
        &mut sections,
        FilterType::Peak,
        360.0,
        0.9,
        calibration.transformer_mid_dip_db,
        sample_rate,
        2,
    );
    push_filter(
        &mut sections,
        FilterType::Peak,
        3200.0,
        0.75,
        calibration.transformer_presence_db,
        sample_rate,
        2,
    );
    push_filter(
        &mut sections,
        FilterType::HighShelf,
        12000.0,
        0.68,
        calibration.transformer_high_trim_db,
        sample_rate,
        2,
    );

    if !settings.eq_in {
        return sections;
    }

    if let Some(freq) = settings.hpf.hz() {
        push_filter(
            &mut sections,
            FilterType::Highpass,
            freq,
            0.707,
            0.0,
            sample_rate,
            3,
        );
        // A small resonant skirt near the selected HPF point gives the response
        // the analog-style bump visible in measured/reference curves.
        push_filter(
            &mut sections,
            FilterType::Peak,
            freq * calibration.hpf_bump_ratio,
            1.2,
            calibration.hpf_bump_gain_db,
            sample_rate,
            2,
        );
    }

    if let Some(freq) = settings.low_freq.hz() {
        let gain = settings.low_gain_db.clamp(-16.0, 16.0);
        if gain.abs() > 1.0e-9 {
            push_filter(
                &mut sections,
                FilterType::LowShelf,
                freq,
                0.72,
                gain,
                sample_rate,
                2,
            );
            push_filter(
                &mut sections,
                FilterType::Peak,
                freq * calibration.low_overshoot_ratio,
                0.85,
                calibration.low_overshoot_gain_factor * gain,
                sample_rate,
                2,
            );
        }
    }

    if let Some(freq) = settings.mid_freq.hz() {
        let gain = settings.mid_gain_db.clamp(-18.0, 18.0);
        if gain.abs() > 1.0e-9 {
            push_filter(
                &mut sections,
                FilterType::Peak,
                freq,
                1.05,
                gain,
                sample_rate,
                2,
            );
            push_filter(
                &mut sections,
                FilterType::Peak,
                freq * calibration.mid_lower_ratio,
                0.75,
                calibration.mid_lower_gain_factor * gain,
                sample_rate,
                2,
            );
            push_filter(
                &mut sections,
                FilterType::Peak,
                freq * calibration.mid_upper_ratio,
                0.8,
                calibration.mid_upper_gain_factor * gain,
                sample_rate,
                2,
            );
        }
    }

    let high_gain = settings.high_gain_db.clamp(-16.0, 16.0);
    if high_gain.abs() > 1.0e-9 {
        push_filter(
            &mut sections,
            FilterType::HighShelf,
            12000.0,
            0.58,
            high_gain,
            sample_rate,
            2,
        );
        push_filter(
            &mut sections,
            FilterType::Peak,
            8200.0,
            0.72,
            calibration.high_presence_gain_factor * high_gain,
            sample_rate,
            2,
        );
    }

    sections
}

fn push_filter(
    sections: &mut Vec<Coeffs>,
    filter_type: FilterType,
    freq_hz: f64,
    q: f64,
    gain_db: f64,
    sample_rate: f64,
    order: usize,
) {
    sections.extend(design::design_filter(
        filter_type,
        freq_hz,
        q,
        gain_db,
        sample_rate,
        order,
    ));
}

fn db_to_gain(db: f64) -> f64 {
    10.0_f64.powf(db / 20.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_model_has_transformer_tone_sections() {
        let model = Neve1073Model::new(48_000.0, Neve1073Settings::default());
        assert!(model.coeffs().len() >= 5);
    }

    #[test]
    fn eq_bypass_keeps_baseline_but_removes_active_controls() {
        let engaged = Neve1073Settings {
            hpf: Neve1073Hpf::Hz160,
            low_freq: Neve1073LowFreq::Hz110,
            low_gain_db: 8.0,
            mid_freq: Neve1073MidFreq::Hz1600,
            mid_gain_db: 6.0,
            high_gain_db: 10.0,
            ..Default::default()
        };
        let bypassed = Neve1073Settings {
            eq_in: false,
            ..engaged
        };

        let engaged_sections = build_neve_1073_sections(engaged, 48_000.0);
        let bypassed_sections = build_neve_1073_sections(bypassed, 48_000.0);
        assert!(engaged_sections.len() > bypassed_sections.len());
        assert!(bypassed_sections.len() >= 5);
    }

    #[test]
    fn hpf_step_attenuates_below_selected_frequency() {
        let settings = Neve1073Settings {
            hpf: Neve1073Hpf::Hz160,
            ..Default::default()
        };
        let model = Neve1073Model::new(48_000.0, settings);
        let mags = model.magnitude_response_db(&[40.0, 160.0, 1000.0]);

        assert!(mags[0] < mags[1] - 8.0, "40 Hz should be cut: {mags:?}");
        assert!(
            mags[1] < mags[2],
            "160 Hz should remain below 1 kHz: {mags:?}"
        );
    }

    #[test]
    fn stepped_controls_shape_expected_bands() {
        let settings = Neve1073Settings {
            low_freq: Neve1073LowFreq::Hz110,
            low_gain_db: 10.0,
            mid_freq: Neve1073MidFreq::Hz1600,
            mid_gain_db: -8.0,
            high_gain_db: 8.0,
            ..Default::default()
        };
        let model = Neve1073Model::new(48_000.0, settings);
        let mags = model.magnitude_response_db(&[80.0, 1600.0, 12_000.0]);

        assert!(mags[0] > 4.0, "low shelf should boost lows: {mags:?}");
        assert!(mags[1] < -4.0, "mid band should cut mids: {mags:?}");
        assert!(mags[2] > 3.0, "high shelf should boost highs: {mags:?}");
    }

    #[test]
    fn arctan_drive_is_gain_compensated_at_full_scale() {
        let clean = apply_gain_compensated_arctan(1.0, 0.0, 0.0);
        let driven = apply_gain_compensated_arctan(1.0, 100.0, 0.0);
        assert!((clean - 1.0).abs() < 1.0e-12);
        assert!((driven - 1.0).abs() < 1.0e-12);
    }

    #[test]
    fn arctan_drive_compresses_mid_level_signal() {
        let clean = apply_gain_compensated_arctan(0.25, 0.0, 0.0);
        let driven = apply_gain_compensated_arctan(0.25, 100.0, 0.0);
        assert!(driven > clean);
        assert!(driven < 1.0);
    }

    #[test]
    fn trim_is_independent_when_drive_is_off() {
        let out = apply_gain_compensated_arctan(0.5, 0.0, 6.0);
        let expected = 0.5 * 10.0_f64.powf(6.0 / 20.0);
        assert!((out - expected).abs() < 1.0e-12);
    }

    #[test]
    fn calibrated_sections_can_match_generated_target() {
        use crate::calibration::{FitOptions, ResponsePoint, ResponseTarget};

        let settings = Neve1073Settings {
            hpf: Neve1073Hpf::Hz160,
            low_freq: Neve1073LowFreq::Hz110,
            low_gain_db: 8.0,
            mid_freq: Neve1073MidFreq::Hz1600,
            mid_gain_db: -6.0,
            high_gain_db: 7.0,
            ..Default::default()
        };
        let target_calibration = Neve1073Calibration {
            hpf_bump_gain_db: 1.10,
            low_overshoot_gain_factor: -0.18,
            mid_upper_gain_factor: -0.11,
            high_presence_gain_factor: 0.16,
            ..Default::default()
        };
        let freqs = vec![40.0, 80.0, 160.0, 500.0, 1600.0, 3200.0, 8200.0, 12000.0];
        let target_sections =
            build_neve_1073_sections_with_calibration(settings, 48_000.0, &target_calibration);
        let target_mags = compute_magnitude_response(&target_sections, &freqs, 48_000.0);
        let target = ResponseTarget::new(
            "generated-neve",
            48_000.0,
            freqs
                .iter()
                .zip(target_mags.iter())
                .map(|(&freq_hz, &magnitude_db)| ResponsePoint::new(freq_hz, magnitude_db))
                .collect(),
        );

        let initial = Neve1073Calibration {
            hpf_bump_gain_db: 0.30,
            hpf_bump_gain_step: 0.20,
            low_overshoot_gain_factor: -0.04,
            low_overshoot_gain_step: 0.04,
            mid_upper_gain_factor: -0.02,
            mid_upper_gain_step: 0.03,
            high_presence_gain_factor: 0.04,
            high_presence_gain_step: 0.04,
            ..Default::default()
        };
        let before = target.evaluate_sections(&build_neve_1073_sections_with_calibration(
            settings, 48_000.0, &initial,
        ));
        let report = fit_neve_1073_response(
            initial,
            settings,
            &target,
            FitOptions {
                iterations: 32,
                ..Default::default()
            },
        );

        assert!(report.error.rms_db < before.rms_db);
        assert!(report.error.rms_db < 0.35, "fit report: {report:?}");
    }
}
