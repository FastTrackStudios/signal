//! Kontakt 8.7.1's Time Machine II (group source mode 5), ported from KONTRA
//! (`crates/engine/voice/src/tm2.rs`, af3e766 + f910818), which decoded it
//! from the macOS arm64 `Kontakt 8` binary (addresses below are in it).
//!
//! Time Machine II is pitch-synchronous. When a sample is loaded its first
//! channel is cut at pitch marks (zplane's `CPSOLAAnalysis`): whole periods
//! apart, about half the analysis block ([`pitch_marks`]). A voice then plays
//! one grain at a time: the span from the mark nearest a virtual source
//! position to the next mark, read at the pitch's rate through
//! `FillBlock_ResL` (Normal quality's Lagrange, whatever the group's
//! quality). Meanwhile the grain before it plays on past its end for the new
//! grain's length, and the two crossfade linearly over it. The virtual
//! position moves `speed · ratio` source frames per output frame, so speed
//! picks which periods repeat or drop while the pitch stays the rate's. The
//! first grain plays alone; grains crossfade only after a grain ended inside
//! a block.
//!
//! - `BSample::CalcMetaInfo_TM2` (0x425c1c) and `BProgram::FindAllPitchMarks`
//!   (0x3eb854) analyse every sample of a group whose machine is Time Machine
//!   II at the group's analysis setting (`BGroup + 0xc18`, the source
//!   record's last integer): 1, 2 → a 512-frame block, 3 → 1024, 4 → 2048
//!   (`BSampleInfo::FindPitchMarks`, 0x43764c). The spec carries the block
//!   (`ZoneSpec::tm2_block`).
//! - Playback: `BTimeMachine2::FillBufferResampled_fwd` (0x6a6504),
//!   `FillBufferFast_fwd` (0x6a5e70), `AdvanceToNextGrain` (0x6a5970),
//!   `BGrainMachine::Reset` (0x69fba8) and `GetGrainIdxFromSamplePos`
//!   (0x69fd68); `BDsp::GetBlockMachine` (0x460b0c) calls them once a host
//!   block at the block's rate.
//!
//! Transient copy (the record's flag, stored off in every group read so far)
//! and loops in the grain machine are not ported.
//!
//! [`pitch_marks`] allocates and runs FFTs: it runs where a sample is
//! warmed or preloaded ([`MarkTable`]), never on the audio thread.
//! [`Machine`] allocates nothing.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use super::cache::SampleData;

use super::resample::{self, Cursor, Layout, Tap};

/// A sample's Time Machine II pitch marks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Marks {
    /// The sample's frames.
    pub frames: i64,
    /// Ascending source frames, the first 0, all below `frames`.
    pub at: Arc<[i64]>,
}

/// A patch's Time Machine II pitch marks, zone by zone.
///
/// Found off the audio thread, where a zone's sample is warmed or preloaded
/// ([`Self::analyse`]: a decode of the whole sample and an FFT per mark,
/// tens of milliseconds a sample), once per sample file and analysis block
/// for every engine built from the patch (clones share it). The spawn reads
/// a zone's marks lock-free ([`Self::get`]); a zone not analysed yet has
/// none, and its voice falls back to the plain stretch.
#[derive(Clone, Debug, Default)]
pub struct MarkTable {
    zones: Arc<OnceLock<Box<[OnceLock<Marks>]>>>,
    by_file: Arc<Mutex<HashMap<(PathBuf, u32), Marks>>>,
}

impl MarkTable {
    /// Zone `zone`'s marks, if analysed. Lock-free; no allocation.
    #[must_use]
    pub fn get(&self, zone: usize) -> Option<&Marks> {
        self.zones.get()?.get(zone)?.get()
    }

