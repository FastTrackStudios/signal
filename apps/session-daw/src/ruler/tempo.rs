//! The TEMPO lane: where the tempo and the signature change.

use super::*;

/// The tempo strip: where the tempo or the signature changes, and to
/// what.
///
/// Its own row rather than a flag among the marks: a tempo change is
/// not a place in the song, it is a change to what every number below
/// it MEANS, and filing it with the marks would file it among the
/// things it reinterprets.
///
/// The reading repeats at the left edge when the change that set it is
/// off screen. A tempo you cannot see is a tempo you will assume, and
/// the assumption is always whatever the project started at.
pub fn tempo(
    painter: &mut impl PaintScene,
    palette: &Palette,
    font: &Font,
    view: Viewport,
    origin: (f64, f64),
    changes: &[daw_ui::studio::project::TempoChange],
) {
    const SIZE: f32 = 8.0;
    if slim() {
        return;
    }
    let (ox, oy) = origin;
    let top = oy + ruler_h() - bars_h() - TEMPO_H;
    let left = ox + view.panel_w;
    let right = ox + view.width;
    fill(
        painter,
        palette.tcp_rule,
        Rect::new(left - LABEL_W, top + TEMPO_H - 1.0, right, top + TEMPO_H),
    );
    label(
        painter,
        font,
        palette.text_faint,
        left,
        "TEMPO",
        top + TEMPO_H - 3.0,
        SIZE,
    );

    let x_of = |t: f64| t.mul_add(view.pps, left - view.scroll_x);
    let (from, to) = view.secs();

    if let Some(current) = changes
        .iter()
        .take_while(|change| change.at <= from + 1e-9)
        .last()
        && changes.iter().any(|change| change.at > from)
    {
        crate::tcp::glyphs(
            painter,
            font,
            palette.text_faint,
            &reading(current),
            left + 4.0,
            top + TEMPO_H - 3.0,
            SIZE,
        );
    }

    for change in changes {
        if change.at < from || change.at > to {
            continue;
        }
        let x = x_of(change.at);
        if x < left - 1.0 || x > right {
            continue;
        }
        fill(
            painter,
            palette.accent,
            Rect::new(x, top + 1.0, x + 1.0, top + TEMPO_H - 1.0),
        );
        crate::tcp::glyphs(
            painter,
            font,
            palette.text,
            &reading(change),
            x + 4.0,
            top + TEMPO_H - 3.0,
            SIZE,
        );
    }
}

/// A tempo change as it reads: "120 4/4".
///
/// Both halves always, even when only one of them moved. A strip that
/// showed the tempo at one change and the signature at the next would
/// make you look back through the project to answer either question.
pub fn reading(change: &daw_ui::studio::project::TempoChange) -> String {
    let bpm = if (change.bpm - change.bpm.round()).abs() < 0.05 {
        format!("{}", change.bpm.round() as i64)
    } else {
        format!("{:.1}", change.bpm)
    };
    format!("{bpm} {}/{}", change.beats_per_bar, change.beat_unit)
}

/// The GRID lane, in the TEMPO lane's place while editing.
///
/// Each Grid region of the track being edited (and its folders') as a
/// band named for its grid (`1/2T`) — the grid the lines under it are
/// drawn on and a quantize puts its hits on.
pub fn grid_lane(
    painter: &mut impl PaintScene,
    palette: &Palette,
    font: &Font,
    view: Viewport,
    origin: (f64, f64),
    (figures, parts): (&[GridRegion], &[GridRegion]),
) {
    const SIZE: f32 = 8.0;
    if slim() {
        return;
    }
    let (ox, oy) = origin;
    let top = oy + ruler_h() - bars_h() - TEMPO_H;
    let left = ox + view.panel_w;
    let right = ox + view.width;
    fill(
        painter,
        palette.tcp_rule,
        Rect::new(left - LABEL_W, top + TEMPO_H - 1.0, right, top + TEMPO_H),
    );
    label(
        painter,
        font,
        palette.text_faint,
        left,
        "GRID",
        top + TEMPO_H - 3.0,
        SIZE,
    );
    let x_of = |t: f64| t.mul_add(view.pps, left - view.scroll_x);
    // A part's grid quiet under the figures, which are in the accent.
    let quiet = parts.iter().map(|r| (r, palette.text_faint));
    let loud = figures.iter().map(|r| (r, palette.accent));
    for (&(start, end, division), tint) in quiet.chain(loud) {
        let (a, b) = (x_of(start).max(left), x_of(end).min(right));
        if b <= a {
            continue;
        }
        fill(
            painter,
            tint.multiply_alpha(0.3),
            Rect::new(a, top + 1.0, b, top + TEMPO_H - 1.0),
        );
        fill(
            painter,
            tint,
            Rect::new(a, top + 1.0, a + 1.0, top + TEMPO_H - 1.0),
        );
        crate::tcp::glyphs(
            painter,
            font,
            palette.text,
            &crate::track_regions::grid_name(division),
            a + 3.0,
            top + TEMPO_H - 3.0,
            SIZE,
        );
    }
}
