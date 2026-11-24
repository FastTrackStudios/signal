//! Document playback driven like a host (document mode phases 2 and 3):
//! a fake transport advancing block by block through [`DocumentPlayer`],
//! held against the offline walker (`document::render_schedule`).
//!
//! - From the top, and from any start — inside a legato transition, mid
//!   sustain, in a release tail, on a prefire, on an arrival — the output
//!   is the full render sliced there, bit for bit.
//! - A seek reconstructed on the worker is silent until the playhead
//!   reaches where it was built, fades in, then is the full render again.
//! - Live input beside the document is summed in and leaves the
//!   document's own output exact.
//! - A new schedule (an edit) swapped in mid-playback plays on with no
//!   silent block: the next block is the new full render's.
//!
//! The library is a synthetic CSS-shaped legato patch (sines), the one the
//! phase-2 host-sim used.

use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sampler::document::{DocumentRenderOptions, render_schedule};
use sampler::{
    BlockTransport, DocCc, DocNote, DocumentPlayer, Input, LiveEvent, Played, SamplerBank,
    Schedule, SeekPolicy, TrackDocument, annotate,
};

const SR: u32 = 48_000;
const ID: &str = "fixture";
/// The fixture's slow legato delay (velocities 0–64).
const SLOW_DELAY_FRAMES: i64 = 333 * SR as i64 / 1000;

fn write_sine_wav(path: &Path, frames: usize, freq: f64, amp: f32) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SR,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec).expect("create wav");
    for i in 0..frames {
        let t = i as f64 / f64::from(SR);
        let s = (t * freq * std::f64::consts::TAU).sin() as f32 * amp;
        w.write_sample(s).expect("write sample");
    }
    w.finalize().expect("finalize wav");
}

fn build_fixture(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).expect("mkdir");
    write_sine_wav(&dir.join("sus.wav"), SR as usize, 220.0, 0.5);
    let mut zones = String::from(
        "    { file sus.wav, key_min 0, key_max 127, root_key 60, articulation Sus }\n",
    );
    for rr in 0..4u32 {
        for dirn in ["up", "down"] {
            let f = format!("leg_{dirn}_{rr}.wav");
            write_sine_wav(
                &dir.join(&f),
                SR as usize,
                40.0f64.mul_add(f64::from(rr), 300.0),
                0.4,
            );
            let _ = writeln!(
                zones,
                "    {{ file {f}, key_min 0, key_max 127, root_key 64, articulation Leg, direction {dirn}, rr_index {rr} }}"
            );
        }
        let f = format!("stac_{rr}.wav");
        write_sine_wav(
            &dir.join(&f),
            SR as usize / 4,
            60.0f64.mul_add(f64::from(rr), 500.0),
            0.4,
        );
        let _ = writeln!(
            zones,
            "    {{ file {f}, key_min 0, key_max 127, root_key 60, articulation Stac, rr_index {rr} }}"
        );
    }
    let styx = format!(
        r#"
name DocFixture
articulations (
    {{ id Sus,  label Sustain,  kind @Sustain, rr 1 }}
    {{ id Leg,  label Legato,   kind @Legato,  rr 4, directional true }}
    {{ id Stac, label Staccato, kind @Short,   rr 4 }}
)
legato_engine {{
    expressive {{
        zones (
            {{vel_range (0 64),   label slow, delay_ms 333}}
            {{vel_range (65 127), label fast, delay_ms 100}}
        )
    }}
    low_latency {{
        zones ( {{vel_range (0 127), label fast, delay_ms 100}} )
    }}
}}
short_note_timing {{ pre_delay_ms 60 }}
keyswitch {{
    cc58_map {{
        0-5   "Sustain: Low Latency Legato"
        21-25 Staccato
    }}
}}
zones (
{zones})
"#
    );
    let spec_path = dir.join("lib.styx");
    std::fs::write(&spec_path, styx).expect("write styx");
    spec_path
}

/// The fixture library on disk for the life of the test.
struct Fixture {
    dir: PathBuf,
    spec: PathBuf,
}

