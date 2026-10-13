//! Pre-computed summary and index over a [`ReaperProject`].
//!
//! Built once in O(n), all queries are O(1). Use [`ProjectIndex::build`] to
//! create an index from a typed project, then access counts and inventories
//! without repeated traversals.
//!
//! # Example
//!
//! ```no_run
//! use dawfile_reaper::{parse_rpp_file, ReaperProject, ProjectIndex};
//!
//! let content = std::fs::read_to_string("project.RPP").unwrap();
//! let parsed = parse_rpp_file(&content).unwrap();
//! let project = ReaperProject::from_rpp_project(&parsed).unwrap();
//! let index = ProjectIndex::build(&project);
//!
//! println!("{}", index.summary());
//! println!("MIDI sources: {}", index.source_count("MIDI"));
//! println!("Unique plugins: {:?}", index.plugin_inventory());
//! ```

use std::collections::HashMap;
use std::fmt;

use crate::types::{Envelope, FxChain, FxChainNode, Item, ReaperProject, Track};

// ─── ProjectIndex ────────────────────────────────────────────────────────────

/// Pre-computed summary and index over a [`ReaperProject`].
///
/// All fields are `pub` for direct access. Helper methods provide
/// typed lookups for common queries.
#[derive(Debug, Clone)]
pub struct ProjectIndex {
    // ── Counts ──
    pub track_count: u32,
    pub item_count: u32,
    pub take_count: u32,
    pub envelope_count: u32,
    pub marker_count: u32,
    pub region_count: u32,
    pub tempo_point_count: u32,
    pub midi_event_count: u32,
    pub midi_extended_event_count: u32,

    // ── Inventories (string keys to avoid modifying existing types) ──
    pub sources_by_type: HashMap<String, u32>,
    pub plugins_by_type: HashMap<String, u32>,
    pub plugin_names: Vec<String>,
    pub envelopes_by_param: HashMap<String, u32>,

    // ── FX summary ──
    pub fx_chain_count: u32,
    pub fx_plugin_count: u32,
    pub fx_container_count: u32,
}

impl ProjectIndex {
    /// Build an index from a typed [`ReaperProject`].
    ///
    /// Single-pass O(n) over all tracks, items, FX chains, envelopes,
    /// markers/regions, and tempo points.
    pub fn build(project: &ReaperProject) -> Self {
        let mut idx = Self {
            track_count: 0,
            item_count: 0,
            take_count: 0,
            envelope_count: 0,
            marker_count: 0,
            region_count: 0,
            tempo_point_count: 0,
            midi_event_count: 0,
            midi_extended_event_count: 0,
            sources_by_type: HashMap::new(),
            plugins_by_type: HashMap::new(),
            plugin_names: Vec::new(),
            envelopes_by_param: HashMap::new(),
            fx_chain_count: 0,
            fx_plugin_count: 0,
            fx_container_count: 0,
        };

        // ── Tracks ──
        idx.track_count = project.tracks.len() as u32;
        for track in &project.tracks {
            idx.index_track(track);
        }

        // ── Project-level items ──
        for item in &project.items {
            idx.index_item(item);
        }

        // ── Project-level envelopes ──
        for env in &project.envelopes {
            idx.index_envelope(env);
        }

        // ── Project-level FX chains ──
        for chain in &project.fx_chains {
            idx.index_fx_chain(chain);
        }

        // ── Markers and regions ──
        idx.marker_count = project.markers_regions.markers.len() as u32;
        idx.region_count = project.markers_regions.regions.len() as u32;

        // ── Tempo envelope ──
        if let Some(ref tempo) = project.tempo_envelope {
            idx.tempo_point_count = tempo.points.len() as u32;
        }

        // ── Finalize plugin inventory ──
        idx.plugin_names.sort();
        idx.plugin_names.dedup();

        idx
    }

    /// Total items across all tracks + project-level items.
    pub fn total_items(&self) -> u32 {
        self.item_count
    }

    /// Total takes across all items.
    pub fn total_takes(&self) -> u32 {
        self.take_count
    }

    /// Count of a specific source type (e.g. "MIDI", "WAVE").
    pub fn source_count(&self, source_type: &str) -> u32 {
        self.sources_by_type
            .get(&source_type.to_uppercase())
            .copied()
            .unwrap_or(0)
    }

    /// Count of a specific plugin type (e.g. "VST", "AU", "JS", "CLAP").
    pub fn plugin_count(&self, plugin_type: &str) -> u32 {
        self.plugins_by_type
            .get(&plugin_type.to_uppercase())
            .copied()
            .unwrap_or(0)
    }

    /// All unique plugin names, sorted alphabetically.
    pub fn plugin_inventory(&self) -> &[String] {
        &self.plugin_names
    }

    /// Human-readable summary for logging/display.
    pub fn summary(&self) -> ProjectSummary {
        ProjectSummary {
            tracks: self.track_count,
            items: self.item_count,
            takes: self.take_count,
            markers: self.marker_count,
            regions: self.region_count,
            tempo_points: self.tempo_point_count,
            envelopes: self.envelope_count,
            plugins: self.fx_plugin_count,
            containers: self.fx_container_count,
            midi_events: self.midi_event_count,
        }
    }

