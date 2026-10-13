//! Source → [`Expr`]: a lexer and a precedence-climbing parser.

use std::fmt;

use crate::{BinOp, Expr, Field, Func, Rgba, UnOp, Value};

/// Why an expression didn't parse (`at` = byte offset).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub at: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (at {})", self.message, self.at)
    }
}

impl std::error::Error for ParseError {}

/// Parse a whole expression.
///
/// # Errors
/// The first problem found.
pub fn parse(src: &str) -> Result<Expr, ParseError> {
    let tokens = lex(src)?;
    let mut p = Parser { tokens, i: 0, src_len: src.len() };
    let e = p.expr()?;
    match p.peek() {
        None => Ok(e),
        Some((at, t)) => Err(ParseError { at, message: format!("unexpected {t} after the expression") }),
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Str(String),
    Color(Rgba),
    /// A bare name: `w`, `true`, a function, or a dotted field path.
    Name(String),
    /// `$name`.
    Token(String),
    Punct(&'static str),
}

impl fmt::Display for Tok {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Tok::Num(n) => write!(f, "`{n}`"),
            Tok::Str(s) => write!(f, "\"{s}\""),
            Tok::Color(c) => write!(f, "`{}`", c.to_hex()),
            Tok::Name(n) => write!(f, "`{n}`"),
            Tok::Token(n) => write!(f, "`${n}`"),
            Tok::Punct(p) => write!(f, "`{p}`"),
        }
    }
}

const PUNCT: [&str; 21] =
    ["==", "!=", "<=", ">=", "&&", "||", "+", "-", "*", "/", "%", "<", ">", "!", "?", ":", "(", ")", ",", "{", "}"];

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.'
}

fn lex(src: &str) -> Result<Vec<(usize, Tok)>, ParseError> {
    let mut out = Vec::new();
    let bytes = src.as_bytes();
    let mut i = 0;
    while i < src.len() {
        let rest = &src[i..];
        let c = rest.chars().next().unwrap_or(' ');
        if c.is_whitespace() {
            i += c.len_utf8();
            continue;
        }
        let start = i;
        if c.is_ascii_digit() || (c == '.' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            let len = rest.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E')).unwrap_or(rest.len());
            // An exponent's sign: `1e-3`.
            let mut len = len;
            if rest[..len].ends_with(['e', 'E']) && rest[len..].starts_with(['-', '+']) {
                len += 1 + rest[len + 1..].find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len() - len - 1);
            }
            let text = &rest[..len];
            let n = text.parse::<f64>().map_err(|_| ParseError { at: start, message: format!("bad number `{text}`") })?;
            out.push((start, Tok::Num(n)));
            i += len;
        } else if c == '"' {
            let mut s = String::new();
            let mut j = i + 1;
            loop {
                match src[j..].chars().next() {
                    None => return Err(ParseError { at: start, message: "unterminated string".into() }),
                    Some('"') => break,
                    Some('\\') => {
                        let esc = src[j + 1..].chars().next().ok_or(ParseError { at: j, message: "unterminated string".into() })?;
                        s.push(match esc {
                            'n' => '\n',
                            't' => '\t',
                            other => other,
                        });
                        j += 1 + esc.len_utf8();
                    }
                    Some(ch) => {
                        s.push(ch);
                        j += ch.len_utf8();
                    }
                }
            }
            out.push((start, Tok::Str(s)));
            i = j + 1;
        } else if c == '#' {
            let len = 1 + rest[1..].find(|c: char| !c.is_ascii_hexdigit()).unwrap_or(rest.len() - 1);
            let text = &rest[..len];
            let color = Rgba::from_hex(text)
                .ok_or_else(|| ParseError { at: start, message: format!("bad colour `{text}` — expected #rgb, #rrggbb or #rrggbbaa") })?;
            out.push((start, Tok::Color(color)));
            i += len;
        } else if c == '$' {
            let len = rest[1..].find(|c: char| !is_name_char(c)).unwrap_or(rest.len() - 1);
            if len == 0 {
                return Err(ParseError { at: start, message: "`$` must be followed by a token's name".into() });
            }
            out.push((start, Tok::Token(rest[1..=len].to_owned())));
            i += 1 + len;
        } else if c.is_ascii_alphabetic() || c == '_' {
            let len = rest.find(|c: char| !is_name_char(c)).unwrap_or(rest.len());
            out.push((start, Tok::Name(rest[..len].to_owned())));
            i += len;
        } else if let Some(p) = PUNCT.iter().find(|p| rest.starts_with(**p)) {
            out.push((start, Tok::Punct(p)));
            i += p.len();
        } else {
            return Err(ParseError { at: start, message: format!("unexpected `{c}`") });
        }
    }
    Ok(out)
}

