//! Native proof of the headless keys rig: a lane program whose pack arrives
//! as BYTES (the browser path — `pack_registry`), opened with
//! `KeysRig::open_headless`, driven through daw's live-MIDI ring, and
//! rendered by daw's own render path (`ProjectRenderer`) — no audio device
//! anywhere.

use daw_standalone::audio_engine::render::ProjectRenderer;
use signal_sampler::KeysRig;
use signal_sampler::keys_rig::{LaneEngine, LaneLayer, LaneProgram};
use signal_sampler::rig_node::Container;

const SR: u32 = 48_000;

/// Write a 1-second stereo tone as a wav, pack it (PCM-16) with an embedded
/// zone spec spanning the whole keyboard, and return the pack's BYTES.
fn build_tone_pack_bytes(dir: &std::path::Path) -> Vec<u8> {
    use fts_sample::cache::{PackCodec, PackSpecSource, create_signal_pack_with};

    std::fs::create_dir_all(dir).expect("tmp dir");
    let wav = dir.join("tone.wav");
    {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: SR,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(&wav, spec).expect("wav");
        for i in 0..SR as usize {
            // A4-ish sine at healthy level.
            let s = (i as f32 * 440.0 / SR as f32 * std::f32::consts::TAU).sin() * 0.5;
            w.write_sample(s).expect("write");
            w.write_sample(s).expect("write");
        }
        w.finalize().expect("finalize");
    }

    // Zone mode: one zone covering every key/velocity, rooted at C4.
    let spec_toml = r#"
name = "tone"

[[zones]]
file = "tone.wav"
key_min = 0
key_max = 127
root_key = 60
vel_min = 0
vel_max = 127
"#;

    let pack_path = dir.join("tone.signalpack");
    create_signal_pack_with(
        &pack_path,
        PackSpecSource::Text {
            text: spec_toml,
            format: "toml",
        },
        dir,
        [wav.as_path()].into_iter(),
        PackCodec::PcmI16,
    )
    .expect("build pack");
    std::fs::read(&pack_path).expect("read pack bytes")
}

#[test]
fn open_headless_renders_live_midi_from_pack_bytes() {
    let dir = std::env::temp_dir().join(format!("keys-worklet-headless-{}", std::process::id()));
    let bytes = build_tone_pack_bytes(&dir);

    // The browser seam: the pack reaches the engine as installed bytes,
    // keyed by the spec-path string the lane tree references.
    const PACK_KEY: &str = "test-tone.signalpack";
    signal_sampler::pack_registry::install(PACK_KEY, bytes).expect("install pack bytes");

    let program = LaneProgram {
        name: "Headless Test".into(),
        engines: vec![LaneEngine {
            name: "Keys".into(),
            layers: vec![LaneLayer {
                name: "Tone".into(),
                tree: Container::layer("Tone").sample_block("Tone", PACK_KEY),
            }],
        }],
        tail: None,
    };

    let rig = KeysRig::open_headless(SR, &program).expect("open_headless");
    assert!(rig.is_lanes());

    // Render through daw's own path: one renderer over the rig's project,
    // fed by the same live-MIDI ring an AudioEngine would install.
    let renderer = ProjectRenderer::new(rig.daw(), rig.project_guid(), SR);
    renderer.connect_live_midi(64);

    // Sample decode may lag the first note-on (background preload) —
    // retrigger until audible, like the native piano test.
    let mut heard = 0.0f32;
    for _ in 0..600 {
        rig.note_on(60, 100);
        let block = renderer.render_block(0, 512);
        let rms = (block.samples.iter().map(|s| s * s).sum::<f32>()
            / block.samples.len() as f32)
            .sqrt();
        heard = heard.max(rms);
        if heard > 1e-3 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        heard > 1e-3,
        "headless keys rig should render audible output from pack bytes, rms={heard}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
