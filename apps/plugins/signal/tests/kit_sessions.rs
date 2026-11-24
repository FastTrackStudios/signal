//! The drum kit in the per-track mode, through the bundled CLAP and daw's
//! plugin host (what Studio's engine loads plugins with): one instance per
//! track, one session. A trigger's instance keys the gate in another's,
//! and the block parameters are the host's, named by phase and block and
//! printed in the blocks' own words.
//!
//! Needs the bundle (`cargo run -p fts-plugin-xtask -- bundle
//! signal-plugin --release --features dev`, or `SIGNAL_CLAP=<the .clap>`):
//!
//! ```sh
//! cargo test -p signal-plugin --test kit_sessions -- --ignored --nocapture
//! ```

use std::path::PathBuf;

use daw::plugin::{PluginEvents, PluginInstance, load_plugin};
use signal_core::kit::{KitPieces, drum_kit};
use signal_core::session::{PortName, SessionId, SessionLink};
use signal_core::state::{Instance, Preset, SignalState};
use signal_plugin::plugin::clap_state;

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

/// An instance on track `port` of the kit, in session `session`.
fn instance(preset: &Preset, session: &str, port: &str) -> Box<dyn PluginInstance> {
    let path = bundle();
    assert!(
        path.exists(),
        "no bundle at {} (bundle it first)",
        path.display()
    );
    let mut p = load_plugin(&path.to_string_lossy())
        .expect("loads")
        .expect("a CLAP");
    let state = SignalState {
        instance: Instance::Port(SessionLink {
            session: SessionId(session.into()),
            port: PortName(port.into()),
        }),
        ..SignalState::new(preset.clone())
    };
    p.load_state(&clap_state(&state)).expect("takes the state");
    p.prepare(SR, BLOCK as u32).expect("activates");
    p
}

/// Deterministic noise.
fn noise(n: usize, level: f32, seed: u32) -> Vec<f32> {
    let mut x = seed;
    (0..n)
        .map(|_| {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            level * ((x >> 8) as f32 / 8_388_608.0 - 1.0)
        })
        .collect()
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0, |m, s| m.max(s.abs()))
}

