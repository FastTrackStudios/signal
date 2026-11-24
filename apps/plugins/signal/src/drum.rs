//! A drum piece from a sample library, played by hits: a kit piece's
//! reinforcement (the kick's and the snare's `Sample`, a tom's), from a
//! Signal-format library such as GGD Modern and Massive 2 (one
//! `.signalengine` per piece, its velocity layers and round-robins).
//!
//! Its hits come from the lane it is keyed to (that lane's trigger, FTS
//! Trigger's detection) or from a hit-note MIDI, each at its frame: the
//! piece is rendered up to a hit, struck, and rendered on, so the
//! reinforcement sits on the drum to the sample. `dynamics` sets how much
//! of the drummer's dynamics it follows: 0 plays every hit at `velocity`,
//! 1 at the hit's own.
//!
//! Desktop: the bank streams the piece's samples from disk on its own
//! thread, as the `sampler` block does.

use std::path::Path;

use signal_core::block::{
    Backend, BlockFactory, BlockKind, BlockProcessor, BuildContext, Built, Midi, Unavailable,
};
use signal_core::param::{ParamSpec, Unit};
use signal_core::platform::Platforms;
use signal_core::state::BlockSpec;

const PIECE: &str = "piece";

const PARAMS: &[ParamSpec] = &[
    // 0 dB: the piece's hardest hit peaks at 0 dBFS (each piece is
    // measured when it loads; libraries record theirs at all levels).
    ParamSpec::new("level", "Level", Unit::Db, -60.0, 12.0, -6.0),
    // The note the piece is struck on (its articulation: a snare's centre
    // on 38, its rimshot on 40); 0 is the piece's own, from its kind.
    ParamSpec::new("note", "Note", Unit::Plain(0), 0.0, 127.0, 0.0),
    ParamSpec::new("dynamics", "Dynamics", Unit::Percent, 0.0, 1.0, 1.0),
    // The velocity a hit plays at when `dynamics` is 0.
    ParamSpec::new("velocity", "Velocity", Unit::Plain(0), 1.0, 127.0, 100.0),
];

const PENDING: usize = 16;

pub struct Drum {
    bank: sampler::SamplerBank,
    scratch: Vec<f32>,
    note: u8,
    /// The note its kind is struck on (`note` 0).
    own_note: u8,
    level: f32,
    /// What brings the piece's hardest hit to 0 dBFS.
    normalize: f32,
    dynamics: f32,
    velocity: f32,
    pending: [(u32, f32); PENDING],
    pending_count: usize,
    /// Keyed by its piece\'s lane: played by its hits, not its own MIDI.
    keyed: bool,
}

impl Drum {
    fn velocity_of(&self, hit: f32) -> u8 {
        let v = self
            .dynamics
            .mul_add(hit.clamp(0.0, 1.0) * 127.0 - self.velocity, self.velocity);
        v.round().clamp(1.0, 127.0) as u8
    }

    /// Render frames `from..to` of the piece into `scratch`.
    fn render(&mut self, from: usize, to: usize) {
        if let Some(out) = self.scratch.get_mut(from * 2..to * 2) {
            self.bank.render(out);
        }
    }
}

impl BlockProcessor for Drum {
    fn params(&self) -> &'static [ParamSpec] {
        PARAMS
    }

    fn set_param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.level = 10f32.powf(value / 20.0) * self.normalize,
            1 => {
                let n = value.round().clamp(0.0, 127.0) as u8;
                self.note = if n == 0 { self.own_note } else { n };
            }
            2 => self.dynamics = value,
            3 => self.velocity = value,
            _ => {}
        }
    }

    /// As built: its voices gone (a one-shot rings through a note-off),
    /// its round robins and random draws at their first, so a host
    /// reusing it hears the hits a fresh one would play.
    fn reset(&mut self) {
        self.bank.restart(PIECE);
        self.pending_count = 0;
    }

    fn hit(&mut self, offset: u32, velocity: f32) {
        if let Some(slot) = self.pending.get_mut(self.pending_count) {
            *slot = (offset, velocity);
            self.pending_count += 1;
        }
    }

    fn keyed(&mut self) {
        // Its hits come through the key, notes included (the lane publishes
        // its notes as the piece's hits): not its own MIDI as well.
        self.keyed = true;
    }

    fn midi(&mut self, event: Midi) {
        // A hit note on the lane's own MIDI (an instrument lane, unkeyed),
        // at the piece's start.
        if let (Midi::NoteOn { velocity, .. }, false) = (event, self.keyed) {
            self.hit(0, f32::from(velocity) / 127.0);
        }
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = left.len().min(right.len()).min(self.scratch.len() / 2);
        if let Some(s) = self.scratch.get_mut(..n * 2) {
            s.fill(0.0);
        }
        // Hits in frame order; the graph hands them in order already, the
        // host's MIDI and the detector's may interleave.
        let count = core::mem::take(&mut self.pending_count);
        if let Some(hits) = self.pending.get_mut(..count) {
            hits.sort_unstable_by_key(|h| h.0);
        }
        let mut at = 0;
        for k in 0..count {
            let Some(&(offset, velocity)) = self.pending.get(k) else {
                break;
            };
            let to = usize::try_from(offset).unwrap_or(0).min(n);
            if to > at {
                self.render(at, to);
                at = to;
            }
            let v = self.velocity_of(velocity);
            self.bank.note_on(PIECE, self.note, v);
            // Drums are one-shots; the note-off frees the key, not the voice.
            self.bank.note_off(PIECE, self.note);
        }
        if n > at {
            self.render(at, n);
        }
        for ((frame, l), r) in self
            .scratch
            .chunks_exact(2)
            .take(n)
            .zip(left.iter_mut())
            .zip(right.iter_mut())
        {
            *l += frame[0] * self.level;
            *r += frame[1] * self.level;
        }
    }
}

