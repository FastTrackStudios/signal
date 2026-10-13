//! A chart for a song that has none, written from what it does have: its
//! sections, its song region and its tempo map.
//!
//! A song prepared from a chart keeps the chart; one prepared without
//! (sections laid by hand, by a model, by another DAW) still has the
//! form of a chart in its ruler — every section a region on the Sections
//! lane, the song a region of its own — so the chart view shows that form
//! rather than nothing: each section by name and bar count, the song's
//! name, tempo and metre at its head. Chords come when a chart does.

use daw::service::ProjectContext;
use session::ruler_lanes::CoreLane;

/// The chart text of `project`'s sections, `None` when it has none.
#[must_use]
pub fn of_song<D>(daw: &D, project: &ProjectContext) -> Option<String>
where
    D: daw::service::Regions + daw::service::TempoMap,
{
    let regions = daw.all(project.clone());
    let song_lane = CoreLane::Song.lane_index();
    let section_lane = CoreLane::Sections.lane_index();
    let song = regions.iter().find(|r| r.lane == Some(song_lane));
    let mut sections: Vec<&daw_proto::Region> = regions
        .iter()
        .filter(|r| r.lane == Some(section_lane) || (r.lane.is_none() && Some(*r) != song))
        .collect();
    if sections.is_empty() {
        return None;
    }
    sections.sort_by(|a, b| {
        a.time_range
            .start_seconds()
            .total_cmp(&b.time_range.start_seconds())
    });
    let from = sections
        .first()
        .map_or(0.0, |s| s.time_range.start_seconds());
    let bpm = daw.get_tempo_at(project.clone(), from);
    let (beats, unit) = daw.get_time_signature_at(project.clone(), from);
    let bar_at = |seconds: f64| daw.time_to_musical(project.clone(), seconds).0;
    let mut lines = vec![
        song.map_or_else(|| "Untitled".to_owned(), |s| s.name.trim().to_owned()),
        format!("{}bpm {beats}/{unit}", tempo_text(bpm)),
    ];
    for section in sections {
        let start = section.time_range.start_seconds();
        let end = section.time_range.end_seconds();
        // Bars from where it starts to where it ends, at least one.
        let bars = bar_at(end - 1e-6)
            .saturating_sub(bar_at(start + 1e-6))
            .saturating_add(1)
            .max(1);
        lines.push(section_line(&section.name, bars));
    }
    Some(lines.join("\n") + "\n")
}

/// A tempo as a chart writes it: whole when it is, else to a hundredth.
fn tempo_text(bpm: f64) -> String {
    if (bpm - bpm.round()).abs() < 1e-6 {
        format!("{bpm:.0}")
    } else {
        format!("{bpm:.2}")
    }
}

/// A section's line from its region's name.
///
/// Its kind (`VS`, `PRE-CH`), its bars, and its note when it has one in
/// quotes — the number or letter after the kind dropped, since a chart
/// numbers its sections itself (`VS 1B` is the first verse going on).
#[must_use]
pub fn section_line(name: &str, bars: i32) -> String {
    let name = name.trim();
    let (head, note) = match name.split_once('"') {
        Some((head, rest)) => (head.trim(), rest.split('"').next().map(str::trim)),
        None => (name, None),
    };
    let kind = head.split_whitespace().next().unwrap_or("SECTION");
    note.filter(|n| !n.is_empty()).map_or_else(
        || format!("{kind} {bars}"),
        |note| format!("{kind} {bars} \"{note}\""),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A region's name becomes its kind and bars, its quoted note kept,
    /// its number and letter left for the chart to work out.
    #[test]
    fn a_region_name_becomes_a_section_line() {
        assert_eq!(section_line("VS 1", 8), "VS 8");
        assert_eq!(section_line("CH 2B", 4), "CH 4");
        assert_eq!(section_line("PRE-CH 1", 4), "PRE-CH 4");
        assert_eq!(section_line("POST-CH", 2), "POST-CH 2");
        assert_eq!(
            section_line("BD C \"Double Kick\"", 4),
            "BD 4 \"Double Kick\""
        );
        assert_eq!(tempo_text(160.0), "160");
        assert_eq!(tempo_text(87.5), "87.50");
    }

    /// The song's sections, read from an engine, make a chart keyflow
    /// reads back with the same sections in the same order.
    #[test]
    fn a_songs_sections_make_a_chart() {
        use daw::service::{Regions as _, TempoMap as _};
        let runtime = tokio::runtime::Runtime::new().expect("a runtime");
        let _inside = runtime.enter();
        let daw = daw::standalone::Standalone::new();
        let guid = daw.seed_project(daw_proto::ProjectInfo {
            guid: "chart".into(),
            name: "chart".into(),
            path: String::new(),
        });
        let project = ProjectContext::Project(guid);
        // 120 bpm 4/4: a bar is two seconds.
        daw.set_default_tempo(project.clone(), 120.0)
            .expect("a tempo");
        let add = |name: &str, from: f64, to: f64, lane: u32| {
            let id = daw.add(project.clone(), from, to, name).expect("a region");
            daw.set_lane(project.clone(), id, Some(lane))
                .expect("its lane");
        };
        add("Song - Band", 0.0, 40.0, 0);
        add("IN", 0.0, 8.0, 1);
        add("VS 1", 8.0, 24.0, 1);
        add("CH 1 \"Big\"", 24.0, 40.0, 1);
        let text = of_song(&daw, &project).expect("a chart");
        assert_eq!(text, "Song - Band\n120bpm 4/4\nIN 4\nVS 8\nCH 8 \"Big\"\n");
        let chart = keyflow::parse(text.as_str()).expect("keyflow reads it");
        assert_eq!(chart.sections.len(), 3);
    }
}
