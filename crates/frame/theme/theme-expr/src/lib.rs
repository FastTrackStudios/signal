//! The binding language of frame themes.
//!
//! A theme draws a host's view — a mixer strip, a track row — from a
//! document whose properties can be *bound*: `bind:fill="$strip.ground"`,
//! `bind:visible="w >= 60"`, `bind:text="track.name"`. This crate is the
//! language those bindings are written in: a small, total, typed
//! expression language.
//!
//! ```text
//! 12  0.5  "Solo"  #ff8800  #ff880080  true  false     literals
//! w  h                                                 the instance's box (the whole strip)
//! pw  ph                                               the bound node's parent's box
//! $strip.ground                                        a token (a document variable)
//! track.name  track.volume  meter.peak                 the host's data
//! + - * / %   == != < <= > >=   && || !   c ? a : b    operators
//! min max clamp abs floor ceil round lerp step         numbers
//! smoothstep pow sqrt sin cos
//! db(amp) amp(db)                                      gain
//! rgb(r, g, b[, a]) mix(a, b, t) alpha(c, a)           colours (channels 0..1)
//! fade(c, k) lighten(c, k) darken(c, k) lift(c, k) luma(c)
//! ink(bg[, dark, light])                               legible text on a ground
//! upper(s) lower(s) fmt(x[, decimals]) len(s)          text (`+` joins text)
//! if(c, a, b) or(a, b, …)                              choice
//! arc(cx, cy, r, start, sweep)                         path data: an arc, degrees clockwise from twelve
//! ```
//!
//! Evaluation never fails. A value the host doesn't have (a field it does
//! not know, a meter that has not reported) is [`Value::Missing`]; missing
//! propagates through operators, `or` picks the first present argument,
//! and a binding that comes out missing leaves the authored property in
//! place. A wrong type (a colour where a number belongs) is missing too.
//!
//! # Cost
//!
//! Bindings are evaluated for every strip a host records and, for live
//! ones, every frame. So a compiled theme does the lookups once:
//! [`Expr::inline_tokens`] replaces tokens with their values (a mode
//! switch recompiles), [`Expr::resolve_fields`] replaces field names with
//! the host's slot numbers, and [`Expr::fold`] evaluates whatever is left
//! constant. What remains is arithmetic over slots.

mod eval;
mod func;
mod parse;
mod value;

pub use eval::Scope;
pub use func::Func;
pub use parse::{ParseError, parse};
pub use value::{Rgba, Value};

use std::collections::BTreeSet;

/// A parsed expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Lit(Value),
    /// The width of the instance's box (the strip, the row).
    W,
    /// Its height.
    H,
    /// The width of the bound node's parent, as laid out: a fader cap's
    /// travel, a meter's well.
    PW,
    /// Its height.
    PH,
    /// A document variable, by name (without the `$`).
    Token(String),
    /// A host field.
    Field(Field),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    /// `c ? a : b`.
    Cond(Box<Expr>, Box<Expr>, Box<Expr>),
    Call(Func, Vec<Expr>),
}

/// A host field: by name as written, or by the slot the host resolved it
/// to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    Name(String),
    Slot(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

/// What an expression reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Refs {
    /// `w`, `h`, `pw` or `ph`: it depends on the layout.
    pub size: bool,
    pub tokens: BTreeSet<String>,
    /// Field names (unresolved fields only).
    pub fields: BTreeSet<String>,
    /// Resolved field slots.
    pub slots: BTreeSet<u32>,
}

impl Expr {
    /// Visit this expression and every sub-expression, parents first.
    pub fn visit<'a>(&'a self, f: &mut impl FnMut(&'a Expr)) {
        f(self);
        match self {
            Expr::Lit(_) | Expr::W | Expr::H | Expr::PW | Expr::PH | Expr::Token(_) | Expr::Field(_) => {}
            Expr::Unary(_, a) => a.visit(f),
            Expr::Binary(_, a, b) => {
                a.visit(f);
                b.visit(f);
            }
            Expr::Cond(c, a, b) => {
                c.visit(f);
                a.visit(f);
                b.visit(f);
            }
            Expr::Call(_, args) => args.iter().for_each(|a| a.visit(f)),
        }
    }

    fn visit_mut(&mut self, f: &mut impl FnMut(&mut Expr)) {
        f(self);
        match self {
            Expr::Lit(_) | Expr::W | Expr::H | Expr::PW | Expr::PH | Expr::Token(_) | Expr::Field(_) => {}
            Expr::Unary(_, a) => a.visit_mut(f),
            Expr::Binary(_, a, b) => {
                a.visit_mut(f);
                b.visit_mut(f);
            }
            Expr::Cond(c, a, b) => {
                c.visit_mut(f);
                a.visit_mut(f);
                b.visit_mut(f);
            }
            Expr::Call(_, args) => args.iter_mut().for_each(|a| a.visit_mut(f)),
        }
    }

