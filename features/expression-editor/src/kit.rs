//! Folding a recorded kit into the lanes a drum editor reads.
//!
//! Three kick mics and four snare mics are seven tracks, but while
//! editing they are one kick and one snare. This module decides which
//! role a track plays (`Kick` / `Snare` / `Toms` / `Other`), from its
//! place in the host's folder hierarchy first and its name second, and
//! rebuilds the workspace layout as four role lanes, bottom-up:
//! `Kick` at the bottom, `Snare` above it, `Toms` above that, `Other`
//! on top.
//!
//! Nothing here is drum-specific *in type*: a role is a label plus a
//! draw rule, and the fold takes `(guid, role)` pairs. A guitar
//! workspace later defines `DI` / `Amps` the same way.
//!
//! Spec: `features/expression-editor/spec/drum-mode.md` (`drums.lanes.*`).

use crate::rows::{DrumFamily, drum_family};
use crate::tracks::{Lane, LaneLayout, Workspace};

/// The role a lane plays in a folded group.
///
/// Order is draw order **top to bottom** — `Other` is drawn first
/// (top), `Kick` last (bottom) — so sorting lanes by role puts the kick
/// at the bottom where a drum editor expects it.
///
/// A role is a label plus two draw/detect rules, not a drum-specific
/// type: a guitar workspace defines `DI` / `Amps` over the same `Lane`
/// machinery by adding variants here, and the fold, the group rule and
/// the apply path need no changes.
// r[impl drums.scope.generic-groups]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LaneRole {
    /// Cymbals, hats, rooms, returns — everything that is not a drum the
    /// editor targets. Drawn summed, not a detection source.
    Other,
    /// One sub-row per tom.
    Toms,
    Snare,
    Kick,
}

impl LaneRole {
    /// The label drawn on the lane.
    pub fn label(self) -> &'static str {
        match self {
            LaneRole::Kick => "Kick",
            LaneRole::Snare => "Snare",
            LaneRole::Toms => "Toms",
            LaneRole::Other => "Other",
        }
    }

    /// Whether hits are detected on this lane and edited there.
    ///
    /// Kick, snare and toms are the drums the editor targets; the
    /// other lane — hats, cymbals, rooms, returns — is drawn summed
    /// for context and carries no hit list. A gesture on it selects
    /// the lane and nothing more.
    // r[impl flow.drums.editing.stack]
    pub fn detects(self) -> bool {
        !matches!(self, LaneRole::Other)
    }

    /// Whether the lane draws each member in its own sub-row (toms)
    /// rather than one summed waveform (kick, snare, other).
    pub fn splits_members(self) -> bool {
        matches!(self, LaneRole::Toms)
    }

    /// Whether hits are detected on this lane's signal by default.
    /// `Other` is never a source — it is rooms, hats and returns, where
    /// a "hit" means nothing the editor can act on.
    // r[impl drums.lanes.other]
    pub fn is_detection_source(self) -> bool {
        matches!(self, LaneRole::Kick | LaneRole::Snare | LaneRole::Toms)
    }

    /// The role's hue — the drum map's own kit palette, so a kick is
    /// red whether it is a MIDI row or an audio lane. `Other` takes the
    /// hat cyan: hats and cymbals are most of what lives there.
    pub fn color(self) -> &'static str {
        use crate::rows;
        match self {
            LaneRole::Kick => rows::DRUM_KICK_COLOR,
            LaneRole::Snare => rows::DRUM_SNARE_COLOR,
            LaneRole::Toms => rows::DRUM_TOM_COLOR,
            LaneRole::Other => rows::DRUM_HAT_COLOR,
        }
    }

    /// Every role, top to bottom.
    pub const ALL: [LaneRole; 4] = [
        LaneRole::Other,
        LaneRole::Toms,
        LaneRole::Snare,
        LaneRole::Kick,
    ];
}

