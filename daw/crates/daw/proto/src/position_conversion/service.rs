//! Position conversion service — time ↔ beats ↔ quarter notes,
//! per-project (depends on tempo map + time signatures).

use crate::ProjectContext;
use crate::primitives::{
    MeasureMode, PositionInBeats, PositionInQuarterNotes, PositionInSeconds,
    QuarterNotesToMeasureResult, TimeToBeatsResult, TimeToQuarterNotesResult,
};

#[architect::rpc]
pub trait PositionConversion {
    /// Convert time position to beats. Returns beat position plus
    /// measure context and time signature.
    fn time_to_beats(
        &self,
        project: ProjectContext,
        position: PositionInSeconds,
        measure_mode: MeasureMode,
    ) -> TimeToBeatsResult;

    fn beats_to_time(
        &self,
        project: ProjectContext,
        position: PositionInBeats,
        measure_mode: MeasureMode,
    ) -> PositionInSeconds;

    fn time_to_quarter_notes(
        &self,
        project: ProjectContext,
        position: PositionInSeconds,
    ) -> TimeToQuarterNotesResult;

    fn quarter_notes_to_time(
        &self,
        project: ProjectContext,
        position: PositionInQuarterNotes,
    ) -> PositionInSeconds;

    fn quarter_notes_to_measure(
        &self,
        project: ProjectContext,
        position: PositionInQuarterNotes,
    ) -> QuarterNotesToMeasureResult;

    fn beats_to_quarter_notes(
        &self,
        project: ProjectContext,
        position: PositionInBeats,
    ) -> PositionInQuarterNotes;

    fn quarter_notes_to_beats(
        &self,
        project: ProjectContext,
        position: PositionInQuarterNotes,
    ) -> PositionInBeats;
}
