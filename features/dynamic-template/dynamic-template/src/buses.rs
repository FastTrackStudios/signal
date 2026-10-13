//! The canonical FTS bus tree — where each instrument group hands off to the
//! mix.
//!
//! [`colors`](crate::colors) answers "what color is this group?" and
//! [`layouts`](crate::layouts) answers "what TCP/MCP layout does it get?".
//! This module answers the third per-group question: **what does it feed?**
//! Like those two it resolves by canonical group path (top-level first, e.g.
//! `["Vocals", "Lead"]`), most specific first, so a sub-group can attach
//! somewhere its parent does not.
//!
//! # Shape
//!
//! Group buses sum into one of two stem buses — `INST BUS` and `VOX BUS` —
//! which sum into `MIX BUS`. That middle tier is what makes an instrumental
//! and an acappella fall out of the session for free: mute one stem bus and
//! the other is already printed.
//!
//! ```text
//! MIX BUS                            (always present)
//! ├─ INST BUS
//! │  ├─ DRUM BUS      ← Drums
//! │  ├─ PERC BUS      ← Percussion
//! │  ├─ BASS BUS      ← Bass
//! │  ├─ GUITAR BUS    ← Guitars (steel, banjo, mandolin)
//! │  │  ├─ ACOUSTIC BUS ← Guitars/Acoustic
//! │  │  └─ ELECTRIC BUS ← Guitars/Electric
//! │  ├─ KEYS BUS      ← Keys, Synths
//! │  ├─ ORCH BUS      ← Orchestra, Horns, Fiddle, Harmonica
//! │  └─ FX BUS        ← SFX, Tracks
//! └─ VOX BUS
//!    ├─ LEAD VOX BUS  ← Vocals, Vocals/Lead
//!    └─ BGV BUS       ← Vocals/BGVs, Choir
//!
//! CLICK + GUIDE BUS   ← Guide        (parallel to MIX BUS, off master)
//! HEADPHONE MIXES     ← Headphones (per-player cue mixes)
//! TALKBACK BUS        ← Talkback
//! UTILITY BUS         ← Reference (incl. Reference/Stem Split)
//! ```
//!
//! `INST BUS` and `VOX BUS` have no group sources of their own — nothing
//! attaches to them directly, they only sum the group buses beneath them. They
//! materialize when any of those does, via the ancestor walk in
//! [`buses_for_paths`].
//!
//! # Buses are tracks
//!
//! Each bus is a real track, nested as a **folder track** inside the bus it
//! feeds: `DRUM BUS` is a child of `INST BUS`, which is a child of `MIX BUS`.
//! So a bus reaches its parent by the ordinary folder parent-send —
//! [`TemplateBus::parent`] is that nesting, not a send.
//!
//! Instrument folders are the other case: they sit elsewhere in the project
//! and reach their bus by an explicit send
//! ([`NodeRouting::to_bus`](dynamic_template_proto::NodeRouting::to_bus),
//! which drops the parent send). [`bus_nodes`] renders the bus tree as the
//! nested [`TemplateNode`] folder tracks a DAW layer can create directly.
//!
//! The monitor buses sit **beside** `MIX BUS` rather than inside it: click,
//! count-ins, cues, per-player headphone mixes, talkback chatter, the reference
//! master and stem-split imports are all monitor-only, and routing them through
//! `MIX BUS` would print them into any bounce or stem unless muted by hand.
//!
//! Some groups reach **no** bus at all, deliberately: a `VCA` carries no audio,
//! only fader control, so a send from one would be silent. See
//! [`is_deliberately_unrouted`].
//!
//! **No bus exists unless something feeds it.** Every bus is
//! [`WhenPopulated`](BusMaterialization::WhenPopulated), including `MIX BUS`:
//! [`buses_for_paths`] starts from the group paths a session actually contains
//! and keeps only the buses those reach, plus their ancestors. An empty project
//! gets no buses; an acoustic-only session gets `ACOUSTIC BUS` but no
//! `ELECTRIC BUS`, and no `VOX BUS` at all.

use std::collections::HashSet;

use dynamic_template_proto::{BusMaterialization, TemplateBus, TemplateNode};

use crate::colors::color_for_path;

