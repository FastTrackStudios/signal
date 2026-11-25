//! A preset built into blocks: lanes from input channels to output
//! channels. Built off the audio thread (everything is allocated here);
//! processed on it without allocating.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::block::{
    Backend, BlockFactory, BlockKind, BlockStatus, BuildContext, Built, ChainBlock, Midi,
    ParamBank, ParamChange, Passthrough, Unavailable,
};
use crate::host::SignalHost;
use crate::param::MACROS;
use crate::platform::Platforms;
use crate::session::{HitCursor, Session, SessionHit};
use crate::state::{BlockSpec, LaneSpec, MacroMode, Phase, Preset};

/// What a graph is built for.
pub struct BuildOptions<'a> {
    pub sample_rate: f32,
    /// The longest piece the processor hands a block (≤ [`crate::MAX_BLOCK`]).
    pub max_block: usize,
    /// The port this instance is, in the per-track mode; `None` for the
    /// multi-channel mode.
    pub port: Option<&'a str>,
    /// Asked in turn for each block. Put [`crate::BuiltinFactory`] in.
    pub factories: &'a [&'a dyn BlockFactory],
    pub host: &'a dyn SignalHost,
}

/// One block's outcome: what the UI lists, and what usage is logged from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockReport {
    pub lane: String,
    pub block: String,
    pub kind: BlockKind,
    /// `linked` or `clap`.
    pub backend: String,
    #[serde(flatten)]
    pub status: BlockStatus,
}

/// What building a preset came to.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildReport {
    pub blocks: Vec<BlockReport>,
    /// Things in the preset that were ignored (an unknown parameter, a
    /// macro target that names no block).
    pub warnings: Vec<String>,
}

impl BuildReport {
    /// Every block runs as the preset has it.
    #[must_use]
    pub fn all_active(&self) -> bool {
        self.blocks.iter().all(|b| b.status == BlockStatus::Active)
    }
}

struct Lane {
    name: String,
    /// `(note, piece, the piece's lane here)`: notes that are hits.
    midi_map: Vec<(u8, u16, Option<usize>)>,
    /// Its blocks' mix phases, each a run of blocks with its make-up.
    phases: Vec<PhaseGroup>,
    input: Vec<usize>,
    output: usize,
    gain: f32,
    mute: bool,
    blocks: Vec<ChainBlock>,
    l: Vec<f32>,
    r: Vec<f32>,
    /// What other lanes routed here this piece (a bus's input).
    in_l: Vec<f32>,
    in_r: Vec<f32>,
    /// This lane's input before its chain: a key, a pre send.
    pre_l: Vec<f32>,
    pre_r: Vec<f32>,
    /// The lane it feeds instead of the outputs.
    to: Option<usize>,
    sends: Vec<Send>,
    /// Delay compensation: the host input (a lane also fed by others),
    /// the feed to `to`, the way to the outputs. Every path into a bus,
    /// and every lane to the outputs, arrives as late as the latest.
    in_delay: Delay,
    to_delay: Delay,
    out_delay: Delay,
    /// `(block, key lane)`: blocks listening to another lane.
    keys: Vec<(usize, usize)>,
    #[cfg(feature = "fx")]
    detector: Option<plugin_processors::trigger::HitDetector>,
    /// A MIDI note that is also one of this lane's hits.
    note: Option<u8>,
    /// This lane's place in the preset: the piece its hits are, to the
    /// session.
    piece: u16,
    hits: [Hit; MAX_HITS],
    hit_count: usize,
    /// Hits from MIDI, for the next piece.
    midi_hits: [Hit; MAX_HITS],
    midi_hit_count: usize,
    /// Hits placed past the end of the piece they were found in (a
    /// detection's lag back, the lookahead on): `(song frame, velocity,
    /// midi)`, for the piece that hears them.
    later: [(u64, f32, bool); MAX_HITS],
    later_count: usize,
}

/// One mix phase's run of blocks in a lane: `first..=last`, its
/// gain-neutral make-up after it, and its level meter (the energy into
/// its first block and out of its last, before the make-up).
struct PhaseGroup {
    phase: Phase,
    first: usize,
    last: usize,
    trim: f32,
    energy_in: f64,
    energy_out: f64,
    /// This piece's energy into the phase, counted once its output is
    /// known to be sounding.
    piece_in: f64,
    /// The meters hear as loudness does (K-weighted, BS.1770), so a
    /// gain-neutral phase reads 0 LU on a loudness meter.
    k_in: KWeight,
    k_out: KWeight,
    /// Switched off: its blocks and its make-up skipped.
    bypass: bool,
    /// Its output's K-weighted mean energy over the last piece: a meter.
    level: f64,
}

/// What keys a keyed block now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyVia {
    /// A trigger's detected hits (its audio).
    Hits,
    /// Hit notes (MIDI).
    Midi,
    /// The host placed the piece's every hit (`set_planned_hits`): those
    /// key it, and a gap between them is the song's, so a gate waits
    /// through it rather than falling back to its own input.
    Placed,
    /// No hit for ten seconds: a gate keys from its own input.
    Own,
}

impl KeyVia {
    /// `signal.chain/1`'s word for it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hits => "hits",
            Self::Midi => "midi",
            Self::Placed => "placed",
            Self::Own => "self",
        }
    }
}

/// BS.1770's K-weighting (a high shelf, then a high-pass), stereo, for a
/// meter: what LUFS hears.
#[derive(Clone, Copy, Debug, Default)]
struct KWeight {
    shelf: [f64; 5],
    high_pass: [f64; 5],
    /// Per channel, per stage: the two delay elements.
    z: [[f64; 2]; 4],
}

impl KWeight {
    fn new(sample_rate: f32) -> Self {
        let fs = f64::from(sample_rate.max(1.0));
        let pi = core::f64::consts::PI;
        // The shelf.
        let (f0, g, q) = (
            1681.974_450_955_533,
            3.999_843_853_973_347,
            0.707_175_236_955_419_6,
        );
        let k = libm::tan(pi * f0 / fs);
        let vh = libm::pow(10.0, g / 20.0);
        let vb = libm::pow(vh, 0.499_666_774_154_541_6);
        let a0 = 1.0 + k / q + k * k;
        let shelf = [
            (vh + vb * k / q + k * k) / a0,
            2.0 * (k * k - vh) / a0,
            (vh - vb * k / q + k * k) / a0,
            2.0 * (k * k - 1.0) / a0,
            (1.0 - k / q + k * k) / a0,
        ];
        // The high-pass.
        let (f0, q) = (38.135_470_876_024_44, 0.500_327_037_323_877_3);
        let k = libm::tan(pi * f0 / fs);
        let a0 = 1.0 + k / q + k * k;
        let high_pass = [
            1.0,
            -2.0,
            1.0,
            2.0 * (k * k - 1.0) / a0,
            (1.0 - k / q + k * k) / a0,
        ];
        Self {
            shelf,
            high_pass,
            z: [[0.0; 2]; 4],
        }
    }

    fn stage(c: &[f64; 5], z: &mut [f64; 2], x: f64) -> f64 {
        let y = c[0] * x + z[0];
        z[0] = c[1] * x - c[3] * y + z[1];
        z[1] = c[2] * x - c[4] * y;
        y
    }

    /// The K-weighted energy of a stereo piece.
    fn energy(&mut self, l: &[f32], r: &[f32]) -> f64 {
        let (shelf, hp) = (self.shelf, self.high_pass);
        let mut e = 0.0;
        for (ch, x) in [l, r].into_iter().enumerate() {
            for s in x {
                let a = Self::stage(&shelf, &mut self.z[ch * 2], f64::from(*s));
                let b = Self::stage(&hp, &mut self.z[ch * 2 + 1], a);
                e += b * b;
            }
        }
        e
    }
}

/// Below this mean energy per sample (-80 dBFS) a phase's output is
/// silence: a gate shut, a drum not playing. Those pieces aren't counted,
/// in or out, so a phase is measured on what it passes: a gate's job is
/// to take the bleed away, not to be made up for.
const SOUNDING: f64 = 1e-8;

/// What a phase does to a lane's level, over what the graph has played
/// since its meters were cleared.
#[derive(Clone, Debug, PartialEq)]
pub struct PhaseLevel {
    pub lane: String,
    pub phase: Phase,
    /// Level into the phase and out of it before its make-up, dB (RMS).
    pub in_db: f32,
    pub out_db: f32,
    /// Its make-up now.
    pub trim_db: f32,
}

impl PhaseLevel {
    /// The make-up that makes the phase gain neutral.
    #[must_use]
    pub fn neutral_trim_db(&self) -> f32 {
        self.in_db - self.out_db
    }
}

