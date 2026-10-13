//! Evaluation.

use std::sync::Arc;

use crate::{BinOp, Expr, Field, Func, UnOp, Value};

/// What an expression is evaluated against: the box, and the host's data
/// by slot.
pub trait Scope {
    /// The width and height of the box the node is laid out in.
    fn size(&self) -> (f64, f64);
    /// The width and height of the bound node's parent, as laid out.
    /// Defaults to [`Scope::size`].
    fn parent_size(&self) -> (f64, f64) {
        self.size()
    }
    /// A field's value; [`Value::Missing`] for one the host has no data for.
    fn field(&self, slot: u32) -> Value;
}

/// The empty scope: what constants are folded against.
pub(crate) struct Nothing;

impl Scope for Nothing {
    fn size(&self) -> (f64, f64) {
        (0.0, 0.0)
    }

    fn field(&self, _: u32) -> Value {
        Value::Missing
    }
}

impl Expr {
    /// The value in `scope`. Never fails: see the crate docs on missing.
    #[must_use]
    pub fn eval(&self, scope: &dyn Scope) -> Value {
        match self {
            Expr::Lit(v) => v.clone(),
            Expr::W => Value::Num(scope.size().0),
            Expr::H => Value::Num(scope.size().1),
            Expr::PW => Value::Num(scope.parent_size().0),
            Expr::PH => Value::Num(scope.parent_size().1),
            // Unresolved: the compiler inlines tokens and resolves fields
            // before anything is evaluated for real.
            Expr::Token(_) | Expr::Field(Field::Name(_)) => Value::Missing,
            Expr::Field(Field::Slot(s)) => scope.field(*s),
            Expr::Unary(op, a) => {
                let a = a.eval(scope);
                match op {
                    UnOp::Neg => a.num().map_or(Value::Missing, |x| Value::Num(-x)),
                    UnOp::Not => a.truthy().map_or(Value::Missing, |b| Value::Bool(!b)),
                }
            }
            Expr::Binary(op, a, b) => binary(*op, a, b, scope),
            Expr::Cond(c, a, b) => match c.eval(scope).truthy() {
                Some(true) => a.eval(scope),
                Some(false) => b.eval(scope),
                None => Value::Missing,
            },
            // Lazy: only the branch taken, only up to the first present.
            Expr::Call(Func::If, args) => match args.first().map(|c| c.eval(scope).truthy()) {
                Some(Some(true)) => args.get(1).map_or(Value::Missing, |a| a.eval(scope)),
                Some(Some(false)) => args.get(2).map_or(Value::Missing, |a| a.eval(scope)),
                _ => Value::Missing,
            },
            Expr::Call(Func::Or, args) => {
                args.iter().map(|a| a.eval(scope)).find(|v| !v.is_missing()).unwrap_or(Value::Missing)
            }
            Expr::Call(f, args) => {
                let vals: Vec<Value> = args.iter().map(|a| a.eval(scope)).collect();
                f.apply(&vals)
            }
        }
    }
}

fn binary(op: BinOp, a: &Expr, b: &Expr, scope: &dyn Scope) -> Value {
    // Short-circuit: `track.armed && meter.peak > 0` reads no meter for an
    // unarmed track.
    match op {
        BinOp::And => {
            return match a.eval(scope).truthy() {
                Some(false) => Value::Bool(false),
                Some(true) => b.eval(scope).truthy().map_or(Value::Missing, Value::Bool),
                None => Value::Missing,
            };
        }
        BinOp::Or => {
            return match a.eval(scope).truthy() {
                Some(true) => Value::Bool(true),
                Some(false) => b.eval(scope).truthy().map_or(Value::Missing, Value::Bool),
                None => Value::Missing,
            };
        }
        _ => {}
    }
    let (a, b) = (a.eval(scope), b.eval(scope));
    if a.is_missing() || b.is_missing() {
        return Value::Missing;
    }
    match op {
        BinOp::Eq => return Value::Bool(equal(&a, &b)),
        BinOp::Ne => return Value::Bool(!equal(&a, &b)),
        // Text joins with anything that writes out.
        BinOp::Add if matches!(a, Value::Text(_)) || matches!(b, Value::Text(_)) => {
            return match (a.as_text(), b.as_text()) {
                (Some(x), Some(y)) => Value::Text(Arc::from(format!("{x}{y}"))),
                _ => Value::Missing,
            };
        }
        _ => {}
    }
    let (Some(x), Some(y)) = (a.num(), b.num()) else { return Value::Missing };
    match op {
        BinOp::Add => Value::Num(x + y),
        BinOp::Sub => Value::Num(x - y),
        BinOp::Mul => Value::Num(x * y),
        // Division by zero has nothing to show.
        BinOp::Div if y == 0.0 => Value::Missing,
        BinOp::Div => Value::Num(x / y),
        BinOp::Rem if y == 0.0 => Value::Missing,
        BinOp::Rem => Value::Num(x.rem_euclid(y)),
        BinOp::Lt => Value::Bool(x < y),
        BinOp::Le => Value::Bool(x <= y),
        BinOp::Gt => Value::Bool(x > y),
        BinOp::Ge => Value::Bool(x >= y),
        BinOp::Eq | BinOp::Ne | BinOp::And | BinOp::Or => Value::Missing,
    }
}

fn equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Text(x), Value::Text(y)) => x == y,
        (Value::Color(x), Value::Color(y)) => x.to_rgba8() == y.to_rgba8(),
        _ => match (a.num(), b.num()) {
            (Some(x), Some(y)) => (x - y).abs() < 1e-9,
            _ => false,
        },
    }
}
