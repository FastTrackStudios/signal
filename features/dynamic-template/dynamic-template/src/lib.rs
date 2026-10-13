use daw_proto::{FolderDepthChange, TrackHierarchy, TrackNode};
#[cfg(test)]
use monarchy::StructureAssertions;
use monarchy::{
    cleanup_display_names, collapse_single_child_folders, expand_items_to_children, Config,
    Metadata, Target, ToDisplayName,
};

// region: --- Modules

pub mod apply;
pub mod buses;
pub mod colors;
#[cfg(feature = "reaper")]
pub mod daw_module;
pub mod equipment;
mod error;
pub mod golden;
pub mod golden_session;
pub mod grouping;
mod groups;
pub mod icons;
mod item_metadata;
pub mod layouts;
mod metadata_patterns;
pub mod protools;
pub mod scenes;
pub mod song_name;
pub mod source_defaults;
mod tempo;
pub mod track_schema;
pub mod visibility;

pub use apply::{apply_buses, route_to_bus, AppliedBuses, TemplateTarget};
pub use buses::{bus_for_path, bus_nodes, bus_nodes_for_paths, buses_for_paths, BusSpec, BUS_TREE};
pub use error::{Error, Result};
pub use golden::golden_template;
pub use groups::{
    Bass, Choir, Drums, Guide, Guitars, Harmonica, Headphones, Horns, Keys, Orchestra, Percussion,
    Reference, StemSplit, Strings, Synths, Talkback, Tracks, Vca, Vocals, SFX,
};
pub use item_metadata::ItemMetadata;

// Re-export monarchy types needed for direct classification
pub use groups::stem_split::{is_stem_split_set, stem_category, stem_source};
pub use monarchy::{monarchy_sort, Structure};
pub use protools::{extract_protools_metadata, strip_protools_markers, ProToolsMetadata};
pub use song_name::{
    detect_song_names, detect_song_names_with_config, strip_song_names, SongNameConfig,
};
pub use tempo::{extract_tempo, strip_tempo};

// endregion: --- Modules

/// Type alias for our standard Config with `ItemMetadata`
pub type DynamicTemplateConfig = Config<ItemMetadata>;

/// Dynamic template system for organizing DAW items
#[derive(Default)]
pub struct DynamicTemplate;

impl DynamicTemplate {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

/// [`default_config`], built once and shared.
///
/// Building it constructs every group and its patterns, and the hot
/// paths — reading a track's name for its dimensions, once per track per
/// plan — asked for a fresh one each time: a mixer of 234 strips spent
/// ~100 ms of its opening frame rebuilding the same config.
#[must_use]
pub fn shared_config() -> &'static DynamicTemplateConfig {
    static CONFIG: std::sync::OnceLock<DynamicTemplateConfig> = std::sync::OnceLock::new();
    CONFIG.get_or_init(default_config)
}

/// Creates a default configuration for the dynamic template system
#[must_use]
pub fn default_config() -> DynamicTemplateConfig {
    Config::builder()
        // Add metadata field patterns first (metadata-only group)
        .group(metadata_patterns::default_metadata_field_patterns())
        // Talkback first: monarchy takes the first matching group, and a
        // talkback mic is named after whoever it belongs to ("TB Bass",
        // "TB Drums"). Registered after the instruments, every one of those
        // matches the instrument it names and the chatter lands in the mix.
        .group(Talkback)
        // VCAs are control tracks, not audio; recognising them early keeps a
        // "Drums VCA" from classifying as drums.
        .group(Vca)
        // Then add regular groups
        .group(Drums)
        .group(Percussion)
        .group(Bass)
        .group(Guitars)
        .group(Keys)
        .group(Synths)
        .group(Horns)
        .group(Harmonica)
        .group(Strings)
        .group(Vocals)
        // Top-level Choir catches standalone voice-part choir sessions
        // (soprano/alto/tenor, "chorus", ensembles). Literal "choir"/"chorale"
        // stems are handled by Vocals' Choir subgroup instead (routed to the
        // vocal bus) — the top-level group deliberately does NOT match those two
        // bare words, so a "Choir" stem lands under Vocals with no duplication.
        .group(Choir)
        .group(Orchestra)
        .group(SFX)
        // Backing/playback tracks (loops, sequences) — a live-rig top-level group.
        .group(Tracks)
        // Utility tracks at the bottom. Guide precedes Headphones so a
        // section-cue track reaches Guide rather than the cue-mix group.
        .group(Guide)
        .group(Headphones)
        // Reference carries Stem Split as a sub-group; a separation of the
        // finished record is reference material, not tracked parts.
        .group(Reference)
        .build()
}

