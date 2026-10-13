//! Motion input expressions: a small, total arithmetic language.
//!
//! ```text
//! norm(room/size)                    a param, normalized 0..1
//! param(room/decay)                  a param in real units (seconds)
//! level(spring/in)                   a level feed, 0..1 over −60…0 dBFS (loudest channel; level(id, ch) for one)
//! gr(comp/gr)                        gain reduction, 0..1 over 0…20 dB
//! scalar(chorus/lfo)                 a scalar feed, 0..1 of its range
//! band(eq/spectrum, 60, 250)         a spectrum's energy between two frequencies, 0..1
//! feed(tuner/pitch, 1)               a feed frame's raw value at an index
//! motion(ring)                       another motion's output
//! time  hover  press  pi             seconds; 1 while the pointer is over / pressing the node's control
//! + - * /  ( )                       arithmetic
//! clamp(x, lo, hi) min max abs lerp(a, b, t) smoothstep(e0, e1, x) step(edge, x)
//! pow sqrt sin cos floor fract sign curve(x, k)  or(a, b, …)
//! ```
//!
//! A source with no data yet (a feed that hasn't sent a frame, an unknown
//! param) is *missing*; missing propagates through arithmetic, `or(a, b)`
//! picks the first present argument, and a motion whose input is missing
//! holds its rest. Addresses are written bare (`norm(room/size)`), or
//! quoted when they contain `,` or `)`.

use std::fmt;

/// A parsed expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Num(f64),
    /// Seconds since the clock started.
    Time,
    Hover,
    Press,
    Src(Source),
    /// Another motion's (mapped) output, by name.
    Motion(String),
    Neg(Box<Expr>),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
    Call(Func, Vec<Expr>),
}

/// Something the host samples.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Source {
    /// A param's value: normalized 0..1 (`norm`) or in real units (`param`).
    Param { addr: String, normalized: bool },
    /// A feed read one way.
    Feed { id: String, read: FeedRead },
}

/// How a feed frame becomes one number.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FeedRead {
    /// `feed(id, i)`: `values[i]` as sent.
    Raw(u32),
    /// `level(id[, ch])`: dBFS → 0..1 over −60…0; `None` = loudest channel.
    Level(Option<u8>),
    /// `gr(id)`: dB of reduction → 0..1 over 0…20.
    Gr,
    /// `scalar(id)`: 0..1 of the feed's range.
    Scalar,
    /// `band(id, lo, hi)`: a spectrum's level between `lo` and `hi` Hz
    /// (whole Hz), 0..1 over its floor…0 dB.
    Band(u32, u32),
}

/// Built-in functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Func {
    Clamp,
    Min,
    Max,
    Abs,
    Lerp,
    Smoothstep,
    Step,
    Pow,
    Sqrt,
    Sin,
    Cos,
    Floor,
    Fract,
    Sign,
    Curve,
    Or,
}

