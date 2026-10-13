//! The whole organize pass, once, for every backend.
//!
//! Classify each track, repair the folder structure, colour by
//! classification, build the bus tree the classified tracks justify, route
//! content into it, nest DI captures, and park whatever classified to
//! nothing in `UNSORTED`. The `--apply-buses` CLI runs this over an `.RPP`
//! chunk tree; the Session window and the REAPER action run it over a live
//! project through [`DawTarget`](super::DawTarget). One pipeline, so a live
//! organize and a batch one cannot disagree about a session.

use std::collections::BTreeMap;

use dynamic_template_proto::TemplateBus;

use super::{
    apply_buses, apply_colors, apply_routing, classified_paths, gather_unsorted,
    normalize_folder_depths, AppliedBuses, FolderFix, Gathered, RoutingReport, TemplateTarget,
};
use crate::buses::{bus_for_path, buses_for_paths, is_bus_name};

/// What [`organize`] found and did — everything a caller reports.
pub struct Organized<Id> {
    /// Tracks in the project before anything was added.
    pub existing: usize,
    /// Tracks whose running folder depth went negative before the repair.
    pub broken_depths: usize,
    /// The bus tree the classified tracks justified, parents included.
    pub buses: Vec<TemplateBus>,
    /// Which tracks justified each bus, by bus name.
    pub justified: BTreeMap<&'static str, Vec<String>>,
    /// Track names that classified to no bus.
    pub unclassified: Vec<String>,
    pub folder_fixes: Vec<FolderFix<Id>>,
    /// Tracks coloured by classification.
    pub painted: usize,
    pub applied: AppliedBuses<Id>,
    pub routing: RoutingReport,
    /// The unrouted tracks handed to the gather, by id.
    pub unsorted: Vec<Id>,
    /// What the gather moved into `UNSORTED`, if anything.
    pub gathered: Option<Gathered<Id>>,
}

/// Organize `target` in place. Idempotent: a second run finds every bus
/// and send already there.
///
/// # Errors
///
/// Returns the backend's error if any primitive fails.
pub fn organize<T: TemplateTarget>(target: &mut T) -> Result<Organized<T::TrackId>, T::Error> {
    let depths = target.folder_depths();
    let existing = depths.len();
    let broken_depths = {
        let mut running = 0i32;
        depths
            .iter()
            .filter(|(_, _, change)| {
                running = running.saturating_add(*change);
                running < 0
            })
            .count()
    };

    // Classify every track name and keep the group paths that resolve to a
    // bus. A bus is only built when a real track lands on it, so this also
    // records which tracks justified each one.
    let mut justified: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
    let mut group_paths: Vec<Vec<String>> = Vec::new();
    let mut unclassified: Vec<String> = Vec::new();
    for entry in classified_paths(target) {
        // Never let a bus track justify a bus: their names classify as the
        // content they carry ("VOX BUS" reads as a vocal).
        if is_bus_name(&entry.name) {
            continue;
        }
        match bus_for_path(&entry.path) {
            Some(bus) => {
                justified.entry(bus).or_default().push(entry.name.clone());
                group_paths.push(entry.path);
            }
            None => unclassified.push(entry.name.clone()),
        }
    }
    let buses = buses_for_paths(group_paths.iter().map(Vec::as_slice));

    // Repair the folder structure before anything reasons about folders.
    let folder_fixes = normalize_folder_depths(target)?;
    let painted = apply_colors(target)?;
    let applied = apply_buses(target, &buses)?;
    // Route before the gather, which can renumber tracks.
    let routing = apply_routing(target, &applied)?;
    target.nest_secondary_mics();

    // Gather from the routing report, not the classification pass: only the
    // routing walk knows which tracks reach a bus through a parent folder
    // (leave those alone) and which are control-only VCAs (finished, not
    // unsorted). Last, because gathering renumbers tracks.
    let unsorted: Vec<T::TrackId> = routing
        .unrouted
        .iter()
        .filter_map(|name| target.find_track(name))
        .collect();
    let gathered = gather_unsorted(target, &unsorted)?;

    Ok(Organized {
        existing,
        broken_depths,
        buses,
        justified,
        unclassified,
        folder_fixes,
        painted,
        applied,
        routing,
        unsorted,
        gathered,
    })
}
