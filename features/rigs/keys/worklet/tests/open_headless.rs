//! Native proof of the headless keys rig: a lane program whose pack arrives
//! as BYTES (the browser path — `pack_registry`), opened with
//! `KeysRig::open_headless`, driven through daw's live-MIDI ring, and
//! rendered by daw's own render path (`ProjectRenderer`) — no audio device
//! anywhere.

use daw_standalone::audio_engine::render::ProjectRenderer;
use signal_sampler::keys_rig::{LaneEngine, LaneLayer, LaneProgram};
use signal_sampler::rig_node::Container;
use signal_sampler::KeysRig;

const SR: u32 = 48_000;

/// Write a 1-second stereo tone as a wav, pack it (PCM-16) with an embedded
/// zone spec spanning the whole keyboard, and return the pack's BYTES.
fn build_tone_pack_bytes(dir: &std::path::Path) -> Vec<u8> {
    use fts_sample::cache::{create_signal_pack_with, PackCodec, PackSpecSource};

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

/// The W6 browser seam, proven natively: the pack's bytes stay OUTSIDE the
/// engine (here: a test-global map standing in for the worklet's JS heap),
/// reachable only through the pluggable external reader; the registry entry
/// is `install_external`, exactly what `attachPackExternal` does on wasm.
/// `warm_note` runs before the note-on, the way the worklet's control side
/// does for budget-skipped samples.
#[test]
fn open_headless_renders_from_external_pack_reader() {
    use std::collections::HashMap;
    use std::sync::Mutex;

    static PACKS: Mutex<Option<HashMap<u32, Vec<u8>>>> = Mutex::new(None);

    let dir = std::env::temp_dir().join(format!("keys-worklet-external-{}", std::process::id()));
    let bytes = build_tone_pack_bytes(&dir);
    let len = bytes.len() as u64;

    const ID: u32 = 42;
    PACKS
        .lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .insert(ID, bytes);
    // First install wins process-wide; this test file installs exactly once.
    signal_sampler::engine::cache::set_external_pack_reader(Box::new(|id, offset, dst| {
        let packs = PACKS.lock().unwrap();
        let Some(bytes) = packs.as_ref().and_then(|m| m.get(&id)) else {
            return false;
        };
        let start = offset as usize;
        let Some(src) = bytes.get(start..start + dst.len()) else {
            return false;
        };
        dst.copy_from_slice(src);
        true
    }));

    const PACK_KEY: &str = "test-tone-external.signalpack";
    signal_sampler::pack_registry::install_external(PACK_KEY, ID, len)
        .expect("install external pack");

    let program = LaneProgram {
        name: "External Test".into(),
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
    let renderer = ProjectRenderer::new(rig.daw(), rig.project_guid(), SR);
    renderer.connect_live_midi(64);

    let mut heard = 0.0f32;
    for _ in 0..600 {
        // The worklet's note-on path: warm (synchronous decode through the
        // external reader when cold), then dispatch.
        rig.warm_note(60, 100);
        rig.note_on(60, 100);
        let block = renderer.render_block(0, 512);
        let rms =
            (block.samples.iter().map(|s| s * s).sum::<f32>() / block.samples.len() as f32).sqrt();
        heard = heard.max(rms);
        if heard > 1e-3 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        heard > 1e-3,
        "headless keys rig should render audible output from an EXTERNAL pack, rms={heard}"
    );

    let _ = std::fs::remove_dir_all(&dir);
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
        let rms =
            (block.samples.iter().map(|s| s * s).sum::<f32>() / block.samples.len() as f32).sqrt();
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
