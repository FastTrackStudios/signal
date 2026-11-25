//! The fused path in wide registers: eight lanes (AVX) or sixteen
//! (AVX-512) to a register instead of four, on x86-64.
//!
//! The four-lane path is what NAM's standard widths were written for, and
//! on a machine with wider registers it leaves half of each multiply-add on
//! the table: an eight-channel A2 layer is two SSE registers a frame, where
//! one AVX register holds the frame whole. This module is the same three
//! passes over the same memory — the padded frames, the rings, the
//! pre-activation buffer, the sinks are the parent's, untouched — with the
//! arithmetic in [`Wide`] registers whenever a padded frame is a whole
//! number of them.
//!
//! # Bit-exactness
//!
//! A lane is a channel, as it is in the parent, and a lane never sees its
//! neighbours, so what each channel of each frame gets is computed by the
//! same operations in the same order as the parent computes it: per tap, a
//! fused chain over the inputs ascending from zero, added to the running sum
//! from zero in tap order, then the bias, then the mixin (one fused product
//! onto zero, added to zero); the 1x1 the same chain, added to zero, then
//! its bias, then the residual. Only the width of the register differs, and
//! an input reaches the multiplier by a broadcast load rather than by a lane
//! of a four-input load: the same float either way. This path runs only
//! where the parent's runs fused (FMA is part of both feature sets below),
//! so the two are bit for bit, and the parent's tests, which hold the fused
//! path to the generic one, run this path on any machine that has it.

use core::arch::x86_64::{
    __m256, __m512, _mm256_add_ps, _mm256_broadcast_ss, _mm256_castps_pd, _mm256_fmadd_ps,
    _mm256_loadu_ps, _mm256_set1_ps, _mm256_setzero_ps, _mm256_storeu_ps, _mm512_add_ps,
    _mm512_broadcast_f32x8, _mm512_castpd_ps, _mm512_castps_pd, _mm512_castps256_ps512,
    _mm512_fmadd_ps, _mm512_insertf64x4, _mm512_loadu_ps, _mm512_set1_ps, _mm512_setzero_ps,
    _mm512_storeu_ps,
};

use super::{FusedLayers, Layer, Sink};

/// A register of `N` lanes, and the handful of operations the passes need.
///
/// # Safety
///
/// Every method is an intrinsic of a feature set that is not baseline: it
/// may only be called inside a function compiled with that set (see
/// [`FusedLayers::wide`]), and the pointers it takes must have `N` floats
/// (`bcast`: one) behind them.
trait Wide: Copy {
    const N: usize;
    unsafe fn zero() -> Self;
    unsafe fn splat(x: f32) -> Self;
    unsafe fn load(p: *const f32) -> Self;
    unsafe fn bcast(p: *const f32) -> Self;
    unsafe fn store(self, p: *mut f32);
    unsafe fn add(self, b: Self) -> Self;
    /// `self + w * x`, one rounding.
    unsafe fn fma(self, w: Self, x: Self) -> Self;
}

#[derive(Clone, Copy)]
struct Y(__m256);

impl Wide for Y {
    const N: usize = 8;
    #[inline(always)]
    unsafe fn zero() -> Self {
        Self(unsafe { _mm256_setzero_ps() })
    }
    #[inline(always)]
    unsafe fn splat(x: f32) -> Self {
        Self(unsafe { _mm256_set1_ps(x) })
    }
    #[inline(always)]
    unsafe fn load(p: *const f32) -> Self {
        Self(unsafe { _mm256_loadu_ps(p) })
    }
    #[inline(always)]
    unsafe fn bcast(p: *const f32) -> Self {
        Self(unsafe { _mm256_broadcast_ss(&*p) })
    }
    #[inline(always)]
    unsafe fn store(self, p: *mut f32) {
        unsafe { _mm256_storeu_ps(p, self.0) }
    }
    #[inline(always)]
    unsafe fn add(self, b: Self) -> Self {
        Self(unsafe { _mm256_add_ps(self.0, b.0) })
    }
    #[inline(always)]
    unsafe fn fma(self, w: Self, x: Self) -> Self {
        Self(unsafe { _mm256_fmadd_ps(w.0, x.0, self.0) })
    }
}

#[derive(Clone, Copy)]
struct Z(__m512);

