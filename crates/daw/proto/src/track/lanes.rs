//! Fixed-lane comping: comp areas and named comps.
//!
//! REAPER 7 keeps a track's comping state in three places, none of which
//! the SDK exposes through an accessor:
//!
//! - `LANEREC <record> <comp> <last_comp>` — which lane records, which
//!   lane is the comping lane (the *active comp*), and which one was
//!   before it. Each is a 0-based lane index, `-1` when unset.
//! - `ITEMLANES <n>` — the lane count the comp areas below were written
//!   against; always equal to the track's lane count in every REAPER-saved
//!   project in the corpus.
//! - `LINKEDLANE <start> <end> <source_lane> <comp_lane> -1 <fade_in>
//!   <fade_out>` — one **comp area**: the stretch `start..end` of
//!   `comp_lane` is taken from `source_lane`, crossfaded by the two fade
//!   lengths. The fifth field is `-1` in every project REAPER has written
//!   and its meaning is unknown; the loaders write it back as `-1`.
//!
//! A **named comp** is a lane with comp areas pointing at it (or the lane
//! `LANEREC` names as the comping lane). Its name is the lane's name —
//! `P_LANENAME:n` in the SDK, the n-th `LANENAME` token in the project
//! file — which is why REAPER's own comps come out as `C1`, `C2`, … and
//! why renaming a comp is [`Tracks::set_lane_name`](super::Tracks::set_lane_name).

use facet::Facet;

use crate::primitives::{Duration, PositionInSeconds};

/// One stretch of a comp lane, taken from a source lane.
///
/// The wire form of one `LINKEDLANE` line.
#[derive(Clone, Debug, PartialEq, Facet)]
pub struct CompArea {
    /// Where the area starts, project time.
    pub start: PositionInSeconds,
    /// Where the area ends, project time.
    pub end: PositionInSeconds,
    /// The lane the audio is taken from.
    pub source_lane: u32,
    /// The comp lane the audio is placed on.
    pub comp_lane: u32,
    /// Crossfade into the area.
    pub fade_in: Duration,
    /// Crossfade out of the area.
    pub fade_out: Duration,
}

/// A track's comping state: `LANEREC` plus its comp areas.
///
/// Read with [`Tracks::comping`](super::Tracks::comping); a separate
/// getter rather than a [`Track`](super::Track) field because on the
/// REAPER backend it costs a state-chunk read, which a bulk track read
/// must never pay.
#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct LaneComping {
    /// The lane new recordings land on, when one is pinned.
    pub record_lane: Option<u32>,
    /// The comping lane — the active comp.
    pub comp_lane: Option<u32>,
    /// The comping lane before the current one.
    pub last_comp_lane: Option<u32>,
    /// Every comp area on the track, in file order.
    pub areas: Vec<CompArea>,
}

impl LaneComping {
    /// The comps this state describes, one per comp lane, ascending.
    ///
    /// A lane is a comp when an area targets it or `LANEREC` names it
    /// (current or previous comping lane). `lane_names` is the track's
    /// lane-name list; a lane past its end is named the way REAPER names
    /// an unnamed lane, by its 1-based number.
    pub fn comps(&self, lane_names: &[String]) -> Vec<Comp> {
        let mut lanes: Vec<u32> = self.areas.iter().map(|a| a.comp_lane).collect();
        lanes.extend(self.comp_lane);
        lanes.extend(self.last_comp_lane);
        lanes.sort_unstable();
        lanes.dedup();
        lanes
            .into_iter()
            .map(|lane| Comp {
                lane,
                name: lane_names
                    .get(lane as usize)
                    .cloned()
                    .unwrap_or_else(|| (lane + 1).to_string()),
                is_active: self.comp_lane == Some(lane),
                areas: self
                    .areas
                    .iter()
                    .filter(|a| a.comp_lane == lane)
                    .cloned()
                    .collect(),
            })
            .collect()
    }
}

/// A named comp: a lane whose content is chosen from the other lanes.
#[derive(Clone, Debug, PartialEq, Facet)]
pub struct Comp {
    /// The comp's lane.
    pub lane: u32,
    /// The comp's name — the lane's name.
    pub name: String,
    /// Whether this is the comping lane (`LANEREC`'s second field).
    pub is_active: bool,
    /// The comp's areas, in file order.
    pub areas: Vec<CompArea>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(comp_lane: u32, source_lane: u32) -> CompArea {
        CompArea {
            start: PositionInSeconds::from_seconds(1.0),
            end: PositionInSeconds::from_seconds(2.0),
            source_lane,
            comp_lane,
            fade_in: Duration::from_seconds(0.01),
            fade_out: Duration::from_seconds(0.01),
        }
    }

    #[test]
    fn comps_are_the_lanes_areas_point_at_plus_the_comping_lanes() {
        let comping = LaneComping {
            record_lane: None,
            comp_lane: Some(0),
            last_comp_lane: Some(1),
            areas: vec![area(0, 4), area(1, 4), area(0, 3)],
        };
        let names = vec!["Custom Lane Name".to_string(), "C1".to_string()];
        let comps = comping.comps(&names);
        assert_eq!(comps.len(), 2);
        assert_eq!(comps[0].lane, 0);
        assert_eq!(comps[0].name, "Custom Lane Name");
        assert!(comps[0].is_active);
        assert_eq!(comps[0].areas.len(), 2);
        assert_eq!(comps[1].lane, 1);
        assert_eq!(comps[1].name, "C1");
        assert!(!comps[1].is_active);
        assert_eq!(comps[1].areas, vec![area(1, 4)]);
    }

    #[test]
    fn a_fresh_comp_with_no_areas_is_still_listed_by_its_number() {
        let comping = LaneComping {
            comp_lane: Some(2),
            ..Default::default()
        };
        let comps = comping.comps(&[]);
        assert_eq!(comps.len(), 1);
        assert_eq!(comps[0].name, "3");
        assert!(comps[0].areas.is_empty());
    }
}
