//! Derive the golden [`IdealFullSessionTemplate`] from the classification
//! [`Config`](crate::DynamicTemplateConfig).
//!
//! The configuration *is* the single source of truth: walking the Group tree
//! yields the structural folders (instrument groups and sub-groups), and each
//! group's dimension sub-groups (Performer / Section / Arrangement / Layers /
//! Channel) plus its multi-mic descriptors yield the per-project deepening
//! axes — with their configured descriptor vocabularies attached. The engine
//! later expands and collapses that schema per project.

use dynamic_template_proto::{IdealFullSessionTemplate, NodeRouting, TemplateNode};
use monarchy::Group;

use crate::buses::{all_buses, bus_for_path, is_attachment_point};
use crate::colors::color_for_path;
use crate::ItemMetadata;

/// Sub-group names that are per-project **dimensions** rather than structural
/// folders, in canonical deepening order. Multi-mic is handled separately
/// (it usually arrives via `field_value_descriptors`) but is also recognised
/// here for the configs that model it as a nested group.
const DIMENSION_NAMES: &[&str] = &[
    "Performer",
    "Section",
    "Arrangement",
    "Layers",
    "Channel",
    "MultiMic",
];

fn is_dimension(name: &str) -> bool {
    DIMENSION_NAMES.iter().any(|d| d.eq_ignore_ascii_case(name))
}

/// The slot band whose canonical path matches `path` exactly (case-insensitive,
/// element for element). Unlike `band_for_category`'s loose element matching,
/// this won't mistake `Synths/Lead` for the `Vocals/Lead` band or `Keys/Electric`
/// for `Guitars/Electric`.
fn band_for_exact_path(path: &[String]) -> Option<&'static music_catalog::groups::GroupSlotBand> {
    music_catalog::groups::SLOT_BANDS.iter().find(|b| {
        b.path.len() == path.len()
            && b.path
                .iter()
                .zip(path)
                .all(|(a, c)| a.eq_ignore_ascii_case(c))
    })
}

/// Build the canonical FTS golden session by walking [`default_config`].
///
/// [`default_config`]: crate::default_config
#[must_use]
pub fn golden_template() -> IdealFullSessionTemplate {
    let cfg = crate::default_config();
    let mut root = Vec::new();
    for g in &cfg.groups {
        // Skip metadata-only groups (e.g. the global "MetadataFields" patterns)
        // and transparent groups, which don't produce a folder of their own.
        if g.metadata_only || g.transparent {
            continue;
        }
        let mut path = Vec::new();
        root.push(build_node(g, &mut path));
    }

    IdealFullSessionTemplate {
        name: "FTS Golden Session".to_string(),
        version: 2,
        root,
        // The golden template is the schema, not a session, so it carries the
        // whole bus tree. Per-project pruning is
        // [`buses_for_paths`](crate::buses::buses_for_paths).
        buses: all_buses(),
    }
}

/// Build a [`TemplateNode`] subtree for one config group. `path` tracks the
/// canonical group path (top-level first) as we recurse.
fn build_node(g: &Group<ItemMetadata>, path: &mut Vec<String>) -> TemplateNode {
    path.push(g.name.clone());

    let group_path: Vec<&str> = path.iter().map(String::as_str).collect();
    let mut node = TemplateNode::folder(&g.name, path.clone());
    node.defaults.color_hex = color_for_path(&group_path).map(color_palette::Color::to_hex_string);

    // Group-slot membership where this node's full canonical path maps to a
    // 128-slot band (e.g. ["Guitars","Electric"] → Electric Gtr; ["Drums"] →
    // Drums). Matched on the full path so sub-groups like Synths/Lead or
    // Keys/Electric don't inherit an unrelated band.
    if let Some(band) = band_for_exact_path(path) {
        node = node.in_group(band.label);
    }

    // Bus attachment. Only the exact attachment point carries the send — a
    // descendant like Guitars/Electric feeds Guitars, which feeds GTR BUS, so
    // giving it its own send would double the guitars into the mix.
    if is_attachment_point(path) {
        if let Some(bus) = bus_for_path(path) {
            node.routing = NodeRouting::to_bus(bus);
        }
    }

    // Structural sub-groups become child folders (recurse); dimension
    // sub-groups feed the deepening chain. Config order is preserved.
    let mut children: Vec<TemplateNode> = Vec::new();
    for child in g.groups.iter().filter(|c| !is_dimension(&c.name)) {
        children.push(build_node(child, path));
    }

    let dimensions: Vec<&Group<ItemMetadata>> =
        g.groups.iter().filter(|c| is_dimension(&c.name)).collect();
    let mic_vocab: Vec<String> = g
        .field_value_descriptors
        .get("MultiMic")
        .map(|ds| ds.iter().map(|d| d.value.clone()).collect())
        .unwrap_or_default();

    if let Some(chain) = dimension_chain(&dimensions, &mic_vocab) {
        children.push(chain);
    }

    // A tagged collection (e.g. drum SUM) declares a conditional grouping over
    // its pattern vocabulary — the engine groups matching captures under it and
    // leaves the rest alongside.
    if let Some(tc) = &g.tagged_collection {
        children.push(TemplateNode::collection(&tc.name, tc.patterns.clone()));
    }

    node = node.with_children(children);

    path.pop();
    node
}