impl Func {
    const ALL: [(&'static str, Func, usize, usize); 16] = [
        ("clamp", Func::Clamp, 3, 3),
        ("min", Func::Min, 1, usize::MAX),
        ("max", Func::Max, 1, usize::MAX),
        ("abs", Func::Abs, 1, 1),
        ("lerp", Func::Lerp, 3, 3),
        ("smoothstep", Func::Smoothstep, 3, 3),
        ("step", Func::Step, 2, 2),
        ("pow", Func::Pow, 2, 2),
        ("sqrt", Func::Sqrt, 1, 1),
        ("sin", Func::Sin, 1, 1),
        ("cos", Func::Cos, 1, 1),
        ("floor", Func::Floor, 1, 1),
        ("fract", Func::Fract, 1, 1),
        ("sign", Func::Sign, 1, 1),
        ("curve", Func::Curve, 2, 2),
        ("or", Func::Or, 1, usize::MAX),
    ];

    /// Apply to present arguments (`Or` is handled by the evaluator).
    #[must_use]
    pub fn apply(self, a: &[f64]) -> f64 {
        let x = a.first().copied().unwrap_or(0.0);
        let y = a.get(1).copied().unwrap_or(0.0);
        let z = a.get(2).copied().unwrap_or(0.0);
        match self {
            Func::Clamp => x.max(y).min(z),
            Func::Min => a.iter().copied().fold(f64::INFINITY, f64::min),
            Func::Max => a.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            Func::Abs => x.abs(),
            Func::Lerp => x + (y - x) * z,
            Func::Smoothstep => {
                let t = if (y - x).abs() < f64::EPSILON { f64::from(u8::from(z >= y)) } else { ((z - x) / (y - x)).clamp(0.0, 1.0) };
                t * t * (3.0 - 2.0 * t)
            }
            Func::Step => f64::from(u8::from(y >= x)),
            Func::Pow => x.abs().powf(y) * x.signum(),
            Func::Sqrt => x.max(0.0).sqrt(),
            Func::Sin => x.sin(),
            Func::Cos => x.cos(),
            Func::Floor => x.floor(),
            Func::Fract => x - x.floor(),
            Func::Sign => {
                if x > 0.0 {
                    1.0
                } else if x < 0.0 {
                    -1.0
                } else {
                    0.0
                }
            }
            // A skew that keeps 0 and 1: k > 1 bends toward the end, < 1
            // toward the start (sign-preserving power).
            Func::Curve => x.abs().powf(y.max(1e-3)) * x.signum(),
            Func::Or => x,
        }
    }
}

/// Why an expression didn't parse (`at` = byte offset).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprError {
    pub at: usize,
    pub message: String,
}

impl fmt::Display for ExprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (at {})", self.message, self.at)
    }
}

impl std::error::Error for ExprError {}

/// Parse an expression.
///
/// # Errors
/// Where and why the text isn't an expression.
pub fn parse(src: &str) -> Result<Expr, ExprError> {
    let mut p = Parser { s: src.as_bytes(), src, i: 0 };
    let e = p.sum()?;
    p.ws();
    if p.i < p.s.len() {
        return p.err(format!("unexpected `{}`", &src[p.i..]));
    }
    Ok(e)
}

impl Expr {
    /// Visit this expression and every sub-expression.
    pub fn visit<'a>(&'a self, f: &mut impl FnMut(&'a Expr)) {
        f(self);
        match self {
            Expr::Neg(a) => a.visit(f),
            Expr::Add(a, b) | Expr::Sub(a, b) | Expr::Mul(a, b) | Expr::Div(a, b) => {
                a.visit(f);
                b.visit(f);
            }
            Expr::Call(_, args) => args.iter().for_each(|a| a.visit(f)),
            _ => {}
        }
    }

    /// Every source it samples.
    #[must_use]
    pub fn sources(&self) -> Vec<&Source> {
        let mut v = Vec::new();
        self.visit(&mut |e| {
            if let Expr::Src(s) = e {
                v.push(s);
            }
        });
        v
    }

    /// Every motion it reads.
    #[must_use]
    pub fn motions(&self) -> Vec<&str> {
        let mut v = Vec::new();
        self.visit(&mut |e| {
            if let Expr::Motion(m) = e {
                v.push(m.as_str());
            }
        });
        v
    }

    /// Whether it changes with time by itself (reads `time`).
    #[must_use]
    pub fn uses_time(&self) -> bool {
        let mut t = false;
        self.visit(&mut |e| t |= matches!(e, Expr::Time));
        t
    }

    /// A constant's value.
    #[must_use]
    pub fn constant(&self) -> Option<f64> {
        match self {
            Expr::Num(v) => Some(*v),
            Expr::Neg(a) => a.constant().map(|v| -v),
            _ => None,
        }
    }
}

struct Parser<'a> {
    s: &'a [u8],
    src: &'a str,
    i: usize,
}