#[test]
#[ignore = "needs the bundled .clap"]
fn a_trigger_on_one_track_keys_the_gate_on_another() {
    let preset = drum_kit(&KitPieces::default());
    let session = format!("kit-test-{}", std::process::id());
    let mut kick = instance(&preset, &session, "Drums/Kick");
    let mut trig = instance(&preset, &session, "Drums/Kick/Sum/Trig");
    println!(
        "loaded {} twice from {}",
        kick.descriptor().name,
        bundle().display()
    );

    // Its parameters: by phase and block, in the blocks' own words.
    let params = kick.params();
    let names: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
    println!(
        "Drums/Kick: {} params, e.g. {:?}",
        names.len(),
        &names[..names.len().min(16)]
    );
    for want in [
        "Rescue · Gate · Threshold",
        "Tone · Comp · Attack",
        "Tone · Comp · Ratio",
        "Tone · Sat · Drive",
    ] {
        assert!(names.contains(&want), "no {want}: {names:?}");
    }
    let attack = params
        .iter()
        .find(|p| p.name == "Tone · Comp · Attack")
        .expect("attack");
    let now = kick.param_value(attack.id).expect("a value");
    let text = kick.value_to_text(attack.id, now).expect("text");
    println!("Tone · Comp · Attack = {text}");
    assert!(text.contains("ms"), "{text}");
    let back = kick
        .text_to_value(attack.id, &text)
        .expect("parses what it prints");
    assert!((back - now).abs() < 1e-3, "{text}: {now} → {back}");
    let ratio = params
        .iter()
        .find(|p| p.name == "Tone · Comp · Ratio")
        .expect("ratio");
    let r = kick.param_value(ratio.id).unwrap_or(0.0);
    println!(
        "Tone · Comp · Ratio = {:?}",
        kick.value_to_text(ratio.id, r)
    );

    // The kick's track hears a host block and the far trigger's lookback
    // late: it says so, for the host's delay compensation.
    let latency = kick.latency() as usize;
    println!(
        "latency: Drums/Kick {latency}, Drums/Kick/Sum/Trig {}",
        trig.latency()
    );
    assert!(latency >= BLOCK + 96, "{latency}");

    // Loud bleed on the kick's track throughout; its trigger hits at 1 s
    // and 2 s. The kick's instance runs first in every block.
    let frames = 48_000 * 3;
    let bleed = noise(frames, 0.3, 1);
    let mut hits = vec![0.0_f32; frames];
    for at in [48_000, 96_000] {
        for (i, s) in hits.iter_mut().enumerate().skip(at).take(400) {
            *s = 0.9 * (-((i - at) as f32) / 60.0).exp() * if i % 2 == 0 { 1.0 } else { -1.0 };
        }
    }
    let mut out = vec![0.0_f32; frames];
    let mut trig_out = vec![0.0_f32; frames];
    let mut scratch = vec![0.0_f32; BLOCK];
    for start in (0..frames).step_by(BLOCK) {
        let end = (start + BLOCK).min(frames);
        let n = end - start;
        kick.process_block(
            &bleed[start..end],
            &bleed[start..end],
            &mut out[start..end],
            &mut scratch[..n],
            &PluginEvents::EMPTY,
        )
        .expect("kick processes");
        trig.process_block(
            &hits[start..end],
            &hits[start..end],
            &mut trig_out[start..end],
            &mut scratch[..n],
            &PluginEvents::EMPTY,
        )
        .expect("trigger processes");
    }
    let shut = peak(&out[24_000 + latency..44_000]);
    let open = peak(&out[48_000 + latency + 2_000..48_000 + latency + 8_000]);
    let shut_again = peak(&out[48_000 + latency + 30_000..96_000]);
    println!(
        "kick out: before the hit {shut:.4}, on it {open:.4}, after {shut_again:.4}; trigger track out {:.4}",
        peak(&trig_out)
    );
    assert!(
        open > 0.05,
        "the gate opens on the other track's hit: {open}"
    );
    assert!(
        shut < open * 0.1,
        "and is shut on the bleed before it: {shut}"
    );
    assert!(shut_again < open * 0.1, "and after: {shut_again}");
    assert!(peak(&trig_out) < 1e-6, "the trigger track is heard nowhere");
}

