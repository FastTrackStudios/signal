//! A modelled mic as data: its measured responses and the few numbers
//! that describe how its controls move them.
//!
//! The responses come from a measurement of the reference (see
//! `signal-analyzer`'s `sphere_capture`): for each of the nine pattern
//! steps and each of five axis anchors (0, 45, 90, 135, 180°), the two
//! kernels `P` and `G` with `out = P∗(front + rear) + G∗(front − rear)` —
//! the pressure and gradient parts. Axis positions between anchors are a
//! linear crossfade of the two neighbouring anchors, which is exactly what
//! the reference does.
//!
//! The low-cut switch is, for most mics, an exact first-order move of the
//! mic's built-in high-pass corner; for the rest it changes the response in
//! ways no post-EQ reproduces, and those positions carry their own set of
//! anchors.
//!
//! Asset format (`.micm`, little-endian), version 6 — every field present:
//!
//! ```text
//! b"MICM" u32 version=6 u32 sample_rate u32 taps u32 patterns=9 u32 axes=5
//! f64 low_cut_hz[4]          built-in corner, then the three switch positions
//! f64 proximity_f0[9]        the mic's own gradient corner at each pattern step
//! u32 proximity_law          0 = Corner, 1 = Shelf, 2 = Dynamic
//! f64 proximity_params[4]    Corner: lo hi · Shelf: lo hi · Dynamic: up down cap
//! 4 × output stage, one per low-cut position:
//!   u32 present (0: use position 0's)
//!   u32 poly_len, f64 poly[]  the curve: s + poly[0] s² + poly[1] s³ + …
//!   u32 n, n × (u32 kind, f64 p[3])  sections after the curve:
//!                             kind 0 high-pass (p0 = Hz), 1 peak (Hz, Q, dB),
//!                             2 second-order high-pass (Hz, Q),
//!                             3 the curve's input clamp (p0, not a filter)
//!                             (the measured response, curve's high-passes
//!                             included; the engine derives the curve's input)
//! u32 kernel_sets            bit k: low-cut position k has its own anchor set
//!                            (bit 0, the switch off, is always set)
//! for each set bit k, ascending:
//!   u32 head, u32 modes, modes × (f64 re, f64 im)   the tail's modes z
//!   u32 groups, groups × (u32 start, f64 pole_hz, u32 count)   Laguerre functions
//!   patterns × axes × (P, G), each: f32 head[head], modes × (f64 re, f64 im) c,
//!                             f64 laguerre[Σ count]
//!                             kernel = head (to `head`) + Re Σ c·zᵐ (after it,
//!                             m from the head's end) + Σ laguerre·φ (all taps)
//! f64 delay                  the model's own fractional delay (samples)
//! f64 shelf_k[9]             the Shelf law's k at each pattern step (1 otherwise)
//! f64 dual_weight[9]         the weight dual mode gives this model's blocks when
//!                            crossfading them with another's (a 180 backward
//!                            model: the share v of its own gradient block
//!                            against mic 1's)
//! 4 × block                  the low end at each low-cut position
//! 9 × block                  the pressure block at each pattern step
//! u32 has_gradient, [block]  dual mode's gradient block where it is not the
//!                            proximity law's base
//!
//! block: f64 gain, u32 n ≤ 3, n × (u32 kind, f64 p[3]) — kind 0 high-pass
//!        (Hz), 1 second-order high-pass (Hz, Q), 2 first-order shelf (zero Hz,
//!        pole Hz), 3 peak (Hz, Q, gain)
//! ```

/// Pattern steps, Omni (0) to Figure-8 (8).
pub const PATTERNS: usize = 9;
/// Axis anchors, every 45° from 0 to 180.
pub const AXES: usize = 5;
/// Degrees between axis anchors.
pub const AXIS_STEP_DEG: f64 = 45.0;
/// Low-cut positions: off and three switch settings.
pub const LOW_CUTS: usize = 4;

