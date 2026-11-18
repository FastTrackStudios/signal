//! Kontakt 8.7.1's sample-playback resampler at Normal quality, ported from
//! KONTRA (`crates/engine/voice/src/resample.rs`): which path plays a block
//! (a plain copy, or the 4-point Lagrange), and the f32 cursor it keeps.
//!
//! Addresses are in the macOS arm64 `Kontakt 8` binary (8.7.1).
//!
//! - `BDsp::GetBlockSampler` (0x45f768) forms the block's step once, in
//!   f32; `BDsp::DoGetBlockSampler` (0x46238c) copies frames when
//!   `|step - 1| <= 1e-4` (no interpolation, the fraction left alone).
//! - `BSample::FillBlock_ResL` (0x462a80) reads through 512-frame cache
//!   buffers aligned on 512 source frames (`BSample::CachePCM`, 0x4cde70).
//!   While frames `pos - 1 ..= pos + 2` sit in one buffer and no loop folds
//!   them, `ResampleChannelQuad_L_SSE2` (0x4cf1b4) runs with the cursor as
//!   one f32 *relative to the buffer* (`p = (float)rel + frac`), `p += step`
//!   per frame: a scalar prologue up to the output's 16-byte alignment, then
//!   four lanes `p, p+s, p+2s, p+3s` each stepping by `4s`, then a scalar
//!   tail. Elsewhere (a buffer seam, a loop's fold) one frame at a time,
//!   with `p = frac + 1`.
//!
//! The cursor is in *logical* frames (frames played, a loop unrolled);
//! [`Layout::map`] folds one onto the sample. Everything here is plain
//! arithmetic on caller-owned buffers: no allocation, no I/O.

/// How a voice reads its sample between frames.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Interp {
    /// Linear interpolation on an f64 cursor (the engine's own).
    #[default]
    Linear,
    /// Kontakt's Normal quality (`FillBlock_ResL`): the block-formed f32
    /// step, the cache-relative f32 cursor and the 4-point Lagrange.
    KontaktNormal,
}

impl Interp {
    /// A spec's `interpolation`: `"kontakt_normal"` (or `"kontakt"`); anything
    /// else (`""`, `"linear"`) is [`Interp::Linear`].
    // ponytail: Kontakt's High / Ultra (the 9- and 25-tap windowed sincs)
    // are in KONTRA's resample.rs; no group of a library we play uses them.
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "kontakt_normal" | "kontakt" => Self::KontaktNormal,
            _ => Self::Linear,
        }
    }
}

/// The host block Kontakt forms its step and plans its reads over, frames,
/// on the engine's absolute frame grid: 128, the block KONTRA's `play` (the
/// null reference) and `render_css_live` render in.
pub const KONTAKT_BLOCK: usize = 128;

/// `DoGetBlockSampler`'s test: within this of 1 the block is copied.
const BYPASS: f32 = 1.0e-4;
/// `CachePCM`'s buffer, source frames.
const CACHE: i64 = 512;
/// The kernel's highest relative frame: `pos + 2` stays in the buffer.
const CACHE_LIMIT: i64 = 510;

/// Kontakt's `BFractionalPos`, in logical frames.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Cursor {
    pub pos: i64,
    pub frac: f32,
}

/// One output frame's read: the logical frame it is at and the fraction past
/// it, in the f32 Kontakt holds it in.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Tap {
    pub at: i64,
    pub frac: f32,
}

/// One block of planned reads.
pub(crate) type Plan = [Tap; KONTAKT_BLOCK];

/// What a voice plays, in sample frames: from `start` to `end`, through a
/// forward loop `[ls, le)` taken if the cursor started before its end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Layout {
    pub start: i64,
    pub end: i64,
    pub looped: Option<(i64, i64)>,
}

