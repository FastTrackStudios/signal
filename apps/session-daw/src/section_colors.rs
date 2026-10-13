//! A song's sections in keyflow's colours: each region on the Sections
//! lane coloured by the kind of section its name says it is (`VS 1` a
//! verse, `PRE-CH 2` a pre-chorus) — the colours a chart engraves its
//! sections in, so the ruler and the chart read alike.

use daw::service::{ProjectContext, Regions};

/// Colour `project`'s sections by kind; a region whose name names no
/// kind, or that has its colour already, is left as it is. Returns how
/// many were coloured.
///
/// # Errors
/// The engine refused a colour.
pub fn apply<D: Regions>(daw: &D, project: &ProjectContext) -> daw_proto::DawResult<usize> {
    colour(daw, project, true)
}

/// How many of `project`'s sections [`apply`] would colour.
#[must_use]
pub fn pending<D: Regions>(daw: &D, project: &ProjectContext) -> usize {
    colour(daw, project, false).unwrap_or(0)
}

fn colour<D: Regions>(daw: &D, project: &ProjectContext, set: bool) -> daw_proto::DawResult<usize> {
    let sections = session::ruler_lanes::CoreLane::Sections.lane_index();
    let mut coloured = 0_usize;
    for region in Regions::all(daw, project.clone()) {
        if region.lane.is_some_and(|lane| lane != sections) {
            continue;
        }
        let (Some(id), Some(kind)) = (
            region.id,
            session::keyflow::actions::parse_region_section_type(&region.name),
        ) else {
            continue;
        };
        let colour = session::keyflow::actions::section_type_color(&kind);
        if region.color != Some(rgb_of_native(colour)) {
            if set {
                Regions::set_color(daw, project.clone(), id, colour)?;
            }
            coloured = coloured.saturating_add(1);
        }
    }
    Ok(coloured)
}

/// REAPER's native `0x01BBGGRR` as the `0xRRGGBB` an engine reports a
/// region's colour in.
const fn rgb_of_native(native: u32) -> u32 {
    let (red, green, blue) = (native & 0xFF, (native >> 8) & 0xFF, (native >> 16) & 0xFF);
    (red << 16) | (green << 8) | blue
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each section takes its kind's colour — two verses alike, a chorus
    /// another — and the song's own region, on its lane, is left.
    #[test]
    fn sections_take_their_kinds_colours() {
        let runtime = tokio::runtime::Runtime::new().expect("a runtime");
        let _inside = runtime.enter();
        let daw = daw::standalone::Standalone::new();
        let guid = daw.seed_project(daw_proto::ProjectInfo {
            guid: "colours".into(),
            name: "colours".into(),
            path: String::new(),
        });
        let project = ProjectContext::Project(guid);
        let add = |name: &str, from: f64, lane: u32| {
            let id = Regions::add(&daw, project.clone(), from, from + 1.0, name).expect("a region");
            Regions::set_lane(&daw, project.clone(), id, Some(lane)).expect("its lane");
        };
        add("Song", 0.0, 0);
        add("VS 1", 0.0, 1);
        add("CH 1", 1.0, 1);
        add("VS 2", 2.0, 1);
        assert_eq!(pending(&daw, &project), 3);
        assert_eq!(apply(&daw, &project).ok(), Some(3));
        assert_eq!(apply(&daw, &project).ok(), Some(0), "coloured already");
        let colour = |name: &str| {
            Regions::all(&daw, project.clone())
                .into_iter()
                .find(|r| r.name == name)
                .and_then(|r| r.color)
        };
        assert_eq!(colour("VS 1"), colour("VS 2"));
        assert_ne!(colour("VS 1"), colour("CH 1"));
        assert_eq!(colour("CH 1"), Some(0x003b_82f6), "keyflow's chorus blue");
        assert_eq!(colour("Song"), None, "the song's region is not a section");
    }

    /// A chorus coloured with its bytes the wrong way round (orange where
    /// keyflow's is blue) is coloured again.
    #[test]
    fn a_swapped_colour_is_put_right() {
        let runtime = tokio::runtime::Runtime::new().expect("a runtime");
        let _inside = runtime.enter();
        let daw = daw::standalone::Standalone::new();
        let guid = daw.seed_project(daw_proto::ProjectInfo {
            guid: "swapped".into(),
            name: "swapped".into(),
            path: String::new(),
        });
        let project = ProjectContext::Project(guid);
        let id = Regions::add(&daw, project.clone(), 0.0, 1.0, "CH 1").expect("a region");
        Regions::set_lane(&daw, project.clone(), id, Some(1)).expect("its lane");
        Regions::set_color(&daw, project.clone(), id, 0x00f6_823b).expect("a colour");
        let read = Regions::all(&daw, project.clone())[0].color;
        assert_eq!(pending(&daw, &project), 1, "read back as {read:x?}");
    }
}
