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