impl Wide for Z {
    const N: usize = 16;
    #[inline(always)]
    unsafe fn zero() -> Self {
        Self(unsafe { _mm512_setzero_ps() })
    }
    #[inline(always)]
    unsafe fn splat(x: f32) -> Self {
        Self(unsafe { _mm512_set1_ps(x) })
    }
    #[inline(always)]
    unsafe fn load(p: *const f32) -> Self {
        Self(unsafe { _mm512_loadu_ps(p) })
    }
    #[inline(always)]
    unsafe fn bcast(p: *const f32) -> Self {
        Self(unsafe { _mm512_set1_ps(*p) })
    }
    #[inline(always)]
    unsafe fn store(self, p: *mut f32) {
        unsafe { _mm512_storeu_ps(p, self.0) }
    }
    #[inline(always)]
    unsafe fn add(self, b: Self) -> Self {
        Self(unsafe { _mm512_add_ps(self.0, b.0) })
    }
    #[inline(always)]
    unsafe fn fma(self, w: Self, x: Self) -> Self {
        Self(unsafe { _mm512_fmadd_ps(w.0, x.0, self.0) })
    }
}

/// Pairs of frames a tile at exactly eight channels on AVX-512.
const PAIRS: usize = 8;

/// Which wide path a machine and a width take, if any. One answer per
/// process for a given width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Path {
    /// Sixteen lanes, and AVX-512's 32 registers for the eight-lane code.
    Avx512,
    /// Eight lanes, sixteen registers.
    Avx2,
}

/// Whether this CPU has `path`'s features.
pub(super) fn available(path: Path) -> bool {
    let avx2 = crate::kernels::fused() && std::arch::is_x86_feature_detected!("avx2");
    match path {
        Path::Avx2 => avx2,
        Path::Avx512 => {
            avx2 && std::arch::is_x86_feature_detected!("avx512f")
                && std::arch::is_x86_feature_detected!("avx512vl")
                && std::arch::is_x86_feature_detected!("avx512dq")
        }
    }
}

/// The path for padded frames of `width` floats on this machine.
pub(super) fn path(width: usize) -> Option<Path> {
    if !width.is_multiple_of(8) {
        return None;
    }
    [Path::Avx512, Path::Avx2]
        .into_iter()
        .find(|&p| available(p))
}

/// One tap's (or the 1x1's) contribution to a tile of `T` frames: for each
/// frame and each register of outputs, `acc = acc + w[i] * x[i]` over the
/// inputs `i` ascending, fused.
///
/// `w` is `channels` columns of `R * W::N` floats; `x[t]` the start of the
/// tile's frame `t`, at least `channels` floats.
#[inline(always)]
unsafe fn chain<W: Wide, const R: usize, const T: usize>(
    acc: &mut [[W; R]; T],
    w: &[f32],
    channels: usize,
    x: [*const f32; T],
) {
    let width = R * W::N;
    debug_assert!(w.len() >= channels * width);
    let w = w.as_ptr();
    for i in 0..channels {
        // SAFETY: column `i` is `width` floats inside `w` (asserted by the
        // callers' slicing), and `x[t] + i` is inside frame `t`.
        unsafe {
            let col: [W; R] = core::array::from_fn(|r| W::load(w.add(i * width + r * W::N)));
            for (a, &xt) in acc.iter_mut().zip(&x) {
                let xi = W::bcast(xt.add(i));
                for (a, c) in a.iter_mut().zip(&col) {
                    *a = a.fma(*c, xi);
                }
            }
        }
    }
}

/// The first pass over one segment, `T` frames at a time from frame `at`;
/// returns the frame it stopped at. The parent's `conv_tiles`.
#[inline(always)]
#[allow(clippy::too_many_arguments)]
unsafe fn conv_tiles<W: Wide, const R: usize, const T: usize>(
    layer: &Layer,
    channels: usize,
    starts: &[usize],
    cond: &[f32],
    z: &mut [f32],
    mut at: usize,
) -> usize {
    let width = R * W::N;
    let tap_len = channels * width;
    unsafe {
        let zero = W::zero();
        assert!(layer.conv_b.len() >= width && layer.mixin_w.len() >= width);
        let bias: [W; R] = core::array::from_fn(|r| W::load(layer.conv_b[r * W::N..].as_ptr()));
        let mixin: [W; R] = core::array::from_fn(|r| W::load(layer.mixin_w[r * W::N..].as_ptr()));
        while at + T <= cond.len() {
            let mut sum = [[zero; R]; T];
            for (k, &start) in starts.iter().enumerate() {
                let w = &layer.conv_w[k * tap_len..(k + 1) * tap_len];
                let tile = &layer.ring[(start + at) * width..(start + at + T) * width];
                let x: [*const f32; T] = core::array::from_fn(|t| tile[t * width..].as_ptr());
                let mut acc = [[zero; R]; T];
                chain::<W, R, T>(&mut acc, w, channels, x);
                for (s, a) in sum.iter_mut().zip(&acc) {
                    for (s, a) in s.iter_mut().zip(a) {
                        *s = s.add(*a);
                    }
                }
            }
            for (t, s) in sum.iter().enumerate() {
                let c = W::splat(cond[at + t]);
                let frame = &mut z[(at + t) * channels..(at + t) * channels + width];
                let out = frame.as_mut_ptr();
                for r in 0..R {
                    let mix = zero.add(zero.fma(mixin[r], c));
                    s[r].add(bias[r]).add(mix).store(out.add(r * W::N));
                }
            }
            at += T;
        }
    }
    at
}

