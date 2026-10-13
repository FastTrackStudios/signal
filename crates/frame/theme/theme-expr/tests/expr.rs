//! The binding language, as a theme author writes it.

use theme_expr::{parse, Expr, Rgba, Scope, Value};

/// A strip 86 wide and 400 tall, with a track's data in slots.
struct Strip {
    fields: Vec<(&'static str, Value)>,
}

impl Strip {
    fn new() -> Self {
        Self {
            fields: vec![
                ("track.name", Value::text("Lead Vox")),
                ("track.volume", Value::Num(0.5)),
                ("track.mute", Value::Bool(true)),
                ("track.color", Value::Color(Rgba::from_hex("#3366cc").unwrap())),
                ("meter.peak", Value::Missing),
            ],
        }
    }

    fn compile(&self, src: &str) -> Expr {
        let mut e = parse(src).unwrap_or_else(|err| panic!("{src}: {err}"));
        e.inline_tokens(&mut |name| match name {
            "strip.ground" => Some(Value::Color(Rgba::from_hex("#202020").unwrap())),
            "pad" => Some(Value::Num(4.0)),
            _ => None,
        })
        .unwrap();
        e.resolve_fields(&mut |name| self.fields.iter().position(|(n, _)| *n == name).map(|i| i as u32)).unwrap();
        e.fold()
    }

    fn eval(&self, src: &str) -> Value {
        self.compile(src).eval(self)
    }
}

impl Scope for Strip {
    fn size(&self) -> (f64, f64) {
        (86.0, 400.0)
    }

    fn field(&self, slot: u32) -> Value {
        self.fields.get(slot as usize).map_or(Value::Missing, |(_, v)| v.clone())
    }
}

#[test]
fn arithmetic_and_the_box() {
    let s = Strip::new();
    assert_eq!(s.eval("w - $pad * 2"), Value::Num(78.0));
    assert_eq!(s.eval("h / 4 + 1"), Value::Num(101.0));
    assert_eq!(s.eval("-(2 + 3) * 2"), Value::Num(-10.0));
    assert_eq!(s.eval("7 % 3"), Value::Num(1.0));
}

#[test]
fn conditions_choose() {
    let s = Strip::new();
    assert_eq!(s.eval("w >= 60 && !track.mute"), Value::Bool(false));
    assert_eq!(s.eval("track.mute ? \"M\" : \"\""), Value::text("M"));
    assert_eq!(s.eval("if(w < 60, 1, 2)"), Value::Num(2.0));
}

#[test]
fn colours_derive() {
    let s = Strip::new();
    let mixed = s.eval("mix($strip.ground, track.color, 0.5)").color().unwrap();
    assert_eq!(mixed.to_hex(), "#2a4376");
    assert_eq!(s.eval("alpha(track.color, 0.5)").color().unwrap().to_hex(), "#3366cc80");
    assert_eq!(s.eval("ink($strip.ground)").color().unwrap().to_hex(), "#f2f2f2");
    assert_eq!(s.eval("ink(#ffffff)").color().unwrap().to_hex(), "#141414");
    // A hover: channels scaled, the red clamped — not washed toward white.
    assert_eq!(s.eval("lift(#ef4444, 0.25)").color().unwrap().to_hex(), "#ff5555");
    assert_eq!(s.eval("lighten(#ef4444, 0.25)").color().unwrap().to_hex(), "#f37373");
}

#[test]
fn text_joins_and_formats() {
    let s = Strip::new();
    assert_eq!(s.eval("upper(track.name)"), Value::text("LEAD VOX"));
    assert_eq!(s.eval("fmt(db(track.volume), 1) + \" dB\""), Value::text("-6.0 dB"));
    assert_eq!(s.eval("len(track.name)"), Value::Num(8.0));
}

#[test]
fn missing_propagates_and_or_recovers() {
    let s = Strip::new();
    assert_eq!(s.eval("meter.peak * 2"), Value::Missing);
    assert_eq!(s.eval("or(meter.peak, 0)"), Value::Num(0.0));
    assert_eq!(s.eval("1 / 0"), Value::Missing);
    // A wrong type has nothing to show either.
    assert_eq!(s.eval("track.color + 1"), Value::Missing);
}

#[test]
fn folding_leaves_only_what_varies() {
    let s = Strip::new();
    // Tokens and literals fold away; the box and the fields stay.
    assert_eq!(s.compile("$pad * 2 + 1"), Expr::Lit(Value::Num(9.0)));
    assert!(s.compile("w - $pad").refs().size);
    assert_eq!(s.compile("true ? track.name : 3").refs().slots.len(), 1);
    assert!(s.compile("mix($strip.ground, #ffffff, 0.1)").constant().is_some());
}

#[test]
fn refs_name_what_an_expression_reads() {
    let e = parse("$strip.ground == track.color && w > 10").unwrap();
    let r = e.refs();
    assert!(r.size);
    assert_eq!(r.tokens.into_iter().collect::<Vec<_>>(), ["strip.ground"]);
    assert_eq!(r.fields.into_iter().collect::<Vec<_>>(), ["track.color"]);
}

#[test]
fn undefined_names_are_reported() {
    let mut e = parse("$nope + track.nope").unwrap();
    assert_eq!(e.inline_tokens(&mut |_| None), Err("nope".to_owned()));
    assert_eq!(e.resolve_fields(&mut |_| None), Err("track.nope".to_owned()));
}

#[test]
fn errors_say_where() {
    for (src, needle) in [
        ("1 +", "expected a value"),
        ("frob(1)", "unknown function `frob`"),
        ("clamp(1, 2)", "`clamp` takes 3 arguments"),
        ("#12", "bad colour"),
        ("(1", "expected `)`"),
        ("1 2", "unexpected `2`"),
        ("\"open", "unterminated string"),
    ] {
        let err = parse(src).unwrap_err();
        assert!(err.message.contains(needle), "{src}: {err}");
    }
}

#[test]
fn precedence_is_conventional() {
    let s = Strip::new();
    assert_eq!(s.eval("1 + 2 * 3"), Value::Num(7.0));
    assert_eq!(s.eval("1 < 2 == true"), Value::Bool(true));
    assert_eq!(s.eval("false || true && false"), Value::Bool(false));
    assert_eq!(s.eval("1 > 2 ? 10 : 2 > 1 ? 20 : 30"), Value::Num(20.0));
    assert_eq!(s.eval("1e-3 * 1000"), Value::Num(1.0));
}

#[test]
fn arc_is_path_data_from_twelve_oclock() {
    let s = Strip::new();
    let Value::Text(path) = s.eval("arc(12, 12, 10, 0, 90)") else { panic!("text") };
    // Starts at twelve o'clock, ends at three.
    assert!(path.starts_with("M12,2"), "{path}");
    assert!(path.ends_with("22,12"), "{path}");
}