/// Options for organizing items into tracks
#[derive(Clone)]
pub struct OrganizeOptions {
    /// Expand multiple items on a node into individual child tracks
    pub expand_items: bool,
    /// Clean up display names by stripping redundant context
    pub cleanup_names: bool,
    /// Collapse single-child intermediate folders (e.g., Cymbals/OH -> OH)
    pub collapse_single_child: bool,
}

impl Default for OrganizeOptions {
    /// Default options enable expansion, cleanup, and collapse
    fn default() -> Self {
        Self {
            expand_items: true,
            cleanup_names: true,
            collapse_single_child: true,
        }
    }
}

impl OrganizeOptions {
    /// Create options with no transformations (raw monarchy sort output)
    #[must_use]
    pub const fn none() -> Self {
        Self {
            expand_items: false,
            cleanup_names: false,
            collapse_single_child: false,
        }
    }
}

/// Trait for organizing item names into a `TrackHierarchy` using monarchy sort
///
/// This trait accepts anything that can be converted into item names (strings),
/// allowing you to pass strings directly.
pub trait OrganizeIntoTracks {
    /// Organize items into tracks using the provided configuration
    ///
    /// If `existing_tracks` is provided, items will be matched to existing tracks
    /// and new tracks will only be created when needed.
    ///
    /// # Errors
    ///
    /// Returns an error if the monarchy sort fails due to invalid configuration or input.
    fn organize_into_tracks(
        self,
        config: &DynamicTemplateConfig,
        existing_tracks: Option<&TrackHierarchy>,
    ) -> monarchy::Result<TrackHierarchy>;

    /// Organize items into tracks with additional options
    ///
    /// Use `OrganizeOptions::with_expansion()` to enable item expansion and name cleanup.
    ///
    /// # Errors
    ///
    /// Returns an error if the monarchy sort fails due to invalid configuration or input.
    fn organize_into_tracks_with_options(
        self,
        config: &DynamicTemplateConfig,
        existing_tracks: Option<&TrackHierarchy>,
        options: OrganizeOptions,
    ) -> monarchy::Result<TrackHierarchy>;
}

impl<T> OrganizeIntoTracks for Vec<T>
where
    T: Into<String>,
{
    fn organize_into_tracks(
        self,
        config: &DynamicTemplateConfig,
        existing_tracks: Option<&TrackHierarchy>,
    ) -> monarchy::Result<TrackHierarchy> {
        // Default: no expansion or cleanup (backwards compatible)
        self.organize_into_tracks_with_options(config, existing_tracks, OrganizeOptions::default())
    }

    fn organize_into_tracks_with_options(
        self,
        config: &DynamicTemplateConfig,
        existing_tracks: Option<&TrackHierarchy>,
        options: OrganizeOptions,
    ) -> monarchy::Result<TrackHierarchy> {
        // Convert input to strings
        let input_strings: Vec<String> = self.into_iter().map(std::convert::Into::into).collect();

        // Detect song/project names that appear in 80%+ of inputs
        let song_name_config = song_name::SongNameConfig::default().with_threshold(0.8);
        let detected_song_names =
            song_name::detect_song_names_with_config(&input_strings, &song_name_config);

        // Perform monarchy sort (clone config since monarchy_sort takes ownership)
        let mut structure = monarchy_sort(input_strings, config)?;

        // Extract tempo from item names and store in metadata
        extract_tempo_from_structure(&mut structure);

        // Apply optional transformations
        if options.collapse_single_child {
            collapse_single_child_folders(&mut structure);
        }
        if options.expand_items {
            expand_items_to_children(&mut structure);
        }
        if options.cleanup_names {
            cleanup_display_names(&mut structure);
            // Strip detected song names from display names (after standard cleanup)
            if !detected_song_names.is_empty() {
                strip_song_names_from_structure(&mut structure, &detected_song_names);
            }
            // Strip tempo/BPM markers from display names
            strip_tempo_from_structure(&mut structure);
            // Strip Pro Tools playlist/take markers from display names
            strip_protools_from_structure(&mut structure);
            // Strip equipment names (mics, preamps, synth hardware) from display names
            strip_equipment_from_structure(&mut structure);
        }

        // Convert Structure to TrackHierarchy
        let mut hierarchy = structure_to_hierarchy(&structure);

        // Merge with existing tracks if provided
        if let Some(existing) = existing_tracks {
            hierarchy = merge_hierarchies(existing, hierarchy);
        }

        Ok(hierarchy)
    }
}