/// Whether a folder name is the kit itself (`Drums`, `Drum`, `Kit`).
pub fn is_kit_folder(name: &str) -> bool {
    let n = name.trim().to_ascii_lowercase();
    matches!(
        n.as_str(),
        "drums" | "drum" | "kit" | "drum kit" | "drumkit"
    )
}

/// How much a folder looks like a real drum kit, for picking between
/// several that are all *named* like one.
///
/// A session routinely carries more than one folder called `Drums`: the
/// tracked kit, and a folder of reference stems or a printed drum mix. They
/// are indistinguishable by name — the real kit is obvious only from its
/// shape. Taking the first match opened the stems on a real session
/// (`set in stone`: a four-stem `Drums` folder sitting above the 20-track
/// kit), which then looked like "tom lanes are broken" because that folder
/// has no toms.
///
/// Ordered by `roles` first — covering kick *and* snare *and* toms is what
/// a kit is — then by role-claiming sub-folders, then by sheer size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct KitScore {
    /// Distinct targeted roles present (`Kick`, `Snare`, `Toms`). `Other`
    /// does not count: a folder of overheads is not a kit.
    pub roles: usize,
    /// Descendant folders that claim a role — `Kick/`, `Snare/`, `Toms/`.
    /// A tracked kit groups its mics; a stem folder is flat.
    pub sub_folders: usize,
    /// Leaf tracks beneath it, as the final tie-break.
    pub members: usize,
}

/// Score a candidate kit folder from its descendants, each `(name, is_folder)`.
#[must_use]
pub fn score_kit(descendants: &[(&str, bool)]) -> KitScore {
    let mut roles = [false; 3];
    let mut sub_folders = 0usize;
    let mut members = 0usize;
    for (name, is_folder) in descendants {
        let role = folder_role(name);
        if *is_folder {
            if role.is_some_and(|r| r != LaneRole::Other) {
                sub_folders = sub_folders.saturating_add(1);
            }
        } else {
            members = members.saturating_add(1);
        }
        match role {
            Some(LaneRole::Kick) => roles[0] = true,
            Some(LaneRole::Snare) => roles[1] = true,
            Some(LaneRole::Toms) => roles[2] = true,
            _ => {}
        }
    }
    KitScore {
        roles: roles.iter().filter(|r| **r).count(),
        sub_folders,
        members,
    }
}

/// The role a folder name claims for everything under it, if any.
///
/// `SUM` and similar bus names claim nothing: the folder *above* them
/// decides. Uses the drum map's classifier so the two never disagree
/// about what a "Tom" is.
pub fn folder_role(name: &str) -> Option<LaneRole> {
    match drum_family(name) {
        DrumFamily::Kick => Some(LaneRole::Kick),
        DrumFamily::Snare => Some(LaneRole::Snare),
        DrumFamily::Tom => Some(LaneRole::Toms),
        DrumFamily::HiHat | DrumFamily::Cymbal | DrumFamily::Ride => Some(LaneRole::Other),
        DrumFamily::Other => {
            let n = name.trim().to_ascii_lowercase();
            let head = n.split([' ', '-', '_']).next().unwrap_or("");
            // Words the drum map does not know but a session uses for
            // the whole non-drum remainder.
            if n.contains("overhead")
                || n.contains("room")
                || n.contains("cymbal")
                || n.contains("verb")
                || matches!(head, "oh" | "ohs" | "amb" | "ambience")
            {
                Some(LaneRole::Other)
            } else {
                None
            }
        }
    }
}

/// The role of a track, given its name and its folders **nearest
/// first** (`["SUM", "Kick", "Drums"]` for `Kick/SUM/In`).
///
/// Hierarchy wins over name: a track called `Trig` under the `Snare`
/// folder is a snare mic, and a track called `In` says nothing on its
/// own. The kit folder itself claims no role, so a `Hi-Hat` track
/// directly under `Drums` falls through to its name.
// r[impl drums.lanes.roles]
pub fn kit_role(name: &str, folders_nearest_first: &[&str]) -> LaneRole {
    for f in folders_nearest_first {
        if is_kit_folder(f) {
            break;
        }
        if let Some(role) = folder_role(f) {
            return role;
        }
    }
    folder_role(name).unwrap_or(LaneRole::Other)
}

