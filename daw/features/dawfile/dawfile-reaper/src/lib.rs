// Lint debt: workspace flipped dead_code/unused to warn (task cleanup);
// this crate predates that — burn down separately.
#![allow(dead_code, unused)]

//! # RPP Parser
//!
//! A high-performance RPP (REAPER Project) file format parser using nom parser combinators.
//! This parser focuses on the generic RPP file format parsing without REAPER-specific data structures.
//!
//! ## Features
//!
//! - **High Performance**: Uses nom parser combinators for zero-copy parsing
//! - **WDL Compatible**: Matches the parsing behavior of REAPER's WDL library
//! - **Generic Format**: Parses RPP file structure without REAPER-specific assumptions
//! - **Modular Design**: Separate token, block, and project parsing modules
//! - **Type Safe**: Strongly typed Rust structures for RPP format elements
//!
//! ## Architecture
//!
//! This parser provides the core RPP file format parsing:
//! - **Token Parsing**: Handles all token types (strings, numbers, MIDI events, etc.)
//! - **Block Parsing**: Parses RPP block structures (`<BLOCK>` and `>`)
//! - **Project Parsing**: Top-level RPP file parsing
//!
//! REAPER-specific data structures (tracks, items, envelopes, FX chains) should be handled
//! by separate adapter modules that consume this parser's output.
//!
//! ## Example
//!
//! ```rust
//! use dawfile_reaper::{parse_rpp_file, ReaperProject};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let rpp_content = r#"<REAPER_PROJECT 0.1 "6.75/linux-x86_64" 1681651369
//!       <TRACK
//!         NAME "Track 1"
//!         VOL 1.0 0.0
//!       >
//!     >"#;
//!
//!     let project = parse_rpp_file(rpp_content)?;
//!     let reaper_project = ReaperProject::from_rpp_project(&project)?;
//!     // Now you have strongly-typed REAPER data structures!
//!     Ok(())
//! }
//! ```
//!
//! ## Examples
//!
//! The crate includes examples demonstrating different aspects of RPP parsing:
//!
//!
//! Run any example with: `cargo run --example <example_name>`

use thiserror::Error;

pub mod builder;
pub mod chunk_ops;
pub mod compat;
pub mod convert;
pub mod diff;
pub mod guide_gen;
pub mod index;
pub mod io;
pub mod keysig;
pub mod notation;
pub mod primitives;
/// REAPER `.reapeaks` peak-cache files (waveform mipmaps).
pub mod reapeaks;
pub mod rpp_tree;
pub mod scaffold;
pub mod sessionpeaks;
pub mod setlist_rpp;
pub mod stock_fx;
pub mod toolbar_config;
pub mod types;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_track_hierarchy_parsing() {
        // Test that we can parse track hierarchy information
        let rpp_content = r#"<REAPER_PROJECT 0.1 "6.75/linux-x86_64" 1681651369
          <TRACK
            NAME "DRUMS"
            ISBUS 1 1
          >
          <TRACK
            NAME "Kick"
            ISBUS 0 0
          >
          <TRACK
            NAME "Out"
            ISBUS 2 -1
          >
        >"#;

        let project = parse_rpp_file(rpp_content).unwrap();
        assert_eq!(project.blocks.len(), 3);

        // Verify we can extract folder information
        for block in &project.blocks {
            if block.block_type == BlockType::Track {
                // Track folder hierarchy display logic
                let mut folder_state = 0;
                let mut indentation = 0;

                for child in &block.children {
                    if let RppBlockContent::Content(tokens) = child
                        && let Some(first_token) = tokens.first()
                        && first_token.to_string() == "ISBUS"
                        && tokens.len() >= 3
                    {
                        folder_state = tokens[1].to_string().parse::<i32>().unwrap_or(0);
                        indentation = tokens[2].to_string().parse::<i32>().unwrap_or(0);
                        break;
                    }
                }

                // Verify the folder information is parsed correctly
                match block.children.iter().find_map(|child| {
                    if let RppBlockContent::Content(tokens) = child
                        && let Some(first_token) = tokens.first()
                        && first_token.to_string() == "NAME"
                    {
                        return tokens.get(1).map(|t| t.to_string());
                    }
                    None
                }) {
                    Some(name) if name == "DRUMS" => {
                        assert_eq!(folder_state, 1); // folder parent
                        assert_eq!(indentation, 1); // increase indentation
                    }
                    Some(name) if name == "Kick" => {
                        assert_eq!(folder_state, 0); // regular track
                        assert_eq!(indentation, 0); // no change
                    }
                    Some(name) if name == "Out" => {
                        assert_eq!(folder_state, 2); // last track in folder
                        assert_eq!(indentation, -1); // decrease indentation
                    }
                    _ => {}
                }
            }
        }
    }
}