    /// Analyse zone `zone` (of `zones`) unless it is already: channel 0 of
    /// `data` (the sample at `path`) with an analysis `block`. Decodes the
    /// whole sample and allocates: never call it on the audio thread.
    pub fn analyse(&self, zone: usize, zones: usize, path: &Path, data: &SampleData, block: u32) {
        let table = self
            .zones
            .get_or_init(|| (0..zones).map(|_| OnceLock::new()).collect());
        let Some(slot) = table.get(zone) else {
            return;
        };
        slot.get_or_init(|| {
            let key = (path.to_path_buf(), block);
            if let Some(m) = self.by_file.lock().ok().and_then(|f| f.get(&key).cloned()) {
                return m;
            }
            let pcm = data.to_f32();
            let ch = usize::from(data.channels.max(1));
            let left: Vec<f32> = pcm.iter().step_by(ch).copied().collect();
            let m = pitch_marks(&left, data.sample_rate as f32, block as usize);
            if let Ok(mut f) = self.by_file.lock() {
                f.insert(key, m.clone());
            }
            m
        });
    }
}

/// `CPSOLAAnalysis`'s sizes for a block at a sample rate (the constructor,
/// 0x152d938, with one channel and no pitch tracker).
#[derive(Clone, Copy, Debug)]
struct Sizes {
    /// `0x50`: the correlation window, and half the frames the analysis
    /// holds before it marks.
    n: i64,
    /// `0x5c`: the lag the windows are compared at.
    half: i64,
    /// `0x64`: how far either side of the last lag the next is searched.
    reach: i64,
    /// `0x9c >> 2`: a period shorter than this repeats the last.
    min_period: i64,
}

fn sizes(rate: f32, block: i64) -> Sizes {
    let (n, half, quarter, floor) = if rate < 32000.0 {
        (block, block >> 1, block >> 2, 0x40)
    } else if rate < 64000.0 {
        (block, block >> 1, block >> 2, 0x80)
    } else if rate < 128_000.0 {
        (block << 1, block, block >> 1, 0x100)
    } else if rate < 256_000.0 {
        (block << 2, block << 1, block, 0x200)
    } else {
        (block << 3, block << 2, block << 1, 0x400)
    };
    let reach = if block < 0x200 {
        quarter >> 1
    } else if block > 0x3ff {
        quarter
    } else {
        quarter >> 2
    };
    Sizes {
        n,
        half,
        reach,
        min_period: floor >> 2,
    }
}

/// `ProcessData`'s peak floor below which a window is silence (1e-24).
const SILENT: f64 = 1e-24;