/// A lane's blocks grouped by phase, in order. A phase that comes back
/// after a later one (Tone, Rescue) is out of the method's order: reported,
/// and run where it is.
fn phase_groups(spec: &LaneSpec, sample_rate: f32, warnings: &mut Vec<String>) -> Vec<PhaseGroup> {
    let mut groups: Vec<PhaseGroup> = Vec::new();
    for (i, b) in spec.blocks.iter().enumerate() {
        let Some(phase) = b.phase else { continue };
        match groups.last_mut() {
            Some(g) if g.phase == phase && g.last + 1 == i => g.last = i,
            last => {
                if let Some(prev) = last.map(|g| g.phase).filter(|p| *p >= phase) {
                    warnings.push(alloc::format!(
                        "{}/{}: {} after {}: out of the mix's phase order",
                        spec.name,
                        b.id,
                        phase.slug(),
                        prev.slug()
                    ));
                }
                let trim = spec
                    .trims
                    .iter()
                    .find(|t| t.phase == phase)
                    .map_or(0.0, |t| t.gain_db);
                groups.push(PhaseGroup {
                    phase,
                    first: i,
                    last: i,
                    trim: crate::util::db_to_gain(trim),
                    energy_in: 0.0,
                    energy_out: 0.0,
                    piece_in: 0.0,
                    k_in: KWeight::new(sample_rate),
                    k_out: KWeight::new(sample_rate),
                    bypass: spec.bypass_phases.contains(&phase),
                    level: 0.0,
                });
            }
        }
    }
    groups
}

/// The most a phase's make-up moves its level, dB.
pub const MAX_TRIM_DB: f32 = 24.0;

/// Make every measured phase of `preset` gain neutral: each lane's
/// [`PhaseTrim`](crate::state::PhaseTrim)s set from what its phases did
/// to its level ([`Graph::phase_levels`] over a render of the song). A
/// phase's make-up changes what the next phase and the lanes it feeds
/// are given, so measure and apply again until the trims stop moving.
pub fn apply_neutral_trims(preset: &mut Preset, levels: &[PhaseLevel]) {
    for level in levels
        .iter()
        .filter(|l| l.in_db > -150.0 && l.out_db > -150.0)
    {
        let Some(lane) = preset.lanes.iter_mut().find(|l| l.name == level.lane) else {
            continue;
        };
        // A phase that needs more than this is broken, not quiet: left
        // at the cap, and the caller says so.
        let gain_db = level.neutral_trim_db().clamp(-MAX_TRIM_DB, MAX_TRIM_DB);
        match lane.trims.iter_mut().find(|t| t.phase == level.phase) {
            Some(t) => t.gain_db = gain_db,
            None => lane.trims.push(crate::state::PhaseTrim {
                phase: level.phase,
                gain_db,
            }),
        }
    }
}

/// A send to another lane.
struct Send {
    to: usize,
    gain: f32,
    /// Taken before the chain and the fader.
    pre: bool,
    delay: Delay,
}

/// A stereo delay of a fixed number of frames, for delay compensation.
#[derive(Default)]
struct Delay {
    l: Vec<f32>,
    r: Vec<f32>,
    pos: usize,
}

impl Delay {
    fn new(frames: usize) -> Self {
        Self {
            l: vec![0.0; frames],
            r: vec![0.0; frames],
            pos: 0,
        }
    }

    /// Delay `l`/`r` in place.
    fn run(&mut self, l: &mut [f32], r: &mut [f32]) {
        let n = self.l.len();
        if n == 0 {
            return;
        }
        for (a, b) in l.iter_mut().zip(r.iter_mut()) {
            if let (Some(dl), Some(dr)) = (self.l.get_mut(self.pos), self.r.get_mut(self.pos)) {
                core::mem::swap(a, dl);
                core::mem::swap(b, dr);
            }
            self.pos = (self.pos + 1) % n;
        }
    }

    /// Add `src` delayed, times `gain`, into `dst`.
    fn add(&mut self, src: (&[f32], &[f32]), dst: (&mut [f32], &mut [f32]), gain: f32) {
        let n = self.l.len();
        for (((s_l, s_r), d_l), d_r) in src
            .0
            .iter()
            .zip(src.1)
            .zip(dst.0.iter_mut())
            .zip(dst.1.iter_mut())
        {
            let (x_l, x_r) = if n == 0 {
                (*s_l, *s_r)
            } else {
                let (Some(bl), Some(br)) = (self.l.get_mut(self.pos), self.r.get_mut(self.pos))
                else {
                    continue;
                };
                let out = (*bl, *br);
                *bl = *s_l;
                *br = *s_r;
                self.pos = (self.pos + 1) % n;
                out
            };
            *d_l += x_l * gain;
            *d_r += x_r * gain;
        }
    }

    fn clear(&mut self) {
        self.l.fill(0.0);
        self.r.fill(0.0);
    }
}

/// A drum hit in a piece.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Hit {
    offset: u32,
    velocity: f32,
    /// A note, not a detection.
    midi: bool,
}

/// The most hits a lane takes in one piece.
const MAX_HITS: usize = 16;

/// Two different lanes, both mutable.
fn pair_mut(lanes: &mut [Lane], a: usize, b: usize) -> Option<(&mut Lane, &mut Lane)> {
    if a == b {
        return None;
    }
    let (lo, hi, swap) = if a < b { (a, b, false) } else { (b, a, true) };
    let (left, right) = lanes.split_at_mut(hi);
    let x = left.get_mut(lo)?;
    let y = right.first_mut()?;
    Some(if swap { (y, x) } else { (x, y) })
}

struct MacroBinding {
    lane: usize,
    block: usize,
    param: usize,
    mode: MacroMode,
    min: f32,
    max: f32,
    /// This binding's current modulation offset (Modulate), so a change
    /// replaces its own contribution and keeps the others'.
    applied: f32,
}

/// A built preset.
pub struct Graph {
    max_block: usize,
    lanes: Vec<Lane>,
    /// The order lanes run in: every lane after the lanes that feed it,
    /// send to it or key it.
    order: Vec<usize>,
    macros: [Vec<MacroBinding>; MACROS],
    /// The inputs, copied out before the in-place outputs are written.
    input_copy: Vec<Vec<f32>>,
    /// How far the trigger lanes look ahead: the lanes hear the inputs
    /// this many frames late, the detectors on time, so a hit lands on
    /// its onset however near a piece's end it was (0 without triggers).
    lookahead: usize,
    /// Per input: the last `lookahead` frames, then this piece's.
    delayed: Vec<Vec<f32>>,
    latency: u32,
    /// `(lane, block, piece)`: blocks keyed to a lane another instance of
    /// the session runs, played from the session's hits.
    remote_keys: Vec<(usize, usize, u16)>,
    /// The session this instance is a port of: where its trigger lanes'
    /// hits go and its remote keys' come from.
    session: Option<alloc::sync::Arc<Session>>,
    cursor: Option<HitCursor>,
    /// Remote hits taken from the session, not yet due: `(piece, frame,
    /// velocity)`.
    remote: [(u16, u64, f32, bool); REMOTE_HITS],
    /// Per remote key: its last hit's frame and whether it was a note.
    key_last: Vec<(u64, bool)>,
    /// Per piece a remote key follows: its last note's frame. A piece with
    /// notes ignores its trigger's detected hits for [`Graph::ten_seconds`].
    piece_midi: Vec<(u16, u64)>,
    /// Per piece a remote key follows: the song frame of the last hit
    /// taken. A piece's hits come in song order, so one at or before it is
    /// a copy (two ports publishing one strike) and is dropped.
    piece_taken: Vec<(u16, Option<u64>)>,
    ten_seconds: u64,
    /// Per piece a remote key follows: its hits as the host placed them,
    /// `(song frame, velocity)`, sorted (empty until the host sends them).
    planned: Vec<(u16, Box<[(u64, f32)]>)>,
    /// Per piece a remote key follows: whether the host has placed its
    /// hits (any `set_planned_hits`, an empty list too: a piece the song
    /// never plays). Allocated at build, set on the audio thread.
    placed: Vec<(u16, bool)>,
    remote_count: usize,
    /// The song position of the next piece, in frames (the session's clock).
    clock: u64,
    /// Offline blocks' lanes Signal plays itself (a host without
    /// host-automation/1): set as host automation would set them.
    self_play: crate::offline::SelfPlay,
    /// `(lane, block, ring)`: offline blocks whose arriving audio is being
    /// captured for an analysis (a host that cannot render offline).
    taps: Vec<(
        usize,
        usize,
        alloc::sync::Arc<crate::offline::capture::CaptureRing>,
    )>,
}

/// Remote hits held for later pieces.
const REMOTE_HITS: usize = 64;

/// The lanes of `preset` an instance runs, in the order its graph holds
/// them: every lane in the multi-channel mode; in the per-track mode the
/// lanes of `port`, or the lanes with no port when it has none.
#[must_use]
pub fn lanes_for<'p>(preset: &'p Preset, port: Option<&str>) -> Vec<&'p LaneSpec> {
    match port {
        None => preset.lanes.iter().collect(),
        Some(port) => {
            let own: Vec<_> = preset
                .lanes
                .iter()
                .filter(|l| l.port.as_deref() == Some(port))
                .collect();
            if own.is_empty() {
                preset.lanes.iter().filter(|l| l.port.is_none()).collect()
            } else {
                own
            }
        }
    }
}

