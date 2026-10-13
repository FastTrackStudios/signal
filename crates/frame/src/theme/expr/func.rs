//! Built-in functions.

use std::sync::Arc;

use crate::theme::expr::{Rgba, Value};

macro_rules! funcs {
    ($($v:ident = $name:literal, $lo:literal, $hi:expr;)*) => {
        /// A built-in function.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Func { $($v),* }

        impl Func {
            const ALL: &'static [(Func, &'static str, usize, usize)] = &[$((Func::$v, $name, $lo, $hi)),*];
        }
    };
}

funcs! {
    Min = "min", 1, usize::MAX;
    Max = "max", 1, usize::MAX;
    Clamp = "clamp", 3, 3;
    Abs = "abs", 1, 1;
    Floor = "floor", 1, 1;
    Ceil = "ceil", 1, 1;
    Round = "round", 1, 1;
    Lerp = "lerp", 3, 3;
    Step = "step", 2, 2;
    Smoothstep = "smoothstep", 3, 3;
    Pow = "pow", 2, 2;
    Sqrt = "sqrt", 1, 1;
    Sin = "sin", 1, 1;
    Cos = "cos", 1, 1;
    Db = "db", 1, 1;
    Amp = "amp", 1, 1;
    Rgb = "rgb", 3, 4;
    Mix = "mix", 3, 3;
    Alpha = "alpha", 2, 2;
    Fade = "fade", 2, 2;
    Lighten = "lighten", 2, 2;
    Darken = "darken", 2, 2;
    Lift = "lift", 2, 2;
    Luma = "luma", 1, 1;
    Ink = "ink", 1, 3;
    Upper = "upper", 1, 1;
    Lower = "lower", 1, 1;
    Fmt = "fmt", 1, 2;
    Len = "len", 1, 1;
    If = "if", 3, 3;
    Or = "or", 1, usize::MAX;
    Arc = "arc", 5, 5;
}

impl Func {
    #[must_use]
    pub fn named(name: &str) -> Option<Func> {
        Self::ALL
            .iter()
            .find(|(_, n, ..)| *n == name)
            .map(|(f, ..)| *f)
    }

