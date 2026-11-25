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
    sinks: std::sync::Mutex<Vec<(String, daw_clap_ext::host::EventSink)>>,
}

impl daw_clap_ext::host::HostActions for TestHost {
    fn list(&self) -> Result<String, daw_clap_ext::ExtError> {
        // A host that takes the chain event (Studio polls instead).
        Ok(r#"[{"id": "SIGNAL_CHAIN_CHANGED", "description": "a chain changed"}]"#.into())
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
    fn subscribe(&self, topic: &str, sink: daw_clap_ext::host::EventSink) -> Option<u64> {
        let mut t = self.topics.lock().unwrap();
        t.push(topic.into());
        self.sinks.lock().unwrap().push((topic.into(), sink));
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
            ..daw::plugin::LoadOptions::default()
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
    // The CLAP parameter id, as the host's params extension reports it.
    let reported = kick
        .params()
        .into_iter()
        .find(|i| i.name == "Rescue · Gate · Threshold")
        .expect("the slot")
        .id;
    assert_eq!(
        threshold["clap_id"].as_str(),
        Some(reported.to_string().as_str()),
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
    // The additions: an algorithm on a block whose family has them, each
    // meter's kind and range, the key's configured source and its fallback,
    // and values_revision moving on a value, not revision.
    let now: serde_json::Value =
        serde_json::from_str(&signal.chain().expect("chain")).expect("JSON");
    let eq = now["phases"][1]["blocks"]
        .as_array()
        .expect("tone")
        .iter()
        .find(|b| b["kind"] == "eq")
        .expect("the tone EQ")
        .clone();
    assert_eq!(eq["algorithm"], "parametric", "{eq}");
    let gate_m = now["phases"][0]["blocks"][0]["meters"][0].clone();
    assert_eq!(
        (
            gate_m["kind"].as_str(),
            gate_m["min"].as_f64(),
            gate_m["max"].as_f64()
        ),
        (Some("state"), Some(0.0), Some(1.0)),
        "{gate_m}"
    );
    let comp_m = now["phases"][1]["blocks"][0]["meters"][0].clone();
    assert_eq!(comp_m["kind"], "reduction", "{comp_m}");
    assert_eq!(now["phases"][1]["meters"][0]["kind"], "level");
    assert!(
        now["phases"][0]["blocks"][0]["key"]["fallback"].is_null(),
        "keyed by notes, not fallen back"
    );
    let (rev, values) = (
        now["revision"].as_u64(),
        now["values_revision"].as_u64().expect("values_revision"),
    );
    let ratio = kick
        .params()
        .into_iter()
        .find(|i| i.name == "Tone · Comp · Ratio")
        .expect("ratio")
        .id;
    let to = kick.text_to_value(ratio, "8").expect("parses");
    let mut out_l = vec![0.0_f32; BLOCK];
    kick.process_block(
        &silence,
        &silence,
        &mut out_l,
        &mut scratch,
        &PluginEvents {
            params: &[(ratio, to)],
            ..PluginEvents::EMPTY
        },
    )
    .expect("processes");
    let moved: serde_json::Value =
        serde_json::from_str(&signal.chain().expect("chain")).expect("JSON");
    assert!(
        moved["values_revision"].as_u64() > Some(values),
        "a value set moves values_revision"
    );
    assert_eq!(moved["revision"].as_u64(), rev, "and not revision");
    println!(
        "additions: eq algorithm {}, meters {} / {}, values_revision {values} → {}",
        eq["algorithm"], gate_m["kind"], comp_m["kind"], moved["values_revision"]
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

/// The kit mode (one instance, every lane): block ids are unique per lane,
/// so a structural action names its lane, and is refused without it.
#[test]
#[ignore = "needs the bundled .clap"]
fn kit_mode_actions_name_their_lane() {
    let mut p = load_plugin(&bundle().to_string_lossy())
        .expect("loads")
        .expect("a CLAP");
    p.load_state(&clap_state(&SignalState::new(drum_kit(
        &KitPieces::default(),
    ))))
    .expect("takes the kit");
    p.prepare(SR, BLOCK as u32).expect("activates");
    let signal = p.signal().expect("signal/1");
    let chain: serde_json::Value =
        serde_json::from_str(&signal.chain().expect("chain")).expect("JSON");
    assert_eq!(chain["mode"], "kit");
    let lanes = chain["lanes"].as_array().expect("lanes");
    println!("kit mode: {} lanes", lanes.len());
    assert!(
        signal
            .run_action("SIGNAL_SET", r#"{"block": "rescue.gate", "bypass": true}"#)
            .is_err(),
        "no lane: refused"
    );
    let after: serde_json::Value = serde_json::from_str(
        &signal
            .run_action(
                "SIGNAL_SET",
                r#"{"lane": "Drums/Snare", "block": "rescue.gate", "bypass": true}"#,
            )
            .expect("with its lane"),
    )
    .expect("JSON");
    let gate = |lane: &str| {
        after["lanes"]
            .as_array()
            .expect("lanes")
            .iter()
            .find(|l| l["lane"] == lane)
            .expect(lane)["phases"][0]["blocks"][0]["bypass"]
            .clone()
    };
    assert_eq!(gate("Drums/Snare"), true, "the snare's gate");
    assert_eq!(
        gate("Drums/Kick"),
        false,
        "and not the kick's, which has the same id"
    );
}

/// Studio's placed hits: Signal subscribes to its pieces' `hits:` topics
/// after init, and a `hits:Drums/Kick` event (the piece's every hit) keys
/// the kick's gate on those frames, with no trigger audio and no notes.
#[test]
#[ignore = "needs the bundled .clap"]
fn the_hosts_placed_hits_key_the_gate() {
    let preset = drum_kit(&KitPieces::default());
    let host = std::sync::Arc::new(TestHost::default());
    let options = daw::plugin::LoadOptions {
        actions: Some(host.clone()),
        ..daw::plugin::LoadOptions::default()
    };
    let mut kick = daw::plugin::load_plugin_with(&bundle().to_string_lossy(), &options)
        .expect("loads")
        .expect("a CLAP");
    let state = SignalState {
        instance: Instance::Port(SessionLink {
            session: SessionId(format!("placed-{}", std::process::id())),
            port: PortName("Drums/Kick".into()),
        }),
        ..SignalState::new(preset)
    };
    kick.load_state(&clap_state(&state))
        .expect("takes the state");
    kick.prepare(SR, BLOCK as u32).expect("activates");
    kick.pump_main_thread();
    let topics = host.topics.lock().unwrap().clone();
    println!("subscribed: {topics:?}");
    if topics.is_empty() {
        // daw's standalone host delivers subscriptions from daw-dev's
        // 12c7193d on; one before it refuses them all.
        println!("this daw host offers no subscriptions: nothing to deliver");
        return;
    }
    assert!(topics.iter().any(|t| t == "hits:Drums/Kick"), "{topics:?}");
    let sinks = host.sinks.lock().unwrap().clone();
    let (sub, (_, sink)) = sinks
        .iter()
        .enumerate()
        .find(|(_, (t, _))| t == "hits:Drums/Kick")
        .expect("its sink");
    sink.send(
        (sub + 1) as u64,
        "hits:Drums/Kick",
        r#"{"piece": "Drums/Kick", "track": "Drums/Kick/Sum/Trig", "note": 36, "count": 1, "hits": [{"time": 1.0, "velocity": 100}]}"#,
    );
    let frames = 48_000 * 2;
    let bleed = noise(frames, 0.3, 11);
    let mut out = vec![0.0_f32; frames];
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
        .expect("processes");
    }
    let latency = kick.latency() as usize;
    let shut = peak(&out[latency + 4_000..48_000]);
    let open = peak(&out[48_000 + latency..48_000 + latency + 6_000]);
    println!("kick out before the placed hit {shut:.4}, on it {open:.4}");
    assert!(
        open > 0.05 && shut < open * 0.1,
        "the placed hit opens the gate: {shut} → {open}"
    );
}

/// The tom's peak through `(from, to)` seconds of `out`, `latency` late.
fn peak_s(out: &[f32], latency: usize, (from, to): (f64, f64)) -> f32 {
    let at = |s: f64| ((s * SR) as usize + latency).min(out.len());
    peak(&out[at(from)..at(to)])
}

/// A tom keyed by its trigger on another track, played by `tom` (and the
/// trigger's instance, `trig`, silent: no trigger audio, no notes) over
/// 17 s of loud bleed; the piece's every hit placed at 1 s and 16 s. The
/// tom's output.
fn tom_through_a_gap(
    tom: &mut Box<dyn PluginInstance>,
    trig: &mut Box<dyn PluginInstance>,
) -> Vec<f32> {
    tom_and_trigger(tom, trig, &vec![0.0; 48_000 * 17])
}

/// [`tom_through_a_gap`], the trigger's track playing `trigger` (17 s).
fn tom_and_trigger(
    tom: &mut Box<dyn PluginInstance>,
    trig: &mut Box<dyn PluginInstance>,
    trigger: &[f32],
) -> Vec<f32> {
    let frames = 48_000 * 17;
    let bleed = noise(frames, 0.3, 23);
    let mut out = vec![0.0_f32; frames];
    let mut trig_out = vec![0.0_f32; BLOCK];
    let mut scratch = vec![0.0_f32; BLOCK];
    for start in (0..frames).step_by(BLOCK) {
        let end = (start + BLOCK).min(frames);
        let n = end - start;
        tom.process_block(
            &bleed[start..end],
            &bleed[start..end],
            &mut out[start..end],
            &mut scratch[..n],
            &PluginEvents::EMPTY,
        )
        .expect("processes");
        trig.process_block(
            &trigger[start..end],
            &trigger[start..end],
            &mut trig_out[..n],
            &mut scratch[..n],
            &PluginEvents::EMPTY,
        )
        .expect("processes");
    }
    out
}

/// Studio's `hits:` event for Tom 4: hits at 1 s and 16 s.
const TOM_4_HITS: &str = r#"{"piece": "Tom 4", "track": "Drums/Toms/Tom 4", "note": 50, "count": 2, "hits": [{"time": 1.0, "velocity": 100}, {"time": 16.0, "velocity": 100}]}"#;

/// Two instances of one session, as Studio runs a kit per track: the tom
/// folder's gate keyed by the tom's trigger on its own track, and Studio's
/// `hits:` event for the piece (its every hit). Through a 15 s gap between
/// the placed hits the gate stays shut: the song's hits are all known, so
/// it never falls back to its own input and opens on the bleed. It opens
/// on each placed hit. The key says it is keyed by placed hits, not by
/// itself.
///
/// A copy of the tom's instance (its state saved and loaded into a fresh
/// one, as Studio's offline renders copy every plugin) is subscribed to
/// nothing: handed the same event by its host, it plays the same.
#[test]
#[ignore = "needs the bundled .clap"]
fn a_gate_keyed_from_another_track_waits_through_a_gap_of_placed_hits() {
    let preset = drum_kit(&KitPieces::default());
    let host = std::sync::Arc::new(TestHost::default());
    let session = format!("tom-gap-{}", std::process::id());
    let load = |port: &str, session: &str, state: Option<&[u8]>| {
        let options = daw::plugin::LoadOptions {
            actions: Some(host.clone()),
            ..daw::plugin::LoadOptions::default()
        };
        let mut p = daw::plugin::load_plugin_with(&bundle().to_string_lossy(), &options)
            .expect("loads")
            .expect("a CLAP");
        let fresh = clap_state(&SignalState {
            instance: Instance::Port(SessionLink {
                session: SessionId(session.into()),
                port: PortName(port.into()),
            }),
            ..SignalState::new(preset.clone())
        });
        p.load_state(state.unwrap_or(&fresh))
            .expect("takes the state");
        p.prepare(SR, BLOCK as u32).expect("activates");
        p.pump_main_thread();
        p
    };
    let mut tom = load("Drums/Toms/Tom 4", &session, None);
    let mut trig = load("Drums/Toms/Tom 4/T4 Trig", &session, None);
    let topics = host.topics.lock().unwrap().clone();
    if topics.is_empty() {
        println!("this daw host offers no subscriptions: nothing to deliver");
        return;
    }
    assert!(
        topics.iter().any(|t| t == "hits:Drums/Toms/Tom 4"),
        "the tom follows its piece's hits: {topics:?}"
    );
    let sinks = host.sinks.lock().unwrap().clone();
    for (sub, (topic, sink)) in sinks.iter().enumerate() {
        if topic == "hits:Drums/Toms/Tom 4" {
            sink.send((sub + 1) as u64, topic, TOM_4_HITS);
        }
    }
    let out = tom_through_a_gap(&mut tom, &mut trig);
    let latency = tom.latency() as usize;
    let first = peak_s(&out, latency, (1.0, 1.1));
    let gap = peak_s(&out, latency, (12.0, 15.9));
    let second = peak_s(&out, latency, (16.0, 16.1));
    println!("tom 4: first hit {first:.4}, the gap's 12–15.9 s {gap:.5}, second hit {second:.4}");
    assert!(first > 0.05, "the first placed hit opens the gate: {first}");
    assert!(second > 0.05, "the second placed hit opens it: {second}");
    assert!(
        gap < first * 0.05,
        "the gate stays shut through the gap ({gap}), not open on the bleed"
    );
    let chain: serde_json::Value =
        serde_json::from_str(&tom.signal().expect("signal/1").chain().expect("chain"))
            .expect("JSON");
    let key = &chain["phases"][0]["blocks"][0]["key"];
    println!("the tom's key in the gap: {key}");
    assert_eq!(key["via"], "placed", "keyed by the placed hits: {key}");

    // A render's copies: the tom's state in fresh instances of a session of
    // their own, subscribed to nothing; the host hands the copy what it
    // handed the instance.
    let copies = format!("{session}~render");
    let fresh = |port| {
        let options = daw::plugin::LoadOptions::default();
        let mut p = daw::plugin::load_plugin_with(&bundle().to_string_lossy(), &options)
            .expect("loads")
            .expect("a CLAP");
        p.load_state(&clap_state(&SignalState {
            instance: Instance::Port(SessionLink {
                session: SessionId(copies.clone()),
                port: PortName(port),
            }),
            ..SignalState::new(preset.clone())
        }))
        .expect("takes the state");
        p
    };
    let mut tom_copy = fresh("Drums/Toms/Tom 4".into());
    let mut trig_copy = fresh("Drums/Toms/Tom 4/T4 Trig".into());
    tom_copy
        .event_sink()
        .expect("host-events/1")
        .send(0, "hits:Drums/Toms/Tom 4", TOM_4_HITS);
    tom_copy.prepare(SR, BLOCK as u32).expect("activates");
    trig_copy.prepare(SR, BLOCK as u32).expect("activates");
    let copied = tom_through_a_gap(&mut tom_copy, &mut trig_copy);
    let gap = peak_s(&copied, latency, (12.0, 15.9));
    let second = peak_s(&copied, latency, (16.0, 16.1));
    println!("tom 4's copy: the gap {gap:.5}, second hit {second:.4}");
    assert!(
        second > 0.05 && gap < second * 0.05,
        "the copy plays as the instance: gap {gap}, hit {second}"
    );
    // Handed nothing (Studio's renders before they replayed the event), a
    // copy knows no hit and falls back to its own input: open on the
    // bleed. What a whole-song render heard while playback stayed shut.
    let mut bare = fresh("Drums/Toms/Tom 4".into());
    let mut bare_trig = fresh("Drums/Toms/Tom 4/T4 Trig".into());
    bare.prepare(SR, BLOCK as u32).expect("activates");
    bare_trig.prepare(SR, BLOCK as u32).expect("activates");
    let open = peak_s(
        &tom_through_a_gap(&mut bare, &mut bare_trig),
        latency,
        (12.0, 15.9),
    );
    println!("tom 4's copy handed nothing: the gap {open:.4}");
    assert!(open > 0.05, "a copy with no placed hits fails open: {open}");
}

/// Studio's event for a piece whose every hit it holds for bleed (Tom 4 on
/// Unbreakable: two auto-detected hits flagged `bleed`, so no note): a
/// piece of the kit, `count: 0`, no hits. Its hits are known — there are
/// none — so the tom's gate stays shut through the whole song's bleed and
/// never falls back to its own input; the key says `placed`. A hit its
/// trigger's audio detects still opens it.
#[test]
#[ignore = "needs the bundled .clap"]
fn a_piece_with_no_placed_hit_keeps_its_gate_shut() {
    const NONE: &str =
        r#"{"piece": "Tom 4", "track": "Drums/Toms/Tom 4", "note": 50, "count": 0, "hits": []}"#;
    let preset = drum_kit(&KitPieces::default());
    let host = std::sync::Arc::new(TestHost::default());
    let run = |tag: &str, trigger: &[f32]| {
        let session = format!("tom-none-{tag}-{}", std::process::id());
        let load = |port: &str| {
            let options = daw::plugin::LoadOptions {
                actions: Some(host.clone()),
                ..daw::plugin::LoadOptions::default()
            };
            let mut p = daw::plugin::load_plugin_with(&bundle().to_string_lossy(), &options)
                .expect("loads")
                .expect("a CLAP");
            p.load_state(&clap_state(&SignalState {
                instance: Instance::Port(SessionLink {
                    session: SessionId(session.clone()),
                    port: PortName(port.into()),
                }),
                ..SignalState::new(preset.clone())
            }))
            .expect("takes the state");
            p.prepare(SR, BLOCK as u32).expect("activates");
            p.pump_main_thread();
            p
        };
        let mut tom = load("Drums/Toms/Tom 4");
        let mut trig = load("Drums/Toms/Tom 4/T4 Trig");
        tom.event_sink()
            .expect("host-events/1")
            .send(1, "hits:Drums/Toms/Tom 4", NONE);
        let out = tom_and_trigger(&mut tom, &mut trig, trigger);
        let chain: serde_json::Value =
            serde_json::from_str(&tom.signal().expect("signal/1").chain().expect("chain"))
                .expect("JSON");
        let key = chain["phases"][0]["blocks"][0]["key"].clone();
        (out, tom.latency() as usize, key)
    };
    let (out, latency, key) = run("quiet", &vec![0.0; 48_000 * 17]);
    let song = peak_s(&out, latency, (0.5, 16.9));
    println!("tom 4 with no placed hit: peak {song:.5} over 17 s of bleed; key {key}");
    assert!(song < 0.003, "shut through the bleed: {song}");
    assert_eq!(key["via"], "placed", "{key}");
    assert!(key["fallback"].is_null(), "{key}");

    // The trigger's track hits at 5 s: its audio keys the gate.
    let mut hit = vec![0.0_f32; 48_000 * 17];
    for (i, s) in hit.iter_mut().enumerate().skip(48_000 * 5).take(400) {
        let t = (i - 48_000 * 5) as f32;
        *s = 0.9 * (-t / 60.0).exp() * if i % 2 == 0 { 1.0 } else { -1.0 };
    }
    let (out, latency, _) = run("struck", &hit);
    let open = peak_s(&out, latency, (5.0, 5.3));
    let before = peak_s(&out, latency, (0.5, 4.9));
    println!("tom 4 struck at 5 s: before {before:.5}, on it {open:.4}");
    assert!(
        open > 0.05 && before < 0.003,
        "the trigger opens it: {before} → {open}"
    );
}

/// A host that forgets every subscription made before `forget` (as
/// Studio's pump drops those of a plugin it does not run yet: FX_RELOAD
/// loads and restores the new instance before putting it on the chain).
#[derive(Default)]
struct ForgetfulHost {
    live: std::sync::Mutex<Vec<(u64, String, daw_clap_ext::host::EventSink)>>,
    next: std::sync::atomic::AtomicU64,
}

impl ForgetfulHost {
    fn forget(&self) {
        self.live.lock().unwrap().clear();
    }
    fn send(&self, topic: &str, event: &str) -> usize {
        let live = self.live.lock().unwrap().clone();
        let mut sent = 0;
        for (id, t, sink) in live.iter().filter(|(_, t, _)| t == topic) {
            sink.send(*id, t, event);
            sent += 1;
        }
        sent
    }
}

impl daw_clap_ext::host::HostActions for ForgetfulHost {
    fn list(&self) -> Result<String, daw_clap_ext::ExtError> {
        Ok("[]".into())
    }
    fn describe(&self, id: &str) -> Result<String, daw_clap_ext::ExtError> {
        Err(daw_clap_ext::ExtError::NotFound(id.into()))
    }
    fn run(&self, id: &str, _: &str) -> Result<String, daw_clap_ext::ExtError> {
        Err(daw_clap_ext::ExtError::NotFound(id.into()))
    }
    fn subscribe(&self, topic: &str, sink: daw_clap_ext::host::EventSink) -> Option<u64> {
        let id = self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        self.live.lock().unwrap().push((id, topic.into(), sink));
        Some(id)
    }
    fn unsubscribe(&self, subscription: u64) {
        self.live
            .lock()
            .unwrap()
            .retain(|(id, _, _)| *id != subscription);
    }
}

/// FX_RELOAD's race: the new instance subscribes while its state loads,
/// before the host runs it, and the host forgets those subscriptions.
/// Activated, it subscribes again, so the piece's placed hits reach it:
/// Tom 4's gate shut through a 15 s gap, open on each placed hit.
#[test]
#[ignore = "needs the bundled .clap"]
fn an_instance_the_host_forgot_subscribes_again_when_activated() {
    let preset = drum_kit(&KitPieces::default());
    let host = std::sync::Arc::new(ForgetfulHost::default());
    let session = format!("tom-forgot-{}", std::process::id());
    let load = |port: &str| {
        let options = daw::plugin::LoadOptions {
            actions: Some(host.clone()),
            ..daw::plugin::LoadOptions::default()
        };
        let mut p = daw::plugin::load_plugin_with(&bundle().to_string_lossy(), &options)
            .expect("loads")
            .expect("a CLAP");
        p.load_state(&clap_state(&SignalState {
            instance: Instance::Port(SessionLink {
                session: SessionId(session.clone()),
                port: PortName(port.into()),
            }),
            ..SignalState::new(preset.clone())
        }))
        .expect("takes the state");
        p
    };
    let mut tom = load("Drums/Toms/Tom 4");
    let mut trig = load("Drums/Toms/Tom 4/T4 Trig");
    // Not yet the host's: what it subscribed to is forgotten.
    host.forget();
    tom.prepare(SR, BLOCK as u32).expect("activates");
    trig.prepare(SR, BLOCK as u32).expect("activates");
    tom.pump_main_thread();
    let sent = host.send("hits:Drums/Toms/Tom 4", TOM_4_HITS);
    println!("subscribed after activation: {sent} to hits:Drums/Toms/Tom 4");
    assert!(
        sent > 0,
        "activated, the tom subscribes to its piece's hits again"
    );
    let out = tom_through_a_gap(&mut tom, &mut trig);
    let latency = tom.latency() as usize;
    let gap = peak_s(&out, latency, (12.0, 15.9));
    let second = peak_s(&out, latency, (16.0, 16.1));
    println!("tom 4 after the host forgot: gap {gap:.5}, second hit {second:.4}");
    assert!(
        second > 0.05 && gap < second * 0.05,
        "shut through the gap, open on the hit: {gap} / {second}"
    );
}

/// An OHs instance with `preset`, on a host offering `host-actions/1`.
fn overheads(
    preset: Preset,
    host: &std::sync::Arc<TestHost>,
    tag: &str,
) -> Box<dyn PluginInstance> {
    let options = daw::plugin::LoadOptions {
        actions: Some(host.clone()),
        ..daw::plugin::LoadOptions::default()
    };
    let mut ohs = daw::plugin::load_plugin_with(&bundle().to_string_lossy(), &options)
        .expect("loads")
        .expect("a CLAP");
    let state = SignalState {
        instance: Instance::Port(SessionLink {
            session: SessionId(format!("{tag}-{}", std::process::id())),
            port: PortName("Drums/Cymbals/OHs".into()),
        }),
        ..SignalState::new(preset)
    };
    ohs.load_state(&clap_state(&state))
        .expect("takes the state");
    ohs.prepare(SR, BLOCK as u32).expect("activates");
    ohs.pump_main_thread();
    ohs
}

/// A keyed dynamic EQ added over signal/1: SIGNAL_BLOCK_ADD with a `key`
/// puts a `dyn_eq` on the overheads keyed by the snare's trigger, the chain
/// shows it keyed with its bands and their meters, and the instance follows
/// the snare's placed hits (`hits:Drums/Snare`).
#[test]
#[ignore = "needs the bundled .clap"]
fn a_dyn_eq_added_over_signal_1_follows_the_snares_hits() {
    let host = std::sync::Arc::new(TestHost::default());
    let mut ohs = overheads(drum_kit(&KitPieces::default()), &host, "dyn-eq-add");
    let signal = ohs.signal().expect("signal/1");
    let chain: serde_json::Value = serde_json::from_str(
        &signal
            .run_action(
                "SIGNAL_BLOCK_ADD",
                r#"{"phase": "tone", "kind": "dyn_eq", "key": "Drums/Snare/Sum/Trig"}"#,
            )
            .expect("SIGNAL_BLOCK_ADD"),
    )
    .expect("JSON");
    let block = chain["phases"]
        .as_array()
        .expect("phases")
        .iter()
        .flat_map(|p| p["blocks"].as_array().cloned().unwrap_or_default())
        .find(|b| b["kind"] == "dyn_eq")
        .unwrap_or_else(|| panic!("a dyn_eq in {chain}"));
    println!(
        "dyn_eq: id {}, key {}, meters {}",
        block["id"], block["key"], block["meters"]
    );
    assert_eq!(block["key"]["from"], "Drums/Snare/Sum/Trig");
    assert_eq!(block["algorithm"], "dynamic");
    let ids: Vec<&str> = block["params"]
        .as_array()
        .expect("params")
        .iter()
        .filter_map(|p| p["id"].as_str())
        .collect();
    for id in ["band.1.range", "band.1.source", "band.1.hold", "velocity"] {
        assert!(ids.contains(&id), "{id} in {ids:?}");
    }
    let meters: Vec<&str> = block["meters"]
        .as_array()
        .expect("meters")
        .iter()
        .filter_map(|m| m["id"].as_str())
        .collect();
    assert_eq!(
        meters,
        ["band1_gain", "band2_gain", "band3_gain", "band4_gain"],
        "its bands' meters"
    );
    assert_eq!(block["meters"][0]["kind"], "gain");
    ohs.pump_main_thread();
    let topics = host.topics.lock().unwrap().clone();
    if topics.is_empty() {
        println!("this daw host offers no subscriptions");
        return;
    }
    assert!(
        topics.iter().any(|t| t == "hits:Drums/Snare"),
        "subscribed to the snare's hits: {topics:?}"
    );
}

/// Colt's overhead trick through the CLAP: a `dyn_eq` on the overheads,
/// keyed by the snare's trigger on another track, cuts 450 Hz by 4 dB on
/// the snare's placed hit and only there.
#[test]
#[ignore = "needs the bundled .clap"]
fn the_snares_placed_hits_cut_the_overheads() {
    let mut preset = drum_kit(&KitPieces::default());
    let lane = preset
        .lanes
        .iter_mut()
        .find(|l| l.name == "Drums/Cymbals/OHs")
        .expect("the overheads");
    lane.blocks.push(
        signal_core::state::BlockSpec::signal("snare_eq", "dyn_eq")
            .key("Drums/Snare/Sum/Trig")
            .param("band.1.used", 1.0)
            .param("band.1.freq", 450.0)
            .param("band.1.range", -4.0)
            .param("band.1.source", 2.0)
            .param("band.1.hold", 80.0)
            .param("band.1.release", 60.0)
            .phase(signal_core::state::Phase::Tone),
    );
    let host = std::sync::Arc::new(TestHost::default());
    let mut ohs = overheads(preset, &host, "dyn-eq-hits");
    let sinks = host.sinks.lock().unwrap().clone();
    let Some((sub, (topic, sink))) = sinks
        .iter()
        .enumerate()
        .find(|(_, (t, _))| t == "hits:Drums/Snare")
    else {
        println!(
            "this daw host offers no subscriptions: {sinks:?}",
            sinks = host.topics.lock().unwrap()
        );
        return;
    };
    sink.send(
        (sub + 1) as u64,
        topic,
        r#"{"piece": "Drums/Snare", "track": "Drums/Snare/Sum/Trig", "note": 38, "count": 1, "hits": [{"time": 1.0, "velocity": 100}]}"#,
    );
    let frames = 48_000 * 2;
    let tone: Vec<f32> = (0..frames)
        .map(|i| (i as f32 * 450.0 * std::f32::consts::TAU / SR as f32).sin() * 0.25)
        .collect();
    let mut out = vec![0.0_f32; frames];
    let mut scratch = vec![0.0_f32; BLOCK];
    for start in (0..frames).step_by(BLOCK) {
        let end = (start + BLOCK).min(frames);
        let n = end - start;
        ohs.process_block(
            &tone[start..end],
            &tone[start..end],
            &mut out[start..end],
            &mut scratch[..n],
            &PluginEvents::EMPTY,
        )
        .expect("processes");
    }
    let at = 48_000 + ohs.latency() as usize;
    let before = peak(&out[at - 12_000..at - 500]);
    let on = peak(&out[at + 960..at + 2_880]);
    let cut = 20.0 * (on / before).log10();
    println!("overheads at 450 Hz: {before:.4} before the snare, {on:.4} on it ({cut:.2} dB)");
    assert!(
        (cut + 4.0).abs() < 0.6,
        "−4 dB on the snare's hit: {cut:.2} dB"
    );
}

/// The snare's Non-Linear through the CLAP, on its own track: fed the
/// snare's sum with bleed (its own detector never fires on it) and keyed by
/// the snare's placed hits (`hits:Drums/Snare`), it bursts on the hit.
#[test]
#[ignore = "needs the bundled .clap"]
fn the_snares_nonlinear_bursts_on_its_placed_hit() {
    let mut preset = drum_kit(&KitPieces::default());
    let lane = preset
        .lanes
        .iter_mut()
        .find(|l| l.name == "Drums/Snare/Verb/Nonlin")
        .expect("the snare's Non-Linear");
    lane.mute = false;
    lane.gain_db = 0.0;
    let verb = lane
        .blocks
        .iter_mut()
        .find(|b| b.id == "verb")
        .expect("its verb");
    *verb = verb.clone().param("decay", 0.08).param("extra_a", 0.3);
    let host = std::sync::Arc::new(TestHost::default());
    let options = daw::plugin::LoadOptions {
        actions: Some(host.clone()),
        ..daw::plugin::LoadOptions::default()
    };
    let mut nonlin = daw::plugin::load_plugin_with(&bundle().to_string_lossy(), &options)
        .expect("loads")
        .expect("a CLAP");
    let state = SignalState {
        instance: Instance::Port(SessionLink {
            session: SessionId(format!("nonlin-{}", std::process::id())),
            port: PortName("Drums/Snare/Verb/Nonlin".into()),
        }),
        ..SignalState::new(preset)
    };
    nonlin
        .load_state(&clap_state(&state))
        .expect("takes the state");
    nonlin.prepare(SR, BLOCK as u32).expect("activates");
    nonlin.pump_main_thread();
    let sinks = host.sinks.lock().unwrap().clone();
    let Some((sub, (topic, sink))) = sinks
        .iter()
        .enumerate()
        .find(|(_, (t, _))| t == "hits:Drums/Snare")
    else {
        println!(
            "no hits:Drums/Snare subscription: {:?}",
            host.topics.lock().unwrap()
        );
        return;
    };
    sink.send(
        (sub + 1) as u64,
        topic,
        r#"{"piece": "Drums/Snare", "track": "Drums/Snare/Sum/Trig", "note": 38, "count": 1, "hits": [{"time": 1.0, "velocity": 100}]}"#,
    );
    let frames = 48_000 * 2;
    let mut sum = noise(frames, 0.3, 41);
    for (i, s) in sum.iter_mut().enumerate().skip(48_000).take(4_800) {
        let t = (i - 48_000) as f32;
        *s += 0.6 * (-t / 1_200.0).exp() * (t * 200.0 * std::f32::consts::TAU / SR as f32).sin();
    }
    let mut out = vec![0.0_f32; frames];
    let mut scratch = vec![0.0_f32; BLOCK];
    for start in (0..frames).step_by(BLOCK) {
        let end = (start + BLOCK).min(frames);
        let n = end - start;
        nonlin
            .process_block(
                &sum[start..end],
                &sum[start..end],
                &mut out[start..end],
                &mut scratch[..n],
                &PluginEvents::EMPTY,
            )
            .expect("processes");
    }
    let at = 48_000 + nonlin.latency() as usize;
    let before = peak(&out[at - 20_000..at]);
    let burst = peak(&out[at..at + 12_000]);
    let after = peak(&out[at + 12_500..at + 30_000]);
    println!("snare Non-Linear: {before:.5} before, {burst:.4} on the hit, {after:.5} after");
    assert!(
        burst > 0.05 && before < 1e-5 && after < 1e-5,
        "a gated burst on the hit"
    );
}

/// A transient shaper added to the rooms over signal/1, keyed by the snare:
/// the chain shows it keyed, its controls and its gain meter.
#[test]
#[ignore = "needs the bundled .clap"]
fn a_transient_shaper_added_over_signal_1_is_keyed() {
    let host = std::sync::Arc::new(TestHost::default());
    let options = daw::plugin::LoadOptions {
        actions: Some(host.clone()),
        ..daw::plugin::LoadOptions::default()
    };
    let mut rooms = daw::plugin::load_plugin_with(&bundle().to_string_lossy(), &options)
        .expect("loads")
        .expect("a CLAP");
    let state = SignalState {
        instance: Instance::Port(SessionLink {
            session: SessionId(format!("shape-{}", std::process::id())),
            port: PortName("Drums/Rooms".into()),
        }),
        ..SignalState::new(drum_kit(&KitPieces::default()))
    };
    rooms
        .load_state(&clap_state(&state))
        .expect("takes the state");
    rooms.prepare(SR, BLOCK as u32).expect("activates");
    let signal = rooms.signal().expect("signal/1");
    let chain: serde_json::Value = serde_json::from_str(
        &signal
            .run_action(
                "SIGNAL_BLOCK_ADD",
                r#"{"phase": "tone", "kind": "transient", "key": "Drums/Snare/Sum/Trig"}"#,
            )
            .expect("SIGNAL_BLOCK_ADD"),
    )
    .expect("JSON");
    let block = chain["phases"]
        .as_array()
        .expect("phases")
        .iter()
        .flat_map(|p| p["blocks"].as_array().cloned().unwrap_or_default())
        .find(|b| b["kind"] == "transient")
        .unwrap_or_else(|| panic!("a transient in {chain}"));
    println!(
        "transient: {} key {} meters {}",
        block["id"], block["key"], block["meters"]
    );
    assert_eq!(block["key"]["from"], "Drums/Snare/Sum/Trig");
    let ids: Vec<&str> = block["params"]
        .as_array()
        .expect("params")
        .iter()
        .filter_map(|p| p["id"].as_str())
        .collect();
    assert_eq!(
        ids,
        [
            "attack",
            "sustain",
            "attack_time",
            "sustain_time",
            "key_window",
            "output"
        ]
    );
    assert_eq!(block["meters"][0]["id"], "applied_gain");
    assert_eq!(block["meters"][0]["kind"], "gain");
}

/// Each block's gain reduction over a span, with no playback: the kick
/// rendered as an offline render does (the host's song position every
/// block), SIGNAL_METERS summarises each block's meters over the span it
/// names and nothing outside it: the compressor's reduction where the kick
/// plays, none over the silence after.
#[test]
#[ignore = "needs the bundled .clap"]
fn signal_meters_summarise_each_blocks_reduction_over_a_span() {
    let host = std::sync::Arc::new(TestHost::default());
    let options = daw::plugin::LoadOptions {
        actions: Some(host.clone()),
        ..daw::plugin::LoadOptions::default()
    };
    let mut kick = daw::plugin::load_plugin_with(&bundle().to_string_lossy(), &options)
        .expect("loads")
        .expect("a CLAP");
    let state = SignalState {
        instance: Instance::Port(SessionLink {
            session: SessionId(format!("meters-{}", std::process::id())),
            port: PortName("Drums/Kick".into()),
        }),
        ..SignalState::new(drum_kit(&KitPieces::default()))
    };
    kick.load_state(&clap_state(&state))
        .expect("takes the state");
    kick.prepare(SR, BLOCK as u32).expect("activates");
    kick.pump_main_thread();
    let sinks = host.sinks.lock().unwrap().clone();
    let find = |topic: &str| {
        sinks
            .iter()
            .enumerate()
            .find(|(_, (t, _))| t == topic)
            .map(|(i, (t, s))| ((i + 1) as u64, t.clone(), s.clone()))
    };
    let (Some(transport), Some(hits)) = (find("transport"), find("hits:Drums/Kick")) else {
        println!("this daw host offers no subscriptions");
        return;
    };
    // The kick on every half second for the first two seconds, then nothing.
    let placed: Vec<String> = (0..4)
        .map(|i| {
            format!(
                r#"{{"time": {}, "velocity": 100}}"#,
                f64::from(i) * 0.5 + 0.25
            )
        })
        .collect();
    hits.2.send(
        hits.0,
        &hits.1,
        &format!(
            r#"{{"piece": "Drums/Kick", "track": "Drums/Kick/Sum/Trig", "note": 36, "count": 4, "hits": [{}]}}"#,
            placed.join(",")
        ),
    );
    let signal = kick.signal().expect("signal/1");
    let meters = |args: &str| -> serde_json::Value {
        serde_json::from_str(
            &signal
                .run_action("SIGNAL_METERS", args)
                .expect("SIGNAL_METERS"),
        )
        .expect("JSON")
    };
    let render = |kick: &mut Box<dyn PluginInstance>| {
        let frames = 48_000 * 3;
        let mut input = vec![0.0_f32; frames];
        for h in 0..4 {
            let at = 12_000 + h * 24_000;
            for (i, s) in input.iter_mut().enumerate().skip(at).take(6_000) {
                let t = (i - at) as f32;
                *s = 0.9
                    * (-t / 1_500.0).exp()
                    * (t * 55.0 * std::f32::consts::TAU / SR as f32).sin();
            }
        }
        let mut out = vec![0.0_f32; BLOCK];
        let mut scratch = vec![0.0_f32; BLOCK];
        for start in (0..frames).step_by(BLOCK) {
            let end = (start + BLOCK).min(frames);
            let n = end - start;
            transport.2.send(
                transport.0,
                &transport.1,
                &format!(r#"{{"playing": true, "frame": {start}}}"#),
            );
            kick.process_block(
                &input[start..end],
                &input[start..end],
                &mut out[..n],
                &mut scratch[..n],
                &PluginEvents::EMPTY,
            )
            .expect("processes");
        }
    };
    let comp_of = |m: &serde_json::Value| -> serde_json::Value {
        m["blocks"]
            .as_array()
            .expect("blocks")
            .iter()
            .find(|b| b["block"] == "tone.comp")
            .cloned()
            .unwrap_or_else(|| panic!("tone.comp in {m}"))
    };
    // Where the kick plays.
    let _ = meters(r#"{"from": 0.2, "to": 2.0}"#);
    render(&mut kick);
    let playing = meters("{}");
    let comp = comp_of(&playing);
    println!("over the kicks: {}", comp);
    let gr = &comp["gr_db"];
    assert!(
        gr["max"].as_f64().expect("a max") < -1.0,
        "it reduces: {gr}"
    );
    assert!(gr["mean"].as_f64().expect("a mean") < 0.0, "{gr}");
    let seconds = comp["meters"][0]["seconds"].as_f64().expect("seconds");
    assert!((seconds - 1.8).abs() < 0.05, "the span alone: {seconds} s");
    let gate = playing["blocks"]
        .as_array()
        .expect("blocks")
        .iter()
        .find(|b| b["block"] == "rescue.gate")
        .expect("the gate");
    assert!(
        gate["meters"][0]["max"].as_f64().expect("its opening") > 0.5,
        "{gate}"
    );
    // Over the silence after them: no reduction.
    kick.reset();
    let _ = meters(r#"{"from": 2.5, "to": 3.0}"#);
    render(&mut kick);
    let quiet = comp_of(&meters("{}"));
    println!("over the silence: {}", quiet);
    assert!(
        quiet["gr_db"]["max"].as_f64().expect("a max") > -0.1,
        "{quiet}"
    );
}
