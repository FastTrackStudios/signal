//! Public API contracts, including the first callback and parameter installation.
use eq_dsp::*;
#[path = "support/allocator.rs"]
mod allocator;
#[global_allocator]
static ALLOCATOR: allocator::Allocator = allocator::Allocator;

fn spec() -> ProcessSpec {
    ProcessSpec::new(48_000.0, 512).unwrap()
}

#[test]
fn stable_ids_survive_reordering_and_are_never_reused() {
    let mut config = EqConfig::with_capacity(2);
    let first = config.add_band(BandConfig::bell(100.0, 3.0, 1.0)).unwrap();
    let second = config
        .add_band(BandConfig::bell(3000.0, -3.0, 1.0))
        .unwrap();
    assert_eq!(
        config.add_band(BandConfig::bell(500.0, 1.0, 1.0)),
        Err(Error::CapacityExceeded)
    );
    let saved = *config.band(first).unwrap();
    config.move_band(first, 1).unwrap();
    assert_eq!(config.band(first), Ok(&saved));
    config.remove_band(second).unwrap();
    let third = config.add_band(saved).unwrap();
    assert_ne!(third, second);
    assert_eq!(config.band(second), Err(Error::UnknownBand));
}

#[test]
fn rejects_invalid_configuration_and_unstable_sections() {
    for rate in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(ProcessSpec::new(rate, 128), Err(Error::InvalidSampleRate));
    }
    assert_eq!(ProcessSpec::new(48_000.0, 0), Err(Error::InvalidBlockSize));
    assert_eq!(
        BiquadCoefficients::new([1.0, 0.0, 0.0], [0.0, 0.0, 0.0]),
        Err(Error::InvalidCoefficients)
    );
    assert_eq!(
        BiquadCoefficients::new([1.0, 0.0, 0.0], [1.0, -2.0, 0.0]),
        Err(Error::UnstableFilter)
    );
    for hz in [0.0, -1.0, 24_000.0, f64::NAN, f64::INFINITY] {
        assert!(
            Filter::Bell {
                frequency_hz: hz,
                gain_db: 3.0,
                q: 1.0,
                steepness: Steepness::Order2
            }
            .prepare(48_000.0)
            .is_err()
        );
    }
    assert!(
        Filter::BandShelf {
            frequency_hz: 1000.0,
            gain_db: 12.0,
            q: 0.1,
            steepness: Steepness::Order2
        }
        .prepare(48_000.0)
        .is_err()
    );
}

#[test]
fn invalid_buffers_leave_audio_and_history_unchanged() {
    let mut config = EqConfig::new();
    config.add_band(BandConfig::bell(1000.0, 6.0, 1.0)).unwrap();
    let prepared = config.prepare(spec()).unwrap();
    let mut processor = EqProcessor::new(&prepared).unwrap();
    let mut reference = EqProcessor::new(&prepared).unwrap();
    let mut left = [1.0; 513];
    let mut right = [0.5; 513];
    assert_eq!(
        processor.process_stereo(&mut left, &mut right[..512]),
        Err(Error::ChannelLengthMismatch)
    );
    assert_eq!(
        processor.process_stereo(&mut left, &mut right),
        Err(Error::BlockTooLarge)
    );
    assert_eq!(left, [1.0; 513]);
    assert_eq!(right, [0.5; 513]);
    let mut a = [0.1; 512];
    let mut b = a;
    processor.process_mono(&mut a).unwrap();
    reference.process_mono(&mut b).unwrap();
    assert_eq!(a, b);
}

#[test]
fn preparation_and_response_match_the_legacy_cascade() {
    let config = BandConfig::bell(1000.0, 6.0, 1.0);
    let filter = config.filter.prepare(48_000.0).unwrap();
    let mut modern = FilterProcessor::new(&filter);
    let mut old = eq_dsp::runtime::band::Band::new();
    old.freq_hz = 1000.0;
    old.gain_db = 6.0;
    old.q = 1.0;
    old.update(48_000.0);
    old.snap_bypass();
    for i in 0..4096 {
        let x = if i == 0 { 1.0 } else { 0.0 };
        assert_eq!(modern.process_sample(x), old.tick(x, 0));
    }
    for hz in [20.0, 1000.0, 20_000.0] {
        assert!((filter.magnitude_db(hz).unwrap() - old.magnitude_db(hz, 48_000.0)).abs() < 1e-10);
    }
}

#[test]
fn mixed_stereo_response_composes_complex_transfer_matrices() {
    let mut config = EqConfig::new();
    config
        .add_band(BandConfig::bell(1000.0, 6.0, 1.0).placement(Placement::Mid))
        .unwrap();
    config
        .add_band(BandConfig::bell(3000.0, -3.0, 1.0).placement(Placement::Left))
        .unwrap();
    let prepared = config.prepare(spec()).unwrap();
    let h = prepared.base_response(1000.0).unwrap();
    assert!(h.lr.mag() > 0.1);
    assert_ne!(h.ll, h.rr);
    // Independently multiply the two known placement matrices.
    let filters: Vec<_> = config
        .bands()
        .map(|(id, _)| prepared.filter(id).unwrap().response(1000.0).unwrap())
        .collect();
    let sum = (filters[0] + eq_dsp::math::zpk::Complex::ONE) * 0.5;
    assert!((h.ll - filters[1] * sum).mag() < 1e-12);
    assert!((h.rr - sum).mag() < 1e-12);
}