// Re-export the main types for convenience
pub use compat::{
    AddRChunk, AddRNode, AddRToken, CreateNodeInput, CreateRChunk, CreateRNode, CreateRPP,
    CreateRTokens, LUA_API_MATRIX, ReadRPP, ReadRPPChunk, ReadRPPChunkLines, StringifyRPPNode,
    WriteRPP,
};
pub use convert::{
    daw_track_to_rpp_track_chunk, fx_chain_to_tree, rpp_track_to_daw_track,
    rpp_tracks_to_daw_tracks, tree_to_fx_chain,
};
pub use index::{ProjectIndex, ProjectSummary};
pub use io::{
    daw_tracks_to_rpp_project_text, fx_tree_to_rfxchain_text, parse_chunk_text,
    parse_daw_tracks_from_project_text, parse_fxchain_text, parse_fxchain_tree, parse_project_text,
    parse_project_text_with_options, read_chunk, read_fxchain, read_fxchain_tree, read_project,
    write_chunk, write_fx_tree, write_fxchain,
};
pub use primitives::{
    BlockType, QuoteType, RppBlock, RppBlockContent, RppProject, Token, parse_rpp,
};
pub use rpp_tree::{
    GuidStripPolicy, RChunk, RNode, RNodeTree, RToken as TreeToken, add_rchunk, add_rnode,
    add_rtoken, create_rchunk, create_rnode_from_line, create_rnode_from_tokens, create_rpp,
    create_rtokens, read_rpp, read_rpp_chunk, read_rpp_lines, stringify_rpp_node,
    tokenize as tokenize_tree, write_rpp,
};
pub use stock_fx::{
    EqBand, EqBandType, ReaComp, ReaControlMidi, ReaDelay, ReaEq, ReaFir, ReaFirMode, ReaGate,
    ReaInsert, ReaLimit, ReaPitch, ReaStream, ReaSynth, ReaTune, ReaVerb, ReaVerbate, ReaXcomp,
    StockFx,
};
pub use types::{
    DecodeOptions, Envelope, FxChain, FxChainNode, FxContainer, FxEnvelopePoint, FxParamEnvelope,
    FxParamRef, FxPlugin, Item, JsParamValue, MarkerRegion, MarkerRegionCollection, MidiEvent,
    MidiEventType, MidiSource, MidiSourceEvent, PluginType, ReaperProject, RppSerialize,
    SourceBlock, SourceType, StretchMarker, TempoTimeEnvelope, TempoTimePoint, Track,
    TrackParseOptions, parse_js_params,
};

/// Main error type for RPP parsing
#[derive(Error, Debug)]
pub enum RppParseError {
    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Invalid token: {0}")]
    InvalidToken(String),

    #[error("Unexpected end of input")]
    UnexpectedEof,

    #[error("Invalid block structure")]
    InvalidBlockStructure,
}

/// Result type for RPP parsing operations
pub type RppResult<T> = Result<T, RppParseError>;

/// Parse a complete RPP file
pub fn parse_rpp_file(content: &str) -> RppResult<RppProject> {
    match primitives::fast_project::parse_rpp_fast(content) {
        Ok(project) => Ok(project),
        Err(fast_err) => {
            // Fallback for edge cases to preserve compatibility while we harden the fast path.
            match primitives::project::parse_rpp(content) {
                Ok((remaining, project)) => {
                    if !remaining.trim().is_empty() {
                        return Err(RppParseError::ParseError(format!(
                            "Unexpected remaining input: {}",
                            remaining
                        )));
                    }
                    Ok(project)
                }
                Err(e) => Err(RppParseError::ParseError(format!(
                    "fast parser failed: {fast_err}; nom fallback failed: {e:?}"
                ))),
            }
        }
    }
}
