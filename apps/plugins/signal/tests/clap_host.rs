//! The bundled Signal CLAP, loaded and run headlessly through daw's plugin
//! host (what Studio's engine loads plugins with): it loads, it processes,
//! its state takes a Signal preset, and what it plays nulls against
//! signal-core run directly on the same preset.
//!
//! Needs the bundle: `cargo run -p fts-plugin-xtask -- bundle signal-plugin
//! --release` (or `SIGNAL_CLAP=<path to the .clap>`), hence `#[ignore]`:
//!
//! ```sh
//! cargo test -p signal-plugin --test clap_host -- --ignored --nocapture
//! SIGNAL_TEST_NAM=<a .nam> cargo test … # the guitar rig too
//! ```

use std::path::PathBuf;

use daw::plugin::{PluginEvents, PluginInstance, load_plugin};
use signal_core::graph::{self, BuildOptions};
use signal_core::host::{AssetRef, MemoryHost, NoHost};
use signal_core::state::{Preset, SignalState};
use signal_core::{BlockFactory, BuiltinFactory, Processor, rigs};

const SR: f64 = 48_000.0;
const BLOCK: usize = 512;

fn bundle() -> PathBuf {
    std::env::var_os("SIGNAL_CLAP").map_or_else(
        || {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../target/bundled/FTS Signal.clap")
        },
        PathBuf::from,
    )
}

fn load() -> Box<dyn PluginInstance> {
    let path = bundle();
    assert!(
        path.exists(),
        "no bundle at {} (bundle it first)",
        path.display()
    );
    let mut p = load_plugin(&path.to_string_lossy())
        .expect("loads")
        .expect("a CLAP");
    p.prepare(SR, BLOCK as u32).expect("activates");
    p
}

/// The JSON in a nice-plug CLAP state stream: an 8-byte little-endian
/// length, then the JSON (zstd is off in this build).
fn state_json(stream: &[u8]) -> serde_json::Value {
    let (len, body) = stream.split_at(8);
    let len = u64::from_le_bytes(len.try_into().expect("8 bytes")) as usize;
    serde_json::from_slice(&body[..len]).expect("nice-plug state is JSON")
}

/// Put `state` into the plugin and re-activate it: how a DAW restores a
/// project (state, then activation).
fn load_signal_state(p: &mut Box<dyn PluginInstance>, state: &SignalState) {
    restore_signal_state(p, state);
    p.deactivate();
    p.prepare(SR, BLOCK as u32).expect("re-activates");
}

/// Put `state` into the plugin as it runs: its own state with the
/// `signal-state` field replaced (the rest is nice-plug's parameters).
fn restore_signal_state(p: &mut Box<dyn PluginInstance>, state: &SignalState) {
    let mut json = state_json(&p.save_state().expect("saves state"));
    let blob = String::from_utf8(state.to_blob()).expect("utf-8");
    // A persisted field is itself stored as JSON: a String is a quoted one.
    json["fields"]["signal-state"] =
        serde_json::Value::String(serde_json::to_string(&blob).expect("json"));
    let body = serde_json::to_vec(&json).expect("json");
    let mut stream = (body.len() as u64).to_le_bytes().to_vec();
    stream.extend_from_slice(&body);
    p.load_state(&stream).expect("loads state");
}

fn signal(frames: usize) -> Vec<f32> {
    signal_core::null_test::test_signal(frames, SR as f32)
}

/// Run the plugin over `input` (both channels), a block at a time.
fn run_plugin(p: &mut Box<dyn PluginInstance>, input: &[f32]) -> [Vec<f32>; 2] {
    let mut l = vec![0.0; input.len()];
    let mut r = vec![0.0; input.len()];
    for (i, chunk) in input.chunks(BLOCK).enumerate() {
        let at = i * BLOCK;
        let end = at + chunk.len();
        p.process_block(
            chunk,
            chunk,
            &mut l[at..end],
            &mut r[at..end],
            &PluginEvents::EMPTY,
        )
        .expect("processes");
    }
    [l, r]
}

/// signal-core directly, the same preset and blocks.
fn run_core(preset: &Preset, host: &dyn signal_core::SignalHost, input: &[f32]) -> [Vec<f32>; 2] {
    let factories: [&dyn BlockFactory; 1] = [&BuiltinFactory];
    let opts = BuildOptions {
        sample_rate: SR as f32,
        max_block: BLOCK,
        port: None,
        factories: &factories,
        host,
    };
    let (g, report) = graph::build(preset, &opts);
    assert!(report.all_active(), "{report:?}");
    let mut proc = Processor::new();
    let _ = proc.install(g);
    let mut l = input.to_vec();
    let mut r = input.to_vec();
    for start in (0..input.len()).step_by(BLOCK) {
        let end = (start + BLOCK).min(input.len());
        let mut io: Vec<&mut [f32]> = vec![&mut l[start..end], &mut r[start..end]];
        proc.process(&mut io, 2, end - start, &[], &[]);
    }
    [l, r]
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0, |m, s| m.max(s.abs()))
}