#[test]
fn first_blocks_updates_and_reset_do_not_allocate_in_all_modes() {
    for mode in [
        DynamicsMode::Static,
        DynamicsMode::Dynamic,
        DynamicsMode::Spectral,
    ] {
        let mut config = EqConfig::new();
        let id = config
            .add_band(BandConfig::bell(1000.0, 6.0, 1.0).dynamics(Dynamics {
                mode,
                range_db: -6.0,
                ..Dynamics::default()
            }))
            .unwrap();
        config.output.auto_gain = true;
        let prepared = config.prepare(spec()).unwrap();
        let mut processor = EqProcessor::new(&prepared).unwrap();
        let mut left = [0.3; 512];
        let mut right = [0.1; 512];
        allocator::assert_realtime(|| {
            for _ in 0..12 {
                processor.process_stereo(&mut left, &mut right).unwrap();
            }
            processor.reset();
        });
        config.band_mut(id).unwrap().filter = Filter::Bell {
            frequency_hz: 1200.0,
            gain_db: 4.0,
            q: 2.0,
            steepness: Steepness::Order2,
        };
        let update = config.prepare(spec()).unwrap();
        allocator::assert_realtime(|| processor.apply(&update).unwrap());
    }
}

#[test]
fn bounded_detector_redesign_does_not_allocate() {
    let mut config = EqConfig::new();
    config
        .add_band(
            BandConfig::new(Filter::FlatTilt {
                frequency_hz: 1000.0,
                gain_db: 1.0,
                q: 0.707,
                steepness: Steepness::Order2,
            })
            .dynamics(Dynamics {
                mode: DynamicsMode::Dynamic,
                range_db: -6.0,
                threshold: Threshold::FixedDb(-40.0),
                ..Dynamics::default()
            }),
        )
        .unwrap();
    let id = config.bands().next().unwrap().0;
    let prepared = config.prepare(spec()).unwrap();
    let mut processor = EqProcessor::new(&prepared).unwrap();
    assert!(processor.live_gain_db(id).unwrap().is_some());
    let mut left = [0.3; 512];
    let mut right = [0.1; 512];
    allocator::assert_realtime(|| {
        for _ in 0..100 {
            processor.process_stereo(&mut left, &mut right).unwrap();
        }
    });
}

#[test]
fn capacity_is_configurable_and_empty_configuration_is_valid() {
    for capacity in [0, 1, 32] {
        let mut config = EqConfig::with_capacity(capacity);
        for _ in 0..capacity {
            config.add_band(BandConfig::bell(1000.0, 0.0, 1.0)).unwrap();
        }
        let mut processor = EqProcessor::new(&config.prepare(spec()).unwrap()).unwrap();
        let mut samples = [0.25; 32];
        allocator::assert_realtime(|| processor.process_mono(&mut samples).unwrap());
        assert_eq!(samples, [0.25; 32]);
    }
}

#[test]
fn incompatible_update_is_transactional_and_latency_is_reported_before_use() {
    let config = EqConfig::new();
    let prepared = config.prepare(spec()).unwrap();
    let mut processor = EqProcessor::new(&prepared).unwrap();
    let incompatible = config
        .prepare(ProcessSpec::new(96_000.0, 512).unwrap())
        .unwrap();
    assert_eq!(
        processor.apply(&incompatible),
        Err(Error::IncompatiblePreparation)
    );
    let mut spectral = config;
    spectral
        .add_band(BandConfig::bell(1000.0, 0.0, 1.0).dynamics(Dynamics {
            mode: DynamicsMode::Spectral,
            range_db: -6.0,
            ..Dynamics::default()
        }))
        .unwrap();
    let prepared = spectral.prepare(spec()).unwrap();
    assert_eq!(prepared.latency_samples(), 4095);
    let processor = EqProcessor::new(&prepared).unwrap();
    assert_eq!(processor.latency_samples(), 4095);
}

#[test]
fn transient_listening_and_output_controls_are_realtime() {
    for listen_delta in [false, true] {
        let mut config = EqConfig::new();
        let id = config
            .add_band(BandConfig::bell(1200.0, 3.0, 1.0).stream(Stream::Steady))
            .unwrap();
        config.transient.enabled = true;
        config.output.character = Character::Warm;
        config.output.pan = Pan::MidSide(0.2);
        config.listen = Some(if listen_delta {
            Listen::Delta
        } else {
            Listen::Band(id)
        });
        let prepared = config.prepare(spec()).unwrap();
        let mut processor = EqProcessor::new(&prepared).unwrap();
        let mut left = [0.2; 512];
        let mut right = [-0.1; 512];
        allocator::assert_realtime(|| {
            processor.process_stereo(&mut left, &mut right).unwrap();
            processor.apply(&prepared).unwrap();
            processor.reset();
            processor.process_stereo(&mut left, &mut right).unwrap();
        });
        assert!(left.iter().chain(&right).all(|x| x.is_finite()));
    }
}

