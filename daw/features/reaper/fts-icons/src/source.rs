use anyhow::{Context, Result};

use crate::iconify;

/// Resolve an icon source to SVG text. `aspect` = cell width / cell height
/// (1.0 square, 2.0 double-wide) — text sources use it to widen their canvas.
/// - `text:2/4` → generated stacked time-signature digits
/// - `text:+ MULTI-/MIC` → leading `+ ` renders a plus at vertical center,
///   with the remaining (stackable) text beside it
/// - `a + b` (spaces required) → composite: each part side by side in its
///   own square slot; parts resolve recursively (each needs its own prefix)
/// - anything else → Iconify `prefix:name`
pub fn resolve(source: &str, aspect: f32) -> Result<String> {
    let parts: Vec<&str> = source.split(" + ").map(str::trim).collect();
    if parts.len() > 1 {
        let mut body = String::new();
        for (i, part) in parts.iter().enumerate() {
            let svg = resolve_single(part, 1.0)?;
            body.push_str(&embed(&svg, i as f32 * 24.0)?);
        }
        let vw = 24.0 * parts.len() as f32;
        return Ok(format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {vw} 24">{body}</svg>"#
        ));
    }
    resolve_single(source, aspect)
}

fn resolve_single(source: &str, aspect: f32) -> Result<String> {
    if let Some(text) = source.strip_prefix("text:") {
        Ok(text_svg(text, aspect))
    } else {
        iconify::fetch_svg(source)
    }
}

/// Nest an SVG into a 24x24 slot at horizontal offset `x`.
fn embed(svg: &str, x: f32) -> Result<String> {
    let head_end = svg.find('>').context("malformed svg")?;
    let (head, rest) = svg.split_at(head_end);
    let head = strip_attr(strip_attr(head.to_string(), "width"), "height");
    Ok(format!(
        r#"{head} x="{x}" y="0" width="24" height="24"{rest}"#
    ))
}

fn strip_attr(head: String, name: &str) -> String {
    if let Some(start) = head.find(&format!(" {name}=\"")) {
        let val_start = start + name.len() + 3;
        if let Some(end) = head[val_start..].find('"') {
            let mut s = head;
            s.replace_range(start..val_start + end + 1, "");
            return s;
        }
    }
    head
}

fn text_svg(text: &str, aspect: f32) -> String {
    const FONT: &str = r#"text-anchor="middle" font-family="DejaVu Sans, sans-serif" font-weight="700" fill="currentColor""#;
    let vw = 24.0 * aspect.max(0.1);

    // Leading "+ " → plus glyph at vertical center, rest of the text beside it
    let (lead, text) = match text.strip_prefix("+ ") {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let lead_w = if lead { 9.0 } else { 0.0 };
    let lead_svg = if lead {
        format!(
            r#"<text x="{:.1}" y="16.3" font-size="12" {FONT}>+</text>"#,
            2.0 + lead_w / 2.0
        )
    } else {
        String::new()
    };

    let budget = vw - lead_w - 4.0; // side padding
    let cx = lead_w + (vw - lead_w) / 2.0;
    if let Some((top, bottom)) = text.split_once('/') {
        // stacked lines (time signatures, two-line abbreviations) —
        // one shared font size so the lines read as a unit
        let fs = fit(top, 12.5, budget).min(fit(bottom, 12.5, budget));
        // keep the two baselines centered as the font shrinks
        let (y1, y2) = (11.5 - (12.5 - fs) * 0.35, 23.0 - (12.5 - fs) * 0.65);
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {vw} 24">{lead_svg}<text x="{cx}" y="{y1:.1}" font-size="{fs}" {FONT}>{}</text><text x="{cx}" y="{y2:.1}" font-size="{fs}" {FONT}>{}</text></svg>"#,
            escape(top),
            escape(bottom)
        )
    } else {
        let fs = fit(text, 13.0, budget);
        // baseline so the text sits optically centered for any font size
        let y = 12.0 + fs * 0.36;
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {vw} 24">{lead_svg}<text x="{cx}" y="{y:.1}" font-size="{fs}" {FONT}>{}</text></svg>"#,
            escape(text)
        )
    }
}

/// Shrink the font so `text` fits `budget` viewBox units
/// (~0.68em avg glyph width for bold caps), capped at `max`.
fn fit(text: &str, max: f32, budget: f32) -> f32 {
    let n = text.chars().count().max(1) as f32;
    (budget / (0.68 * n)).min(max)
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
