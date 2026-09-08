//! Model-specific controls can be chosen at the application's configuration boundary.
use comp_dsp::{CompressorConfig, GenericControls, La2aControls, Model, ProcessSpec};
fn main() -> Result<(), comp_dsp::Error> {
    let spec = ProcessSpec::new(48_000.0, 512)?;
    for model in [
        Model::Generic(GenericControls::default()),
        Model::La2aGray(La2aControls {
            peak_reduction: 0.67,
            gain: 0.286,
        }),
    ] {
        let prepared = CompressorConfig::new(model).prepare(spec)?;
        let mut processor = prepared.processor();
        let mut left = [0.5; 512];
        let mut right = [0.25; 512];
        processor.process_stereo(&mut left, &mut right)?;
        println!(
            "{model:?}: reduction {:?} dB",
            processor.gain_reduction_db()
        );
    }
    Ok(())
}