/// The pitch marks of `samples` (a sample's first channel) at `rate` Hz
/// with an analysis `block`, as `BSampleInfo::FindPitchMarks` (0x43764c)
/// leaves them.
///
/// From the last mark `b`, `ProcessData` (0x152e198) correlates the `n`
/// frames at `b` (`x`) with the `n` at `b + half` (`y`):
/// `r(k) = Σ x[i + k]·y[i]`, `x` zero outside its window. It takes the lag
/// `k` of the greatest `floor(r(k)·2n·10000)` (in f32; the first of equals)
/// over `last ± reach`, clipped to `-half ..= half - 2` (when the window runs
/// off one end it slides to keep its width), and marks `b + half - k`. No
/// positive score keeps the last lag; a silent window (peak below 1e-24)
/// takes lag 0. A period below `min_period` repeats the last period instead
/// (and at `n >= 1024` re-centres the search on it and widens it to `n / 2`).
/// Past the sample the analysis reads zeros.
#[must_use]
pub fn pitch_marks(samples: &[f32], rate: f32, block: usize) -> Marks {
    use realfft::RealFftPlanner;
    let frames = samples.len() as i64;
    let mut s = sizes(rate, block as i64);
    let n = s.n;
    let len = usize::try_from(n).unwrap_or(0);
    // The kernel's FFT: the next power of two of 2n (`zplfCcfCreateInstance`).
    let fft = (2 * len).next_power_of_two();
    let mut planner = RealFftPlanner::<f64>::new();
    let forward = planner.plan_fft_forward(fft);
    let inverse = planner.plan_fft_inverse(fft);
    let mut xs = forward.make_input_vec();
    let mut ys = forward.make_input_vec();
    let mut xf = forward.make_output_vec();
    let mut yf = forward.make_output_vec();
    let mut r = inverse.make_output_vec();
    let at = |i: i64| -> f32 {
        usize::try_from(i)
            .ok()
            .and_then(|i| samples.get(i))
            .copied()
            .unwrap_or(0.0)
    };
    let mut marks = vec![0i64];
    let mut base = 0i64;
    let mut lag = 0i64;
    let mut last_period = s.half;
    let centre = n;
    let scale = (2 * n) as f32;
    while base < frames {
        let peak = (0..n).map(|i| at(base + i).abs()).fold(f32::MIN, f32::max);
        let found = if f64::from(peak) >= SILENT {
            for (i, (x, y)) in xs.iter_mut().zip(ys.iter_mut()).enumerate() {
                let i = i as i64;
                let inside = i < n;
                *x = if inside { f64::from(at(base + i)) } else { 0.0 };
                *y = if inside {
                    f64::from(at(base + s.half + i))
                } else {
                    0.0
                };
            }
            if forward.process(&mut xs, &mut xf).is_err()
                || forward.process(&mut ys, &mut yf).is_err()
            {
                break;
            }
            for (a, b) in xf.iter_mut().zip(&yf) {
                *a *= b.conj();
            }
            if let Some(first) = xf.first_mut() {
                first.im = 0.0;
            }
            if let Some(last) = xf.last_mut() {
                last.im = 0.0;
            }
            if inverse.process(&mut xf, &mut r).is_err() {
                break;
            }
            let corr = |k: i64| -> f64 {
                let i = k.rem_euclid(fft as i64) as usize;
                r.get(i).copied().unwrap_or(0.0) / fft as f64
            };
            // The window over `out[centre + k]`, as `ProcessData` clips it.
            let mut lo = centre + lag - s.reach;
            let mut hi = centre + lag + s.reach;
            let top = centre + s.half - 1;
            let bottom = centre - (s.half - 1);
            if top < hi {
                hi = top - 1;
                lo = hi - 2 * s.reach;
            }
            if centre + lag - s.reach < bottom {
                lo = bottom - 1;
                hi = lo + 2 * s.reach;
            }
            let mut best = 0.0f32;
            let mut at_best = None;
            for idx in lo..=hi {
                let v = (corr(idx - centre) as f32) * scale;
                // `fmul` by 10000.0f then `frintm` (0x152e7c4): floored in
                // f32, never through an integer, so loud windows do not
                // saturate.
                let q = (v * 10000.0).floor();
                if q > best {
                    best = q;
                    at_best = Some(idx);
                }
            }
            at_best.map(|i| i - centre)
        } else {
            Some(0)
        };
        if let Some(k) = found {
            lag = k;
        }
        let mut period = s.half - lag;
        if period < s.min_period {
            period = last_period;
            if n > 0x3ff {
                s.half = n >> 1;
                s.reach = n >> 1;
                lag = s.half - period;
            }
        }
        base += period;
        marks.push(base);
        last_period = period;
    }
    marks.retain(|&m| m < frames);
    Marks {
        frames,
        at: marks.into(),
    }
}

/// One output frame of a Time Machine II voice: `w·new + g·old`, the grain
/// playing and the grain before it fading out.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Frame {
    pub new: Tap,
    pub old: Tap,
    pub w: f32,
    pub g: f32,
    /// The block was played by `FillBufferFast_fwd`: each read is its
    /// frame, no interpolation.
    pub copy: bool,
    /// The machine had ended: silence.
    pub silent: bool,
}

/// `BTimeMachine2`'s playback state (offsets into the machine object).
#[derive(Clone, Debug)]
pub(crate) struct Machine {
    marks: Marks,
    /// `0x64`: the playable end, source frames.
    end: i64,
    /// `0x38`: speed, at least 0.01 (`SetSpeed`, 0x6a55f8).
    speed: f32,
    /// The block's rate (source frames per output frame).
    rate: f32,
    /// `0x30`, the added speed ratio: `BEngine::ResetMachine` (0x3db59c)
    /// sets the sample's rate over the engine's, in f32.
    ratio: f64,
    /// `0x58`: the virtual position, moving `speed · ratio` a frame.
    pub vpos: f64,
    /// `0x60`: the last grain's mark, the next search's hint.
    hint: i32,
    /// `0x74`/`0x7c` and `0x94`: the grain's read position and frames left.
    cur: Cursor,
    cur_left: f32,
    /// `0x84`/`0x8c` and `0x9c`: the grain before, played on to fade out.
    old: Cursor,
    old_left: f32,
    /// `0xa4`, `0xac`: the fade's weight and its slope per source frame.
    w: f32,
    dw: f32,
    /// `0x71`: before the first frame (the fading grain counts nothing).
    first: bool,
    /// `0x72`: a frame has played, so grains crossfade.
    fading: bool,
    /// `0xb4`: no grain left.
    pub ended: bool,
}

