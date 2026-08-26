//! Original hardware-style EQ models for the first FTS-EQ profile set.
//!
//! These models use FTS' existing filter designer and profile-specific
//! cascades. They do not port reference implementations; calibration targets
//! can refine the constants without changing the public settings shape.

use crate::biquad::Coeffs;
use crate::calibration::{
    fit_response, CalibratedScalar, CalibrationParameters, FitOptions, FitReport, ResponseTarget,
};
use crate::design::{self, FilterType};
use crate::neve_1073::apply_gain_compensated_arctan;
use crate::response::compute_magnitude_response;
use crate::section::Tdf2Section;

#[derive(Debug, Clone, PartialEq)]
pub struct HardwareEqCalibration {
    pub pultec_low_overshoot_factor: f64,
    pub pultec_low_overshoot_step: f64,
    pub pultec_low_atten_scale: f64,
    pub pultec_low_atten_step: f64,
    pub pultec_high_air_factor: f64,
    pub pultec_high_air_step: f64,
    pub pultec_high_atten_scale: f64,
    pub pultec_high_atten_step: f64,
    pub api_low_overshoot_factor: f64,
    pub api_low_overshoot_step: f64,
    pub api_mid_skirt_factor: f64,
    pub api_mid_skirt_step: f64,
    pub api_high_presence_factor: f64,
    pub api_high_presence_step: f64,
    pub ssl_e_skirt_factor: f64,
    pub ssl_e_skirt_step: f64,
    pub ssl_g_skirt_factor: f64,
    pub ssl_g_skirt_step: f64,
}

impl Default for HardwareEqCalibration {
    fn default() -> Self {
        Self {
            pultec_low_overshoot_factor: -0.16,
            pultec_low_overshoot_step: 0.025,
            pultec_low_atten_scale: -0.78,
            pultec_low_atten_step: 0.035,
            pultec_high_air_factor: 0.18,
            pultec_high_air_step: 0.025,
            pultec_high_atten_scale: -0.85,
            pultec_high_atten_step: 0.035,
            api_low_overshoot_factor: -0.08,
            api_low_overshoot_step: 0.02,
            api_mid_skirt_factor: -0.05,
            api_mid_skirt_step: 0.015,
            api_high_presence_factor: 0.07,
            api_high_presence_step: 0.015,
            ssl_e_skirt_factor: 0.055,
            ssl_e_skirt_step: 0.01,
            ssl_g_skirt_factor: 0.035,
            ssl_g_skirt_step: 0.01,
        }
    }
}

impl CalibrationParameters for HardwareEqCalibration {
    fn len(&self) -> usize {
        9
    }

    fn scalar(&self, index: usize) -> CalibratedScalar {
        match index {
            0 => CalibratedScalar::new(
                self.pultec_low_overshoot_factor,
                -0.5,
                0.2,
                self.pultec_low_overshoot_step,
            ),
            1 => CalibratedScalar::new(
                self.pultec_low_atten_scale,
                -1.3,
                -0.3,
                self.pultec_low_atten_step,
            ),
            2 => CalibratedScalar::new(
                self.pultec_high_air_factor,
                -0.1,
                0.45,
                self.pultec_high_air_step,
            ),
            3 => CalibratedScalar::new(
                self.pultec_high_atten_scale,
                -1.3,
                -0.3,
                self.pultec_high_atten_step,
            ),
            4 => CalibratedScalar::new(
                self.api_low_overshoot_factor,
                -0.3,
                0.2,
                self.api_low_overshoot_step,
            ),
            5 => CalibratedScalar::new(
                self.api_mid_skirt_factor,
                -0.2,
                0.1,
                self.api_mid_skirt_step,
            ),
            6 => CalibratedScalar::new(
                self.api_high_presence_factor,
                -0.1,
                0.25,
                self.api_high_presence_step,
            ),
            7 => CalibratedScalar::new(self.ssl_e_skirt_factor, 0.0, 0.18, self.ssl_e_skirt_step),
            // Out-of-range indices clamp to the last scalar — a panic

            // here would abort the audio host on a bad automation index.
            _ => CalibratedScalar::new(self.ssl_g_skirt_factor, 0.0, 0.14, self.ssl_g_skirt_step),
        }
    }