/// Where every block of `kind` is in the graph [`build`] makes of `preset`
/// for `port`, as `(lane, block)`: what the control side sends
/// [`Command::SwapBackend`](crate::engine::Command::SwapBackend) to when a
/// block is overridden by its CLAP ("every instance of that block").
#[must_use]
pub fn positions_of(preset: &Preset, port: Option<&str>, kind: &BlockKind) -> Vec<(usize, usize)> {
    lanes_for(preset, port)
        .iter()
        .enumerate()
        .flat_map(|(l, lane)| {
            lane.blocks
                .iter()
                .enumerate()
                .filter(|(_, b)| &b.kind == kind)
                .map(move |(b, _)| (l, b))
        })
        .collect()
}

/// Build `preset`.
#[must_use]
pub fn build(preset: &Preset, opts: &BuildOptions<'_>) -> (Graph, BuildReport) {
    let max_block = opts.max_block.clamp(1, crate::MAX_BLOCK);
    let cx = BuildContext {
        sample_rate: opts.sample_rate,
        max_block,
        host: opts.host,
    };
    let mut report = BuildReport::default();

    let lanes_here = lanes_for(preset, opts.port);

    let mut lanes = Vec::with_capacity(lanes_here.len());
    for spec in &lanes_here {
        let (input, output) = if opts.port.is_some() {
            // The track's own channels. A bus's are what the host summed
            // into its folder track: the lanes routed to it.
            let fed = preset.lanes.iter().any(|l| {
                l.to.as_deref() == Some(spec.name.as_str())
                    || l.sends.iter().any(|s| s.to == spec.name)
            });
            let input = match spec.input.len() {
                0 if fed => vec![0, 1],
                0 => Vec::new(),
                1 => vec![0],
                _ => vec![0, 1],
            };
            (input, 0)
        } else {
            (
                spec.input.iter().map(|c| usize::from(*c)).collect(),
                usize::from(spec.output),
            )
        };
        let mut blocks = Vec::with_capacity(spec.blocks.len());
        for b in &spec.blocks {
            let block = build_block(b, &cx, opts.factories, &mut report.warnings, &spec.name);
            log_usage(b, block.status());
            report.blocks.push(BlockReport {
                lane: spec.name.clone(),
                block: b.id.clone(),
                kind: b.kind.clone(),
                backend: if block.backend().is_linked() {
                    "linked".into()
                } else {
                    "clap".into()
                },
                status: block.status().clone(),
            });
            blocks.push(block);
        }
        lanes.push(Lane {
            name: spec.name.clone(),
            midi_map: spec
                .midi_hits
                .iter()
                .filter_map(|m| {
                    let piece = preset.lanes.iter().position(|l| l.name == m.piece)?;
                    let local = lanes_here.iter().position(|l| l.name == m.piece);
                    Some((m.note, u16::try_from(piece).ok()?, local))
                })
                .collect(),
            phases: phase_groups(spec, opts.sample_rate, &mut report.warnings),
            input,
            output,
            // In the per-track mode the track's own fader is the lane's
            // level: the host's, not the preset's.
            gain: if opts.port.is_some() {
                1.0
            } else {
                crate::util::db_to_gain(spec.gain_db)
            },
            // In the per-track mode the track's mute is the host's; a
            // trigger stays unheard (its hits are its job).
            mute: if opts.port.is_some() {
                spec.mute && spec.trigger.is_some()
            } else {
                spec.mute
            },
            blocks,
            l: vec![0.0; max_block],
            r: vec![0.0; max_block],
            in_l: vec![0.0; max_block],
            in_r: vec![0.0; max_block],
            pre_l: vec![0.0; max_block],
            pre_r: vec![0.0; max_block],
            to: None,
            sends: Vec::new(),
            in_delay: Delay::default(),
            to_delay: Delay::default(),
            out_delay: Delay::default(),
            keys: Vec::new(),
            #[cfg(feature = "fx")]
            detector: spec.trigger.as_ref().map(|t| {
                plugin_processors::trigger::HitDetector::new(
                    opts.sample_rate,
                    max_block,
                    t.threshold_db,
                )
            }),
            note: spec.trigger.as_ref().and_then(|t| t.note),
            piece: preset
                .lanes
                .iter()
                .position(|l| core::ptr::eq(l, *spec))
                .and_then(|p| u16::try_from(p).ok())
                .unwrap_or(u16::MAX),
            hits: [Hit::default(); MAX_HITS],
            hit_count: 0,
            midi_hits: [Hit::default(); MAX_HITS],
            midi_hit_count: 0,
            later: [(0, 0.0, false); MAX_HITS],
            later_count: 0,
        });
    }

    // Routing: where each lane goes, what it sends, what its blocks key on.
    let index_of = |name: &str| lanes_here.iter().position(|l| l.name == name);
    // A lane another instance runs: the host's routing reaches it.
    let elsewhere = |name: &str| opts.port.is_some() && preset.lanes.iter().any(|l| l.name == name);
    let mut remote_keys = Vec::new();
    for (i, spec) in lanes_here.iter().enumerate() {
        let to = spec.to.as_deref().and_then(|t| {
            let found = index_of(t).filter(|j| *j != i);
            if found.is_none() && !elsewhere(t) {
                report.warnings.push(alloc::format!(
                    "{}: no lane {t} to feed; to the outputs",
                    spec.name
                ));
            }
            found
        });
        let sends: Vec<Send> = spec
            .sends
            .iter()
            .filter_map(|s| match index_of(&s.to).filter(|j| *j != i) {
                Some(j) => Some(Send {
                    to: j,
                    gain: crate::util::db_to_gain(s.gain_db),
                    pre: s.pre,
                    delay: Delay::default(),
                }),
                None => {
                    if !elsewhere(&s.to) {
                        report.warnings.push(alloc::format!(
                            "{}: no lane {} to send to",
                            spec.name,
                            s.to
                        ));
                    }
                    None
                }
            })
            .collect();
        let keys: Vec<(usize, usize)> = spec
            .blocks
            .iter()
            .enumerate()
            .filter_map(|(b, block)| {
                let k = block.key.as_deref()?;
                match index_of(k) {
                    Some(j) => Some((b, j)),
                    None => {
                        // Another port's lane: its hits, over the session.
                        match preset
                            .lanes
                            .iter()
                            .position(|l| l.name == k)
                            .and_then(|p| u16::try_from(p).ok())
                        {
                            Some(piece) => remote_keys.push((i, b, piece)),
                            None => report.warnings.push(alloc::format!(
                                "{}/{}: no lane {k} to key on",
                                spec.name,
                                block.id
                            )),
                        }
                        None
                    }
                }
            })
            .collect();
        if let Some(lane) = lanes.get_mut(i) {
            lane.to = to;
            lane.sends = sends;
            lane.keys = keys;
        }
    }
    let order = run_order(&mut lanes, &lanes_here, &mut report.warnings);
    // Keyed blocks know it from the start: a gate keyed by another track's
    // trigger is shut until that trigger's first hit, not open on its
    // own bleed.
    let keyed: Vec<(usize, usize)> = lanes
        .iter()
        .enumerate()
        .flat_map(|(l, lane)| lane.keys.iter().map(move |(b, _)| (l, *b)))
        .chain(remote_keys.iter().map(|(l, b, _)| (*l, *b)))
        .collect();
    for (l, b) in keyed {
        if let Some(block) = lanes.get_mut(l).and_then(|lane| lane.blocks.get_mut(b)) {
            block.keyed();
        }
    }

    let mut macros: [Vec<MacroBinding>; MACROS] = core::array::from_fn(|_| Vec::new());
    for (m, spec) in preset.macros.iter().enumerate() {
        let Some(bindings) = macros.get_mut(m) else {
            report.warnings.push(alloc::format!(
                "macro {} beyond the {MACROS} macros: ignored",
                spec.name
            ));
            continue;
        };
        for t in &spec.targets {
            let Some(lane) = lanes_here.iter().position(|l| l.name == t.lane) else {
                // A lane of another port: not this instance's.
                continue;
            };
            let found = lanes.get(lane).and_then(|l: &Lane| {
                let block = l.blocks.iter().position(|b| b.id == t.block)?;
                let param = l.blocks.get(block)?.params().index(&t.param)?;
                Some((block, param))
            });
            match found {
                Some((block, param)) => {
                    bindings.push(MacroBinding {
                        lane,
                        block,
                        param,
                        mode: t.mode,
                        min: t.min,
                        max: t.max,
                        applied: 0.0,
                    });
                }
                None => report.warnings.push(alloc::format!(
                    "macro {}: no {}/{}/{} here",
                    spec.name,
                    t.lane,
                    t.block,
                    t.param
                )),
            }
        }
    }

    let inputs = lanes
        .iter()
        .flat_map(|l| l.input.iter().copied())
        .max()
        .map_or(0, |m| m.saturating_add(1));
    let latency = compensate(&mut lanes, &order);
    #[cfg(feature = "fx")]
    let lookahead = lanes
        .iter()
        .filter_map(|l| l.detector.as_ref())
        .map(|d| usize::try_from(d.lag()).unwrap_or(0))
        .max()
        .unwrap_or(0);
    #[cfg(not(feature = "fx"))]
    let lookahead = 0;
    // A remote hit is published by whichever instance the host runs, before
    // or after this one in a cycle: hearing a host block (and the far
    // detector's lag) late, every hit is in before it's due.
    let lookahead = if remote_keys.is_empty() {
        lookahead
    } else {
        lookahead.max(max_block.saturating_add(remote_lag(opts.sample_rate)))
    };
    let remote_key_count = remote_keys.len();
    let mut remote_pieces: Vec<u16> = remote_keys.iter().map(|k| k.2).collect();
    remote_pieces.sort_unstable();
    remote_pieces.dedup();
    let self_play = self_played(&lanes_here, &lanes, opts.sample_rate);
    let graph = Graph {
        max_block,
        lanes,
        order,
        macros,
        input_copy: vec![vec![0.0; max_block]; inputs],
        lookahead,
        delayed: if lookahead == 0 {
            Vec::new()
        } else {
            vec![vec![0.0; max_block + lookahead]; inputs]
        },
        latency: latency.saturating_add(u32::try_from(lookahead).unwrap_or(0)),
        remote_keys,
        session: None,
        cursor: None,
        remote: [(0, 0, 0.0, false); REMOTE_HITS],
        key_last: vec![(0, false); remote_key_count],
        piece_midi: remote_pieces.iter().map(|p| (*p, 0)).collect(),
        piece_taken: remote_pieces.iter().map(|p| (*p, None)).collect(),
        planned: remote_pieces.iter().map(|p| (*p, Box::default())).collect(),
        placed: remote_pieces.iter().map(|p| (*p, false)).collect(),
        ten_seconds: (f64::from(opts.sample_rate) * 10.0) as u64,
        remote_count: 0,
        clock: 0,
        self_play,
        taps: Vec::new(),
    };
    (graph, report)
}