/// Names of the buses in the canonical tree, so callers can reference one
/// without spelling the string.
pub mod names {
    /// The mix sum. Everything musical ends up here; always present.
    pub const MIX: &str = "MIX BUS";
    /// The instrumental stem: every group bus that is not a vocal.
    pub const INST: &str = "INST BUS";
    /// The vocal stem: lead and background vocals.
    pub const VOX: &str = "VOX BUS";
    /// The drum kit.
    pub const DRUM: &str = "DRUM BUS";
    /// Percussion — its own bus, so it can be balanced (or muted) apart
    /// from the kit.
    pub const PERC: &str = "PERC BUS";
    /// Bass in all its forms (electric, synth, upright).
    pub const BASS: &str = "BASS BUS";
    /// Every guitar. Acoustics and electrics sum through their own buses
    /// beneath this one; steel, banjo and mandolin attach here directly.
    pub const GUITAR: &str = "GUITAR BUS";
    /// Acoustic guitars.
    pub const ACOUSTIC: &str = "ACOUSTIC BUS";
    /// Electric guitars.
    pub const ELECTRIC: &str = "ELECTRIC BUS";
    /// Keys and synths.
    pub const KEYS: &str = "KEYS BUS";
    /// Orchestral and other acoustic-ensemble material.
    pub const ORCH: &str = "ORCH BUS";
    /// Sound design and backing/playback tracks.
    pub const FX: &str = "FX BUS";
    /// Lead vocal.
    pub const LEAD_VOX: &str = "LEAD VOX BUS";
    /// Background vocals and choir.
    pub const BGV: &str = "BGV BUS";
    /// Click, count-ins, cues, section markers. Off master, never in the mix.
    pub const GUIDE: &str = "CLICK + GUIDE BUS";
    /// Reference mixes and stem-split imports. Off master, never in the mix.
    pub const UTILITY: &str = "UTILITY BUS";
    /// Talkback / comms mics. Off master, never in the mix.
    pub const TALKBACK: &str = "TALKBACK BUS";
    /// Per-player monitor mixes. Off master, never in the mix.
    pub const HEADPHONES: &str = "HEADPHONE MIXES";
}

/// One entry in the bus tree: a bus, what feeds it, and where it goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusSpec {
    /// Bus name — one of [`names`].
    pub name: &'static str,
    /// The bus this one feeds, or `None` to go straight to the project master.
    pub parent: Option<&'static str>,
    /// Channel count (2 = stereo).
    pub channels: u32,
    /// Canonical group paths that attach here, most specific first. A group
    /// path attaches to the *longest* matching entry across the whole table.
    pub sources: &'static [&'static [&'static str]],
    /// Other names this bus goes by in projects that predate the taxonomy.
    /// Matched case-insensitively when looking for an existing bus, so
    /// applying the template to a session that already has a `GUITAR BUS`
    /// reuses it instead of creating a second `GTR BUS` beside it.
    pub aliases: &'static [&'static str],
    /// Whether the bus is emitted when nothing routes into it.
    pub materialize: BusMaterialization,
}

impl BusSpec {
    /// Every name this bus answers to: its canonical name first, then its
    /// aliases. [`apply_buses`](crate::apply::apply_buses) tries these in
    /// order when looking for a bus a project already has.
    pub fn known_names(&self) -> impl Iterator<Item = &'static str> + '_ {
        std::iter::once(self.name).chain(self.aliases.iter().copied())
    }

    /// Resolve this bus's color from the first of its source group paths that
    /// has one — so `DRUM BUS` takes the drums color, `GTR BUS` the guitars
    /// color. A bus that only sums other buses has no color of its own.
    #[must_use]
    pub fn color_hex(&self) -> Option<String> {
        self.sources
            .iter()
            .find_map(|path| color_for_path(path))
            .map(color_palette::Color::to_hex_string)
    }

    /// This spec as the pure-data proto type.
    pub fn to_template_bus(self) -> TemplateBus {
        TemplateBus {
            name: self.name.to_string(),
            channels: self.channels,
            parent: self.parent.map(str::to_string),
            sources: self
                .sources
                .iter()
                .map(|p| p.iter().map(|s| (*s).to_string()).collect())
                .collect(),
            color_hex: self.color_hex(),
            materialize: self.materialize,
        }
    }
}

