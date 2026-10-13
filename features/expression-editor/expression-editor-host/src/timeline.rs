//! Host timeline metadata in document units.
use daw::service::ProjectContext;
/// Carry the project's regions onto a document, in its own time units.
///
/// The sections are how a player knows where they are — "bar 38" says
/// nothing, "CH 1" says everything — so every lane's ruler shows them.
/// Host colours come through as `#rrggbb`; REAPER's section colours are
/// the ones the band already knows from the arrange view.
/// Attach the host's timeline chrome — the song's sections — to a doc.
///
/// Both kinds, because sessions use both. A region is a named *span*
/// and a marker is a named *point*, and which one a song's sections
/// live in comes down to how it was set up: the album's projects
/// normally carry regions, but `set in stone` puts its whole structure
/// — IN, VS 1, CH 1, 7/4 part, SOLO A, OUT — in sixteen markers and no
/// regions at all. Reading only regions left the ruler blank on exactly
/// the project with the most structure to show.
///
/// Markers are kept as points rather than being stretched into spans up
/// to the next one. Not every marker is a section boundary — `tempo
/// change` and `back to 4/4` are annotations — so inventing spans from
/// them would draw a song structure that was never written.
pub fn attach_timeline<D: expression_editor_audio::daw_bound::DrumDaw>(
    daw: &D,
    ctx: &ProjectContext,
    doc: &mut expression_editor_core::ExpressionDoc,
) {
    use daw::service::{Markers, Projects, Regions};
    let ups = doc.time_base.units_per_second(120.0);
    if ups <= 0.0 {
        return;
    }
    // The lane's name, for whichever ruler lane an item is filed under.
    let lane_of = |idx: Option<u32>| {
        idx.map(|i| {
            let name = Projects::get_ruler_lane_name(daw, ctx.clone(), i);
            // An unnamed lane still groups; it just has to be labelled
            // by its number rather than pretending to a name.
            let name = if name.is_empty() {
                format!("Lane {i}")
            } else {
                name
            };
            (i, name)
        })
    };
    doc.regions = Regions::all(daw, ctx.clone())
        .into_iter()
        .map(|r| expression_editor_core::doc::Region {
            start: r.time_range.start_seconds() * ups,
            end: r.time_range.end_seconds() * ups,
            label: r.name,
            color: r.color.map(|c| format!("#{c:06x}")),
            lane: lane_of(r.lane),
        })
        .collect();
    // Bar lines from the host's tempo map, so the view can page a
    // phrase at a time and land on a downbeat in 6/8 and 7/4 as well as
    // in 4/4.
    // r[impl drums.view.page-bars]
    let take_secs = if ups > 0.0 { doc.end / ups } else { 0.0 };
    doc.bars = bar_grid(daw, ctx, take_secs)
        .into_iter()
        .map(|t| t * ups)
        .collect();
    doc.markers = Markers::all(daw, ctx.clone())
        .into_iter()
        // A marker whose position will not resolve to seconds cannot be
        // drawn on a time axis; dropping it beats drawing it at zero,
        // where it would claim the downbeat.
        .filter_map(|m| {
            Some(expression_editor_core::doc::Marker {
                t: m.position.seconds()? * ups,
                label: Some(m.name).filter(|n| !n.is_empty()),
                color: m.color.map(|c| format!("#{c:06x}")),
                lane: lane_of(m.lane),
            })
        })
        .collect();
}

/// Bar start times across the take, from the host's tempo map.
///
/// Asks the map where each measure begins rather than multiplying a
/// bar length, because a real take does not have one bar length.
/// `set in stone` is 6/8, changes tempo at 77s, and has a 7/4 section
/// from bar 148 — three different bar durations in one song. Anything
/// derived from a single bpm and a `beats_per_bar` of 4 would drift out
/// of phase within a few bars and put every fill in the wrong place.
///
/// Returns `n + 1` boundaries for `n` bars, the last being the end of
/// the take, which is the shape [`expression_editor_core::fills::detect_fills`] expects.
///
/// Empty when the map cannot place bars — a caller that gets nothing
/// back should do nothing rather than fall back to a guessed grid.
// r[impl drums.fills.bars]
pub fn bar_grid<D: expression_editor_audio::daw_bound::DrumDaw>(
    daw: &D,
    ctx: &ProjectContext,
    take_secs: f64,
) -> Vec<f64> {
    use daw::service::TempoMap;
    if take_secs <= 0.0 {
        return Vec::new();
    }
    let (first_bar, _, _) = TempoMap::time_to_musical(daw, ctx.clone(), 0.0);
    let mut out = Vec::new();
    // A guard rather than a `while true`: a map that answers
    // nonsensically would otherwise spin here forever.
    let max_bars = 4096;
    for i in 0..max_bars {
        let t = TempoMap::musical_to_time(daw, ctx.clone(), first_bar + i, 0, 0.0);
        // Bars must advance. A map that repeats or goes backwards is
        // broken, and continuing would produce zero-length bars that
        // every hit falls into at once.
        if let Some(&last) = out.last()
            && t <= last
        {
            break;
        }
        out.push(t);
        if t >= take_secs {
            break;
        }
    }
    // The grid has to cover the take; the detector scores the span
    // between consecutive boundaries and would drop a fill in the last,
    // unterminated bar.
    match out.last() {
        Some(&last) if last < take_secs => out.push(take_secs),
        None => return Vec::new(),
        _ => {}
    }
    if out.len() < 2 { Vec::new() } else { out }
}
