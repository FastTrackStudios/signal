use eq_dsp::hardware::hardware_eq::PultecEqp1aSettings;
use eq_dsp::model::{Coloration, ColorationPlacement, PreparedModel};
use eq_dsp::{Error, ProcessSpec};
dsp_golden::install_counting_allocator!();

#[test]
fn pultec_prepares_interacting_controls_and_rejects_invalid_settings() {
    let spec = ProcessSpec::new(48000.0, 512).unwrap();
    let settings = PultecEqp1aSettings {
        low_boost_db: 8.0,
        low_atten_db: 5.0,
        drive_percent: 25.0,
        ..Default::default()
    };
    let prepared = settings.prepare(spec).unwrap();
    let boost = PultecEqp1aSettings {
        low_atten_db: 0.0,
        ..settings
    }
    .prepare(spec)
    .unwrap();
    assert!(
        (prepared.linear_filter().magnitude_db(30.0).unwrap()
            - boost.linear_filter().magnitude_db(30.0).unwrap())
        .abs()
            > 1.0
    );
    let bad = PultecEqp1aSettings {
        low_freq_hz: 61.0,
        ..settings
    };
    assert!(matches!(bad.prepare(spec), Err(Error::InvalidFrequency)));
    assert!(
        PultecEqp1aSettings {
            drive_percent: f64::NAN,
            ..settings
        }
        .prepare(spec)
        .is_err()
    );
    let mut unit = prepared.processor();
    let mut left = [0.25; 512];
    let mut right = [0.0; 512];
    dsp_golden::assert_no_alloc(|| {
        unit.process_stereo(&mut left, &mut right).unwrap();
        unit.reset();
    });
    assert!(left.iter().all(|x| x.is_finite()));
    assert_eq!(right, [0.0; 512]);
}

#[derive(Clone)]
struct Memory(f64);
impl Coloration for Memory {
    fn update(&mut self, _prepared: &Self) {}
    fn process(&mut self, sample: f64) -> f64 {
        let out = sample + self.0 * 0.5;
        self.0 = sample;
        out
    }
    fn reset(&mut self) {
        self.0 = 0.0;
    }
}
#[test]
fn custom_coloration_has_independent_channels_and_reset_history() {
    let spec = ProcessSpec::new(48000.0, 512).unwrap();
    let base = PultecEqp1aSettings::default().prepare(spec).unwrap();
    let prepared = PreparedModel::new(
        base.linear_filter().clone(),
        Memory(99.0),
        ColorationPlacement::AfterFilters,
        0.0,
        spec,
    )
    .unwrap();
    let mut processor = prepared.processor();
    for _ in 0..32 {
        assert_eq!(processor.process_frame([0.5, 0.0])[1], 0.0);
    }
    processor.reset();
    let mut fresh = prepared.processor();
    for _ in 0..32 {
        assert_eq!(
            processor.process_frame([0.1, -0.2]),
            fresh.process_frame([0.1, -0.2])
        );
    }
}

#[test]
fn model_blocks_are_partition_invariant_and_bad_buffers_are_transactional() {
    let spec = ProcessSpec::new(48000.0, 512).unwrap();
    let prepared = PultecEqp1aSettings {
        low_boost_db: 10.0,
        drive_percent: 50.0,
        ..Default::default()
    }
    .prepare(spec)
    .unwrap();
    let mut whole = prepared.processor();
    let mut split = prepared.processor();
    let mut left = [0.5; 512];
    let mut right = [-0.25; 512];
    let mut a = left;
    let mut b = right;
    whole.process_stereo(&mut left, &mut right).unwrap();
    for (l, r) in a.chunks_mut(17).zip(b.chunks_mut(17)) {
        split.process_stereo(l, r).unwrap();
    }
    assert_eq!(left, a);
    assert_eq!(right, b);
    let mut l = [0.5; 16];
    let mut r = [0.25; 8];
    assert_eq!(
        split.process_stereo(&mut l, &mut r),
        Err(Error::ChannelLengthMismatch)
    );
    assert_eq!(l, [0.5; 16]);
    assert_eq!(r, [0.25; 8]);
}

#[test]
fn nonlinear_stage_order_is_part_of_the_model() {
    use eq_dsp::model::AnalogColoration;
    let spec = ProcessSpec::new(48000.0, 512).unwrap();
    let base = PultecEqp1aSettings {
        low_boost_db: 12.0,
        ..Default::default()
    }
    .prepare(spec)
    .unwrap();
    let color = AnalogColoration::Arctangent { drive: 0.5 };
    let mut before = PreparedModel::new(
        base.linear_filter().clone(),
        color,
        ColorationPlacement::BeforeFilters,
        0.0,
        spec,
    )
    .unwrap()
    .processor();
    let mut after = PreparedModel::new(
        base.linear_filter().clone(),
        color,
        ColorationPlacement::AfterFilters,
        0.0,
        spec,
    )
    .unwrap()
    .processor();
    let mut difference = 0.0;
    for _ in 0..512 {
        difference += (before.process_frame([0.8; 2])[0] - after.process_frame([0.8; 2])[0]).abs();
    }
    assert!(difference > 1.0);
}

