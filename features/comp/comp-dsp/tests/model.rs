use comp_dsp::components::{Compressor, Envelope, HardKnee, PeakDetector, Transparent};
use comp_dsp::{CompressorConfig, Error, GenericControls, La2aControls, Model, ProcessSpec};
use comp_dsp::{HermiteCubicSmoother, StateFuncHypothesis};
use dsp_core::Channel;
dsp_golden::install_counting_allocator!();

#[test]
fn smoother_settles_to_requested_amplitude_gain() {
    for db in [-1.0, -6.0, -12.0, -24.0, -40.0] {
        let mut smoother = HermiteCubicSmoother::new(StateFuncHypothesis::Identity);
        let gain = 10.0f64.powf(db / 20.0);
        let mut actual = 1.0;
        for _ in 0..8192 {
            actual = smoother.process(gain, 0.5, 0.5, Channel::LEFT);
        }
        assert!((20.0 * actual.log10() - db).abs() < 0.01);
    }
}

#[test]
fn invalid_controls_and_buffers_fail_before_processing() {
    let spec = ProcessSpec::new(48000.0, 16).unwrap();
    let bad = CompressorConfig::new(Model::La2aGray(La2aControls {
        peak_reduction: f64::NAN,
        ..Default::default()
    }));
    assert!(matches!(bad.prepare(spec), Err(Error::InvalidControls)));
    let prepared = CompressorConfig::new(Model::Generic(GenericControls::default()))
        .prepare(spec)
        .unwrap();
    let mut processor = prepared.processor();
    let mut left = [0.5; 16];
    let mut right = [0.25; 8];
    assert_eq!(
        processor.process_stereo(&mut left, &mut right),
        Err(Error::ChannelLengthMismatch)
    );
    assert_eq!(left, [0.5; 16]);
    assert_eq!(right, [0.25; 8]);
    assert_eq!(
        processor.process_mono(&mut [0.5; 17]),
        Err(Error::BlockTooLarge)
    );
    assert_eq!(processor.gain_reduction_db(), [0.0; 2]);
}

#[test]
fn models_are_partition_invariant_and_reset_to_fresh() {
    for model in [
        Model::Generic(GenericControls::default()),
        Model::La2aGray(La2aControls {
            peak_reduction: 0.67,
            ..Default::default()
        }),
    ] {
        let prepared = CompressorConfig::new(model)
            .prepare(ProcessSpec::new(48000.0, 1024).unwrap())
            .unwrap();
        let mut full = prepared.processor();
        let mut split = prepared.processor();
        let mut left = [0.7; 1024];
        let mut right = [0.1; 1024];
        let mut a = left;
        let mut b = right;
        full.process_stereo(&mut left, &mut right).unwrap();
        for (l, r) in a.chunks_mut(37).zip(b.chunks_mut(37)) {
            split.process_stereo(l, r).unwrap();
        }
        assert_eq!(a, left);
        assert_eq!(b, right);
        split.reset();
        let mut fresh = prepared.processor();
        for _ in 0..1024 {
            assert_eq!(
                split.process_frame([0.6, -0.2]),
                fresh.process_frame([0.6, -0.2])
            );
        }
        assert_eq!(split.gain_reduction_db()[0], split.gain_reduction_db()[1]);
    }
}

#[test]
fn unlinked_channels_do_not_share_detector_history() {
    let mut config = CompressorConfig::new(Model::Generic(GenericControls::default()));
    config.stereo_link = 0.0;
    let mut processor = config
        .prepare(ProcessSpec::new(48000.0, 16).unwrap())
        .unwrap()
        .processor();
    for _ in 0..48000 {
        processor.process_frame([1.0, 0.001]);
    }
    let [left, right] = processor.gain_reduction_db();
    assert!(left > 10.0);
    assert_eq!(right, 0.0);
}

struct Instant;
impl Envelope for Instant {
    fn reduction_db(&mut self, target: f64) -> f64 {
        target
    }
    fn reset(&mut self) {}
}
#[test]
fn a_custom_envelope_can_replace_the_algorithm() {
    let mut compressor = Compressor::new(
        PeakDetector::new(48000.0, 2.0),
        HardKnee {
            threshold_db: -24.0,
            ratio: 4.0,
        },
        Instant,
        Transparent,
    );
    let output = compressor.process(1.0);
    assert!((20.0 * output.log10() + 18.0).abs() < 1e-10);
    assert_eq!(compressor.gain_reduction_db(), 18.0);
}