const fn bus(
    name: &'static str,
    parent: Option<&'static str>,
    sources: &'static [&'static [&'static str]],
) -> BusSpec {
    BusSpec {
        name,
        parent,
        channels: 2,
        sources,
        aliases: &[],
        materialize: BusMaterialization::WhenPopulated,
    }
}

/// A bus that also answers to `aliases` — names the same bus already carries in
/// existing sessions.
const fn aliased(
    name: &'static str,
    parent: Option<&'static str>,
    sources: &'static [&'static [&'static str]],
    aliases: &'static [&'static str],
) -> BusSpec {
    BusSpec {
        aliases,
        ..bus(name, parent, sources)
    }
}

/// The canonical FTS bus tree, in display order: the mix sum first, then its
/// group buses, then the two off-master monitor buses.
///
/// Ordering within `sources` does not matter for resolution — [`bus_for_path`]
/// picks the longest match across the whole table — but it does decide a bus's
/// color, which is taken from the first source with one.
pub const BUS_TREE: &[BusSpec] = &[
    bus(names::MIX, None, &[]),
    // The instrumental stem and the group buses feeding it.
    bus(names::INST, Some(names::MIX), &[]),
    bus(names::DRUM, Some(names::INST), &[&["Drums"]]),
    aliased(
        names::PERC,
        Some(names::INST),
        &[&["Percussion"]],
        &["PERCUSSION BUS"],
    ),
    bus(names::BASS, Some(names::INST), &[&["Bass"]]),
    aliased(
        names::GUITAR,
        Some(names::INST),
        &[&["Guitars"]],
        &["GTR BUS", "GTRS BUS", "GUITARS BUS"],
    ),
    aliased(
        names::ACOUSTIC,
        Some(names::GUITAR),
        &[&["Guitars", "Acoustic"]],
        &[
            "Guitar A BUS",
            "GTR A BUS",
            "AC GTR BUS",
            "ACOUSTIC GTR BUS",
        ],
    ),
    aliased(
        names::ELECTRIC,
        Some(names::GUITAR),
        &[&["Guitars", "Electric"]],
        &[
            "Guitar E BUS",
            "GTR E BUS",
            "EL GTR BUS",
            "ELECTRIC GTR BUS",
        ],
    ),
    aliased(
        names::KEYS,
        Some(names::INST),
        &[&["Keys"], &["Synths"]],
        &["KEY BUS", "SYNTH BUS"],
    ),
    bus(
        names::ORCH,
        Some(names::INST),
        &[&["Orchestra"], &["Horns"], &["Fiddle"], &["Harmonica"]],
    ),
    aliased(
        names::FX,
        Some(names::INST),
        &[&["SFX"], &["Tracks"]],
        &["SFX BUS", "TRACKS BUS", "TRK BUS"],
    ),
    // The vocal stem and the group buses feeding it.
    bus(names::VOX, Some(names::MIX), &[]),
    // A bare "Vocals" track is one that classified as a vocal but not as Lead
    // or BGVs — treat it as a lead, which is what an unqualified vocal is.
    bus(
        names::LEAD_VOX,
        Some(names::VOX),
        &[&["Vocals", "Lead"], &["Vocals"]],
    ),
    bus(
        names::BGV,
        Some(names::VOX),
        &[&["Vocals", "BGVs"], &["Choir"]],
    ),
    // Off master, in parallel with MIX BUS — monitor-only, never printed.
    aliased(
        names::GUIDE,
        None,
        &[&["Guide"]],
        &["GUIDE BUS", "CLICK BUS", "CLICK/GUIDE BUS", "CLICK + GUIDE"],
    ),
    aliased(
        names::HEADPHONES,
        None,
        &[&["Headphones"]],
        &["HEADPHONE BUS", "CUE BUS", "HP BUS", "HEADPHONES"],
    ),
    aliased(
        names::TALKBACK,
        None,
        &[&["Talkback"]],
        &["TB BUS", "COMMS BUS"],
    ),
    bus(
        names::UTILITY,
        None,
        &[&["Reference"], &["Reference", "Stem Split"]],
    ),
];

/// The bus a canonical group path attaches to, or `None` if it attaches
/// nowhere (a sub-folder inherits its parent's attachment rather than getting
/// its own).
///
/// Matching is case-insensitive and takes the **longest** source path that is
/// a prefix of `path`, so `["Vocals", "BGVs", "Performer"]` resolves to
/// `BGV BUS` rather than the shorter `["Vocals"]` entry on `LEAD VOX BUS`.
///
/// # Example
/// ```
/// use dynamic_template::buses::{bus_for_path, names};
///
/// assert_eq!(bus_for_path(&["Drums".into()]), Some(names::DRUM));
/// assert_eq!(bus_for_path(&["Vocals".into(), "BGVs".into()]), Some(names::BGV));
///
/// // Electric has a bus of its own, so the longer source wins.
/// assert_eq!(bus_for_path(&["Guitars".into(), "Electric".into()]), Some(names::ELECTRIC));
/// // Steel does not, so it falls back to the shorter ["Guitars"] source.
/// assert_eq!(bus_for_path(&["Guitars".into(), "Steel".into()]), Some(names::GUITAR));
///
/// // A VCA carries no audio, so it deliberately attaches nowhere.
/// assert_eq!(bus_for_path(&["VCA".into()]), None);
/// ```
#[must_use]
pub fn bus_for_path(path: &[String]) -> Option<&'static str> {
    let mut best: Option<(usize, &'static str)> = None;
    for spec in BUS_TREE {
        for source in spec.sources {
            if source.len() <= path.len()
                && source
                    .iter()
                    .zip(path)
                    .all(|(a, b)| a.eq_ignore_ascii_case(b))
                && best.is_none_or(|(len, _)| source.len() > len)
            {
                best = Some((source.len(), spec.name));
            }
        }
    }
    best.map(|(_, name)| name)
}

/// Whether `path` is exactly a bus attachment point — the node that carries
/// the [`NodeRouting`](dynamic_template_proto::NodeRouting) into a bus, as
/// opposed to a descendant that merely inherits it.
#[must_use]
pub fn is_attachment_point(path: &[String]) -> bool {
    BUS_TREE.iter().any(|spec| {
        spec.sources.iter().any(|source| {
            source.len() == path.len()
                && source
                    .iter()
                    .zip(path)
                    .all(|(a, b)| a.eq_ignore_ascii_case(b))
        })
    })
}

/// The spec for a bus by canonical name or [`alias`](BusSpec::aliases),
/// case-insensitively.
#[must_use]
pub fn spec(name: &str) -> Option<&'static BusSpec> {
    let needle = name.trim();
    BUS_TREE.iter().find(|s| {
        s.name.eq_ignore_ascii_case(needle)
            || s.aliases.iter().any(|a| a.eq_ignore_ascii_case(needle))
    })
}

