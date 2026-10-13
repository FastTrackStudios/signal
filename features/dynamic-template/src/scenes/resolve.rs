//! The one function: a scene and a session in, a row list out.
//!
//! Everything a scene means is decided here and nowhere else. The window
//! renders the row list as it is; the REAPER applier walks the same list
//! and writes show, fold and size onto real tracks. Two appliers, one
//! resolve — which is the whole reason the row list is the output rather
//! than a pile of per-surface booleans.

use std::collections::HashMap;

use super::facts::Fact;
use super::language::Language;
use super::selector::{Rank, Role, Selector};
use super::types::{Fold, GroupBy, Resolved, Scene, Size, Surface};
use crate::golden_session::Kind;

/// What a row is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A real track.
    Track(String),
    /// A synthesised header for one performer — `flow.scenes.performer-order`.
    ///
    /// Emitted by `group_by: performer`, which is #51. The variant is
    /// here so that ticket adds the emitting and nothing else, and so
    /// that every consumer written today already skips a row it cannot
    /// apply to a track.
    PerformerHeader(String),
}

/// One row of a resolved scene.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// What the row is about.
    pub target: Target,
    /// How deep it sits, 0 at the top.
    pub depth: u32,
    /// How much room it gets.
    pub size: Size,
    /// What happens to the rows beneath it.
    pub fold: Fold,
}

impl Row {
    /// The track a row is about, when it is about one.
    #[must_use]
    pub const fn guid(&self) -> Option<&str> {
        match &self.target {
            Target::Track(guid) => Some(guid.as_str()),
            Target::PerformerHeader(_) => None,
        }
    }
}

/// The performer group a track with neither dimension nor send falls
/// into (`flow.scenes.performer-identity`, decision #31): shown, never
/// dropped, so the view says what the Patch List says.
pub const UNASSIGNED: &str = "Unassigned";

/// How performer header rows are ordered under an instrument folder.
///
/// Compares two performer names (`UNASSIGNED` for a track with neither
/// dimension nor send). `None` — the only ordering the engine can do
/// until the patch list (#61) supplies one — orders by first appearance
/// in the track list instead, which [`resolve`] does without ever
/// calling a comparator. The patch list will pass `Some(&cmp)` ranking
/// by its own performer order.
pub type PerformerOrder<'a> = dyn Fn(&str, &str) -> std::cmp::Ordering + 'a;

/// Resolve a scene against a session, for one surface.
///
/// Rules are evaluated in order and the **last** one that matches a
/// track wins; the scene's `default` covers every track no rule matched.
/// A folder folded `Collapsed` or `Hidden` takes its rows with it, and a
/// `Hidden` folder takes its own row too.
///
/// Facts must arrive in project order — the row list is in that order,
/// and the fold walk depends on it.
///
/// `order` is `flow.scenes.performer-order`'s injected comparator: read
/// only when `scene.group_by` is [`GroupBy::Performer`], and even then
/// only to break the tie between the performers found — the project's
/// own track order (`facts`) is never touched.
///
/// # Engine invariants
///
/// Two things are true of every scene and cannot be said otherwise in
/// the table, so they are enforced here:
///
/// - **A stereo pair's half is always a rail.** The pair's folder
///   carries the processing and the width; its L and its R are one
///   signal seen twice, and a scene that opened them would be showing
///   the same thing at four times the cost.
/// - **A fold applies down the tree.** Rules say what happens to a
///   folder; what happens to its contents follows from that rather than
///   from a second rule nobody would remember to write.
///
/// `active_language` (`flow.vocals.language.active`) feeds the common
/// prelude the same way `mode` does: `None` hides nothing, `Some(lang)`
/// hides every source whose own language is neither `lang` nor
/// [`Language::All`] — see `table::prelude`.
// r[impl flow.vocals.language.active]
#[must_use]
pub fn resolve(
    scene: &Scene,
    facts: &[Fact],
    surface: Surface,
    mode: Option<&str>,
    active_language: Option<Language>,
    order: Option<&PerformerOrder<'_>>,
) -> Vec<Row> {
    // What is in the mixer only is never an arrangement row.
    let kept: Vec<Fact>;
    let facts = if surface == Surface::Arrange && facts.iter().any(|f| f.mixer_only) {
        kept = facts.iter().filter(|f| !f.mixer_only).cloned().collect();
        kept.as_slice()
    } else {
        facts
    };
    let effects = effects_for(scene, facts, surface, mode, active_language);
    let rows = walk(facts, &effects);
    if scene.group_by == GroupBy::Performer {
        group_by_performer(facts, &rows, order)
    } else {
        rows
    }
}