/// The third pass over one segment, `T` frames at a time from frame `at`;
/// returns the frame it stopped at. The parent's `mix_tiles`. `x` is the
/// segment's input frames, `width` apart.
#[inline(always)]
unsafe fn mix_tiles<W: Wide, const R: usize, const T: usize>(
    layer: &Layer,
    channels: usize,
    z: &[f32],
    x: &[f32],
    out: &mut [f32],
    out_stride: usize,
    mut at: usize,
) -> usize {
    let width = R * W::N;
    let frames = x.len() / width;
    unsafe {
        let zero = W::zero();
        assert!(layer.l1x1_b.len() >= width && layer.l1x1_w.len() >= channels * width);
        let bias: [W; R] = core::array::from_fn(|r| W::load(layer.l1x1_b[r * W::N..].as_ptr()));
        while at + T <= frames {
            let zt = &z[at * channels..(at + T) * channels];
            let zp: [*const f32; T] = core::array::from_fn(|t| zt[t * channels..].as_ptr());
            let mut acc = [[zero; R]; T];
            chain::<W, R, T>(&mut acc, &layer.l1x1_w, channels, zp);
            let xt = &x[at * width..(at + T) * width];
            for (t, a) in acc.iter().enumerate() {
                let frame = &mut out[(at + t) * out_stride..(at + t) * out_stride + width];
                let o = frame.as_mut_ptr();
                let xp = xt[t * width..].as_ptr();
                for r in 0..R {
                    let residual = zero.add(a[r]).add(bias[r]);
                    W::load(xp.add(r * W::N))
                        .add(residual)
                        .store(o.add(r * W::N));
                }
            }
            at += T;
        }
    }
    at
}

// ---------------------------------------------------------------------------
// Two frames to a register.
//
// At exactly eight channels a frame is one AVX register, and an AVX-512
// register holds two: frame `t` in the low half, `t + 1` in the high. Every
// lane is still one channel of one frame, computed by the operations above
// in the same order; what changes is that one fused multiply-add does both
// frames. An input reaches the multiplier as two broadcasts, one a half.
// Frames of eight channels are adjacent in every buffer this path touches
// (the rings, the pre-activation, the sinks: all eight floats a frame), so a
// pair is one load or store.

/// `[*a; 8] ++ [*b; 8]`.
#[inline(always)]
unsafe fn pair_bcast(a: *const f32, b: *const f32) -> __m512 {
    unsafe {
        let lo = _mm512_castps_pd(_mm512_castps256_ps512(_mm256_broadcast_ss(&*a)));
        let hi = _mm256_castps_pd(_mm256_broadcast_ss(&*b));
        _mm512_castpd_ps(_mm512_insertf64x4::<1>(lo, hi))
    }
}

/// The eight floats at `p`, twice.
#[inline(always)]
unsafe fn twice(p: *const f32) -> __m512 {
    unsafe { _mm512_broadcast_f32x8(_mm256_loadu_ps(p)) }
}

/// [`chain`] over `P` pairs of frames: eight inputs, eight outputs.
#[inline(always)]
unsafe fn chain_pair<const P: usize>(acc: &mut [Z; P], w: &[f32], x: &[*const f32]) {
    assert!(w.len() >= 64 && x.len() >= 2 * P);
    let w = w.as_ptr();
    for i in 0..8 {
        // SAFETY: column `i` is eight floats inside `w`; `x[t] + i` is
        // inside frame `t`, which has eight.
        unsafe {
            let col = Z(twice(w.add(i * 8)));
            for (p, a) in acc.iter_mut().enumerate() {
                let xi = Z(pair_bcast(x[2 * p].add(i), x[2 * p + 1].add(i)));
                *a = a.fma(col, xi);
            }
        }
    }
}