impl Fixture {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "signal-document-player-{tag}-{}",
            std::process::id()
        ));
        let spec = build_fixture(&dir);
        Self { dir, spec }
    }

    fn bank(&self) -> SamplerBank {
        let mut bank = SamplerBank::new(SR);
        bank.load_instrument(ID, &self.spec, Some(&self.dir), "", "")
            .expect("load fixture instrument");
        bank.preload_instrument(ID).expect("preload");
        bank
    }

    fn schedule(&self, doc: &TrackDocument) -> Arc<Schedule> {
        let spec = self.bank().instrument_spec(ID).expect("spec");
        Arc::new(annotate(doc, &spec, SR))
    }

    /// The offline full render (the bounce every start is held to).
    fn full(&self, sched: &Schedule) -> Vec<f32> {
        let full = render_schedule(
            &mut self.bank(),
            ID,
            sched,
            &DocumentRenderOptions {
                tail_sec: 1.0,
                ..Default::default()
            },
        );
        assert_eq!(full.reactive_fallbacks, 0);
        full.audio
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

const fn note(start_qn: f64, end_qn: f64, chan: u8, pitch: u8, vel: u8) -> DocNote {
    DocNote {
        start_qn,
        end_qn,
        chan,
        pitch,
        vel,
    }
}

/// Two legato lines on two channels, each with its own pre-starts: line 0
/// slow (333 ms) then fast (100 ms), line 1 fast.
fn song() -> TrackDocument {
    TrackDocument {
        seed: 0xC1A9_5EED,
        notes: vec![
            note(0.0, 2.1, 0, 60, 90),
            note(2.0, 4.1, 0, 62, 30),
            note(4.0, 6.0, 0, 65, 110),
            note(1.0, 3.1, 1, 48, 80),
            note(3.0, 5.0, 1, 50, 100),
        ],
        ccs: vec![DocCc {
            qn: 0.0,
            chan: 0,
            cc: 1,
            val: 96,
        }],
        ..Default::default()
    }
}

fn bits(audio: &[f32]) -> Vec<u32> {
    audio.iter().map(|s| s.to_bits()).collect()
}

/// A host playing from `start` for `frames` frames, in blocks of `block`.
struct Host<'a> {
    player: &'a mut DocumentPlayer,
    sched: Option<Arc<Schedule>>,
    pos: i64,
    block: usize,
    input: Input,
}

impl Host<'_> {
    fn play(&mut self, frames: usize, live: &[(i64, LiveEvent)]) -> (Vec<f32>, Vec<Played>) {
        let mut audio = Vec::with_capacity(frames * 2);
        let mut played = Vec::new();
        let mut buf = vec![0.0f32; self.block * 2];
        let mut done = 0usize;
        while done < frames {
            let n = self.block.min(frames - done);
            let t = BlockTransport {
                playing: true,
                pos_frame: self.pos,
                tempo_bpm: Some(120.0),
            };
            let events: Vec<(u32, LiveEvent)> = live
                .iter()
                .filter(|(at, _)| *at >= self.pos && *at < self.pos + n as i64)
                .map(|(at, e)| ((at - self.pos) as u32, *e))
                .collect();
            played.push(self.player.process(
                self.sched.as_ref(),
                &t,
                self.input,
                &events,
                &mut buf[..n * 2],
            ));
            audio.extend_from_slice(&buf[..n * 2]);
            self.pos += n as i64;
            done += n;
        }
        (audio, played)
    }
}

#[test]
fn from_the_top_the_player_is_the_offline_render() {
    let fx = Fixture::new("top");
    let sched = fx.schedule(&song());
    let full = fx.full(&sched);
    for block in [512, 479, 64] {
        let mut player = DocumentPlayer::new(ID, fx.bank(), Some(fx.bank()), 512);
        let (out, played) = Host {
            player: &mut player,
            sched: Some(sched.clone()),
            pos: 0,
            block,
            input: Input::Live,
        }
        .play(full.len() / 2, &[]);
        assert!(played.iter().all(|p| *p == Played::Document));
        assert_eq!(bits(&out), bits(&full), "block {block}");
        assert_eq!(player.scheduler().reconstructed_voices(), 0);
    }
}

/// The adversarial starts, and what each must reconstruct.
fn starts() -> Vec<(i64, &'static str)> {
    let tick2: i64 = 48_000; // QN 2 at 120 bpm
    let prefire2 = tick2 - SLOW_DELAY_FRAMES;
    vec![
        (20_000, "mid-sustain of the first note"),
        (
            prefire2 + SLOW_DELAY_FRAMES / 2,
            "inside the 333 ms legato transition",
        ),
        (prefire2, "exactly on the prefire"),
        (tick2, "exactly on the arrival"),
        (146_000, "in the release tail after the last note-off"),
    ]
}

#[test]
fn any_start_is_the_full_render_sliced_there() {
    let fx = Fixture::new("starts");
    let sched = fx.schedule(&song());
    let full = fx.full(&sched);
    for (p, what) in starts() {
        let slice = &full[(p as usize) * 2..];
        let mut player = DocumentPlayer::new(ID, fx.bank(), None, 512);
        let (out, _) = Host {
            player: &mut player,
            sched: Some(sched.clone()),
            pos: p,
            block: 512,
            input: Input::Track,
        }
        .play(slice.len() / 2, &[]);
        assert_eq!(bits(&out), bits(slice), "start at {p} ({what})");
        assert!(player.scheduler().reconstructed_voices() > 0, "{what}");
    }
}

