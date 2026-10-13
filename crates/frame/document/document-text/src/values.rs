//! Value codecs shared by the printer and the parser: numbers, colors,
//! keyword enums, and the compact JSON forms of paints / effects / runs.

use document_proto::{
    AlignSelf, BlendMode, BooleanOp, Color, CrossAxisAlign, Effect, MainAxisAlign, MaskKind, Paint,
    PaintKind, ResizeX, ResizeY, StrokeAlign, StrokeCap, StrokeJoin, TextAlign, TextCase, TextRun,
    Transform,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};

// ── numbers ──────────────────────────────────────────────────────────────

/// Shortest decimal that parses back to exactly `v` (Rust's `Display`).
pub(crate) fn num(v: f64) -> String {
    format!("{v}")
}

pub(crate) fn num_list(vs: &[f64]) -> String {
    vs.iter().map(|v| num(*v)).collect::<Vec<_>>().join(" ")
}

/// A JSON string literal (the attribute string syntax).
pub(crate) fn quote(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| String::from("\"\""))
}

// ── colors ───────────────────────────────────────────────────────────────

fn channel_byte(v: f64) -> Option<u8> {
    if !(0.0..=1.0).contains(&v) {
        return None;
    }
    let n = (v * 255.0).round();
    // In range by the check above.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let b = n as u8;
    (f64::from(b) / 255.0 == v).then_some(b)
}

/// `#rrggbb` / `#rrggbbaa`, only when every channel is exactly `n/255`.
pub(crate) fn hex(c: Color) -> Option<String> {
    let (r, g, b, a) = (
        channel_byte(c.r)?,
        channel_byte(c.g)?,
        channel_byte(c.b)?,
        channel_byte(c.a)?,
    );
    Some(if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    })
}

/// `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa` → straight RGBA (channel = n/255).
pub(crate) fn parse_hex(s: &str) -> Option<Color> {
    let h = s.strip_prefix('#')?;
    if !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    let nib = |i: usize| u8::from_str_radix(&h[i..=i], 16).ok().map(|n| n * 17);
    let (r, g, b, a) = match h.len() {
        3 => (nib(0)?, nib(1)?, nib(2)?, 255),
        4 => (nib(0)?, nib(1)?, nib(2)?, nib(3)?),
        6 => (byte(0)?, byte(2)?, byte(4)?, 255),
        8 => (byte(0)?, byte(2)?, byte(4)?, byte(6)?),
        _ => return None,
    };
    let f = |n: u8| f64::from(n) / 255.0;
    Some(Color::rgba(f(r), f(g), f(b), f(a)))
}

// ── keyword enums ────────────────────────────────────────────────────────