/// What a host does with a state (Studio's FX_STATE_SET): into an instance
/// that's already running, then its parameter info read at once, before
/// any process call, the transport stopped. The block slots are there by
/// name, value and text; a slot set takes, and is in the state it saves.
#[test]
#[ignore = "needs the bundled .clap"]
fn a_state_loaded_while_active_shows_its_slots_at_once() {
    let preset = drum_kit(&KitPieces::default());
    let path = bundle();
    let state = |port: &str| {
        clap_state(&SignalState {
            instance: Instance::Port(SessionLink {
                session: SessionId(format!("slots-{}", std::process::id())),
                port: PortName(port.into()),
            }),
            ..SignalState::new(preset.clone())
        })
    };
    // Active first, a fresh instance; then the state.
    let mut p = load_plugin(&path.to_string_lossy())
        .expect("loads")
        .expect("a CLAP");
    p.prepare(SR, BLOCK as u32).expect("activates");
    p.load_state(&state("Drums/Kick/Sum/In"))
        .expect("takes the state");
    let params = p.params();
    let named: Vec<&str> = params
        .iter()
        .map(|i| i.name.as_str())
        .filter(|n| n.contains(" · "))
        .collect();
    println!(
        "Drums/Kick/Sum/In, loaded while active: {} block params, {:?}…",
        named.len(),
        &named[..named.len().min(6)]
    );
    assert!(named.contains(&"Rescue · EQ · Low Cut"), "{named:?}");
    assert!(named.contains(&"Tone · Comp · Ratio"), "{named:?}");
    let ratio = params
        .iter()
        .find(|i| i.name == "Tone · Comp · Ratio")
        .expect("ratio")
        .id;
    let now = p.param_value(ratio).expect("a value");
    assert_eq!(
        p.value_to_text(ratio, now).as_deref(),
        Some("3.0"),
        "the state's ratio"
    );

    // Set it, as FX_SET does: it takes, and the saved state has it.
    let to = p.text_to_value(ratio, "6").expect("parses");
    p.process_block(
        &[0.0; BLOCK],
        &[0.0; BLOCK],
        &mut [0.0; BLOCK],
        &mut [0.0; BLOCK],
        &PluginEvents {
            params: &[(ratio, to)],
            ..PluginEvents::EMPTY
        },
    )
    .expect("processes");
    let set = p.param_value(ratio).unwrap_or(0.0);
    assert_eq!(p.value_to_text(ratio, set).as_deref(), Some("6.0"));
    let saved = p.save_state().expect("saves");
    let len = u64::from_le_bytes(saved[..8].try_into().expect("8")) as usize;
    let json: serde_json::Value = serde_json::from_slice(&saved[8..8 + len]).expect("json");
    let inner: SignalState = SignalState::from_blob(
        serde_json::from_str::<String>(json["fields"]["signal-state"].as_str().expect("field"))
            .expect("string")
            .as_bytes(),
    )
    .expect("a Signal state");
    let comp = inner
        .preset
        .lanes
        .iter()
        .find(|l| l.name == "Drums/Kick/Sum/In")
        .and_then(|l| l.blocks.iter().find(|b| b.id == "comp"))
        .expect("the comp");
    assert!(
        (comp.params["ratio"] - 6.0).abs() < 1e-3,
        "{:?}",
        comp.params
    );

    // Before activation: the same, from a provisional build.
    let mut q = load_plugin(&path.to_string_lossy())
        .expect("loads")
        .expect("a CLAP");
    q.load_state(&state("Drums/Toms/Tom 1"))
        .expect("takes the state");
    let names: Vec<String> = q.params().into_iter().map(|i| i.name).collect();
    assert!(
        names.iter().any(|n| n == "Rescue · Gate · Threshold"),
        "{names:?}"
    );
}

/// A host offering `host-actions/1`, recording what Signal asks of it.
#[derive(Default)]
struct TestHost {
    runs: std::sync::Mutex<Vec<(String, String)>>,
    topics: std::sync::Mutex<Vec<String>>,
}

impl daw_clap_ext::host::HostActions for TestHost {
    fn list(&self) -> Result<String, daw_clap_ext::ExtError> {
        Ok("[]".into())
    }
    fn describe(&self, id: &str) -> Result<String, daw_clap_ext::ExtError> {
        Err(daw_clap_ext::ExtError::NotFound(id.into()))
    }
    fn run(&self, id: &str, args_json: &str) -> Result<String, daw_clap_ext::ExtError> {
        self.runs
            .lock()
            .unwrap()
            .push((id.into(), args_json.into()));
        Ok("{}".into())
    }
    fn subscribe(&self, topic: &str, _sink: daw_clap_ext::host::EventSink) -> Option<u64> {
        let mut t = self.topics.lock().unwrap();
        t.push(topic.into());
        Some(t.len() as u64)
    }
}

fn note_on(offset: u32, note: u8, velocity: u8) -> daw::plugin::PluginMidiEvent {
    daw::plugin::PluginMidiEvent {
        offset,
        message: daw_proto::MidiEvent::decode(&[0x90, note, velocity])
            .expect("a note-on")
            .0,
    }
}