/// Implement Target trait for `TrackHierarchy` so monarchy can work with existing tracks
impl Target<ItemMetadata> for TrackHierarchy {
    fn existing_items(&self) -> Vec<monarchy::Item<ItemMetadata>> {
        // Extract items from existing tracks and convert to monarchy Items
        self.tracks
            .iter()
            .flat_map(|track| {
                track.items.iter().map(|item_name| monarchy::Item {
                    id: item_name.clone(),
                    original: item_name.clone(),
                    metadata: ItemMetadata::default(),
                    matched_groups: Vec::new(),
                })
            })
            .collect()
    }
}

/// Merge new hierarchy with existing hierarchy, matching by name and adding items to existing tracks
fn merge_hierarchies(existing: &TrackHierarchy, new: TrackHierarchy) -> TrackHierarchy {
    let mut result_tracks: Vec<TrackNode> = existing.tracks.clone();
    let mut existing_names: std::collections::HashSet<String> =
        existing.tracks.iter().map(|t| t.name.clone()).collect();

    for new_track in new.tracks {
        if existing_names.contains(&new_track.name) {
            // Find existing track and merge items
            if let Some(existing_track) =
                result_tracks.iter_mut().find(|t| t.name == new_track.name)
            {
                existing_track.items.extend(new_track.items);
            }
        } else {
            // New track, add it
            existing_names.insert(new_track.name.clone());
            result_tracks.push(new_track);
        }
    }

    TrackHierarchy {
        tracks: result_tracks,
    }
}

/// Trait for converting monarchy Structure to `TrackHierarchy`
///
/// This recursively converts the hierarchical Structure into a flat list of `TrackNodes`
/// where folder relationships are represented using `folder_depth_change`.
pub trait IntoTracks<M: Metadata> {
    /// Convert the Structure to a `TrackHierarchy`
    ///
    /// # Example
    /// ```ignore
    /// use dynamic_template::{default_config, IntoTracks};
    /// use monarchy::monarchy_sort;
    ///
    /// let structure = monarchy_sort(vec!["Kick In"], default_config())?;
    /// let hierarchy = structure.to_tracks();
    /// ```
    fn to_tracks(self) -> TrackHierarchy;
}

impl<M: Metadata + ToDisplayName> IntoTracks<M> for Structure<M> {
    fn to_tracks(self) -> TrackHierarchy {
        structure_to_hierarchy(&self)
    }
}

/// Convert a monarchy Structure to a `TrackHierarchy`
fn structure_to_hierarchy<M: Metadata + ToDisplayName>(structure: &Structure<M>) -> TrackHierarchy {
    let mut tracks = Vec::new();
    structure_to_tracks_recursive(structure, &mut tracks, true, &[]);
    TrackHierarchy { tracks }
}

