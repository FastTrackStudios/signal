use comp_dsp::{CompressionStyle, GainCurve};

#[test]
fn downward_soft_knee_is_continuous_monotonic_and_never_boosts() {
    for knee in [0.0, 2.0, 12.0, 30.0] {
        let mut curve = GainCurve::new(48000.0);
        curve.set_threshold(-18.0);
        curve.set_ratio(4.0);
        curve.set_knee(knee);
        let mut previous = 1.0;
        for i in 0..10001 {
            let level = -60.0 + f64::from(i) * 0.01;
            let gain = curve.compute_gr(level);
            assert!(gain.is_finite() && (0.0..=1.0).contains(&gain));
            assert!(gain <= previous + 1e-12);
            previous = gain;
        }
        assert!(curve.compute_gr(-18.0).is_finite());
        for boundary in [-18.0 - knee * 0.5, -18.0 + knee * 0.5] {
            assert!(
                (curve.compute_gr(boundary - 1e-6) - curve.compute_gr(boundary + 1e-6)).abs()
                    < 1e-6
            );
        }
    }
}

#[test]
fn all_style_time_scales_keep_poles_inside_the_unit_circle() {
    for sr in [8000.0, 48000.0, 192000.0] {
        let mut curve = GainCurve::new(sr);
        for style in [
            CompressionStyle::Clean,
            CompressionStyle::Fet,
            CompressionStyle::Vca,
            CompressionStyle::Optical,
        ] {
            curve.set_style(style);
            for ms in [0.1, 1.0, 100.0, 10000.0] {
                curve.set_time_constants_ms(ms, ms);
                for freq in [20.0, 1000.0, 20000.0] {
                    let (attack, release) = curve.apply_coefficient_scaling(freq);
                    assert!((0.0..1.0).contains(&attack));
                    assert!((0.0..1.0).contains(&release));
                }
            }
        }
    }
}