/// Canonical group paths that deliberately reach no bus.
///
/// A `VCA` is fader control, not audio: a send from one carries no signal, so
/// "no bus" is the finished state rather than a gap. Callers use this to tell a
/// track that is *done* from one that nothing recognised and which therefore
/// needs a human — the difference between leaving `BAND RECORD VCA` alone and
/// sweeping it into `UNSORTED`.
pub const UNROUTED_GROUPS: &[&str] = &["VCA"];

/// Whether `path` belongs to a group that deliberately reaches no bus.
#[must_use]
pub fn is_deliberately_unrouted(path: &[String]) -> bool {
    path.first()
        .is_some_and(|top| UNROUTED_GROUPS.iter().any(|g| g.eq_ignore_ascii_case(top)))
}

/// Whether `name` is a bus track — under its canonical name or any
/// [`alias`](BusSpec::aliases).
///
/// Bus tracks must be excluded before classifying a project's content, because
/// their names classify as the very thing they carry: `VOX BUS` matches the
/// vocal patterns, `DRUM BUS` the drum ones. Left in, an existing bus counts as
/// content justifying its own existence, and a project with buses but no vocals
/// still grows a vocal bus — defeating the whole point of building a bus only
/// when something feeds it.
#[must_use]
pub fn is_bus_name(name: &str) -> bool {
    spec(name).is_some()
}