/// The lanes of offline blocks that Signal plays itself: analysed, played
/// by Signal ([`Playback::Signal`](crate::offline::Playback)), and not
/// bypassed. Their curves are copied once, here, off the audio thread.
fn self_played(specs: &[&LaneSpec], lanes: &[Lane], sample_rate: f32) -> crate::offline::SelfPlay {
    let mut played = Vec::new();
    for (li, (spec, lane)) in specs.iter().zip(lanes).enumerate() {
        for (bi, (b, block)) in spec.blocks.iter().zip(&lane.blocks).enumerate() {
            let Some(a) = b.analysis.as_ref().filter(|a| {
                a.playback == crate::offline::Playback::Signal
                    && a.state != crate::offline::State::None
            }) else {
                continue;
            };
            for (param, curve) in &a.lanes {
                if let Some(pi) = block.params().index(param).filter(|_| !curve.is_empty()) {
                    played.push(crate::offline::PlayedLane {
                        lane: li,
                        block: bi,
                        param: pi,
                        curve: curve.clone(),
                        cursor: crate::offline::Cursor::default(),
                    });
                }
            }
        }
    }
    crate::offline::SelfPlay {
        lanes: played,
        expected: None,
        sample_rate: f64::from(sample_rate),
    }
}

/// Delay compensation: how late each lane's audio is (its feeds' latest,
/// then its chain's latency), and the delays that line every path into a
/// lane, and every lane to the outputs, up with the latest. Returns the
/// outputs' latency.
fn compensate(lanes: &mut [Lane], order: &[usize]) -> u32 {
    let n = lanes.len();
    let chain: Vec<usize> = lanes
        .iter()
        .map(|l| {
            let frames = l
                .blocks
                .iter()
                .filter(|b| !b.bypass)
                .map(ChainBlock::latency)
                .fold(0_u32, u32::saturating_add);
            usize::try_from(frames).unwrap_or(0)
        })
        .collect();
    // How late a lane's input is, and its output.
    let mut input = vec![0_usize; n];
    let mut output = vec![0_usize; n];
    for &i in order {
        let late = input.get(i).copied().unwrap_or(0);
        output[i] = late + chain.get(i).copied().unwrap_or(0);
        let lane = &lanes[i];
        let mut feed = |to: usize, at: usize| {
            if let Some(x) = input.get_mut(to) {
                *x = (*x).max(at);
            }
        };
        if let Some(t) = lane.to {
            feed(t, output[i]);
        }
        for s in &lane.sends {
            feed(s.to, if s.pre { late } else { output[i] });
        }
    }
    let heard = |l: &Lane| l.to.is_none() && !l.mute;
    let latest = lanes
        .iter()
        .enumerate()
        .filter(|(_, l)| heard(l))
        .map(|(i, _)| output[i])
        .max()
        .unwrap_or(0);
    for i in 0..n {
        let (late, out) = (input[i], output[i]);
        let to_late = lanes[i].to.map(|t| input[t]);
        let sends: Vec<usize> = lanes[i]
            .sends
            .iter()
            .map(|s| input[s.to] - if s.pre { late } else { out })
            .collect();
        let lane = &mut lanes[i];
        if !lane.input.is_empty() && late > 0 {
            lane.in_delay = Delay::new(late);
        }
        if let Some(t) = to_late {
            lane.to_delay = Delay::new(t - out);
        } else if heard(lane) {
            lane.out_delay = Delay::new(latest - out);
        }
        for (s, d) in lane.sends.iter_mut().zip(sends) {
            s.delay = Delay::new(d);
        }
    }
    u32::try_from(latest).unwrap_or(u32::MAX)
}

/// How far a far trigger lane's detector looks back for a hit's onset:
/// its default confirmation window (1 ms) twice, as `HitDetector` counts
/// it.
fn remote_lag(sample_rate: f32) -> usize {
    signal_dsp_core::num::f64_to_index(f64::from(sample_rate) * 0.001).saturating_mul(2)
}

/// The order lanes run in (Kahn's): a lane after every lane that feeds
/// it, sends to it or keys it. A loop can't run: its edges are dropped,
/// with a warning, and its lanes go to the outputs.
fn run_order(lanes: &mut [Lane], specs: &[&LaneSpec], warnings: &mut Vec<String>) -> Vec<usize> {
    let n = lanes.len();
    let edges = |lanes: &[Lane]| -> Vec<(usize, usize)> {
        let mut e = Vec::new();
        for (i, lane) in lanes.iter().enumerate() {
            if let Some(t) = lane.to {
                e.push((i, t));
            }
            e.extend(lane.sends.iter().map(|s| (i, s.to)));
            e.extend(
                lane.keys
                    .iter()
                    .filter(|(_, k)| *k != i)
                    .map(|(_, k)| (*k, i)),
            );
        }
        e
    };
    let mut order = Vec::with_capacity(n);
    let mut done = vec![false; n];
    loop {
        let e = edges(lanes);
        let mut progressed = false;
        for i in 0..n {
            if done.get(i).copied().unwrap_or(true) {
                continue;
            }
            let blocked = e
                .iter()
                .any(|(from, to)| *to == i && !done.get(*from).copied().unwrap_or(true));
            if !blocked {
                if let Some(d) = done.get_mut(i) {
                    *d = true;
                }
                order.push(i);
                progressed = true;
            }
        }
        if order.len() == n {
            return order;
        }
        if !progressed {
            // A loop: cut every edge into a lane still waiting.
            let waiting: Vec<usize> = (0..n)
                .filter(|i| !done.get(*i).copied().unwrap_or(true))
                .collect();
            for &i in &waiting {
                let name = specs.get(i).map_or("?", |s| s.name.as_str());
                warnings.push(alloc::format!(
                    "{name}: in a routing loop; its routing is dropped"
                ));
                if let Some(lane) = lanes.get_mut(i) {
                    lane.to = None;
                    lane.sends.clear();
                    lane.keys.clear();
                }
            }
        }
    }
}

fn log_usage(spec: &BlockSpec, status: &BlockStatus) {
    // Which third-party blocks presets use: a block that keeps showing up
    // is the next one Signal builds itself.
    if let BlockKind::Plugin(p) = &spec.kind {
        tracing::info!(
            target: "signal::usage",
            plugin_title = %p.name,
            plugin_id = %p.id,
            plugin_format = ?p.format,
            plugin_version = p.version.as_deref().unwrap_or(""),
            block_status = ?status,
            "third-party block in a preset"
        );
    }
}

fn try_build(
    spec: &BlockSpec,
    cx: &BuildContext<'_>,
    factories: &[&dyn BlockFactory],
) -> Result<Built, Unavailable> {
    let required = spec.requires.unwrap_or(Platforms::ALL);
    let Some((factory, runs_on)) = factories
        .iter()
        .find_map(|f| f.runs_on(&spec.kind).map(|p| (*f, p)))
    else {
        return Err(match &spec.kind {
            // No host for this format in this build: say where it would run.
            BlockKind::Plugin(p) if !p.format.platforms().intersection(required).here() => {
                Unavailable::Platform {
                    needs: p.format.platforms().intersection(required),
                }
            }
            _ => Unavailable::UnknownKind,
        });
    };
    let needs = runs_on.intersection(required);
    if !needs.here() {
        return Err(Unavailable::Platform { needs });
    }
    factory.build(spec, cx)
}

