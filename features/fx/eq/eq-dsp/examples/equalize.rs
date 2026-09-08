//! Run with `cargo run -p eq-dsp --example equalize`.
use eq_dsp::{BandConfig, CutSlope, EqConfig, EqProcessor, Placement, ProcessSpec};

fn main() -> Result<(), eq_dsp::Error> {
    let mut config = EqConfig::new();
    let presence =
        config.add_band(BandConfig::bell(3_000.0, 2.5, 0.8).placement(Placement::Mid))?;
    config.add_band(BandConfig::high_pass(80.0, CutSlope::DbPerOctave(24.0)))?;
    let spec = ProcessSpec::new(48_000.0, 512)?;
    let prepared = config.prepare(spec)?;
    let mut processor = EqProcessor::new(&prepared)?;
    let mut left = [0.0; 512];
    let mut right = [0.0; 512];
    processor.process_stereo(&mut left, &mut right)?;

    // Control side: edit intent and validate a complete update.
    config.band_mut(presence)?.enabled = false;
    let update = config.prepare(spec)?;
    // Audio side: borrow the update; its owner reclaims it off-thread.
    processor.apply(&update)?;
    processor.process_stereo(&mut left, &mut right)?;
    Ok(())
}