/// How the proximity control moves a mic's gradient-path response.
///
/// Every law is one or two first-order corner moves relative to the
/// setting of 0 %, with `f0` the mic's own corner at the current pattern
/// step and `u` the setting as −1 … +1. Each move is `(zero, pole)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ProximityLaw {
    /// Condensers and ribbons: the pole moves from `f0` toward `lo` (20 Hz)
    /// linearly as `u` rises to +1, toward `hi` (500 Hz) with `u²` as it
    /// falls to −1.
    Corner { f0: [f64; PATTERNS], lo: f64, hi: f64 },
    /// As [`Self::Corner`], but the gradient path is a shelf rather than a
    /// high-pass: a zero follows the pole at `k` times its frequency.
    Shelf { f0: [f64; PATTERNS], lo: f64, hi: f64, k: [f64; PATTERNS] },
    /// Dynamics: raising proximity moves the pole down geometrically to
    /// `f0 / up`; lowering it slides a zero down from `f0` to
    /// `f0 / down` with `u²` — a deepening low shelf.
    ///
    /// Lowering it is a shelf `down^(u²)` deep that first raises the pole
    /// from `f0` toward `cap` (Hz); once the pole reaches `cap` it stays
    /// there and a second section's zero slides down from it. `cap` at or
    /// below `f0` (0 in older files) means the zero moves from the start.
    Dynamic { f0: [f64; PATTERNS], up: f64, down: f64, cap: f64 },
}

/// One `(zero, pole)` corner move, in Hz.
pub type Move = (f64, f64);

impl ProximityLaw {
    /// The whole gradient-path proximity filter at a setting, base
    /// included, as `(zero, pole)` sections: a high-pass at the corner
    /// (Corner), a shelf at it (Shelf), or only the moves (Dynamic, whose
    /// base is flat). The kernels carry the filter at 0 %.
    #[must_use]
    pub fn filter(self, percent: f64, pattern: usize) -> [Option<Move>; 2] {
        let u = (percent / 100.0).clamp(-1.0, 1.0);
        let corner = |f0: f64, lo: f64, hi: f64| if u >= 0.0 { (f0 - lo).mul_add(-u, f0) } else { (hi - f0).mul_add(u * u, f0) };
        match self {
            Self::Corner { lo, hi, .. } => [Some((0.0, corner(self.f0(pattern), lo, hi))), None],
            Self::Shelf { lo, hi, k, .. } => {
                let k = k.get(pattern).copied().unwrap_or(1.0);
                let c = corner(self.f0(pattern), lo, hi);
                [Some((k * c, c)), None]
            }
            Self::Dynamic { .. } => self.moves(percent, pattern),
        }
    }

    /// The mic's own corner at a pattern step.
    #[must_use]
    pub fn f0(self, pattern: usize) -> f64 {
        let table = match self {
            Self::Corner { f0, .. } | Self::Shelf { f0, .. } | Self::Dynamic { f0, .. } => f0,
        };
        table.get(pattern).copied().unwrap_or(100.0)
    }

    /// The corner moves for a proximity setting in percent.
    #[must_use]
    pub fn moves(self, percent: f64, pattern: usize) -> [Option<Move>; 2] {
        let u = (percent / 100.0).clamp(-1.0, 1.0);
        let corner = |f0: f64, lo: f64, hi: f64| if u >= 0.0 { (f0 - lo).mul_add(-u, f0) } else { (hi - f0).mul_add(u * u, f0) };
        match self {
            Self::Corner { f0, lo, hi } => {
                let f0 = f0.get(pattern).copied().unwrap_or(100.0);
                [Some((f0, corner(f0, lo, hi))), None]
            }
            Self::Shelf { f0, lo, hi, k } => {
                let f0 = f0.get(pattern).copied().unwrap_or(100.0);
                let k = k.get(pattern).copied().unwrap_or(1.0);
                let p = corner(f0, lo, hi);
                [Some((f0, p)), Some((k * p, k * f0))]
            }
            Self::Dynamic { f0, up, down, cap } => {
                let f0 = f0.get(pattern).copied().unwrap_or(100.0);
                let depth = down.powf(u * u);
                if u >= 0.0 {
                    [Some((f0, f0 * up.powf(-u))), None]
                } else if cap <= f0 {
                    [Some((f0 / depth, f0)), None]
                } else if f0 * depth <= cap {
                    [Some((f0, f0 * depth)), None]
                } else {
                    // the shelf's whole depth is f0·z / cap² = 1 / depth
                    [Some((f0, cap)), Some((cap.powi(2) / (f0 * depth), cap))]
                }
            }
        }
    }
}

