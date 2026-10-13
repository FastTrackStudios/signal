//! `impl PositionConversion for Standalone` — uses canonical
//! `ProjectState::tempo_points` for linear approximations.

use daw_proto::{
    MeasureMode, PositionConversion, PositionInBeats, PositionInQuarterNotes, PositionInSeconds,
    ProjectContext, QuarterNotesToMeasureResult, TimeSignature, TimeToBeatsResult,
    TimeToQuarterNotesResult,
};

use crate::sync::Standalone;

fn resolve_project(daw: &Standalone, ctx: &ProjectContext) -> Option<String> {
    match ctx {
        ProjectContext::Project(guid) => Some(guid.clone()),
        ProjectContext::Current => {
            let state = daw.state.lock().ok()?;
            state.current_project_guid.clone()
        }
    }
}

fn bpm_at(daw: &Standalone, guid: &str, _time_seconds: f64) -> f64 {
    daw.with_project(guid, |p| p.transport.tempo.bpm())
        .unwrap_or(120.0)
}

fn time_sig_at(daw: &Standalone, guid: &str) -> TimeSignature {
    daw.with_project(guid, |p| {
        TimeSignature::new(
            p.transport.time_signature.numerator,
            p.transport.time_signature.denominator,
        )
    })
    .unwrap_or_else(|_| TimeSignature::new(4, 4))
}

impl PositionConversion for Standalone {
    fn time_to_beats(
        &self,
        project: ProjectContext,
        position: PositionInSeconds,
        _measure_mode: MeasureMode,
    ) -> TimeToBeatsResult {
        let Some(guid) = resolve_project(self, &project) else {
            return TimeToBeatsResult::default();
        };
        let bpm = bpm_at(self, &guid, position.as_seconds());
        let ts = time_sig_at(self, &guid);
        let beats = position.as_seconds() * bpm / 60.0;
        let beats_per_measure = ts.numerator as f64;
        let measure = (beats / beats_per_measure).floor() as i32;
        let beats_since = beats - (measure as f64) * beats_per_measure;
        TimeToBeatsResult {
            full_beats: PositionInBeats::from_beats(beats),
            measure_index: measure,
            beats_since_measure: PositionInBeats::from_beats(beats_since),
            time_signature: ts,
        }
    }

    fn beats_to_time(
        &self,
        project: ProjectContext,
        position: PositionInBeats,
        _measure_mode: MeasureMode,
    ) -> PositionInSeconds {
        let Some(guid) = resolve_project(self, &project) else {
            return PositionInSeconds::default();
        };
        let bpm = bpm_at(self, &guid, 0.0);
        PositionInSeconds::from_seconds(position.as_beats() * 60.0 / bpm)
    }

    fn time_to_quarter_notes(
        &self,
        project: ProjectContext,
        position: PositionInSeconds,
    ) -> TimeToQuarterNotesResult {
        let Some(guid) = resolve_project(self, &project) else {
            return TimeToQuarterNotesResult::default();
        };
        let bpm = bpm_at(self, &guid, position.as_seconds());
        let ts = time_sig_at(self, &guid);
        let qn = position.as_seconds() * bpm / 60.0;
        let qn_per_measure = ts.numerator as f64 * 4.0 / ts.denominator as f64;
        let measure = (qn / qn_per_measure).floor() as i32;
        let qn_since = qn - (measure as f64) * qn_per_measure;
        TimeToQuarterNotesResult {
            quarter_notes: PositionInQuarterNotes::from_quarter_notes(qn),
            measure_index: measure,
            quarter_notes_since_measure: PositionInQuarterNotes::from_quarter_notes(qn_since),
            time_signature: ts,
        }
    }

    fn quarter_notes_to_time(
        &self,
        project: ProjectContext,
        position: PositionInQuarterNotes,
    ) -> PositionInSeconds {
        let Some(guid) = resolve_project(self, &project) else {
            return PositionInSeconds::default();
        };
        let bpm = bpm_at(self, &guid, 0.0);
        PositionInSeconds::from_seconds(position.as_quarter_notes() * 60.0 / bpm)
    }

    fn quarter_notes_to_measure(
        &self,
        project: ProjectContext,
        position: PositionInQuarterNotes,
    ) -> QuarterNotesToMeasureResult {
        let Some(guid) = resolve_project(self, &project) else {
            return QuarterNotesToMeasureResult::default();
        };
        let ts = time_sig_at(self, &guid);
        let qn = position.as_quarter_notes();
        let qn_per_measure = ts.numerator as f64 * 4.0 / ts.denominator as f64;
        let measure = (qn / qn_per_measure).floor() as i32;
        let start = measure as f64 * qn_per_measure;
        QuarterNotesToMeasureResult {
            measure_index: measure,
            start: PositionInQuarterNotes::from_quarter_notes(start),
            end: PositionInQuarterNotes::from_quarter_notes(start + qn_per_measure),
            time_signature: ts,
        }
    }

    fn beats_to_quarter_notes(
        &self,
        _project: ProjectContext,
        position: PositionInBeats,
    ) -> PositionInQuarterNotes {
        PositionInQuarterNotes::from_quarter_notes(position.as_beats())
    }

    fn quarter_notes_to_beats(
        &self,
        _project: ProjectContext,
        position: PositionInQuarterNotes,
    ) -> PositionInBeats {
        PositionInBeats::from_beats(position.as_quarter_notes())
    }
}