impl Layout {
    /// The sample frame logical frame `l` plays.
    #[inline]
    #[must_use]
    pub fn map(&self, l: i64) -> i64 {
        match self.looped {
            Some((ls, le)) if l >= le => ls + (l - le) % (le - ls),
            _ => l,
        }
    }

    /// The sample frame logical `l` plays, if any: silence before the start
    /// and past the end.
    #[inline]
    #[must_use]
    pub fn frame(&self, l: i64) -> Option<usize> {
        if l < self.start {
            return None;
        }
        let f = self.map(l);
        (f < self.end).then_some(f as usize)
    }

    /// The forward run holding logical `l`: its sample frame and the frame
    /// the run ends before (the loop's end, or the region's).
    fn run(&self, l: i64) -> Option<(i64, i64)> {
        if l < self.start {
            return None;
        }
        let f = self.map(l);
        let end = self.looped.map_or(self.end, |(_, le)| le);
        (f < end).then_some((f, end))
    }
}

/// Fills `out` with one Kontakt block's reads (one `GetBlockSampler` call,
/// its output buffer aligned at `out[0]`) at `step`, moving `cur`. Returns
/// `true` for a block copied whole (each read is its frame, the fraction
/// ignored).
pub(crate) fn block(layout: &Layout, cur: &mut Cursor, step: f32, out: &mut [Tap]) -> bool {
    if (step + -1.0).abs() <= BYPASS {
        for tap in out {
            *tap = Tap {
                at: cur.pos,
                frac: cur.frac,
            };
            cur.pos += 1;
        }
        return true;
    }
    resl(layout, cur, step, out, 0, None);
    false
}

/// Where the 4-at-a-time kernel may run from logical `pos`: the logical
/// position of its cache buffer's first frame and the kernel's limit (the
/// relative frame it stops before). `None`: frame `pos - 1` is not in a
/// forward run with `pos + 2`.
fn cache_window(layout: &Layout, pos: i64) -> Option<(i64, i64)> {
    let (first, end) = layout.run(pos - 1)?;
    let buffer = first - first.rem_euclid(CACHE);
    let rel = first + 1 - buffer;
    let last = end - 1;
    let limit = CACHE_LIMIT.min(last - 1 - buffer);
    Some((pos - rel, limit))
}

/// One `FillBlock_ResL` call over `out[o..]` (the output buffer aligned at
/// `out[0]`), up to its end position `end` when it has one (its `param_8`:
/// the kernel's limit is at most `end` less the cache buffer's first frame,
/// and it reads while the cursor is before `end`); returns the next output
/// index. Time Machine II reads its grains through it ([`super::tm2`]).
pub(crate) fn resl(
    layout: &Layout,
    cur: &mut Cursor,
    step: f32,
    out: &mut [Tap],
    mut o: usize,
    end: Option<i64>,
) -> usize {
    let n = out.len();
    while o < n && end.is_none_or(|e| cur.pos < e) {
        if let Some((base, limit)) = cache_window(layout, cur.pos)
            .map(|(base, limit)| (base, end.map_or(limit, |e| limit.min(e - base))))
            && cur.pos - base < limit
        {
            let mut rel = (cur.pos - base) as i32;
            let before = o;
            o = quad(base, &mut rel, &mut cur.frac, step, limit as i32, out, o);
            cur.pos = base + i64::from(rel);
            if o > before {
                continue;
            }
        }
        // One source frame at a time (a cache seam or a loop's fold).
        let mut p = cur.frac + 1.0;
        let mut i = 1i32;
        loop {
            if let Some(tap) = out.get_mut(o) {
                *tap = Tap {
                    at: cur.pos,
                    frac: p - i as f32,
                };
            }
            p = step + p;
            i = p as i32;
            let left = n - o;
            o += 1;
            if left < 2 || i >= 2 {
                break;
            }
        }
        cur.frac = p - i as f32;
        cur.pos += i64::from(i - 1);
    }
    o
}

