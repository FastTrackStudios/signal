//! A block's named outputs, routed by a send by port (signal#28): any block
//! with several outputs — a multi-mic piano here, not a drum — sends one of
//! them to another lane, sample-aligned with the block's other outputs
//! through the graph's delay compensation, and silent when its block is
//! passed by.

use signal_core::block::{BlockProcessor, BuildContext, Built, Unavailable};
use signal_core::graph::{self, BuildOptions};
use signal_core::host::NoHost;
use signal_core::param::ParamSpec;
use signal_core::state::{BlockSpec, LaneSpec, Preset, SendSpec};
use signal_core::{Backend, BlockFactory, BlockKind, BuiltinFactory, Platforms, Processor};

const SR: f32 = 48_000.0;
const MAX: usize = 512;
const LATE: usize = 64;

/// A multi-mic instrument: its close mic is its in-place audio (output
/// `close`), its room mic (output `room`) the same at half level, a room's
/// colour put aside.
struct Piano {
    close: (Vec<f32>, Vec<f32>),
    room: (Vec<f32>, Vec<f32>),
    n: usize,
}

impl BlockProcessor for Piano {
    fn params(&self) -> &'static [ParamSpec] {
        &[]
    }
    fn set_param(&mut self, _: usize, _: f32) {}
    fn reset(&mut self) {}
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = left.len().min(right.len()).min(self.close.0.len());
        for i in 0..n {
            self.close.0[i] = left[i];
            self.close.1[i] = right[i];
            self.room.0[i] = left[i] * 0.5;
            self.room.1[i] = right[i] * 0.5;
        }
        self.n = n;
    }
    fn output_count(&self) -> usize {
        2
    }
    fn output_name(&self, index: usize) -> Option<&str> {
        ["close", "room"].get(index).copied()
    }
    fn output(&self, index: usize) -> Option<(&[f32], &[f32])> {
        let (l, r) = match index {
            0 => &self.close,
            1 => &self.room,
            _ => return None,
        };
        Some((&l[..self.n], &r[..self.n]))
    }
}

/// A block `LATE` frames late (a lookahead), before the piano.
struct Late {
    l: Vec<f32>,
    r: Vec<f32>,
    pos: usize,
}

impl BlockProcessor for Late {
    fn params(&self) -> &'static [ParamSpec] {
        &[]
    }
    fn set_param(&mut self, _: usize, _: f32) {}
    fn reset(&mut self) {
        self.l.fill(0.0);
        self.r.fill(0.0);
    }
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (a, b) in left.iter_mut().zip(right.iter_mut()) {
            core::mem::swap(a, &mut self.l[self.pos]);
            core::mem::swap(b, &mut self.r[self.pos]);
            self.pos = (self.pos + 1) % LATE;
        }
    }
    fn latency(&self) -> u32 {
        LATE as u32
    }
}

struct Factory;

impl BlockFactory for Factory {
    fn runs_on(&self, kind: &BlockKind) -> Option<Platforms> {
        match kind {
            BlockKind::Signal(n) if n == "piano" || n == "late" => Some(Platforms::ALL),
            _ => None,
        }
    }
    fn build(&self, spec: &BlockSpec, _cx: &BuildContext<'_>) -> Result<Built, Unavailable> {
        let backend: Box<dyn BlockProcessor> = match &spec.kind {
            BlockKind::Signal(n) if n == "piano" => Box::new(Piano {
                close: (vec![0.0; MAX], vec![0.0; MAX]),
                room: (vec![0.0; MAX], vec![0.0; MAX]),
                n: 0,
            }),
            _ => Box::new(Late {
                l: vec![0.0; LATE],
                r: vec![0.0; LATE],
                pos: 0,
            }),
        };
        Ok(Built {
            params: &[],
            sources: &[],
            backend: Backend::Linked(backend),
        })
    }
}

/// The piano on its lane (outputs 0/1), its room mic sent by port to a
/// room bus (outputs 2/3).
fn preset(bypass_piano: bool) -> Preset {
    let mut piano = BlockSpec::signal("piano", "piano");
    piano.bypass = bypass_piano;
    Preset {
        name: "Piano".into(),
        inputs: 2,
        outputs: 4,
        lanes: vec![
            LaneSpec {
                name: "Piano".into(),
                input: vec![0, 1],
                output: 0,
                sends: vec![SendSpec::to("Room", 0.0).port("piano", "room")],
                blocks: vec![BlockSpec::signal("late", "late"), piano],
                ..LaneSpec::default()
            },
            LaneSpec {
                name: "Room".into(),
                output: 2,
                blocks: vec![],
                ..LaneSpec::default()
            },
        ],
        macros: vec![],
    }
}

fn render(preset: &Preset) -> (Vec<f32>, Vec<f32>, Vec<String>) {
    let factories: [&dyn BlockFactory; 2] = [&Factory, &BuiltinFactory];
    let (g, report) = graph::build(
        preset,
        &BuildOptions {
            sample_rate: SR,
            max_block: MAX,
            port: None,
            factories: &factories,
            host: &NoHost,
        },
    );
    let mut p = Processor::new();
    let _ = p.install(g);
    let frames = 48_000;
    let input: Vec<f32> = (0..frames)
        .map(|i| ((i as f32 * 0.37).sin() * 0.4 + (i as f32 * 0.011).sin() * 0.3))
        .collect();
    let (mut piano, mut room) = (Vec::new(), Vec::new());
    for piece in input.chunks(MAX) {
        let n = piece.len();
        let mut c0 = piece.to_vec();
        let mut c1 = piece.to_vec();
        let mut c2 = vec![0.0; n];
        let mut c3 = vec![0.0; n];
        p.process(&mut [&mut c0, &mut c1, &mut c2, &mut c3], 2, n, &[], &[]);
        piano.extend(c0);
        room.extend(c2);
    }
    (piano, room, report.warnings)
}

#[test]
fn a_ports_send_is_sample_aligned_with_the_blocks_other_outputs() {
    let (piano, room, warnings) = render(&preset(false));
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(piano.iter().any(|s| s.abs() > 0.1), "the piano plays");
    // The room bus is the room mic: half the close mic, on the same frame.
    let worst = piano
        .iter()
        .zip(&room)
        .fold(0.0_f32, |m, (c, r)| m.max((r - c * 0.5).abs()));
    assert!(worst < 1e-7, "room against close: {worst}");
    // Both as late as the lookahead before the piano, no later.
    assert!(piano[..LATE].iter().all(|s| *s == 0.0));
    assert!(piano[LATE..LATE + 8].iter().any(|s| *s != 0.0));
}

#[test]
fn a_ports_send_from_a_block_passed_by_is_silent() {
    let (piano, room, _) = render(&preset(true));
    assert!(piano.iter().any(|s| s.abs() > 0.1), "the lane still plays");
    assert!(room.iter().all(|s| *s == 0.0), "its port is silent");
}

#[test]
fn a_send_by_a_port_the_block_has_not_is_a_warning() {
    let mut p = preset(false);
    p.lanes[0].sends = vec![SendSpec::to("Room", 0.0).port("piano", "oh")];
    let (_, room, warnings) = render(&p);
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("no output oh of block piano")),
        "{warnings:?}"
    );
    assert!(room.iter().all(|s| *s == 0.0));
}