fn build_block(
    spec: &BlockSpec,
    cx: &BuildContext<'_>,
    factories: &[&dyn BlockFactory],
    warnings: &mut Vec<String>,
    lane: &str,
) -> ChainBlock {
    let (built, status, params_of) = match try_build(spec, cx, factories) {
        Ok(b) => (b, BlockStatus::Active, spec),
        Err(reason) => match spec
            .fallback
            .as_deref()
            .map(|fb| (fb, try_build(fb, cx, factories)))
        {
            Some((fb, Ok(b))) => (
                b,
                BlockStatus::Fallback {
                    reason,
                    fallback: fb.kind.clone(),
                },
                fb,
            ),
            _ => {
                tracing::warn!(block.lane = lane, block.id = %spec.id, block.kind = %spec.kind, block.reason = %reason, "block bypassed");
                let pass = Built {
                    backend: Backend::Linked(Box::new(Passthrough)),
                    params: &[],
                    sources: &[],
                };
                (pass, BlockStatus::Bypassed { reason }, spec)
            }
        },
    };
    let bank = ParamBank::new(built.params);
    let mut values = Vec::new();
    for (id, value) in &params_of.params {
        match bank.index(id) {
            Some(i) => values.push((i, *value)),
            None if !matches!(status, BlockStatus::Bypassed { .. }) => {
                warnings.push(alloc::format!("{lane}/{}: no parameter {id}", spec.id));
            }
            None => {}
        }
    }
    let mut block = ChainBlock::new(
        spec.id.clone(),
        params_of.kind.clone(),
        bank,
        built.sources,
        built.backend,
        status,
    );
    for (i, v) in values {
        block.apply(i, ParamChange::Set(v));
    }
    block.bypass = spec.bypass;
    // A third-party block's own state (`spec.state`) is the CLAP
    // factory's to load: it built the instance from `spec`.
    block.reset();
    block
}

impl Graph {
    /// Make this graph a port of `session`: its trigger lanes' hits are
    /// published there, and its blocks keyed to another port's lane play
    /// that lane's hits from it. Control thread, before it is installed.
    pub fn join(&mut self, session: alloc::sync::Arc<Session>) {
        self.cursor = Some(session.hits.cursor());
        self.session = Some(session);
    }

    /// Blocks keyed to another port's lane, which play only in a session.
    #[must_use]
    pub fn remote_keys(&self) -> usize {
        self.remote_keys.len()
    }

    /// The song position of the next piece, in frames: the session's
    /// clock, which every port's hits are stamped with. The processor
    /// sets it from the host's transport; between, it counts.
    pub const fn set_clock(&mut self, frame: u64) {
        self.clock = frame;
    }

    /// Frames from the next piece's start to the next breakpoint of a lane
    /// Signal plays itself: where the processor ends a piece, so each of
    /// the apply stage's ramps is one of the lane's segments, exactly.
    #[must_use]
    pub fn frames_to_next_offline(&self) -> Option<usize> {
        self.self_play
            .frames_to_next(self.clock.saturating_sub(self.lookahead as u64))
    }

    /// Block `(lane, block)` takes its parameters' new values now rather
    /// than ramping to them over the next piece: what a host's automation
    /// of an offline block's parameter means (its value lands at the start
    /// of the host's block). Allocation-free.
    pub fn settle(&mut self, lane: usize, block: usize) {
        if let Some(b) = self
            .lanes
            .get_mut(lane)
            .and_then(|l| l.blocks.get_mut(block))
        {
            b.process(&mut [], &mut []);
        }
    }

    /// Whether block `(lane, block)` is an offline block.
    #[must_use]
    pub fn is_offline(&self, lane: usize, block: usize) -> bool {
        self.block(lane, block).is_some_and(|b| match &b.kind {
            BlockKind::Signal(n) => crate::offline::mode_of(n) == crate::offline::Mode::Offline,
            BlockKind::Plugin(_) => false,
        })
    }

    /// How many lanes Signal plays itself.
    #[must_use]
    pub fn self_played_lanes(&self) -> usize {
        self.self_play.lanes.len()
    }

    /// Capture the audio arriving at block `block` of lane `lane` into
    /// `ring` from now on (control side, before the graph is installed).
    pub fn tap(
        &mut self,
        lane: usize,
        block: usize,
        ring: alloc::sync::Arc<crate::offline::capture::CaptureRing>,
    ) {
        self.taps.retain(|(l, b, _)| (*l, *b) != (lane, block));
        self.taps.push((lane, block, ring));
    }

    /// Set the self-played lanes' parameters for the piece from song frame
    /// `clock` (the lanes' clock) of `len` frames: each ramps to its
    /// value at the piece's end. After a seek (or at the start) each first
    /// jumps to its value at the piece's start. Allocation-free.
    fn play_offline(&mut self, clock: u64, len: usize) {
        let sp = &mut self.self_play;
        if sp.lanes.is_empty() || sp.sample_rate <= 0.0 {
            return;
        }
        let jumped = sp.expected != Some(clock);
        sp.expected = Some(clock.saturating_add(len as u64));
        #[allow(clippy::cast_precision_loss)]
        let (t0, t1) = (
            clock as f64 / sp.sample_rate,
            clock.saturating_add(len as u64) as f64 / sp.sample_rate,
        );
        for pl in &mut sp.lanes {
            let Some(block) = self
                .lanes
                .get_mut(pl.lane)
                .and_then(|l| l.blocks.get_mut(pl.block))
            else {
                continue;
            };
            if jumped {
                if let Some(v) = pl.cursor.value(&pl.curve, t0) {
                    block.apply(pl.param, ParamChange::Set(v));
                    block.process(&mut [], &mut []);
                }
            }
            if let Some(v) = pl.cursor.value(&pl.curve, t1) {
                block.apply(pl.param, ParamChange::Set(v));
            }
        }
    }

    /// What each lane's mix phases do to its level since the meters were
    /// last cleared: the measure a phase's gain-neutral make-up is set
    /// from ([`PhaseLevel::neutral_trim_db`]). Control side, or offline.
    #[must_use]
    pub fn phase_levels(&self) -> Vec<PhaseLevel> {
        let db = |e: f64| -> f32 {
            if e <= 0.0 {
                -200.0
            } else {
                signal_dsp_core::num::narrow(10.0 * libm::log10(e))
            }
        };
        self.lanes
            .iter()
            .flat_map(|lane| {
                lane.phases.iter().map(move |g| PhaseLevel {
                    lane: lane.name.clone(),
                    phase: g.phase,
                    in_db: db(g.energy_in),
                    out_db: db(g.energy_out),
                    trim_db: crate::util::gain_to_db(g.trim),
                })
            })
            .collect()
    }

    /// Make every phase gain neutral on what it has measured: each phase
    /// that has sounded gets the make-up that brings it back to its input's
    /// level (at most ±[`MAX_TRIM_DB`]). On the audio thread, as the
    /// song plays: an instance calibrating itself on its own input.
    /// Allocates nothing.
    pub fn calibrate_phases(&mut self) {
        let db = |e: f64| 10.0 * libm::log10(e);
        for g in self.lanes.iter_mut().flat_map(|l| l.phases.iter_mut()) {
            if g.energy_in > 0.0 && g.energy_out > 0.0 {
                let trim = signal_dsp_core::num::narrow(db(g.energy_in) - db(g.energy_out));
                g.trim = crate::util::db_to_gain(trim.clamp(-MAX_TRIM_DB, MAX_TRIM_DB));
            }
        }
    }

    /// What keys block `block` of lane `lane` now, when another lane keys
    /// it.
    #[must_use]
    pub fn key_via(&self, lane: usize, block: usize) -> Option<KeyVia> {
        if let Some(i) = self
            .remote_keys
            .iter()
            .position(|k| k.0 == lane && k.1 == block)
        {
            let piece = self.remote_keys[i].2;
            if self.placed.iter().any(|(p, known)| *p == piece && *known) {
                return Some(KeyVia::Placed);
            }
            let (frame, midi) = self.key_last.get(i).copied().unwrap_or((0, false));
            return Some(
                if frame == 0 || self.clock.saturating_sub(frame) > self.ten_seconds {
                    KeyVia::Own
                } else if midi {
                    KeyVia::Midi
                } else {
                    KeyVia::Hits
                },
            );
        }
        self.lanes
            .get(lane)?
            .keys
            .iter()
            .any(|(b, _)| *b == block)
            .then_some(KeyVia::Hits)
    }

    /// Each phase's output over the last piece: `each(lane, phase,
    /// level_db)`, K-weighted (loudness-like). Allocates nothing.
    pub fn each_phase_level(&self, mut each: impl FnMut(usize, Phase, f32)) {
        for (l, lane) in self.lanes.iter().enumerate() {
            for g in &lane.phases {
                let db = if g.level > 0.0 {
                    10.0 * libm::log10(g.level)
                } else {
                    -200.0
                };
                each(l, g.phase, signal_dsp_core::num::narrow(db));
            }
        }
    }

