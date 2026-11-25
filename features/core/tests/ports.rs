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

const SUB: &str = "Drums/Kick/Sub";
const ROOM: &str = "Drums/Kick/Verb/Room";
const IN: &str = "Drums/Kick/Sum/In";
const OUT: &str = "Drums/Kick/Sum/Out";
const SAMPLE: &str = "Drums/Kick/Sum/Sample";

/// The kick's output with `others` running beside its trigger, over
/// `from..FRAMES`, each port given the kick's input (the trigger its own).
fn kick_with(preset: &Preset, i: &Inputs, others: &[&str], from: usize, tag: &str) -> Vec<f32> {
    let s = session(tag);
    let mut trig = port(preset, &s, TRIG);
    let mut kick = port(preset, &s, KICK);
    let mut rest: Vec<Processor> = others.iter().map(|o| port(preset, &s, o)).collect();
    let mut ports: Vec<(&mut Processor, &[f32])> =
        vec![(&mut trig, &i.trig[..]), (&mut kick, &i.kick[..])];
    ports.extend(rest.iter_mut().map(|p| (p, &i.kick[..])));
    render(&mut ports, (from, FRAMES), MAX).swap_remove(1)
}

/// A keyed port hears its own input and its key's hits, nothing else: the
/// kick is the same to the sample whichever other ports of its session run
/// (its Sub and Room, keyed or fed from the same trigger and sum; its mics;
/// its sample).
#[test]
fn a_keyed_port_hears_the_same_whatever_else_of_its_session_runs() {
    let preset = drum_kit(&KitPieces::default());
    let i = inputs();
    let alone = kick_with(&preset, &i, &[], 0, "alone");
    assert!(peak(&alone) > 0.05);
    for (k, others) in [
        &[SUB][..],
        &[ROOM][..],
        &[SUB, ROOM][..],
        &[IN, OUT, SAMPLE, SUB, ROOM][..],
    ]
    .iter()
    .enumerate()
    {
        let with = kick_with(&preset, &i, others, 0, &format!("with-{k}"));
        println!("with {others:?}: worst {:.1} dBFS", worst_db(&alone, &with));
        assert_eq!(alone, with, "the kick heard {others:?} run");
    }
}

/// A render from part way in, with a second's preroll, is the full render
/// over the same frames, to below a 24-bit sample: nothing a port does
/// depends on how long it has been running, or on hits from before the
/// render. Not bit for bit: a recursive filter (an EQ, a compressor's
/// envelope) still holds an exponentially faded trace of what came before
/// the preroll, about -143 dBFS here.
#[test]
fn a_partial_render_with_its_preroll_is_the_full_render_sliced() {
    let preset = drum_kit(&KitPieces::default());
    let i = inputs();
    let at = 96_000;
    let full = kick_with(&preset, &i, &[SUB], 0, "full");
    let part = kick_with(&preset, &i, &[SUB], at - 48_000, "part");
    let worst = worst_db(&full[at..], &part[at..]);
    assert!(peak(&full[at..]) > 0.05);
    assert!(
        worst < -120.0,
        "a partial render is {worst:.1} dBFS off the full one sliced"
    );
}

/// Render `ports` over `from..to` in `MAX` blocks, each port with its input
/// and its hit notes (song frames of note-ons of `note`); each block told
/// its position. Every port's output, by song frame.
fn render_with_notes(
    ports: &mut [(&mut Processor, &[f32], &[usize])],
    (from, to): (usize, usize),
    note: u8,
) -> Vec<Vec<f32>> {
    let mut outs = vec![vec![0.0_f32; FRAMES]; ports.len()];
    let mut at = from;
    while at < to {
        let end = (at + MAX).min(to);
        for ((p, input, notes), out) in ports.iter_mut().zip(outs.iter_mut()) {
            let mut l = input[at..end].to_vec();
            let mut r = l.clone();
            let midi: Vec<signal_core::MidiEvent> = notes
                .iter()
                .filter(|n| (at..end).contains(*n))
                .map(|n| signal_core::MidiEvent::note_on((*n - at) as u32, note, 100))
                .collect();
            p.set_position(at as u64);
            p.process(&mut [&mut l, &mut r], 2, end - at, &midi, &[]);
            out[at..end].copy_from_slice(&l);
        }
        at = end;
    }
    outs
}