/// signal/1 through daw's host, two instances of one session: the chain
/// as `signal.chain/1` has it, a SIGNAL_SET moving its revision and the
/// host's rescan (and telling the host), meters that read while it
/// plays, and a note-on on the Sample track playing the kick and opening
/// the kick's gate on its other track, keyed by MIDI.
#[test]
#[ignore = "needs the bundled .clap (and SIGNAL_TEST_DRUMS, an MM2 kick, for the sample)"]
fn signal_1_shows_the_chain_and_midi_plays_and_keys_the_kit() {
    let engine = std::env::var("SIGNAL_TEST_DRUMS").ok();
    let preset = drum_kit(&KitPieces {
        kick: engine.as_deref().map(signal_core::host::AssetRef::path),
        ..KitPieces::default()
    });
    let session = format!("chain-{}", std::process::id());
    let host = std::sync::Arc::new(TestHost::default());
    let load = |port: &str| {
        let options = daw::plugin::LoadOptions {
            actions: Some(host.clone()),
        };
        let mut p = daw::plugin::load_plugin_with(&bundle().to_string_lossy(), &options)
            .expect("loads")
            .expect("a CLAP");
        let state = SignalState {
            instance: Instance::Port(SessionLink {
                session: SessionId(session.clone()),
                port: PortName(port.into()),
            }),
            ..SignalState::new(preset.clone())
        };
        p.load_state(&clap_state(&state)).expect("takes the state");
        p.prepare(SR, BLOCK as u32).expect("activates");
        p
    };
    let mut kick = load("Drums/Kick");
    let mut sample = load("Drums/Kick/Sum/Sample");
    let signal = kick.signal().expect("signal/1");

    // The chain, as the contract has it.
    let chain: serde_json::Value =
        serde_json::from_str(&signal.chain().expect("a chain")).expect("JSON");
    assert_eq!(chain["schema"], "signal.chain/1");
    assert_eq!(chain["port"], "Drums/Kick");
    assert_eq!(chain["session"], session.as_str());
    assert_eq!(chain["mode"], "per-track");
    let phases: Vec<&str> = chain["phases"]
        .as_array()
        .expect("phases")
        .iter()
        .map(|p| p["id"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(phases, vec!["rescue", "tone"], "{chain}");
    let gate = &chain["phases"][0]["blocks"][0];
    assert_eq!(gate["id"], "rescue.gate");
    assert_eq!(gate["kind"], "gate");
    assert_eq!(gate["key"]["from"], "Drums/Kick/Sum/Trig");
    let threshold = gate["params"]
        .as_array()
        .expect("params")
        .iter()
        .find(|p| p["id"] == "threshold")
        .expect("threshold");
    assert!(
        threshold["clap_id"]
            .as_str()
            .is_some_and(|c| c.starts_with('b')),
        "{threshold}"
    );
    assert_eq!(threshold["text"], "-40.0 dB");
    let gate_meter = gate["meters"][0]["index"]
        .as_u64()
        .expect("the gate's meter") as u32;
    let comp = &chain["phases"][1]["blocks"][0];
    assert_eq!(comp["id"], "tone.comp");
    let gr_meter = comp["meters"][0]["index"].as_u64().expect("the comp's GR") as u32;
    println!(
        "chain: {} phases, gate key {}, meters gate #{gate_meter} comp GR #{gr_meter}",
        phases.len(),
        gate["key"]
    );
    let sample_chain: serde_json::Value =
        serde_json::from_str(&sample.signal().expect("signal/1").chain().expect("chain"))
            .expect("JSON");
    println!("sample's midi: {}", sample_chain["midi"]);
    assert_eq!(sample_chain["midi"]["plays"][0]["note"], 36);
    assert_eq!(sample_chain["midi"]["publishes_hits"], true);

    // A structural change: the revision moves, the host rescans and hears.
    let (r0, rescans0) = (signal.revision(), kick.param_info_revision());
    let after: serde_json::Value = serde_json::from_str(
        &signal
            .run_action("SIGNAL_SET", r#"{"block": "rescue.gate", "bypass": true}"#)
            .expect("SIGNAL_SET"),
    )
    .expect("JSON");
    assert_eq!(after["phases"][0]["blocks"][0]["bypass"], true);
    assert!(signal.revision() > r0, "the revision moves");
    kick.pump_main_thread();
    assert!(
        kick.param_info_revision() > rescans0,
        "the host was asked to rescan"
    );
    assert!(
        host.runs
            .lock()
            .unwrap()
            .iter()
            .any(|(id, _)| id == "SIGNAL_CHAIN_CHANGED"),
        "the host was told"
    );
    signal
        .run_action("SIGNAL_SET", r#"{"block": "rescue.gate", "bypass": false}"#)
        .expect("back on");
    println!(
        "revision {r0} → {}; host topics {:?}",
        signal.revision(),
        host.topics.lock().unwrap()
    );

    // The session: both ports.
    let s: serde_json::Value = serde_json::from_str(
        &signal
            .run_action("SIGNAL_SESSION", "{}")
            .expect("SIGNAL_SESSION"),
    )
    .expect("JSON");
    let ports: Vec<&str> = s["ports"]
        .as_array()
        .expect("ports")
        .iter()
        .map(|p| p["port"].as_str().unwrap_or(""))
        .collect();
    assert!(
        ports.contains(&"Drums/Kick") && ports.contains(&"Drums/Kick/Sum/Sample"),
        "{ports:?}"
    );

    // Bleed on the kick's track; a note-on (kick, 36) on the Sample track
    // at 1 s. The kick's instance runs first.
    let frames = 48_000 * 2;
    let bleed = noise(frames, 0.3, 7);
    let silence = vec![0.0_f32; BLOCK];
    let (mut kick_out, mut sample_out) = (vec![0.0_f32; frames], vec![0.0_f32; frames]);
    let mut scratch = vec![0.0_f32; BLOCK];
    let mut gate_open = 0.0_f64;
    let mut gr = 0.0_f64;
    for start in (0..frames).step_by(BLOCK) {
        let end = (start + BLOCK).min(frames);
        let n = end - start;
        kick.process_block(
            &bleed[start..end],
            &bleed[start..end],
            &mut kick_out[start..end],
            &mut scratch[..n],
            &PluginEvents::EMPTY,
        )
        .expect("kick processes");
        gate_open = gate_open.max(signal.mod_source_value(gate_meter).unwrap_or(0.0));
        gr = gr.max(signal.mod_source_value(gr_meter).unwrap_or(0.0).abs());
        let notes = if start == 48_000 / BLOCK * BLOCK {
            vec![note_on(100, 36, 110)]
        } else {
            vec![]
        };
        sample
            .process_block(
                &silence[..n],
                &silence[..n],
                &mut sample_out[start..end],
                &mut scratch[..n],
                &PluginEvents {
                    midi: &notes,
                    ..PluginEvents::EMPTY
                },
            )
            .expect("sample processes");
    }
    let latency = kick.latency() as usize;
    let note_at = 48_000 / BLOCK * BLOCK + 100;
    let shut = peak(&kick_out[latency + 4_000..note_at]);
    let open = peak(&kick_out[note_at + latency..note_at + latency + 6_000]);
    let played = peak(&sample_out[note_at..]);
    println!(
        "kick out before the note {shut:.4}, after {open:.4}; sample out {played:.4}; meters: gate {gate_open:.2}, GR {gr:.2} dB"
    );
    assert!(
        open > 0.05 && shut < open * 0.1,
        "the note opens the kick's gate on its other track: {shut} → {open}"
    );
    assert!(
        gate_open > 0.5,
        "the gate's meter read it open: {gate_open}"
    );
    let via: serde_json::Value =
        serde_json::from_str(&signal.chain().expect("chain")).expect("JSON");
    assert_eq!(
        via["phases"][0]["blocks"][0]["key"]["via"], "midi",
        "keyed by the note"
    );
    if engine.is_some() {
        assert!(played > 0.05, "the note plays the kick's sample: {played}");
    }
}