    /// Each phase's make-up now: `each(lane, phase, gain_db)`. Allocates
    /// nothing.
    pub fn each_trim(&self, mut each: impl FnMut(usize, Phase, f32)) {
        for (l, lane) in self.lanes.iter().enumerate() {
            for g in &lane.phases {
                each(l, g.phase, crate::util::gain_to_db(g.trim));
            }
        }
    }

    /// Start the phase meters again.
    pub fn clear_phase_levels(&mut self) {
        for g in self.lanes.iter_mut().flat_map(|l| l.phases.iter_mut()) {
            g.energy_in = 0.0;
            g.energy_out = 0.0;
        }
    }

    /// Frames of latency: the longest lane's, and the trigger lanes'
    /// lookahead.
    #[must_use]
    pub const fn latency(&self) -> u32 {
        self.latency
    }

    /// The longest piece [`process_piece`](Self::process_piece) takes.
    #[must_use]
    pub const fn max_block(&self) -> usize {
        self.max_block
    }

    /// The block at `lane`, `block`.
    #[must_use]
    pub fn block(&self, lane: usize, block: usize) -> Option<&ChainBlock> {
        self.lanes.get(lane)?.blocks.get(block)
    }

    /// How many lanes, and how many blocks lane `lane` has.
    #[must_use]
    pub fn lane_count(&self) -> usize {
        self.lanes.len()
    }

    #[must_use]
    pub fn block_count(&self, lane: usize) -> usize {
        self.lanes.get(lane).map_or(0, |l| l.blocks.len())
    }

    /// Lane `lane`'s name (its path in the kit's tree).
    #[must_use]
    pub fn lane_name(&self, lane: usize) -> Option<&str> {
        self.lanes.get(lane).map(|l| l.name.as_str())
    }

    /// The mix phase of block `block` of lane `lane`.
    #[must_use]
    pub fn block_phase(&self, lane: usize, block: usize) -> Option<Phase> {
        self.lanes
            .get(lane)?
            .phases
            .iter()
            .find(|g| (g.first..=g.last).contains(&block))
            .map(|g| g.phase)
    }