/// The note a piece of this kind is struck on (the GM map, which the
/// Modern and Massive engines' key ranges follow).
#[must_use]
pub fn note_for_kind(kind: &str) -> u8 {
    match kind {
        "kick" => 36,
        "tom" => 45,
        "hi-hat" => 42,
        "ride" => 51,
        "crash" => 49,
        "china" => 52,
        "splash" => 55,
        _ => 38,
    }
}

/// The peak of the piece's hardest hit: struck at full velocity and
/// rendered for half a second, then silenced. At build, off the audio
/// thread.
fn hardest_hit_peak(bank: &mut sampler::SamplerBank, note: u8, sample_rate: f32) -> f32 {
    let mut buf = vec![0.0_f32; 1024];
    let mut peak = 0.0_f32;
    bank.note_on(PIECE, note, 127);
    bank.note_off(PIECE, note);
    for _ in 0..(sample_rate * 0.5 / 512.0) as usize {
        buf.fill(0.0);
        bank.render(&mut buf);
        peak = buf.iter().fold(peak, |m, s| m.max(s.abs()));
    }
    bank.panic(PIECE);
    peak
}

/// Makes the `drum` block, on the desktop: its asset is the piece's
/// `.signalengine`.
pub struct DrumFactory;

impl BlockFactory for DrumFactory {
    fn runs_on(&self, kind: &BlockKind) -> Option<Platforms> {
        matches!(kind, BlockKind::Signal(name) if name == "drum").then_some(Platforms::DESKTOP)
    }

    fn build(&self, spec: &BlockSpec, cx: &BuildContext<'_>) -> Result<Built, Unavailable> {
        let engine = spec.asset.as_ref().ok_or_else(|| Unavailable::Missing {
            what: format!("{}: no drum engine named", spec.id),
        })?;
        let path = Path::new(&engine.path);
        let failed = |e: &dyn std::fmt::Display| Unavailable::Failed {
            why: format!("{}: {e}", engine.path),
        };
        let engine_spec = sampler::EngineSpec::from_file(path).map_err(|e| failed(&e))?;
        let sr = cx.sample_rate.round() as u32;
        let mut bank = sampler::SamplerBank::new(sr);
        bank.set_preload_profile(sampler::PreloadProfile::DrumKit);
        bank.load_engine_spec(PIECE, &engine_spec, path.parent().unwrap_or(Path::new("")))
            .map_err(|e| failed(&e))?;
        // Every hit from memory: a reinforcement can't wait on the disk.
        // The whole piece, here, off the audio thread.
        let stats = bank.preload_instrument(PIECE).map_err(|e| failed(&e))?;
        let note = note_for_kind(&engine_spec.engine_type.to_ascii_lowercase());
        let peak = hardest_hit_peak(&mut bank, note, cx.sample_rate);
        // Its first hit is the first round robin, as after a reset.
        bank.restart(PIECE);
        // Up to +40 dB: a silent piece isn't made noise.
        let normalize = if peak > 0.01 { 1.0 / peak } else { 100.0 };
        tracing::info!(
            engine = %engine.path,
            kind = %engine_spec.engine_type,
            note,
            sr,
            loaded = stats.loaded,
            failed = stats.failed,
            peak_db = 20.0 * peak.max(1e-9).log10(),
            "drum block: piece loaded"
        );
        let block = Drum {
            bank,
            scratch: vec![0.0; cx.max_block * 2],
            note,
            own_note: note,
            level: normalize * 10f32.powf(-6.0 / 20.0),
            normalize,
            dynamics: 1.0,
            velocity: 100.0,
            pending: [(0, 0.0); PENDING],
            pending_count: 0,
            keyed: false,
        };
        Ok(Built {
            backend: Backend::Linked(Box::new(block)),
            params: PARAMS,
            sources: &[],
        })
    }
}