/// `|rate − 1|` within this plays a block by `FillBufferFast_fwd`
/// (`BDsp::GetBlockMachine`, 0x460b0c).
const FAST: f32 = 1.0e-6;

impl Machine {
    /// `BGrainMachine::Reset` (0x69fba8) then `BTimeMachine2::Reset`
    /// (0x6a55d4) at source position `start`: the grain cursor at the mark
    /// nearest `start` (ties to the earlier), carrying `start`'s fraction,
    /// with nothing left to play, so the first block advances at once.
    pub fn new(marks: Marks, end: i64, (speed, ratio): (f64, f64), start: f64) -> Self {
        let frames = marks.frames;
        let nm = marks.at.len() as i32;
        let p = start as i32;
        let idx = if i64::from(p) < frames {
            let last = nm - 1;
            let top = last.max(0);
            let mut i7 = 0;
            loop {
                let i9 = i7;
                if top == i9 {
                    i7 = if nm - 2 <= top { last } else { 0 };
                    break;
                }
                i7 = i9 + 1;
                let i2 = i7.clamp(0, last.max(0));
                if mark(&marks, i2) - i64::from(p) >= 0 {
                    let i8 = i9.clamp(0, last.max(0));
                    if last <= i7
                        || i64::from(p) - mark(&marks, i8) <= mark(&marks, i2) - i64::from(p)
                    {
                        i7 = i9;
                    }
                    break;
                }
            }
            i7
        } else {
            nm - 1
        };
        let at = mark(&marks, idx).min(frames);
        let frac = (start - f64::from(p)) as f32;
        Self {
            marks,
            end,
            speed: (speed as f32).max(0.01),
            rate: 1.0,
            ratio,
            vpos: start,
            hint: idx,
            cur: Cursor { pos: at, frac },
            cur_left: 0.0,
            old: Cursor { pos: 0, frac: 0.0 },
            old_left: 0.0,
            w: 0.0,
            dw: 0.0,
            first: true,
            fading: false,
            ended: false,
        }
    }

    /// Source frames the virtual position moves per output frame, as a
    /// block adds them (`speed · ratio`, in f32).
    pub fn per_frame(&self) -> f32 {
        (self.ratio * f64::from(self.speed)) as f32
    }

    /// One `GetBlockMachine` call: `out`'s frames at the block's `rate`
    /// (`FillBufferResampled_fwd` 0x6a6504, or `FillBufferFast_fwd`
    /// 0x6a5e70 within [`FAST`] of 1). Grain by grain: the grain reads to its
    /// end, the grain before it reads as many frames to its own, and over
    /// those the two crossfade linearly, the new one's weight rising
    /// `rate / length` a frame from 0; then the virtual position moves
    /// `frames · speed · ratio` and, when the grain ended inside the block,
    /// [`Self::advance`] starts the next one. `new_taps` holds at least
    /// `out.len()` reads, `old_taps` twice that.
    pub fn block(
        &mut self,
        layout: &Layout,
        rate: f32,
        out: &mut [Frame],
        old_taps: &mut [Tap],
        new_taps: &mut [Tap],
    ) {
        self.rate = rate;
        out.fill(Frame {
            silent: true,
            ..Frame::default()
        });
        if self.ended {
            return;
        }
        let n = out.len().min(new_taps.len());
        let fast = (rate + -1.0).abs() <= FAST;
        let mut o = 0usize;
        let mut ko = 0usize;
        loop {
            let rem = n - o;
            let before = self.cur.pos;
            let end = self.cur.pos + i64::from(self.cur_left as i32);
            let m = fill(layout, &mut self.cur, rate, fast, new_taps, o, n, end) - o;
            for (f, t) in out.iter_mut().zip(new_taps.iter()).skip(o).take(m) {
                *f = Frame {
                    new: *t,
                    old: Tap::default(),
                    w: 1.0,
                    g: 0.0,
                    copy: fast,
                    silent: false,
                };
            }
            if self.old_left >= 1.0 {
                let before = self.old.pos;
                let end = self.old.pos + i64::from(self.old_left as i32);
                let lim = (ko + rem).min(old_taps.len());
                let k = fill(layout, &mut self.old, rate, fast, old_taps, ko, lim, end) - ko;
                self.old_left -= (self.old.pos - before) as f32;
                let g = if self.first { 0.0 } else { 1.0 };
                if self.fading && k > 0 {
                    let step = if fast { self.dw } else { rate * self.dw };
                    for j in 0..k {
                        let w = self.w;
                        if j < m
                            && let (Some(f), Some(t)) = (out.get_mut(o + j), old_taps.get(ko + j))
                        {
                            f.old = *t;
                            f.w = w;
                            f.g = g * (1.0 - w);
                        }
                        self.w = w + step;
                    }
                }
                ko += k;
            }
            self.cur_left -= (self.cur.pos - before) as f32;
            self.vpos += f64::from(m as f32 * self.per_frame());
            o += m;
            if n - o < 1 {
                return;
            }
            if m > 0 {
                self.first = false;
                self.fading = true;
            }
            if !self.advance() {
                self.ended = true;
                return;
            }
        }
    }

