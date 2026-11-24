//! A one-shot sample, played by hits: a kit piece's reinforcement (the
//! kick's and the snare's `Sample`), compiled in everywhere.
//!
//! Its hits come from the lane it is keyed to (that lane's trigger, or
//! the host's hit MIDI), each at its frame in the piece. A hit's velocity
//! sets the level through `dynamics`: 0 plays every hit at `level`, 1
//! follows the drummer. Eight voices; a ninth steals the oldest, so fast
//! hits overlap as a real drum's ring does.

use alloc::vec::Vec;

use crate::block::{BlockProcessor, Unavailable};
use crate::param::{ParamSpec, Unit};

const PARAMS: &[ParamSpec] = &[
    ParamSpec::new("level", "Level", Unit::Db, -48.0, 12.0, 0.0),
    ParamSpec::new("dynamics", "Dynamics", Unit::Percent, 0.0, 1.0, 0.8),
];

const VOICES: usize = 8;
const PENDING: usize = 16;

#[derive(Clone, Copy, Default)]
struct Voice {
    active: bool,
    /// Where in the sample it plays next.
    pos: usize,
    /// Frames into the next piece before it starts.
    start: usize,
    gain: f32,
    /// When it started, to steal the oldest.
    born: u64,
}

pub struct Sample {
    data: Vec<f32>,
    voices: [Voice; VOICES],
    pending: [(u32, f32); PENDING],
    pending_count: usize,
    level: f32,
    dynamics: f32,
    clock: u64,
}

impl Sample {
    /// From a WAV file's bytes (its first channel).
    ///
    /// # Errors
    ///
    /// When the bytes aren't a WAV this reads, or hold no samples.
    pub fn from_wav(bytes: &[u8]) -> Result<Self, Unavailable> {
        let data = crate::wav::first_channel(bytes).map_err(|why| Unavailable::Failed {
            why: alloc::format!("sample: {why}"),
        })?;
        if data.is_empty() {
            return Err(Unavailable::Failed {
                why: "sample has no audio".into(),
            });
        }
        Ok(Self::from_samples(data))
    }

    #[must_use]
    pub fn from_samples(data: Vec<f32>) -> Self {
        Self {
            data,
            voices: [Voice::default(); VOICES],
            pending: [(0, 0.0); PENDING],
            pending_count: 0,
            level: 1.0,
            dynamics: 0.8,
            clock: 0,
        }
    }

    fn start(&mut self, offset: usize, velocity: f32) {
        let gain = self.level * ((1.0 - self.dynamics) + self.dynamics * velocity.clamp(0.0, 1.0));
        self.clock = self.clock.wrapping_add(1);
        let born = self.clock;
        let slot = match self.voices.iter().position(|v| !v.active) {
            Some(i) => i,
            None => self
                .voices
                .iter()
                .enumerate()
                .min_by_key(|(_, v)| v.born)
                .map_or(0, |(i, _)| i),
        };
        if let Some(v) = self.voices.get_mut(slot) {
            *v = Voice {
                active: true,
                pos: 0,
                start: offset,
                gain,
                born,
            };
        }
    }
}

impl BlockProcessor for Sample {
    fn params(&self) -> &'static [ParamSpec] {
        PARAMS
    }

    fn set_param(&mut self, index: usize, value: f32) {
        match index {
            0 => self.level = crate::util::db_to_gain(value),
            1 => self.dynamics = value,
            _ => {}
        }
    }

    fn reset(&mut self) {
        self.voices = [Voice::default(); VOICES];
        self.pending_count = 0;
    }

    fn hit(&mut self, offset: u32, velocity: f32) {
        if let Some(slot) = self.pending.get_mut(self.pending_count) {
            *slot = (offset, velocity);
            self.pending_count += 1;
        }
    }

    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = left.len().min(right.len());
        for k in 0..core::mem::take(&mut self.pending_count) {
            if let Some(&(offset, velocity)) = self.pending.get(k) {
                let at = usize::try_from(offset)
                    .unwrap_or(0)
                    .min(n.saturating_sub(1));
                self.start(at, velocity);
            }
        }
        for v in self.voices.iter_mut().filter(|v| v.active) {
            let begin = core::mem::take(&mut v.start).min(n);
            for (i, (l, r)) in left
                .iter_mut()
                .zip(right.iter_mut())
                .enumerate()
                .skip(begin)
            {
                let _ = i;
                let Some(s) = self.data.get(v.pos) else {
                    v.active = false;
                    break;
                };
                *l += s * v.gain;
                *r += s * v.gain;
                v.pos += 1;
            }
            if v.pos >= self.data.len() {
                v.active = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hit_plays_at_its_frame_and_velocity() {
        let mut s = Sample::from_samples(alloc::vec![1.0; 10]);
        s.set_param(1, 1.0);
        s.hit(5, 0.5);
        let (mut l, mut r) = (alloc::vec![0.0_f32; 8], alloc::vec![0.0_f32; 8]);
        s.process(&mut l, &mut r);
        assert_eq!(&l[..5], &[0.0; 5], "silent before the hit");
        assert!(
            (l[5] - 0.5).abs() < 1e-6,
            "half velocity, full dynamics: half level"
        );
        // It rings on into the next piece, and stops at its end.
        let (mut l, mut r) = (alloc::vec![0.0_f32; 8], alloc::vec![0.0_f32; 8]);
        s.process(&mut l, &mut r);
        assert_eq!(
            l.iter().filter(|x| **x > 0.0).count(),
            7,
            "the sample's remaining 7 frames"
        );
    }
}