fn diff_dbfs(a: &[Vec<f32>; 2], b: &[Vec<f32>; 2]) -> f32 {
    let d = a
        .iter()
        .zip(b)
        .flat_map(|(x, y)| x.iter().zip(y).map(|(p, q)| (p - q).abs()))
        .fold(0.0, f32::max);
    if d > 0.0 {
        20.0 * d.log10()
    } else {
        f32::NEG_INFINITY
    }
}

#[test]
#[ignore = "needs the bundled .clap"]
fn the_clap_loads_and_exposes_signals_parameters() {
    let mut p = load();
    let d = p.descriptor();
    println!(
        "loaded {} {} ({:?}) from {}",
        d.name,
        d.version,
        d.format,
        bundle().display()
    );
    assert_eq!(d.name, "FTS Signal");
    let names: Vec<String> = p.params().into_iter().map(|i| i.name).collect();
    println!("params: {names:?}");
    for want in ["Input Gain", "Output Gain", "Bypass", "Macro 1", "Macro 8"] {
        assert!(names.iter().any(|n| n == want), "no {want}: {names:?}");
    }
}

#[test]
#[ignore = "needs the bundled .clap"]
fn a_fresh_clap_passes_audio_through() {
    let mut p = load();
    let input = signal(BLOCK * 8);
    let [l, r] = run_plugin(&mut p, &input);
    assert_eq!(l, input, "left");
    assert_eq!(r, input, "right");
    println!("passthrough: {} frames bit-exact", input.len());
}

#[test]
#[ignore = "needs the bundled .clap"]
fn the_clap_plays_a_dsp_chain_as_signal_core_does() {
    let mut p = load();
    let state = SignalState::new(rigs::dsp_chain());
    load_signal_state(&mut p, &state);
    let input = signal(48_000);
    let out = run_plugin(&mut p, &input);
    let core = run_core(&state.preset, &NoHost, &input);
    let db = diff_dbfs(&out, &core);
    let changed = out[0]
        .iter()
        .zip(&input)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f32::max);
    println!(
        "dsp chain: peak {:.3}, changed the input by up to {changed:.3}, vs signal-core {db:.1} dBFS",
        peak(&out[0])
    );
    assert!(changed > 0.01, "the chain did nothing");
    assert!(db < -100.0, "the CLAP and the core differ: {db} dBFS");
    // Its state carries the preset back out.
    let saved = state_json(&p.save_state().expect("saves"));
    let field: String =
        serde_json::from_str(saved["fields"]["signal-state"].as_str().expect("the field"))
            .expect("a string");
    let back = SignalState::from_blob(field.as_bytes()).expect("a Signal state");
    assert_eq!(back.preset, state.preset, "state lost the preset");
}

#[test]
#[ignore = "needs the bundled .clap"]
fn a_state_restored_while_playing_takes_effect() {
    let mut p = load();
    let input = signal(BLOCK);
    // Playing the passthrough.
    let [l, _] = run_plugin(&mut p, &input);
    assert_eq!(l, input);
    restore_signal_state(&mut p, &SignalState::new(rigs::dsp_chain()));
    // The rebuild runs in the background; the song keeps playing meanwhile.
    let started = std::time::Instant::now();
    let mut blocks = 0;
    loop {
        let [l, _] = run_plugin(&mut p, &input);
        blocks += 1;
        let changed = l
            .iter()
            .zip(&input)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        if changed > 0.01 {
            println!(
                "restored while playing: the chain took over after {blocks} blocks ({:?})",
                started.elapsed()
            );
            break;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "the restored chain never took over"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
#[ignore = "needs the bundled .clap and SIGNAL_TEST_NAM"]
fn the_clap_plays_the_guitar_rig_as_signal_core_does() {
    let nam = PathBuf::from(std::env::var_os("SIGNAL_TEST_NAM").expect("SIGNAL_TEST_NAM: a .nam"));
    let dir = std::env::temp_dir().join(format!("signal-clap-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let ir: Vec<f32> = (0..512)
        .map(|i| (-(i as f32) / 40.0).exp() * if i % 2 == 0 { 1.0 } else { 0.6 })
        .collect();
    let cab = dir.join("cab.wav");
    std::fs::write(&cab, signal_core::wav::write_mono_f32(&ir, 48_000)).expect("cab");
    let preset = rigs::guitar_rig(
        AssetRef::path(nam.to_string_lossy()),
        AssetRef::path(cab.to_string_lossy()),
    );
    let mut p = load();
    load_signal_state(&mut p, &SignalState::new(preset.clone()));
    let input = signal(48_000);
    let out = run_plugin(&mut p, &input);
    let host = MemoryHost::default()
        .with(nam.to_string_lossy(), std::fs::read(&nam).expect("nam"))
        .with(cab.to_string_lossy(), std::fs::read(&cab).expect("cab"));
    let core = run_core(&preset, &host, &input);
    let db = diff_dbfs(&out, &core);
    println!(
        "guitar rig: peak L {:.3} R {:.3}, vs signal-core {db:.1} dBFS",
        peak(&out[0]),
        peak(&out[1])
    );
    assert!(peak(&out[0]) > 1e-3 && peak(&out[1]) > 1e-3, "silent rig");
    assert!(db < -100.0, "the CLAP and the core differ: {db} dBFS");
    let _ = std::fs::remove_dir_all(&dir);
}