    /// `AdvanceToNextGrain` (0x6a5970), transients off: the grain before
    /// plays on from where the grain stopped; the new grain is the span from
    /// the mark nearest the virtual position to the next mark, read from its
    /// first frame, and both get that span's length. `false` when the
    /// position passed the end or no grain is left.
    fn advance(&mut self) -> bool {
        self.old = self.cur;
        self.old_left = self.cur_left;
        let idx = grain_index(&self.marks, self.vpos as i32, self.hint);
        self.hint = idx;
        let end = self.end;
        if end as f64 <= self.vpos {
            return false;
        }
        let nm = self.marks.at.len() as i32;
        if nm - 1 <= idx {
            return false;
        }
        let i9 = idx.max(0).min(nm - 1);
        self.cur = Cursor {
            pos: mark(&self.marks, i9).min(end),
            frac: 0.0,
        };
        let i10 = (idx.max(-1) + 1).min(nm - 1);
        let next = mark(&self.marks, i10);
        let stop = if next <= end || end < mark(&self.marks, i9) {
            next
        } else {
            end
        };
        let len = (stop - mark(&self.marks, i9)) as f32;
        self.cur_left = len;
        self.old_left = len;
        let frames = self.marks.frames;
        if self.cur_left + self.cur.pos as f32 > frames as f32 {
            self.cur_left = (frames - self.cur.pos) as f32;
        }
        if (frames as f32) < self.old_left + self.old.pos as f32 {
            self.old_left = (frames - self.old.pos) as f32;
        }
        self.w = 0.0;
        self.dw = 1.0 / self.old_left;
        true
    }
}

fn mark(marks: &Marks, i: i32) -> i64 {
    usize::try_from(i)
        .ok()
        .and_then(|i| marks.at.get(i))
        .copied()
        .unwrap_or(0)
}

/// `BGrainMachine::GetGrainIdxFromSamplePos` (0x69fd68), not exact, no loop:
/// from `hint` back to a mark before `pos`, then forward to the mark pair
/// around it; the nearer of the two (ties to the earlier), never the last
/// mark but at the end.
fn grain_index(marks: &Marks, pos: i32, hint: i32) -> i32 {
    let nm = marks.at.len() as i32;
    let last = nm - 1;
    if marks.frames <= i64::from(pos) {
        return last;
    }
    let mut p = hint;
    if p >= 1 {
        loop {
            if mark(marks, p.min(last)) < i64::from(pos) {
                break;
            }
            let was = p;
            p -= 1;
            if !(p != 0 && was > 0) {
                p = 0;
                break;
            }
        }
    }
    let top = if p <= last { last } else { p };
    let mut i5 = p;
    let d;
    loop {
        p = i5;
        if top == p {
            return last;
        }
        i5 = p + 1;
        let i2 = i5.max(0).min(last);
        let diff = mark(marks, i2) - i64::from(pos);
        if diff >= 0 {
            d = diff;
            break;
        }
    }
    let here = mark(marks, p.max(0).min(last));
    if i64::from(pos) - here <= d {
        return p;
    }
    if last <= i5 { p } else { i5 }
}

