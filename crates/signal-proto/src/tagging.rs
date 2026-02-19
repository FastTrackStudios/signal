//! Structured tagging + browser semantics for Signal domain entities.
//!
//! This module keeps storage compatibility with flat `Metadata.tags` strings,
//! while providing a typed layer for:
//! - semantic browser columns
//! - weighted matching and fallback lookups
//! - category-aware tag parsing/encoding

use facet::Facet;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::metadata::Tags;
use crate::rig::RigType;

// ─── Taxonomy ───────────────────────────────────────────────────

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Facet,
)]
#[repr(C)]
pub enum TagCategory {
    RigType,
    EngineType,
    DomainLevel,
    Instrument,
    Tone,
    Character,
    Genre,
    Context,
    Module,
    Block,
    Vendor,
    Plugin,
    Workflow,
    Custom,
}

impl TagCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RigType => "rig_type",
            Self::EngineType => "engine_type",
            Self::DomainLevel => "domain_level",
            Self::Instrument => "instrument",
            Self::Tone => "tone",
            Self::Character => "character",
            Self::Genre => "genre",
            Self::Context => "context",
            Self::Module => "module",
            Self::Block => "block",
            Self::Vendor => "vendor",
            Self::Plugin => "plugin",
            Self::Workflow => "workflow",
            Self::Custom => "custom",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "rig_type" | "rigtype" => Some(Self::RigType),
            "engine_type" | "enginetype" => Some(Self::EngineType),
            "domain_level" | "domainlevel" => Some(Self::DomainLevel),
            "instrument" => Some(Self::Instrument),
            "tone" => Some(Self::Tone),
            "character" => Some(Self::Character),
            "genre" => Some(Self::Genre),
            "context" => Some(Self::Context),
            "module" => Some(Self::Module),
            "block" => Some(Self::Block),
            "vendor" => Some(Self::Vendor),
            "plugin" => Some(Self::Plugin),
            "workflow" => Some(Self::Workflow),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Facet, Default)]
#[repr(C)]
pub enum TagSource {
    #[default]
    Manual,
    InferredName,
    InferredStructure,
    Imported,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Facet)]
pub struct StructuredTag {
    pub category: TagCategory,
    pub value: String,
    pub source: TagSource,
    pub weight: u8,
}

impl StructuredTag {
    pub fn new(category: TagCategory, value: impl Into<String>) -> Self {
        Self {
            category,
            value: normalize_value(value.into()),
            source: TagSource::Manual,
            weight: default_weight(category),
        }
    }

    #[must_use]
    pub fn with_source(mut self, source: TagSource) -> Self {
        self.source = source;
        self
    }

    #[must_use]
    pub fn with_weight(mut self, weight: u8) -> Self {
        self.weight = weight;
        self
    }

    pub fn key(&self) -> String {
        format!("{}:{}", self.category.as_str(), self.value)
    }

    pub fn encode(&self) -> String {
        self.key()
    }

    pub fn parse(raw: &str) -> Self {
        let raw = raw.trim();
        let Some((left, right)) = raw.split_once(':') else {
            return Self::new(TagCategory::Custom, raw).with_source(TagSource::Imported);
        };
        let Some(category) = TagCategory::parse(left) else {
            return Self::new(TagCategory::Custom, raw).with_source(TagSource::Imported);
        };
        Self::new(category, right).with_source(TagSource::Imported)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Facet, Default)]
pub struct TagSet(BTreeMap<String, StructuredTag>);