/// The hit notes on the Sample track (Studio writes every hit there),
/// published as the kick's hits: reset and rendered again from a preroll,
/// the kick plays them as a fresh session does — none heard twice, none
/// at a frame from before the reset.
#[test]
fn hit_notes_published_by_a_reset_port_play_as_from_fresh() {
    let preset = drum_kit(&KitPieces::default());
    let i = inputs();
    let notes: Vec<usize> = (0..)
        .map(|k| 12_345 + k * 11_111)
        .take_while(|f| *f < FRAMES)
        .collect();
    let silence = vec![0.0_f32; FRAMES];
    let run = |s: &Arc<Session>, warm_first: bool| {
        let mut trig = port(&preset, s, TRIG);
        let mut kick = port(&preset, s, KICK);
        let mut sample = port(&preset, s, SAMPLE);
        if warm_first {
            let _ = render_with_notes(
                &mut [
                    (&mut trig, &i.trig, &[]),
                    (&mut kick, &i.kick, &[]),
                    (&mut sample, &silence, &notes),
                ],
                (0, FRAMES),
                signal_core::kit::KICK_NOTE,
            );
            trig.reset();
            kick.reset();
            sample.reset();
        }
        render_with_notes(
            &mut [
                (&mut trig, &i.trig, &[]),
                (&mut kick, &i.kick, &[]),
                (&mut sample, &silence, &notes),
            ],
            (24_000, 120_000),
            signal_core::kit::KICK_NOTE,
        )
        .swap_remove(1)
    };
    let fresh = run(&session("notes-fresh"), false);
    let warm = run(&session("notes-warm"), true);
    println!(
        "notes, reset vs fresh: worst {:.1} dBFS",
        worst_db(&fresh, &warm)
    );
    assert!(peak(&fresh) > 0.05);
    assert_eq!(
        fresh, warm,
        "a reset port's hit notes played as fresh ones don't"
    );
}

/// As [`render_with_notes`], each port held back by its own lateness (what
/// a host's delay compensation does, telling it the song time of the audio
/// it hands it): port `k` processes song frames `at - late[k]` in the host
/// block that starts at `at`. Outputs by song frame.
fn render_late(
    ports: &mut [(&mut Processor, &[f32], &[usize], usize)],
    (from, to): (usize, usize),
    note: u8,
) -> Vec<Vec<f32>> {
    let mut outs = vec![vec![0.0_f32; FRAMES]; ports.len()];
    let most = ports.iter().map(|p| p.3).max().unwrap_or(0);
    let mut at = from;
    while at < to + most {
        for ((p, input, notes, late), out) in ports.iter_mut().zip(outs.iter_mut()) {
            let Some(start) = at.checked_sub(*late) else {
                continue;
            };
            let end = (start + MAX).min(FRAMES);
            if start >= end {
                continue;
            }
            let mut l = input[start..end].to_vec();
            let mut r = l.clone();
            let midi: Vec<signal_core::MidiEvent> = notes
                .iter()
                .filter(|n| (start..end).contains(*n))
                .map(|n| signal_core::MidiEvent::note_on((*n - start) as u32, note, 100))
                .collect();
            p.set_position(start as u64);
            p.process(&mut [&mut l, &mut r], 2, end - start, &midi, &[]);
            out[start..end].copy_from_slice(&l);
        }
        at += MAX;
    }
    outs
}

