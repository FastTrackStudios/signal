//! Hardware EQ response target snapshots.
//!
//! The points here are intentionally structured as calibration data, not DSP
//! code. They are first-pass anchors from public/reference response plots and
//! expected hardware behavior; future measurement/digitization passes can
//! replace the magnitudes while keeping the settings and evaluation path.

use crate::calibration::{CalibrationError, ResponsePoint, ResponseTarget};
use crate::hardware_eq::{
    build_api_550a_sections, build_pultec_eqp1a_sections, build_ssl_channel_sections,
    Api550aSettings, PultecEqp1aSettings, SslChannelSettings,
};
use crate::neve_1073::{
    build_neve_1073_sections, Neve1073Hpf, Neve1073LowFreq, Neve1073MidFreq, Neve1073Settings,
};

pub const TARGET_SAMPLE_RATE: f64 = 48_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardwareTargetKind {
    PultecEqp1a,
    Neve1073,
    Api550a,
    SslE,
    SslG,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HardwareTargetSettings {
    Pultec(PultecEqp1aSettings),
    Neve1073(Neve1073Settings),
    Api(Api550aSettings),
    Ssl(SslChannelSettings),
}

#[derive(Debug, Clone, PartialEq)]
pub struct HardwareResponseSnapshot {
    pub kind: HardwareTargetKind,
    pub settings: HardwareTargetSettings,
    pub target: ResponseTarget,
}

impl HardwareResponseSnapshot {
    pub fn evaluate_current_model(&self) -> CalibrationError {
        match self.settings {
            HardwareTargetSettings::Pultec(settings) => self.target.evaluate_sections(
                &build_pultec_eqp1a_sections(settings, self.target.sample_rate),
            ),
            HardwareTargetSettings::Neve1073(settings) => self
                .target
                .evaluate_sections(&build_neve_1073_sections(settings, self.target.sample_rate)),
            HardwareTargetSettings::Api(settings) => self
                .target
                .evaluate_sections(&build_api_550a_sections(settings, self.target.sample_rate)),
            HardwareTargetSettings::Ssl(settings) => self.target.evaluate_sections(
                &build_ssl_channel_sections(settings, self.target.sample_rate),
            ),
        }
    }
}

pub fn hardware_response_snapshots() -> Vec<HardwareResponseSnapshot> {
    let mut snapshots = eq1979_digitized_snapshots();
    snapshots.extend([
        pultec_low_boost_atten_target(),
        pultec_high_boost_atten_target(),
        api_mid_1500_boost_target(),
        api_low_high_smile_target(),
        ssl_e_channel_boost_cut_target(),
        ssl_g_channel_broad_boost_target(),
    ]);
    snapshots
}

pub fn eq1979_digitized_snapshots() -> Vec<HardwareResponseSnapshot> {
    vec![
        eq1979_raw_target(),
        eq1979_raw_mid_engaged_target(),
        eq1979_raw_low_engaged_target(),
        eq1979_low_110hz_7db_target(),
        eq1979_mid_1600hz_14db_target(),
        eq1979_high_shelf_14db_target(),
        eq1979_hpf_160hz_target(),
        eq1979_hpf_330hz_target(),
    ]
}

pub fn eq1979_raw_target() -> HardwareResponseSnapshot {
    csv_snapshot(
        HardwareTargetKind::Neve1073,
        HardwareTargetSettings::Neve1073(Neve1073Settings::default()),
        "eq1979_raw",
        include_str!("data/hardware_targets/eq1979_raw.csv"),
    )
}

pub fn eq1979_raw_mid_engaged_target() -> HardwareResponseSnapshot {
    csv_snapshot(
        HardwareTargetKind::Neve1073,
        HardwareTargetSettings::Neve1073(Neve1073Settings {
            mid_freq: Neve1073MidFreq::Hz1600,
            ..Default::default()
        }),
        "eq1979_raw_mid_engaged",
        include_str!("data/hardware_targets/eq1979_raw_mid_engaged.csv"),
    )
}

pub fn eq1979_raw_low_engaged_target() -> HardwareResponseSnapshot {
    csv_snapshot(
        HardwareTargetKind::Neve1073,
        HardwareTargetSettings::Neve1073(Neve1073Settings {
            low_freq: Neve1073LowFreq::Hz110,
            ..Default::default()
        }),
        "eq1979_raw_low_engaged",
        include_str!("data/hardware_targets/eq1979_raw_low_engaged.csv"),
    )
}

pub fn eq1979_low_110hz_7db_target() -> HardwareResponseSnapshot {
    csv_snapshot(
        HardwareTargetKind::Neve1073,
        HardwareTargetSettings::Neve1073(Neve1073Settings {
            low_freq: Neve1073LowFreq::Hz110,
            low_gain_db: 7.0,
            ..Default::default()
        }),
        "eq1979_low_110hz_7db",
        include_str!("data/hardware_targets/eq1979_low_110hz_7db.csv"),
    )
}

pub fn eq1979_mid_1600hz_14db_target() -> HardwareResponseSnapshot {
    csv_snapshot(
        HardwareTargetKind::Neve1073,
        HardwareTargetSettings::Neve1073(Neve1073Settings {
            mid_freq: Neve1073MidFreq::Hz1600,
            mid_gain_db: 14.0,
            ..Default::default()
        }),
        "eq1979_mid_1600hz_14db",
        include_str!("data/hardware_targets/eq1979_mid_1600hz_14db.csv"),
    )
}

pub fn eq1979_high_shelf_14db_target() -> HardwareResponseSnapshot {
    csv_snapshot(
        HardwareTargetKind::Neve1073,
        HardwareTargetSettings::Neve1073(Neve1073Settings {
            high_gain_db: 14.0,
            ..Default::default()
        }),
        "eq1979_high_shelf_14db",
        include_str!("data/hardware_targets/eq1979_high_shelf_14db.csv"),
    )
}

pub fn eq1979_hpf_160hz_target() -> HardwareResponseSnapshot {
    csv_snapshot(
        HardwareTargetKind::Neve1073,
        HardwareTargetSettings::Neve1073(Neve1073Settings {
            hpf: Neve1073Hpf::Hz160,
            ..Default::default()
        }),
        "eq1979_hpf_160hz",
        include_str!("data/hardware_targets/eq1979_hpf_160hz.csv"),
    )
}

pub fn eq1979_hpf_330hz_target() -> HardwareResponseSnapshot {
    csv_snapshot(
        HardwareTargetKind::Neve1073,
        HardwareTargetSettings::Neve1073(Neve1073Settings {
            hpf: Neve1073Hpf::Hz300,
            ..Default::default()
        }),
        "eq1979_hpf_330hz",
        include_str!("data/hardware_targets/eq1979_hpf_330hz.csv"),
    )
}

pub fn pultec_low_boost_atten_target() -> HardwareResponseSnapshot {
    let settings = PultecEqp1aSettings {
        low_freq_hz: 60.0,
        low_boost_db: 8.0,
        low_atten_db: 6.0,
        ..Default::default()
    };
    snapshot(
        HardwareTargetKind::PultecEqp1a,
        HardwareTargetSettings::Pultec(settings),
        "pultec_eqp1a_low_60_boost8_atten6",
        &[
            (20.0, 5.6, 1.2),
            (30.0, 6.8, 1.4),
            (60.0, 7.0, 1.8),
            (120.0, 3.1, 1.2),
            (300.0, -1.4, 1.4),
            (700.0, -1.0, 1.0),
            (1_000.0, -0.3, 0.8),
            (5_000.0, 0.0, 0.5),
            (12_000.0, -0.1, 0.5),
        ],
    )
}

pub fn pultec_high_boost_atten_target() -> HardwareResponseSnapshot {
    let settings = PultecEqp1aSettings {
        high_boost_freq_hz: 5_000.0,
        high_boost_db: 7.0,
        high_bandwidth: 6.0,
        high_atten_freq_hz: 10_000.0,
        high_atten_db: 4.0,
        ..Default::default()
    };
    snapshot(
        HardwareTargetKind::PultecEqp1a,
        HardwareTargetSettings::Pultec(settings),
        "pultec_eqp1a_high_5k_boost7_10k_atten4",
        &[
            (100.0, 0.0, 0.5),
            (700.0, 0.2, 0.7),
            (1_500.0, 1.1, 1.0),
            (3_000.0, 4.6, 1.5),
            (5_000.0, 6.8, 1.8),
            (8_000.0, 4.9, 1.3),
            (10_000.0, 2.0, 1.4),
            (14_000.0, -1.3, 1.2),
            (20_000.0, -3.5, 1.0),
        ],
    )
}

pub fn neve_hpf_160_target() -> HardwareResponseSnapshot {
    let settings = Neve1073Settings {
        hpf: Neve1073Hpf::Hz160,
        ..Default::default()
    };
    snapshot(
        HardwareTargetKind::Neve1073,
        HardwareTargetSettings::Neve1073(settings),
        "neve_1073_hpf_160",
        &[
            (30.0, -28.0, 1.2),
            (50.0, -18.0, 1.2),
            (80.0, -8.4, 1.5),
            (120.0, -2.6, 1.6),
            (160.0, -0.7, 1.8),
            (210.0, 0.8, 1.5),
            (320.0, 0.4, 1.0),
            (1_000.0, 0.0, 0.8),
            (10_000.0, -0.2, 0.5),
        ],
    )
}

pub fn neve_mid_1600_boost_target() -> HardwareResponseSnapshot {
    let settings = Neve1073Settings {
        mid_freq: Neve1073MidFreq::Hz1600,
        mid_gain_db: 14.0,
        ..Default::default()
    };
    snapshot(
        HardwareTargetKind::Neve1073,
        HardwareTargetSettings::Neve1073(settings),
        "neve_1073_mid_1600_boost14",
        &[
            (120.0, 0.2, 0.5),
            (350.0, -0.4, 0.8),
            (700.0, 4.8, 1.1),
            (1_000.0, 9.8, 1.5),
            (1_600.0, 14.0, 2.0),
            (2_500.0, 8.0, 1.5),
            (4_000.0, 2.3, 1.0),
            (8_000.0, 0.5, 0.6),
            (16_000.0, -0.3, 0.5),
        ],
    )
}

pub fn api_mid_1500_boost_target() -> HardwareResponseSnapshot {
    let settings = Api550aSettings {
        mid_freq_hz: 1_500.0,
        mid_gain_db: 9.0,
        ..Default::default()
    };
    snapshot(
        HardwareTargetKind::Api550a,
        HardwareTargetSettings::Api(settings),
        "api_550a_mid_1500_boost9",
        &[
            (150.0, 0.0, 0.5),
            (400.0, 0.4, 0.8),
            (800.0, 3.1, 1.2),
            (1_500.0, 8.8, 2.0),
            (2_400.0, 3.7, 1.2),
            (5_000.0, 0.4, 0.8),
            (12_000.0, -0.1, 0.5),
        ],
    )
}

pub fn api_low_high_smile_target() -> HardwareResponseSnapshot {
    let settings = Api550aSettings {
        low_freq_hz: 100.0,
        low_gain_db: 6.0,
        high_freq_hz: 10_000.0,
        high_gain_db: 6.0,
        ..Default::default()
    };
    snapshot(
        HardwareTargetKind::Api550a,
        HardwareTargetSettings::Api(settings),
        "api_550a_low100_high10k_boost6",
        &[
            (40.0, 5.6, 1.2),
            (100.0, 5.8, 1.8),
            (250.0, 2.0, 1.0),
            (1_000.0, 0.0, 0.8),
            (4_000.0, 0.8, 0.8),
            (10_000.0, 5.7, 1.8),
            (16_000.0, 5.0, 1.2),
        ],
    )
}

pub fn ssl_e_channel_boost_cut_target() -> HardwareResponseSnapshot {
    let settings = SslChannelSettings {
        hpf_hz: 80.0,
        lpf_hz: 18_000.0,
        lmf_freq_hz: 700.0,
        lmf_gain_db: -6.0,
        hmf_freq_hz: 3_000.0,
        hmf_gain_db: 6.0,
        ..SslChannelSettings::e_series()
    };
    snapshot(
        HardwareTargetKind::SslE,
        HardwareTargetSettings::Ssl(settings),
        "ssl_e_lmf700_cut6_hmf3k_boost6",
        &[
            (40.0, -8.0, 1.0),
            (80.0, -2.8, 1.2),
            (250.0, -1.1, 0.8),
            (700.0, -6.0, 1.8),
            (1_400.0, -1.3, 1.0),
            (3_000.0, 6.0, 1.8),
            (6_000.0, 1.7, 1.0),
            (18_000.0, -2.4, 1.2),
        ],
    )
}

pub fn ssl_g_channel_broad_boost_target() -> HardwareResponseSnapshot {
    let settings = SslChannelSettings {
        lf_freq_hz: 100.0,
        lf_gain_db: 4.0,
        hmf_freq_hz: 3_000.0,
        hmf_gain_db: 4.0,
        hf_freq_hz: 10_000.0,
        hf_gain_db: 3.0,
        ..SslChannelSettings::g_series()
    };
    snapshot(
        HardwareTargetKind::SslG,
        HardwareTargetSettings::Ssl(settings),
        "ssl_g_lf100_hmf3k_hf10k_broad_boost",
        &[
            (50.0, 3.8, 1.2),
            (100.0, 4.0, 1.5),
            (300.0, 1.4, 0.9),
            (1_000.0, 0.4, 0.7),
            (3_000.0, 4.0, 1.6),
            (6_000.0, 2.3, 1.0),
            (10_000.0, 3.2, 1.4),
            (16_000.0, 2.8, 0.9),
        ],
    )
}

fn snapshot(
    kind: HardwareTargetKind,
    settings: HardwareTargetSettings,
    id: &'static str,
    anchors: &[(f64, f64, f64)],
) -> HardwareResponseSnapshot {
    HardwareResponseSnapshot {
        kind,
        settings,
        target: ResponseTarget::new(
            id,
            TARGET_SAMPLE_RATE,
            anchors
                .iter()
                .map(|&(freq_hz, magnitude_db, weight)| {
                    ResponsePoint::weighted(freq_hz, magnitude_db, weight)
                })
                .collect(),
        ),
    }
}

fn csv_snapshot(
    kind: HardwareTargetKind,
    settings: HardwareTargetSettings,
    id: &'static str,
    csv: &'static str,
) -> HardwareResponseSnapshot {
    HardwareResponseSnapshot {
        kind,
        settings,
        target: ResponseTarget::new(id, TARGET_SAMPLE_RATE, parse_target_csv(csv)),
    }
}

fn parse_target_csv(csv: &str) -> Vec<ResponsePoint> {
    csv.lines()
        .skip(1)
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let mut fields = line.split(',');
            let freq_hz = fields.next()?.parse().ok()?;
            let magnitude_db = fields.next()?.parse().ok()?;
            let weight = fields.next()?.parse().ok()?;
            Some(ResponsePoint::weighted(freq_hz, magnitude_db, weight))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_suite_covers_first_profile_set() {
        let snapshots = hardware_response_snapshots();
        assert!(snapshots
            .iter()
            .any(|s| s.kind == HardwareTargetKind::PultecEqp1a));
        assert!(snapshots
            .iter()
            .any(|s| s.kind == HardwareTargetKind::Neve1073));
        assert!(snapshots
            .iter()
            .any(|s| s.kind == HardwareTargetKind::Api550a));
        assert!(snapshots.iter().any(|s| s.kind == HardwareTargetKind::SslE));
        assert!(snapshots.iter().any(|s| s.kind == HardwareTargetKind::SslG));
    }

    #[test]
    fn all_targets_evaluate_current_models() {
        for snapshot in hardware_response_snapshots() {
            let error = snapshot.evaluate_current_model();
            assert!(error.rms_db.is_finite(), "{}", snapshot.target.id);
            assert!(error.max_abs_db.is_finite(), "{}", snapshot.target.id);
        }
    }

    #[test]
    fn target_points_are_monotonic_by_frequency() {
        for snapshot in hardware_response_snapshots() {
            for pair in snapshot.target.points.windows(2) {
                assert!(
                    pair[0].freq_hz < pair[1].freq_hz,
                    "{} has unsorted points",
                    snapshot.target.id
                );
            }
        }
    }

    #[test]
    fn eq1979_digitized_targets_are_loaded_from_csv() {
        let snapshots = eq1979_digitized_snapshots();
        assert_eq!(snapshots.len(), 8);
        for snapshot in snapshots {
            assert!(
                snapshot.target.points.len() >= 90,
                "{} has too few digitized points",
                snapshot.target.id
            );
        }
    }
}