/// Whether a member is parked — named `Unused` (any case) — and so drawn
/// faded and left out of detection.
pub fn is_unused_name(name: &str) -> bool {
    name.to_ascii_lowercase().contains("unused")
}

/// Whether a track is a trigger — a `Trig` / `Trigger` token in its name.
///
/// Matched as a whole token, not a substring: a mic called `Trigate` is
/// not a trigger, and the token is what the FTS naming actually writes
/// (`T3 Trig`, `Kick Trigger`).
pub fn is_trigger_name(name: &str) -> bool {
    name.to_ascii_lowercase()
        .split([' ', '-', '_'])
        .any(|t| t == "trig" || t == "trigger")
}

/// Which tom a track belongs to, `T1`–`T4` → `1`–`4`.
///
/// A trigger shares its tom's number (`T3 Trig` is tom 3), which is what
/// lets a trigger be drawn over the tom it triggers rather than beside
/// it. Any token may carry the number, because the mic name comes first
/// in some sessions and last in others (`T2 Close`, `Close T2`).
pub fn tom_number(name: &str) -> Option<u8> {
    name.to_ascii_lowercase()
        .split([' ', '-', '_'])
        .find_map(|t| match t {
            "t1" => Some(1),
            "t2" => Some(2),
            "t3" => Some(3),
            "t4" => Some(4),
            _ => None,
        })
}

/// Group a split lane's members into the ones that own a sub-row and the
/// triggers that ride on top of one, as indices into `names`.
///
/// A trigger is not another tom — it is the same drum, sensed a second
/// way — so `T3 Trig` belongs in `T3`'s row, drawn over it. Given a row
/// each, four toms with triggers read as an eight-piece kit and every
/// row is half the height it should be.
///
/// A trigger keeps its own row when nothing claims it: no tom number
/// (`Trig`, ambiguous), or a number with no matching tom (`T4 Trig`
/// where `T4` was never recorded). Folding those away silently would
/// hide a track that is really there.
// r[impl drums.lanes.trigger-overlay]
pub fn trigger_sub_rows(names: &[&str]) -> Vec<(usize, Vec<usize>)> {
    let mut rows: Vec<(usize, Vec<usize>)> = (0..names.len())
        .filter(|&i| !is_trigger_name(names[i]))
        .map(|i| (i, Vec::new()))
        .collect();

    for t in (0..names.len()).filter(|&i| is_trigger_name(names[i])) {
        let host = tom_number(names[t]).and_then(|n| {
            rows.iter_mut()
                .find(|(h, _)| tom_number(names[*h]) == Some(n))
        });
        match host {
            Some((_, overlays)) => overlays.push(t),
            None => rows.push((t, Vec::new())),
        }
    }
    rows
}

/// How much more a trigger counts than an acoustic mic in the same
/// detection unit.
///
/// A trigger is a contact mic on the drum: almost no bleed, almost no
/// decay, a near-vertical attack — better evidence of *when* the drum
/// was hit than any acoustic mic. But it is not infallible: a trigger
/// can drop out, double-fire on a rim shot, or sit slightly out of
/// alignment. So the mics keep a vote rather than sitting the detection
/// out; the trigger just outweighs them. At 4:1 one trigger outweighs
/// any realistic number of mics on one drum while still being pulled by
/// them where they agree — and where the trigger misses a hit entirely,
/// the mics can still put one there.
const TRIGGER_WEIGHT: f64 = 4.0;

