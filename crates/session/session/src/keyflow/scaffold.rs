//! Scaffold a REAPER project from keyflow chart text.
//!
//! Prompts for keyflow text, then builds the **Keyflow folder**
//! (`KEY` / `CHORD` / `LINES` / `HITS` child tracks) and lays every song
//! section out as a coloured region — a one-shot "paste a chart, get the
//! arrangement" scaffold.
//!
//! Phase 1 (this): folder + tracks + section regions, reusing
//! [`crate::setlist::chart_import::chart_to_layout`] and the region helpers in
//! [`crate::keyflow::actions`]. Later phases add project key signatures,
//! chords as items, and melody MIDI.

use daw::service::{Markers, ProjectContext, Projects, Regions, TrackRef, Tracks, UiDialogs};
use tracing::{info, warn};

use session_proto::keyflow_scaffold::{KeyflowScaffoldActions, register_keyflow_scaffold_actions};

use crate::keyflow::actions::{normalize_section_regions, section_type_color};
use crate::setlist::chart_import::{ChartLayout, chart_to_layout};

/// Everything a backend needs to scaffold a project from keyflow text.
pub trait ScaffoldDaw:
    UiDialogs + Tracks + Regions + Markers + Projects + Clone + Send + Sync + 'static
{
}

impl<T> ScaffoldDaw for T where
    T: UiDialogs + Tracks + Regions + Markers + Projects + Clone + Send + Sync + 'static
{
}

/// Prompt for keyflow text and scaffold the project structure.
pub fn scaffold_from_prompt<D: ScaffoldDaw>(daw: &D) {
    let Some(result) = daw.get_user_inputs(
        "Scaffold from Keyflow",
        vec!["Keyflow chart text".to_string()],
        vec![String::new()],
    ) else {
        return;
    };
    if !result.ok {
        return;
    }
    let text = result.values.into_iter().next().unwrap_or_default();
    if text.trim().is_empty() {
        info!("[keyflow-scaffold] empty input, nothing to do");
        return;
    }
    if let Err(e) = scaffold(daw, &text) {
        warn!(error = %e, "[keyflow-scaffold] failed");
    }
}

fn scaffold<D: ScaffoldDaw>(daw: &D, text: &str) -> eyre::Result<()> {
    let layout = chart_to_layout(text)?;
    let project = ProjectContext::Current;

    daw.begin_undo_block(project.clone(), "Scaffold Keyflow project");
    build_keyflow_folder(daw, &project)?;
    lay_out_sections(daw, &project, &layout)?;
    daw.end_undo_block(project, "Scaffold Keyflow project", None);

    info!(sections = layout.sections.len(), "[keyflow-scaffold] done");
    Ok(())
}

/// Create the Keyflow folder with KEY / CHORD / LINES / Lyrics / HITS
/// child tracks.
///
/// The same four the golden session's shape names and the generator
/// writes into. They disagreed until now — this built MELODY and SCALE
/// while the shape expected CHORDS, LINES and HITS — so a project
/// scaffolded from a chart could not pass the checklist meant to check
/// it, and nothing noticed because nothing ran both.
///
/// Bound to `Tracks` alone so `daw.add(..)` resolves unambiguously to
/// `Tracks::add` (both `Tracks` and `Regions` expose an `add`).
pub(crate) fn build_keyflow_folder<D: Tracks>(
    daw: &D,
    project: &ProjectContext,
) -> eyre::Result<()> {
    let folder = daw.add(project.clone(), "Keyflow", None)?;
    daw.set_folder_depth(project.clone(), TrackRef::Guid(folder), 1)?; // open folder

    daw.add(project.clone(), "KEY", None)?;
    daw.add(project.clone(), "CHORD", None)?;
    daw.add(project.clone(), "LINES", None)?;
    daw.add(project.clone(), crate::lyrics::LYRICS_TRACK, None)?;
    let hits = daw.add(project.clone(), "HITS", None)?;
    daw.set_folder_depth(project.clone(), TrackRef::Guid(hits), -1)?; // last child closes folder
    Ok(())
}

/// Insert one coloured region per song section, then normalize numbering.
fn lay_out_sections<D: Regions>(
    daw: &D,
    project: &ProjectContext,
    layout: &ChartLayout,
) -> eyre::Result<()> {
    for section in &layout.sections {
        let name = section.kind.section_type().abbreviation();
        let id = Regions::add(
            daw,
            project.clone(),
            section.start_seconds,
            section.end_seconds,
            &name,
        )?;
        Regions::set_color(
            daw,
            project.clone(),
            id,
            section_type_color(&section.kind.section_type()),
        )?;
    }
    let regions = Regions::all(daw, project.clone());
    normalize_section_regions(daw, regions)?;
    Ok(())
}

// ── architect action ────────────────────────────────────────────────────
//
// Contract in `session_proto::keyflow_scaffold`.

pub struct KeyflowScaffoldImpl<D> {
    pub daw: D,
}

impl<D: ScaffoldDaw> KeyflowScaffoldActions for KeyflowScaffoldImpl<D> {
    fn scaffold_keyflow(&self) {
        scaffold_from_prompt(&self.daw);
    }
}

/// Register the scaffold action with `backend`.
pub fn register_actions<D, B>(backend: &B, daw: D)
where
    D: ScaffoldDaw,
    B: architect::action::ActionBackend + ?Sized,
{
    register_keyflow_scaffold_actions(backend, std::sync::Arc::new(KeyflowScaffoldImpl { daw }));
}