    // ── Private helpers ──────────────────────────────────────────────

    fn index_track(&mut self, track: &Track) {
        for item in &track.items {
            self.index_item(item);
        }
        for env in &track.envelopes {
            self.index_envelope(env);
        }
        if let Some(ref chain) = track.fx_chain {
            self.index_fx_chain(chain);
        }
        if let Some(ref chain) = track.input_fx {
            self.index_fx_chain(chain);
        }
    }

    fn index_item(&mut self, item: &Item) {
        self.item_count += 1;
        self.take_count += item.takes.len() as u32;

        for take in &item.takes {
            if let Some(ref source) = take.source {
                let type_key = format!("{:?}", source.source_type).to_uppercase();
                *self.sources_by_type.entry(type_key).or_insert(0) += 1;

                if let Some(ref midi) = source.midi_data {
                    self.midi_event_count += midi.events.len() as u32;
                    self.midi_extended_event_count += midi.extended_events.len() as u32;
                }
            }
        }
    }

    fn index_envelope(&mut self, env: &Envelope) {
        self.envelope_count += 1;
        *self
            .envelopes_by_param
            .entry(env.envelope_type.clone())
            .or_insert(0) += 1;
    }

    fn index_fx_chain(&mut self, chain: &FxChain) {
        self.fx_chain_count += 1;
        for node in &chain.nodes {
            self.index_fx_node(node);
        }
    }

    fn index_fx_node(&mut self, node: &FxChainNode) {
        match node {
            FxChainNode::Plugin(plugin) => {
                self.fx_plugin_count += 1;
                let type_key = format!("{:?}", plugin.plugin_type).to_uppercase();
                *self.plugins_by_type.entry(type_key).or_insert(0) += 1;
                self.plugin_names.push(plugin.name.clone());
            }
            FxChainNode::Container(container) => {
                self.fx_container_count += 1;
                for child in &container.children {
                    self.index_fx_node(child);
                }
            }
        }
    }
}

// ─── ProjectSummary ──────────────────────────────────────────────────────────

/// Human-readable project summary for logging and display.
#[derive(Debug, Clone, Copy)]
pub struct ProjectSummary {
    pub tracks: u32,
    pub items: u32,
    pub takes: u32,
    pub markers: u32,
    pub regions: u32,
    pub tempo_points: u32,
    pub envelopes: u32,
    pub plugins: u32,
    pub containers: u32,
    pub midi_events: u32,
}

impl fmt::Display for ProjectSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Project: {} tracks, {} items ({} takes), {} envelopes, \
             {} markers, {} regions, {} tempo pts, {} plugins ({} containers), \
             {} MIDI events",
            self.tracks,
            self.items,
            self.takes,
            self.envelopes,
            self.markers,
            self.regions,
            self.tempo_points,
            self.plugins,
            self.containers,
            self.midi_events,
        )
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_project() -> ReaperProject {
        ReaperProject {
            version: 0.1,
            version_string: "test".to_string(),
            timestamp: 0,
            properties: Default::default(),
            tracks: Vec::new(),
            items: Vec::new(),
            envelopes: Vec::new(),
            fx_chains: Vec::new(),
            markers_regions: Default::default(),
            tempo_envelope: None,
            ruler_lanes: Vec::new(),
            ruler_height: None,
        }
    }

    #[test]
    fn empty_project_all_zeros() {
        let idx = ProjectIndex::build(&empty_project());
        assert_eq!(idx.track_count, 0);
        assert_eq!(idx.item_count, 0);
        assert_eq!(idx.take_count, 0);
        assert_eq!(idx.envelope_count, 0);
        assert_eq!(idx.marker_count, 0);
        assert_eq!(idx.region_count, 0);
        assert_eq!(idx.tempo_point_count, 0);
        assert_eq!(idx.midi_event_count, 0);
        assert_eq!(idx.fx_plugin_count, 0);
        assert_eq!(idx.fx_container_count, 0);
        assert!(idx.plugin_names.is_empty());
        assert!(idx.sources_by_type.is_empty());
    }

    #[test]
    fn summary_display() {
        let idx = ProjectIndex::build(&empty_project());
        let s = idx.summary().to_string();
        assert!(s.contains("0 tracks"));
        assert!(s.contains("0 items"));
    }

    #[test]
    fn source_count_case_insensitive() {
        let idx = ProjectIndex::build(&empty_project());
        assert_eq!(idx.source_count("midi"), 0);
        assert_eq!(idx.source_count("MIDI"), 0);
    }

    #[test]
    fn plugin_count_case_insensitive() {
        let idx = ProjectIndex::build(&empty_project());
        assert_eq!(idx.plugin_count("vst"), 0);
        assert_eq!(idx.plugin_count("VST"), 0);
    }
}
