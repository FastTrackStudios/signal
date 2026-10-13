//! Waveform reduction and polygon construction.
/// A member's mean peak value over one column's time span.
///
/// The member's peaks are uniform over `[start, end]` seconds; the
/// column covers `[t0, t1)`. Bins overlapping the column are averaged,
/// and a column entirely outside the member is silence — the member
/// simply isn't sounding there.
fn member_column(peaks: &[f32], start: f64, end: f64, t0: f64, t1: f64) -> f32 {
    if peaks.is_empty() || end <= start {
        return 0.0;
    }
    let n = peaks.len();
    let span = end - start;
    let lo = ((t0 - start) / span * n as f64).floor() as isize;
    let hi = ((t1 - start) / span * n as f64).ceil() as isize;
    if hi <= 0 || lo >= n as isize {
        return 0.0;
    }
    let lo = lo.max(0) as usize;
    let hi = (hi.min(n as isize) as usize).max(lo + 1);
    peaks[lo..hi].iter().sum::<f32>() / (hi - lo) as f32
}

/// The summed waveform of a role lane, one value per viewport column.
///
/// Per column, the **mean** of the members' peaks — the members are
/// phase-aligned mics of one source, so the mean is the mix a SUM bus
/// would render — normalised so the lane's loudest column is 1.0.
/// Members with no peaks are skipped rather than dragging the mean
/// down; a lane with one member yields that member.
///
/// `members` is `(peaks, start_secs, end_secs)` per member;
/// `view_start`/`view_end` bound the columns, in seconds.
// r[impl drums.lanes.summed]
pub fn summed_columns(
    members: &[(&[f32], f64, f64)],
    view_start: f64,
    view_end: f64,
    columns: usize,
) -> Vec<f32> {
    let columns = columns.max(2);
    let mut out = vec![0.0f32; columns];
    let live: Vec<_> = members.iter().filter(|(p, _, _)| !p.is_empty()).collect();
    if live.is_empty() || view_end <= view_start {
        return out;
    }
    let step = (view_end - view_start) / columns as f64;
    for (i, v) in out.iter_mut().enumerate() {
        let t0 = view_start + step * i as f64;
        let sum: f32 = live
            .iter()
            .map(|(p, s, e)| member_column(p, *s, *e, t0, t0 + step))
            .sum();
        *v = sum / live.len() as f32;
    }
    let max = out.iter().copied().fold(0.0f32, f32::max);
    if max > 0.0 {
        for v in &mut out {
            *v /= max;
        }
    }
    out
}

/// Column values as one mirrored polygon, the shape
/// [`crate::canvas::take_waveform`] draws: zero on the row's midline, full
/// scale just short of its edges. `None` when there is nothing to draw.
pub(super) fn columns_polygon(
    cols: &[f32],
    w: f64,
    y0: f64,
    h: f64,
) -> Option<super::geometry::Polygon> {
    if cols.len() < 2 || cols.iter().all(|&v| v <= 0.0) {
        return None;
    }
    let mid = y0 + h * 0.5;
    let max_half = h * 0.46;
    let last = cols.len() - 1;
    let mut top = Vec::with_capacity(cols.len() * 2);
    let mut bottom = Vec::with_capacity(cols.len());
    for (i, &v) in cols.iter().enumerate() {
        let x = w * (i as f64 / last as f64);
        let half = max_half * f64::from(v).clamp(0.0, 1.0);
        top.push((x, mid - half));
        bottom.push((x, mid + half));
    }
    bottom.reverse();
    top.extend(bottom);
    Some(top)
}

#[cfg(test)]
mod tests {
    use super::summed_columns;

    // r[verify drums.lanes.summed]
    #[test]
    fn summed_columns_means_members_and_normalises() {
        // Two members over the same 4-second span, viewed whole in four
        // columns: column values are the per-column MEAN of the two,
        // then scaled so the loudest column is exactly 1.0.
        let a = [1.0f32, 1.0, 0.0, 0.0];
        let b = [0.0f32, 1.0, 0.0, 0.0];
        let cols = summed_columns(&[(&a, 0.0, 4.0), (&b, 0.0, 4.0)], 0.0, 4.0, 4);
        assert_eq!(cols, vec![0.5, 1.0, 0.0, 0.0]);
        assert_eq!(cols.iter().copied().fold(0.0f32, f32::max), 1.0);
    }

    // r[verify drums.lanes.summed]
    #[test]
    fn summed_columns_skips_empty_members_and_scales_one() {
        // A member with no peaks is skipped, not averaged in as silence.
        let a = [1.0f32, 1.0, 0.0, 0.0];
        let none: [f32; 0] = [];
        let cols = summed_columns(&[(&a, 0.0, 4.0), (&none, 0.0, 4.0)], 0.0, 4.0, 4);
        assert_eq!(cols, vec![1.0, 1.0, 0.0, 0.0]);

        // A single quiet member still fills the lane: normalised so its
        // own loudest column is 1.0.
        let quiet = [0.2f32, 0.4];
        let cols = summed_columns(&[(&quiet, 0.0, 2.0)], 0.0, 2.0, 2);
        assert_eq!(cols, vec![0.5, 1.0]);
    }

    // r[verify drums.lanes.summed]
    #[test]
    fn summed_columns_is_silent_outside_a_member() {
        // A member spanning only the first half of the view contributes
        // silence to columns past its end.
        let a = [1.0f32, 1.0];
        let cols = summed_columns(&[(&a, 0.0, 2.0)], 0.0, 4.0, 4);
        assert_eq!(cols, vec![1.0, 1.0, 0.0, 0.0]);
    }
}