    /// Where every block of `kind` is: what an override ("every instance of
    /// that block switches to its CLAP") swaps.
    #[must_use]
    pub fn blocks_of(&self, kind: &BlockKind) -> Vec<(usize, usize)> {
        self.lanes
            .iter()
            .enumerate()
            .flat_map(|(l, lane)| {
                lane.blocks
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| &b.kind == kind)
                    .map(move |(b, _)| (l, b))
            })
            .collect()
    }

    /// Hand a MIDI event to every block. A note-on of a lane's trigger
    /// note is a hit of that lane, at the start of the next piece (the
    /// processor splits a block at every MIDI event).
    pub fn midi(&mut self, event: Midi) {
        // Notes that are a piece's hits: to the session, stamped now, and to
        // the piece's lane when it's here.
        if let Midi::NoteOn { note, velocity, .. } = event {
            let v = f32::from(velocity) / 127.0;
            let mut local: [Option<usize>; 8] = [None; 8];
            let mut n = 0;
            // Each piece once, however many lanes map the note to it.
            let mut published: [u16; 8] = [0; 8];
            let mut np = 0;
            for lane in &self.lanes {
                for (_, piece, here) in lane.midi_map.iter().filter(|m| m.0 == note) {
                    // A piece here is published by its own lane, with its
                    // other hits.
                    let again = published.iter().take(np).any(|p| p == piece);
                    if let (Some(session), None, false) = (self.session.as_ref(), here, again) {
                        session.hits.publish(SessionHit {
                            piece: *piece,
                            velocity: v,
                            frame: self.clock,
                            midi: true,
                        });
                        if let Some(slot) = published.get_mut(np) {
                            *slot = *piece;
                            np += 1;
                        }
                    }
                    if let (Some(j), Some(slot)) = (here, local.get_mut(n)) {
                        *slot = Some(*j);
                        n += 1;
                    }
                }
            }
            for j in local.iter().take(n).flatten() {
                if let Some(lane) = self.lanes.get_mut(*j) {
                    if let Some(slot) = lane.midi_hits.get_mut(lane.midi_hit_count) {
                        *slot = Hit {
                            offset: 0,
                            velocity: v,
                            midi: true,
                        };
                        lane.midi_hit_count += 1;
                    }
                }
            }
        }
        for lane in &mut self.lanes {
            if let (
                Some(note),
                Midi::NoteOn {
                    note: n, velocity, ..
                },
            ) = (lane.note, event)
            {
                if note == n {
                    if let Some(slot) = lane.midi_hits.get_mut(lane.midi_hit_count) {
                        *slot = Hit {
                            offset: 0,
                            velocity: f32::from(velocity) / 127.0,
                            midi: true,
                        };
                        lane.midi_hit_count += 1;
                    }
                }
            }
            for block in &mut lane.blocks {
                block.midi(event);
            }
        }
    }

    /// Change a block's parameter, by index: lands before the next piece.
    pub fn apply(&mut self, lane: usize, block: usize, param: usize, change: ParamChange) {
        if let Some(b) = self
            .lanes
            .get_mut(lane)
            .and_then(|l| l.blocks.get_mut(block))
        {
            b.apply(param, change);
        }
    }

    /// Swap a block's backend at this block boundary (see
    /// [`ChainBlock::swap_backend`]). Returns the old one, or `backend`
    /// back if there is no such block.
    pub fn swap_backend(&mut self, lane: usize, block: usize, backend: Backend) -> Backend {
        match self
            .lanes
            .get_mut(lane)
            .and_then(|l| l.blocks.get_mut(block))
        {
            Some(b) => b.swap_backend(backend),
            None => backend,
        }
    }

    /// Move macro `index` to `value` (0–1).
    pub fn apply_macro(&mut self, index: usize, value: f32) {
        let Some(bindings) = self.macros.get_mut(index) else {
            return;
        };
        let v = value.clamp(0.0, 1.0);
        for b in bindings {
            let Some(block) = self
                .lanes
                .get_mut(b.lane)
                .and_then(|l| l.blocks.get_mut(b.block))
            else {
                continue;
            };
            let target = v * (b.max - b.min) + b.min;
            match b.mode {
                MacroMode::Set => block.apply(b.param, ParamChange::Set(target)),
                MacroMode::Modulate => {
                    let delta = target - b.applied;
                    b.applied = target;
                    block.apply(b.param, ParamChange::Modulate(delta));
                }
            }
        }
    }

    /// Clear every block's state.
    pub fn reset(&mut self) {
        for lane in &mut self.lanes {
            for block in &mut lane.blocks {
                block.reset();
            }
            #[cfg(feature = "fx")]
            if let Some(d) = lane.detector.as_mut() {
                d.reset();
            }
            lane.hit_count = 0;
            lane.midi_hit_count = 0;
            lane.later_count = 0;
            lane.in_delay.clear();
            lane.to_delay.clear();
            lane.out_delay.clear();
            for s in &mut lane.sends {
                s.delay.clear();
            }
        }
        for d in &mut self.delayed {
            d.fill(0.0);
        }
        // As built: no hit from before the reset is heard after it (those
        // the session's ports published and this one hadn't read yet are
        // skipped), and nothing remembers when the last ones came.
        self.remote_count = 0;
        if let (Some(session), Some(cursor)) = (self.session.as_ref(), self.cursor.as_mut()) {
            *cursor = session.hits.cursor();
        }
        self.key_last.fill((0, false));
        for m in &mut self.piece_midi {
            m.1 = 0;
        }
        for t in &mut self.piece_taken {
            t.1 = None;
        }
    }

    /// Process `start..start + len` of `io` in place (`len` ≤
    /// [`max_block`](Self::max_block)): on entry its first `in_channels`
    /// hold the input, on exit every channel holds the output.
    pub fn process_piece(
        &mut self,
        io: &mut [&mut [f32]],
        in_channels: usize,
        start: usize,
        len: usize,
        in_gain: f32,
    ) {
        let len = len.min(self.max_block);
        let end = start.saturating_add(len);
        for (ch, copy) in self.input_copy.iter_mut().enumerate() {
            let dst = copy.get_mut(..len);
            let src = if ch < in_channels {
                io.get(ch).and_then(|c| c.get(start..end))
            } else {
                None
            };
            match (dst, src) {
                (Some(d), Some(s)) => d.iter_mut().zip(s).for_each(|(d, s)| *d = *s * in_gain),
                (Some(d), None) => d.fill(0.0),
                _ => {}
            }
        }
        // The lanes' inputs, `lookahead` late: last piece's tail, then
        // this piece, read from the front, the new tail kept.
        let la = self.lookahead;
        for (d, raw) in self.delayed.iter_mut().zip(&self.input_copy) {
            if let (Some(dst), Some(src)) = (d.get_mut(la..la + len), raw.get(..len)) {
                dst.copy_from_slice(src);
            }
        }
        for ch in io.iter_mut() {
            if let Some(s) = ch.get_mut(start..end) {
                s.fill(0.0);
            }
        }
        for lane in &mut self.lanes {
            if let (Some(l), Some(r)) = (lane.in_l.get_mut(..len), lane.in_r.get_mut(..len)) {
                l.fill(0.0);
                r.fill(0.0);
            }
        }
        self.take_remote_hits();
        self.take_planned_hits(self.clock.saturating_sub(self.lookahead as u64), len);
        self.play_offline(self.clock.saturating_sub(self.lookahead as u64), len);
        let input_copy = &self.input_copy;
        let lanes_in = if la == 0 {
            &self.input_copy
        } else {
            &self.delayed
        };
        let channel =
            |c: Option<&usize>| c.and_then(|c| lanes_in.get(*c)).and_then(|b| b.get(..len));
        // What a trigger lane's detector hears: the input on time.
        let on_time = |c: Option<&usize>| {
            c.and_then(|c| input_copy.get(*c))
                .and_then(|b| b.get(..len))
        };
        for oi in 0..self.order.len() {
            let Some(&i) = self.order.get(oi) else {
                continue;
            };
            let Some(lane) = self.lanes.get_mut(i) else {
                continue;
            };
            {
                let (Some(l), Some(r)) = (lane.l.get_mut(..len), lane.r.get_mut(..len)) else {
                    continue;
                };
                match (channel(lane.input.first()), channel(lane.input.get(1))) {
                    (Some(a), Some(b)) => {
                        l.copy_from_slice(a);
                        r.copy_from_slice(b);
                    }
                    (Some(a), None) => {
                        l.copy_from_slice(a);
                        r.copy_from_slice(a);
                    }
                    _ => {
                        l.fill(0.0);
                        r.fill(0.0);
                    }
                }
                // As late as what else feeds the lane.
                lane.in_delay.run(l, r);
                // What was routed here: a bus's sum.
                for (d, s) in l.iter_mut().zip(&lane.in_l) {
                    *d += s;
                }
                for (d, s) in r.iter_mut().zip(&lane.in_r) {
                    *d += s;
                }
                if let (Some(pl), Some(pr)) = (lane.pre_l.get_mut(..len), lane.pre_r.get_mut(..len))
                {
                    pl.copy_from_slice(l);
                    pr.copy_from_slice(r);
                }
            }
            // This lane's hits: those an earlier piece placed in this one,
            // then from MIDI, then from its trigger. Each is published to
            // the session at its song frame as it's found, and heard in the
            // piece it falls in: where a hit lands is the song's, not where
            // a host's block happens to end.
            let heard = self.clock.saturating_sub(la as u64);
            lane.hit_count = 0;
            take_later(lane, heard, len);
            let midi = core::mem::take(&mut lane.midi_hit_count);
            for k in 0..midi {
                if let Some(h) = lane.midi_hits.get(k).copied() {
                    // A MIDI hit is on time: where the lanes hear it.
                    let at = u64::from(h.offset).saturating_add(la as u64);
                    place(
                        lane,
                        self.session.as_deref(),
                        heard,
                        len,
                        at,
                        h.velocity,
                        true,
                    );
                }
            }
            #[cfg(feature = "fx")]
            if lane.detector.is_some() {
                // The lane's host input on time (`lookahead` ahead of what
                // the lanes hear); a lane fed only by others, its input.
                let pre = (
                    lane.pre_l.get(..len).unwrap_or(&[]),
                    lane.pre_r.get(..len).unwrap_or(&[]),
                );
                let (dl, dr, ahead) =
                    match (on_time(lane.input.first()), on_time(lane.input.get(1))) {
                        (Some(a), Some(b)) => (a, b, true),
                        (Some(a), None) => (a, a, true),
                        _ => (pre.0, pre.1, false),
                    };
                // Detected `lag` after its onset, heard `lookahead` late:
                // the onset is `lookahead - lag` after the detection. Without
                // the lookahead, `lag` back, clamped to the piece.
                let ahead = if ahead { la } else { 0 };
                let mut found = [(0_u64, 0.0_f32); MAX_HITS];
                let mut n = 0;
                if let Some(det) = lane.detector.as_mut() {
                    for h in det.detect(dl, dr) {
                        if let Some(slot) = found.get_mut(n) {
                            let lag = u64::from(h.lag);
                            *slot = (
                                (u64::from(h.offset) + ahead as u64).saturating_sub(lag),
                                h.velocity,
                            );
                            n += 1;
                        }
                    }
                }
                for &(at, velocity) in found.iter().take(n) {
                    place(
                        lane,
                        self.session.as_deref(),
                        heard,
                        len,
                        at,
                        velocity,
                        false,
                    );
                }
            }
            // Blocks keyed to a lane: its pre-chain audio, and its hits.
            for ki in 0..self.lanes.get(i).map_or(0, |l| l.keys.len()) {
                let Some(&(b, k)) = self.lanes.get(i).and_then(|l| l.keys.get(ki)) else {
                    continue;
                };
                if k == i {
                    let Some(lane) = self.lanes.get_mut(i) else {
                        continue;
                    };
                    let (hits, count) = (lane.hits, lane.hit_count);
                    let (pl, pr) = (
                        core::mem::take(&mut lane.pre_l),
                        core::mem::take(&mut lane.pre_r),
                    );
                    if let Some(block) = lane.blocks.get_mut(b) {
                        block.set_key(pl.get(..len).unwrap_or(&[]), pr.get(..len).unwrap_or(&[]));
                        for h in hits.iter().take(count) {
                            block.hit(h.offset, h.velocity);
                        }
                    }
                    lane.pre_l = pl;
                    lane.pre_r = pr;
                } else if let Some((lane, key)) = pair_mut(&mut self.lanes, i, k) {
                    if let Some(block) = lane.blocks.get_mut(b) {
                        block.set_key(
                            key.pre_l.get(..len).unwrap_or(&[]),
                            key.pre_r.get(..len).unwrap_or(&[]),
                        );
                        for h in key.hits.iter().take(key.hit_count) {
                            block.hit(h.offset, h.velocity);
                        }
                    }
                }
            }
            give_remote_hits(
                &mut self.lanes,
                &self.remote_keys,
                &mut self.key_last,
                self.remote.get(..self.remote_count).unwrap_or(&[]),
                self.clock.saturating_sub(la as u64),
                i,
                len,
            );
            let Some(lane) = self.lanes.get_mut(i) else {
                continue;
            };
            {
                let (Some(l), Some(r)) = (lane.l.get_mut(..len), lane.r.get_mut(..len)) else {
                    continue;
                };
                let mut group = 0;
                for (b, block) in lane.blocks.iter_mut().enumerate() {
                    let off = lane
                        .phases
                        .get(group)
                        .is_some_and(|g| g.bypass && (g.first..=g.last).contains(&b));
                    if let Some(g) = lane.phases.get_mut(group).filter(|g| g.first == b) {
                        g.piece_in = g.k_in.energy(l, r);
                    }
                    if let Some((_, _, ring)) =
                        self.taps.iter().find(|(tl, tb, _)| *tl == i && *tb == b)
                    {
                        ring.push(self.clock.saturating_sub(la as u64), l, r);
                    }
                    if !off {
                        block.process(l, r);
                    }
                    if let Some(g) = lane.phases.get_mut(group).filter(|g| g.last == b) {
                        let out = g.k_out.energy(l, r);
                        g.level = out / (l.len() + r.len()).max(1) as f64;
                        if out > SOUNDING * (l.len() + r.len()) as f64 && !off {
                            g.energy_in += g.piece_in;
                            g.energy_out += out;
                        }
                        // Gain neutral: the phase's make-up.
                        if g.trim != 1.0 && !off {
                            let t = g.trim;
                            l.iter_mut().chain(r.iter_mut()).for_each(|s| *s *= t);
                        }
                        group += 1;
                    }
                }
            }
            if lane.mute {
                continue;
            }
            let g = lane.gain;
            // Sends, then where the lane goes.
            for si in 0..lane.sends.len() {
                let Some(t) = self
                    .lanes
                    .get(i)
                    .and_then(|l| l.sends.get(si))
                    .map(|s| s.to)
                else {
                    continue;
                };
                if let Some((src, dst)) = pair_mut(&mut self.lanes, i, t) {
                    let Lane {
                        sends,
                        pre_l,
                        pre_r,
                        l,
                        r,
                        ..
                    } = src;
                    let Some(send) = sends.get_mut(si) else {
                        continue;
                    };
                    // Pre: before the chain and the lane's fader; post:
                    // after both.
                    let (sl, sr, k) = if send.pre {
                        (pre_l, pre_r, send.gain)
                    } else {
                        (l, r, send.gain * g)
                    };
                    if let (Some(sl), Some(sr), Some(dl), Some(dr)) = (
                        sl.get(..len),
                        sr.get(..len),
                        dst.in_l.get_mut(..len),
                        dst.in_r.get_mut(..len),
                    ) {
                        send.delay.add((sl, sr), (dl, dr), k);
                    }
                }
            }
            let to = self.lanes.get(i).and_then(|l| l.to);
            if let Some(t) = to {
                if let Some((src, dst)) = pair_mut(&mut self.lanes, i, t) {
                    if let (Some(sl), Some(sr), Some(dl), Some(dr)) = (
                        src.l.get(..len),
                        src.r.get(..len),
                        dst.in_l.get_mut(..len),
                        dst.in_r.get_mut(..len),
                    ) {
                        src.to_delay.add((sl, sr), (dl, dr), g);
                    }
                }
                continue;
            }
            let Some(lane) = self.lanes.get_mut(i) else {
                continue;
            };
            let (Some(l), Some(r)) = (lane.l.get_mut(..len), lane.r.get_mut(..len)) else {
                continue;
            };
            // As late as the latest lane to the outputs.
            lane.out_delay.run(l, r);
            let (l, r) = (&*l, &*r);
            let right_out = lane.output.saturating_add(1);
            let has_right = io.get(right_out).is_some_and(|c| c.len() >= end);
            if let Some(out) = io.get_mut(lane.output).and_then(|c| c.get_mut(start..end)) {
                if has_right {
                    out.iter_mut().zip(l.iter()).for_each(|(o, s)| *o += s * g);
                } else {
                    // One channel out: the lane folded to mono.
                    out.iter_mut()
                        .zip(l.iter().zip(r.iter()))
                        .for_each(|(o, (a, b))| *o += (a + b) * 0.5 * g);
                }
            }
            if has_right {
                if let Some(out) = io.get_mut(right_out).and_then(|c| c.get_mut(start..end)) {
                    out.iter_mut().zip(r.iter()).for_each(|(o, s)| *o += s * g);
                }
            }
        }
        // Keep the last `lookahead` frames for the next piece.
        for d in &mut self.delayed {
            d.copy_within(len..len + la, 0);
        }
        // Remote hits this piece played, or that came too late to.
        let heard_end = self
            .clock
            .saturating_sub(la as u64)
            .saturating_add(len as u64);
        let mut kept = 0;
        for k in 0..self.remote_count {
            if let Some(h) = self.remote.get(k).copied() {
                if h.1 >= heard_end {
                    if let Some(slot) = self.remote.get_mut(kept) {
                        *slot = h;
                    }
                    kept += 1;
                }
            }
        }
        self.remote_count = kept;
        self.clock = self.clock.saturating_add(len as u64);
    }

    /// Piece `piece`'s planned hits: `hits` in, the ones they replace back
    /// (to drop off the audio thread). A piece no key here follows hands
    /// `hits` back.
    ///
    /// The host placed every hit the piece has, `hits` empty too (a piece
    /// the song never plays, or whose every hit the host holds for bleed):
    /// the blocks the piece keys are told its hits are known, so a gate
    /// waits through a gap the song has — however long, the whole song —
    /// rather than falling back to its own input and opening on the bleed.
    /// A trigger's detected hits still key it while the list is empty.
    pub fn set_planned_hits(&mut self, piece: u16, hits: Box<[(u64, f32)]>) -> Box<[(u64, f32)]> {
        let Some((_, slot)) = self.planned.iter_mut().find(|(p, _)| *p == piece) else {
            return hits;
        };
        let old = core::mem::replace(slot, hits);
        if let Some((_, known)) = self.placed.iter_mut().find(|(p, _)| *p == piece) {
            *known = true;
        }
        for (l, b, _) in self.remote_keys.iter().filter(|k| k.2 == piece) {
            if let Some(block) = self.lanes.get_mut(*l).and_then(|x| x.blocks.get_mut(*b)) {
                block.hits_known(true);
            }
        }
        old
    }

    /// The planned hits this piece hears (`heard..heard + len` of the
    /// song), as notes: with the session's, for the remote keys.
    fn take_planned_hits(&mut self, heard: u64, len: usize) {
        let end = heard.saturating_add(len as u64);
        for (piece, hits) in &self.planned {
            let from = hits.partition_point(|h| h.0 < heard);
            for &(frame, velocity) in hits.iter().skip(from).take_while(|h| h.0 < end) {
                if let Some(slot) = self.remote.get_mut(self.remote_count) {
                    *slot = (*piece, frame, velocity, true);
                    self.remote_count += 1;
                }
                if let Some(m) = self.piece_midi.iter_mut().find(|(p, _)| p == piece) {
                    m.1 = frame.max(1);
                }
            }
        }
    }

    /// Take the hits the session's other ports published since the last
    /// piece, for the pieces this graph's remote keys follow.
    fn take_remote_hits(&mut self) {
        let (Some(session), Some(cursor)) = (self.session.as_ref(), self.cursor.as_mut()) else {
            return;
        };
        let (keys, remote, count) = (&self.remote_keys, &mut self.remote, &mut self.remote_count);
        let (piece_midi, ten, planned) = (&mut self.piece_midi, self.ten_seconds, &self.planned);
        let taken = &mut self.piece_taken;
        session.hits.drain(cursor, |h| {
            if !keys.iter().any(|(_, _, p)| *p == h.piece) {
                return;
            }
            // A piece whose hits the host has placed plays those: the same
            // hits as its notes, which would play it twice.
            if planned
                .iter()
                .any(|(p, list)| *p == h.piece && !list.is_empty())
            {
                return;
            }
            // A piece with notes is keyed by them: its trigger's detected
            // hits are the fallback, for when the notes stop.
            if let Some(m) = piece_midi.iter_mut().find(|(p, _)| *p == h.piece) {
                if h.midi {
                    m.1 = h.frame.max(1);
                } else if m.1 > 0 && h.frame < m.1.saturating_add(ten) {
                    return;
                }
            }
            // One strike, published twice (two ports with the piece's
            // notes): the first copy plays.
            if let Some(t) = taken.iter_mut().find(|(p, _)| *p == h.piece) {
                if t.1.is_some_and(|last| h.frame <= last) {
                    return;
                }
                t.1 = Some(h.frame);
            }
            if let Some(slot) = remote.get_mut(*count) {
                *slot = (h.piece, h.frame, h.velocity, h.midi);
                *count += 1;
            }
        });
    }
}