/// `ResampleChannelQuad_L_SSE2` from output `o` on: reads into `out` while
/// the relative frame stays below `limit`; returns the next output index.
/// The output buffer is 16-byte aligned at `out[0]`.
fn quad(
    base: i64,
    rel: &mut i32,
    frac: &mut f32,
    s: f32,
    limit: i32,
    out: &mut [Tap],
    mut o: usize,
) -> usize {
    let mut count = out.len() - o;
    let mut emit = |o: &mut usize, i: i32, fr: f32| {
        if let Some(tap) = out.get_mut(*o) {
            *tap = Tap {
                at: base + i64::from(i),
                frac: fr,
            };
        }
        *o += 1;
    };
    // Scalar up to the output's alignment.
    let k = ((4 - o % 4) % 4).min(count);
    let mut p = *frac + *rel as f32;
    let mut i = *rel;
    if k >= 1 && i < limit {
        let mut left = k;
        loop {
            emit(&mut o, i, p - i as f32);
            p = s + p;
            i = p as i32;
            let was = left;
            left -= 1;
            if was < 2 || i >= limit {
                break;
            }
        }
        count -= k - left;
    } else {
        i = p as i32;
    }
    *frac = p - i as f32;
    *rel = i;
    if count < 1 {
        return o;
    }
    // Four lanes, each stepping by 4s.
    p = *frac + *rel as f32;
    let l1 = p + s;
    let l2 = l1 + s;
    let mut lanes = [p, l1, l2, l2 + s];
    if count >= 4 {
        let s4 = s * 4.0;
        loop {
            if lanes[3] as i32 >= limit {
                break;
            }
            for lane in &mut lanes {
                let i = *lane as i32;
                emit(&mut o, i, *lane - i as f32);
                *lane = s4 + *lane;
            }
            let was = count;
            count -= 4;
            if was <= 7 {
                break;
            }
        }
    }
    i = lanes[0] as i32;
    *frac = lanes[0] - i as f32;
    *rel = i;
    if i >= limit || count < 1 {
        return o;
    }
    // The scalar tail.
    p = *frac + i as f32;
    loop {
        emit(&mut o, i, p - i as f32);
        p = s + p;
        i = p as i32;
        let was = count;
        count -= 1;
        if was < 2 || i >= limit {
            break;
        }
    }
    *frac = p - i as f32;
    *rel = i;
    o
}

/// Normal quality's 4-point Lagrange between `y1` and `y2`, at `frac` past
/// `y1`, in Kontakt's order of operations (no fused multiply-adds).
#[inline]
#[must_use]
pub fn lagrange(frac: f32, y0: f32, y1: f32, y2: f32, y3: f32) -> f32 {
    let d = frac + 1.0;
    let d1 = d + -1.0;
    let d2 = d1 + -1.0;
    let d3 = d2 + -1.0;
    let half = d3 * ((d * y1) * d2 - (d * y2) * d1) * 0.5;
    let sixth = (((d * y3) * d1) * d2 - ((y0 * d1) * d2) * d3) * SIXTH;
    half + sixth
}

/// `0x3e2aaaab`.
const SIXTH: f32 = 0.166_666_67;

#[cfg(test)]
mod tests {
    use super::*;

    const OPEN: Layout = Layout {
        start: 0,
        end: 1 << 40,
        looped: None,
    };

    #[test]
    fn a_step_within_1e_4_of_one_copies_frames() {
        let mut cur = Cursor { pos: 7, frac: 0.3 };
        let mut out = vec![Tap::default(); 3];
        assert!(block(&OPEN, &mut cur, 1.000_05, &mut out));
        assert_eq!(cur, Cursor { pos: 10, frac: 0.3 });
        assert_eq!(out[2], Tap { at: 9, frac: 0.3 });
    }

