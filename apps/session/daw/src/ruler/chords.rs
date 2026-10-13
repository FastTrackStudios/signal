//! The CHORDS lane: the song's harmony under its sections.

use super::*;

/// The CHORDS lane: the song's key changes and its chords, under the
/// marks and over the tempo, when the ruler shows it.
///
/// A key change is a violet tag with the key in it — the KEY track's own
/// colour — and a chord is its symbol from where it starts, the next
/// chord's edge a faint tick. Numbers or chord names by the same
/// [`Lettering`](crate::arrangement::Lettering) the items wear; with the
/// items on their titles, the lane reads in numbers — the song's own
/// notation, and the one that survives a change of key.
pub fn chord_lane(
    painter: &mut impl PaintScene,
    palette: &Palette,
    font: &Font,
    view: Viewport,
    origin: (f64, f64),
    chart: &crate::arrangement::ChartMarks,
    lettering: crate::arrangement::Lettering,
) {
    use crate::arrangement::Lettering;
    const SIZE: f32 = 10.0;
    const TAG: f32 = 8.0;
    if !chords_shown() {
        return;
    }
    let (ox, oy) = origin;
    // Under SECTIONS, over MARKS — see `row_top`.
    let top = oy + row_top(MARKS_ROW) - CHORD_H;
    let left = ox + view.panel_w;
    let right = ox + view.width;
    let x_of = |t: f64| t.mul_add(view.pps, left - view.scroll_x);
    fill(
        painter,
        palette.tcp_rule,
        Rect::new(left - LABEL_W, top + CHORD_H - 1.0, right, top + CHORD_H),
    );
    label(
        painter,
        font,
        palette.text_faint,
        left,
        "CHORDS",
        top + CHORD_H - 4.0,
        8.0,
    );
    // Where each key tag ends, so a chord starting under one is written
    // after it rather than through it.
    let tags: Vec<(f64, f64, &str)> = chart
        .keys
        .iter()
        .map(|key| {
            let x = x_of(key.at);
            (x, x + font.width(&key.name, TAG) + 10.0, key.name.as_str())
        })
        .collect();
    for chord in &chart.chords {
        let x0 = x_of(chord.x0);
        let x1 = x_of(chord.x1).min(right);
        if x1 <= left || x0 >= right {
            continue;
        }
        if x0 >= left {
            fill(
                painter,
                palette.text_faint.multiply_alpha(0.5),
                Rect::new(x0, top + 3.0, x0 + 1.0, top + CHORD_H - 3.0),
            );
        }
        // Past a key tag that starts where this chord does, and past the
        // lane's left edge for a chord that began off screen.
        let under_tag = tags
            .iter()
            .filter(|(at, _, _)| (at - x0).abs() < 1.0)
            .map(|(_, end, _)| *end)
            .fold(x0, f64::max);
        let at = under_tag.max(left) + 4.0;
        let text = match lettering {
            Lettering::Chords => chord.spelled.chords.as_str(),
            Lettering::Titles | Lettering::Numbers => chord.spelled.numbers.as_str(),
        };
        // Smaller rather than shorter: a chord cut to fit is a different
        // chord ("5/7" read as "5/"), so the lettering shrinks, and a
        // chord with no room at all is left to its tick.
        let (fitted, size) = font.fit(text, SIZE, 6.0, x1 - at - 2.0);
        if fitted == text {
            let baseline = top + (CHORD_H + f64::from(size) * 0.72) / 2.0;
            crate::tcp::glyphs(painter, font, palette.text, text, at, baseline, size);
        }
    }
    let violet = Color::from_rgba8(0x7c, 0x3a, 0xed, 0xff);
    for (x0, x1, name) in tags {
        if x1 <= left || x0 >= right {
            continue;
        }
        let x0 = x0.max(left);
        fill(
            painter,
            violet,
            Rect::new(x0, top + 3.0, x1.min(right), top + CHORD_H - 3.0),
        );
        crate::tcp::glyphs(
            painter,
            font,
            palette.text,
            name,
            x0 + 5.0,
            top + (CHORD_H + f64::from(TAG) * 0.72) / 2.0,
            TAG,
        );
    }
}
