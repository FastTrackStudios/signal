//! The kit's ports as a host with a transport runs them (Studio's offline
//! renders, `LISTEN_*`): each block told its song position. A port's
//! output is its input's and its key's only: the same on any block grid,
//! from any start with its preroll, and after a reset as from fresh.

use std::sync::Arc;

use signal_core::graph::{self, BuildOptions};
use signal_core::host::NoHost;
use signal_core::kit::{KitPieces, drum_kit};
use signal_core::session::{HitBus, Session, SessionId};
use signal_core::state::Preset;
use signal_core::{BlockFactory, BuiltinFactory, Processor};

const SR: f32 = 48_000.0;
const MAX: usize = 1_024;
const FRAMES: usize = 48_000 * 4;
const TRIG: &str = "Drums/Kick/Sum/Trig";
const KICK: &str = "Drums/Kick";

fn noise(n: usize, level: f32, seed: u32) -> Vec<f32> {
    let mut x = seed;
    (0..n)
        .map(|_| {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            level * ((x >> 8) as f32 / 8_388_608.0 - 1.0)
        })
        .collect()
}

/// A kick's hits (bursts) every `every` frames from `first`, over `bed`.
fn hits(first: usize, every: usize, level: f32, bed: Vec<f32>) -> Vec<f32> {
    let mut x = bed;
    let mut at = first;
    while at < x.len() {
        for (i, s) in x.iter_mut().enumerate().skip(at).take(2_000) {
            let t = (i - at) as f32;
            *s += level * (-t / 400.0).exp() * (t * 60.0 * std::f32::consts::TAU / SR).sin();
        }
        at += every;
    }
    x
}

/// Port `port` of `preset`, in `session`.
fn port(preset: &Preset, session: &Arc<Session>, port: &str) -> Processor {
    let factories: [&dyn BlockFactory; 1] = [&BuiltinFactory];
    let (mut g, _) = graph::build(
        preset,
        &BuildOptions {
            sample_rate: SR,
            max_block: MAX,
            port: Some(port),
            factories: &factories,
            host: &NoHost,
        },
    );
    g.join(session.clone());
    let mut p = Processor::new();
    let _ = p.install(g);
    p
}

fn session(name: &str) -> Arc<Session> {
    Arc::new(Session {
        id: SessionId(name.into()),
        hits: HitBus::new(),
    })
}

/// Render `ports` (each with its input, by song frame) over `from..to`,
/// in blocks: `first` frames, then `MAX`. Each block told its position.
/// Every port's output, by song frame (0 before `from`).
fn render(
    ports: &mut [(&mut Processor, &[f32])],
    (from, to): (usize, usize),
    first: usize,
) -> Vec<Vec<f32>> {
    let mut outs = vec![vec![0.0_f32; FRAMES]; ports.len()];
    let mut at = from;
    let mut len = first.min(MAX);
    while at < to {
        let end = (at + len).min(to);
        for ((p, input), out) in ports.iter_mut().zip(outs.iter_mut()) {
            let mut l = input[at..end].to_vec();
            let mut r = l.clone();
            p.set_position(at as u64);
            p.process(&mut [&mut l, &mut r], 2, end - at, &[], &[]);
            out[at..end].copy_from_slice(&l);
        }
        at = end;
        len = MAX;
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
}

fn inputs() -> Inputs {
    Inputs {
        trig: hits(12_345, 11_111, 0.9, vec![0.0; FRAMES]),
        kick: hits(12_345, 11_111, 0.6, noise(FRAMES, 0.05, 1)),
    }
}

/// The kick, keyed by its trigger, the same on any block grid: where a
/// hit lands is the song's, not where a block happens to end.
#[test]
fn a_keyed_port_is_the_same_on_any_block_grid() {
    let preset = drum_kit(&KitPieces::default());
    let i = inputs();
    let mut out = Vec::new();
    for (k, first) in [MAX, 300, 1_000].into_iter().enumerate() {
        let s = session(&format!("grid-{k}"));
        let mut trig = port(&preset, &s, TRIG);
        let mut kick = port(&preset, &s, KICK);
        out.push(render(
            &mut [(&mut trig, &i.trig), (&mut kick, &i.kick)],
            (0, FRAMES),
            first,
        ));
    }
    assert!(peak(&out[0][1]) > 0.05);
    for (k, o) in out.iter().enumerate().skip(1) {
        println!("grid {k}: worst {:.1} dBFS", worst_db(&out[0][1], &o[1]));
        assert!(
            worst_db(&out[0][1], &o[1]) < -100.0,
            "grid {k}: {:.1} dBFS",
            worst_db(&out[0][1], &o[1])
        );
    }
}

/// Ports reset (a host reusing them: CLAP's `reset`) and rendered again
/// from a preroll sound as fresh ones rendering the same.
#[test]
fn a_reset_port_sounds_as_a_fresh_one() {
    let preset = drum_kit(&KitPieces::default());
    let i = inputs();
    let s = session("reset-warm");
    let mut trig = port(&preset, &s, TRIG);
    let mut kick = port(&preset, &s, KICK);
    // The whole song once, then reset, then 1.5 s from 0.5 s.
    let _ = render(
        &mut [(&mut trig, &i.trig), (&mut kick, &i.kick)],
        (0, FRAMES),
        MAX,
    );
    trig.reset();
    kick.reset();
    let warm = render(
        &mut [(&mut trig, &i.trig), (&mut kick, &i.kick)],
        (24_000, 96_000),
        MAX,
    );
    let s = session("reset-fresh");
    let mut trig = port(&preset, &s, TRIG);
    let mut kick = port(&preset, &s, KICK);
    let fresh = render(
        &mut [(&mut trig, &i.trig), (&mut kick, &i.kick)],
        (24_000, 96_000),
        MAX,
    );
    println!(
        "reset vs fresh: kick peak {:.3}, worst {:.1} dBFS",
        peak(&fresh[1]),
        worst_db(&warm[1], &fresh[1])
    );
    assert!(peak(&fresh[1]) > 0.05);
    assert_eq!(warm[1], fresh[1], "a reset port doesn't null a fresh one");
}

/// A mono 16-bit WAV of `samples`.
fn wav16(samples: &[f32]) -> Vec<u8> {
    let data: Vec<u8> = samples
        .iter()
        .flat_map(|s| ((s.clamp(-1.0, 1.0) * 32_767.0) as i16).to_le_bytes())
        .collect();
    let mut w = Vec::new();
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&48_000u32.to_le_bytes());
    w.extend_from_slice(&96_000u32.to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&(data.len() as u32).to_le_bytes());
    w.extend_from_slice(&data);
    w
}

