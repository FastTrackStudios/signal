//! Text primitives as SVG `<text>`: family, weight, style and width carried
//! as font attributes (the viewer sets the glyphs), laid out line by line
//! the way the painter lays them out.
//!
//! Figma imports never reach this: their text is exported outlines
//! (`Path` primitives), which the parity corpus compares.

use render_fonts::{FontBook, FontSpec};
use svg::node::element::{TSpan, Text};

use crate::anyrender_renderer::{PaintPrimitive, Rgba, TextAlignment, TextCase};

use super::render::{fmt_f64, svg_color_paint_values, to_u8};

/// Where the painter sets a text box's first glyph, relative to the box.
const ANCHOR_SHIFT: f64 = 1.0;

fn cased(text: &str, case: &TextCase) -> String {
    match case {
        TextCase::Upper | TextCase::SmallCaps | TextCase::SmallCapsForced => text.to_uppercase(),
        TextCase::Lower => text.to_lowercase(),
        TextCase::Title => {
            let mut out = String::with_capacity(text.len());
            let mut start = true;
            for c in text.chars() {
                if start && !c.is_whitespace() {
                    out.extend(c.to_uppercase());
                } else {
                    out.push(c);
                }
                start = c.is_whitespace();
            }
            out
        }
        TextCase::Original => text.to_string(),
    }
}

/// `font-family`: the family asked for, the face it resolves to, the book's
/// fallbacks, then a generic — so a viewer without frame's fonts still
/// sets something close.
fn family_list(fonts: &FontBook, spec: &FontSpec) -> String {
    let mut names: Vec<String> = Vec::new();
    let mut push = |n: &str| {
        if !names
            .iter()
            .any(|m| render_fonts::fold(m) == render_fonts::fold(n))
        {
            names.push(n.to_string());
        }
    };
    if let Some(f) = &spec.family {
        push(f);
    }
    if let Some(r) = fonts.resolve(&spec.query()) {
        push(&r.face.family);
    }
    for f in fonts.fallback() {
        push(f);
    }
    let quoted: Vec<String> = names
        .iter()
        .map(|n| {
            if n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                n.clone()
            } else {
                format!("'{n}'")
            }
        })
        .collect();
    format!("{}, sans-serif", quoted.join(", "))
}

/// Font attributes for `spec` — every one on the node, only what differs
/// from `base` on a run.
fn font_attrs(
    fonts: &FontBook,
    spec: &FontSpec,
    base: Option<&FontSpec>,
) -> Vec<(&'static str, String)> {
    let q = spec.query();
    let b = base.map(FontSpec::query);
    let mut a = Vec::new();
    if b.is_none_or(|b| {
        render_fonts::fold(b.family.unwrap_or("")) != render_fonts::fold(q.family.unwrap_or(""))
    }) {
        a.push(("font-family", family_list(fonts, spec)));
    }
    if b.is_none_or(|b| b.weight != q.weight) {
        a.push(("font-weight", fmt_f64(f64::from(q.weight))));
    }
    if q.italic || b.is_some_and(|b| b.italic) {
        a.push((
            "font-style",
            if q.italic { "italic" } else { "normal" }.to_string(),
        ));
    }
    if (q.width - 100.0).abs() > f32::EPSILON
        || b.is_some_and(|b| (b.width - 100.0).abs() > f32::EPSILON)
    {
        a.push(("font-stretch", format!("{}%", fmt_f64(f64::from(q.width)))));
    }
    a
}

fn fill(color: &Rgba) -> (String, Option<String>) {
    svg_color_paint_values(to_u8(color.r), to_u8(color.g), to_u8(color.b), color.a)
}