/// Helper function to recursively convert Structure to `TrackNodes`
///
/// The `group_path` parameter tracks the hierarchy of group names from root to
/// current node, used for color lookup via `colors::color_for_path()`.
fn structure_to_tracks_recursive<M: Metadata + ToDisplayName>(
    structure: &Structure<M>,
    tracks: &mut Vec<TrackNode>,
    skip_root: bool,
    group_path: &[&str],
) {
    // Skip root if it's just a container (name is "root" or empty)
    if skip_root && (structure.name == "root" || structure.name.is_empty()) {
        // Process all children
        for child in &structure.children {
            structure_to_tracks_recursive(child, tracks, false, group_path);
        }
        return;
    }

    // Build the path for this node (used for color lookup)
    let mut current_path: Vec<&str> = group_path.to_vec();
    current_path.push(&structure.name);

    // Create a track node for this structure
    let mut track = TrackNode::new(structure.name.clone());

    // Look up color from the hierarchical path
    track.color = colors::color_for_path(&current_path).map(color_palette::Color::to_hex);

    // Add items from monarchy structure to the track
    for monarchy_item in &structure.items {
        track.items.push(monarchy_item.original.clone());
    }

    // If this structure has children, it's a folder
    if structure.children.is_empty() {
        // Leaf track
        tracks.push(track);
    } else {
        track.is_folder = true;
        track.folder_depth_change = FolderDepthChange::FolderStart;
        tracks.push(track);

        // Recursively convert children
        let num_children = structure.children.len();
        for (i, child) in structure.children.iter().enumerate() {
            let tracks_before = tracks.len();
            structure_to_tracks_recursive(child, tracks, false, &current_path);

            // If this is the last child, apply folder closing to the last track added
            if i.saturating_add(1) == num_children && tracks.len() > tracks_before {
                if let Some(last_track) = tracks.last_mut() {
                    let current = last_track.folder_depth_change.to_raw_value();
                    last_track.folder_depth_change =
                        FolderDepthChange::from_raw_value(current.saturating_sub(1));
                }
            }
        }
    }
}

/// Extract tempo from all items in a structure and store in their metadata
///
/// This function recursively traverses the structure and extracts tempo (BPM)
/// from each item's original name, storing it in the item's metadata.
fn extract_tempo_from_structure(structure: &mut Structure<ItemMetadata>) {
    // Extract tempo for items at this level
    for item in &mut structure.items {
        if item.metadata.tempo.is_none() {
            if let Some(tempo) = tempo::extract_tempo(&item.original) {
                item.metadata.tempo = Some(tempo);
            }
        }
    }

    // Recursively process children
    for child in &mut structure.children {
        extract_tempo_from_structure(child);
    }
}

/// Strip detected song names from structure display names
///
/// This function recursively traverses the structure and strips song/project names
/// from each node's display name. Song names are detected as strings that appear
/// in ALL input items and don't match known audio production patterns.
fn strip_song_names_from_structure(
    structure: &mut Structure<ItemMetadata>,
    song_names: &std::collections::HashSet<String>,
) {
    // Strip song names from this node's display name
    if !structure.name.is_empty() {
        let cleaned = song_name::strip_song_names(&structure.name, song_names);
        if !cleaned.is_empty() && cleaned != structure.name {
            structure.name = cleaned;
        }
    }

    // Recursively process children
    for child in &mut structure.children {
        strip_song_names_from_structure(child, song_names);
    }
}

/// Strip tempo/BPM markers from structure display names
///
/// This function recursively traverses the structure and strips tempo patterns
/// like "126bpm", "83.5BPM" from each node's display name.
fn strip_tempo_from_structure(structure: &mut Structure<ItemMetadata>) {
    // Strip tempo from this node's display name
    if !structure.name.is_empty() {
        let cleaned = tempo::strip_tempo(&structure.name);
        if !cleaned.is_empty() && cleaned != structure.name {
            structure.name = cleaned;
        }
    }

    // Recursively process children
    for child in &mut structure.children {
        strip_tempo_from_structure(child);
    }
}

/// Strip Pro Tools playlist/take markers from structure display names
///
/// This function recursively traverses the structure and strips Pro Tools
/// session metadata like "_01", ".01", "_01-02" from each node's display name.
fn strip_protools_from_structure(structure: &mut Structure<ItemMetadata>) {
    // Strip Pro Tools markers from this node's display name
    if !structure.name.is_empty() {
        let cleaned = protools::strip_protools_markers(&structure.name);
        if !cleaned.is_empty() && cleaned != structure.name {
            structure.name = cleaned;
        }
    }

    // Recursively process children
    for child in &mut structure.children {
        strip_protools_from_structure(child);
    }
}

