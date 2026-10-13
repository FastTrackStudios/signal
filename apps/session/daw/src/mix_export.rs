//! A prepared song written back out as a REAPER project, to finish the
//! mix in REAPER.
//!
//! The engine writes its own project as RPP text, carrying every track's
//! original FX chain over from the file it was opened from — so the
//! plugins a session came with come back as they were. Tracks the
//! session made itself (the buses, the drum Process folder) have no
//! chain there: they get the golden template's, as REAPER's stock
//! plugins with their settings (`dawfile_reaper::stock_fx`), spliced in.
//!
//! The project goes beside the one it was opened from, so the media
//! paths — relative to that folder — still find the audio.

#![cfg(feature = "native")]

use daw::service::{ProjectContext, Projects as _, Tracks};
use dawfile_reaper::builder::ReaperProjectBuilder;
use dawfile_reaper::stock_fx::{ReaComp, ReaEq, ReaVerbate};

/// What [`write_rpp`] wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exported {
    pub path: std::path::PathBuf,
    /// The tracks given a chain, by name.
    pub chained: Vec<String>,
}

/// The golden template's chain for a track the session made, as one
/// REAPER `<FXCHAIN>` block. `None` for a track that gets none.
#[must_use]
pub fn chain_for(name: &str) -> Option<String> {
    let comp = |threshold: f64, ratio: f64, attack: f64, release: f64| {
        ReaComp::new()
            .threshold_db(threshold)
            .ratio(ratio)
            .attack_ms(attack)
            .release_ms(release)
            .auto_makeup()
    };
    let room = |size: f64, damping: f64| {
        ReaVerbate::new()
            .room_size(size)
            .damping(damping)
            .wet(1.0)
            .dry(0.0)
    };
    let text = match name.trim() {
        // Parallel compressors, dry to crushed.
        "Dry" => one_track(name, |t| t.stock_fx(comp(-6.0, 1.5, 20.0, 150.0))),
        "Tight" => one_track(name, |t| t.stock_fx(comp(-20.0, 4.0, 1.0, 50.0))),
        "Punch" => one_track(name, |t| t.stock_fx(comp(-20.0, 4.0, 15.0, 80.0))),
        "Smash" => one_track(name, |t| t.stock_fx(comp(-35.0, 10.0, 0.5, 100.0))),
        "Crunch" => one_track(name, |t| t.stock_fx(comp(-45.0, 20.0, 0.1, 60.0))),
        // The rooms, small to vast.
        "Room Sim" => one_track(name, |t| t.stock_fx(room(0.15, 0.7))),
        "Wood Room" => one_track(name, |t| t.stock_fx(room(0.3, 0.6))),
        "Music Club" => one_track(name, |t| t.stock_fx(room(0.45, 0.5))),
        "Stadium" => one_track(name, |t| t.stock_fx(room(0.95, 0.3))),
        "RMX 16" => one_track(name, |t| t.stock_fx(room(0.7, 0.4))),
        "Nonlin" => one_track(name, |t| t.stock_fx(room(0.6, 0.8))),
        "Brick Wall" => one_track(name, |t| t.stock_fx(room(0.5, 0.2))),
        // Every bus: EQ, then a gentle compressor (the template's chain).
        bus if dynamic_template::apply::is_bus_track(bus) => one_track(name, |t| {
            t.stock_fx(ReaEq::new())
                .stock_fx(comp(-12.0, 2.0, 10.0, 100.0))
        }),
        _ => return None,
    };
    fx_block(&text)
}

/// A one-track project's RPP text: the track `name` as `fx` builds it.
fn one_track(
    name: &str,
    fx: impl FnOnce(dawfile_reaper::builder::TrackBuilder) -> dawfile_reaper::builder::TrackBuilder,
) -> String {
    use dawfile_reaper::types::RppSerialize as _;
    ReaperProjectBuilder::new()
        .track(name, fx)
        .build()
        .to_rpp_string()
}

/// How far a line is indented.
fn indent(line: &str) -> usize {
    line.len().saturating_sub(line.trim_start().len())
}

/// The line closing the block opened at `open`: the next bare `>` at
/// its indent.
fn close_of(lines: &[&str], open: usize) -> Option<usize> {
    let at = indent(lines.get(open)?);
    lines
        .iter()
        .enumerate()
        .skip(open.saturating_add(1))
        .find(|(_, l)| l.trim() == ">" && indent(l) == at)
        .map(|(i, _)| i)
}

