//! High-level IO entry points for REAPER formats.
//!
//! This module provides direct helpers for:
//! - full project `.RPP` files
//! - standalone `.RfxChain` / `<FXCHAIN>` chunks
//! - generic chunk tree read/write

use std::fs;
use std::path::Path;

use daw_proto::fx::tree::FxTree;
use daw_proto::track::Track as DawTrack;

use crate::convert::{
    daw_track_to_rpp_track_chunk, fx_chain_to_tree, rpp_tracks_to_daw_tracks, tree_to_fx_chain,
};
use crate::primitives::{RppBlock, RppBlockContent, Token};
use crate::rpp_tree::{RChunk, read_rpp, read_rpp_chunk, write_rpp};
use crate::types::serialize::RppSerialize;
use crate::types::track::{
    FolderSettings, FolderState, MonitorMode, MuteSoloSettings, RecordMode, RecordSettings,
    Track as RppTrack, TrackSoloState, VolPanSettings,
};
use crate::types::{DecodeOptions, FxChain, ReaperProject};
use crate::{RppParseError, RppResult, parse_rpp_file};

/// Parse a full project text into typed [`ReaperProject`].
// r[impl rpp.parse.project]
pub fn parse_project_text(content: &str) -> RppResult<ReaperProject> {
    let parsed = parse_rpp_file(content)?;
    ReaperProject::from_rpp_project(&parsed).map_err(RppParseError::ParseError)
}

/// Parse project text with selective typed decode options.
pub fn parse_project_text_with_options(
    content: &str,
    options: DecodeOptions,
) -> RppResult<ReaperProject> {
    let parsed = parse_rpp_file(content)?;
    ReaperProject::from_rpp_project_with_options(&parsed, options)
        .map_err(RppParseError::ParseError)
}

/// Read and parse a full `.RPP` project file into typed [`ReaperProject`].
pub fn read_project(path: impl AsRef<Path>) -> RppResult<ReaperProject> {
    let content = fs::read_to_string(path)?;
    parse_project_text(&content)
}

/// Parse FX chain text (`<FXCHAIN ...>` or `<FXCHAIN_REC ...>`) into typed [`FxChain`].
pub fn parse_fxchain_text(content: &str) -> RppResult<FxChain> {
    FxChain::parse(content).map_err(RppParseError::ParseError)
}

/// Read and parse a standalone FX chain file.
pub fn read_fxchain(path: impl AsRef<Path>) -> RppResult<FxChain> {
    let content = fs::read_to_string(path)?;
    parse_fxchain_text(&content)
}

/// Write a typed [`FxChain`] to disk as REAPER chunk text.
pub fn write_fxchain(path: impl AsRef<Path>, chain: &FxChain) -> RppResult<()> {
    fs::write(path, chain.to_rpp_string())?;
    Ok(())
}

/// Parse an FX chain and convert it to daw-proto [`FxTree`].
pub fn parse_fxchain_tree(content: &str) -> RppResult<FxTree> {
    let chain = parse_fxchain_text(content)?;
    Ok(fx_chain_to_tree(&chain))
}

/// Read a standalone FX chain and convert it to daw-proto [`FxTree`].
pub fn read_fxchain_tree(path: impl AsRef<Path>) -> RppResult<FxTree> {
    let chain = read_fxchain(path)?;
    Ok(fx_chain_to_tree(&chain))
}

/// Serialize a daw-proto [`FxTree`] as REAPER FX chain text.
pub fn fx_tree_to_rfxchain_text(tree: &FxTree) -> String {
    tree_to_fx_chain(tree).to_rpp_string()
}

/// Write a daw-proto [`FxTree`] as standalone FX chain text.
pub fn write_fx_tree(path: impl AsRef<Path>, tree: &FxTree) -> RppResult<()> {
    fs::write(path, fx_tree_to_rfxchain_text(tree))?;
    Ok(())
}

/// Serialize daw-proto tracks to a minimal REAPER project text.
pub fn daw_tracks_to_rpp_project_text(tracks: &[DawTrack]) -> String {
    let mut out = String::from("<REAPER_PROJECT 0.1 \"7.0/x64\" 0\n");
    out.push_str("  RIPPLE 0\n");
    for track in tracks {
        let chunk = daw_track_to_rpp_track_chunk(track);
        for line in chunk.lines() {
            out.push_str("  ");
            out.push_str(line);
            out.push('\n');
        }
    }
    out.push('>');
    out
}