/// One section of a low-end block, an analog prototype (bilinear,
/// prewarped to its corners).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LowSection {
    /// `s / (s + ω)`.
    HighPass(f64),
    /// `s² / (s² + s ω/q + ω²)`.
    HighPass2 { hz: f64, q: f64 },
    /// `(s + ω_zero) / (s + ω_pole)`: unity at the top, `zero/pole` at DC.
    Shelf { zero: f64, pole: f64 },
    /// `(s² + s·√g·ω/q + ω²) / (s² + s·ω/(√g·q) + ω²)`: `gain` (linear) at
    /// `hz`, unity away from it.
    Peak { hz: f64, q: f64, gain: f64 },
}

/// Most sections in a low-end block.
pub const MAX_LOW: usize = 3;

/// The low end a response carries at one low-cut position.
///
/// Dual mode crossfades it plainly, both paths alike, after the curve. It
/// is the built-in high-pass for a model whose switch is a corner move,
/// the switch's own filter for one measured at every position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LowEnd {
    pub gain: f64,
    pub sections: [Option<LowSection>; MAX_LOW],
}

impl LowEnd {
    /// No filter.
    pub const FLAT: Self = Self { gain: 1.0, sections: [None; MAX_LOW] };

    /// A first-order high-pass at `hz`.
    #[must_use]
    pub const fn high_pass(hz: f64) -> Self {
        Self { gain: 1.0, sections: [Some(LowSection::HighPass(hz)), None, None] }
    }
}

/// One modelled mic.
#[derive(Clone, Debug)]
pub struct MicModel {
    pub sample_rate: f64,
    pub taps: usize,
    /// One anchor set per low-cut position that has its own; `None` means
    /// the position is the off set plus a corner move.
    sets: [Option<Vec<f32>>; LOW_CUTS],
    /// The built-in high-pass corner (Filter off) and the three switch
    /// positions it moves to.
    pub low_cut_hz: [f64; LOW_CUTS],
    pub proximity: ProximityLaw,
    /// The output stage per low-cut position (`None`: position 0's).
    stages: [Option<OutputStage>; LOW_CUTS],
    /// The model's own fractional delay in samples (linear interpolation):
    /// 0.146 for every model but Sphere Diffuse (0). Only dual mode sees it.
    pub delay: f64,
    /// Dual mode's weight on this model's proximity filter, per pattern
    /// step (relative to Sphere Linear's; 1 when the file gives none).
    pub dual_weight: [f64; PATTERNS],
    /// The low-end block at each low-cut position.
    low_ends: [LowEnd; LOW_CUTS],
    /// Dual mode's pressure block at each pattern step.
    pressure_blocks: [LowEnd; PATTERNS],
    /// Dual mode's gradient block, where it is not the proximity law's
    /// base (the dynamics': a shelf).
    pub dual_gradient: Option<LowEnd>,
}

/// One section after the output curve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PostSection {
    /// First-order high-pass at this corner (Hz).
    HighPass(f64),
    /// Peaking EQ.
    Peak { hz: f64, q: f64, db: f64 },
    /// Second-order high-pass.
    HighPass2 { hz: f64, q: f64 },
}

/// The mic's output stage: a static curve, then filters.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OutputStage {
    /// `y = s + poly[0] s² + poly[1] s³ + …`.
    pub poly: [f64; MAX_POLY],
    pub post: Vec<PostSection>,
    /// The polynomial's input is held to ±this (the reference's curve stops
    /// growing beyond it); infinite when the file gives none.
    pub clamp: f64,
}

/// The asset format version this reads.
pub const VERSION: u32 = 6;

/// Highest curve coefficient stored (`s⁷`).
pub const MAX_POLY: usize = 6;
/// Most high-passes after the curve.
pub const MAX_POST: usize = 4;

/// Why an asset was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelError {
    BadMagic,
    BadVersion,
    BadShape,
    Truncated,
}

