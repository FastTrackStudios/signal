//! A port's output is its input's and its key's, nothing else: the same
//! whichever other ports of its session run, in whichever order the host
//! runs them, and the same from an instance that held another state as
//! from a fresh one. Through the bundled CLAP and daw's plugin host, as
//! Studio's offline renders (`LISTEN_*`) run Signal.
//!
//! ```sh
//! cargo test -p signal-plugin --test port_isolation -- --ignored --nocapture
//! ```

use std::path::PathBuf;

use daw::plugin::{PluginEvents, PluginInstance, load_plugin};
use signal_core::kit::{KitPieces, drum_kit};
use signal_core::session::{PortName, SessionId, SessionLink};
use signal_core::state::{Instance, Preset, SignalState};
use signal_plugin::plugin::clap_state;

const SR: f64 = 48_000.0;
const BLOCK: usize = 512;
const FRAMES: usize = 48_000 * 4;

fn bundle() -> PathBuf {
    std::env::var_os("SIGNAL_CLAP").map_or_else(
        || {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../target/bundled/FTS Signal.clap")
        },
        PathBuf::from,
    )
}

fn state(preset: &Preset, session: &str, port: &str) -> Vec<u8> {
    clap_state(&SignalState {
        instance: Instance::Port(SessionLink {
            session: SessionId(session.into()),
            port: PortName(port.into()),
        }),
        ..SignalState::new(preset.clone())
    })
}

fn instance(preset: &Preset, session: &str, port: &str) -> Box<dyn PluginInstance> {
    let path = bundle();
    assert!(path.exists(), "no bundle at {}", path.display());
    let mut p = load_plugin(&path.to_string_lossy())
        .expect("loads")
        .expect("a CLAP");
    p.load_state(&state(preset, session, port))
        .expect("takes the state");
    p.prepare(SR, BLOCK as u32).expect("activates");
    p
}

fn noise(n: usize, level: f32, seed: u32) -> Vec<f32> {
    let mut x = seed;
    (0..n)
        .map(|_| {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            level * ((x >> 8) as f32 / 8_388_608.0 - 1.0)
        })
        .collect()
}

/// A kick's hits (bursts) at `every` frames, from `first`, over `bed`.
fn hits(first: usize, every: usize, level: f32, bed: Vec<f32>) -> Vec<f32> {
    let mut x = bed;
    let mut at = first;
    while at < x.len() {
        for (i, s) in x.iter_mut().enumerate().skip(at).take(2_000) {
            let t = (i - at) as f32;
            *s += level * (-t / 400.0).exp() * (t * 60.0 * std::f32::consts::TAU / 48_000.0).sin();
        }
        at += every;
    }
    x
}

/// Run `ports` (each an instance and its input) over the whole input, a
/// block at a time, in their order: each one's output.
fn run(ports: &mut [(&mut Box<dyn PluginInstance>, &[f32])]) -> Vec<Vec<f32>> {
    let mut outs = vec![vec![0.0_f32; FRAMES]; ports.len()];
    let mut scratch = vec![0.0_f32; BLOCK];
    for start in (0..FRAMES).step_by(BLOCK) {
        let end = (start + BLOCK).min(FRAMES);
        for ((p, input), out) in ports.iter_mut().zip(outs.iter_mut()) {
            p.process_block(
                &input[start..end],
                &input[start..end],
                &mut out[start..end],
                &mut scratch[..end - start],
                &PluginEvents::EMPTY,
            )
            .expect("processes");
        }
    }
    outs
}

fn worst_db(a: &[f32], b: &[f32]) -> f32 {
    let d = a
        .iter()
        .zip(b)
        .fold(0.0_f32, |m, (x, y)| m.max((x - y).abs()));
    20.0 * d.max(1e-12).log10()
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0, |m, s| m.max(s.abs()))
}

struct Inputs {
    trig: Vec<f32>,
    kick: Vec<f32>,
    sub: Vec<f32>,
    room: Vec<f32>,
}