/// A keyed port plays its key's hits where the song has them, however far
/// the host's delay compensation holds it back behind the ports that
/// publish them: the kick, its trigger's hits and the Sample track's hit
/// notes coming from ports on time, is the same held back 2 480 frames
/// (a kit whose sample lane is latent) as not held back at all.
#[test]
fn a_port_held_back_by_delay_compensation_plays_the_same_hits() {
    let preset = drum_kit(&KitPieces::default());
    let i = inputs();
    // The hit notes a few ms off the trigger's onsets, as a song's hit
    // markers are off Signal's own detection.
    let notes: Vec<usize> = (0..)
        .map(|k| 12_345 + 150 + k * 11_111)
        .take_while(|f| *f < FRAMES)
        .collect();
    let silence = vec![0.0_f32; FRAMES];
    let run = |late: usize, tag: &str| {
        let s = session(tag);
        let mut trig = port(&preset, &s, TRIG);
        let mut kick = port(&preset, &s, KICK);
        let mut sample = port(&preset, &s, SAMPLE);
        render_late(
            &mut [
                (&mut trig, &i.trig, &[], 0),
                (&mut sample, &silence, &notes, 0),
                (&mut kick, &i.kick, &[], late),
            ],
            (0, FRAMES - 4 * MAX),
            signal_core::kit::KICK_NOTE,
        )
        .swap_remove(2)
    };
    let on_time = run(0, "late-0");
    let held = run(2_480, "late-2480");
    let span = 24_000..FRAMES - 8 * MAX;
    println!(
        "held back vs on time: worst {:.1} dBFS",
        worst_db(&on_time[span.clone()], &held[span.clone()])
    );
    assert!(peak(&on_time[span.clone()]) > 0.05);
    assert_eq!(
        on_time[span.clone()],
        held[span],
        "the kick's hits moved with its delay compensation"
    );
}

/// Every block of the kit, reset after a render, sounds as fresh: the
/// Room's reverb (its tail, its modulation, its diffusers) as much as the
/// kick's gate.
#[test]
fn a_reset_room_sounds_as_a_fresh_one() {
    let preset = drum_kit(&KitPieces::default());
    let i = inputs();
    let s = session("room-warm");
    let mut room = port(&preset, &s, ROOM);
    let _ = render(&mut [(&mut room, &i.kick)], (0, FRAMES), MAX);
    room.reset();
    let warm = render(&mut [(&mut room, &i.kick)], (24_000, 96_000), MAX);
    let s = session("room-fresh");
    let mut room = port(&preset, &s, ROOM);
    let fresh = render(&mut [(&mut room, &i.kick)], (24_000, 96_000), MAX);
    println!(
        "room reset vs fresh: worst {:.1} dBFS",
        worst_db(&warm[0], &fresh[0])
    );
    assert!(peak(&fresh[0]) > 0.001);
    assert_eq!(warm[0], fresh[0], "a reset room doesn't null a fresh one");
}

/// Builds the builtin blocks, counting the hits handed to them.
struct Counting(Arc<std::sync::atomic::AtomicUsize>);

struct CountHits {
    inner: Box<dyn signal_core::BlockProcessor>,
    hits: Arc<std::sync::atomic::AtomicUsize>,
}

impl signal_core::BlockProcessor for CountHits {
    fn params(&self) -> &'static [signal_core::param::ParamSpec] {
        self.inner.params()
    }
    fn set_param(&mut self, index: usize, value: f32) {
        self.inner.set_param(index, value);
    }
    fn reset(&mut self) {
        self.inner.reset();
    }
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.inner.process(left, right);
    }
    fn latency(&self) -> u32 {
        self.inner.latency()
    }
    fn midi(&mut self, event: signal_core::block::Midi) {
        self.inner.midi(event);
    }
    fn set_key(&mut self, left: &[f32], right: &[f32]) {
        self.inner.set_key(left, right);
    }
    fn keyed(&mut self) {
        self.inner.keyed();
    }
    fn hit(&mut self, offset: u32, velocity: f32) {
        self.hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.inner.hit(offset, velocity);
    }
    fn mod_sources(&self) -> &'static [signal_core::block::ModSourceSpec] {
        self.inner.mod_sources()
    }
    fn mod_source(&self, index: usize) -> f32 {
        self.inner.mod_source(index)
    }
}

impl BlockFactory for Counting {
    fn runs_on(&self, kind: &signal_core::BlockKind) -> Option<signal_core::Platforms> {
        BuiltinFactory.runs_on(kind)
    }
    fn build(
        &self,
        spec: &signal_core::state::BlockSpec,
        cx: &signal_core::block::BuildContext<'_>,
    ) -> Result<signal_core::block::Built, signal_core::block::Unavailable> {
        let mut built = BuiltinFactory.build(spec, cx)?;
        if let signal_core::Backend::Linked(inner) = built.backend {
            built.backend = signal_core::Backend::Linked(Box::new(CountHits {
                inner,
                hits: self.0.clone(),
            }));
        }
        Ok(built)
    }
}