/// The buses a session containing `paths` needs, in [`BUS_TREE`] order.
///
/// A bus is included when a group path attaches to it, when one of its
/// descendant buses is included, or when it is
/// [`Always`](BusMaterialization::Always) (nothing is, today — see the module
/// docs). Nothing else survives: a session of drums, bass, two guitars, keys
/// and one vocal gets ten buses, not the full thirteen, and a session with no
/// tracks at all gets none.
pub fn buses_for_paths<'a>(paths: impl IntoIterator<Item = &'a [String]>) -> Vec<TemplateBus> {
    let mut needed: HashSet<&'static str> = BUS_TREE
        .iter()
        .filter(|s| s.materialize == BusMaterialization::Always)
        .map(|s| s.name)
        .collect();

    for path in paths {
        if let Some(name) = bus_for_path(path) {
            needed.insert(name);
        }
    }

    // Pull in every ancestor of an included bus. The tree is shallow and
    // acyclic, so walking each included bus's parent chain once suffices.
    let direct: Vec<&'static str> = needed.iter().copied().collect();
    for name in direct {
        let mut cursor = spec(name).and_then(|s| s.parent);
        while let Some(parent) = cursor {
            needed.insert(parent);
            cursor = spec(parent).and_then(|s| s.parent);
        }
    }

    BUS_TREE
        .iter()
        .filter(|s| needed.contains(s.name))
        .map(|s| s.to_template_bus())
        .collect()
}

/// The full bus tree as proto types, ignoring materialization — every bus,
/// populated or not. Used for the golden template, which is the schema rather
/// than any one session.
#[must_use]
pub fn all_buses() -> Vec<TemplateBus> {
    BUS_TREE.iter().map(|s| s.to_template_bus()).collect()
}

/// Render `buses` as nested [`TemplateNode`] folder tracks, ready for a DAW
/// layer to create in order.
///
/// A bus becomes a child of the node named by its
/// [`parent`](TemplateBus::parent); buses with no parent come back as roots,
/// in `buses` order. A bus with children is a [`Folder`](dynamic_template_proto::NodeKind::Folder)
/// and reaches its parent by the folder parent-send; a leaf bus like
/// `DRUM BUS` is a [`Track`](dynamic_template_proto::NodeKind::Track).
///
/// A bus whose parent is not in `buses` is emitted as a root rather than
/// dropped, so a partial slice still renders everything it was given.
#[must_use]
pub fn bus_nodes(buses: &[TemplateBus]) -> Vec<TemplateNode> {
    fn build(bus: &TemplateBus, buses: &[TemplateBus]) -> TemplateNode {
        let children: Vec<TemplateNode> = buses
            .iter()
            .filter(|b| b.parent.as_deref() == Some(bus.name.as_str()))
            .map(|b| build(b, buses))
            .collect();

        // A bus track carries no canonical group path — it is a destination,
        // not a classification of anything.
        let mut node = if children.is_empty() {
            TemplateNode::track(&bus.name)
        } else {
            TemplateNode::folder(&bus.name, Vec::new())
        };
        node.defaults.color_hex.clone_from(&bus.color_hex);
        node.children = children;
        node
    }

    let known = |name: &str| buses.iter().any(|b| b.name == name);
    buses
        .iter()
        .filter(|b| b.parent.as_deref().is_none_or(|p| !known(p)))
        .map(|b| build(b, buses))
        .collect()
}