fn inputs() -> Inputs {
    // Hits a little off the block grid, so some land at a block's end.
    Inputs {
        trig: hits(12_345, 11_111, 0.9, vec![0.0; FRAMES]),
        kick: hits(12_345, 11_111, 0.6, noise(FRAMES, 0.05, 1)),
        sub: hits(12_345, 11_111, 0.3, noise(FRAMES, 0.02, 2)),
        room: noise(FRAMES, 0.1, 3),
    }
}

#[test]
#[ignore = "needs the bundled .clap"]
fn a_keyed_port_hears_only_its_input_and_its_key() {
    let preset = drum_kit(&KitPieces::default());
    let i = inputs();
    let pid = std::process::id();

    // Its trigger first, the two alone.
    let alone = format!("iso-alone-{pid}");
    let mut trig = instance(&preset, &alone, "Drums/Kick/Sum/Trig");
    let mut kick = instance(&preset, &alone, "Drums/Kick");
    let a = run(&mut [(&mut trig, &i.trig), (&mut kick, &i.kick)]);

    // Its trigger after it, with the kick's Sub and Room running too.
    let crowd = format!("iso-crowd-{pid}");
    let mut kick2 = instance(&preset, &crowd, "Drums/Kick");
    let mut sub = instance(&preset, &crowd, "Drums/Kick/Sub");
    let mut room = instance(&preset, &crowd, "Drums/Kick/Verb/Room");
    let mut trig2 = instance(&preset, &crowd, "Drums/Kick/Sum/Trig");
    let b = run(&mut [
        (&mut kick2, &i.kick),
        (&mut sub, &i.sub),
        (&mut room, &i.room),
        (&mut trig2, &i.trig),
    ]);

    let (ka, kb) = (&a[1], &b[0]);
    println!(
        "Drums/Kick: peak {:.3}; alone vs with Sub and Room, its trigger after it: worst {:.1} dBFS",
        peak(ka),
        worst_db(ka, kb)
    );
    assert!(peak(ka) > 0.05, "the kick plays: {}", peak(ka));
    assert_eq!(
        ka, kb,
        "the kick's output depends on its session's other ports"
    );
}

#[test]
#[ignore = "needs the bundled .clap"]
fn a_state_loaded_over_another_sounds_as_a_fresh_instance() {
    let preset = drum_kit(&KitPieces::default());
    let i = inputs();
    let pid = std::process::id();

    // One instance holds the snare, in a session of its own, and plays.
    let before = format!("load-before-{pid}");
    let mut snare_trig = instance(&preset, &before, "Drums/Snare/Sum/Trig");
    let mut warm = instance(&preset, &before, "Drums/Snare");
    let _ = run(&mut [(&mut snare_trig, &i.trig), (&mut warm, &i.kick)]);

    // Then takes the kick, in the session a fresh kick is in.
    let s1 = format!("load-warm-{pid}");
    warm.load_state(&state(&preset, &s1, "Drums/Kick"))
        .expect("takes the kick");
    warm.pump_main_thread();
    let mut trig1 = instance(&preset, &s1, "Drums/Kick/Sum/Trig");
    let a = run(&mut [(&mut trig1, &i.trig), (&mut warm, &i.kick)]);

    let s2 = format!("load-fresh-{pid}");
    let mut trig2 = instance(&preset, &s2, "Drums/Kick/Sum/Trig");
    let mut fresh = instance(&preset, &s2, "Drums/Kick");
    let b = run(&mut [(&mut trig2, &i.trig), (&mut fresh, &i.kick)]);

    println!(
        "Drums/Kick: peak {:.3}; loaded over the snare vs fresh: worst {:.1} dBFS",
        peak(&b[1]),
        worst_db(&a[1], &b[1])
    );
    assert!(peak(&b[1]) > 0.05);
    assert_eq!(
        a[1], b[1],
        "a state loaded over another doesn't null a fresh one"
    );
}
