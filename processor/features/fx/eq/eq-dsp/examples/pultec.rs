//! Independent boost and attenuation controls plus a separate coloration stage.
use eq_dsp::{ProcessSpec, hardware::hardware_eq::PultecEqp1aSettings};
fn main() -> Result<(), eq_dsp::Error> {
    let controls = PultecEqp1aSettings {
        low_boost_db: 8.0,
        low_atten_db: 5.0,
        drive_percent: 25.0,
        ..Default::default()
    };
    let prepared = controls.prepare(ProcessSpec::new(48_000.0, 512)?)?;
    let mut processor = prepared.processor();
    let mut left = [0.25; 512];
    let mut right = [0.0; 512];
    processor.process_stereo(&mut left, &mut right)?;
    println!(
        "Linear cascade at 60 Hz: {} dB",
        prepared.linear_filter().magnitude_db(60.0)?
    );
    Ok(())
}