/// [`conv_tiles`] at eight channels, `2 * P` frames at a time.
#[inline(always)]
unsafe fn conv_tiles_pair<const P: usize>(
    layer: &Layer,
    starts: &[usize],
    cond: &[f32],
    z: &mut [f32],
    mut at: usize,
) -> usize {
    const C: usize = 8;
    unsafe {
        let zero = Z::zero();
        assert!(layer.conv_b.len() >= C && layer.mixin_w.len() >= C);
        let bias = Z(twice(layer.conv_b.as_ptr()));
        let mixin = Z(twice(layer.mixin_w.as_ptr()));
        while at + 2 * P <= cond.len() {
            let mut sum = [zero; P];
            for (k, &start) in starts.iter().enumerate() {
                let w = &layer.conv_w[k * C * C..(k + 1) * C * C];
                let tile = &layer.ring[(start + at) * C..(start + at + 2 * P) * C];
                let x: [*const f32; 16] =
                    core::array::from_fn(|t| tile[(t % (2 * P)) * C..].as_ptr());
                let mut acc = [zero; P];
                chain_pair::<P>(&mut acc, w, &x);
                for (s, a) in sum.iter_mut().zip(&acc) {
                    *s = s.add(*a);
                }
            }
            let frames = &mut z[at * C..(at + 2 * P) * C];
            let out = frames.as_mut_ptr();
            for (p, s) in sum.iter().enumerate() {
                let c = Z(pair_bcast(&cond[at + 2 * p], &cond[at + 2 * p + 1]));
                let mix = zero.add(zero.fma(mixin, c));
                s.add(bias).add(mix).store(out.add(2 * p * C));
            }
            at += 2 * P;
        }
    }
    at
}

/// [`mix_tiles`] at eight channels, `2 * P` frames at a time. `out_stride`
/// is eight.
#[inline(always)]
unsafe fn mix_tiles_pair<const P: usize>(
    layer: &Layer,
    z: &[f32],
    x: &[f32],
    out: &mut [f32],
    mut at: usize,
) -> usize {
    const C: usize = 8;
    let frames = x.len() / C;
    unsafe {
        let zero = Z::zero();
        assert!(layer.l1x1_b.len() >= C);
        let bias = Z(twice(layer.l1x1_b.as_ptr()));
        while at + 2 * P <= frames {
            let zt = &z[at * C..(at + 2 * P) * C];
            let zp: [*const f32; 16] = core::array::from_fn(|t| zt[(t % (2 * P)) * C..].as_ptr());
            let mut acc = [zero; P];
            chain_pair::<P>(&mut acc, &layer.l1x1_w, &zp);
            let xt = &x[at * C..(at + 2 * P) * C];
            let o = &mut out[at * C..(at + 2 * P) * C];
            for (p, a) in acc.iter().enumerate() {
                let residual = zero.add(*a).add(bias);
                Z::load(xt.as_ptr().add(2 * p * C))
                    .add(residual)
                    .store(o.as_mut_ptr().add(2 * p * C));
            }
            at += 2 * P;
        }
    }
    at
}

impl Layer {
    /// The parent's `convolve`, in wide registers.
    #[inline(always)]
    unsafe fn convolve_wide<W: Wide, const R: usize, const T: usize, const P: usize>(
        &self,
        channels: usize,
        cond: &[f32],
        z: &mut [f32],
        starts: &mut [usize],
        n: usize,
    ) {
        let cols = self.cols;
        let starts = &mut starts[..self.kernel_size];
        for (k, s) in starts.iter_mut().enumerate() {
            let back = (self.kernel_size - 1 - k) * self.dilation;
            *s = (self.write + cols - back) % cols;
        }
        let mut done = 0;
        while done < n {
            let mut len = n - done;
            for &s in starts.iter() {
                len = len.min(cols - s);
            }
            let cond = &cond[done..done + len];
            let z = &mut z[done * channels..];
            unsafe {
                let at = if P > 0 {
                    conv_tiles_pair::<P>(self, starts, cond, z, 0)
                } else {
                    0
                };
                let at = conv_tiles::<W, R, T>(self, channels, starts, cond, z, at);
                conv_tiles::<W, R, 1>(self, channels, starts, cond, z, at);
            }
            for s in starts.iter_mut() {
                *s += len;
                if *s == cols {
                    *s = 0;
                }
            }
            done += len;
        }
    }