/// A unit enum written as a kebab-case keyword (`BlendMode::PassThrough` →
/// `pass-through`). Parsing ignores case, `-`, `_` and spaces.
pub(crate) trait Keyword: Sized + Copy + Serialize + 'static {
    const ALL: &'static [Self];

    fn keyword(self) -> String {
        let pascal = serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default();
        kebab(&pascal)
    }

    fn from_keyword(s: &str) -> Option<Self> {
        let want = squash(s);
        Self::ALL
            .iter()
            .copied()
            .find(|v| squash(&v.keyword()) == want)
    }

    fn expected() -> String {
        Self::ALL
            .iter()
            .map(|v| format!("\"{}\"", v.keyword()))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn kebab(pascal: &str) -> String {
    let mut out = String::new();
    for (i, ch) in pascal.chars().enumerate() {
        if ch.is_ascii_uppercase() && i > 0 {
            out.push('-');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}

fn squash(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

macro_rules! keywords {
    ($($ty:ident: [$($v:ident),* $(,)?];)*) => {$(
        impl Keyword for $ty {
            const ALL: &'static [Self] = &[$($ty::$v),*];
        }
    )*};
}

keywords! {
    BlendMode: [Normal, PassThrough, Multiply, Screen, Overlay, Darken, Lighten, ColorDodge, ColorBurn, HardLight,
        SoftLight, Difference, Exclusion, Hue, Saturation, Color, Luminosity, LinearBurn, LinearDodge];
    StrokeCap: [None, Round, Square, LineArrow, TriangleArrow, DiamondFilled, CircleFilled, TriangleFilled,
        WashiTape1, WashiTape2, WashiTape3, WashiTape4, WashiTape5, WashiTape6];
    StrokeJoin: [Miter, Bevel, Round];
    StrokeAlign: [Inside, Outside, Center];
    MaskKind: [Alpha, Vector, Luminance];
    TextCase: [Original, Upper, Lower, Title, SmallCaps, SmallCapsForced];
    TextAlign: [Left, Center, Right, Justified];
    MainAxisAlign: [Start, Center, End, SpaceBetween];
    CrossAxisAlign: [Start, Center, End, Baseline];
    AlignSelf: [Start, Center, End, Stretch];
    ResizeX: [Left, Right, Center, Stretch, Scale];
    ResizeY: [Top, Bottom, Center, Stretch, Scale];
    BooleanOp: [Union, Subtract, Intersect, Exclude];
}

// ── transforms ───────────────────────────────────────────────────────────

/// `(a, b, c, d)` of a rotation by `deg` degrees (clockwise on screen, y
/// down): `a = cos, b = -sin, c = sin, d = cos`. Multiples of 90° are exact.
pub(crate) fn rotation_matrix(deg: f64) -> (f64, f64, f64, f64) {
    let (s, c) = if deg % 90.0 == 0.0 {
        // Finite multiple of 90 ⇒ quarter turns are small integers mod 4.
        #[allow(clippy::cast_possible_truncation)]
        match (deg / 90.0).rem_euclid(4.0) as i64 {
            0 => (0.0, 1.0),
            1 => (1.0, 0.0),
            2 => (0.0, -1.0),
            _ => (-1.0, 0.0),
        }
    } else {
        deg.to_radians().sin_cos()
    };
    (c, -s, s, c)
}

/// The shortest `rotation=` value that rebuilds `t`'s linear part exactly.
pub(crate) fn rotation_of(t: &Transform) -> Option<f64> {
    let base = t.c.atan2(t.a).to_degrees();
    if !base.is_finite() {
        return None;
    }
    let exact = |deg: f64| rotation_matrix(deg) == (t.a, t.b, t.c, t.d);
    (0..=12)
        .map(|p| {
            let k = 10f64.powi(p);
            (base * k).round() / k
        })
        .chain(std::iter::once(base))
        .find(|d| exact(*d))
}

// ── JSON forms ───────────────────────────────────────────────────────────

/// Serialize for a `{…}` attribute: nulls (unset `Option`s) dropped, and any
/// `"color": {r,g,b,a}` written as a hex string when lossless.
pub(crate) fn to_json<T: Serialize>(v: &T) -> Value {
    let mut v = serde_json::to_value(v).unwrap_or(Value::Null);
    compact(&mut v);
    v
}

fn compact(v: &mut Value) {
    match v {
        Value::Object(m) => {
            m.retain(|_, x| !x.is_null());
            for (k, x) in m.iter_mut() {
                if k == "color"
                    && let Some(h) = color_hex_of(x)
                {
                    *x = Value::String(h);
                    continue;
                }
                compact(x);
            }
        }
        Value::Array(a) => a.iter_mut().for_each(compact),
        _ => {}
    }
}

fn color_hex_of(v: &Value) -> Option<String> {
    let m = v.as_object()?;
    if m.len() != 4 {
        return None;
    }
    let c = serde_json::from_value::<Color>(v.clone()).ok()?;
    hex(c)
}

/// The inverse of [`compact`]'s color rule: `"color": "#hex"` → object.
fn expand_colors(v: &mut Value) -> Result<(), String> {
    match v {
        Value::Object(m) => {
            for (k, x) in m.iter_mut() {
                if k == "color"
                    && let Value::String(s) = x
                {
                    let c = parse_hex(s).ok_or_else(|| {
                        format!("invalid color `{s}` (expected #rrggbb or #rrggbbaa)")
                    })?;
                    *x = serde_json::to_value(c).map_err(|e| e.to_string())?;
                    continue;
                }
                expand_colors(x)?;
            }
        }
        Value::Array(a) => {
            for x in a {
                expand_colors(x)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Deserialize a `{…}` attribute value (after [`expand_colors`]).
pub(crate) fn from_json<T: DeserializeOwned>(mut v: Value) -> Result<T, String> {
    expand_colors(&mut v)?;
    serde_json::from_value(v).map_err(|e| e.to_string())
}

fn strip(m: &mut Map<String, Value>, key: &str, default: &Value) {
    if m.get(key) == Some(default) {
        m.remove(key);
    }
}

fn fill_default(m: &mut Map<String, Value>, key: &str, default: Value) {
    m.entry(key.to_owned()).or_insert(default);
}

/// A plain solid paint (opacity 1, normal, visible) with a hex-exact color.
pub(crate) fn plain_solid_hex(p: &Paint) -> Option<String> {
    match &p.kind {
        PaintKind::Solid { color }
            if p.opacity == 1.0 && p.blend == BlendMode::Normal && p.visible =>
        {
            hex(*color)
        }
        _ => None,
    }
}

fn paint_defaults() -> [(&'static str, Value); 3] {
    [
        ("opacity", json!(1.0)),
        ("blend", json!("Normal")),
        ("visible", json!(true)),
    ]
}

pub(crate) fn paint_json(p: &Paint) -> Value {
    if let Some(h) = plain_solid_hex(p) {
        return Value::String(h);
    }
    let mut v = to_json(p);
    if let Value::Object(m) = &mut v {
        for (k, d) in paint_defaults() {
            strip(m, k, &d);
        }
        if let Some(Value::Object(img)) = m.get_mut("kind").and_then(|k| k.get_mut("Image")) {
            strip(img, "scale_mode", &json!("Fill"));
        }
    }
    v
}

pub(crate) fn paints_json(ps: &[Paint]) -> Value {
    Value::Array(ps.iter().map(paint_json).collect())
}

pub(crate) fn paint_from_json(v: Value) -> Result<Paint, String> {
    match v {
        Value::String(s) => parse_hex(&s)
            .map(Paint::solid)
            .ok_or_else(|| format!("invalid color `{s}` (expected #rrggbb or #rrggbbaa)")),
        Value::Object(mut m) => {
            for (k, d) in paint_defaults() {
                fill_default(&mut m, k, d);
            }
            if let Some(Value::Object(img)) = m.get_mut("kind").and_then(|k| k.get_mut("Image")) {
                fill_default(img, "scale_mode", json!("Fill"));
            }
            from_json(Value::Object(m))
        }
        other => Err(format!(
            "a paint is a color string or an object, got `{other}`"
        )),
    }
}

/// A paint list: an array of paints, or a single paint.
pub(crate) fn paints_from_json(v: Value) -> Result<Vec<Paint>, String> {
    match v {
        Value::Array(a) => a.into_iter().map(paint_from_json).collect(),
        other => Ok(vec![paint_from_json(other)?]),
    }
}

fn effect_defaults() -> [(&'static str, Value); 4] {
    [
        ("visible", json!(true)),
        ("offset_x", json!(0.0)),
        ("offset_y", json!(0.0)),
        ("spread", json!(0.0)),
    ]
}

pub(crate) fn effects_json(es: &[Effect]) -> Value {
    Value::Array(
        es.iter()
            .map(|e| {
                let mut v = to_json(e);
                if let Value::Object(m) = &mut v {
                    for (k, d) in effect_defaults() {
                        strip(m, k, &d);
                    }
                }
                v
            })
            .collect(),
    )
}

pub(crate) fn effects_from_json(v: Value) -> Result<Vec<Effect>, String> {
    let items = match v {
        Value::Array(a) => a,
        other => vec![other],
    };
    items
        .into_iter()
        .map(|e| match e {
            Value::Object(mut m) => {
                for (k, d) in effect_defaults() {
                    fill_default(&mut m, k, d);
                }
                from_json(Value::Object(m))
            }
            other => Err(format!(
                "an effect is an object like {{\"kind\":\"DropShadow\",…}}, got `{other}`"
            )),
        })
        .collect()
}

pub(crate) fn runs_json(rs: &[TextRun]) -> Value {
    Value::Array(
        rs.iter()
            .map(|r| {
                let mut v = to_json(r);
                if let Value::Object(m) = &mut v {
                    if r.fills.is_empty() {
                        m.remove("fills");
                    } else {
                        m.insert("fills".into(), paints_json(&r.fills));
                    }
                }
                v
            })
            .collect(),
    )
}

pub(crate) fn runs_from_json(v: Value) -> Result<Vec<TextRun>, String> {
    let Value::Array(items) = v else {
        return Err("`runs` is an array of run objects: [{\"text\":\"…\"}, …]".into());
    };
    items
        .into_iter()
        .map(|r| {
            let Value::Object(mut m) = r else {
                return Err(format!(
                    "a run is an object like {{\"text\":\"…\"}}, got `{r}`"
                ));
            };
            let fills = m
                .remove("fills")
                .map(paints_from_json)
                .transpose()?
                .unwrap_or_default();
            m.insert("fills".into(), Value::Array(Vec::new()));
            let mut run: TextRun = from_json(Value::Object(m))?;
            run.fills = fills;
            Ok(run)
        })
        .collect()
}

pub(crate) fn transform_from_json(v: Value) -> Result<Transform, String> {
    let Value::Object(mut m) = v else {
        return Err(
            "`transform` is an object {\"a\":…,\"b\":…,\"tx\":…,\"c\":…,\"d\":…,\"ty\":…}".into(),
        );
    };
    for (k, d) in [
        ("a", 1.0),
        ("b", 0.0),
        ("tx", 0.0),
        ("c", 0.0),
        ("d", 1.0),
        ("ty", 0.0),
    ] {
        fill_default(&mut m, k, json!(d));
    }
    from_json(Value::Object(m))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_is_exact_only() {
        assert_eq!(
            hex(Color::rgba(1.0, 0.0, 0.0, 1.0)).as_deref(),
            Some("#ff0000")
        );
        assert_eq!(
            hex(Color::rgba(1.0, 0.0, 0.0, 128.0 / 255.0)).as_deref(),
            Some("#ff000080")
        );
        assert_eq!(hex(Color::rgba(0.5, 0.0, 0.0, 1.0)), None);
        for n in 0..=255u8 {
            let c = Color::rgba(f64::from(n) / 255.0, 0.0, 0.0, 1.0);
            assert_eq!(parse_hex(&hex(c).unwrap()), Some(c));
        }
    }

    #[test]
    fn keywords() {
        assert_eq!(BlendMode::PassThrough.keyword(), "pass-through");
        assert_eq!(
            BlendMode::from_keyword("PassThrough"),
            Some(BlendMode::PassThrough)
        );
        assert_eq!(
            StrokeCap::from_keyword("washi-tape3"),
            Some(StrokeCap::WashiTape3)
        );
        assert_eq!(
            MainAxisAlign::from_keyword("space_between"),
            Some(MainAxisAlign::SpaceBetween)
        );
    }

    #[test]
    fn rotations() {
        for deg in [0.0, 90.0, 180.0, -90.0, 45.0, 30.0, 12.5] {
            let (a, b, c, d) = rotation_matrix(deg);
            let t = Transform {
                a,
                b,
                tx: 0.0,
                c,
                d,
                ty: 0.0,
            };
            let back = rotation_of(&t).unwrap();
            assert_eq!(rotation_matrix(back), (a, b, c, d), "{deg}");
        }
    }
}
