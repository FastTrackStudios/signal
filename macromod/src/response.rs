//! Response curves — generalized shaping for macro bindings and modulation.
//!
//! `ResponseCurve` extends `EasingCurve` with a `Power` variant inspired by
//! `ReaMotionPad`'s binding curve, providing single-slider response control.

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::easing::EasingCurve;

/// A response curve for mapping input → output values.
///
/// Two families:
/// - `Easing(EasingCurve)` — the existing named curves (Linear, `EaseIn`, etc.)
/// - `Power { exponent }` — a power curve `t^exponent` for continuous control
#[derive(Debug, Clone, Copy, PartialEq, Facet)]
#[repr(C)]
pub enum ResponseCurve {
    /// One of the named easing curves.
    Easing(EasingCurve),
    /// Power curve: `t^exponent`. Exponent 1.0 = linear, 2.0 = quadratic,
    /// 0.5 = square root (fast start).
    Power { exponent: f32 },
}

impl Default for ResponseCurve {
    fn default() -> Self {
        Self::Easing(EasingCurve::Linear)
    }
}

impl ResponseCurve {
    /// Human-readable display name.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Easing(curve) => curve.display_name(),
            Self::Power { .. } => "Power",
        }
    }

    /// Apply the response curve to a normalized `t` in `[0.0, 1.0]`.
    #[must_use]
    pub fn apply(self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Easing(curve) => curve.apply(t),
            Self::Power { exponent } => t.powf(f64::from(exponent)),
        }
    }
}

impl From<EasingCurve> for ResponseCurve {
    fn from(curve: EasingCurve) -> Self {
        Self::Easing(curve)
    }
}

// ─── Serde: backward-compatible deserialization ─────────────────

impl Serialize for ResponseCurve {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        #[serde(tag = "type")]
        enum Wire {
            Easing { curve: EasingCurve },
            Power { exponent: f32 },
        }

        match self {
            Self::Easing(curve) => Wire::Easing { curve: *curve }.serialize(serializer),
            Self::Power { exponent } => Wire::Power {
                exponent: *exponent,
            }
            .serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ResponseCurve {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de;

        // Accept either the tagged object or a bare EasingCurve string for backward compat.
        let value = serde_json::Value::deserialize(deserializer)?;

        // Try bare string → EasingCurve (backward compat from old MacroBinding.curve field)
        if value.is_string() {
            let curve: EasingCurve = serde_json::from_value(value).map_err(de::Error::custom)?;
            return Ok(Self::Easing(curve));
        }

        // Try tagged object
        if let Some(ty) = value.get("type").and_then(|v| v.as_str()) {
            match ty {
                "Easing" => {
                    let curve: EasingCurve = value
                        .get("curve")
                        .ok_or_else(|| de::Error::missing_field("curve"))
                        .and_then(|v| {
                            serde_json::from_value::<EasingCurve>(v.clone())
                                .map_err(de::Error::custom)
                        })?;
                    Ok(Self::Easing(curve))
                }
                "Power" => {
                    let exponent: f32 = value
                        .get("exponent")
                        .ok_or_else(|| de::Error::missing_field("exponent"))
                        .and_then(|v| {
                            serde_json::from_value::<f32>(v.clone()).map_err(de::Error::custom)
                        })?;
                    Ok(Self::Power { exponent })
                }
                other => Err(de::Error::unknown_variant(other, &["Easing", "Power"])),
            }
        } else {
            Err(de::Error::custom(
                "expected a string (EasingCurve) or object with 'type' field",
            ))
        }
    }
}
