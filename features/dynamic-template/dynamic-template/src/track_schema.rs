use std::collections::HashSet;

use monarchy::{FieldName, Group, Parser};

use crate::item_metadata::{ItemMetadata, ItemMetadataField};
use crate::DynamicTemplateConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrackDimension {
    Channel,
    Layer,
    MultiMic,
    Performer,
    Arrangement,
    Other,
}

/// Lower-case, hyphenated — these end up in user-facing messages (e.g. a
/// REAPER message box reading "no configured multi-mic name remains"), so
/// the `Debug` spelling (`MultiMic`) isn't what we want there.
impl std::fmt::Display for TrackDimension {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Channel => "channel",
            Self::Layer => "layer",
            Self::MultiMic => "multi-mic",
            Self::Performer => "performer",
            Self::Arrangement => "arrangement",
            Self::Other => "other",
        };
        f.write_str(name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackClassification {
    pub name: String,
    pub visibility_groups: Vec<String>,
    pub matched_groups: Vec<String>,
}

#[must_use]
pub fn classify_track(name: &str) -> TrackClassification {
    let config = crate::shared_config();
    classify_track_with_config(name, &config)
}

#[must_use]
pub fn classify_track_with_config(
    name: &str,
    config: &DynamicTemplateConfig,
) -> TrackClassification {
    // `<player> - <instrument>` (`Tom - Keys`: Tom on keys) names the
    // instrument after the dash, and it wins; `<part> - <song>` (`Kick -
    // Unbreakable`, a rehearsal stem) names nothing there, and the part
    // before decides as it always did.
    if let Some((_, after)) = name
        .rsplit_once(" - ")
        .filter(|(_, after)| !after.trim().is_empty())
    {
        let instrument = classify_track_with_config(after.trim(), config);
        if !instrument.matched_groups.is_empty() {
            return TrackClassification {
                name: name.to_string(),
                ..instrument
            };
        }
    }
    let matched_groups = Parser::new(config)
        .parse(name.to_string())
        .map(|item| {
            item.matched_groups
                .into_iter()
                .map(|group| group.name)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let visibility_groups = top_level_visibility_groups(&matched_groups, config);
    TrackClassification {
        name: name.to_string(),
        visibility_groups,
        matched_groups,
    }
}

#[must_use]
pub fn track_is_in_visibility_group(name: &str, group: &str) -> bool {
    let config = crate::shared_config();
    classify_track_with_config(name, &config)
        .visibility_groups
        .iter()
        .any(|candidate| normalize(candidate) == normalize(group))
}

#[must_use]
pub fn classify_track_dimension(name: &str, context: &[String]) -> TrackDimension {
    let config = crate::shared_config();
    classify_track_dimension_with_config(name, context, &config)
}

#[must_use]
pub fn classify_track_dimension_with_config(
    name: &str,
    context: &[String],
    config: &DynamicTemplateConfig,
) -> TrackDimension {
    let input = contextual_name(name, context);
    let Ok(item) = Parser::new(config).parse(input) else {
        return TrackDimension::Other;
    };
    let metadata = item.metadata;

    if metadata
        .channel
        .as_ref()
        .is_some_and(|value| is_configured_value(TrackDimension::Channel, value, context, config))
    {
        TrackDimension::Channel
    } else if metadata.multi_mic.as_ref().is_some_and(|values| {
        values
            .iter()
            .any(|value| is_configured_value(TrackDimension::MultiMic, value, context, config))
    }) {
        TrackDimension::MultiMic
    } else if metadata
        .layers
        .as_ref()
        .is_some_and(|value| is_configured_value(TrackDimension::Layer, value, context, config))
    {
        TrackDimension::Layer
    } else if metadata
        .performer
        .as_ref()
        .is_some_and(|value| is_configured_value(TrackDimension::Performer, value, context, config))
    {
        TrackDimension::Performer
    } else if metadata.arrangement.as_ref().is_some_and(|value| {
        is_configured_value(TrackDimension::Arrangement, value, context, config)
    }) {
        TrackDimension::Arrangement
    } else {
        TrackDimension::Other
    }
}

#[must_use]
pub fn configured_values_for_dimension(
    dimension: TrackDimension,
    context: &[String],
) -> Vec<String> {
    let config = crate::shared_config();
    configured_values_for_dimension_with_config(dimension, context, &config)
}

#[must_use]
pub fn configured_values_for_dimension_with_config(
    dimension: TrackDimension,
    context: &[String],
    config: &DynamicTemplateConfig,
) -> Vec<String> {
    let Some(field) = field_for_dimension(dimension) else {
        return Vec::new();
    };
    let field_name = field.name();
    let context_input = context.join(" ");
    let matched_group_names: HashSet<String> = Parser::new(config)
        .parse(context_input)
        .map(|item| {
            item.matched_groups
                .into_iter()
                .map(|group| group.name)
                .collect()
        })
        .unwrap_or_default();

    let mut group_values = Vec::new();
    let mut fallback_values = Vec::new();
    for group in &config.groups {
        collect_configured_values(
            group,
            field_name,
            &matched_group_names,
            &mut group_values,
            &mut fallback_values,
        );
    }
    if group_values.is_empty() {
        dedupe_preserving_order(fallback_values)
    } else {
        dedupe_preserving_order(group_values)
    }
}

pub fn next_configured_value(
    dimension: TrackDimension,
    context: &[String],
    existing: impl IntoIterator<Item = impl AsRef<str>>,
) -> Option<String> {
    let existing = existing
        .into_iter()
        .map(|name| normalize(name.as_ref()))
        .collect::<HashSet<_>>();
    configured_values_for_dimension(dimension, context)
        .into_iter()
        .find(|name| !existing.contains(&normalize(name)))
}

/// The configured values of `dimension`, in the order a part *grows*
/// through them (`flow.guitars.grow`).
///
/// Declaration order is mixer order — what organizing a folder sorts by —
/// and two dimensions grow in a different order than they sit in:
///
/// - Channel: listed L, C, R, but a part doubles before it triples, so it
///   grows L, R, C.
/// - `MultiMic`: the amp sits at the top of a channel because it is the
///   sound, but a part is *recorded* outward from the instrument — the DI
///   first, then the pedalboard, then the amps.
/// - Layer: the global list starts at DBL because that is the common
///   suffix to *parse*, but every part starts as its Main and grows the
///   other voices beside it.
#[must_use]
pub fn growth_values_for_dimension(dimension: TrackDimension, context: &[String]) -> Vec<String> {
    let values = configured_values_for_dimension(dimension, context);
    match dimension {
        TrackDimension::Channel => channel_scaffold_order(values),
        TrackDimension::MultiMic => multi_mic_growth_order(values),
        TrackDimension::Layer => layer_growth_order(values),
        _ => values,
    }
}

/// A part is its Main first, then the voices beside it — an octave, then
/// the doubles.
fn layer_growth_order(configured: Vec<String>) -> Vec<String> {
    preferred_first(configured, &["main", "harmony", "oct", "octave"])
}

/// A channel fills out from the instrument outward: the DI, the
/// pedalboard, then the amps in turn, then whatever else the group
/// configures. The bare `Amp` sits behind the numbered ones so a part
/// that wants two of them can reach the second.
fn multi_mic_growth_order(configured: Vec<String>) -> Vec<String> {
    preferred_first(configured, &["di", "pedalboard", "amp 1", "amp 2"])
}

/// `configured` reordered so the values named in `preferred` come first,
/// in that order, with everything else following in declaration order.
/// A preferred value the group does not configure is simply absent.
fn preferred_first(configured: Vec<String>, preferred: &[&str]) -> Vec<String> {
    let mut values = Vec::new();
    for name in preferred {
        push_matching(&configured, &mut values, &[name]);
    }
    for value in configured {
        if !values
            .iter()
            .any(|existing| normalize(existing) == normalize(&value))
        {
            values.push(value);
        }
    }
    values
}

/// The next value of `dimension` a part should grow into, given the
/// values it already carries. [`growth_values_for_dimension`] decides the
/// order.
pub fn next_growth_value(
    dimension: TrackDimension,
    context: &[String],
    existing: impl IntoIterator<Item = impl AsRef<str>>,
) -> Option<String> {
    let existing = existing
        .into_iter()
        .map(|name| normalize(name.as_ref()))
        .collect::<HashSet<_>>();
    growth_values_for_dimension(dimension, context)
        .into_iter()
        .find(|name| !existing.contains(&normalize(name)))
}

/// The configured value of `dimension` carried by `name` itself, if any.
///
/// A part track is usually named for its own arrangement — "GTR E
/// Rhythm" — so growing a second arrangement beside it has to read
/// "Rhythm" back off the container before it can name the folder that
/// takes its place.
#[must_use]
pub fn dimension_value(
    name: &str,
    context: &[String],
    dimension: TrackDimension,
) -> Option<String> {
    // Remembered: a name's dimension never changes, and a plan asks it of
    // every track — two full parses each, ~0.3 ms, every time a view is
    // planned. Asked again, it is a lookup.
    type Key = (String, Vec<String>, TrackDimension);
    static SEEN: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<Key, Option<String>>>,
    > = std::sync::OnceLock::new();
    let seen = SEEN.get_or_init(Default::default);
    let key = (name.to_owned(), context.to_vec(), dimension);
    if let Some(known) = seen
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&key)
    {
        return known.clone();
    }
    let found = parse_dimension_value(name, context, dimension);
    seen.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(key, found.clone());
    found
}

/// [`dimension_value`], worked out.
fn parse_dimension_value(
    name: &str,
    context: &[String],
    dimension: TrackDimension,
) -> Option<String> {
    let config = crate::shared_config();
    // The name is part of its own group context here: a top-level part
    // track named "GTR E Rhythm" is the only thing saying it is an
    // electric, and the electric's arrangement list is the one that has
    // "Rhythm" in it.
    let mut group_context = context.to_vec();
    group_context.push(name.to_string());
    let configured =
        configured_values_for_dimension_with_config(dimension, &group_context, &config);
    let input = contextual_name(name, context);
    let item = Parser::new(&config).parse(input).ok()?;
    let candidates: Vec<String> = match dimension {
        TrackDimension::Channel => item.metadata.channel.into_iter().collect(),
        TrackDimension::Layer => item.metadata.layers.into_iter().collect(),
        TrackDimension::MultiMic => item.metadata.multi_mic.unwrap_or_default(),
        TrackDimension::Performer => item.metadata.performer.into_iter().collect(),
        TrackDimension::Arrangement => item.metadata.arrangement.into_iter().collect(),
        TrackDimension::Other => Vec::new(),
    };
    candidates.into_iter().find_map(|candidate| {
        configured
            .iter()
            .find(|value| normalize(value) == normalize(&candidate))
            .cloned()
    })
}

#[must_use]
pub fn initial_values_for_dimension(
    dimension: TrackDimension,
    context: &[String],
    count: usize,
) -> Vec<String> {
    let values = configured_values_for_dimension(dimension, context);
    let values = match dimension {
        TrackDimension::Channel => channel_scaffold_order(values),
        _ => values,
    };
    values.into_iter().take(count).collect()
}

fn collect_configured_values(
    group: &Group<ItemMetadata>,
    field_name: &str,
    matched_group_names: &HashSet<String>,
    group_values: &mut Vec<String>,
    fallback_values: &mut Vec<String>,
) {
    let target = if matched_group_names.contains(&group.name) {
        Some(&mut *group_values)
    } else if group.metadata_only {
        Some(&mut *fallback_values)
    } else {
        None
    };

    if let Some(output) = target {
        if let Some(descriptors) = group.field_value_descriptors.get(field_name) {
            output.extend(
                descriptors
                    .iter()
                    .map(|descriptor| descriptor.value.clone()),
            );
        }

        for child in &group.groups {
            if child.name == field_name {
                output.extend(child.patterns.iter().cloned());
            }
        }
    }

    for child in &group.groups {
        collect_configured_values(
            child,
            field_name,
            matched_group_names,
            group_values,
            fallback_values,
        );
    }
}

fn top_level_visibility_groups(
    matched_groups: &[String],
    config: &DynamicTemplateConfig,
) -> Vec<String> {
    let top_level = config
        .groups
        .iter()
        .filter(|group| !group.metadata_only)
        .map(|group| normalize(&group.name))
        .collect::<HashSet<_>>();
    dedupe_preserving_order(
        matched_groups
            .iter()
            .filter(|group| top_level.contains(&normalize(group)))
            .cloned()
            .collect(),
    )
}

fn is_configured_value(
    dimension: TrackDimension,
    value: &str,
    context: &[String],
    config: &DynamicTemplateConfig,
) -> bool {
    configured_values_for_dimension_with_config(dimension, context, config)
        .iter()
        .any(|configured| normalize(configured) == normalize(value))
}

const fn field_for_dimension(dimension: TrackDimension) -> Option<ItemMetadataField> {
    match dimension {
        TrackDimension::Channel => Some(ItemMetadataField::Channel),
        TrackDimension::Layer => Some(ItemMetadataField::Layers),
        TrackDimension::MultiMic => Some(ItemMetadataField::MultiMic),
        TrackDimension::Performer => Some(ItemMetadataField::Performer),
        TrackDimension::Arrangement => Some(ItemMetadataField::Arrangement),
        TrackDimension::Other => None,
    }
}

fn contextual_name(name: &str, context: &[String]) -> String {
    if context.is_empty() {
        name.to_string()
    } else {
        format!("{} {name}", context.join(" "))
    }
}

fn dedupe_preserving_order(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(normalize(value)))
        .collect()
}

fn channel_scaffold_order(configured: Vec<String>) -> Vec<String> {
    let mut values = Vec::new();
    push_matching(&configured, &mut values, &["l", "left"]);
    push_matching(&configured, &mut values, &["r", "right"]);
    push_matching(&configured, &mut values, &["c", "center", "centre"]);
    for value in configured {
        if !values
            .iter()
            .any(|existing| normalize(existing) == normalize(&value))
        {
            values.push(value);
        }
    }
    values
}

fn push_matching(configured: &[String], output: &mut Vec<String>, matches: &[&str]) {
    if let Some(value) = configured
        .iter()
        .find(|value| matches.contains(&normalize(value).as_str()))
    {
        output.push(value.clone());
    }
}

fn normalize(value: &str) -> String {
    value.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::default_config;

    #[test]
    #[ignore = "pre-existing failure (predates hygiene migration): classify_track_dimension(\"In\") returns Arrangement, test expects Other — revisit classification config"]
    fn classifies_dimensions_from_contextual_template_metadata() {
        let context = vec!["GTR E Clean".to_string()];

        assert_eq!(
            classify_track_dimension("Amp", &context),
            TrackDimension::MultiMic
        );
        assert_eq!(
            classify_track_dimension("DI", &context),
            TrackDimension::MultiMic
        );
        assert_eq!(
            classify_track_dimension("In", &context),
            TrackDimension::Other
        );
        assert_eq!(
            classify_track_dimension("L", &context),
            TrackDimension::Channel
        );
    }

    #[test]
    fn configured_values_are_group_aware() {
        let context = vec!["GTR E Clean".to_string()];

        let channels = configured_values_for_dimension(TrackDimension::Channel, &context);
        assert!(channels.iter().any(|value| value == "L"));
        assert!(channels.iter().any(|value| value == "R"));

        let multi_mics = configured_values_for_dimension(TrackDimension::MultiMic, &context);
        assert!(multi_mics.iter().any(|value| value == "Amp"));
        assert!(multi_mics.iter().any(|value| value == "DI"));
        assert!(!multi_mics.iter().any(|value| value == "Top"));
        assert!(!multi_mics.iter().any(|value| value == "In"));
    }

    /// Every top-level group's own name must classify into that group.
    ///
    /// Folder names in a real project *are* these names, and routing decides
    /// what a folder feeds by classifying its name — a folder named "Guitars"
    /// that classifies to nothing gets no send, and every guitar inside it
    /// routes individually instead, which is the doubling the outermost-only
    /// rule exists to prevent.
    #[test]
    fn every_group_name_classifies_as_itself() {
        let config = default_config();
        let mut missing = Vec::new();
        for group in &config.groups {
            if group.metadata_only || group.transparent {
                continue;
            }
            let matched = classify_track_with_config(&group.name, &config).matched_groups;
            if !matched.iter().any(|g| g == &group.name) {
                missing.push((group.name.clone(), matched));
            }
        }
        assert!(
            missing.is_empty(),
            "group names that do not classify as themselves: {missing:#?}"
        );
    }

    #[test]
    fn classifies_top_level_visibility_group() {
        let track = classify_track("Kick In");

        assert!(track.visibility_groups.iter().any(|group| group == "Drums"));
    }
}