/// A `Text` primitive as an SVG `<text>`, offset by (`dx`, `dy`); `None`
/// for any other primitive or empty text.
#[must_use]
pub fn text_element(
    primitive: &PaintPrimitive,
    dx: f64,
    dy: f64,
    fonts: &FontBook,
) -> Option<Text> {
    let PaintPrimitive::Text {
        x,
        y,
        text,
        color,
        font_size,
        line_height,
        letter_spacing,
        text_case,
        font,
        style_runs,
        width,
        align,
        ..
    } = primitive
    else {
        return None;
    };
    let text = cased(text, text_case);
    if text.is_empty() {
        return None;
    }
    let size = font_size.max(1.0);
    let line_height = line_height.unwrap_or(size * 1.2).max(1.0);
    let ascent = fonts
        .chain(&font.query())
        .primary()
        .map_or(size * 0.8, |f| f.ascent(size));
    let (anchor, ax) = match align {
        TextAlignment::Left => ("start", 0.0),
        TextAlignment::Center => ("middle", width * 0.5),
        TextAlignment::Right => ("end", *width),
    };
    let (x0, y0) = (x + dx + ANCHOR_SHIFT + ax, y + dy + ANCHOR_SHIFT + ascent);

    let (hex, opacity) = fill(color);
    let mut el = Text::new("")
        .set("font-size", fmt_f64(size))
        .set("fill", hex);
    if let Some(o) = opacity {
        el = el.set("fill-opacity", o);
    }
    for (k, v) in font_attrs(fonts, font, None) {
        el = el.set(k, v);
    }
    if letter_spacing.abs() > f64::EPSILON {
        el = el.set("letter-spacing", fmt_f64(*letter_spacing));
    }
    if anchor != "start" {
        el = el.set("text-anchor", anchor);
    }

    // Split the runs (or the whole text) into lines of styled spans.
    let spans: Vec<(String, Option<&crate::anyrender_renderer::TextStyleRun>)> =
        if style_runs.is_empty() {
            vec![(text.clone(), None)]
        } else {
            style_runs
                .iter()
                .map(|r| (cased(&r.text, text_case), Some(r)))
                .collect()
        };
    let mut lines: Vec<Vec<(String, Option<&crate::anyrender_renderer::TextStyleRun>)>> =
        vec![Vec::new()];
    for (s, run) in spans {
        for (i, part) in s.split('\n').enumerate() {
            if i > 0 {
                lines.push(Vec::new());
            }
            if !part.is_empty()
                && let Some(line) = lines.last_mut()
            {
                line.push((part.to_string(), run));
            }
        }
    }
    for (i, line) in lines.into_iter().enumerate() {
        let mut row = TSpan::new("")
            .set("x", fmt_f64(x0))
            .set("y", fmt_f64(y0 + i as f64 * line_height));
        for (s, run) in line {
            let Some(run) = run else {
                row = row.add(svg::node::Text::new(s));
                continue;
            };
            let mut span = TSpan::new(s);
            for (k, v) in font_attrs(fonts, &run.font().over(font), Some(font)) {
                span = span.set(k, v);
            }
            if let Some(sz) = run.font_size.filter(|s| (s - size).abs() > f64::EPSILON) {
                span = span.set("font-size", fmt_f64(sz));
            }
            if let Some(ls) = run
                .letter_spacing
                .filter(|l| (l - letter_spacing).abs() > f64::EPSILON)
            {
                span = span.set("letter-spacing", fmt_f64(ls));
            }
            if let Some(c) = run.color.as_ref().filter(|c| *c != color) {
                let (hex, opacity) = fill(c);
                span = span.set("fill", hex);
                if let Some(o) = opacity {
                    span = span.set("fill-opacity", o);
                }
            }
            row = row.add(span);
        }
        el = el.add(row);
    }
    Some(el)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anyrender_renderer::{BlendMix, TextStyleRun};

    fn prim(font: FontSpec, runs: Vec<TextStyleRun>) -> PaintPrimitive {
        PaintPrimitive::Text {
            node_id: document_proto::NodeId::new(),
            x: 10.0,
            y: 20.0,
            text: "GAIN\ndrive".into(),
            color: Rgba {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
            font_size: 18.0,
            line_height: None,
            letter_spacing: 0.5,
            text_case: TextCase::Original,
            font,
            style_runs: runs,
            blend: BlendMix::Normal,
            width: 100.0,
            align: TextAlignment::Center,
            effects: Vec::new(),
        }
    }

    #[test]
    fn text_carries_family_weight_style_and_width() {
        let book = FontBook::bundled();
        let font = FontSpec {
            family: Some("Oswald".into()),
            weight: Some(700.0),
            style: Some("italic".into()),
            width: Some(75.0),
        };
        let svg = text_element(&prim(font, Vec::new()), 0.0, 0.0, book)
            .unwrap()
            .to_string();
        assert!(
            svg.contains("font-family=\"Oswald, Inter, sans-serif\""),
            "{svg}"
        );
        assert!(svg.contains("font-weight=\"700\""), "{svg}");
        assert!(svg.contains("font-style=\"italic\""), "{svg}");
        assert!(svg.contains("font-stretch=\"75%\""), "{svg}");
        assert!(svg.contains("text-anchor=\"middle\""), "{svg}");
        let words: Vec<&str> = svg.split(['<', '>']).map(str::trim).collect();
        assert!(words.contains(&"GAIN") && words.contains(&"drive"), "{svg}");
    }

    #[test]
    fn runs_carry_only_what_they_change() {
        let book = FontBook::bundled();
        let run = |text: &str, weight: Option<f64>, family: Option<&str>| TextStyleRun {
            text: text.into(),
            color: None,
            font_size: None,
            letter_spacing: None,
            font_style: None,
            font_weight: weight,
            font_family: family.map(Into::into),
            font_width: None,
        };
        let svg = text_element(
            &prim(
                FontSpec::default(),
                vec![
                    run("GAIN\n", Some(900.0), None),
                    run("drive", None, Some("Dancing Script")),
                ],
            ),
            0.0,
            0.0,
            book,
        )
        .unwrap()
        .to_string();
        assert!(svg.contains("font-family=\"Inter, sans-serif\""), "{svg}");
        assert!(svg.contains("font-weight=\"400\""), "{svg}");
        let squashed: String = svg.lines().map(str::trim).collect();
        assert!(
            squashed.contains("<tspan font-weight=\"900\">GAIN</tspan>"),
            "{svg}"
        );
        assert!(
            squashed.contains(
                "<tspan font-family=\"&apos;Dancing Script&apos;, Inter, sans-serif\">drive</tspan>"
            ),
            "{svg}"
        );
    }
}
