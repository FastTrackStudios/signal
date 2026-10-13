//! Lossless `serde_json::Value` ⇄ [`LoroValue`] conversion.
//!
//! Every typed prop is stored in the Loro doc as the `LoroValue` image of
//! its serde JSON representation. The mapping is structural:
//!
//! | JSON                      | Loro                 |
//! | ------------------------- | -------------------- |
//! | `null` / bool / string    | `Null` / `Bool` / `String` |
//! | integer (fits `i64`)      | `I64`                |
//! | float                     | `Double` (bit-exact) |
//! | array / object            | `List` / `Map`       |
//!
//! Floats round-trip bit-for-bit: serde_json keeps an `f64` as an `f64`,
//! and `Double` stores it unchanged. Integers larger than `i64::MAX` (no
//! field of the model has them) fall back to `Double`. The reverse
//! direction maps `Binary` to an array of byte numbers and a `Container`
//! reference to its id string — neither is produced by [`to_loro`].

use crdt::loro::LoroValue;
use serde_json::{Map, Number, Value};

/// JSON → Loro.
#[must_use]
pub fn to_loro(value: &Value) -> LoroValue {
    match value {
        Value::Null => LoroValue::Null,
        Value::Bool(b) => LoroValue::Bool(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                LoroValue::I64(i)
            } else {
                // A float (or a u64 beyond i64): `as_f64` is exact for floats.
                LoroValue::Double(n.as_f64().unwrap_or(f64::NAN))
            }
        }
        Value::String(s) => LoroValue::String(s.as_str().into()),
        Value::Array(items) => LoroValue::List(items.iter().map(to_loro).collect()),
        Value::Object(map) => {
            LoroValue::Map(map.iter().map(|(k, v)| (k.clone(), to_loro(v))).collect())
        }
    }
}

/// Loro → JSON.
#[must_use]
pub fn from_loro(value: &LoroValue) -> Value {
    match value {
        LoroValue::Null => Value::Null,
        LoroValue::Bool(b) => Value::Bool(*b),
        // Non-finite doubles have no JSON form (serde_json writes them as `null` too).
        LoroValue::Double(d) => Number::from_f64(*d).map_or(Value::Null, Value::Number),
        LoroValue::I64(i) => Value::Number((*i).into()),
        LoroValue::String(s) => Value::String(s.to_string()),
        LoroValue::Binary(b) => {
            Value::Array(b.iter().map(|x| Value::Number((*x).into())).collect())
        }
        LoroValue::List(items) => Value::Array(items.iter().map(from_loro).collect()),
        LoroValue::Map(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), from_loro(v)))
                .collect::<Map<_, _>>(),
        ),
        LoroValue::Container(id) => Value::String(id.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_and_ints_round_trip_exactly() {
        for f in [
            0.1_f64,
            1.0 / 3.0,
            10.0,
            -0.0,
            f64::MAX,
            f64::MIN_POSITIVE,
            5e-324,
            1e300,
        ] {
            let json = serde_json::to_value(f).unwrap();
            let back = from_loro(&to_loro(&json));
            let g: f64 = serde_json::from_value(back).unwrap();
            assert_eq!(f.to_bits(), g.to_bits(), "{f}");
        }
        for i in [0_i64, -1, i64::MAX, i64::MIN] {
            let json = serde_json::to_value(i).unwrap();
            assert_eq!(to_loro(&json), LoroValue::I64(i));
            assert_eq!(from_loro(&to_loro(&json)), json);
        }
    }

    #[test]
    fn nested_values_round_trip() {
        let json = serde_json::json!({
            "a": [1, 2.5, null, true, "s"],
            "b": { "c": { "d": [] }, "e": {} },
        });
        assert_eq!(from_loro(&to_loro(&json)), json);
    }
}
