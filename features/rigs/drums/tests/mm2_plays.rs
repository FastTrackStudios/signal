//! Regression guard: an MM2 `.signalpreset` loaded via `load_preset_kit`
//! actually produces audio when driven on the GM percussion channel.
//!
//! Skips when the AudioHaven library isn't mounted (CI / other machines).
//! This is the test the canonical `bench_drum_load` example lacked — it would
//! have caught the "note_on dispatches on channel 0, preset mapped to none →
//! silence" footgun.

use std::time::{Duration, Instant};

use signal_drums::{GM_DRUM_CHANNEL, load_preset_kit};
use signal_sampler::SamplerRig;

const PRESET: &str = "/run/media/AudioHaven/Signal/Libraries/Drum Kits/\
GGD Modern and Massive 2/Presets/Metal Monster.signalpreset";

#[test]
fn mm2_metal_monster_plays_on_gm_channel() {
    let preset = std::path::Path::new(PRESET);
    if !preset.exists() {
        eprintln!("skip: MM2 library not mounted");
        return;
    }

    let rig = SamplerRig::new_offline(48_000);
    let ids = load_preset_kit(&rig, "kit", preset).expect("load_preset_kit");
    assert!(!ids.is_empty(), "preset loaded no engines");

    // Wait for the background FLAC preload so the offline walk doesn't miss.
    let start = Instant::now();
    loop {
        let (mut loaded, mut total) = (0usize, 0usize);
        for id in &ids {
            let (l, t) = rig.preload_progress(id);
            loaded += l;
            total += t;
        }
        if total > 0 && loaded >= total {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(240),
            "preload timed out at {loaded}/{total}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }

    // Drive kick (36) + snare (38) on the GM percussion channel (10 → index 9),
    // exactly as an e-kit / MIDI file would, and render ~0.4 s.
    rig.midi_message(GM_DRUM_CHANNEL, 0x90, 36, 120);
    rig.midi_message(GM_DRUM_CHANNEL, 0x90, 38, 120);

    let mut block = vec![0.0f32; 512 * 2];
    let mut peak = 0.0f32;
    for _ in 0..40 {
        for s in block.iter_mut() {
            *s = 0.0;
        }
        rig.render_offline(&mut block).expect("render");
        for &s in &block {
            peak = peak.max(s.abs());
        }
    }

    assert!(
        peak > 1e-3,
        "MM2 kit rendered silence (peak={peak}) — note routing / channel map regressed"
    );
    eprintln!("MM2 Metal Monster played: {} engines, master peak {peak:.4}", ids.len());
}