#[test]
fn a_seek_built_on_the_worker_is_silent_then_the_full_render() {
    let fx = Fixture::new("worker");
    let sched = fx.schedule(&song());
    let full = fx.full(&sched);
    let policy = SeekPolicy {
        inline_max_frames: 0, // every replay goes to the worker
        lead_frames: 4_800,
        catch_up_max: 48_000,
        fade_frames: 240,
    };
    for (p, what) in starts() {
        let mut player = DocumentPlayer::new(ID, fx.bank(), None, 512)
            .with_worker(fx.bank(), policy)
            .expect("worker");
        let mut host = Host {
            player: &mut player,
            sched: Some(sched.clone()),
            pos: p,
            block: 256,
            input: Input::Track,
        };
        // Play until it comes in (the worker is quick, but real time is
        // not simulated: wait a little between blocks while silent).
        let mut out = Vec::new();
        let mut came_in = None;
        for _ in 0..2_000 {
            let (block, played) = host.play(256, &[]);
            if played[0] == Played::Reconstructing {
                assert!(
                    block.iter().all(|s| *s == 0.0),
                    "{what}: silent while it builds"
                );
                std::thread::sleep(std::time::Duration::from_micros(200));
            } else if came_in.is_none() {
                came_in = Some(host.pos - 256);
            }
            out.extend_from_slice(&block);
            if came_in.is_some_and(|c| host.pos > c + 9_600) {
                break;
            }
        }
        let came_in = came_in.unwrap_or_else(|| panic!("{what}: never came in"));
        assert!(
            came_in >= p + 4_800,
            "{what}: not before where it was built"
        );
        // From the end of the fade-in on: the full render, bit for bit.
        let from = (came_in + 240) as usize;
        let ours = &out[(from - p as usize) * 2..];
        let theirs = &full[from * 2..from * 2 + ours.len()];
        assert_eq!(bits(ours), bits(theirs), "{what}: after the fade");
    }
}

#[test]
fn live_input_beside_the_document_leaves_it_exact() {
    let fx = Fixture::new("live");
    let sched = fx.schedule(&song());
    let full = fx.full(&sched);
    let live = [
        (
            10_000,
            LiveEvent::NoteOn {
                note: 72,
                velocity: 100,
            },
        ),
        (40_000, LiveEvent::NoteOff { note: 72 }),
        (
            50_000,
            LiveEvent::NoteOn {
                note: 74,
                velocity: 60,
            },
        ),
        (90_000, LiveEvent::NoteOff { note: 74 }),
    ];
    let frames = full.len() / 2;
    let mut player = DocumentPlayer::new(ID, fx.bank(), Some(fx.bank()), 512);
    let (both, _) = Host {
        player: &mut player,
        sched: Some(sched.clone()),
        pos: 0,
        block: 512,
        input: Input::Live,
    }
    .play(frames, &live);

    // The live part alone, played the same way on a plain live bank.
    let mut alone = DocumentPlayer::new(ID, fx.bank(), None, 512);
    let (live_only, _) = Host {
        player: &mut alone,
        sched: None,
        pos: 0,
        block: 512,
        input: Input::Live,
    }
    .play(frames, &live);
    assert!(live_only.iter().any(|s| *s != 0.0), "the live notes sound");
    let sum: Vec<f32> = full.iter().zip(&live_only).map(|(d, l)| d + l).collect();
    assert_eq!(
        bits(&both),
        bits(&sum),
        "document + live, the document's own part untouched"
    );

    // Without the host's claim, the same events are the track's own
    // notes: dropped, the document alone.
    let mut track = DocumentPlayer::new(ID, fx.bank(), Some(fx.bank()), 512);
    let (doc_only, _) = Host {
        player: &mut track,
        sched: Some(sched),
        pos: 0,
        block: 512,
        input: Input::Track,
    }
    .play(frames, &live);
    assert_eq!(bits(&doc_only), bits(&full));
}

#[test]
fn an_edit_swapped_in_mid_playback_plays_on_without_a_dropout() {
    let fx = Fixture::new("edit");
    let before = fx.schedule(&song());
    let mut edited = song();
    edited.notes[2].pitch = 67; // the third note moves up
    edited.notes[2].vel = 40; // and slows its transition
    let after = fx.schedule(&edited);
    let full_after = fx.full(&after);

    let mut player = DocumentPlayer::new(ID, fx.bank(), None, 512);
    let mut host = Host {
        player: &mut player,
        sched: Some(before),
        pos: 0,
        block: 512,
        input: Input::Track,
    };
    let _ = host.play(512 * 100, &[]); // 1.07 s in: mid first note
    host.sched = Some(after);
    let swap_at = host.pos as usize;
    let (rest, played) = host.play(48_000, &[]);
    assert!(played.iter().all(|p| *p == Played::Document));
    assert!(
        rest[..1024].iter().any(|s| *s != 0.0),
        "the block after the edit sounds"
    );
    assert_eq!(
        bits(&rest),
        bits(&full_after[swap_at * 2..swap_at * 2 + rest.len()]),
        "from the swap on: the edited song's full render"
    );
}