/// Strip equipment names from structure display names
///
/// This function recursively traverses the structure and strips equipment-related
/// metadata like mic models (U47, SM57), preamp models (`OptoComp`, 1176),
/// processing markers (LIM, EDT), and synth hardware (CASIO, Roland FA06).
fn strip_equipment_from_structure(structure: &mut Structure<ItemMetadata>) {
    // Strip equipment from this node's display name
    if !structure.name.is_empty() {
        let cleaned = equipment::strip_equipment(&structure.name);
        if !cleaned.is_empty() && cleaned != structure.name {
            structure.name = cleaned;
        }
    }

    // Recursively process children
    for child in &mut structure.children {
        strip_equipment_from_structure(child);
    }
}

// region: --- Tests

#[cfg(test)]
mod tests {
    use super::*;

    type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

    #[test]
    fn test_acc_guitar_structure() {
        // -- Setup & Fixtures
        let inputs = vec!["Acc Guitar"];
        let config = default_config();

        // -- Exec
        let result = monarchy_sort(inputs, &config).unwrap();

        // -- Check
        // Guitars is not transparent, so it keeps its name even with a single item
        // The structure should be: Guitars: [item]
        println!("\nAcc Guitar result:");
        result.print_tree();

        let guitars = result
            .find_child("Guitars")
            .expect("Should have Guitars folder");
        assert_eq!(guitars.items.len(), 1, "Guitars should have 1 item");
        assert_eq!(guitars.items[0].original, "Acc Guitar");
    }

    #[test]
    fn test_kick_and_drumkit() -> Result<()> {
        // -- Setup & Fixtures
        let inputs = vec!["kick_in.wav", "kick_out.wav", "snare.wav"];
        let config = default_config();

        // -- Exec
        let result = monarchy_sort(inputs, &config)?;

        // -- Check
        // The structure should be:
        // Drums
        //   Kick (folder with In/Out children)
        //     In: [kick_in.wav]
        //     Out: [kick_out.wav]
        //   Snare: [snare.wav]
        result.print_tree();
        result
            .assert()
            .has_total_items(3)
            .has_groups(1)
            .group("Drums")
            .has_groups(2) // Kick folder and Snare track
            .group("Kick")
            .has_groups(2) // In and Out sub-groups
            .done()
            .group("Drums")
            .group("Snare")
            .contains_exactly(&["snare.wav"])
            .done();

        Ok(())
    }

    #[test]
    fn test_bass_types() -> Result<()> {
        // -- Setup & Fixtures
        // Note: "808_bass" will match Electronic Kit's "808" pattern before Bass Synth,
        // so we use "sub_bass" instead to properly test synth bass categorization
        let inputs = vec![
            "bass_guitar_di.wav",
            "synth_bass_sub.wav",
            "upright_pizz.wav",
            "electric_bass_amp.wav",
            "sub_bass.wav", // Changed from "808_bass.wav" to avoid matching Electronic Kit
            "acoustic_bass.wav",
        ];
        let config = default_config();

        // -- Exec
        let result = monarchy_sort(inputs, &config)?;

        // -- Check
        // Structure should be:
        // Bass
        //   Guitar
        //     DI:  [bass_guitar_di.wav]      ← bass guitar now splits into
        //     Amp: [electric_bass_amp.wav]     its DI / Amp multi-mic captures
        //   Synth: [synth_bass_sub.wav, sub_bass.wav]
        //   Upright Bass: [upright_pizz.wav, acoustic_bass.wav]
        // Note: Group names are "Guitar", "Synth", "Upright Bass" (not prefixed with "Bass")
        result.print_tree();
        result
            .assert()
            .has_total_items(6)
            .has_groups(1)
            .group("Bass")
            .has_groups(3)
            .group("Guitar") // The group name is "Guitar", display name is "Bass Guitar"
            .has_groups(2)
            .group("DI")
            .contains_exactly(&["bass_guitar_di.wav"])
            .done()
            .group("Bass")
            .group("Guitar")
            .group("Amp")
            .contains_exactly(&["electric_bass_amp.wav"])
            .done()
            .group("Bass")
            .group("Synth") // The group name is "Synth", display name is "Bass Synth"
            .contains_exactly(&["synth_bass_sub.wav", "sub_bass.wav"])
            .done()
            .group("Bass")
            .group("Upright Bass")
            .contains_exactly(&["upright_pizz.wav", "acoustic_bass.wav"])
            .done();

        Ok(())
    }
}

// endregion: --- Tests
