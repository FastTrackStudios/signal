//! The SONG, SECTIONS and MARKS lanes across the ruler's top, and the
//! lines they drop through the arrangement.

use super::*;

/// The ruler's lanes: the song, its sections and its marks, over the
/// bars. Drawn per frame like the bars, and after them.
///
/// A region is a band across its lane with its name at the left; a
/// marker is a flag with its name after it. The lane names stand in
/// the panel's column, where the lanes have no timeline to be on.
pub fn lanes(
    painter: &mut impl PaintScene,
    palette: &Palette,
    font: &Font,
    view: Viewport,
    origin: (f64, f64),
    sections: &[daw_ui::studio::project::Section],
    markers: &[daw_ui::studio::project::Marker],
) {
    const SIZE: f32 = 8.0;
    let (ox, oy) = origin;
    let left = ox + view.panel_w;
    let right = ox + view.width;
    let x_of = |t: f64| t.mul_add(view.pps, left - view.scroll_x);
    for (row, name) in LANE_NAMES.iter().enumerate() {
        if !lane_shown(row) {
            continue;
        }
        let top = oy + row_top(row);
        // A rule under each lane, and the lane's name in the column.
        fill(
            painter,
            palette.tcp_rule,
            Rect::new(left - LABEL_W, top + LANE_H - 1.0, right, top + LANE_H),
        );
        label(
            painter,
            font,
            palette.text_faint,
            left,
            name,
            top + LANE_H - 4.0,
            SIZE,
        );
    }
    // Regions: a band, clipped to the timeline, named where it starts
    // — or where the view starts, if the band began off screen, so a
    // long section still says what it is.
    for section in sections {
        let row = lane_row(section.lane);
        if !lane_shown(row) {
            continue;
        }
        let top = oy + row_top(row) + 2.0;
        let x0 = x_of(section.start).max(left);
        let x1 = x_of(section.end).min(right);
        if x1 <= x0 {
            continue;
        }
        let tint = section
            .color
            .as_deref()
            .and_then(css_hex)
            .unwrap_or(palette.accent);
        fill(
            painter,
            tint.multiply_alpha(0.45),
            Rect::new(x0, top, x1, top + LANE_H - 4.0),
        );
        fill(
            painter,
            tint,
            Rect::new(x0, top, (x0 + 2.0).min(x1), top + LANE_H - 4.0),
        );
        if x1 - x0 > 24.0 {
            let room = x1 - x0 - 8.0;
            if let Some(name) = fit(font, &section.name, SIZE, room) {
                crate::tcp::glyphs(
                    painter,
                    font,
                    palette.text,
                    name,
                    x0 + 5.0,
                    top + LANE_H - 6.0,
                    SIZE,
                );
            }
        }
    }
    // Markers: a flag on its lane, named to the right of it.
    for marker in markers {
        let row = lane_row(marker.lane);
        if !lane_shown(row) {
            continue;
        }
        let top = oy + row_top(row) + 2.0;
        let x = x_of(marker.at);
        if x < left || x > right {
            continue;
        }
        let tint = marker
            .color
            .as_deref()
            .and_then(css_hex)
            .unwrap_or(palette.ruler_fg);
        fill(
            painter,
            tint,
            Rect::new(x, top, x + 2.0, top + LANE_H - 4.0),
        );
        fill(
            painter,
            tint,
            Rect::new(x, top, (x + 8.0).min(right), top + 4.0),
        );
        if let Some(name) = fit(font, &marker.name, SIZE, right - x - 6.0) {
            crate::tcp::glyphs(
                painter,
                font,
                palette.text,
                name,
                x + 5.0,
                top + LANE_H - 6.0,
                SIZE,
            );
        }
    }
}

/// The lanes' lines, down through the arrangement: every region edge
/// and every marker, from under its own band to the bottom of the view,
/// so the song's shape is read against the items and not only over
/// them.
///
/// Drawn OVER the ruler, after it: a line starts where its band or flag
/// ends and crosses the lanes below it, the tempo and the bars, as
/// REAPER's do — a section's edge read against the bar number it
/// falls on. So it has to come after [`ruler`], [`tempo`] and [`lanes`].
///
/// Where two fall on one pixel the LOWEST lane wins — a section's
/// edge over the song's, a mark over a section — and a start beats an
/// end, so the chorus's first line is the chorus's and not the end of
/// the verse before it.
pub fn lane_lines(
    painter: &mut impl PaintScene,
    palette: &Palette,
    view: Viewport,
    origin: (f64, f64),
    sections: &[daw_ui::studio::project::Section],
    markers: &[daw_ui::studio::project::Marker],
    bottom: f64,
) {
    let (ox, oy) = origin;
    let left = ox + view.panel_w;
    let right = ox + view.width;
    let x_of = |t: f64| t.mul_add(view.pps, left - view.scroll_x);
    // Every line, with what decides between two on one pixel: the lane
    // row first (lower wins), then whether it is a start.
    let mut lines: Vec<(f64, usize, bool, Color)> = Vec::new();
    for section in sections {
        let row = lane_row(section.lane);
        let tint = section
            .color
            .as_deref()
            .and_then(css_hex)
            .unwrap_or(palette.accent);
        lines.push((x_of(section.start), row, true, tint));
        lines.push((x_of(section.end), row, false, tint));
    }
    for marker in markers {
        let tint = marker
            .color
            .as_deref()
            .and_then(css_hex)
            .unwrap_or(palette.ruler_fg);
        lines.push((x_of(marker.at), lane_row(marker.lane), true, tint));
    }
    // Sorted so the winner of each pixel comes LAST, then drawn in that
    // order — the winner paints over the rest.
    lines.sort_by(|a, b| {
        a.1.cmp(&b.1)
            .then(a.2.cmp(&b.2))
            .then(a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
    });
    for (x, row, start, tint) in lines {
        if x < left || x > right {
            continue;
        }
        // The song's edges faint, a section's clearer, a mark's clearest:
        // the line's weight is the lane's.
        let alpha = match (row, start) {
            (0, _) => 0.22,
            (1, true) => 0.45,
            (1, false) => 0.3,
            (_, true) => 0.7,
            (_, false) => 0.5,
        };
        // From the foot of the band or flag it belongs to — which is
        // drawn inset 2px from its lane — so the line continues it.
        // A lane the slim ruler leaves off drops its line from the
        // ruler's foot.
        let top = if lane_shown(row) {
            oy + row_top(row) + LANE_H - 2.0
        } else {
            oy + ruler_h()
        };
        let x = x.round();
        fill(
            painter,
            tint.multiply_alpha(alpha),
            Rect::new(x, top, x + 1.0, bottom),
        );
    }
}

/// Which lane row a lane index lands on. Lanes are numbered from 0, as
/// REAPER's API and the daw service number them (`CoreLane`: SONG 0,
/// SECTIONS 1, MARKS 2) — the `.rpp` file's own 1-based rows are
/// converted where it is read. Anything past the last row is drawn on it
/// rather than off the strip.
#[must_use]
pub fn lane_row(lane: u32) -> usize {
    usize::try_from(lane)
        .unwrap_or(0)
        .min(LANES.saturating_sub(1))
}
