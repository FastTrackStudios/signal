//! `fts template` — the Dynamic Template classifier, offline.
//!
//! Answers "what would the organiser do with these names?" without a DAW
//! in the loop: pipe in track or item names and see the folder hierarchy
//! they sort into, the colours they'd be assigned, or the groups the
//! default config knows about. Useful for tuning a config, for diffing a
//! naming convention against the classifier, and as the thing to reach
//! for when a track lands in a surprising folder.
//!
//! Colours come from `session::color::classify`, NOT from a second rule
//! table — grouping and colour agree by construction because both fall
//! out of the same `monarchy_sort` pass (see that module's header for why
//! that matters).

use std::io::{self, IsTerminal, Read};
use std::path::{Path, PathBuf};

use clap::Subcommand;
use dynamic_template::{OrganizeIntoTracks, default_config};
use eyre::{Result, eyre};
use serde::Serialize;

#[derive(Subcommand)]
pub enum TemplateCommand {
    /// Organize track/item names into the default Dynamic Template hierarchy
    Organize(NameInputArgs),
    /// Classify names and print the colour each one would be assigned
    Colors(NameInputArgs),
    /// List the groups in the default Dynamic Template config
    Groups {
        /// Emit JSON instead of one name per line
        #[arg(long)]
        json: bool,
    },
}

/// Names to classify: positional, from `--file`, or piped on stdin.
#[derive(clap::Args)]
pub struct NameInputArgs {
    /// Read one name per line from a file (`-` for stdin)
    #[arg(long, short)]
    file: Option<PathBuf>,
    /// Emit JSON instead of the human-readable listing
    #[arg(long)]
    json: bool,
    /// Track or item names to classify
    names: Vec<String>,
}

#[derive(Serialize)]
struct TrackHierarchyOutput {
    input_count: usize,
    track_count: usize,
    tracks: Vec<TrackNodeOutput>,
}

#[derive(Serialize)]
struct TrackNodeOutput {
    name: String,
    is_folder: bool,
    folder_depth_change: String,
    folder_depth_raw: i32,
    items: Vec<String>,
    color: Option<u32>,
    metadata: Option<String>,
}

#[derive(Serialize)]
struct ColorOutput {
    name: String,
    color: u32,
    color_hex: String,
}

pub fn run(cmd: TemplateCommand) -> Result<()> {
    match cmd {
        TemplateCommand::Organize(args) => organize(args),
        TemplateCommand::Colors(args) => colors(args),
        TemplateCommand::Groups { json } => groups(json),
    }
}

fn organize(args: NameInputArgs) -> Result<()> {
    let as_json = args.json;
    let names = read_names(args)?;
    let input_count = names.len();
    let hierarchy = names.organize_into_tracks(&default_config(), None)?;
    let output = TrackHierarchyOutput {
        input_count,
        track_count: hierarchy.tracks.len(),
        tracks: hierarchy
            .tracks
            .iter()
            .map(|node| TrackNodeOutput {
                name: node.name.clone(),
                is_folder: node.is_folder,
                folder_depth_change: format!("{:?}", node.folder_depth_change),
                folder_depth_raw: node.folder_depth_change.to_raw_value(),
                items: node.items.clone(),
                color: node.color,
                metadata: node.metadata.clone(),
            })
            .collect(),
    };

    if as_json {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!(
            "Organized {} names into {} tracks",
            output.input_count, output.track_count
        );
        for track in &output.tracks {
            let marker = if track.is_folder { "[F] " } else { "" };
            println!(
                "{}{}  depth={} items={}",
                marker,
                track.name,
                track.folder_depth_raw,
                track.items.len()
            );
        }
    }

    Ok(())
}

fn colors(args: NameInputArgs) -> Result<()> {
    let as_json = args.json;
    let names = read_names(args)?;
    let mut output: Vec<_> = session::color::classify::classify_and_color(names)
        .into_iter()
        .map(|(name, color)| {
            let color = color.to_hex();
            ColorOutput {
                name,
                color,
                color_hex: format!("#{color:06X}"),
            }
        })
        .collect();
    output.sort_by(|a, b| a.name.cmp(&b.name));

    if as_json {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else if output.is_empty() {
        // Not an error: a name that classifies into no coloured group is
        // exactly the case the runtime handles by inheriting its parent.
        println!("No color assignments found");
    } else {
        for item in &output {
            println!("{} {}", item.color_hex, item.name);
        }
    }

    Ok(())
}

fn groups(as_json: bool) -> Result<()> {
    let config = default_config();
    let names: Vec<_> = config
        .groups
        .iter()
        .map(|group| group.name.clone())
        .collect();

    if as_json {
        println!("{}", serde_json::to_string_pretty(&names)?);
    } else {
        for name in names {
            println!("{name}");
        }
    }

    Ok(())
}

/// Positional names, `--file`, or piped stdin — in that order, and
/// combining the first two when both are given.
fn read_names(args: NameInputArgs) -> Result<Vec<String>> {
    let mut names = args.names;

    if let Some(path) = args.file {
        if path.as_os_str() == "-" {
            names.extend(read_names_from_reader(io::stdin().lock())?);
        } else {
            names.extend(read_names_file(&path)?);
        }
    } else if names.is_empty() && !io::stdin().is_terminal() {
        // Bare `fts template organize` with a pipe means "read the pipe".
        // The terminal check keeps an interactive invocation from hanging
        // on a stdin nobody is going to type into.
        names.extend(read_names_from_reader(io::stdin().lock())?);
    }

    names.retain(|name| !name.trim().is_empty());
    if names.is_empty() {
        return Err(eyre!("provide at least one name, --file, or piped stdin"));
    }

    Ok(names)
}

fn read_names_from_reader<R: Read>(mut reader: R) -> Result<Vec<String>> {
    let mut content = String::new();
    reader.read_to_string(&mut content)?;
    Ok(content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

fn read_names_file(path: &Path) -> Result<Vec<String>> {
    let content = std::fs::read_to_string(path)?;
    Ok(content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_names_rejects_empty_input() {
        let err = read_names(NameInputArgs {
            file: None,
            json: false,
            names: Vec::new(),
        })
        .unwrap_err();

        assert!(err.to_string().contains("provide at least one name"));
    }

    #[test]
    fn read_names_from_reader_trims_blank_lines() {
        let names = read_names_from_reader("  Kick In\n\n Snare  \n\t\nBass\n".as_bytes()).unwrap();

        assert_eq!(names, vec!["Kick In", "Snare", "Bass"]);
    }

    #[test]
    fn colors_assigns_a_known_drum_name() {
        // Guards the seam this port moved across: colours now come from
        // session's classifier, not the retired `dynamic_template::auto_color`.
        let colors = session::color::classify::classify_and_color(vec!["Kick In".to_string()]);

        assert!(colors.contains_key("Kick In"));
    }
}
