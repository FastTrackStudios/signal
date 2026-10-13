//! Time-signature insertion for standalone audio editing.
//!
//! This provides the standalone-safe equivalent of `fts-extensions`'
//! `tempo::time_signature` (which drives REAPER's tempo/timesig map
//! directly via `reaper_low`'s `GetTempoTimeSigMarker`/`SetTempoTimeSigMarker`).
//! Same behavior, rebuilt on `daw::service::TempoMap` so it also runs against
//! `daw-standalone` — no REAPER FFI in this crate.
//!
//! Inserts a `num/denom` time signature at the measure containing the
//! edit cursor. With `single_measure` set, the signature lasts exactly
//! one measure and the signature previously in effect is restored on
//! the next downbeat (matching the REAPER action's Shift-click
//! behavior — the toolbar caller reads the modifier key and passes it
//! through here as a plain bool).

use daw::service::transport::service::Transport as TransportService;
use daw::service::{ProjectContext, TempoMap};

use super::actions::edit_cursor_position;

/// The signatures the Organize-mode toolbar renders buttons for.
/// Mirrors `fts-extensions`' `TIME_SIGNATURES` table.
pub const TIME_SIGNATURES: &[(i32, i32)] = &[
    (2, 4),
    (3, 4),
    (4, 4),
    (5, 4),
    (6, 4),
    (7, 4),
    (3, 8),
    (5, 8),
    (6, 8),
    (7, 8),
    (9, 8),
    (12, 8),
    (13, 8),
];

/// Insert (or update) a time-signature change at `seconds`: edit the
/// existing tempo point there if one already sits at that exact
/// position, otherwise add a new one carrying the tempo already in
/// effect (so the tempo map itself is unchanged).
fn upsert_timesig_at<D>(
    daw: &D,
    project: ProjectContext,
    seconds: f64,
    num: i32,
    denom: i32,
) -> eyre::Result<()>
where
    D: TempoMap,
{
    const EPSILON_SECONDS: f64 = 1e-6;
    let points = daw.get_tempo_points(project.clone());
    let existing = points.iter().enumerate().find(|(_, p)| {
        p.position
            .seconds()
            .is_some_and(|s| (s - seconds).abs() < EPSILON_SECONDS)
    });
    let index = if let Some((idx, _)) = existing {
        u32::try_from(idx).map_err(|_| eyre::eyre!("tempo point index exceeds u32 range"))?
    } else {
        let tempo = daw.get_tempo_at(project.clone(), seconds);
        daw.add_tempo_point(project.clone(), seconds, tempo)?
    };
    daw.set_time_signature_at_point(project, index, num, denom)?;
    Ok(())
}

/// Insert a `num/denom` time signature at the measure containing the edit cursor.
///
/// With `single_measure`, the signature lasts one measure and the previous signature
/// is restored on the next downbeat — unless the project already changes signature
/// there, which takes precedence.
///
/// # Errors
///
/// Returns an error if any underlying DAW service call fails (e.g., unable to
/// add or modify tempo points or time signatures).
pub fn insert_time_signature<D>(
    daw: &D,
    num: i32,
    denom: i32,
    single_measure: bool,
) -> eyre::Result<()>
where
    D: TransportService + TempoMap,
{
    let project = ProjectContext::Current;
    let position = edit_cursor_position(daw, project.clone());
    let (measure, ..) = daw.time_to_musical(project.clone(), position);
    let prev = daw.get_time_signature_at(project.clone(), position);
    let measure_start = daw.musical_to_time(project.clone(), measure, 0, 0.0);

    upsert_timesig_at(daw, project.clone(), measure_start, num, denom)?;

    if single_measure && prev != (num, denom) {
        let next_start = daw.musical_to_time(project.clone(), measure.saturating_add(1), 0, 0.0);
        let already_changes = daw.get_tempo_points(project.clone()).iter().any(|p| {
            p.time_signature.is_some()
                && p.position
                    .seconds()
                    .is_some_and(|s| (s - next_start).abs() < 1e-6)
        });
        if !already_changes {
            upsert_timesig_at(daw, project, next_start, prev.0, prev.1)?;
        }
    }

    Ok(())
}