struct Parser {
    tokens: Vec<(usize, Tok)>,
    i: usize,
    src_len: usize,
}

/// Binary operators by precedence, loosest first (`?:` is looser still).
const LEVELS: [&[(&str, BinOp)]; 5] = [
    &[("||", BinOp::Or)],
    &[("&&", BinOp::And)],
    &[("==", BinOp::Eq), ("!=", BinOp::Ne), ("<=", BinOp::Le), (">=", BinOp::Ge), ("<", BinOp::Lt), (">", BinOp::Gt)],
    &[("+", BinOp::Add), ("-", BinOp::Sub)],
    &[("*", BinOp::Mul), ("/", BinOp::Div), ("%", BinOp::Rem)],
];

impl Parser {
    fn peek(&self) -> Option<(usize, &Tok)> {
        self.tokens.get(self.i).map(|(at, t)| (*at, t))
    }

    fn at(&self) -> usize {
        self.peek().map_or(self.src_len, |(at, _)| at)
    }

    fn eat(&mut self, p: &str) -> bool {
        if matches!(self.peek(), Some((_, Tok::Punct(q))) if *q == p) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, p: &str) -> Result<(), ParseError> {
        if self.eat(p) {
            return Ok(());
        }
        let found = self.peek().map_or_else(|| "the end".to_owned(), |(_, t)| t.to_string());
        Err(ParseError { at: self.at(), message: format!("expected `{p}`, found {found}") })
    }

    fn expr(&mut self) -> Result<Expr, ParseError> {
        let c = self.binary(0)?;
        if self.eat("?") {
            let a = self.expr()?;
            self.expect(":")?;
            let b = self.expr()?;
            return Ok(Expr::Cond(Box::new(c), Box::new(a), Box::new(b)));
        }
        Ok(c)
    }

    fn binary(&mut self, level: usize) -> Result<Expr, ParseError> {
        let Some(ops) = LEVELS.get(level) else { return self.unary() };
        let mut lhs = self.binary(level + 1)?;
        'more: loop {
            for (p, op) in *ops {
                if self.eat(p) {
                    let rhs = self.binary(level + 1)?;
                    lhs = Expr::Binary(*op, Box::new(lhs), Box::new(rhs));
                    continue 'more;
                }
            }
            return Ok(lhs);
        }
    }

    fn unary(&mut self) -> Result<Expr, ParseError> {
        if self.eat("-") {
            return Ok(Expr::Unary(UnOp::Neg, Box::new(self.unary()?)));
        }
        if self.eat("!") {
            return Ok(Expr::Unary(UnOp::Not, Box::new(self.unary()?)));
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<Expr, ParseError> {
        let at = self.at();
        let Some((_, tok)) = self.peek() else {
            return Err(ParseError { at, message: "expected a value, found the end".into() });
        };
        let tok = tok.clone();
        self.i += 1;
        Ok(match tok {
            Tok::Num(n) => Expr::Lit(Value::Num(n)),
            Tok::Str(s) => Expr::Lit(Value::text(&s)),
            Tok::Color(c) => Expr::Lit(Value::Color(c)),
            Tok::Token(t) => Expr::Token(t),
            Tok::Punct("(") => {
                let e = self.expr()?;
                self.expect(")")?;
                e
            }
            Tok::Name(name) => match name.as_str() {
                "w" => Expr::W,
                "h" => Expr::H,
                "pw" => Expr::PW,
                "ph" => Expr::PH,
                "true" => Expr::Lit(Value::Bool(true)),
                "false" => Expr::Lit(Value::Bool(false)),
                "missing" => Expr::Lit(Value::Missing),
                _ if self.eat("(") => {
                    let func = Func::named(&name).ok_or_else(|| ParseError {
                        at,
                        message: format!("unknown function `{name}` (functions: {})", Func::names().join(", ")),
                    })?;
                    let mut args = Vec::new();
                    if !self.eat(")") {
                        loop {
                            args.push(self.expr()?);
                            if self.eat(")") {
                                break;
                            }
                            self.expect(",")?;
                        }
                    }
                    let (lo, hi) = func.arity();
                    if args.len() < lo || args.len() > hi {
                        let want = if lo == hi { lo.to_string() } else if hi == usize::MAX { format!("{lo}+") } else { format!("{lo}–{hi}") };
                        return Err(ParseError { at, message: format!("`{name}` takes {want} arguments, given {}", args.len()) });
                    }
                    Expr::Call(func, args)
                }
                _ if name.ends_with('.') || name.starts_with('.') || name.contains("..") => {
                    return Err(ParseError { at, message: format!("bad field name `{name}`") });
                }
                _ => Expr::Field(Field::Name(name)),
            },
            other => return Err(ParseError { at, message: format!("expected a value, found {other}") }),
        })
    }
}