impl Parser<'_> {
    fn err<T>(&self, message: impl Into<String>) -> Result<T, ExprError> {
        Err(ExprError { at: self.i, message: message.into() })
    }

    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    fn eat(&mut self, c: u8) -> bool {
        self.ws();
        if self.s.get(self.i) == Some(&c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn sum(&mut self) -> Result<Expr, ExprError> {
        let mut a = self.product()?;
        loop {
            if self.eat(b'+') {
                a = Expr::Add(Box::new(a), Box::new(self.product()?));
            } else if self.eat(b'-') {
                a = Expr::Sub(Box::new(a), Box::new(self.product()?));
            } else {
                return Ok(a);
            }
        }
    }

    fn product(&mut self) -> Result<Expr, ExprError> {
        let mut a = self.unary()?;
        loop {
            if self.eat(b'*') {
                a = Expr::Mul(Box::new(a), Box::new(self.unary()?));
            } else if self.eat(b'/') {
                a = Expr::Div(Box::new(a), Box::new(self.unary()?));
            } else {
                return Ok(a);
            }
        }
    }

    fn unary(&mut self) -> Result<Expr, ExprError> {
        if self.eat(b'-') {
            return Ok(match self.unary()? {
                Expr::Num(v) => Expr::Num(-v),
                e => Expr::Neg(Box::new(e)),
            });
        }
        if self.eat(b'+') {
            return self.unary();
        }
        self.atom()
    }

    fn atom(&mut self) -> Result<Expr, ExprError> {
        self.ws();
        let Some(&c) = self.s.get(self.i) else { return self.err("expected a value, found the end") };
        if c == b'(' {
            self.i += 1;
            let e = self.sum()?;
            if !self.eat(b')') {
                return self.err("expected `)`");
            }
            return Ok(e);
        }
        if c.is_ascii_digit() || c == b'.' {
            return self.number().map(Expr::Num);
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            let start = self.i;
            while self.i < self.s.len() && (self.s[self.i].is_ascii_alphanumeric() || self.s[self.i] == b'_') {
                self.i += 1;
            }
            let name = &self.src[start..self.i];
            if !self.eat(b'(') {
                return match name {
                    "time" | "t" => Ok(Expr::Time),
                    "hover" => Ok(Expr::Hover),
                    "press" => Ok(Expr::Press),
                    "pi" => Ok(Expr::Num(std::f64::consts::PI)),
                    _ => Err(ExprError { at: start, message: format!("unknown name `{name}` (sources are calls: norm(addr), level(id), …; also time, hover, press, pi)") }),
                };
            }
            return self.call(name, start);
        }
        self.err(format!("unexpected `{}`", c as char))
    }

    fn number(&mut self) -> Result<f64, ExprError> {
        self.ws();
        let start = self.i;
        while self.i < self.s.len() && (self.s[self.i].is_ascii_digit() || self.s[self.i] == b'.') {
            self.i += 1;
        }
        if self.i < self.s.len() && (self.s[self.i] == b'e' || self.s[self.i] == b'E') {
            let save = self.i;
            self.i += 1;
            if self.i < self.s.len() && (self.s[self.i] == b'-' || self.s[self.i] == b'+') {
                self.i += 1;
            }
            if self.i < self.s.len() && self.s[self.i].is_ascii_digit() {
                while self.i < self.s.len() && self.s[self.i].is_ascii_digit() {
                    self.i += 1;
                }
            } else {
                self.i = save;
            }
        }
        self.src[start..self.i].parse::<f64>().map_err(|_| ExprError { at: start, message: format!("bad number `{}`", &self.src[start..self.i]) })
    }

    /// A bare or quoted address up to `,` or `)`.
    fn address(&mut self) -> Result<String, ExprError> {
        self.ws();
        let q = self.s.get(self.i).copied();
        if q == Some(b'"') || q == Some(b'\'') {
            let quote = q.unwrap_or(b'"');
            self.i += 1;
            let start = self.i;
            while self.i < self.s.len() && self.s[self.i] != quote {
                self.i += 1;
            }
            if self.i >= self.s.len() {
                return self.err("unterminated quote");
            }
            let a = self.src[start..self.i].to_string();
            self.i += 1;
            return Ok(a);
        }
        let start = self.i;
        while self.i < self.s.len() && self.s[self.i] != b',' && self.s[self.i] != b')' {
            self.i += 1;
        }
        let a = self.src[start..self.i].trim();
        if a.is_empty() {
            return Err(ExprError { at: start, message: "expected an address".into() });
        }
        Ok(a.to_string())
    }

    fn close(&mut self, name: &str) -> Result<(), ExprError> {
        if self.eat(b')') { Ok(()) } else { self.err(format!("expected `)` to close {name}(…)")) }
    }

    fn int_arg(&mut self, name: &str) -> Result<f64, ExprError> {
        if !self.eat(b',') {
            return self.err(format!("{name}(…) needs another argument"));
        }
        self.number()
    }

    fn call(&mut self, name: &str, start: usize) -> Result<Expr, ExprError> {
        let feed = |id: String, read: FeedRead| Expr::Src(Source::Feed { id, read });
        match name {
            "param" | "norm" => {
                let addr = self.address()?;
                self.close(name)?;
                Ok(Expr::Src(Source::Param { addr, normalized: name == "norm" }))
            }
            "motion" => {
                let m = self.address()?;
                self.close(name)?;
                Ok(Expr::Motion(m))
            }
            "level" => {
                let id = self.address()?;
                let ch = if self.eat(b',') { Some(self.number()? as u8) } else { None };
                self.close(name)?;
                Ok(feed(id, FeedRead::Level(ch)))
            }
            "gr" | "scalar" => {
                let id = self.address()?;
                self.close(name)?;
                Ok(feed(id, if name == "gr" { FeedRead::Gr } else { FeedRead::Scalar }))
            }
            "feed" => {
                let id = self.address()?;
                let i = if self.eat(b',') { self.number()? as u32 } else { 0 };
                self.close(name)?;
                Ok(feed(id, FeedRead::Raw(i)))
            }
            "band" => {
                let id = self.address()?;
                let lo = self.int_arg(name)?;
                let hi = self.int_arg(name)?;
                self.close(name)?;
                if !(lo >= 0.0 && hi > lo) {
                    return Err(ExprError { at: start, message: format!("band(…) needs 0 ≤ lo < hi, got {lo}, {hi}") });
                }
                Ok(feed(id, FeedRead::Band(lo as u32, hi as u32)))
            }
            _ => {
                let Some((_, f, min, max)) = Func::ALL.iter().find(|(n, ..)| *n == name).copied() else {
                    return Err(ExprError { at: start, message: format!("unknown function `{name}`") });
                };
                let mut args = Vec::new();
                if !self.eat(b')') {
                    loop {
                        args.push(self.sum()?);
                        if self.eat(b')') {
                            break;
                        }
                        if !self.eat(b',') {
                            return self.err(format!("expected `,` or `)` in {name}(…)"));
                        }
                    }
                }
                if args.len() < min || args.len() > max {
                    let want = if min == max { format!("{min}") } else if max == usize::MAX { format!("at least {min}") } else { format!("{min}–{max}") };
                    return Err(ExprError { at: start, message: format!("{name}(…) takes {want} argument(s), got {}", args.len()) });
                }
                Ok(Expr::Call(f, args))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precedence_and_sources() {
        let e = parse("0.2 + norm(room/size) * 2 - -1").unwrap();
        let Expr::Sub(a, b) = &e else { panic!("{e:?}") };
        assert_eq!(**b, Expr::Num(-1.0));
        let Expr::Add(_, m) = &**a else { panic!() };
        assert!(matches!(&**m, Expr::Mul(..)));
        assert_eq!(e.sources(), [&Source::Param { addr: "room/size".into(), normalized: true }]);
        let e = parse("clamp(level(spring/in, 1) + band('eq/spec', 60, 250), 0, 1) * motion(ring) + time").unwrap();
        assert_eq!(e.sources().len(), 2);
        assert_eq!(e.motions(), ["ring"]);
        assert!(e.uses_time());
        assert_eq!(parse("1e-3").unwrap(), Expr::Num(0.001));
        assert_eq!(parse(" -2 ").unwrap().constant(), Some(-2.0));
    }

    #[test]
    fn errors_point_at_the_problem() {
        assert!(parse("norm(a/b").unwrap_err().message.contains("close norm"));
        assert!(parse("foo").unwrap_err().message.contains("unknown name"));
        assert!(parse("wobble(1)").unwrap_err().message.contains("unknown function"));
        assert!(parse("clamp(1, 2)").unwrap_err().message.contains("takes 3"));
        assert!(parse("1 2").unwrap_err().message.contains("unexpected"));
        assert!(parse("").is_err());
    }
}