/// The plain walk: every fact that survives its fold, in project order,
/// each a `Target::Track` row. `resolve` regroups this by performer
/// afterwards when the scene asks for it.
fn walk(facts: &[Fact], effects: &[Resolved]) -> Vec<Row> {
    let mut out: Vec<Row> = Vec::with_capacity(facts.len());
    // The depth of the shallowest folded folder we are inside, if any.
    let mut folded: Option<u32> = None;
    // The depths of the lifted folders we are inside: each lifts what it
    // holds one level.
    let mut lifted: Vec<u32> = Vec::new();
    for (fact, effect) in facts.iter().zip(effects) {
        if let Some(at) = folded {
            if fact.depth > at {
                continue;
            }
            folded = None;
        }
        while lifted.last().is_some_and(|at| fact.depth <= *at) {
            lifted.pop();
        }
        if effect.fold == Fold::Lifted && fact.is_folder {
            lifted.push(fact.depth);
            continue;
        }
        let depth = fact
            .depth
            .saturating_sub(u32::try_from(lifted.len()).unwrap_or(u32::MAX));
        if effect.fold == Fold::Hidden {
            // A hidden leaf drops its own row; a hidden folder drops
            // everything under it as well.
            if fact.is_folder {
                folded = Some(fact.depth);
            }
            continue;
        }
        if fact.is_folder && !effect.fold.keeps_children() {
            folded = Some(fact.depth);
        }
        let size = if fact.pair_half && !fact.is_folder {
            Size::Minimum
        } else {
            effect.size
        };
        out.push(Row {
            target: Target::Track(fact.guid.clone()),
            depth,
            size,
            fold: effect.fold,
        });
    }
    out
}