#[test]
fn built_in_callbacks_and_reset_allocate_nothing_from_the_first_block() {
    for model in [
        Model::Generic(GenericControls::default()),
        Model::La2aGray(La2aControls::default()),
    ] {
        let mut processor = CompressorConfig::new(model)
            .prepare(ProcessSpec::new(48000.0, 512).unwrap())
            .unwrap()
            .processor();
        let mut left = [0.7; 512];
        let mut right = [0.2; 512];
        dsp_golden::assert_no_alloc(|| {
            processor.process_stereo(&mut left, &mut right).unwrap();
            processor.reset();
            processor.process_stereo(&mut left, &mut right).unwrap();
        });
    }
}

#[test]
fn prepared_updates_preserve_history_and_reject_topology_changes() {
    let spec = ProcessSpec::new(48000.0, 512).unwrap();
    for model in [
        Model::Generic(GenericControls::default()),
        Model::La2aGray(La2aControls {
            peak_reduction: 0.67,
            ..Default::default()
        }),
    ] {
        let prepared = CompressorConfig::new(model).prepare(spec).unwrap();
        let mut updated = prepared.processor();
        let mut unchanged = prepared.processor();
        for _ in 0..1024 {
            updated.process_frame([0.8; 2]);
            unchanged.process_frame([0.8; 2]);
        }
        dsp_golden::assert_no_alloc(|| updated.apply(&prepared).unwrap());
        for _ in 0..512 {
            assert_eq!(
                updated.process_frame([0.2; 2]),
                unchanged.process_frame([0.2; 2])
            );
        }
        let different = match model {
            Model::Generic(_) => Model::La2aGray(La2aControls::default()),
            Model::La2aGray(_) => Model::Generic(GenericControls::default()),
        };
        let next = CompressorConfig::new(different).prepare(spec).unwrap();
        assert_eq!(updated.apply(&next), Err(Error::IncompatiblePreparation));
        assert_eq!(
            updated.process_frame([0.3; 2]),
            unchanged.process_frame([0.3; 2])
        );
        let mut changed = CompressorConfig::new(model);
        changed.mix = 0.0;
        let next = changed.prepare(spec).unwrap();
        dsp_golden::assert_no_alloc(|| updated.apply(&next).unwrap());
        assert_eq!(updated.process_frame([0.8, 0.2]), [0.8, 0.2]);
    }
}

#[test]
fn reserved_lookahead_edits_do_not_allocate() {
    let mut chain = comp_dsp::CompChain::new();
    chain.reserve_lookahead(20.0);
    dsp_golden::assert_no_alloc(|| {
        for delay in [20.0, 0.0, 3.0, 20.0] {
            chain.set_lookahead(delay);
            let (mut l, mut r) = (0.5, 0.25);
            chain.process_sample(&mut l, &mut r);
        }
        chain.reset();
    });
}

#[test]
fn the_host_chain_runs_the_selected_model_and_reports_its_reduction() {
    let controls = CompressorConfig::new(Model::La2aGray(La2aControls {
        peak_reduction: 0.67,
        gain: 0.286,
    }));
    let prepared = controls
        .prepare(ProcessSpec::new(48000.0, 512).unwrap())
        .unwrap();
    let mut direct = prepared.processor();
    let mut chain = comp_dsp::CompChain::new();
    dsp_golden::assert_no_alloc(|| chain.set_model(&prepared));
    chain.reset();
    for _ in 0..4800 {
        let (mut left, mut right) = (0.5, 0.2);
        chain.process_sample(&mut left, &mut right);
        assert_eq!([left, right], direct.process_frame([0.5, 0.2]));
    }
    assert_eq!(chain.gain_reduction_db(), direct.gain_reduction_db()[0]);
    dsp_golden::assert_no_alloc(|| {
        chain.clear_model();
        for _ in 0..512 {
            let (mut left, mut right) = (0.5, 0.2);
            chain.process_sample(&mut left, &mut right);
            assert!(left.is_finite() && right.is_finite());
        }
    });
}