/// The `<FXCHAIN … >` block of a one-track project's text, its indent
/// taken off.
fn fx_block(text: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines
        .iter()
        .position(|l| l.trim_start().starts_with("<FXCHAIN"))?;
    let end = close_of(&lines, start)?;
    let at = indent(lines.get(start)?);
    Some(
        lines
            .get(start..=end)?
            .iter()
            .map(|l| l.get(at..).unwrap_or(l))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// `text` with `block` added to the track whose `<TRACK` line names
/// `guid`, when it has no FX chain yet: before its first item, else
/// before its closing line. `None` when it already has one, or no such
/// track.
fn splice(text: &str, guid: &str, block: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let open = lines
        .iter()
        .position(|l| l.trim_start().starts_with("<TRACK") && l.contains(guid))?;
    let close = close_of(&lines, open)?;
    let inner = lines.get(open.saturating_add(1)..close)?;
    if inner.iter().any(|l| l.trim_start().starts_with("<FXCHAIN")) {
        return None;
    }
    let at = inner
        .iter()
        .position(|l| l.trim_start().starts_with("<ITEM"))
        .map_or(close, |i| open.saturating_add(1).saturating_add(i));
    let pad = " ".repeat(indent(lines.get(open)?).saturating_add(2));
    let mut out: Vec<String> = lines.get(..at)?.iter().map(|l| (*l).to_string()).collect();
    out.extend(block.lines().map(|l| format!("{pad}{l}")));
    out.extend(lines.get(at..)?.iter().map(|l| (*l).to_string()));
    Some(out.join("\n") + "\n")
}

/// Write `project_guid` as a REAPER project named `name` (`.RPP` added)
/// beside the file it was opened from, the session's own tracks given
/// the template's chains ([`chain_for`]).
///
/// # Errors
///
/// No such project, no file it was opened from, or the write failed.
pub fn write_rpp(
    daw: &daw_standalone::sync::Standalone,
    project_guid: &str,
    name: &str,
) -> eyre::Result<Exported> {
    let project = ProjectContext::Project(project_guid.to_owned());
    let mut text = daw_standalone::session_file::project_rpp_text(daw, project_guid)
        .map_err(|e| eyre::eyre!("project as RPP: {e}"))?;
    let mut chained = Vec::new();
    for track in Tracks::all(daw, project.clone()) {
        let Some(block) = chain_for(&track.name) else {
            continue;
        };
        if let Some(with) = splice(&text, &track.guid, &block) {
            text = with;
            chained.push(track.name.clone());
        }
    }
    let from = daw.info(project)?.path;
    let dir = std::path::Path::new(&from)
        .parent()
        .ok_or_else(|| eyre::eyre!("the project has no folder: {from:?}"))?;
    // A `.session` folder's project is written beside the folder.
    let dir = if dir.extension().is_some_and(|e| e == "session") {
        dir.parent().unwrap_or(dir)
    } else {
        dir
    };
    let path = dir.join(format!("{name}.RPP"));
    std::fs::write(&path, text)?;
    Ok(Exported { path, chained })
}

#[cfg(test)]
mod tests {
    use super::{chain_for, splice};

    #[test]
    fn the_session_made_tracks_get_the_templates_chains() {
        let bus = chain_for("DRUM BUS").expect("a bus chain");
        assert!(bus.starts_with("<FXCHAIN") && bus.ends_with('>'));
        assert!(bus.contains("ReaEQ") && bus.contains("ReaComp"), "{bus}");
        assert!(chain_for("Smash").is_some_and(|c| c.contains("ReaComp")));
        assert!(chain_for("Stadium").is_some_and(|c| c.contains("ReaVerbate")));
        assert!(
            chain_for("Kick In").is_none(),
            "a recorded track keeps its own"
        );
    }

    #[test]
    fn a_chain_goes_in_before_the_items_and_never_twice() {
        let text = "<REAPER_PROJECT\n  <TRACK {A}\n    NAME a\n    <ITEM\n      POSITION 0\n    >\n  >\n  <TRACK {B}\n    NAME b\n  >\n>\n";
        let block = "<FXCHAIN\n  SHOW 0\n>";
        let with = splice(text, "{A}", block).expect("spliced");
        let fx = with.find("<FXCHAIN").expect("in");
        assert!(fx < with.find("<ITEM").expect("item") && fx > with.find("<TRACK {A}").expect("a"));
        assert!(splice(&with, "{A}", block).is_none(), "never twice");
        let b = splice(text, "{B}", block).expect("no items: before the close");
        assert!(b.find("<FXCHAIN").expect("in") > b.find("<TRACK {B}").expect("b"));
    }
}