/// One `FillBlock_ResL` (or, fast, `BSample::FillBlock`) call: taps into
/// `taps[o..lim]` from `cur` while it is before `end`; returns the next
/// index.
#[allow(clippy::too_many_arguments)]
fn fill(
    layout: &Layout,
    cur: &mut Cursor,
    rate: f32,
    fast: bool,
    taps: &mut [Tap],
    o: usize,
    lim: usize,
    end: i64,
) -> usize {
    let Some(taps) = taps.get_mut(..lim) else {
        return o;
    };
    if fast {
        let mut o = o;
        while o < taps.len() && cur.pos < end {
            if let Some(t) = taps.get_mut(o) {
                *t = Tap {
                    at: cur.pos,
                    frac: 0.0,
                };
            }
            cur.pos += 1;
            o += 1;
        }
        return o;
    }
    resample::resl(layout, cur, rate, taps, o, Some(end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_steady_tone_is_marked_at_whole_periods() {
        // 48 kHz, a 120-sample period: at block 1024 the search spans
        // 256..=768, so marks fall on whole periods in it.
        let period = 120.0f32;
        let tone: Vec<f32> = (0..48000)
            .map(|i| (std::f32::consts::TAU * i as f32 / period).sin() * 0.3)
            .collect();
        let m = pitch_marks(&tone, 48000.0, 1024);
        assert_eq!(m.at.first(), Some(&0));
        let gaps: Vec<i64> = (m.at.windows(2))
            .filter(|w| w[1] + 2048 < 48000)
            .map(|w| w[1] - w[0])
            .collect();
        assert!(
            gaps.iter().all(|g| g % 120 == 0 && (256..=768).contains(g)),
            "{gaps:?}"
        );
    }

    #[test]
    fn silence_steps_half_a_window() {
        let m = pitch_marks(&[0.0; 4000], 48000.0, 1024);
        assert_eq!(&m.at[..4], &[0, 512, 1024, 1536]);
    }

    /// At speed 1 and rate 1 the grains tile the sample: every frame reads
    /// the source frame the clock is at, the grain before faded out.
    #[test]
    fn unity_speed_and_rate_walk_the_sample_frame_by_frame() {
        let marks = Marks {
            frames: 10_000,
            at: (0..20).map(|i| i * 500).collect::<Vec<_>>().into(),
        };
        let layout = Layout {
            start: 0,
            end: 10_000,
            looped: None,
        };
        let mut m = Machine::new(marks, 10_000, (1.0, 1.0), 0.0);
        let mut out = [Frame::default(); 128];
        let mut old = [Tap::default(); 256];
        let mut new = [Tap::default(); 128];
        let mut t = 0i64;
        for _ in 0..40 {
            m.block(&layout, 1.0, &mut out, &mut old, &mut new);
            for f in &out {
                assert!(!f.silent);
                assert_eq!(f.new.at, t);
                t += 1;
            }
        }
    }

    /// At half speed every grain plays twice as the clock walks half as far.
    #[test]
    fn half_speed_repeats_grains() {
        let marks = Marks {
            frames: 100_000,
            at: (0..200).map(|i| i * 400).collect::<Vec<_>>().into(),
        };
        let layout = Layout {
            start: 0,
            end: 100_000,
            looped: None,
        };
        let mut m = Machine::new(marks, 100_000, (0.5, 1.0), 0.0);
        let mut out = [Frame::default(); 128];
        let mut old = [Tap::default(); 256];
        let mut new = [Tap::default(); 128];
        for _ in 0..100 {
            m.block(&layout, 1.0, &mut out, &mut old, &mut new);
        }
        assert!((m.vpos - 6400.0).abs() < 1.0, "{}", m.vpos);
        // The grain playing is near the clock, not 12800 frames on.
        let last = out[127].new.at;
        assert!((5600..7600).contains(&last), "{last}");
    }
}
