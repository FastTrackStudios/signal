//! 4× oversampling for the nonlinear stages.
//!
//! Two cascaded 2× polyphase all-pass half-band stages: zero latency, a
//! little phase near the top of the band — the same kind of filter the
//! references' zero-latency oversampling implies.
//!
//! Each 2× stage is `H(z) = ½[A₀(z²) + z⁻¹A₁(z²)]`, `A` a cascade of
//! first-order all-passes in `z²`; coefficients alternate between the two
//! branches. Stage 1 (10 coefficients) passes to 20 kHz at 48 kHz and
//! rejects ≥ 28 kHz by 96 dB; stage 2 (4 coefficients) has the whole octave
//! to roll off and rejects by 120 dB. Designed numerically (minimax on the
//! stop band) by the saturator-models tool `halfband.py`.

/// Stage 1 coefficients (sorted; even indices are branch 0).
#[expect(clippy::unreadable_literal, reason = "designed coefficients, generated")]
const STAGE1: [f64; 10] = [0.0320714673216549, 0.11095265436835794, 0.20387184033773734, 0.2980698862368612, 0.39830234813989873, 0.5028839895900741, 0.6096900499341105, 0.7196931449298735, 0.8300796258659789, 0.9415403637519943];
/// Stage 2 coefficients.
#[expect(clippy::unreadable_literal, reason = "designed coefficients, generated")]
const STAGE2: [f64; 4] = [0.04466474076366215, 0.20705549699837933, 0.5713026738938929, 0.9410306618368389];

/// One first-order all-pass in `z²`, run at the low rate.
#[derive(Clone, Copy, Debug, Default)]
struct Allpass {
    a: f64,
    x1: f64,
    y1: f64,
}

impl Allpass {
    #[inline]
    fn run(&mut self, x: f64) -> f64 {
        let y = self.a * x + self.x1 - self.a * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
}

/// The two branches of one 2× stage.
#[derive(Clone, Copy, Debug)]
struct Branches<const N: usize> {
    p0: [Allpass; N],
    p1: [Allpass; N],
}

impl<const N: usize> Branches<N> {
    fn new(coefs: &[f64]) -> Self {
        let mut p0 = [Allpass::default(); N];
        let mut p1 = [Allpass::default(); N];
        for (i, &a) in coefs.iter().enumerate() {
            let slot = if i % 2 == 0 { p0.get_mut(i / 2) } else { p1.get_mut(i / 2) };
            if let Some(s) = slot {
                s.a = a;
            }
        }
        Self { p0, p1 }
    }

    #[inline]
    fn branch0(&mut self, x: f64) -> f64 {
        self.p0.iter_mut().fold(x, |v, f| f.run(v))
    }

    #[inline]
    fn branch1(&mut self, x: f64) -> f64 {
        self.p1.iter_mut().fold(x, |v, f| f.run(v))
    }

    fn reset(&mut self) {
        for f in self.p0.iter_mut().chain(self.p1.iter_mut()) {
            f.x1 = 0.0;
            f.y1 = 0.0;
        }
    }
}

/// 2× interpolator: one sample in, two out (gain 1 in the pass band).
#[derive(Clone, Copy, Debug)]
struct Up2<const N: usize>(Branches<N>);

impl<const N: usize> Up2<N> {
    #[inline]
    fn run(&mut self, x: f64) -> [f64; 2] {
        [self.0.branch0(x), self.0.branch1(x)]
    }
}

/// 2× decimator: two samples in (earlier first), one out.
#[derive(Clone, Copy, Debug)]
struct Down2<const N: usize> {
    b: Branches<N>,
    odd: f64,
}

impl<const N: usize> Down2<N> {
    #[inline]
    fn run(&mut self, [even, odd]: [f64; 2]) -> f64 {
        let a = self.b.branch0(even);
        let b = self.b.branch1(self.odd);
        self.odd = odd;
        0.5 * (a + b)
    }
}

/// 4× up, a per-sample function at the high rate, 4× down.
#[derive(Clone, Copy, Debug)]
pub struct Oversample4 {
    up1: Up2<5>,
    up2: [Up2<2>; 2],
    down2: [Down2<2>; 2],
    down1: Down2<5>,
}

impl Default for Oversample4 {
    fn default() -> Self {
        Self::new()
    }
}

impl Oversample4 {
    #[must_use]
    pub fn new() -> Self {
        let s1 = Branches::<5>::new(&STAGE1);
        let s2 = Branches::<2>::new(&STAGE2);
        Self {
            up1: Up2(s1),
            up2: [Up2(s2), Up2(s2)],
            down2: [Down2 { b: s2, odd: 0.0 }, Down2 { b: s2, odd: 0.0 }],
            down1: Down2 { b: s1, odd: 0.0 },
        }
    }

    pub fn reset(&mut self) {
        self.up1.0.reset();
        self.down1.b.reset();
        self.down1.odd = 0.0;
        for (u, d) in self.up2.iter_mut().zip(self.down2.iter_mut()) {
            u.0.reset();
            d.b.reset();
            d.odd = 0.0;
        }
    }

    /// Run `f` on `x` at four times the rate.
    #[inline]
    pub fn process(&mut self, x: f64, mut f: impl FnMut(f64) -> f64) -> f64 {
        let [x0, x1] = self.up1.run(x);
        let [up_a, up_b] = &mut self.up2;
        let [y00, y01] = up_a.run(x0).map(&mut f);
        let [y10, y11] = up_b.run(x1).map(&mut f);
        let [down_a, down_b] = &mut self.down2;
        let z0 = down_a.run([y00, y01]);
        let z1 = down_b.run([y10, y11]);
        self.down1.run([z0, z1])
    }

    /// As [`Self::process`] for two signals that meet inside `f`
    /// (`f(a, b)` at the high rate).
    #[inline]
    pub fn process2(&mut self, other: &mut Self, a: f64, b: f64, mut f: impl FnMut(f64, f64) -> f64) -> f64 {
        let [a0, a1] = self.up1.run(a);
        let [b0, b1] = other.up1.run(b);
        let [ua, ub] = &mut self.up2;
        let [va, vb] = &mut other.up2;
        let [a00, a01] = ua.run(a0);
        let [a10, a11] = ub.run(a1);
        let [b00, b01] = va.run(b0);
        let [b10, b11] = vb.run(b1);
        let [da, db] = &mut self.down2;
        let z0 = da.run([f(a00, b00), f(a01, b01)]);
        let z1 = db.run([f(a10, b10), f(a11, b11)]);
        self.down1.run([z0, z1])
    }
}