/// Parse daw-proto tracks from REAPER project text.
pub fn parse_daw_tracks_from_project_text(content: &str) -> RppResult<Vec<DawTrack>> {
    let project = parse_rpp_file(content)?;
    let mut parsed_tracks = Vec::new();
    for block in &project.blocks {
        if block.block_type == crate::primitives::BlockType::Track {
            let track = match crate::types::track::Track::from_block(block) {
                Ok(track) => track,
                Err(err) => {
                    let _ = err;
                    parse_track_block_lenient(block)
                }
            };
            parsed_tracks.push(track);
        }
    }
    Ok(rpp_tracks_to_daw_tracks(&parsed_tracks))
}

/// Lenient track parser used as fallback when strict parsing fails.
///
/// Reads only commonly needed fields and ignores malformed lines.
// r[impl rpp.parse.lenient]
fn parse_track_block_lenient(block: &RppBlock) -> RppTrack {
    let mut track = RppTrack::from_rpp_block("<TRACK\n>").unwrap_or_else(|_| RppTrack {
        name: String::new(),
        selected: false,
        locked: false,
        peak_color: None,
        beat: None,
        automation_mode: crate::types::track::AutomationMode::TrimRead,
        volpan: None,
        mutesolo: None,
        invert_phase: false,
        folder: None,
        bus_compact: None,
        show_in_mixer: None,
        free_mode: None,
        group_flags: None,
        group_flags_high: None,
        fixed_lanes: None,
        lane_solo: None,
        lane_record: None,
        lane_names: None,
        item_lanes: None,
        comp_areas: Vec::new(),
        record: None,
        track_height: None,
        input_quantize: None,
        channel_count: 2,
        rec_cfg: None,
        midi_color_map_fn: None,
        fx_enabled: false,
        track_id: None,
        perf: None,
        layouts: None,
        extension_data: Vec::new(),
        receives: Vec::new(),
        midi_output: None,
        custom_note_order: None,
        midi_note_names: Vec::new(),
        master_send: None,
        hardware_outputs: Vec::new(),
        items: Vec::new(),
        envelopes: Vec::new(),
        fx_chain: None,
        freeze: None,
        input_fx: None,
        raw_content: String::new(),
    });

    for child in &block.children {
        // Handle both Content (nom parser) and RawLine (fast parser) variants.
        // The fast parser stores TRACK children as RawLine for performance;
        // we tokenize on demand here.
        let tokens_owned;
        let tokens: &[Token] = match child {
            RppBlockContent::Content(t) => t,
            RppBlockContent::RawLine(raw) => {
                match crate::primitives::token::parse_token_line(raw) {
                    Ok((_, t)) => {
                        tokens_owned = t;
                        &tokens_owned
                    }
                    Err(_) => continue,
                }
            }
            _ => continue,
        };
        let Some(Token::Identifier(id)) = tokens.first() else {
            continue;
        };

        let get_num = |idx: usize| tokens.get(idx).and_then(Token::as_number);
        let get_bool = |idx: usize| get_num(idx).map(|v| v != 0.0);
        let get_i32 = |idx: usize| get_num(idx).map(|v| v as i32);
        let get_string = |idx: usize| {
            tokens
                .get(idx)
                .and_then(Token::as_string)
                .map(std::string::ToString::to_string)
        };

        match id.as_str() {
            "NAME" => {
                if let Some(name) = get_string(1) {
                    track.name = name;
                }
            }
            "SEL" => {
                if let Some(sel) = get_bool(1) {
                    track.selected = sel;
                }
            }
            "TRACKID" => {
                if let Some(guid) = get_string(1) {
                    track.track_id = Some(guid);
                }
            }
            "PEAKCOL" => {
                if let Some(pc) = get_i32(1) {
                    track.peak_color = Some(pc);
                }
            }
            "VOLPAN" => {
                if let (Some(vol), Some(pan), Some(pan_law)) = (get_num(1), get_num(2), get_num(3))
                {
                    track.volpan = Some(VolPanSettings {
                        volume: vol,
                        pan,
                        pan_law,
                    });
                }
            }
            "MUTESOLO" => {
                if let (Some(mute), Some(solo), Some(solo_defeat)) =
                    (get_bool(1), get_i32(2), get_bool(3))
                {
                    track.mutesolo = Some(MuteSoloSettings {
                        mute,
                        solo: TrackSoloState::from(solo),
                        solo_defeat,
                    });
                }
            }
            "ISBUS" => {
                if let (Some(folder_state), Some(indentation)) = (get_i32(1), get_i32(2)) {
                    track.folder = Some(FolderSettings {
                        folder_state: FolderState::from(folder_state),
                        indentation,
                    });
                }
            }
            "REC" => {
                if let (Some(armed), Some(input), Some(monitor), Some(record_mode)) =
                    (get_bool(1), get_i32(2), get_i32(3), get_i32(4))
                {
                    track.record = Some(RecordSettings {
                        armed,
                        input,
                        monitor: MonitorMode::from(monitor),
                        record_mode: RecordMode::from(record_mode),
                        monitor_track_media: get_bool(5).unwrap_or(false),
                        preserve_pdc_delayed: get_bool(6).unwrap_or(false),
                        record_path: get_i32(7).unwrap_or(0),
                    });
                }
            }
            _ => {}
        }
    }

    track
}