#[test]
fn single_filter_updates_crossfade_and_do_not_allocate() {
    let mut processor = FilterProcessor::default();
    let old = PreparedFilter::from_sections(48_000.0, &[[1.0, 0.0, 0.0, 1.0, 0.0, 0.0]]).unwrap();
    let new = PreparedFilter::from_sections(48_000.0, &[[1.0, 0.0, 0.0, 0.0, 0.0, 0.0]]).unwrap();
    processor.install(&old);
    assert_eq!(processor.process_sample(1.0), 1.0);
    allocator::assert_realtime(|| processor.install(&new));
    let first = processor.process_frame([1.0, 1.0]);
    assert!(first[0] > 0.99 && first[0] < 1.0);
    assert_eq!(first[0], first[1]);
    for _ in 0..240 {
        let _ = processor.process_frame([1.0, 1.0]);
    }
    assert_eq!(processor.process_frame([1.0, 1.0]), [0.0, 0.0]);
}

#[test]
fn configured_low_level_filters_are_realtime_at_every_supported_order() {
    for steepness in [
        Steepness::Order2,
        Steepness::Order3,
        Steepness::Order4,
        Steepness::Order5,
        Steepness::Order6,
        Steepness::Order8,
        Steepness::Order12,
        Steepness::Order16,
    ] {
        let mut processor = FilterProcessor::default();
        allocator::assert_realtime(|| {
            processor
                .configure(
                    Filter::Bell {
                        frequency_hz: 1000.0,
                        gain_db: 6.0,
                        q: 0.707,
                        steepness,
                    },
                    48_000.0,
                )
                .unwrap()
        });
    }
}

#[test]
fn reset_matches_fresh_processing_including_spectral_learning() {
    for mode in [
        DynamicsMode::Static,
        DynamicsMode::Dynamic,
        DynamicsMode::Spectral,
    ] {
        let mut config = EqConfig::new();
        config
            .add_band(BandConfig::bell(1000.0, 3.0, 1.0).dynamics(Dynamics {
                mode,
                range_db: -6.0,
                ..Dynamics::default()
            }))
            .unwrap();
        let prepared = config.prepare(spec()).unwrap();
        let mut processor = EqProcessor::new(&prepared).unwrap();
        let mut fresh = EqProcessor::new(&prepared).unwrap();
        let mut history = [0.3; 512];
        for _ in 0..16 {
            processor.process_mono(&mut history).unwrap();
        }
        allocator::assert_realtime(|| processor.reset());
        for _ in 0..16 {
            let mut a = [0.15; 512];
            let mut b = a;
            processor.process_mono(&mut a).unwrap();
            fresh.process_mono(&mut b).unwrap();
            if let Some((i, (a, b))) = a.iter().zip(&b).enumerate().find(|(_, (a, b))| a != b) {
                panic!("{mode:?} reset differs at sample {i}: {a} vs {b}");
            }
        }
    }
}

#[test]
fn matrix_response_matches_stereo_audio_routing() {
    let mut config = EqConfig::new();
    config
        .add_band(BandConfig::bell(1000.0, 6.0, 1.0).placement(Placement::Mid))
        .unwrap();
    config
        .add_band(BandConfig::bell(3000.0, -3.0, 1.0).placement(Placement::Left))
        .unwrap();
    let prepared = config.prepare(spec()).unwrap();
    let expected = prepared.base_response(1000.0).unwrap();
    let mut processor = EqProcessor::new(&prepared).unwrap();
    for _ in 0..24 {
        processor
            .process_stereo(&mut [0.0; 512], &mut [0.0; 512])
            .unwrap();
    }
    let mut ll = Complex::ZERO;
    let mut rl = Complex::ZERO;
    for block in 0..16 {
        let mut left = [0.0; 512];
        let mut right = [0.0; 512];
        if block == 0 {
            left[0] = 1.0;
        }
        processor.process_stereo(&mut left, &mut right).unwrap();
        for (i, (&l, &r)) in left.iter().zip(&right).enumerate() {
            let phase =
                -core::f64::consts::TAU * 1000.0 * dsp_core::num::count_to_f64(block * 512 + i)
                    / 48_000.0;
            let z = Complex::from_polar(1.0, phase);
            ll = ll + z * l;
            rl = rl + z * r;
        }
    }
    assert!((ll - expected.ll).mag() < 1e-8);
    assert!((rl - expected.rl).mag() < 1e-8);
}