/// `flow.scenes.performer-order`: under each visible instrument folder
/// (`Kind::Group` — `Drum Kit`, `Electric`, …), replace its direct
/// children with one `PerformerHeader` row per performer found among
/// them, that performer's children nested beneath, each shifted one
/// level deeper to make room for the header. The folder's own row, and
/// everything outside it, is untouched: this is a view over `rows`, not
/// a second pass over the project.
///
/// A child's performer is the first one found among its own subtree —
/// every leaf a rig sends is one performer's, so any leaf will do. A
/// child with none is `UNASSIGNED`, grouped and shown rather than
/// dropped, exactly as the Patch List shows an unassigned track rather
/// than hiding it.
///
/// r[impl flow.scenes.performer-order]
fn group_by_performer(
    facts: &[Fact],
    rows: &[Row],
    order: Option<&PerformerOrder<'_>>,
) -> Vec<Row> {
    let by_guid: HashMap<&str, &Fact> = facts.iter().map(|f| (f.guid.as_str(), f)).collect();
    let is_instrument_folder = |row: &Row| -> bool {
        row.guid()
            .and_then(|g| by_guid.get(g))
            .is_some_and(|f| f.is_folder && f.kind == Some(Kind::Group))
    };
    let performer_of_block = |block: &[Row]| -> Option<String> {
        block.iter().find_map(|row| {
            row.guid()
                .and_then(|g| by_guid.get(g))
                .and_then(|f| f.performer.clone())
        })
    };

    let mut out = Vec::with_capacity(rows.len());
    let mut i = 0_usize;
    while let Some(row) = rows.get(i) {
        if !is_instrument_folder(row) {
            out.push(row.clone());
            i = i.saturating_add(1);
            continue;
        }
        let folder_depth = row.depth;
        out.push(row.clone());
        i = i.saturating_add(1);

        // The folder's direct children, each with its whole subtree, as
        // contiguous blocks — a block is everything from one child down
        // to (not including) the next row at the child's own depth.
        let mut blocks: Vec<(Option<String>, Vec<Row>)> = Vec::new();
        while rows.get(i).is_some_and(|r| r.depth > folder_depth) {
            let child_depth = rows.get(i).map_or(folder_depth, |r| r.depth);
            let start = i;
            i = i.saturating_add(1);
            while rows.get(i).is_some_and(|r| r.depth > child_depth) {
                i = i.saturating_add(1);
            }
            let block: Vec<Row> = rows.get(start..i).map_or_else(Vec::new, <[Row]>::to_vec);
            let performer = performer_of_block(&block);
            blocks.push((performer, block));
        }
        // Group the blocks by performer, keeping the order their first
        // block appeared in — first appearance, until #61's patch list
        // supplies `order`.
        let mut seen: Vec<String> = Vec::new();
        let mut groups: HashMap<String, Vec<Vec<Row>>> = HashMap::new();
        for (performer, block) in blocks {
            let key = performer.unwrap_or_else(|| UNASSIGNED.to_owned());
            if !seen.contains(&key) {
                seen.push(key.clone());
            }
            groups.entry(key).or_default().push(block);
        }
        if let Some(cmp) = order {
            seen.sort_by(|a, b| cmp(a, b));
        }
        let header_depth = folder_depth.saturating_add(1);
        for key in seen {
            out.push(Row {
                target: Target::PerformerHeader(key.clone()),
                depth: header_depth,
                size: Size::Compact,
                fold: Fold::Open,
            });
            for block in groups.remove(&key).unwrap_or_default() {
                for row in block {
                    out.push(Row {
                        depth: row.depth.saturating_add(1),
                        ..row
                    });
                }
            }
        }
    }
    out
}

/// The effect each track ends up with, before the fold walk drops rows.
///
/// The common prelude goes first, so a scene that wants the Guide folder
/// open only has to say so — it is data, and it is overridable.
fn effects_for(
    scene: &Scene,
    facts: &[Fact],
    surface: Surface,
    mode: Option<&str>,
    active_language: Option<Language>,
) -> Vec<Resolved> {
    let mut effects = vec![Resolved::from(scene.default); facts.len()];
    let prelude = super::table::prelude(mode, active_language);
    for rule in prelude.iter().chain(&scene.rules) {
        let effect = rule.for_surface(surface);
        for i in matching(&rule.selector, facts) {
            if let Some(slot) = effects.get_mut(i) {
                *slot = effect.over(*slot);
            }
        }
    }
    effects
}

/// The indices of every fact a selector matches.
fn matching(selector: &Selector, facts: &[Fact]) -> Vec<usize> {
    let base: Vec<usize> = facts
        .iter()
        .enumerate()
        .filter(|(_, fact)| matches_but_for_rank(selector, fact))
        .map(|(i, _)| i)
        .collect();
    match selector.rank {
        Rank::All => base,
        Rank::TopmostPerInstrument => topmost_per_instrument(&base, facts),
    }
}

fn matches_but_for_rank(selector: &Selector, fact: &Fact) -> bool {
    let role_ok = match selector.role {
        Role::Any => true,
        Role::Bus => fact.is_folder,
        Role::Leaf => !fact.is_folder,
    };
    if !role_ok {
        return false;
    }
    if !path_starts_with(&fact.path, &selector.group) {
        return false;
    }
    if let Some(kind) = &selector.kind {
        if !fact
            .kind
            .is_some_and(|k| k.as_str().eq_ignore_ascii_case(kind))
        {
            return false;
        }
    }
    if selector.depth.is_some_and(|depth| fact.depth != depth) {
        return false;
    }
    if let Some(language) = selector.language {
        if fact.language != Some(language) {
            return false;
        }
    }
    let dimensions = [
        (&selector.performer, fact.performer.as_deref()),
        (&selector.layer, fact.layer.as_deref()),
        (&selector.channel, fact.channel.as_deref()),
        (&selector.multi_mic, fact.multi_mic.as_deref()),
        (&selector.arrangement, fact.arrangement.as_deref()),
        (&selector.name, Some(fact.name.as_str())),
    ];
    dimensions.into_iter().all(|(want, have)| {
        want.as_ref()
            .is_none_or(|want| have.is_some_and(|have| have.eq_ignore_ascii_case(want)))
    })
}