    fn set_scalar(&mut self, index: usize, value: f64) {
        match index {
            0 => self.pultec_low_overshoot_factor = value,
            1 => self.pultec_low_atten_scale = value,
            2 => self.pultec_high_air_factor = value,
            3 => self.pultec_high_atten_scale = value,
            4 => self.api_low_overshoot_factor = value,
            5 => self.api_mid_skirt_factor = value,
            6 => self.api_high_presence_factor = value,
            7 => self.ssl_e_skirt_factor = value,
            8 => self.ssl_g_skirt_factor = value,
            _ => {} // ignore out-of-range calibration indices
        }
    }

    fn set_step(&mut self, index: usize, step: f64) {
        match index {
            0 => self.pultec_low_overshoot_step = step,
            1 => self.pultec_low_atten_step = step,
            2 => self.pultec_high_air_step = step,
            3 => self.pultec_high_atten_step = step,
            4 => self.api_low_overshoot_step = step,
            5 => self.api_mid_skirt_step = step,
            6 => self.api_high_presence_step = step,
            7 => self.ssl_e_skirt_step = step,
            8 => self.ssl_g_skirt_step = step,
            _ => {} // ignore out-of-range calibration indices
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PultecEqp1aSettings {
    pub eq_in: bool,
    pub low_freq_hz: f64,
    pub low_boost_db: f64,
    pub low_atten_db: f64,
    pub high_boost_freq_hz: f64,
    pub high_boost_db: f64,
    pub high_bandwidth: f64,
    pub high_atten_freq_hz: f64,
    pub high_atten_db: f64,
    pub drive_percent: f64,
    pub trim_db: f64,
}

impl Default for PultecEqp1aSettings {
    fn default() -> Self {
        Self {
            eq_in: true,
            low_freq_hz: 60.0,
            low_boost_db: 0.0,
            low_atten_db: 0.0,
            high_boost_freq_hz: 5000.0,
            high_boost_db: 0.0,
            high_bandwidth: 5.0,
            high_atten_freq_hz: 10000.0,
            high_atten_db: 0.0,
            drive_percent: 0.0,
            trim_db: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Api550aSettings {
    pub eq_in: bool,
    pub low_freq_hz: f64,
    pub low_gain_db: f64,
    pub mid_freq_hz: f64,
    pub mid_gain_db: f64,
    pub high_freq_hz: f64,
    pub high_gain_db: f64,
    pub drive_percent: f64,
    pub trim_db: f64,
}

impl Default for Api550aSettings {
    fn default() -> Self {
        Self {
            eq_in: true,
            low_freq_hz: 100.0,
            low_gain_db: 0.0,
            mid_freq_hz: 1500.0,
            mid_gain_db: 0.0,
            high_freq_hz: 10000.0,
            high_gain_db: 0.0,
            drive_percent: 0.0,
            trim_db: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SslChannelSettings {
    pub eq_in: bool,
    pub e_series: bool,
    pub hpf_hz: f64,
    pub lpf_hz: f64,
    pub lf_freq_hz: f64,
    pub lf_gain_db: f64,
    pub lmf_freq_hz: f64,
    pub lmf_gain_db: f64,
    pub hmf_freq_hz: f64,
    pub hmf_gain_db: f64,
    pub hf_freq_hz: f64,
    pub hf_gain_db: f64,
    pub drive_percent: f64,
    pub trim_db: f64,
}

impl SslChannelSettings {
    pub fn e_series() -> Self {
        Self {
            eq_in: true,
            e_series: true,
            hpf_hz: 20.0,
            lpf_hz: 22000.0,
            lf_freq_hz: 100.0,
            lf_gain_db: 0.0,
            lmf_freq_hz: 800.0,
            lmf_gain_db: 0.0,
            hmf_freq_hz: 3000.0,
            hmf_gain_db: 0.0,
            hf_freq_hz: 8000.0,
            hf_gain_db: 0.0,
            drive_percent: 0.0,
            trim_db: 0.0,
        }
    }

    pub fn g_series() -> Self {
        Self {
            e_series: false,
            ..Self::e_series()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HardwareEqSettings {
    Pultec(PultecEqp1aSettings),
    Api(Api550aSettings),
    Ssl(SslChannelSettings),
}

pub struct HardwareEqModel {
    settings: HardwareEqSettings,
    sample_rate: f64,
    sections: Vec<Tdf2Section>,
    coeffs: Vec<Coeffs>,
    drive_percent: f64,
    trim_db: f64,
}

impl HardwareEqModel {
    pub fn new(sample_rate: f64, settings: HardwareEqSettings) -> Self {
        let mut model = Self {
            settings,
            sample_rate,
            sections: Vec::new(),
            coeffs: Vec::new(),
            drive_percent: 0.0,
            trim_db: 0.0,
        };
        model.rebuild();
        model
    }

    pub fn settings(&self) -> HardwareEqSettings {
        self.settings
    }

    pub fn set_settings(&mut self, settings: HardwareEqSettings) {
        if self.settings != settings {
            self.settings = settings;
            self.rebuild();
        }
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
        let mut out = apply_gain_compensated_arctan(sample, self.drive_percent, self.trim_db);
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
        let (coeffs, drive_percent, trim_db) = match self.settings {
            HardwareEqSettings::Pultec(settings) => (
                build_pultec_eqp1a_sections(settings, self.sample_rate),
                settings.drive_percent,
                settings.trim_db,
            ),
            HardwareEqSettings::Api(settings) => (
                build_api_550a_sections(settings, self.sample_rate),
                settings.drive_percent,
                settings.trim_db,
            ),
            HardwareEqSettings::Ssl(settings) => (
                build_ssl_channel_sections(settings, self.sample_rate),
                settings.drive_percent,
                settings.trim_db,
            ),
        };

        self.coeffs = coeffs;
        self.drive_percent = drive_percent;
        self.trim_db = trim_db;
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

pub fn build_pultec_eqp1a_sections(settings: PultecEqp1aSettings, sample_rate: f64) -> Vec<Coeffs> {
    build_pultec_eqp1a_sections_with_calibration(
        settings,
        sample_rate,
        &HardwareEqCalibration::default(),
    )
}

pub fn build_pultec_eqp1a_sections_with_calibration(
    settings: PultecEqp1aSettings,
    sample_rate: f64,
    calibration: &HardwareEqCalibration,
) -> Vec<Coeffs> {
    let mut sections = Vec::new();
    push(
        &mut sections,
        FilterType::Highpass,
        14.0,
        0.58,
        0.0,
        sample_rate,
        2,
    );
    push(
        &mut sections,
        FilterType::Lowpass,
        28000.0,
        0.62,
        0.0,
        sample_rate,
        2,
    );
    push(
        &mut sections,
        FilterType::Peak,
        900.0,
        0.45,
        -0.18,
        sample_rate,
        2,
    );

    if !settings.eq_in {
        return sections;
    }

    let low_freq = settings.low_freq_hz.clamp(20.0, 200.0);
    let low_boost = settings.low_boost_db.clamp(0.0, 13.0);
    let low_atten = settings.low_atten_db.clamp(0.0, 13.0);
    if low_boost > 1.0e-9 {
        push(
            &mut sections,
            FilterType::LowShelf,
            low_freq,
            0.52,
            low_boost,
            sample_rate,
            2,
        );
        push(
            &mut sections,
            FilterType::Peak,
            low_freq * 2.2,
            0.62,
            calibration.pultec_low_overshoot_factor * low_boost,
            sample_rate,
            2,
        );
    }
    if low_atten > 1.0e-9 {
        push(
            &mut sections,
            FilterType::LowShelf,
            low_freq * 1.55,
            0.82,
            calibration.pultec_low_atten_scale * low_atten,
            sample_rate,
            2,
        );
        if low_boost > 1.0e-9 {
            push(
                &mut sections,
                FilterType::Peak,
                low_freq * 5.0,
                0.55,
                -0.10 * low_atten,
                sample_rate,
                2,
            );
        }
    }

    let high_boost = settings.high_boost_db.clamp(0.0, 16.0);
    if high_boost > 1.0e-9 {
        let bandwidth = settings.high_bandwidth.clamp(0.0, 10.0);
        let q = 0.35 + bandwidth * 0.115;
        push(
            &mut sections,
            FilterType::Peak,
            settings.high_boost_freq_hz.clamp(1000.0, 16000.0),
            q,
            high_boost,
            sample_rate,
            2,
        );
        push(
            &mut sections,
            FilterType::HighShelf,
            settings.high_boost_freq_hz.clamp(1000.0, 16000.0) * 1.2,
            0.58,
            calibration.pultec_high_air_factor * high_boost,
            sample_rate,
            2,
        );
    }

    let high_atten = settings.high_atten_db.clamp(0.0, 16.0);
    if high_atten > 1.0e-9 {
        push(
            &mut sections,
            FilterType::HighShelf,
            settings.high_atten_freq_hz.clamp(4000.0, 20000.0),
            0.55,
            calibration.pultec_high_atten_scale * high_atten,
            sample_rate,
            2,
        );
    }

    sections
}

pub fn fit_pultec_eqp1a_response(
    initial: HardwareEqCalibration,
    settings: PultecEqp1aSettings,
    target: &ResponseTarget,
    options: FitOptions,
) -> FitReport<HardwareEqCalibration> {
    fit_response(initial, target, options, |calibration| {
        build_pultec_eqp1a_sections_with_calibration(settings, target.sample_rate, calibration)
    })
}

pub fn build_api_550a_sections(settings: Api550aSettings, sample_rate: f64) -> Vec<Coeffs> {
    build_api_550a_sections_with_calibration(
        settings,
        sample_rate,
        &HardwareEqCalibration::default(),
    )
}

pub fn build_api_550a_sections_with_calibration(
    settings: Api550aSettings,
    sample_rate: f64,
    calibration: &HardwareEqCalibration,
) -> Vec<Coeffs> {
    let mut sections = Vec::new();
    push(
        &mut sections,
        FilterType::Highpass,
        18.0,
        0.6,
        0.0,
        sample_rate,
        2,
    );
    push(
        &mut sections,
        FilterType::HighShelf,
        18000.0,
        0.65,
        -0.18,
        sample_rate,
        2,
    );

    if !settings.eq_in {
        return sections;
    }

    let low = settings.low_gain_db.clamp(-12.0, 12.0);
    if low.abs() > 1.0e-9 {
        let q = proportional_q(low, 0.58, 1.05);
        push(
            &mut sections,
            FilterType::LowShelf,
            settings.low_freq_hz,
            q,
            low,
            sample_rate,
            2,
        );
        push(
            &mut sections,
            FilterType::Peak,
            settings.low_freq_hz * 2.4,
            0.8,
            calibration.api_low_overshoot_factor * low,
            sample_rate,
            2,
        );
    }

    let mid = settings.mid_gain_db.clamp(-12.0, 12.0);
    if mid.abs() > 1.0e-9 {
        let q = proportional_q(mid, 0.72, 1.85);
        push(
            &mut sections,
            FilterType::Peak,
            settings.mid_freq_hz,
            q,
            mid,
            sample_rate,
            2,
        );
        push(
            &mut sections,
            FilterType::Peak,
            settings.mid_freq_hz * 0.58,
            0.75,
            calibration.api_mid_skirt_factor * mid,
            sample_rate,
            2,
        );
        push(
            &mut sections,
            FilterType::Peak,
            settings.mid_freq_hz * 1.72,
            0.75,
            calibration.api_mid_skirt_factor * mid,
            sample_rate,
            2,
        );
    }

    let high = settings.high_gain_db.clamp(-12.0, 12.0);
    if high.abs() > 1.0e-9 {
        let q = proportional_q(high, 0.55, 1.25);
        push(
            &mut sections,
            FilterType::HighShelf,
            settings.high_freq_hz,
            q,
            high,
            sample_rate,
            2,
        );
        push(
            &mut sections,
            FilterType::Peak,
            settings.high_freq_hz * 0.72,
            0.85,
            calibration.api_high_presence_factor * high,
            sample_rate,
            2,
        );
    }

    sections
}

pub fn fit_api_550a_response(
    initial: HardwareEqCalibration,
    settings: Api550aSettings,
    target: &ResponseTarget,
    options: FitOptions,
) -> FitReport<HardwareEqCalibration> {
    fit_response(initial, target, options, |calibration| {
        build_api_550a_sections_with_calibration(settings, target.sample_rate, calibration)
    })
}

pub fn build_ssl_channel_sections(settings: SslChannelSettings, sample_rate: f64) -> Vec<Coeffs> {
    build_ssl_channel_sections_with_calibration(
        settings,
        sample_rate,
        &HardwareEqCalibration::default(),
    )
}

pub fn build_ssl_channel_sections_with_calibration(
    settings: SslChannelSettings,
    sample_rate: f64,
    calibration: &HardwareEqCalibration,
) -> Vec<Coeffs> {
    let mut sections = Vec::new();
    push(
        &mut sections,
        FilterType::Highpass,
        16.0,
        0.58,
        0.0,
        sample_rate,
        1,
    );
    push(
        &mut sections,
        FilterType::HighShelf,
        22000.0,
        0.62,
        -0.08,
        sample_rate,
        1,
    );

    if !settings.eq_in {
        return sections;
    }

    if settings.e_series {
        push_cut_if_active(
            &mut sections,
            FilterType::Highpass,
            settings.hpf_hz,
            18.0,
            350.0,
            sample_rate,
        );
        push_cut_if_active(
            &mut sections,
            FilterType::Lowpass,
            settings.lpf_hz,
            3000.0,
            21500.0,
            sample_rate,
        );
    }

    let (lf_q, lmf_q, hmf_q, hf_q, skirt) = if settings.e_series {
        (0.72, 1.25, 1.35, 0.74, calibration.ssl_e_skirt_factor)
    } else {
        (0.58, 0.85, 0.95, 0.58, calibration.ssl_g_skirt_factor)
    };

    push_gain(
        &mut sections,
        FilterType::LowShelf,
        settings.lf_freq_hz,
        lf_q,
        settings.lf_gain_db,
        sample_rate,
    );
    push_gain(
        &mut sections,
        FilterType::Peak,
        settings.lmf_freq_hz,
        lmf_q,
        settings.lmf_gain_db,
        sample_rate,
    );
    push_gain(
        &mut sections,
        FilterType::Peak,
        settings.hmf_freq_hz,
        hmf_q,
        settings.hmf_gain_db,
        sample_rate,
    );
    push_gain(
        &mut sections,
        FilterType::HighShelf,
        settings.hf_freq_hz,
        hf_q,
        settings.hf_gain_db,
        sample_rate,
    );

    for (freq, gain) in [
        (settings.lmf_freq_hz * 0.48, -skirt * settings.lmf_gain_db),
        (settings.lmf_freq_hz * 2.1, -skirt * settings.lmf_gain_db),
        (settings.hmf_freq_hz * 0.52, -skirt * settings.hmf_gain_db),
        (settings.hmf_freq_hz * 1.9, -skirt * settings.hmf_gain_db),
    ] {
        push_gain(
            &mut sections,
            FilterType::Peak,
            freq,
            0.7,
            gain,
            sample_rate,
        );
    }

    sections
}

pub fn fit_ssl_channel_response(
    initial: HardwareEqCalibration,
    settings: SslChannelSettings,
    target: &ResponseTarget,
    options: FitOptions,
) -> FitReport<HardwareEqCalibration> {
    fit_response(initial, target, options, |calibration| {
        build_ssl_channel_sections_with_calibration(settings, target.sample_rate, calibration)
    })
}

fn push_gain(
    sections: &mut Vec<Coeffs>,
    filter_type: FilterType,
    freq_hz: f64,
    q: f64,
    gain_db: f64,
    sample_rate: f64,
) {
    let gain = gain_db.clamp(-15.0, 15.0);
    if gain.abs() > 1.0e-9 {
        push(sections, filter_type, freq_hz, q, gain, sample_rate, 2);
    }
}

fn push_cut_if_active(
    sections: &mut Vec<Coeffs>,
    filter_type: FilterType,
    freq_hz: f64,
    off_edge: f64,
    active_edge: f64,
    sample_rate: f64,
) {
    if (filter_type == FilterType::Highpass && freq_hz > off_edge + 1.0)
        || (filter_type == FilterType::Lowpass && freq_hz < active_edge)
    {
        push(sections, filter_type, freq_hz, 0.707, 0.0, sample_rate, 3);
    }
}

fn proportional_q(gain_db: f64, base: f64, max: f64) -> f64 {
    base + (max - base) * (gain_db.abs() / 12.0).clamp(0.0, 1.0)
}

fn push(
    sections: &mut Vec<Coeffs>,
    filter_type: FilterType,
    freq_hz: f64,
    q: f64,
    gain_db: f64,
    sample_rate: f64,
    order: usize,
) {
    let nyquist_safe = sample_rate * 0.46;
    sections.extend(design::design_filter(
        filter_type,
        freq_hz.clamp(8.0, nyquist_safe),
        q.clamp(0.2, 8.0),
        gain_db,
        sample_rate,
        order,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calibration::{ResponsePoint, ResponseTarget};
    use crate::response::compute_magnitude_response;

    #[test]
    fn pultec_boost_and_atten_build_interacting_sections() {
        let settings = PultecEqp1aSettings {
            low_boost_db: 8.0,
            low_atten_db: 6.0,
            high_boost_db: 5.0,
            high_atten_db: 4.0,
            ..Default::default()
        };
        let sections = build_pultec_eqp1a_sections(settings, 48_000.0);
        assert!(sections.len() >= 8);
    }

    #[test]
    fn api_proportional_q_tightens_with_gain() {
        assert!(proportional_q(12.0, 0.72, 1.85) > proportional_q(3.0, 0.72, 1.85));
    }

    #[test]
    fn ssl_e_has_more_sections_than_ssl_g_when_filters_active() {
        let e = SslChannelSettings {
            hpf_hz: 120.0,
            lpf_hz: 12_000.0,
            lf_gain_db: 4.0,
            lmf_gain_db: -3.0,
            hmf_gain_db: 5.0,
            hf_gain_db: 2.0,
            ..SslChannelSettings::e_series()
        };
        let g = SslChannelSettings {
            e_series: false,
            ..e
        };
        assert!(
            build_ssl_channel_sections(e, 48_000.0).len()
                > build_ssl_channel_sections(g, 48_000.0).len()
        );
    }

    #[test]
    fn hardware_calibration_fit_reduces_generated_api_error() {
        let settings = Api550aSettings {
            mid_gain_db: 9.0,
            ..Default::default()
        };
        let target_calibration = HardwareEqCalibration {
            api_mid_skirt_factor: -0.11,
            ..Default::default()
        };
        let freqs = vec![400.0, 800.0, 1500.0, 3000.0, 5000.0];
        let target_sections =
            build_api_550a_sections_with_calibration(settings, 48_000.0, &target_calibration);
        let mags = compute_magnitude_response(&target_sections, &freqs, 48_000.0);
        let target = ResponseTarget::new(
            "generated_api",
            48_000.0,
            freqs
                .iter()
                .zip(mags.iter())
                .map(|(&freq_hz, &magnitude_db)| ResponsePoint::new(freq_hz, magnitude_db))
                .collect(),
        );

        let before = target.evaluate_sections(&build_api_550a_sections_with_calibration(
            settings,
            48_000.0,
            &HardwareEqCalibration::default(),
        ));
        let report = fit_api_550a_response(
            HardwareEqCalibration::default(),
            settings,
            &target,
            FitOptions {
                iterations: 16,
                ..Default::default()
            },
        );

        assert!(report.error.rms_db < before.rms_db);
    }
}