/// A little-endian reader over the asset.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
    sample_rate: f64,
}

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], ModelError> {
        let end = self.at.checked_add(N).ok_or(ModelError::Truncated)?;
        let s = self.bytes.get(self.at..end).ok_or(ModelError::Truncated)?;
        self.at = end;
        let mut out = [0u8; N];
        out.copy_from_slice(s);
        Ok(out)
    }

    fn u32(&mut self) -> Result<u32, ModelError> {
        self.take::<4>().map(u32::from_le_bytes)
    }

    fn f64(&mut self) -> Result<f64, ModelError> {
        self.take::<8>().map(f64::from_le_bytes)
    }

    fn f32s(&mut self, count: usize) -> Result<Vec<f32>, ModelError> {
        let len = count.checked_mul(4).ok_or(ModelError::BadShape)?;
        let end = self.at.checked_add(len).ok_or(ModelError::Truncated)?;
        let s = self.bytes.get(self.at..end).ok_or(ModelError::Truncated)?;
        self.at = end;
        Ok(s.chunks_exact(4).map(|c| f32::from_le_bytes([c.first().copied().unwrap_or(0), c.get(1).copied().unwrap_or(0), c.get(2).copied().unwrap_or(0), c.get(3).copied().unwrap_or(0)])).collect())
    }
}

/// One block: a gain and up to [`MAX_LOW`] sections.
fn read_block(r: &mut Reader<'_>) -> Result<LowEnd, ModelError> {
    let gain = r.f64()?;
    let n = dsp_core::u32_to_index(r.u32()?);
    if n > MAX_LOW {
        return Err(ModelError::BadShape);
    }
    let mut sections = [None; MAX_LOW];
    for section in sections.iter_mut().take(n) {
        let kind = r.u32()?;
        let (first, second, third) = (r.f64()?, r.f64()?, r.f64()?);
        *section = Some(match kind {
            0 => LowSection::HighPass(first),
            1 => LowSection::HighPass2 { hz: first, q: second },
            2 => LowSection::Shelf { zero: first, pole: second },
            3 => LowSection::Peak { hz: first, q: second, gain: third },
            _ => return Err(ModelError::BadShape),
        });
    }
    Ok(LowEnd { gain, sections })
}

/// Most tail modes a kernel set may carry.
const MAX_MODES: usize = 256;

/// Most Laguerre functions a kernel set may carry.
const MAX_LAGUERRE: usize = 64;

/// One kernel set, expanded to `kernels × taps`: each kernel is stored as
/// its first `head` taps, a tail `Re Σ c_k z_kᵐ` over modes the set shares
/// (`m` counted from the end of the head), and a sum of discrete Laguerre
/// functions over the whole kernel (the low-end correction, exact).
fn read_set(r: &mut Reader<'_>, taps: usize, kernels: usize) -> Result<Vec<f32>, ModelError> {
    let head = dsp_core::u32_to_index(r.u32()?);
    let modes = dsp_core::u32_to_index(r.u32()?);
    if head > taps || modes > MAX_MODES {
        return Err(ModelError::BadShape);
    }
    let mut z = Vec::with_capacity(modes);
    for _ in 0..modes {
        z.push((r.f64()?, r.f64()?));
    }
    let basis = read_laguerre(r, taps)?;
    let mut out = Vec::with_capacity(kernels.checked_mul(taps).ok_or(ModelError::BadShape)?);
    let mut c = Vec::with_capacity(modes);
    let mut kernel = vec![0.0f64; taps];
    for _ in 0..kernels {
        for (k, v) in kernel.iter_mut().zip(r.f32s(head)?) {
            *k = f64::from(v);
        }
        c.clear();
        for _ in 0..modes {
            c.push((r.f64()?, r.f64()?));
        }
        // w_k = c_k z_kᵐ, stepped one sample at a time
        let mut w = c.clone();
        for k in kernel.iter_mut().skip(head) {
            let mut sum = 0.0;
            for (wk, &(zr, zi)) in w.iter_mut().zip(&z) {
                sum += wk.0;
                *wk = (wk.0.mul_add(zr, -(wk.1 * zi)), wk.0.mul_add(zi, wk.1 * zr));
            }
            *k = sum;
        }
        for phi in &basis {
            let weight = r.f64()?;
            for (k, p) in kernel.iter_mut().zip(phi) {
                *k = weight.mul_add(*p, *k);
            }
        }
        #[expect(clippy::cast_possible_truncation, clippy::as_conversions, reason = "kernels are stored as f32")]
        out.extend(kernel.iter().map(|&v| v as f32));
    }
    Ok(out)
}