/// A hit found in the piece that hears `heard..heard + len`, `at` frames
/// in (past its end when it is placed later): published to the session at
/// its song frame, and heard in the piece it falls in.
fn place(
    lane: &mut Lane,
    session: Option<&Session>,
    heard: u64,
    len: usize,
    at: u64,
    velocity: f32,
    midi: bool,
) {
    let frame = heard.saturating_add(at);
    if let Some(session) = session {
        session.hits.publish(SessionHit {
            piece: lane.piece,
            velocity,
            frame,
            midi,
        });
    }
    if at < len as u64 {
        if let Some(slot) = lane.hits.get_mut(lane.hit_count) {
            *slot = Hit {
                offset: u32::try_from(at).unwrap_or(0),
                velocity,
                midi,
            };
            lane.hit_count += 1;
        }
    } else if let Some(slot) = lane.later.get_mut(lane.later_count) {
        *slot = (frame, velocity, midi);
        lane.later_count += 1;
    }
}

/// The hits an earlier piece placed in this one (`heard..heard + len`),
/// to the lane's hits; the later ones kept.
fn take_later(lane: &mut Lane, heard: u64, len: usize) {
    let end = heard.saturating_add(len as u64);
    let mut kept = 0;
    for k in 0..lane.later_count {
        let Some((frame, velocity, midi)) = lane.later.get(k).copied() else {
            continue;
        };
        if frame >= end {
            if let Some(slot) = lane.later.get_mut(kept) {
                *slot = (frame, velocity, midi);
            }
            kept += 1;
        } else if let (Some(at), Some(slot)) =
            (frame.checked_sub(heard), lane.hits.get_mut(lane.hit_count))
        {
            *slot = Hit {
                offset: u32::try_from(at).unwrap_or(0),
                velocity,
                midi,
            };
            lane.hit_count += 1;
        }
    }
    lane.later_count = kept;
}

/// Hand lane `lane`'s remote-keyed blocks the hits due in this piece,
/// which hears the song from `heard`.
fn give_remote_hits(
    lanes: &mut [Lane],
    keys: &[(usize, usize, u16)],
    key_last: &mut [(u64, bool)],
    remote: &[(u16, u64, f32, bool)],
    heard: u64,
    lane: usize,
    len: usize,
) {
    for ((l, b, piece), last) in keys
        .iter()
        .zip(key_last.iter_mut())
        .filter(|(k, _)| k.0 == lane)
    {
        let Some(block) = lanes.get_mut(*l).and_then(|x| x.blocks.get_mut(*b)) else {
            continue;
        };
        for (_, frame, velocity, midi) in remote.iter().filter(|h| h.0 == *piece) {
            if let Some(at) = frame.checked_sub(heard).filter(|at| *at < len as u64) {
                block.hit(u32::try_from(at).unwrap_or(0), *velocity);
                *last = ((*frame).max(1), *midi);
            }
        }
    }
}