impl TagSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_tags(tags: &Tags) -> Self {
        let mut set = Self::new();
        for raw in tags.as_slice() {
            set.insert(StructuredTag::parse(raw));
        }
        set
    }

    pub fn to_tags(&self) -> Tags {
        Tags::from_vec(self.0.values().map(StructuredTag::encode).collect())
    }

    pub fn insert(&mut self, tag: StructuredTag) {
        self.0.insert(tag.key(), tag);
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    pub fn values(&self) -> impl Iterator<Item = &StructuredTag> {
        self.0.values()
    }

    pub fn by_category(&self, category: TagCategory) -> Vec<&StructuredTag> {
        self.0
            .values()
            .filter(|t| t.category == category)
            .collect::<Vec<_>>()
    }

    pub fn merge(&mut self, other: &TagSet) {
        for tag in other.values() {
            self.insert(tag.clone());
        }
    }

    pub fn weighted_overlap(&self, other: &TagSet, weights: &TagWeights) -> f32 {
        let mut score = 0.0;
        for tag in self.values() {
            if other.contains_key(&tag.key()) {
                score += weights.weight_for(tag.category) * f32::from(tag.weight);
            }
        }
        score
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct TagWeights(BTreeMap<TagCategory, f32>);

impl Default for TagWeights {
    fn default() -> Self {
        let mut out = BTreeMap::new();
        out.insert(TagCategory::RigType, 4.0);
        out.insert(TagCategory::EngineType, 3.5);
        out.insert(TagCategory::DomainLevel, 2.5);
        out.insert(TagCategory::Instrument, 3.0);
        out.insert(TagCategory::Tone, 3.0);
        out.insert(TagCategory::Character, 2.5);
        out.insert(TagCategory::Genre, 2.0);
        out.insert(TagCategory::Context, 1.5);
        out.insert(TagCategory::Module, 2.0);
        out.insert(TagCategory::Block, 2.0);
        out.insert(TagCategory::Vendor, 1.25);
        out.insert(TagCategory::Plugin, 1.25);
        out.insert(TagCategory::Workflow, 1.0);
        out.insert(TagCategory::Custom, 0.5);
        Self(out)
    }
}

impl TagWeights {
    pub fn weight_for(&self, category: TagCategory) -> f32 {
        *self.0.get(&category).unwrap_or(&1.0)
    }
}

// ─── Browser semantics ─────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Facet, Default)]
#[repr(C)]
pub enum BrowserMode {
    #[default]
    Semantic,
    Vendor,
    Genre,
    Performance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum BrowserEntityKind {
    BlockCollection,
    BlockVariant,
    ModuleCollection,
    ModuleVariant,
    LayerCollection,
    LayerVariant,
    EngineCollection,
    EngineVariant,
    RigCollection,
    RigVariant,
    ProfileCollection,
    ProfileVariant,
    SongCollection,
    SongVariant,
    SetlistCollection,
    SetlistVariant,
}

pub fn browser_columns(mode: BrowserMode, rig_type: Option<RigType>) -> &'static [TagCategory] {
    match (mode, rig_type) {
        (BrowserMode::Semantic, Some(RigType::Keys)) => &[
            TagCategory::Instrument,
            TagCategory::EngineType,
            TagCategory::Character,
            TagCategory::Context,
        ],
        (BrowserMode::Semantic, Some(RigType::Vocals)) => &[
            TagCategory::EngineType,
            TagCategory::Module,
            TagCategory::Character,
            TagCategory::Context,
        ],
        (BrowserMode::Semantic, _) => &[
            TagCategory::Tone,
            TagCategory::Genre,
            TagCategory::Character,
            TagCategory::Module,
        ],
        (BrowserMode::Vendor, _) => &[TagCategory::Vendor, TagCategory::Plugin, TagCategory::Tone],
        (BrowserMode::Genre, _) => &[
            TagCategory::Genre,
            TagCategory::Tone,
            TagCategory::Character,
            TagCategory::Context,
        ],
        (BrowserMode::Performance, _) => &[
            TagCategory::Context,
            TagCategory::Workflow,
            TagCategory::Character,
            TagCategory::Tone,
        ],
    }
}

pub fn fallback_categories(
    kind: BrowserEntityKind,
    rig_type: Option<RigType>,
) -> &'static [TagCategory] {
    match kind {
        BrowserEntityKind::BlockCollection | BrowserEntityKind::BlockVariant => &[
            TagCategory::Block,
            TagCategory::Tone,
            TagCategory::Character,
        ],
        BrowserEntityKind::ModuleCollection | BrowserEntityKind::ModuleVariant => &[
            TagCategory::Module,
            TagCategory::Tone,
            TagCategory::Character,
        ],
        BrowserEntityKind::LayerCollection | BrowserEntityKind::LayerVariant => &[
            TagCategory::EngineType,
            TagCategory::Tone,
            TagCategory::Context,
        ],
        BrowserEntityKind::EngineCollection | BrowserEntityKind::EngineVariant => &[
            TagCategory::EngineType,
            TagCategory::Tone,
            TagCategory::Character,
        ],
        BrowserEntityKind::RigCollection | BrowserEntityKind::RigVariant => match rig_type {
            Some(RigType::Keys) => &[
                TagCategory::RigType,
                TagCategory::Instrument,
                TagCategory::Character,
            ],
            _ => &[
                TagCategory::RigType,
                TagCategory::Tone,
                TagCategory::Character,
            ],
        },
        BrowserEntityKind::ProfileCollection | BrowserEntityKind::ProfileVariant => &[
            TagCategory::RigType,
            TagCategory::Context,
            TagCategory::Character,
        ],
        BrowserEntityKind::SongCollection | BrowserEntityKind::SongVariant => &[
            TagCategory::Context,
            TagCategory::Genre,
            TagCategory::Character,
        ],
        BrowserEntityKind::SetlistCollection | BrowserEntityKind::SetlistVariant => &[
            TagCategory::Context,
            TagCategory::RigType,
            TagCategory::Workflow,
        ],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Facet)]
pub struct BrowserNodeId {
    pub kind: BrowserEntityKind,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct BrowserEntry {
    pub node: BrowserNodeId,
    pub name: String,
    pub tags: TagSet,
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct BrowserQuery {
    pub mode: BrowserMode,
    pub rig_type: Option<RigType>,
    pub strict_rig_type: bool,
    pub kinds: Vec<BrowserEntityKind>,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub text: Option<String>,
}

impl Default for BrowserQuery {
    fn default() -> Self {
        Self {
            mode: BrowserMode::Semantic,
            rig_type: None,
            strict_rig_type: false,
            kinds: Vec::new(),
            include: Vec::new(),
            exclude: Vec::new(),
            text: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct BrowserHit {
    pub node: BrowserNodeId,
    pub score: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet, Default)]
pub struct BrowserIndex {
    entries: Vec<BrowserEntry>,
}

impl BrowserIndex {
    pub fn with_entries(entries: Vec<BrowserEntry>) -> Self {
        Self { entries }
    }

    pub fn entries(&self) -> &[BrowserEntry] {
        &self.entries
    }

    pub fn push(&mut self, entry: BrowserEntry) {
        self.entries.push(entry);
    }

    pub fn query(&self, query: &BrowserQuery, weights: &TagWeights) -> Vec<BrowserHit> {
        let text = query.text.as_ref().map(|s| s.to_ascii_lowercase());
        let mut include = TagSet::new();
        for tag in &query.include {
            include.insert(StructuredTag::parse(tag));
        }
        let mut exclude_keys = BTreeSet::new();
        for tag in &query.exclude {
            exclude_keys.insert(StructuredTag::parse(tag).key());
        }
        let strict_rig_tag = query.rig_type.map(|r| format!("rig_type:{}", r.as_str()));

        let mut hits = Vec::new();
        for e in &self.entries {
            if !query.kinds.is_empty() && !query.kinds.contains(&e.node.kind) {
                continue;
            }
            if !exclude_keys.is_empty() && e.tags.values().any(|t| exclude_keys.contains(&t.key()))
            {
                continue;
            }
            if query.strict_rig_type
                && strict_rig_tag
                    .as_ref()
                    .is_some_and(|k| !e.tags.contains_key(k))
            {
                continue;
            }

            if let Some(t) = &text {
                let name_match = e.name.to_ascii_lowercase().contains(t);
                let alias_match = e.aliases.iter().any(|a| a.to_ascii_lowercase().contains(t));
                if !name_match && !alias_match {
                    continue;
                }
            }

            let mut score = 0.0;
            score += e.tags.weighted_overlap(&include, weights);

            // Fallback boost: if no explicit include tags, reward fallback category coverage.
            if query.include.is_empty() {
                for c in fallback_categories(e.node.kind, query.rig_type) {
                    if !e.tags.by_category(*c).is_empty() {
                        score += weights.weight_for(*c);
                    }
                }
            }

            // Mode boost: entries aligned with visible columns rank higher.
            for c in browser_columns(query.mode, query.rig_type) {
                if !e.tags.by_category(*c).is_empty() {
                    score += 0.25 * weights.weight_for(*c);
                }
            }

            hits.push(BrowserHit {
                node: e.node.clone(),
                score,
            });
        }

        hits.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| a.node.id.cmp(&b.node.id))
        });
        hits
    }
}

// ─── Inference ──────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet, Default)]
pub struct TagInferenceHints {
    pub rig_type: Option<RigType>,
    pub section: Option<String>,
    pub arrangement: Option<String>,
    pub effects: Vec<String>,
    pub group_path: Vec<String>,
    pub performer: Option<String>,
}

pub fn infer_tags_from_hints(hints: &TagInferenceHints) -> TagSet {
    let mut out = TagSet::new();

    if let Some(rig_type) = hints.rig_type {
        out.insert(StructuredTag::new(TagCategory::RigType, rig_type.as_str()));
    }

    if let Some(section) = &hints.section {
        out.insert(
            StructuredTag::new(TagCategory::Context, section)
                .with_source(TagSource::InferredStructure),
        );
    }

    if let Some(arrangement) = &hints.arrangement {
        out.insert(
            StructuredTag::new(TagCategory::Workflow, arrangement)
                .with_source(TagSource::InferredStructure),
        );
    }

    for fx in &hints.effects {
        out.insert(
            StructuredTag::new(TagCategory::Block, fx).with_source(TagSource::InferredStructure),
        );
    }

    for group in &hints.group_path {
        out.insert(
            StructuredTag::new(TagCategory::Instrument, group)
                .with_source(TagSource::InferredStructure),
        );
    }

    if let Some(performer) = &hints.performer {
        out.insert(
            StructuredTag::new(TagCategory::Custom, performer).with_source(TagSource::Imported),
        );
    }

    out
}

pub fn infer_tags_from_name(name: &str) -> TagSet {
    let mut out = TagSet::new();
    let lower = name.to_ascii_lowercase();
    let words = lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();

    for w in words {
        match w {
            "clean" | "crunch" | "drive" | "lead" | "ambient" => {
                out.insert(
                    StructuredTag::new(TagCategory::Tone, w).with_source(TagSource::InferredName),
                );
            }
            "warm" | "bright" | "dark" | "smooth" | "aggressive" | "airy" => {
                out.insert(
                    StructuredTag::new(TagCategory::Character, w)
                        .with_source(TagSource::InferredName),
                );
            }
            "intro" | "verse" | "chorus" | "bridge" | "outro" | "solo" | "hook" => {
                out.insert(
                    StructuredTag::new(TagCategory::Context, w)
                        .with_source(TagSource::InferredName),
                );
            }
            "worship" | "rock" | "metal" | "jazz" | "country" | "pop" => {
                out.insert(
                    StructuredTag::new(TagCategory::Genre, w).with_source(TagSource::InferredName),
                );
            }
            "compressor" | "eq" | "gate" | "deesser" | "delay" | "reverb" | "flanger"
            | "phaser" | "tremolo" | "saturator" | "amp" => {
                out.insert(
                    StructuredTag::new(TagCategory::Block, w).with_source(TagSource::InferredName),
                );
            }
            _ => {}
        }
    }

    out
}

fn normalize_value(raw: String) -> String {
    raw.trim().to_ascii_lowercase().replace(' ', "_")
}

const fn default_weight(category: TagCategory) -> u8 {
    match category {
        TagCategory::RigType => 9,
        TagCategory::EngineType => 8,
        TagCategory::DomainLevel => 6,
        TagCategory::Instrument => 7,
        TagCategory::Tone => 7,
        TagCategory::Character => 6,
        TagCategory::Genre => 5,
        TagCategory::Context => 4,
        TagCategory::Module => 6,
        TagCategory::Block => 6,
        TagCategory::Vendor => 3,
        TagCategory::Plugin => 3,
        TagCategory::Workflow => 2,
        TagCategory::Custom => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_tag_parse_handles_namespaced_and_raw() {
        let namespaced = StructuredTag::parse("tone:clean");
        assert_eq!(namespaced.category, TagCategory::Tone);
        assert_eq!(namespaced.value, "clean");

        let raw = StructuredTag::parse("Warm");
        assert_eq!(raw.category, TagCategory::Custom);
        assert_eq!(raw.value, "warm");
    }

    #[test]
    fn tagset_roundtrip_uses_flat_strings() {
        let mut set = TagSet::new();
        set.insert(StructuredTag::new(TagCategory::Tone, "clean"));
        set.insert(StructuredTag::new(TagCategory::Genre, "worship"));
        let flat = set.to_tags();
        assert!(flat.contains("tone:clean"));
        assert!(flat.contains("genre:worship"));
    }

    #[test]
    fn browser_index_ranks_by_overlap() {
        let mut clean_tags = TagSet::new();
        clean_tags.insert(StructuredTag::new(TagCategory::Tone, "clean"));
        clean_tags.insert(StructuredTag::new(TagCategory::RigType, "keys"));
        let clean = BrowserEntry {
            node: BrowserNodeId {
                kind: BrowserEntityKind::RigVariant,
                id: "clean".into(),
            },
            name: "Clean Keys".into(),
            tags: clean_tags,
            aliases: vec![],
        };

        let mut drive_tags = TagSet::new();
        drive_tags.insert(StructuredTag::new(TagCategory::Tone, "drive"));
        drive_tags.insert(StructuredTag::new(TagCategory::RigType, "keys"));
        let drive = BrowserEntry {
            node: BrowserNodeId {
                kind: BrowserEntityKind::RigVariant,
                id: "drive".into(),
            },
            name: "Driven Keys".into(),
            tags: drive_tags,
            aliases: vec![],
        };

        let index = BrowserIndex::with_entries(vec![drive, clean]);
        let query = BrowserQuery {
            include: vec!["tone:clean".into()],
            ..BrowserQuery::default()
        };
        let hits = index.query(&query, &TagWeights::default());
        assert_eq!(hits.first().map(|h| h.node.id.as_str()), Some("clean"));
    }

    #[test]
    fn browser_query_strict_rig_type_filters_results() {
        let mut keys_tags = TagSet::new();
        keys_tags.insert(StructuredTag::new(TagCategory::RigType, "keys"));
        let keys = BrowserEntry {
            node: BrowserNodeId {
                kind: BrowserEntityKind::RigVariant,
                id: "keys-rig".into(),
            },
            name: "Keys Rig".into(),
            tags: keys_tags,
            aliases: vec![],
        };

        let mut guitar_tags = TagSet::new();
        guitar_tags.insert(StructuredTag::new(TagCategory::RigType, "guitar"));
        let guitar = BrowserEntry {
            node: BrowserNodeId {
                kind: BrowserEntityKind::RigVariant,
                id: "guitar-rig".into(),
            },
            name: "Guitar Rig".into(),
            tags: guitar_tags,
            aliases: vec![],
        };

        let index = BrowserIndex::with_entries(vec![guitar, keys]);
        let query = BrowserQuery {
            rig_type: Some(RigType::Keys),
            strict_rig_type: true,
            ..BrowserQuery::default()
        };

        let hits = index.query(&query, &TagWeights::default());
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].node.id, "keys-rig");
    }

    #[test]
    fn browser_query_kinds_filters_entity_types() {
        let mut tags = TagSet::new();
        tags.insert(StructuredTag::new(TagCategory::RigType, "keys"));
        let setlist = BrowserEntry {
            node: BrowserNodeId {
                kind: BrowserEntityKind::SetlistCollection,
                id: "setlist-1".into(),
            },
            name: "Setlist".into(),
            tags: tags.clone(),
            aliases: vec![],
        };
        let rig = BrowserEntry {
            node: BrowserNodeId {
                kind: BrowserEntityKind::RigCollection,
                id: "rig-1".into(),
            },
            name: "Rig".into(),
            tags,
            aliases: vec![],
        };

        let index = BrowserIndex::with_entries(vec![setlist, rig]);
        let query = BrowserQuery {
            kinds: vec![BrowserEntityKind::SetlistCollection],
            ..BrowserQuery::default()
        };

        let hits = index.query(&query, &TagWeights::default());
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].node.id, "setlist-1");
    }

    #[test]
    fn inference_from_hints_maps_to_structured_categories() {
        let hints = TagInferenceHints {
            rig_type: Some(RigType::Keys),
            section: Some("Chorus".into()),
            arrangement: Some("Build".into()),
            effects: vec!["Reverb".into()],
            group_path: vec!["Keys".into(), "Pad".into()],
            performer: Some("Cody".into()),
        };
        let set = infer_tags_from_hints(&hints);
        assert!(!set.by_category(TagCategory::RigType).is_empty());
        assert!(!set.by_category(TagCategory::Context).is_empty());
        assert!(!set.by_category(TagCategory::Workflow).is_empty());
        assert!(!set.by_category(TagCategory::Block).is_empty());
        assert!(!set.by_category(TagCategory::Instrument).is_empty());
    }
}