    #[must_use]
    pub fn names() -> Vec<&'static str> {
        Self::ALL.iter().map(|(_, n, ..)| *n).collect()
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(f, ..)| *f == self)
            .map_or("?", |(_, n, ..)| n)
    }

    /// Fewest and most arguments.
    #[must_use]
    pub fn arity(self) -> (usize, usize) {
        Self::ALL
            .iter()
            .find(|(f, ..)| *f == self)
            .map_or((0, 0), |(_, _, lo, hi)| (*lo, *hi))
    }

    /// Apply to evaluated arguments. `If` and `Or` are lazy and handled by
    /// the evaluator; here they are the eager fallback.
    #[must_use]
    pub fn apply(self, a: &[Value]) -> Value {
        let n = |i: usize| a.get(i).and_then(Value::num);
        let c = |i: usize| a.get(i).and_then(Value::color);
        let t = |i: usize| a.get(i).and_then(Value::as_text);
        let num = |v: Option<f64>| v.map_or(Value::Missing, Value::Num);
        let col = |v: Option<Rgba>| v.map_or(Value::Missing, Value::Color);
        let txt = |v: Option<String>| v.map_or(Value::Missing, |s| Value::Text(Arc::from(s)));
        match self {
            Func::Min | Func::Max => {
                let xs: Option<Vec<f64>> = a.iter().map(Value::num).collect();
                num(xs.and_then(|xs| {
                    let pick = if self == Func::Min {
                        f64::min
                    } else {
                        f64::max
                    };
                    xs.into_iter().reduce(pick)
                }))
            }
            Func::Clamp => num((|| Some(n(0)?.max(n(1)?).min(n(2)?)))()),
            Func::Abs => num(n(0).map(f64::abs)),
            Func::Floor => num(n(0).map(f64::floor)),
            Func::Ceil => num(n(0).map(f64::ceil)),
            Func::Round => num(n(0).map(f64::round)),
            Func::Lerp => num((|| Some(n(0)? + (n(1)? - n(0)?) * n(2)?))()),
            Func::Step => num((|| Some(f64::from(u8::from(n(1)? >= n(0)?))))()),
            Func::Smoothstep => num((|| {
                let (e0, e1, x) = (n(0)?, n(1)?, n(2)?);
                let t = if (e1 - e0).abs() < f64::EPSILON {
                    f64::from(u8::from(x >= e1))
                } else {
                    ((x - e0) / (e1 - e0)).clamp(0.0, 1.0)
                };
                Some(t * t * (3.0 - 2.0 * t))
            })()),
            Func::Pow => num((|| Some(n(0)?.powf(n(1)?)))()),
            Func::Sqrt => num(n(0).map(|x| x.max(0.0).sqrt())),
            Func::Sin => num(n(0).map(f64::sin)),
            Func::Cos => num(n(0).map(f64::cos)),
            // Silence is the floor of every meter, not −∞.
            Func::Db => num(n(0).map(|x| if x <= 0.0 { -150.0 } else { 20.0 * x.log10() })),
            Func::Amp => num(n(0).map(|db| 10f64.powf(db / 20.0))),
            Func::Rgb => col((|| {
                let ch = |i| n(i).map(|v| v.clamp(0.0, 1.0) as f32);
                Some(Rgba::new(
                    ch(0)?,
                    ch(1)?,
                    ch(2)?,
                    if a.len() > 3 { ch(3)? } else { 1.0 },
                ))
            })()),
            Func::Mix => col((|| Some(c(0)?.mix(c(1)?, n(2)?)))()),
            Func::Alpha => col((|| {
                Some(Rgba {
                    a: n(1)?.clamp(0.0, 1.0) as f32,
                    ..c(0)?
                })
            })()),
            Func::Fade => col((|| {
                let base = c(0)?;
                Some(Rgba {
                    a: (f64::from(base.a) * n(1)?).clamp(0.0, 1.0) as f32,
                    ..base
                })
            })()),
            Func::Lighten => col((|| {
                let base = c(0)?;
                Some(Rgba {
                    a: base.a,
                    ..base.mix(Rgba::WHITE, n(1)?)
                })
            })()),
            Func::Darken => col((|| {
                let base = c(0)?;
                Some(Rgba {
                    a: base.a,
                    ..base.mix(Rgba::BLACK, n(1)?)
                })
            })()),
            // Each channel scaled up by `k` and clamped: a lit control's
            // hover, red running into the ceiling while green and blue
            // climb — brighter, not whiter, as `lighten` would make it.
            Func::Lift => col((|| {
                let (base, k) = (c(0)?, n(1)? as f32);
                let up = |v: f32| (v * (1.0 + k)).clamp(0.0, 1.0);
                Some(Rgba {
                    r: up(base.r),
                    g: up(base.g),
                    b: up(base.b),
                    a: base.a,
                })
            })()),
            Func::Luma => num(c(0).map(Rgba::luma)),
            Func::Ink => col((|| {
                let ground = c(0)?;
                let dark = if a.len() > 1 {
                    c(1)?
                } else {
                    Rgba::from_hex("#141414")?
                };
                let light = if a.len() > 2 {
                    c(2)?
                } else {
                    Rgba::from_hex("#f2f2f2")?
                };
                Some(if ground.luma() > 0.5 { dark } else { light })
            })()),
            Func::Upper => txt(t(0).map(|s| s.to_uppercase())),
            Func::Lower => txt(t(0).map(|s| s.to_lowercase())),
            Func::Fmt => txt((|| {
                let x = n(0)?;
                Some(match n(1) {
                    Some(d) => format!("{x:.*}", d.clamp(0.0, 9.0) as usize),
                    None => format!("{x}"),
                })
            })()),
            Func::Len => num(t(0).map(|s| s.chars().count() as f64)),
            Func::If => match a.first().and_then(Value::truthy) {
                Some(true) => a.get(1).cloned().unwrap_or(Value::Missing),
                Some(false) => a.get(2).cloned().unwrap_or(Value::Missing),
                None => Value::Missing,
            },
            Func::Or => a
                .iter()
                .find(|v| !v.is_missing())
                .cloned()
                .unwrap_or(Value::Missing),
            // Degrees clockwise from twelve o'clock, as knobs are drawn;
            // flattened as finely as a 9 px ring needs.
            Func::Arc => txt((|| {
                use kurbo::Shape as _;
                let arc = kurbo::Arc::new(
                    (n(0)?, n(1)?),
                    (n(2)?, n(2)?),
                    (n(3)? - 90.0).to_radians(),
                    n(4)?.to_radians(),
                    0.0,
                );
                Some(arc.to_path(0.05).to_svg())
            })()),
        }
    }
}