/// One strike, published by two ports (two Sample tracks of the kit, both
/// with the hit notes): the Sub's blocks are handed it once, as from one
/// port.
#[test]
fn a_strike_two_ports_publish_is_heard_once() {
    let preset = drum_kit(&KitPieces::default());
    let i = inputs();
    let notes: Vec<usize> = (0..)
        .map(|k| 12_345 + k * 11_111)
        .take_while(|f| *f < FRAMES)
        .collect();
    let silence = vec![0.0_f32; FRAMES];
    let run = |tag: &str, twice: bool| {
        let s = session(tag);
        let heard = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counting = Counting(heard.clone());
        let factories: [&dyn BlockFactory; 1] = [&counting];
        let (mut g, _) = graph::build(
            &preset,
            &BuildOptions {
                sample_rate: SR,
                max_block: MAX,
                port: Some(SUB),
                factories: &factories,
                host: &NoHost,
            },
        );
        g.join(s.clone());
        let mut sub = Processor::new();
        let _ = sub.install(g);
        let mut sample = port(&preset, &s, SAMPLE);
        let mut again = port(&preset, &s, SAMPLE);
        let none: &[usize] = &[];
        let _ = render_with_notes(
            &mut [
                (&mut sample, &silence, &notes),
                (&mut again, &silence, if twice { &notes } else { none }),
                (&mut sub, &i.kick, &[]),
            ],
            (0, FRAMES),
            signal_core::kit::KICK_NOTE,
        );
        heard.load(std::sync::atomic::Ordering::Relaxed)
    };
    let once = run("strike-once", false);
    let twice = run("strike-twice", true);
    assert!(once > 0, "the Sub heard no hit");
    assert_eq!(once, twice, "a strike two ports publish is heard twice");
}

/// A kick keyed by hits the host placed, through a gap longer than the
/// gate's ten seconds: rendered from part way into the gap (a second's
/// preroll) it plays what the whole song's render plays there. The gate
/// knows its hits are all placed, so it waits through the gap in both,
/// rather than failing open on its own clock in one.
#[test]
fn a_partial_render_in_a_long_gap_of_placed_hits_is_the_full_one() {
    let preset = drum_kit(&KitPieces::default());
    let piece = preset
        .lanes
        .iter()
        .position(|l| l.port.as_deref() == Some(TRIG))
        .and_then(|i| u16::try_from(i).ok())
        .expect("the kit's kick trigger");
    let frames = 48_000 * 15;
    // The kick mic: its drum, and bleed throughout.
    let input = hits(24_000, 24_000, 0.8, noise(frames, 0.3, 5));
    let placed = vec![(24_000_u64, 1.0_f32), (48_000, 1.0), (48_000 * 14, 1.0)];
    let render_from = |from: usize, tag: &str| -> Vec<f32> {
        let s = session(tag);
        let factories: [&dyn BlockFactory; 1] = [&BuiltinFactory];
        let (mut g, _) = graph::build(
            &preset,
            &BuildOptions {
                sample_rate: SR,
                max_block: MAX,
                port: Some(KICK),
                factories: &factories,
                host: &NoHost,
            },
        );
        g.join(s);
        let (mut controller, mut kick) = signal_core::processor_pair();
        let _ = kick.install(g);
        let _ = controller.send(signal_core::engine::Command::PlannedHits {
            piece,
            hits: placed.clone().into_boxed_slice(),
        });
        let mut out = vec![0.0_f32; frames];
        let mut at = from;
        while at < frames {
            let end = (at + MAX).min(frames);
            let mut l = input[at..end].to_vec();
            let mut r = l.clone();
            kick.set_position(at as u64);
            kick.process(&mut [&mut l, &mut r], 2, end - at, &[], &[]);
            out[at..end].copy_from_slice(&l);
            at = end;
        }
        controller.collect();
        out
    };
    let at = 48_000 * 12;
    let full = render_from(0, "gap-full");
    let part = render_from(at - 48_000, "gap-part");
    let worst = worst_db(&full[at..], &part[at..]);
    assert!(
        peak(&full[48_000 * 14..]) > 0.05,
        "the hit after the gap plays"
    );
    assert!(
        worst < -120.0,
        "a partial render in the gap is {worst:.1} dBFS off the full one"
    );
}