/// A trigger's hits published in a render's last block, after the port it
/// keys had run, aren't heard again when both are reset and render the
/// span once more: the sample plays each hit once, as from fresh.
#[test]
fn a_reset_port_hears_no_hit_from_before_it() {
    use signal_core::host::{AssetRef, MemoryHost};
    use signal_core::state::{BlockSpec, LaneSpec, TriggerSpec};
    let host = MemoryHost::default().with("kick.wav", wav16(&[0.5; 100]));
    let preset = Preset {
        name: "Kick".into(),
        inputs: 1,
        outputs: 2,
        lanes: vec![
            LaneSpec {
                name: "Trig".into(),
                port: Some("Trig".into()),
                input: vec![0],
                mute: true,
                trigger: Some(TriggerSpec {
                    threshold_db: -30.0,
                    note: None,
                }),
                ..LaneSpec::default()
            },
            LaneSpec {
                name: "Sample".into(),
                port: Some("Sample".into()),
                blocks: vec![
                    BlockSpec::signal("sample", "sample")
                        .asset(AssetRef::path("kick.wav"))
                        .param("dynamics", 0.0)
                        .key("Trig"),
                ],
                ..LaneSpec::default()
            },
        ],
        macros: vec![],
    };
    let factories: [&dyn BlockFactory; 1] = [&BuiltinFactory];
    let make = |s: &Arc<Session>, name: &str| {
        let (mut g, _) = graph::build(
            &preset,
            &BuildOptions {
                sample_rate: SR,
                max_block: MAX,
                port: Some(name),
                factories: &factories,
                host: &host,
            },
        );
        g.join(s.clone());
        let mut p = Processor::new();
        let _ = p.install(g);
        p
    };
    let trig_in = hits(12_345, 11_111, 0.9, vec![0.0; FRAMES]);
    let silence = vec![0.0; FRAMES];
    let s = session("stale-fresh");
    let (mut sample, mut trig) = (make(&s, "Sample"), make(&s, "Trig"));
    let fresh = render(
        &mut [(&mut sample, &silence), (&mut trig, &trig_in)],
        (24_000, 120_000),
        MAX,
    );
    // The sample's port first in every block; the first render ends
    // somewhere about the trigger hearing a hit (at 90 010), so in one of
    // them it hears it in the last block, after the sample's port ran.
    let mut worst = -240.0_f32;
    for end in (89_900..91_300).step_by(37) {
        let s = session(&format!("stale-warm-{end}"));
        let (mut sample, mut trig) = (make(&s, "Sample"), make(&s, "Trig"));
        let _ = render(
            &mut [(&mut sample, &silence), (&mut trig, &trig_in)],
            (0, end),
            MAX,
        );
        sample.reset();
        trig.reset();
        let warm = render(
            &mut [(&mut sample, &silence), (&mut trig, &trig_in)],
            (24_000, 120_000),
            MAX,
        );
        let d = worst_db(&warm[0], &fresh[0]);
        if d > worst {
            println!("first render to {end}: reset vs fresh worst {d:.1} dBFS");
            worst = d;
        }
    }
    assert!(peak(&fresh[0]) > 0.25);
    assert!(
        worst < -200.0,
        "a hit from before the reset was heard after it: {worst:.1} dBFS"
    );
}
