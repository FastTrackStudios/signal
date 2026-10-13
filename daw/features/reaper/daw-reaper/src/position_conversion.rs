//! `impl PositionConversion for Reaper` — sync trait + REAPER TimeMap.

use crate::safe_wrappers::time_map as sw;
use daw_proto::{
    MeasureMode, PositionConversion, PositionInBeats, PositionInQuarterNotes, PositionInSeconds,
    ProjectContext, QuarterNotesToMeasureResult, TimeSignature, TimeToBeatsResult,
    TimeToQuarterNotesResult,
};
use reaper_high::Reaper;

fn resolve_project(ctx: &ProjectContext) -> Option<reaper_high::Project> {
    match ctx {
        ProjectContext::Current => Some(Reaper::get().current_project()),
        ProjectContext::Project(guid) => {
            use crate::project_context::find_project_by_guid;
            find_project_by_guid(guid)
        }
    }
}

impl PositionConversion for crate::Reaper {
    fn time_to_beats(
        &self,
        project: ProjectContext,
        position: PositionInSeconds,
        measure_mode: MeasureMode,
    ) -> TimeToBeatsResult {
        let Some(proj) = resolve_project(&project) else {
            return TimeToBeatsResult::default();
        };
        let low = Reaper::get().medium_reaper().low();
        let proj_ctx = proj.context();
        let time = position.as_seconds();
        let result = sw::time_to_beats(low, proj_ctx, time);
        let final_measure = match measure_mode {
            MeasureMode::IgnoreMeasure => 0,
            MeasureMode::FromMeasureAtIndex(idx) => idx - result.measure_index,
        };
        let ts = sw::get_time_sig_at_time(low, proj_ctx, time);
        TimeToBeatsResult {
            full_beats: PositionInBeats::from_beats(result.full_beats),
            measure_index: final_measure,
            beats_since_measure: PositionInBeats::from_beats(result.beats_frac),
            time_signature: TimeSignature::new(ts.num as u32, ts.denom as u32),
        }
    }

    fn beats_to_time(
        &self,
        project: ProjectContext,
        position: PositionInBeats,
        measure_mode: MeasureMode,
    ) -> PositionInSeconds {
        let Some(proj) = resolve_project(&project) else {
            return PositionInSeconds::default();
        };
        let low = Reaper::get().medium_reaper().low();
        let proj_ctx = proj.context();
        let full_beats = position.as_beats();
        let adjusted_beats = match measure_mode {
            MeasureMode::IgnoreMeasure => full_beats,
            MeasureMode::FromMeasureAtIndex(measure_idx) => {
                let measure_start_time = sw::get_measure_info(low, proj_ctx, measure_idx);
                let tb = sw::time_to_beats(low, proj_ctx, measure_start_time);
                tb.full_beats + full_beats
            }
        };
        let time = sw::beats_to_time(low, proj_ctx, adjusted_beats, None);
        PositionInSeconds::from_seconds(time)
    }

    fn time_to_quarter_notes(
        &self,
        project: ProjectContext,
        position: PositionInSeconds,
    ) -> TimeToQuarterNotesResult {
        let Some(proj) = resolve_project(&project) else {
            return TimeToQuarterNotesResult::default();
        };
        let low = Reaper::get().medium_reaper().low();
        let proj_ctx = proj.context();
        let time = position.as_seconds();
        let qn_position = sw::time_to_qn(low, proj_ctx, time);
        let minfo = sw::qn_to_measures(low, proj_ctx, qn_position);
        let qn_since_measure = qn_position - minfo.qn_start;
        let ts = sw::get_time_sig_at_time(low, proj_ctx, time);
        TimeToQuarterNotesResult {
            quarter_notes: PositionInQuarterNotes::from_quarter_notes(qn_position),
            measure_index: minfo.measure_index,
            quarter_notes_since_measure: PositionInQuarterNotes::from_quarter_notes(
                qn_since_measure,
            ),
            time_signature: TimeSignature::new(ts.num as u32, ts.denom as u32),
        }
    }

    fn quarter_notes_to_time(
        &self,
        project: ProjectContext,
        position: PositionInQuarterNotes,
    ) -> PositionInSeconds {
        let Some(proj) = resolve_project(&project) else {
            return PositionInSeconds::default();
        };
        let low = Reaper::get().medium_reaper().low();
        PositionInSeconds::from_seconds(sw::qn_to_time(
            low,
            proj.context(),
            position.as_quarter_notes(),
        ))
    }

    fn quarter_notes_to_measure(
        &self,
        project: ProjectContext,
        position: PositionInQuarterNotes,
    ) -> QuarterNotesToMeasureResult {
        let Some(proj) = resolve_project(&project) else {
            return QuarterNotesToMeasureResult::default();
        };
        let low = Reaper::get().medium_reaper().low();
        let proj_ctx = proj.context();
        let qn = position.as_quarter_notes();
        let minfo = sw::qn_to_measures(low, proj_ctx, qn);
        let time = sw::qn_to_time(low, proj_ctx, qn);
        let ts = sw::get_time_sig_at_time(low, proj_ctx, time);
        QuarterNotesToMeasureResult {
            measure_index: minfo.measure_index,
            start: PositionInQuarterNotes::from_quarter_notes(minfo.qn_start),
            end: PositionInQuarterNotes::from_quarter_notes(minfo.qn_end),
            time_signature: TimeSignature::new(ts.num as u32, ts.denom as u32),
        }
    }

    fn beats_to_quarter_notes(
        &self,
        project: ProjectContext,
        position: PositionInBeats,
    ) -> PositionInQuarterNotes {
        let time = self.beats_to_time(project.clone(), position, MeasureMode::IgnoreMeasure);
        self.time_to_quarter_notes(project, time).quarter_notes
    }

    fn quarter_notes_to_beats(
        &self,
        project: ProjectContext,
        position: PositionInQuarterNotes,
    ) -> PositionInBeats {
        let time = self.quarter_notes_to_time(project.clone(), position);
        self.time_to_beats(project, time, MeasureMode::IgnoreMeasure)
            .full_beats
    }
}
