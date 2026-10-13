//! Observational callback benchmark; run on an idle machine for comparisons.
use eq_dsp::{BandConfig, Dynamics, DynamicsMode, EqConfig, EqProcessor, ProcessSpec};
use std::hint::black_box;
use std::time::Instant;

fn main() -> Result<(), eq_dsp::Error> {
    for (name, mode, transient, modulated) in [
        ("static", DynamicsMode::Static, false, false),
        ("dynamic", DynamicsMode::Dynamic, false, false),
        ("spectral", DynamicsMode::Spectral, false, false),
        ("transient", DynamicsMode::Static, true, false),
        ("modulated cascade", DynamicsMode::Dynamic, false, true),
    ] {
        let mut config = EqConfig::new();
        for i in 0..24 {
            config.add_band(
                BandConfig::bell(100.0 * 1.19_f64.powi(i), -1.0, 1.0).dynamics(Dynamics {
                    mode,
                    range_db: -3.0,
                    ..Dynamics::default()
                }),
            )?;
        }
        config.transient.enabled = transient;
        if modulated {
            let ids: Vec<_> = config.bands().map(|(id, _)| id).collect();
            for id in ids {
                config.band_mut(id)?.filter = eq_dsp::Filter::FlatTilt {
                    frequency_hz: 1000.0,
                    gain_db: 1.0,
                    q: 0.707,
                    steepness: eq_dsp::Steepness::Order2,
                };
                config.band_mut(id)?.dynamics.threshold = eq_dsp::Threshold::FixedDb(-40.0);
            }
        }
        let prepared = config.prepare(ProcessSpec::new(48_000.0, 512)?)?;
        let mut processor = EqProcessor::new(&prepared)?;
        let mut left = [0.01; 512];
        let mut right = [0.007; 512];
        let mut timings = Vec::with_capacity(256);
        for _ in 0..32 {
            processor.process_stereo(&mut left, &mut right)?;
        }
        for _ in 0..256 {
            left.fill(0.01);
            right.fill(0.007);
            let start = Instant::now();
            processor.process_stereo(black_box(&mut left), black_box(&mut right))?;
            timings.push(start.elapsed());
        }
        timings.sort_unstable();
        if let (Some(median), Some(max)) = (timings.get(128), timings.last()) {
            println!(
                "{name}: 24 bands, 512 frames, median={median:?}, max={max:?}, block budget=10.667ms"
            );
        }
    }
    Ok(())
}
