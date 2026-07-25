//! Sanity-check the AIFF decoder against a real Stylus RMX loop, when
//! the AudioHaven extraction tree is mounted. Skipped otherwise so CI
//! without the data doesn't fail.

use signal_sampler::engine::cache::load_sample;
use std::path::Path;

#[test]
fn decode_real_stylus_loop() {
    let path = Path::new(
        "/run/media/AudioHaven/Sampled/Drum Kits/Stylus RMX-fresh/Stylus RMX/Core Library/Sound Menus/SOUND MENUS  -  Hi-Hats/MENU - Hi-Hats Lo Fi 1.aif",
    );
    if !path.exists() {
        eprintln!(
            "skipped — Stylus extraction not mounted at {}",
            path.display()
        );
        return;
    }
    let data = load_sample(path).expect("decode real Stylus AIFF");
    assert!(data.channels == 1 || data.channels == 2);
    assert_eq!(data.sample_rate, 44100);
    assert!(data.num_frames > 0);
    assert_eq!(data.len(), data.num_frames * data.channels as usize);
    let pcm = data.to_f32();
    let peak = pcm
        .iter()
        .copied()
        .fold(0.0_f32, |a, b| a.max(b.abs()));
    assert!(
        peak > 0.0 && peak <= 1.0,
        "expected non-silent in-range PCM, peak={peak}"
    );
}