/// Fold the dimension sub-groups (outer → inner) into a nested chain of
/// dimension nodes, bottoming out at the multi-mic captures from
/// `field_value_descriptors` (if any). Each dimension carries its configured
/// `patterns` as its vocabulary. Returns `None` when there are no dimensions
/// and no multi-mic captures.
fn dimension_chain(dims: &[&Group<ItemMetadata>], mic_vocab: &[String]) -> Option<TemplateNode> {
    // Innermost: the multi-mic capture dimension from field_value_descriptors.
    let mut inner: Option<TemplateNode> =
        (!mic_vocab.is_empty()).then(|| TemplateNode::dimension("MultiMic", mic_vocab.to_vec()));

    for d in dims.iter().rev() {
        // A nested-group "MultiMic" already supplies its own vocabulary via
        // patterns; other dimensions likewise use their configured patterns.
        let mut dim = TemplateNode::dimension(&d.name, d.patterns.clone());
        if let Some(child) = inner.take() {
            dim = dim.with_child(child);
        }
        inner = Some(dim);
    }
    inner
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buses::names as bus;
    use dynamic_template_proto::{BusMaterialization, NodeKind};

    fn find<'a>(node: &'a TemplateNode, name: &str) -> Option<&'a TemplateNode> {
        node.children.iter().find(|c| c.name == name)
    }

    #[test]
    fn report_uncolored_folders() {
        fn walk<'a>(n: &'a TemplateNode, out: &mut Vec<&'a TemplateNode>) {
            if n.kind == NodeKind::Folder
                && !n.group_path.is_empty()
                && n.defaults.color_hex.is_none()
            {
                out.push(n);
            }
            for c in &n.children {
                walk(c, out);
            }
        }
        let t = golden_template();
        let mut missing = Vec::new();
        for n in &t.root {
            walk(n, &mut missing);
        }
        let paths: Vec<String> = missing.iter().map(|n| n.group_path.join("/")).collect();
        assert!(
            paths.is_empty(),
            "folders without a resolved color: {paths:#?}"
        );
    }

    #[test]
    fn golden_has_top_level_instrument_folders() {
        let t = golden_template();
        let names: Vec<&str> = t.root.iter().map(|n| n.name.as_str()).collect();
        // A few expected instrument groups; the metadata-only group is skipped.
        assert!(names.contains(&"Drums"));
        assert!(names.contains(&"Guitars"));
        assert!(names.contains(&"Synths"));
        assert!(!names.contains(&"MetadataFields"));
    }

    #[test]
    fn drums_kick_carries_multimic_vocabulary() {
        let t = golden_template();
        let drums = t.root.iter().find(|n| n.name == "Drums").unwrap();
        // Drums slot-band membership is set on the top folder.
        assert_eq!(
            drums.group_membership.as_ref().map(|g| g.category.as_str()),
            Some("Drums")
        );
        let kit = find(drums, "Drum Kit").expect("Drum Kit sub-folder");
        let kick = find(kit, "Kick").expect("Kick sub-folder");
        let mics = find(kick, "MultiMic").expect("Kick MultiMic dimension");
        assert_eq!(mics.kind, NodeKind::Dimension);
        assert_eq!(mics.vocabulary, vec!["In", "Out", "Top", "Bottom"]);

        // Kick also declares the SUM tagged collection (In/Out/Trig).
        let sum = find(kick, "SUM").expect("Kick SUM collection");
        assert_eq!(sum.kind, NodeKind::Collection);
        assert_eq!(sum.vocabulary, vec!["In", "Out", "Trig"]);
    }

    #[test]
    fn electric_guitar_nests_configured_dimensions() {
        let t = golden_template();
        let guitars = t.root.iter().find(|n| n.name == "Guitars").unwrap();
        let electric = find(guitars, "Electric").expect("Electric sub-folder");
        // Electric maps to the Electric Gtr slot band.
        assert_eq!(
            electric
                .group_membership
                .as_ref()
                .map(|g| g.category.as_str()),
            Some("Electric Gtr")
        );

        // Performer → Arrangement → Layers → Channel → MultiMic, in order.
        let performer = find(electric, "Performer").expect("Performer dimension");
        assert_eq!(performer.kind, NodeKind::Dimension);
        let arrangement = find(performer, "Arrangement").expect("Arrangement");
        assert!(arrangement.vocabulary.contains(&"Clean".to_string()));
        assert!(arrangement.vocabulary.contains(&"Crunch".to_string()));
        let layers = find(arrangement, "Layers").expect("Layers");
        let channel = find(layers, "Channel").expect("Channel");
        assert!(channel.vocabulary.contains(&"L".to_string()));
        assert!(channel.vocabulary.contains(&"R".to_string()));
        // The seven sources of one electric channel
        // (`flow.guitars.dimensions`), in mixer order.
        let mics = find(channel, "MultiMic").expect("MultiMic captures");
        assert_eq!(
            mics.vocabulary,
            vec!["Amp", "DI", "Pedalboard", "Amp 1", "Amp 2", "SM57", "Royer"]
        );
    }

    /// Acoustics carry the same four dimensions as the electric with
    /// their own values, which is what lets one set of grow actions
    /// handle both (`flow.guitars.acoustics`).
    ///
    /// Asserted through the classifier rather than the template tree:
    /// only `MultiMic` is declared on the acoustic group — its performer,
    /// arrangement, layer and channel names are the global ones — so the
    /// question "what does the template say Nashville is, under an
    /// acoustic" is the one worth asking.
    #[test]
    fn acoustic_guitar_carries_its_own_source_vocabulary() {
        use crate::track_schema::{
            classify_track_dimension, growth_values_for_dimension, TrackDimension,
        };

        let context = vec!["GTR A".to_string()];
        assert_eq!(
            growth_values_for_dimension(TrackDimension::MultiMic, &context),
            vec!["DI", "Neck", "Body"]
        );
        for (name, dimension) in [
            ("Strum", TrackDimension::Arrangement),
            ("Fingerpick", TrackDimension::Arrangement),
            ("Main", TrackDimension::Layer),
            ("DBL", TrackDimension::Layer),
            ("L", TrackDimension::Channel),
            ("R", TrackDimension::Channel),
            ("DI", TrackDimension::MultiMic),
            ("Neck", TrackDimension::MultiMic),
            ("Body", TrackDimension::MultiMic),
        ] {
            assert_eq!(
                classify_track_dimension(name, &context),
                dimension,
                "{name} under an acoustic should read as {dimension}"
            );
        }
    }

    #[test]
    fn top_level_groups_carry_their_bus_send() {
        let t = golden_template();
        let bus_of = |name: &str| {
            t.root
                .iter()
                .find(|n| n.name == name)
                .unwrap_or_else(|| panic!("no {name} folder"))
                .routing
                .clone()
        };

        for (group, expected) in [
            ("Drums", bus::DRUM),
            ("Percussion", bus::PERC),
            ("Bass", bus::BASS),
            ("Guitars", bus::GUITAR),
            ("Keys", bus::KEYS),
            ("Synths", bus::KEYS),
            ("Orchestra", bus::ORCH),
            ("Horns", bus::ORCH),
            ("SFX", bus::FX),
            ("Tracks", bus::FX),
            ("Choir", bus::BGV),
            ("Guide", bus::GUIDE),
            ("Reference", bus::UTILITY),
        ] {
            let routing = bus_of(group);
            assert_eq!(routing.bus.as_deref(), Some(expected), "{group}");
            // The attachment point feeds the bus, not the master.
            assert!(!routing.parent_send, "{group} should not also parent-send");
        }
    }

    #[test]
    fn vocals_attach_per_sub_group_not_at_the_folder() {
        let t = golden_template();
        let vocals = t.root.iter().find(|n| n.name == "Vocals").unwrap();
        // Lead and BGVs each leave the Vocals folder for their own bus, so the
        // folder carries only vocals that classified as neither — which are
        // leads by default, and land on the lead bus.
        assert_eq!(vocals.routing.bus.as_deref(), Some(bus::LEAD_VOX));

        let lead = find(vocals, "Lead").expect("Vocals/Lead");
        assert_eq!(lead.routing.bus.as_deref(), Some(bus::LEAD_VOX));
        let bgvs = find(vocals, "BGVs").expect("Vocals/BGVs");
        assert_eq!(bgvs.routing.bus.as_deref(), Some(bus::BGV));
    }

    #[test]
    fn sub_folders_inherit_rather_than_re_send() {
        let t = golden_template();
        let drums = t.root.iter().find(|n| n.name == "Drums").unwrap();
        let kit = find(drums, "Drum Kit").expect("Drums/Drum Kit");
        // Drum Kit feeds Drums, which feeds DRUM BUS. A second send here would
        // double the kit into the mix.
        assert_eq!(kit.routing.bus, None);
        assert!(kit.routing.parent_send);
    }

    #[test]
    fn guitar_sub_folders_attach_to_their_own_buses() {
        let t = golden_template();
        let guitars = t.root.iter().find(|n| n.name == "Guitars").unwrap();
        // Guitars itself still carries GUITAR BUS, for the steel/banjo/mandolin
        // that have no sub-bus of their own.
        assert_eq!(guitars.routing.bus.as_deref(), Some(bus::GUITAR));

        let acoustic = find(guitars, "Acoustic").expect("Guitars/Acoustic");
        assert_eq!(acoustic.routing.bus.as_deref(), Some(bus::ACOUSTIC));
        assert!(!acoustic.routing.parent_send);

        let electric = find(guitars, "Electric").expect("Guitars/Electric");
        assert_eq!(electric.routing.bus.as_deref(), Some(bus::ELECTRIC));
        assert!(!electric.routing.parent_send);
    }

    #[test]
    fn golden_carries_the_whole_bus_tree() {
        let t = golden_template();
        let names: Vec<&str> = t.buses.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names[0], bus::MIX);
        assert!(names.contains(&bus::GUIDE));
        assert!(names.contains(&bus::UTILITY));

        let mix = t.buses.iter().find(|b| b.name == bus::MIX).unwrap();
        assert_eq!(mix.parent, None);
        // The golden template is the schema, so it carries every bus. Nothing
        // is unconditional: a real session only gets the buses it feeds.
        for b in &t.buses {
            assert_eq!(
                b.materialize,
                BusMaterialization::WhenPopulated,
                "{} should not exist unless something feeds it",
                b.name
            );
        }

        // Every bus reaches the master: group buses through a stem bus,
        // stem buses through the mix, monitor buses directly.
        for b in &t.buses {
            let mut cursor = b.parent.clone();
            let mut hops = 0;
            while let Some(name) = cursor {
                let parent = t
                    .buses
                    .iter()
                    .find(|x| x.name == name)
                    .unwrap_or_else(|| panic!("{} feeds unknown bus {name}", b.name));
                cursor = parent.parent.clone();
                hops += 1;
                assert!(hops < t.buses.len(), "cycle above {}", b.name);
            }
        }

        // The guitar sub-buses sum through GUITAR BUS, not straight to INST.
        for name in [bus::ACOUSTIC, bus::ELECTRIC] {
            let b = t.buses.iter().find(|b| b.name == name).unwrap();
            assert_eq!(b.parent.as_deref(), Some(bus::GUITAR));
        }

        // The two stem buses are the only things under the mix.
        let under_mix: Vec<&str> = t
            .buses
            .iter()
            .filter(|b| b.parent.as_deref() == Some(bus::MIX))
            .map(|b| b.name.as_str())
            .collect();
        assert_eq!(under_mix, vec![bus::INST, bus::VOX]);

        // Guide and reference material never reaches the mix.
        for name in [bus::GUIDE, bus::UTILITY] {
            assert_eq!(
                t.buses.iter().find(|b| b.name == name).unwrap().parent,
                None
            );
        }
    }

    #[test]
    fn membership_matches_full_path_not_stray_elements() {
        let t = golden_template();
        // Synths/Lead must NOT inherit the Vocals/Lead band.
        let synths = t.root.iter().find(|n| n.name == "Synths").unwrap();
        let synth_lead = find(synths, "Lead").expect("Synths/Lead");
        assert!(synth_lead.group_membership.is_none());
        // Keys/Electric must NOT inherit the Guitars/Electric band.
        let keys = t.root.iter().find(|n| n.name == "Keys").unwrap();
        let keys_electric = find(keys, "Electric").expect("Keys/Electric");
        assert!(keys_electric.group_membership.is_none());

        // Vocals/BGVs DOES map to the Background Vox band (path aligned to the
        // config's "BGVs" sub-group name).
        let vocals = t.root.iter().find(|n| n.name == "Vocals").unwrap();
        let bgvs = find(vocals, "BGVs").expect("Vocals/BGVs");
        assert_eq!(
            bgvs.group_membership.as_ref().map(|g| g.category.as_str()),
            Some("Background Vox")
        );
    }
}