    #[test]
    fn lagrange_passes_through_its_frames() {
        assert_eq!(lagrange(0.0, 3.0, -1.0, 2.0, 5.0), -1.0);
        let mid = lagrange(0.5, 0.0, 1.0, 1.0, 0.0);
        assert!((mid - 1.125).abs() < 1e-6, "{mid}");
    }

    #[test]
    fn the_quad_kernel_keeps_four_lanes_stepping_by_four_steps() {
        let s = 1.003_5f32;
        let mut out = vec![Tap::default(); 64];
        let (mut rel, mut frac) = (300, 0.25f32);
        let o = quad(1000, &mut rel, &mut frac, s, 510, &mut out, 0);
        assert_eq!(o, 64);
        // Lane 0 of the second quad is p + 4s, not ((p+s)+s)+s+s.
        let p = 0.25f32 + 300.0;
        let next = s * 4.0 + p;
        assert_eq!(out[4].at, 1000 + next as i64);
        assert_eq!(out[4].frac, next - (next as i32) as f32);
        let lane1 = p + s;
        assert_eq!(out[1].frac, lane1 - (lane1 as i32) as f32);
    }

    #[test]
    fn a_quad_call_from_an_unaligned_output_runs_scalar_to_the_alignment() {
        let s = 1.003_5f32;
        let mut out = vec![Tap::default(); 16];
        let (mut rel, mut frac) = (10, 0.5f32);
        let o = quad(0, &mut rel, &mut frac, s, 510, &mut out, 1);
        assert_eq!(o, 16);
        // Outputs 1..4 step one at a time, then lanes from output 4.
        let mut p = 10.5f32;
        for tap in &out[1..4] {
            assert_eq!(tap.frac, p - (p as i32) as f32);
            p = s + p;
        }
        assert_eq!(out[4].frac, p - (p as i32) as f32);
    }

    /// Across cache seams, block after block, the reads walk the line the
    /// step draws: never back, never more than a frame off it.
    #[test]
    fn the_reads_follow_the_step_across_cache_seams() {
        for step in [0.918_75f32, 1.25, 0.8, 1.6] {
            let mut cur = Cursor::default();
            let mut out = [Tap::default(); KONTAKT_BLOCK];
            let mut last = -1.0f64;
            for b in 0..40 {
                block(&OPEN, &mut cur, step, &mut out);
                for (k, t) in out.iter().enumerate() {
                    let x = t.at as f64 + f64::from(t.frac);
                    let want = (b * KONTAKT_BLOCK + k) as f64 * f64::from(step);
                    assert!((0.0..1.0).contains(&t.frac), "{step} frac {}", t.frac);
                    assert!(x > last, "{step}: {x} after {last}");
                    assert!((x - want).abs() < 0.05, "{step} at {b}/{k}: {x} vs {want}");
                    last = x;
                }
            }
        }
    }

    /// At a loop's fold the reads go one frame at a time, and the logical
    /// cursor maps back into the loop.
    #[test]
    fn a_loop_folds_logical_frames_into_its_span() {
        let layout = Layout {
            start: 0,
            end: 2000,
            looped: Some((1000, 1500)),
        };
        assert_eq!(layout.map(1499), 1499);
        assert_eq!(layout.map(1500), 1000);
        assert_eq!(layout.map(2001), 1001);
        assert_eq!(layout.frame(-1), None);
        // Before the fold the kernel stops short of the loop's end.
        assert_eq!(
            cache_window(&layout, 1497).map(|(_, l)| l),
            Some(1498 - 1024)
        );
        let mut cur = Cursor {
            pos: 1490,
            frac: 0.0,
        };
        let mut out = [Tap::default(); 32];
        block(&layout, &mut cur, 0.75, &mut out);
        let mut prev = 1489.0;
        for t in out {
            let x = t.at as f64 + f64::from(t.frac);
            assert!(
                (x - prev - 0.75).abs() < 1e-3 || prev == 1489.0,
                "{x} after {prev}"
            );
            prev = x;
        }
    }
}