/// The signals a lane detects on: groups of `(member index, weight)`
/// into `names`, each group blended into one signal the detector runs
/// over. Weights within a unit sum to 1, so every unit's signal comes
/// out at a comparable level regardless of how many mics it has.
///
/// Two rules, and both exist because the alternative loses hits.
///
/// **A trigger is weighted over the mics it shares a drum with** — see
/// [`TRIGGER_WEIGHT`]. It dominates the onset without silencing the
/// mics that corroborate it.
///
/// **Toms detect per tom, not as a lane.** A summed toms signal answers
/// "a tom was hit" when the question is "which one". With four triggers
/// the toms are four clean independent signals, so they are four units.
/// Un-numbered tom tracks share a unit of their own rather than being
/// dropped.
///
/// `Unused` members are excluded everywhere (r[drums.lanes.toms-split]),
/// and a lane that is not a detection source yields nothing.
// r[impl drums.group.detection-source]
pub fn detection_units(role: LaneRole, names: &[&str]) -> Vec<Vec<(usize, f64)>> {
    if !role.is_detection_source() {
        return Vec::new();
    }
    let live: Vec<usize> = (0..names.len())
        .filter(|&i| !is_unused_name(names[i]))
        .collect();

    // Toms split by which tom; every other role is one unit.
    let units: Vec<Vec<usize>> = if role.splits_members() {
        let mut keys: Vec<Option<u8>> = Vec::new();
        for &i in &live {
            let k = tom_number(names[i]);
            if !keys.contains(&k) {
                keys.push(k);
            }
        }
        keys.into_iter()
            .map(|k| {
                live.iter()
                    .copied()
                    .filter(|&i| tom_number(names[i]) == k)
                    .collect()
            })
            .collect()
    } else {
        vec![live]
    };

    units
        .into_iter()
        .filter(|unit| !unit.is_empty())
        .map(|unit| {
            let weight = |i: usize| {
                if is_trigger_name(names[i]) {
                    TRIGGER_WEIGHT
                } else {
                    1.0
                }
            };
            let total: f64 = unit.iter().map(|&i| weight(i)).sum();
            unit.into_iter().map(|i| (i, weight(i) / total)).collect()
        })
        .collect()
}

impl Workspace {
    /// Rebuild the layout as role lanes over `members` (`(guid, role)`),
    /// top to bottom `Other`, `Toms`, `Snare`, `Kick`; roles with no
    /// members get no lane. Member order within a lane follows
    /// `members`. Tracks not mentioned keep a lane of their own below.
    ///
    /// The fold is an inference, not an arrangement: it does not mark the
    /// layout arranged, so a later hand merge/split still wins and is the
    /// thing that gets persisted.
    // r[impl drums.lanes.roles]
    // r[impl drums.lanes.heights]
    pub fn fold_roles(&mut self, members: &[(String, LaneRole)]) {
        let mut layout = LaneLayout::default();
        for role in LaneRole::ALL {
            let guids: Vec<String> = members
                .iter()
                .filter(|(g, r)| *r == role && self.track_by_guid(g).is_some())
                .map(|(g, _)| g.clone())
                .collect();
            if guids.is_empty() {
                continue;
            }
            // Equal weight per role lane: the kick, the snare, the toms
            // and the rest share the stack evenly, and the toms divide
            // their share among themselves.
            layout.push(Lane::role(role, guids, 1.0));
        }
        for t in self.tracks() {
            if layout.lane_of(&t.guid).is_none() {
                layout.push(Lane::single(t.guid.clone(), t.mode.stack_weight()));
            }
        }
        *self.layout_mut() = layout;
    }

    /// The role lane a track is in, if its lane has one.
    pub fn role_of(&self, guid: &str) -> Option<LaneRole> {
        self.layout()
            .lane_of(guid)
            .and_then(|i| self.layout().lane(i))
            .and_then(|l| l.role)
    }

