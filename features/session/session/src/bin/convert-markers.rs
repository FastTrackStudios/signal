//! Offline batch tool: convert an `.RPP`'s markers/regions into the FTS
//! lane system (see `session::keyflow::offline::auto_organize_regions`) —
//! a whole album can be run in one shot without opening REAPER.
//!
//! ```text
//! convert-markers <in.rpp>... [-o <out.rpp>] [--in-place]
//! ```
//!
//! # This tool used to destroy the projects it was given
//!
//! It read a project into `ReaperProject`, converted, and wrote back with
//! `to_rpp_string()` — a typed serializer that can only emit the fields it
//! models — **over the input, by default**. Everything else went: the master
//! track, `<NOTES>`, `<RECORD_CFG>`, every `RENDER_*`, per-item
//! `CHANMODE`/`YPOS`, all the `<EXT>` blocks holding original capture
//! filenames; and each take came back doubled around a `LANE` token REAPER
//! has never had. That is how the Crescendum originals were lost.
//!
//! Two changes, and both matter:
//!
//! - The write goes through the chunk tree, so untouched lines are returned
//!   byte for byte. Only what this tool actually changes — `MARKER`,
//!   `RULERLANE`, `RULERHEIGHT`, all top-level lines — is spliced in.
//! - Overwriting the input now requires `--in-place`. The conversion being
//!   idempotent made in-place look safe; it was never the risk. Losing the
//!   rest of the file was.

use std::env;

use dawfile_reaper::rpp_tree::{RChunk, RNodeTree};
use dawfile_reaper::types::RppSerialize;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        print_usage();
        std::process::exit(2);
    }

    let mut output: Option<String> = None;
    let mut in_place = false;
    let mut inputs: Vec<String> = Vec::new();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        if arg == "-o" {
            output = Some(iter.next().unwrap_or_else(|| {
                eprintln!("-o requires a path");
                std::process::exit(2);
            }));
        } else if arg == "--in-place" {
            in_place = true;
        } else {
            inputs.push(arg);
        }
    }

    if inputs.is_empty() {
        print_usage();
        std::process::exit(2);
    }
    if output.is_some() && inputs.len() > 1 {
        eprintln!("-o only accepts a single input");
        std::process::exit(2);
    }
    // Writing over the input is opt-in. It used to be the default, and it is
    // how the Crescendum originals were lost: a lossy rewrite landing on the
    // only copy of each project. The conversion being idempotent made that
    // look safe — idempotence was never the risk.
    if output.is_none() && !in_place {
        eprintln!(
            "refusing to overwrite the input.\n\
             Pass -o <out.rpp> to write elsewhere, or --in-place if you really\n\
             mean to rewrite the project you are pointing at."
        );
        std::process::exit(2);
    }

    let mut failed = false;
    for input in &inputs {
        let out = output.clone().unwrap_or_else(|| input.clone());
        if let Err(err) = convert_one(input, &out) {
            eprintln!("error: {input}: {err}");
            failed = true;
        }
    }
    if failed {
        std::process::exit(1);
    }
}

/// The top-level lines `auto_organize_regions` is allowed to change.
///
/// Everything it does — `ensure_core_lanes`, `convert_markers_to_session_format`,
/// `normalize_section_regions`, `ensure_song_region`, `normalize_marker_lanes`,
/// `hide_stray_lanes` — lands in exactly these three, all children of
/// `<REAPER_PROJECT>` itself. Anything outside this set changing is the typed
/// model losing something, not the conversion doing its job.
const OWNED: &[&str] = &["MARKER", "RULERLANE", "RULERHEIGHT"];

fn is_owned(node: &RNodeTree) -> bool {
    match node {
        RNodeTree::Node(n) => {
            let mut probe = n.clone();
            probe
                .get_name()
                .is_some_and(|name| OWNED.contains(&name.as_str()))
        }
        RNodeTree::Chunk(_) => false,
    }
}

fn convert_one(input: &str, output: &str) -> Result<(), Box<dyn std::error::Error>> {
    // The lossless tree is what gets written; the typed model is only used to
    // compute the new markers.
    let source = std::fs::read_to_string(input)?;
    let mut tree = dawfile_reaper::read_rpp_chunk(&source)?;

    let mut project = dawfile_reaper::io::read_project(input)?;
    let name = std::path::Path::new(input)
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("Song");
    session::keyflow::offline::auto_organize_regions(&mut project, name)?;

    // Re-parse the typed serializer's output rather than formatting the lines
    // by hand: it already knows how to write a MARKER, and a second
    // implementation here would be one more thing to drift.
    let converted = dawfile_reaper::read_rpp_chunk(&project.to_rpp_string())?;
    let new_lines: Vec<RNodeTree> = converted
        .children
        .iter()
        .filter(|c| is_owned(c))
        .cloned()
        .collect();

    // Splice: drop the old owned lines, put the new ones where the first of
    // them was (or ahead of the first track, for a project that had none).
    let at = tree
        .children
        .iter()
        .position(is_owned)
        .or_else(|| {
            tree.children.iter().position(
                |c| matches!(c, RNodeTree::Chunk(k) if k.name().as_deref() == Some("TRACK")),
            )
        })
        .unwrap_or(tree.children.len());
    tree.children.retain(|c| !is_owned(c));
    let at = at.min(tree.children.len());
    for (offset, line) in new_lines.into_iter().enumerate() {
        tree.children.insert(at + offset, line);
    }

    write_like_source(output, &tree, &source)?;
    println!("{input} → {output}");
    Ok(())
}

/// Write `tree`, restoring the line endings `source` used.
///
/// REAPER reads either, but a session saved on macOS is CRLF throughout and
/// silently rewriting all 21k lines to LF makes the change impossible to
/// review as a diff — the one thing the chunk tree exists to preserve.
fn write_like_source(
    output: &str,
    tree: &RChunk,
    source: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = dawfile_reaper::stringify_rpp_node(&RNodeTree::Chunk(tree.clone()));
    let mut text = if text.ends_with('\n') {
        text
    } else {
        text + "\n"
    };
    if source.contains("\r\n") {
        text = text.replace('\n', "\r\n");
    }
    std::fs::write(output, text)?;
    Ok(())
}

fn print_usage() {
    eprintln!(
        "usage: convert-markers <in.rpp>... [-o <out.rpp>]\n\n\
         Converts plain REAPER markers/regions into the FTS lane system:\n\
         section regions recolored via the section-color taxonomy and pinned\n\
         to the Sections lane, a whole-song region added to the Song lane,\n\
         stray point markers swept up, non-FTS lanes hidden.\n\n\
         Writing over the input requires --in-place; otherwise pass -o.\n\
         -o only accepts a single input.\n\n\
         Untouched lines are returned byte for byte: only MARKER, RULERLANE\n\
         and RULERHEIGHT are rewritten."
    );
}