/// The bus tree a session containing `paths` needs, as nested folder tracks.
///
/// [`buses_for_paths`] then [`bus_nodes`] — the pruning and the nesting in one
/// call, which is what a DAW layer organizing a real project wants.
pub fn bus_nodes_for_paths<'a>(paths: impl IntoIterator<Item = &'a [String]>) -> Vec<TemplateNode> {
    bus_nodes(&buses_for_paths(paths))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dynamic_template_proto::NodeKind;

    fn p(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn every_top_level_group_attaches_to_a_bus() {
        // Walking the classification config rather than a hardcoded list, so a
        // new top-level group fails here until it is given a bus.
        let cfg = crate::default_config();
        let mut unrouted = Vec::new();
        for g in &cfg.groups {
            if g.metadata_only || g.transparent {
                continue;
            }
            if bus_for_path(&p(&[&g.name])).is_none()
                // A group whose children each attach separately (Vocals) is
                // fine as long as every child does.
                && !g
                    .groups
                    .iter()
                    .all(|c| bus_for_path(&p(&[&g.name, &c.name])).is_some())
            {
                unrouted.push(g.name.clone());
            }
        }
        assert!(
            unrouted.is_empty(),
            "top-level groups with no bus: {unrouted:?}"
        );
    }

    #[test]
    fn longest_source_path_wins() {
        assert_eq!(bus_for_path(&p(&["Vocals"])), Some(names::LEAD_VOX));
        assert_eq!(bus_for_path(&p(&["Vocals", "Lead"])), Some(names::LEAD_VOX));
        assert_eq!(bus_for_path(&p(&["Vocals", "BGVs"])), Some(names::BGV));
        // Descendants of an attachment point inherit it.
        assert_eq!(
            bus_for_path(&p(&["Vocals", "BGVs", "Performer"])),
            Some(names::BGV)
        );
    }

    #[test]
    fn sub_groups_without_their_own_bus_do_not_attach() {
        // Drums/Drum Kit inherits DRUM BUS through Drums; it is not itself an
        // attachment point, so the engine must not put a send on it.
        assert!(!is_attachment_point(&p(&["Drums", "Drum Kit"])));
        assert!(is_attachment_point(&p(&["Drums"])));
        assert!(is_attachment_point(&p(&["Vocals", "Lead"])));
    }

    #[test]
    fn guitars_split_acoustic_from_electric() {
        assert_eq!(
            bus_for_path(&p(&["Guitars", "Acoustic"])),
            Some(names::ACOUSTIC)
        );
        assert_eq!(
            bus_for_path(&p(&["Guitars", "Electric"])),
            Some(names::ELECTRIC)
        );
        // Steel, banjo and mandolin have no sub-bus, so they fall back to the
        // shorter ["Guitars"] source on GUITAR BUS.
        assert_eq!(bus_for_path(&p(&["Guitars", "Steel"])), Some(names::GUITAR));
        assert_eq!(bus_for_path(&p(&["Guitars"])), Some(names::GUITAR));

        // Both sub-buses sum through GUITAR BUS, which sums through INST BUS.
        assert_eq!(spec(names::ACOUSTIC).unwrap().parent, Some(names::GUITAR));
        assert_eq!(spec(names::ELECTRIC).unwrap().parent, Some(names::GUITAR));
        assert_eq!(spec(names::GUITAR).unwrap().parent, Some(names::INST));
    }

    #[test]
    fn an_acoustic_only_session_grows_no_electric_bus() {
        let paths = [p(&["Guitars", "Acoustic"])];
        let buses = buses_for_paths(paths.iter().map(Vec::as_slice));
        let got: Vec<&str> = buses.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(
            got,
            vec![names::MIX, names::INST, names::GUITAR, names::ACOUSTIC]
        );
    }

    #[test]
    fn monitor_buses_bypass_the_mix() {
        assert_eq!(spec(names::GUIDE).unwrap().parent, None);
        assert_eq!(spec(names::UTILITY).unwrap().parent, None);
        assert_eq!(spec(names::TALKBACK).unwrap().parent, None);
        assert_eq!(spec(names::MIX).unwrap().parent, None);
    }

    #[test]
    fn group_buses_sum_through_a_stem_bus() {
        for name in [
            names::DRUM,
            names::PERC,
            names::BASS,
            names::GUITAR,
            names::KEYS,
            names::ORCH,
            names::FX,
        ] {
            assert_eq!(spec(name).unwrap().parent, Some(names::INST), "{name}");
        }
        for name in [names::LEAD_VOX, names::BGV] {
            assert_eq!(spec(name).unwrap().parent, Some(names::VOX), "{name}");
        }
        assert_eq!(spec(names::INST).unwrap().parent, Some(names::MIX));
        assert_eq!(spec(names::VOX).unwrap().parent, Some(names::MIX));
    }

    #[test]
    fn stem_buses_have_no_group_sources() {
        // Nothing attaches to INST/VOX directly — they only sum group buses.
        for name in [names::INST, names::VOX] {
            assert!(spec(name).unwrap().sources.is_empty(), "{name}");
        }
        assert!(!is_attachment_point(&p(&["INST BUS"])));
    }

    #[test]
    fn png_band_prunes_to_what_it_plays() {
        // Drums, bass, two guitars, keys, one vocal — the PNG Worship
        // Collective band. No ORCH, no FX, no BGV bus. The stem buses and
        // GUITAR BUS are pulled in by the ancestor walk, not by anything
        // attaching to them directly.
        let paths = [
            p(&["Drums"]),
            p(&["Bass"]),
            p(&["Guitars", "Electric"]),
            p(&["Guitars", "Acoustic"]),
            p(&["Keys"]),
            p(&["Vocals", "Lead"]),
        ];
        let buses = buses_for_paths(paths.iter().map(Vec::as_slice));
        let names: Vec<&str> = buses.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                names::MIX,
                names::INST,
                names::DRUM,
                names::BASS,
                names::GUITAR,
                names::ACOUSTIC,
                names::ELECTRIC,
                names::KEYS,
                names::VOX,
                names::LEAD_VOX,
            ]
        );
    }

    #[test]
    fn an_instrumental_only_session_grows_no_vox_bus() {
        let paths = [p(&["Drums"]), p(&["Bass"])];
        let buses = buses_for_paths(paths.iter().map(Vec::as_slice));
        let names: Vec<&str> = buses.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(
            names,
            vec![names::MIX, names::INST, names::DRUM, names::BASS]
        );
    }

    #[test]
    fn guide_alone_pulls_in_no_mix_bus() {
        // CLICK + GUIDE BUS goes straight to the master, so a project with
        // nothing but guide material needs no mix path at all.
        let paths = [p(&["Guide"])];
        let buses = buses_for_paths(paths.iter().map(Vec::as_slice));
        let got: Vec<&str> = buses.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(got, vec![names::GUIDE]);
        assert_eq!(buses[0].parent, None);
    }

    /// Talkback must never reach the mix: a comms mic on the bass player
    /// summed into BASS BUS prints room chatter over the record.
    #[test]
    fn headphone_mixes_route_off_master() {
        assert_eq!(bus_for_path(&p(&["Headphones"])), Some(names::HEADPHONES));
        assert_eq!(spec(names::HEADPHONES).unwrap().parent, None);

        let paths = [p(&["Headphones"])];
        let buses = buses_for_paths(paths.iter().map(Vec::as_slice));
        let got: Vec<&str> = buses.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(got, vec![names::HEADPHONES], "no mix path at all");
    }

    #[test]
    fn a_vca_is_deliberately_unrouted() {
        assert!(is_deliberately_unrouted(&p(&["VCA"])));
        assert!(!is_deliberately_unrouted(&p(&["Drums"])));
        assert!(
            !is_deliberately_unrouted(&[]),
            "unclassified is not the same"
        );
    }

    #[test]
    fn talkback_routes_off_master_not_into_the_mix() {
        assert_eq!(bus_for_path(&p(&["Talkback"])), Some(names::TALKBACK));
        let tb = spec(names::TALKBACK).unwrap();
        assert_eq!(tb.parent, None);

        let paths = [p(&["Talkback"])];
        let buses = buses_for_paths(paths.iter().map(Vec::as_slice));
        let got: Vec<&str> = buses.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(got, vec![names::TALKBACK], "no mix path at all");
    }

    #[test]
    fn bus_tracks_are_recognised_under_every_name() {
        assert!(is_bus_name("VOX BUS"));
        assert!(is_bus_name("vox bus"), "matching is case-insensitive");
        assert!(is_bus_name("Guitar E BUS"), "an alias is still a bus track");
        assert!(is_bus_name("CLICK + GUIDE BUS"));
        assert!(!is_bus_name("Kick In"));
        assert!(!is_bus_name("GTR E - Chords"));
    }

    #[test]
    fn a_project_of_nothing_but_buses_needs_no_buses() {
        // The failure this guards: bus names classify as the content they
        // carry, so without the filter an existing VOX BUS justifies a
        // LEAD VOX BUS beneath it, forever.
        let names = ["MIX BUS", "VOX BUS", "DRUM BUS", "Guitar E BUS"];
        let paths: Vec<Vec<String>> = names
            .iter()
            .filter(|n| !is_bus_name(n))
            .map(|n| crate::track_schema::classify_track(n).matched_groups)
            .filter(|p| !p.is_empty())
            .collect();
        assert!(
            paths.is_empty(),
            "bus tracks leaked in as content: {paths:#?}"
        );
        assert!(buses_for_paths(paths.iter().map(Vec::as_slice)).is_empty());
    }

    #[test]
    fn an_empty_session_gets_no_buses() {
        let buses = buses_for_paths(std::iter::empty());
        assert!(buses.is_empty(), "no content means no buses: {buses:#?}");
    }

    #[test]
    fn bus_nodes_nest_as_folder_tracks() {
        let nodes = bus_nodes(&all_buses());
        let names: Vec<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
        // Five roots on the master: the mix, and the four monitor buses.
        assert_eq!(
            names,
            vec![
                names::MIX,
                names::GUIDE,
                names::HEADPHONES,
                names::TALKBACK,
                names::UTILITY
            ]
        );

        let mix = &nodes[0];
        assert_eq!(mix.kind, NodeKind::Folder);
        let stems: Vec<&str> = mix.children.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(stems, vec![names::INST, names::VOX]);

        let inst = &mix.children[0];
        assert_eq!(inst.kind, NodeKind::Folder);
        let groups: Vec<&str> = inst.children.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(
            groups,
            vec![
                names::DRUM,
                names::PERC,
                names::BASS,
                names::GUITAR,
                names::KEYS,
                names::ORCH,
                names::FX,
            ]
        );

        // A leaf bus is a plain track, and reaches INST BUS by the folder
        // parent-send rather than an explicit send.
        let drum = &inst.children[0];
        assert_eq!(drum.kind, NodeKind::Track);
        assert!(drum.children.is_empty());
        assert!(drum.routing.parent_send);
        assert_eq!(drum.routing.bus, None);

        // GUIDE BUS has nothing under it, so it is a track, not a folder.
        assert_eq!(nodes[1].kind, NodeKind::Track);
    }

    #[test]
    fn bus_nodes_carry_the_bus_color() {
        let nodes = bus_nodes(&all_buses());
        let drum = &nodes[0].children[0].children[0];
        assert_eq!(drum.name, names::DRUM);
        assert_eq!(
            drum.defaults.color_hex,
            spec(names::DRUM).unwrap().color_hex()
        );
    }

    #[test]
    fn png_band_bus_tracks_are_pruned_too() {
        let paths = [
            p(&["Drums"]),
            p(&["Bass"]),
            p(&["Guitars", "Electric"]),
            p(&["Keys"]),
            p(&["Vocals", "Lead"]),
        ];
        let nodes = bus_nodes_for_paths(paths.iter().map(Vec::as_slice));
        assert_eq!(nodes.len(), 1, "no guide or reference material in this set");
        let mix = &nodes[0];
        assert_eq!(mix.name, names::MIX);

        let inst = &mix.children[0];
        let groups: Vec<&str> = inst.children.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(
            groups,
            vec![names::DRUM, names::BASS, names::GUITAR, names::KEYS]
        );

        let vox = &mix.children[1];
        let vox_groups: Vec<&str> = vox.children.iter().map(|n| n.name.as_str()).collect();
        // VOX BUS has one child, so it stays a folder — no BGV bus.
        assert_eq!(vox_groups, vec![names::LEAD_VOX]);
    }

    #[test]
    fn an_orphaned_parent_still_renders_as_a_root() {
        // A slice that names a parent it does not contain must not silently
        // drop the bus.
        let orphan: Vec<TemplateBus> = all_buses()
            .into_iter()
            .filter(|b| b.name == names::DRUM)
            .collect();
        let nodes = bus_nodes(&orphan);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, names::DRUM);
    }

    #[test]
    fn group_buses_take_their_group_color() {
        let drum = spec(names::DRUM).unwrap();
        assert_eq!(
            drum.color_hex(),
            color_for_path(&["Drums"]).map(color_palette::Color::to_hex_string)
        );
        // MIX BUS sums buses, not groups, so it has no color of its own.
        assert_eq!(spec(names::MIX).unwrap().color_hex(), None);
    }
}