    /// Everything this expression reads.
    #[must_use]
    pub fn refs(&self) -> Refs {
        let mut r = Refs::default();
        self.visit(&mut |e| match e {
            Expr::W | Expr::H | Expr::PW | Expr::PH => r.size = true,
            Expr::Token(t) => {
                r.tokens.insert(t.clone());
            }
            Expr::Field(Field::Name(n)) => {
                r.fields.insert(n.clone());
            }
            Expr::Field(Field::Slot(s)) => {
                r.slots.insert(*s);
            }
            _ => {}
        });
        r
    }

    /// Replace every token with its value. `value` answers `None` for a
    /// token the document does not define; the first such name is the
    /// error.
    ///
    /// # Errors
    /// The name of an undefined token.
    pub fn inline_tokens(&mut self, value: &mut impl FnMut(&str) -> Option<Value>) -> Result<(), String> {
        let mut missing = None;
        self.visit_mut(&mut |e| {
            if let Expr::Token(name) = e {
                match value(name) {
                    Some(v) => *e = Expr::Lit(v),
                    None => {
                        missing.get_or_insert_with(|| name.clone());
                    }
                }
            }
        });
        missing.map_or(Ok(()), Err)
    }

    /// Replace every field name with the host's slot for it. `slot`
    /// answers `None` for a field the host does not have; the first such
    /// name is the error.
    ///
    /// # Errors
    /// The name of a field the host does not have.
    pub fn resolve_fields(&mut self, slot: &mut impl FnMut(&str) -> Option<u32>) -> Result<(), String> {
        let mut missing = None;
        self.visit_mut(&mut |e| {
            if let Expr::Field(f @ Field::Name(_)) = e {
                let Field::Name(name) = &*f else { return };
                match slot(name) {
                    Some(s) => *f = Field::Slot(s),
                    None => {
                        missing.get_or_insert_with(|| name.clone());
                    }
                }
            }
        });
        missing.map_or(Ok(()), Err)
    }

    /// Replace each field named `name` that `with` answers for by the
    /// expression it gives — a component's `arg.on` by the `track.mute`
    /// its instance passes. Fields it answers `None` for are left as they
    /// are.
    ///
    /// What it substitutes is not looked into again: an instance's
    /// argument is already in its own scope.
    pub fn substitute_fields(&mut self, with: &mut impl FnMut(&str) -> Option<Expr>) {
        match self {
            Expr::Field(Field::Name(name)) => {
                if let Some(by) = with(name) {
                    *self = by;
                }
            }
            Expr::Lit(_) | Expr::W | Expr::H | Expr::PW | Expr::PH | Expr::Token(_) | Expr::Field(_) => {}
            Expr::Unary(_, a) => a.substitute_fields(with),
            Expr::Binary(_, a, b) => {
                a.substitute_fields(with);
                b.substitute_fields(with);
            }
            Expr::Cond(c, a, b) => {
                c.substitute_fields(with);
                a.substitute_fields(with);
                b.substitute_fields(with);
            }
            Expr::Call(_, args) => args.iter_mut().for_each(|a| a.substitute_fields(with)),
        }
    }

    /// Evaluate every sub-expression that reads nothing (no box, no token,
    /// no field) down to its value.
    #[must_use]
    pub fn fold(self) -> Expr {
        let refs = self.refs();
        if !refs.size && refs.tokens.is_empty() && refs.fields.is_empty() && refs.slots.is_empty() {
            return Expr::Lit(self.eval(&eval::Nothing));
        }
        match self {
            Expr::Unary(op, a) => Expr::Unary(op, Box::new(a.fold())),
            Expr::Binary(op, a, b) => Expr::Binary(op, Box::new(a.fold()), Box::new(b.fold())),
            Expr::Cond(c, a, b) => {
                let c = c.fold();
                // A constant condition picks its branch outright.
                if let Some(t) = c.constant().and_then(Value::truthy) {
                    return if t { a.fold() } else { b.fold() };
                }
                Expr::Cond(Box::new(c), Box::new(a.fold()), Box::new(b.fold()))
            }
            Expr::Call(f, args) => Expr::Call(f, args.into_iter().map(Expr::fold).collect()),
            e => e,
        }
    }

    /// The value, when this expression is a literal.
    #[must_use]
    pub fn constant(&self) -> Option<&Value> {
        match self {
            Expr::Lit(v) => Some(v),
            _ => None,
        }
    }
}