/// The set's Laguerre functions: groups of (start sample, pole Hz, count),
/// each `√(1 − a²) aⁿ` from `start` then repeated all-passes
/// `(−a + z⁻¹)/(1 − a z⁻¹)`, `a = e^(−2π·pole/fs)` — the functions the
/// low-end correction was fitted with.
fn read_laguerre(r: &mut Reader<'_>, taps: usize) -> Result<Vec<Vec<f64>>, ModelError> {
    let groups = dsp_core::u32_to_index(r.u32()?);
    let mut basis = Vec::new();
    for _ in 0..groups {
        let start = dsp_core::u32_to_index(r.u32()?);
        let pole_hz = r.f64()?;
        let count = dsp_core::u32_to_index(r.u32()?);
        if start > taps || basis.len().saturating_add(count) > MAX_LAGUERRE {
            return Err(ModelError::BadShape);
        }
        let a = (-core::f64::consts::TAU * pole_hz / r.sample_rate).exp();
        let mut v = vec![0.0; taps];
        let mut g = a.mul_add(-a, 1.0).sqrt();
        for x in v.iter_mut().skip(start) {
            *x = g;
            g *= a;
        }
        for _ in 0..count {
            let mut next = vec![0.0; taps];
            let (mut x1, mut y1) = (0.0, 0.0);
            for (y, &x) in next.iter_mut().zip(&v) {
                let out = a.mul_add(y1, (-a).mul_add(x, x1));
                *y = out;
                x1 = x;
                y1 = out;
            }
            basis.push(core::mem::replace(&mut v, next));
        }
    }
    Ok(basis)
}

/// Nine values, one per pattern step.
fn read_table(r: &mut Reader<'_>) -> Result<[f64; PATTERNS], ModelError> {
    let mut table = [0.0; PATTERNS];
    for v in &mut table {
        *v = r.f64()?;
    }
    Ok(table)
}