#[test]
fn invalid_raw_redesign_retains_a_running_stable_filter() {
    use eq_dsp::{FilterType, runtime::band::Band};
    let mut band = Band::new();
    band.filter_type = FilterType::Peak;
    band.freq_hz = 1000.0;
    band.q = 1.0;
    band.gain_db = 6.0;
    band.update(48000.0);
    for _ in 0..512 {
        band.tick(0.25, 0);
    }
    let mut unchanged = band.clone();
    band.filter_type = FilterType::BandShelf;
    band.q = 0.1;
    band.update(48000.0);
    assert_eq!(band.last_design_error(), Some(Error::UnstableFilter));
    for _ in 0..512 {
        assert_eq!(band.tick(0.25, 0), unchanged.tick(0.25, 0));
    }
}

#[test]
fn hardware_host_matches_prepared_pultec_and_updates_without_allocation() {
    use eq_dsp::hardware::hardware_eq::{HardwareEqModel, HardwareEqSettings};
    let settings = PultecEqp1aSettings {
        low_boost_db: 8.0,
        low_atten_db: 4.0,
        drive_percent: 20.0,
        ..Default::default()
    };
    let mut host = HardwareEqModel::new(48000.0, HardwareEqSettings::Pultec(settings));
    let spec = ProcessSpec::new(48000.0, 512).unwrap();
    let mut direct = settings.prepare(spec).unwrap().processor();
    let mut left = [0.5; 512];
    let mut right = [0.0; 512];
    host.process(&mut left, &mut right);
    for (l, r) in left.into_iter().zip(right) {
        assert_eq!([l, r], direct.process_frame([0.5, 0.0]));
    }
    dsp_golden::assert_no_alloc(|| {
        let next = PultecEqp1aSettings {
            low_boost_db: 12.0,
            trim_db: -3.0,
            drive_percent: 70.0,
            ..settings
        };
        let prepared = next.prepare(spec).unwrap();
        direct.apply(&prepared).unwrap();
        host.set_settings(HardwareEqSettings::Pultec(next));
        host.process(&mut left, &mut right);
        host.reset();
    });
    assert_eq!(host.last_design_error(), None);
}

#[test]
fn incompatible_model_update_is_transactional() {
    let spec = ProcessSpec::new(48000.0, 512).unwrap();
    let settings = PultecEqp1aSettings::default();
    let prepared = settings.prepare(spec).unwrap();
    let mut running = prepared.processor();
    let mut reference = prepared.processor();
    for _ in 0..100 {
        assert_eq!(
            running.process_frame([0.5; 2]),
            reference.process_frame([0.5; 2])
        );
    }
    let incompatible = settings
        .prepare(ProcessSpec::new(96000.0, 512).unwrap())
        .unwrap();
    assert_eq!(
        running.apply(&incompatible),
        Err(Error::IncompatiblePreparation)
    );
    for _ in 0..100 {
        assert_eq!(
            running.process_frame([0.5; 2]),
            reference.process_frame([0.5; 2])
        );
    }
}

#[test]
fn hardware_updates_preserve_valid_configuration_and_history() {
    use eq_dsp::hardware::hardware_eq::{HardwareEqModel, HardwareEqSettings};
    use eq_dsp::hardware::neve_1073::{Neve1073Model, Neve1073Settings};
    let settings = PultecEqp1aSettings::default();
    let mut host = HardwareEqModel::new(48000.0, HardwareEqSettings::Pultec(settings));
    host.set_settings(HardwareEqSettings::Pultec(PultecEqp1aSettings {
        low_boost_db: 14.0,
        ..settings
    }));
    assert_eq!(host.last_design_error(), Some(Error::InvalidGain));
    assert_eq!(host.settings(), HardwareEqSettings::Pultec(settings));
    host.set_sample_rate(f64::NAN);
    assert_eq!(host.last_design_error(), Some(Error::InvalidSampleRate));
    assert_eq!(host.prepared_filter().unwrap().sample_rate(), 48000.0);

    let settings = Neve1073Settings::default();
    let mut host = Neve1073Model::new(48000.0, settings);
    let mut reference = Neve1073Model::new(48000.0, settings);
    for _ in 0..4 {
        let mut left = [0.5; 512];
        let mut right = [-0.25; 512];
        let mut expected_left = left;
        let mut expected_right = right;
        dsp_golden::assert_no_alloc(|| {
            host.set_settings(settings);
            host.process(&mut left, &mut right);
        });
        reference.process(&mut expected_left, &mut expected_right);
        assert_eq!(left, expected_left);
        assert_eq!(right, expected_right);
    }
}