    /// The guids in the lane with `role`, in draw order.
    pub fn role_members(&self, role: LaneRole) -> Vec<String> {
        self.layout()
            .lanes()
            .iter()
            .find(|l| l.role == Some(role))
            .map(|l| l.tracks.clone())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // r[verify drums.lanes.trigger-overlay]
    #[test]
    fn a_trigger_is_a_whole_token_not_a_substring() {
        assert!(is_trigger_name("T3 Trig"));
        assert!(is_trigger_name("Kick-Trigger"));
        assert!(is_trigger_name("S_trig"));
        assert!(is_trigger_name("Trig"));
        // A mic whose name merely contains the letters is not a trigger.
        assert!(!is_trigger_name("Trigate"));
        assert!(!is_trigger_name("T1"));
    }

    // r[verify drums.lanes.trigger-overlay]
    #[test]
    fn a_trigger_carries_the_number_of_the_tom_it_triggers() {
        // This pairing is the whole point: it is what puts `T3 Trig` in
        // `T3`'s sub-row instead of a row of its own.
        assert_eq!(tom_number("T3 Trig"), tom_number("T3"));
        assert_eq!(tom_number("T1"), Some(1));
        assert_eq!(tom_number("T4 Trig"), Some(4));
        // The mic name comes first in some sessions and last in others.
        assert_eq!(tom_number("Close T2"), Some(2));
        assert_eq!(tom_number("T2_Close"), Some(2));
        // Nothing to pair with.
        assert_eq!(tom_number("Trig"), None);
        assert_eq!(tom_number("Snare Top"), None);
        assert_eq!(tom_number("T5"), None);
    }

    // r[verify drums.lanes.trigger-overlay]
    #[test]
    fn four_toms_with_triggers_are_four_rows_not_eight() {
        let rows = trigger_sub_rows(&[
            "T1", "T2", "T3", "T4", "T1 Trig", "T2 Trig", "T3 Trig", "T4 Trig",
        ]);
        assert_eq!(rows.len(), 4, "one row per tom, triggers riding along");
        // Each tom carries exactly its own trigger.
        assert_eq!(rows[0], (0, vec![4]));
        assert_eq!(rows[2], (2, vec![6]));
    }

    // r[verify drums.lanes.trigger-overlay]
    #[test]
    fn an_unclaimed_trigger_keeps_its_own_row() {
        // T4 was never recorded, and a bare `Trig` says which drum but
        // not which tom. Neither may vanish.
        let rows = trigger_sub_rows(&["T1", "T1 Trig", "T4 Trig", "Trig"]);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], (0, vec![1]), "T1 keeps its trigger");
        assert_eq!(rows[1], (2, vec![]), "T4 Trig has no tom to ride");
        assert_eq!(rows[2], (3, vec![]), "a bare Trig is ambiguous");
    }

    // r[verify drums.lanes.trigger-overlay]
    #[test]
    fn a_tom_with_two_mics_keeps_both_rows() {
        // Overlaying is only for triggers; two mics on one tom are two
        // captures worth seeing side by side.
        let rows = trigger_sub_rows(&["T1 Top", "T1 Bottom", "T1 Trig"]);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], (0, vec![2]), "the trigger rides the first match");
        assert_eq!(rows[1], (1, vec![]));
    }

    /// Just the member indices of each unit, for the tests that care
    /// about grouping rather than weighting.
    fn members_of(units: &[Vec<(usize, f64)>]) -> Vec<Vec<usize>> {
        units
            .iter()
            .map(|u| u.iter().map(|&(i, _)| i).collect())
            .collect()
    }

    // r[verify drums.group.detection-source]
    #[test]
    fn a_trigger_outweighs_the_mics_without_silencing_them() {
        let u = detection_units(LaneRole::Kick, &["In", "Out", "Trig"]);
        assert_eq!(members_of(&u), vec![vec![0, 1, 2]], "the mics still vote");

        // 4:1 over each mic, and the unit normalised to 1.
        let w: Vec<f64> = u[0].iter().map(|&(_, w)| w).collect();
        assert!((w[2] / w[0] - 4.0).abs() < 1e-9, "trigger is 4x a mic");
        assert!((w[0] - w[1]).abs() < 1e-9, "the two mics are equal");
        assert!((w.iter().sum::<f64>() - 1.0).abs() < 1e-9, "normalised");
        // The trigger dominates but does not own the unit outright.
        assert!(w[2] > 0.5 && w[2] < 1.0);
    }

    // r[verify drums.group.detection-source]
    #[test]
    fn without_a_trigger_the_mics_weigh_equally() {
        let u = detection_units(LaneRole::Snare, &["Top", "Bottom"]);
        assert_eq!(members_of(&u), vec![vec![0, 1]]);
        assert_eq!(u[0], vec![(0, 0.5), (1, 0.5)]);
    }

    // r[verify drums.group.detection-source]
    #[test]
    fn toms_detect_one_unit_per_tom() {
        // Four toms with triggers are four independent signals — a
        // summed lane could only say "a tom was hit", not which.
        let u = detection_units(
            LaneRole::Toms,
            &[
                "T1", "T2", "T3", "T4", "T1 Trig", "T2 Trig", "T3 Trig", "T4 Trig",
            ],
        );
        assert_eq!(
            members_of(&u),
            vec![vec![0, 4], vec![1, 5], vec![2, 6], vec![3, 7]],
            "each tom with its own trigger, and nothing crossing units"
        );
    }

    // r[verify drums.group.detection-source]
    #[test]
    fn a_tom_without_a_trigger_falls_back_to_its_mics() {
        // Mixed kit: T1 triggered, T2 not. Each still detects alone.
        let u = detection_units(LaneRole::Toms, &["T1", "T1 Trig", "T2 Close", "T2 Far"]);
        assert_eq!(members_of(&u), vec![vec![0, 1], vec![2, 3]]);
        assert_eq!(u[1], vec![(2, 0.5), (3, 0.5)], "no trigger, equal mics");
    }

    // r[verify drums.group.detection-source]
    #[test]
    fn unused_members_never_detect() {
        let u = detection_units(LaneRole::Toms, &["T1", "T2 Unused"]);
        assert_eq!(members_of(&u), vec![vec![0]], "the parked tom is out");
        // And a lane of nothing but parked members yields no signal at
        // all rather than an empty one the detector would chew on.
        assert!(detection_units(LaneRole::Kick, &["Unused"]).is_empty());
    }

    // r[verify drums.lanes.other]
    #[test]
    fn the_other_lane_is_never_a_detection_source() {
        assert!(detection_units(LaneRole::Other, &["OH L", "Room"]).is_empty());
    }

    // r[verify drums.lanes.roles]
    #[test]
    fn roles_come_from_the_hierarchy_first() {
        assert_eq!(kit_role("In", &["SUM", "Kick", "Drums"]), LaneRole::Kick);
        assert_eq!(
            kit_role("Trig", &["SUM", "Snare", "Drums"]),
            LaneRole::Snare
        );
        assert_eq!(kit_role("Verb", &["Snare", "Drums"]), LaneRole::Snare);
        assert_eq!(kit_role("T2", &["Toms", "Drums"]), LaneRole::Toms);
        assert_eq!(kit_role("T1 - Unused", &["Toms", "Drums"]), LaneRole::Toms);
        // Directly under the kit: the name decides.
        assert_eq!(kit_role("Hi-Hat", &["Drums"]), LaneRole::Other);
        assert_eq!(kit_role("Overheads", &["Drums"]), LaneRole::Other);
        assert_eq!(kit_role("Room", &["Drums"]), LaneRole::Other);
        assert_eq!(kit_role("Kick In", &[]), LaneRole::Kick);
        assert_eq!(kit_role("Snare Top", &[]), LaneRole::Snare);
        assert_eq!(kit_role("Something", &[]), LaneRole::Other);
        assert!(is_unused_name("T1 - Unused"));
    }

    #[test]
    fn roles_order_bottom_up() {
        let mut v = vec![
            LaneRole::Kick,
            LaneRole::Other,
            LaneRole::Toms,
            LaneRole::Snare,
        ];
        v.sort();
        assert_eq!(v, LaneRole::ALL);
    }
}