/// Parse any single-root REAPER chunk text into generic [`RChunk`] tree.
pub fn parse_chunk_text(content: &str) -> RppResult<RChunk> {
    read_rpp_chunk(content)
}

/// Read any single-root REAPER chunk file into generic [`RChunk`] tree.
pub fn read_chunk(path: impl AsRef<Path>) -> RppResult<RChunk> {
    read_rpp(path)
}

/// Write generic chunk tree to disk.
pub fn write_chunk(path: impl AsRef<Path>, root: &RChunk) -> RppResult<()> {
    write_rpp(path, root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk_ops::extract_fxchain_block;
    use crate::convert::fx_chain_to_tree;
    use daw_proto::fx::tree::{FxNode, FxNodeId, FxNodeKind};
    use daw_proto::fx::{Fx, FxType};
    use std::collections::HashSet;

    #[derive(Clone)]
    struct FuzzRng(u64);

    impl FuzzRng {
        fn new(seed: u64) -> Self {
            Self(seed.max(1))
        }

        fn next_u64(&mut self) -> u64 {
            // xorshift64*
            let mut x = self.0;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            self.0 = x;
            x.wrapping_mul(0x2545F4914F6CDD1D)
        }

        fn chance(&mut self, num: u64, den: u64) -> bool {
            debug_assert!(den > 0 && num <= den);
            self.next_u64() % den < num
        }
    }

    fn perturb_project_text(base: &str, seed: u64) -> String {
        let mut rng = FuzzRng::new(seed);
        let mut out = String::new();

        for line in base.lines() {
            let trimmed = line.trim();
            let structural = trimmed.starts_with("<REAPER_PROJECT")
                || trimmed == "<TRACK"
                || trimmed == ">"
                || trimmed.is_empty();

            if structural {
                out.push_str(line);
                out.push('\n');
                continue;
            }

            // Occasionally drop non-structural lines.
            if rng.chance(1, 10) {
                continue;
            }

            let mut mutated = line.to_string();
            if rng.chance(1, 4) {
                if trimmed.starts_with("VOLPAN") {
                    mutated = "    VOLPAN nope NaN -1 -1 1".to_string();
                } else if trimmed.starts_with("MUTESOLO") {
                    mutated = "    MUTESOLO x y z".to_string();
                } else if trimmed.starts_with("ISBUS") {
                    mutated = "    ISBUS maybe +".to_string();
                } else if trimmed.starts_with("REC") {
                    mutated = "    REC ???".to_string();
                } else if trimmed.starts_with("TRACKID") {
                    mutated = "    TRACKID \"\"".to_string();
                } else if trimmed.starts_with("SEL") {
                    mutated = "    SEL maybe".to_string();
                }
            }

            // Randomly append unknown tokens.
            if rng.chance(1, 5) {
                mutated.push_str("   GARBAGE_TOKEN");
            }

            out.push_str(&mutated);
            out.push('\n');

            // Occasionally duplicate a mutated line.
            if rng.chance(1, 12) {
                out.push_str(&mutated);
                out.push('\n');
            }
        }

        out
    }

    #[test]
    fn test_parse_project_text_minimal() {
        let src = r#"<REAPER_PROJECT 0.1 "7.0/x64" 123
  RIPPLE 0 0
>"#;
        let project = parse_project_text(src).expect("project parse");
        assert_eq!(project.version, 0.1);
    }

    #[test]
    fn test_fxchain_tree_roundtrip_text() {
        let src = r#"<FXCHAIN
  SHOW 0
  LASTSEL 0
  DOCKED 0
  BYPASS 0 0 0
  <VST "VST: ReaEQ (Cockos)" reaeq.dll 0 "" 0<00> ""
    ZXE=
  >
  FXID {GUID-EQ}
>"#;

        let tree = parse_fxchain_tree(src).expect("fx tree parse");
        assert_eq!(tree.nodes.len(), 1);
        let out = fx_tree_to_rfxchain_text(&tree);
        assert!(out.contains("<FXCHAIN"));
        assert!(out.contains("BYPASS "));
    }

    #[test]
    fn test_write_fx_tree_text_shape() {
        let tree = FxTree {
            nodes: vec![FxNode {
                id: FxNodeId::from_guid("{X}"),
                kind: FxNodeKind::Plugin(Fx {
                    guid: "{X}".to_string(),
                    index: 0,
                    name: "VST: Test (Vendor)".to_string(),
                    plugin_name: "Test".to_string(),
                    plugin_type: FxType::Vst2,
                    enabled: true,
                    offline: false,
                    window_open: false,
                    parameter_count: 0,
                    preset_name: None,
                }),
                enabled: true,
                parent_id: None,
            }],
        };

        let text = fx_tree_to_rfxchain_text(&tree);
        assert!(text.starts_with("<FXCHAIN"));
    }

    #[test]
    fn test_bridge_end_to_end_standalone_rfxchain() {
        let src = r#"<FXCHAIN
  SHOW 0
  LASTSEL 0
  DOCKED 0
  BYPASS 0 0 0
  <VST "VST: ReaEQ (Cockos)" reaeq.dll 0 "" 0<00> ""
    ZXE=
  >
  FXID {EQ-GUID}
>"#;

        let mut tree = parse_fxchain_tree(src).expect("parse tree");
        assert_eq!(tree.nodes.len(), 1);
        tree.nodes[0].enabled = false; // mutate through daw-proto view

        let text = fx_tree_to_rfxchain_text(&tree);
        let parsed_chain = parse_fxchain_text(&text).expect("parse serialized chain");
        let tree2 = fx_chain_to_tree(&parsed_chain);
        assert_eq!(tree2.nodes.len(), 1);
        assert!(!tree2.nodes[0].enabled);
    }

    #[test]
    fn test_bridge_project_embedded_fxchain_extract_mutate_roundtrip() {
        let track_chunk = r#"<TRACK
  NAME "Guitar"
  <FXCHAIN
    SHOW 0
    LASTSEL 0
    DOCKED 0
    BYPASS 0 0 0
    <VST "VST: ReaEQ (Cockos)" reaeq.dll 0 "" 0<00> ""
      ZXE=
    >
    FXID {EQ-GUID}
  >
>"#;

        let fx_text = extract_fxchain_block(track_chunk).expect("extract fxchain");
        let mut tree = parse_fxchain_tree(fx_text).expect("parse fx tree");
        assert_eq!(tree.nodes.len(), 1);

        tree.nodes.push(FxNode {
            id: FxNodeId::from_guid("{EXTRA}"),
            kind: FxNodeKind::Plugin(Fx {
                guid: "{EXTRA}".to_string(),
                index: 1,
                name: "VST: Added (Vendor)".to_string(),
                plugin_name: "Added".to_string(),
                plugin_type: FxType::Vst2,
                enabled: true,
                offline: false,
                window_open: false,
                parameter_count: 0,
                preset_name: None,
            }),
            enabled: true,
            parent_id: None,
        });

        let out = fx_tree_to_rfxchain_text(&tree);
        let reparsed = parse_fxchain_tree(&out).expect("reparse");
        assert_eq!(reparsed.nodes.len(), 2);
    }

    #[test]
    fn test_daw_tracks_roundtrip_rpp_project_text() {
        let mut t1 = DawTrack::new("{TRACK-1}".to_string(), 0, "Drums".to_string());
        t1.color = Some(0x112233);
        t1.selected = true;
        t1.muted = true;
        t1.soloed = false;
        t1.armed = true;
        t1.volume = 0.875;
        t1.pan = -0.25;
        t1.folder_depth = 1;
        t1.is_folder = true;
        t1.fx_count = 2;

        let mut t2 = DawTrack::new("{TRACK-2}".to_string(), 1, "Bass".to_string());
        t2.color = Some(0x334455);
        t2.selected = false;
        t2.muted = false;
        t2.soloed = true;
        t2.armed = false;
        t2.volume = 0.66;
        t2.pan = 0.2;
        t2.folder_depth = -1;
        t2.is_folder = false;
        t2.fx_count = 0;

        let src = vec![t1.clone(), t2.clone()];
        let text = daw_tracks_to_rpp_project_text(&src);
        let out = parse_daw_tracks_from_project_text(&text).expect("parse roundtrip");

        assert_eq!(out.len(), 2);
        assert_eq!(out[0].guid, t1.guid);
        assert_eq!(out[0].name, t1.name);
        assert_eq!(out[0].color, t1.color);
        assert_eq!(out[0].selected, t1.selected);
        assert_eq!(out[0].muted, t1.muted);
        assert_eq!(out[0].soloed, t1.soloed);
        assert_eq!(out[0].armed, t1.armed);
        assert!((out[0].volume - t1.volume).abs() < 1e-12);
        assert!((out[0].pan - t1.pan).abs() < 1e-12);
        assert_eq!(out[0].folder_depth, t1.folder_depth);
        assert_eq!(out[0].is_folder, t1.is_folder);

        assert_eq!(out[1].guid, t2.guid);
        assert_eq!(out[1].name, t2.name);
        assert_eq!(out[1].color, t2.color);
        assert_eq!(out[1].selected, t2.selected);
        assert_eq!(out[1].muted, t2.muted);
        assert_eq!(out[1].soloed, t2.soloed);
        assert_eq!(out[1].armed, t2.armed);
        assert!((out[1].volume - t2.volume).abs() < 1e-12);
        assert!((out[1].pan - t2.pan).abs() < 1e-12);
        assert_eq!(out[1].folder_depth, t2.folder_depth);
        assert_eq!(out[1].is_folder, t2.is_folder);

        // Parent reconstruction from folder structure.
        assert_eq!(out[0].parent_guid, None);
        assert_eq!(out[1].parent_guid.as_deref(), Some("{TRACK-1}"));
    }

    #[test]
    fn test_daw_tracks_nested_folders_multi_pop_parent_reconstruction() {
        let src = r#"<REAPER_PROJECT 0.1 "7.0/x64" 0
  <TRACK
    NAME "A"
    SEL 1
    ISBUS 1 1
    TRACKID "{A}"
  >
  <TRACK
    NAME "B"
    SEL 0
    ISBUS 1 1
    TRACKID "{B}"
  >
  <TRACK
    NAME "C"
    SEL 0
    ISBUS 0 0
    TRACKID "{C}"
  >
  <TRACK
    NAME "D"
    SEL 1
    ISBUS 2 -2
    TRACKID "{D}"
  >
  <TRACK
    NAME "E"
    SEL 0
    ISBUS 0 0
    TRACKID "{E}"
  >
>"#;

        let out = parse_daw_tracks_from_project_text(src).expect("parse nested folders");
        assert_eq!(out.len(), 5);

        // Parent assignment while nested.
        assert_eq!(out[0].guid, "{A}");
        assert_eq!(out[0].parent_guid, None);
        assert!(out[0].is_folder);
        assert!(out[0].selected);

        assert_eq!(out[1].guid, "{B}");
        assert_eq!(out[1].parent_guid.as_deref(), Some("{A}"));
        assert!(out[1].is_folder);
        assert!(!out[1].selected);

        assert_eq!(out[2].guid, "{C}");
        assert_eq!(out[2].parent_guid.as_deref(), Some("{B}"));
        assert!(!out[2].is_folder);

        // D closes two folder levels but is still inside B while evaluating its parent.
        assert_eq!(out[3].guid, "{D}");
        assert_eq!(out[3].parent_guid.as_deref(), Some("{B}"));
        assert!(out[3].selected);

        // After D closes both levels, E is at top-level.
        assert_eq!(out[4].guid, "{E}");
        assert_eq!(out[4].parent_guid, None);
    }

    #[test]
    fn test_daw_tracks_missing_trackid_and_volpan_defaults() {
        let src = r#"<REAPER_PROJECT 0.1 "7.0/x64" 0
  <TRACK
    NAME "NoGuidNoVolpan"
    SEL 1
    ISBUS 0 0
  >
>"#;

        let out = parse_daw_tracks_from_project_text(src).expect("parse defaults");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].guid, "track-0");
        assert_eq!(out[0].name, "NoGuidNoVolpan");
        assert_eq!(out[0].volume, 1.0);
        assert_eq!(out[0].pan, 0.0);
        assert!(out[0].selected);
    }

    #[test]
    fn test_daw_tracks_duplicate_trackid_gets_repaired() {
        let src = r#"<REAPER_PROJECT 0.1 "7.0/x64" 0
  <TRACK
    NAME "A"
    TRACKID "{DUP}"
  >
  <TRACK
    NAME "B"
    TRACKID "{DUP}"
  >
>"#;

        let out = parse_daw_tracks_from_project_text(src).expect("parse dup guid");
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].guid, "{DUP}");
        assert_ne!(out[1].guid, "{DUP}");
        assert_eq!(out[1].guid, "track-1");
    }

    #[test]
    fn test_daw_tracks_inconsistent_isbus_positive_indent_infers_folder() {
        // Track A is inconsistent: ISBUS state says regular (0), but indentation is +1.
        // We still infer folder-parent semantics for robust parent reconstruction.
        let src = r#"<REAPER_PROJECT 0.1 "7.0/x64" 0
  <TRACK
    NAME "A"
    ISBUS 0 1
    TRACKID "{A}"
  >
  <TRACK
    NAME "B"
    ISBUS 0 0
    TRACKID "{B}"
  >
>"#;

        let out = parse_daw_tracks_from_project_text(src).expect("parse inconsistent isbus");
        assert_eq!(out.len(), 2);
        assert!(out[0].is_folder);
        assert_eq!(out[1].parent_guid.as_deref(), Some("{A}"));
    }

    #[test]
    fn test_daw_tracks_malformed_lines_are_tolerated() {
        let src = r#"<REAPER_PROJECT 0.1 "7.0/x64" 0
  <TRACK
    NAME "A"
    VOLPAN not_a_number nope -1 -1 1
    SEL 1
    TRACKID "{A}"
  >
>"#;

        let out = parse_daw_tracks_from_project_text(src).expect("parse malformed lines");
        assert_eq!(out.len(), 1);
        // Bad VOLPAN line should be ignored; defaults remain.
        assert_eq!(out[0].volume, 1.0);
        assert_eq!(out[0].pan, 0.0);
        // Other valid fields still parsed.
        assert!(out[0].selected);
        assert_eq!(out[0].guid, "{A}");
    }

    #[test]
    fn test_daw_tracks_fuzz_chunk_perturbation_is_resilient() {
        let mut tracks = Vec::new();
        for i in 0..6u32 {
            let mut t = DawTrack::new(format!("{{T-{i}}}"), i, format!("Track-{i}"));
            t.selected = i % 2 == 0;
            t.muted = i % 3 == 0;
            t.soloed = i == 4;
            t.armed = i == 2;
            t.volume = 1.0 - (i as f64 * 0.05);
            t.pan = (i as f64 * 0.1) - 0.25;
            t.color = Some(0x101010 + i);
            t.folder_depth = match i {
                0 => 1,
                1 => 1,
                2 => 0,
                3 => -2,
                _ => 0,
            };
            t.is_folder = t.folder_depth > 0;
            tracks.push(t);
        }

        let base = daw_tracks_to_rpp_project_text(&tracks);
        let cases = 300u64;
        for seed in 1..=cases {
            let perturbed = perturb_project_text(&base, seed);
            let parsed = parse_daw_tracks_from_project_text(&perturbed)
                .unwrap_or_else(|e| panic!("seed {seed} failed parse: {e}\n{perturbed}"));
            assert_eq!(
                parsed.len(),
                tracks.len(),
                "seed {seed} changed track count unexpectedly"
            );

            // IDs must be non-empty and de-duplicated even under TRACKID corruption.
            let mut seen = HashSet::new();
            for (idx, t) in parsed.iter().enumerate() {
                assert!(
                    !t.guid.trim().is_empty(),
                    "seed {seed}, idx {idx}: empty guid"
                );
                assert!(
                    seen.insert(t.guid.clone()),
                    "seed {seed}, idx {idx}: duplicate guid {}",
                    t.guid
                );
            }
        }
    }
}