/// Whether a taxonomy path begins with a selector's group path.
fn path_starts_with(path: &[super::facts::Segment], group: &[String]) -> bool {
    if group.len() > path.len() {
        return false;
    }
    group
        .iter()
        .zip(path)
        .all(|(want, segment)| segment.answers_to(want))
}

/// Keep only the lowest-index track per taxonomy group.
fn topmost_per_instrument(base: &[usize], facts: &[Fact]) -> Vec<usize> {
    let mut best: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for &i in base {
        let Some(fact) = facts.get(i) else { continue };
        let key = fact.group_key();
        match best.get(&key) {
            Some(&j) if facts.get(j).is_some_and(|f| f.index <= fact.index) => {}
            _ => {
                best.insert(key, i);
            }
        }
    }
    let mut out: Vec<usize> = best.into_values().collect();
    out.sort_unstable();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenes::facts::Segment;
    use crate::scenes::types::{Audience, Effect, GroupBy, Rule};

    fn scene(default: Effect, rules: Vec<Rule>) -> Scene {
        Scene {
            name: "Test".to_owned(),
            slug: "test".to_owned(),
            short: "Test".to_owned(),
            instrument: "drums".to_owned(),
            modes: Vec::new(),
            audience: Audience::Engineer,
            group_by: GroupBy::Arrangement,
            spec: Vec::new(),
            default,
            rules,
        }
    }

    fn kit() -> Vec<Fact> {
        let drums = Segment::named("Drum Kit")
            .of(Kind::Group)
            .standing_for("Drum Kit");
        let kick = Segment::named("Kick").of(Kind::Piece).standing_for("Kick");
        vec![
            Fact::folder("kit", "Drum Kit", 0, 0)
                .of(Kind::Group)
                .at(vec![drums.clone()]),
            Fact::folder("kick", "Kick", 1, 1)
                .of(Kind::Piece)
                .at(vec![drums.clone(), kick.clone()]),
            Fact::leaf("in", "In", 2, 2)
                .of(Kind::Source)
                .at(vec![drums.clone(), kick.clone()]),
            Fact::leaf("out", "Out", 3, 2)
                .of(Kind::Source)
                .at(vec![drums, kick]),
        ]
    }

    /// The rule that comes last is the one that happened — which is what
    /// lets a scene say "every leaf is a rail, except these".
    #[test]
    fn the_last_matching_rule_wins() {
        let scene = scene(
            Effect::at(Size::Compact),
            vec![
                Rule::new(
                    Selector {
                        role: Role::Leaf,
                        ..Selector::default()
                    },
                    Effect::at(Size::Minimum),
                ),
                Rule::new(
                    Selector {
                        kind: Some("source".to_owned()),
                        ..Selector::default()
                    },
                    Effect::at(Size::Working),
                ),
            ],
        );
        let rows = resolve(&scene, &kit(), Surface::Mixer, None, None, None);
        let size = |guid: &str| {
            rows.iter()
                .find(|r| r.guid() == Some(guid))
                .expect("the row")
                .size
        };
        assert_eq!(size("kit"), Size::Compact, "the default covers the folder");
        assert_eq!(size("in"), Size::Working, "the later rule won");
    }

    /// A lifted folder drops its own row and keeps what it holds, one
    /// level up: a piece's Sum while editing, its mics under the piece.
    #[test]
    fn a_lifted_folder_leaves_its_rows_a_level_up() {
        let drums = Segment::named("Drum Kit")
            .of(Kind::Group)
            .standing_for("Drum Kit");
        let kick = Segment::named("Kick").of(Kind::Piece).standing_for("Kick");
        let sum = Segment::named("Sum").of(Kind::Sum);
        let facts = vec![
            Fact::folder("kick", "Kick", 0, 0)
                .of(Kind::Piece)
                .at(vec![drums.clone(), kick.clone()]),
            Fact::folder("sum", "Sum", 1, 1).of(Kind::Sum).at(vec![
                drums.clone(),
                kick.clone(),
                sum.clone(),
            ]),
            Fact::leaf("in", "In", 2, 2).of(Kind::Source).at(vec![
                drums.clone(),
                kick.clone(),
                sum,
            ]),
            Fact::leaf("sub", "Sub", 3, 1)
                .of(Kind::Sub)
                .at(vec![drums, kick]),
        ];
        let lifted = scene(
            Effect::at(Size::Compact),
            vec![Rule::new(
                Selector {
                    kind: Some("sum".to_owned()),
                    ..Selector::default()
                },
                Effect {
                    size: None,
                    fold: Some(Fold::Lifted),
                },
            )],
        );
        let rows = resolve(&lifted, &facts, Surface::Arrange, None, None, None);
        let placed: Vec<(&str, u32)> = rows
            .iter()
            .filter_map(|r| Some((r.guid()?, r.depth)))
            .collect();
        assert_eq!(placed, [("kick", 0), ("in", 1), ("sub", 1)]);
    }

    /// A collapsed folder keeps its own row and drops what it holds; a
    /// hidden one drops both.
    #[test]
    fn a_fold_reaches_the_rows_beneath_it() {
        let collapsed = scene(
            Effect::at(Size::Compact),
            vec![Rule::new(
                Selector {
                    kind: Some("piece".to_owned()),
                    ..Selector::default()
                },
                Effect::at(Size::Working).folded(Fold::Collapsed),
            )],
        );
        let rows = resolve(&collapsed, &kit(), Surface::Mixer, None, None, None);
        let guids: Vec<&str> = rows.iter().filter_map(Row::guid).collect();
        assert_eq!(guids, ["kit", "kick"], "the mics went with the fold");

        let hidden = scene(
            Effect::at(Size::Compact),
            vec![Rule::new(
                Selector {
                    kind: Some("piece".to_owned()),
                    ..Selector::default()
                },
                Effect::hidden(),
            )],
        );
        let rows = resolve(&hidden, &kit(), Surface::Mixer, None, None, None);
        let guids: Vec<&str> = rows.iter().filter_map(Row::guid).collect();
        assert_eq!(guids, ["kit"], "the piece went too");
    }

    /// The pair-half invariant: not a rule a scene can state, and not a
    /// rule a scene can override.
    #[test]
    fn a_stereo_pairs_half_is_always_a_rail() {
        let mut facts = kit();
        if let Some(fact) = facts.get_mut(2) {
            fact.pair_half = true;
        }
        let wide = scene(Effect::at(Size::Focus), Vec::new());
        let rows = resolve(&wide, &facts, Surface::Mixer, None, None, None);
        let size = |guid: &str| {
            rows.iter()
                .find(|r| r.guid() == Some(guid))
                .expect("the row")
                .size
        };
        assert_eq!(size("in"), Size::Minimum, "a half is a rail");
        assert_eq!(size("out"), Size::Focus, "and nothing else changed");
    }

    /// A selector's group path is a prefix, and each of its segments
    /// matches the kind, the template group or the name — which is what
    /// a renamed Process folder relies on.
    #[test]
    fn a_group_prefix_matches_a_renamed_folder() {
        let parallel = Segment::named("Parallel").of(Kind::Process);
        let facts = vec![
            Fact::folder("proc", "Parallel", 0, 0)
                .of(Kind::Process)
                .at(vec![parallel.clone()]),
            Fact::leaf("dry", "Dry", 1, 1)
                .of(Kind::Part)
                .at(vec![parallel]),
        ];
        let s = scene(
            Effect::at(Size::Compact),
            vec![Rule::new(
                Selector {
                    group: vec!["process".to_owned()],
                    ..Selector::default()
                },
                Effect::at(Size::Minimum),
            )],
        );
        let rows = resolve(&s, &facts, Surface::Mixer, None, None, None);
        assert!(rows.iter().all(|r| r.size == Size::Minimum), "{rows:?}");
    }

    /// A per-surface override is what lets one scene rail a track in the
    /// mixer and still give its row height in the arrangement.
    #[test]
    fn the_two_surfaces_can_disagree() {
        let s = scene(
            Effect::at(Size::Compact),
            vec![Rule {
                selector: Selector {
                    role: Role::Leaf,
                    ..Selector::default()
                },
                effect: Effect::at(Size::Minimum),
                arrange: Some(Effect::at(Size::Working)),
                mixer: None,
            }],
        );
        let mixer = resolve(&s, &kit(), Surface::Mixer, None, None, None);
        let arrange = resolve(&s, &kit(), Surface::Arrange, None, None, None);
        assert_eq!(mixer[2].size, Size::Minimum);
        assert_eq!(arrange[2].size, Size::Working);
    }

    /// One mic per piece, which is what an edit view of a kit is.
    #[test]
    fn topmost_per_instrument_keeps_one_of_each() {
        let s = scene(
            Effect::at(Size::Minimum),
            vec![Rule::new(
                Selector {
                    role: Role::Leaf,
                    rank: Rank::TopmostPerInstrument,
                    ..Selector::default()
                },
                Effect::at(Size::Working),
            )],
        );
        let rows = resolve(&s, &kit(), Surface::Mixer, None, None, None);
        let opened: Vec<&str> = rows
            .iter()
            .filter(|r| r.size == Size::Working)
            .filter_map(Row::guid)
            .collect();
        assert_eq!(opened, ["in"], "one mic of the piece, not both");
    }

    /// `Vocals / Ron / Main / {EN, ES, PT}` and `Ron / DBL / {EN, ES,
    /// PT}` — a lead's two layers, each with a source per language.
    fn ron() -> Vec<Fact> {
        use super::super::language::Language;

        let vocals = Segment::named("Vocals").of(Kind::Group);
        let ron = Segment::named("Ron");
        let main = Segment::named("Main");
        let dbl = Segment::named("DBL");
        vec![
            Fact::folder("vocals", "Vocals", 0, 0)
                .of(Kind::Group)
                .at(vec![vocals.clone()]),
            Fact::folder("ron", "Ron", 1, 1).at(vec![vocals.clone(), ron.clone()]),
            Fact::folder("main", "Main", 2, 2).at(vec![vocals.clone(), ron.clone(), main.clone()]),
            Fact::leaf("main-en", "EN", 3, 3)
                .of(Kind::Source)
                .at(vec![vocals.clone(), ron.clone(), main.clone()])
                .speaking(Language::En),
            Fact::leaf("main-es", "ES", 4, 3)
                .of(Kind::Source)
                .at(vec![vocals.clone(), ron.clone(), main.clone()])
                .speaking(Language::Es),
            Fact::leaf("main-pt", "PT", 5, 3)
                .of(Kind::Source)
                .at(vec![vocals.clone(), ron.clone(), main])
                .speaking(Language::Pt),
            Fact::folder("dbl", "DBL", 6, 2).at(vec![vocals.clone(), ron.clone(), dbl.clone()]),
            Fact::leaf("dbl-en", "EN", 7, 3)
                .of(Kind::Source)
                .at(vec![vocals.clone(), ron.clone(), dbl.clone()])
                .speaking(Language::En),
            Fact::leaf("dbl-es", "ES", 8, 3)
                .of(Kind::Source)
                .at(vec![vocals.clone(), ron.clone(), dbl.clone()])
                .speaking(Language::Es),
            Fact::leaf("dbl-pt", "PT", 9, 3)
                .of(Kind::Source)
                .at(vec![vocals, ron, dbl])
                .speaking(Language::Pt),
        ]
    }

    /// The active language hides every source whose language is neither
    /// active nor `All`; the mix tracks above them — `Vocals`, `Ron`,
    /// `Main`, `DBL` — carry no language and stay in view whatever the
    /// language.
    // r[verify flow.vocals.language]
    // r[verify flow.vocals.language.active]
    #[test]
    fn the_active_language_hides_the_others_and_keeps_the_mix_tracks() {
        use super::super::language::Language;

        let s = scene(Effect::at(Size::Compact), Vec::new());
        let facts = ron();
        let rows = resolve(&s, &facts, Surface::Mixer, None, Some(Language::En), None);
        let shown: Vec<&str> = rows.iter().filter_map(Row::guid).collect();
        assert_eq!(
            shown,
            ["vocals", "ron", "main", "main-en", "dbl", "dbl-en"],
            "only English sources survive, the mix tracks stay"
        );
    }

    /// Switching the language switches which sources it hides, in one
    /// step — no VCA, no separate switch, just a different value in.
    #[test]
    fn switching_the_language_switches_which_sources_show() {
        use super::super::language::Language;

        let s = scene(Effect::at(Size::Compact), Vec::new());
        let facts = ron();
        let rows = resolve(&s, &facts, Surface::Mixer, None, Some(Language::Es), None);
        let shown: Vec<&str> = rows.iter().filter_map(Row::guid).collect();
        assert_eq!(shown, ["vocals", "ron", "main", "main-es", "dbl", "dbl-es"]);
    }

    /// With no active language set, nothing is hidden — an untouched
    /// project shows every source rather than guessing which one.
    #[test]
    fn with_no_active_language_nothing_is_hidden() {
        let s = scene(Effect::at(Size::Compact), Vec::new());
        let facts = ron();
        let rows = resolve(&s, &facts, Surface::Mixer, None, None, None);
        assert_eq!(rows.len(), facts.len(), "nothing hidden");
    }

    /// A language-free source (`All`) is never hidden, whatever the
    /// active language is.
    #[test]
    fn all_is_never_hidden() {
        use super::super::language::Language;

        let vocals = Segment::named("Vocals").of(Kind::Group);
        let hey = Segment::named("Hey");
        let facts = vec![
            Fact::folder("hey", "Hey", 0, 0).at(vec![vocals.clone(), hey.clone()]),
            Fact::leaf("hey-all", "All", 1, 1)
                .of(Kind::Source)
                .at(vec![vocals, hey])
                .speaking(Language::All),
        ];
        let s = scene(Effect::at(Size::Compact), Vec::new());
        let rows = resolve(&s, &facts, Surface::Mixer, None, Some(Language::En), None);
        let shown: Vec<&str> = rows.iter().filter_map(Row::guid).collect();
        assert_eq!(shown, ["hey", "hey-all"]);
    }

    /// A guitar rig, two performers: each with a Rhythm and a Lead
    /// track, in arrangement order — the project's own order, which the
    /// scenario below asserts stays exactly this.
    fn guitars() -> Vec<Fact> {
        let group = Segment::named("Electric").of(Kind::Group);
        let fact = |guid: &str, name: &str, index: u32, performer: &str| {
            let mut f = Fact::leaf(guid, name, index, 1)
                .of(Kind::Part)
                .at(vec![group.clone()]);
            f.performer = Some(performer.to_owned());
            f
        };
        vec![
            Fact::folder("electric", "Electric", 0, 0)
                .of(Kind::Group)
                .at(vec![group.clone()]),
            fact("cody-rhythm", "Rhythm", 1, "Cody"),
            fact("ron-rhythm", "Rhythm", 2, "Ron"),
            fact("cody-lead", "Lead", 3, "Cody"),
            fact("ron-lead", "Lead", 4, "Ron"),
        ]
    }

    /// `flow.scenes.performer-order`: tracking regroups a guitar rig
    /// under one header per performer, that performer's tracks beneath
    /// it, without moving a single track in the underlying fact list —
    /// which is the project's own order, asserted unchanged below.
    ///
    /// r[verify flow.scenes.performer-order]
    #[test]
    fn group_by_performer_synthesises_one_header_per_performer() {
        let facts = guitars();
        let before: Vec<&str> = facts.iter().map(|f| f.guid.as_str()).collect();
        let mut s = scene(Effect::at(Size::Compact), Vec::new());
        s.group_by = GroupBy::Performer;
        let rows = resolve(&s, &facts, Surface::Mixer, None, None, None);

        // The project's own order is exactly what it was — the
        // regrouping is a view, not a move.
        let after: Vec<&str> = facts.iter().map(|f| f.guid.as_str()).collect();
        assert_eq!(before, after, "resolve never touches its input");

        let targets: Vec<(u32, String)> = rows
            .iter()
            .map(|r| {
                let label = match &r.target {
                    Target::Track(guid) => facts
                        .iter()
                        .find(|f| &f.guid == guid)
                        .map_or_else(|| guid.clone(), |f| f.name.clone()),
                    Target::PerformerHeader(name) => format!("#{name}"),
                };
                (r.depth, label)
            })
            .collect();
        assert_eq!(
            targets,
            vec![
                (0, "Electric".to_owned()),
                (1, "#Cody".to_owned()),
                (2, "Rhythm".to_owned()),
                (2, "Lead".to_owned()),
                (1, "#Ron".to_owned()),
                (2, "Rhythm".to_owned()),
                (2, "Lead".to_owned()),
            ],
            "one header per performer, first appearance order, tracks nested a level deeper"
        );
    }

    /// A track with no identified performer is shown, under
    /// `UNASSIGNED`, rather than dropped — the Patch List's own rule for
    /// an unassigned track, read onto the scene's view.
    ///
    /// r[verify flow.scenes.performer-order]
    #[test]
    fn an_unidentified_performer_groups_under_unassigned() {
        let mut s = scene(Effect::at(Size::Compact), Vec::new());
        s.group_by = GroupBy::Performer;
        // The kit fixture's mics carry no performer at all.
        let rows = resolve(&s, &kit(), Surface::Mixer, None, None, None);
        assert_eq!(
            rows.iter()
                .find(|r| matches!(&r.target, Target::PerformerHeader(name) if name == UNASSIGNED))
                .map(|r| r.depth),
            Some(1),
            "{rows:?}"
        );
    }

    /// `order` breaks the tie between performers found — Ron before
    /// Cody here, though Cody appeared first in the track list.
    ///
    /// r[verify flow.scenes.performer-order]
    #[test]
    fn the_injected_order_overrides_first_appearance() {
        let facts = guitars();
        let mut s = scene(Effect::at(Size::Compact), Vec::new());
        s.group_by = GroupBy::Performer;
        let reverse = |a: &str, b: &str| b.cmp(a);
        let rows = resolve(&s, &facts, Surface::Mixer, None, None, Some(&reverse));
        let headers: Vec<&str> = rows
            .iter()
            .filter_map(|r| match &r.target {
                Target::PerformerHeader(name) => Some(name.as_str()),
                Target::Track(_) => None,
            })
            .collect();
        assert_eq!(headers, ["Ron", "Cody"]);
    }

    /// A scene that groups by arrangement is untouched by any of this —
    /// the engine invariant is scoped to `GroupBy::Performer` alone.
    #[test]
    fn arrangement_grouping_is_the_plain_walk() {
        let s = scene(Effect::at(Size::Compact), Vec::new());
        let rows = resolve(&s, &guitars(), Surface::Mixer, None, None, None);
        assert!(
            rows.iter().all(|r| r.guid().is_some()),
            "no header without group_by: performer"
        );
    }
}