    /// The parent's `mix`, in wide registers.
    #[inline(always)]
    unsafe fn mix_wide<W: Wide, const R: usize, const T: usize, const P: usize>(
        &self,
        channels: usize,
        z: &[f32],
        to: Sink<'_>,
        n: usize,
    ) {
        let width = R * W::N;
        let (mut src, mut dst) = (self.write, to.at);
        let mut done = 0;
        while done < n {
            let len = (n - done).min(self.cols - src).min(to.cols - dst);
            let x = &self.ring[src * width..(src + len) * width];
            let z = &z[done * channels..];
            let out = &mut to.frames[dst * to.stride..];
            unsafe {
                let at = if P > 0 {
                    mix_tiles_pair::<P>(self, z, x, out, 0)
                } else {
                    0
                };
                let at = mix_tiles::<W, R, T>(self, channels, z, x, out, to.stride, at);
                mix_tiles::<W, R, 1>(self, channels, z, x, out, to.stride, at);
            }
            src += len;
            if src == self.cols {
                src = 0;
            }
            dst += len;
            if dst == to.cols {
                dst = 0;
            }
            done += len;
        }
    }
}

impl FusedLayers {
    /// Run the layers in wide registers, by `path`. The parent's `run`.
    pub(super) fn wide(
        &mut self,
        path: Path,
        input: &crate::buffer::Buf,
        condition: &crate::buffer::Buf,
        head: &mut crate::buffer::Buf,
        n: usize,
    ) {
        let width = 4 * self.lanes;
        // SAFETY: `path` came from `path()`, which saw the features each
        // twin is compiled for on this CPU.
        unsafe {
            match (path, width, self.channels) {
                (Path::Avx512, 16, _) => self.wide_avx512::<Z, 1, 8, 0>(input, condition, head, n),
                (Path::Avx512, _, 8) => {
                    self.wide_avx512::<Y, 1, 8, PAIRS>(input, condition, head, n)
                }
                (Path::Avx512, _, _) => self.wide_avx512::<Y, 1, 8, 0>(input, condition, head, n),
                (Path::Avx2, 16, _) => self.wide_avx2::<Y, 2, 4, 0>(input, condition, head, n),
                (Path::Avx2, _, _) => self.wide_avx2::<Y, 1, 6, 0>(input, condition, head, n),
            }
        }
    }

    #[target_feature(enable = "avx,avx2,fma,avx512f,avx512vl,avx512dq")]
    unsafe fn wide_avx512<W: Wide, const R: usize, const T: usize, const P: usize>(
        &mut self,
        input: &crate::buffer::Buf,
        condition: &crate::buffer::Buf,
        head: &mut crate::buffer::Buf,
        n: usize,
    ) {
        unsafe { self.run_wide::<W, R, T, P>(input, condition, head, n) }
    }

    #[target_feature(enable = "avx,avx2,fma")]
    unsafe fn wide_avx2<W: Wide, const R: usize, const T: usize, const P: usize>(
        &mut self,
        input: &crate::buffer::Buf,
        condition: &crate::buffer::Buf,
        head: &mut crate::buffer::Buf,
        n: usize,
    ) {
        unsafe { self.run_wide::<W, R, T, P>(input, condition, head, n) }
    }

    #[inline(always)]
    unsafe fn run_wide<W: Wide, const R: usize, const T: usize, const P: usize>(
        &mut self,
        input: &crate::buffer::Buf,
        condition: &crate::buffer::Buf,
        head: &mut crate::buffer::Buf,
        n: usize,
    ) {
        debug_assert_eq!(R * W::N, 4 * self.lanes);
        let Self {
            channels,
            layers,
            z,
            output,
            wants_output,
            starts,
            ..
        } = self;
        let c = *channels;
        let cond = condition.left(n);
        layers[0].write_frames(input.left(n), c, n);
        for i in 0..layers.len() {
            let (done, rest) = layers.split_at_mut(i + 1);
            let layer = &mut done[i];
            unsafe { layer.convolve_wide::<W, R, T, P>(c, cond, z, starts, n) };
            layer.activation.apply(&mut z[..n * c]);
            for (h, z) in head.left_mut(n).iter_mut().zip(&z[..n * c]) {
                *h += *z;
            }
            match rest.first_mut() {
                Some(next) => {
                    let to = Sink {
                        frames: &mut next.ring,
                        stride: R * W::N,
                        cols: next.cols,
                        at: next.write,
                    };
                    unsafe { layer.mix_wide::<W, R, T, P>(c, z, to, n) };
                }
                None if *wants_output => {
                    let to = Sink {
                        frames: output.data_mut(),
                        stride: c,
                        cols: usize::MAX,
                        at: 0,
                    };
                    unsafe { layer.mix_wide::<W, R, T, P>(c, z, to, n) };
                }
                None => {}
            }
            layer.write = (layer.write + n) % layer.cols;
        }
    }
}