impl MicModel {
    /// Parse a `.micm` asset.
    ///
    /// # Errors
    /// The asset is not version 5 of the format, does not have the
    /// 9 × 5 layout, or is shorter than its header says.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ModelError> {
        let mut r = Reader { bytes, at: 0, sample_rate: 48_000.0 };
        if r.take::<4>()? != *b"MICM" {
            return Err(ModelError::BadMagic);
        }
        if r.u32()? != VERSION {
            return Err(ModelError::BadVersion);
        }
        let sample_rate = f64::from(r.u32()?);
        r.sample_rate = sample_rate;
        let taps = dsp_core::u32_to_index(r.u32()?);
        if dsp_core::u32_to_index(r.u32()?) != PATTERNS || dsp_core::u32_to_index(r.u32()?) != AXES {
            return Err(ModelError::BadShape);
        }
        let mut low_cut_hz = [0.0; LOW_CUTS];
        for v in &mut low_cut_hz {
            *v = r.f64()?;
        }
        let mut f0 = [0.0; PATTERNS];
        for v in &mut f0 {
            *v = r.f64()?;
        }
        let law = r.u32()?;
        let mut params = [0.0; 4];
        for v in &mut params {
            *v = r.f64()?;
        }
        let [first, second, third, _] = params;
        let proximity = match law {
            0 => ProximityLaw::Corner { f0, lo: first, hi: second },
            1 => ProximityLaw::Shelf { f0, lo: first, hi: second, k: [1.0; PATTERNS] },
            2 => ProximityLaw::Dynamic { f0, up: first, down: second, cap: third },
            _ => return Err(ModelError::BadShape),
        };
        let mut stages: [Option<OutputStage>; LOW_CUTS] = Default::default();
        for stage in &mut stages {
            if r.u32()? == 0 {
                continue;
            }
            let mut poly = [0.0; MAX_POLY];
            let poly_len = dsp_core::u32_to_index(r.u32()?);
            if poly_len > MAX_POLY {
                return Err(ModelError::BadShape);
            }
            for v in poly.iter_mut().take(poly_len) {
                *v = r.f64()?;
            }
            let n = dsp_core::u32_to_index(r.u32()?);
            if n > MAX_POST {
                return Err(ModelError::BadShape);
            }
            let mut post = Vec::with_capacity(n);
            let mut clamp = f64::INFINITY;
            for _ in 0..n {
                let kind = r.u32()?;
                let p = [r.f64()?, r.f64()?, r.f64()?];
                post.push(match kind {
                    0 => PostSection::HighPass(p[0]),
                    1 => PostSection::Peak { hz: p[0], q: p[1], db: p[2] },
                    2 => PostSection::HighPass2 { hz: p[0], q: p[1] },
                    3 => {
                        clamp = p[0];
                        continue;
                    }
                    _ => return Err(ModelError::BadShape),
                });
            }
            *stage = Some(OutputStage { poly, post, clamp });
        }
        let mask = r.u32()? | 1;
        let count = PATTERNS
            .checked_mul(AXES)
            .and_then(|n| n.checked_mul(2))
            .and_then(|n| n.checked_mul(taps))
            .ok_or(ModelError::BadShape)?;
        let kernels = count.checked_div(taps).unwrap_or(0);
        let mut sets: [Option<Vec<f32>>; LOW_CUTS] = Default::default();
        let mut bit = 1u32;
        for set in &mut sets {
            if mask & bit != 0 {
                *set = Some(read_set(&mut r, taps, kernels)?);
            }
            bit = bit.wrapping_shl(1);
        }
        let delay = r.f64()?;
        let shelf_k = read_table(&mut r)?;
        let mut proximity = proximity;
        if let ProximityLaw::Shelf { k, .. } = &mut proximity {
            *k = shelf_k;
        }
        let dual_weight = read_table(&mut r)?;
        let mut low_ends = [LowEnd::FLAT; LOW_CUTS];
        for block in &mut low_ends {
            *block = read_block(&mut r)?;
        }
        let mut pressure_blocks = [LowEnd::FLAT; PATTERNS];
        for block in &mut pressure_blocks {
            *block = read_block(&mut r)?;
        }
        let dual_gradient = if r.u32()? != 0 { Some(read_block(&mut r)?) } else { None };
        Ok(Self { sample_rate, taps, sets, low_cut_hz, proximity, stages, delay, dual_weight, low_ends, pressure_blocks, dual_gradient })
    }

    /// The output stage at a low-cut position.
    #[must_use]
    pub fn stage(&self, low_cut: usize) -> Option<&OutputStage> {
        self.stages.get(low_cut).and_then(Option::as_ref).or_else(|| self.stages.first().and_then(Option::as_ref))
    }

    /// Whether a low-cut position has its own anchors (otherwise it is the
    /// off set with the built-in corner moved).
    #[must_use]
    pub fn has_set(&self, low_cut: usize) -> bool {
        self.sets.get(low_cut).is_some_and(Option::is_some)
    }

    /// The low-end block at a low-cut position, as dual mode crossfades it.
    #[must_use]
    pub fn low_end(&self, position: usize) -> LowEnd {
        self.low_ends.get(position).copied().unwrap_or(LowEnd::FLAT)
    }

    /// Dual mode's pressure block at a pattern step.
    #[must_use]
    pub fn dual_pressure(&self, pattern: usize) -> LowEnd {
        self.pressure_blocks.get(pattern).copied().unwrap_or(LowEnd::FLAT)
    }

    /// The low-end block the kernels at a low-cut position carry: the
    /// position's own where it has anchors, else the off set's.
    #[must_use]
    pub fn carried_low_end(&self, position: usize) -> LowEnd {
        self.low_end(if self.has_set(position) { position } else { 0 })
    }

    /// The `(P, G)` kernels at one low-cut position, pattern step and axis
    /// anchor (the off set where the position has none of its own).
    #[must_use]
    pub fn anchor(&self, low_cut: usize, pattern: usize, axis: usize) -> Option<(&[f32], &[f32])> {
        let set = self.sets.get(low_cut).and_then(Option::as_ref).or_else(|| self.sets.first().and_then(Option::as_ref))?;
        let index = pattern.checked_mul(AXES)?.checked_add(axis)?;
        let pair = set.chunks_exact(self.taps.checked_mul(2)?).nth(index)?;
        Some(pair.split_at(self.taps))
    }
}
