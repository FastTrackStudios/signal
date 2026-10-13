//! Safe wrappers for REAPER TimeMap APIs.

use super::ReaperLow;
use reaper_medium::ProjectContext;

/// Result from `TimeMap2_timeToBeats`.
pub struct TimeToBeatsResult {
    pub beats_frac: f64,
    pub measure_index: i32,
    pub beats_since_measure: i32,
    pub full_beats: f64,
    pub denom: i32,
}

/// Convert time to beats using `TimeMap2_timeToBeats`.
pub fn time_to_beats(low: &ReaperLow, project: ProjectContext, time: f64) -> TimeToBeatsResult {
    let mut measure_index: i32 = 0;
    let mut beats_since_measure: i32 = 0;
    let mut full_beats: f64 = 0.0;
    let mut denom: i32 = 0;

    let beats_frac = unsafe {
        low.TimeMap2_timeToBeats(
            project.to_raw(),
            time,
            &mut measure_index,
            &mut beats_since_measure,
            &mut full_beats,
            &mut denom,
        )
    };

    TimeToBeatsResult {
        beats_frac,
        measure_index,
        beats_since_measure,
        full_beats,
        denom,
    }
}

/// Convert beats to time using `TimeMap2_beatsToTime`.
///
/// `measures_in` optionally specifies the measure context. Pass `None` for the
/// default behavior.
pub fn beats_to_time(
    low: &ReaperLow,
    project: ProjectContext,
    beats: f64,
    measures_in: Option<i32>,
) -> f64 {
    let ptr = measures_in
        .as_ref()
        .map_or(std::ptr::null(), |v| v as *const i32);
    unsafe { low.TimeMap2_beatsToTime(project.to_raw(), beats, ptr) }
}

/// Convert time to quarter notes using `TimeMap2_timeToQN`.
pub fn time_to_qn(low: &ReaperLow, project: ProjectContext, time: f64) -> f64 {
    unsafe { low.TimeMap2_timeToQN(project.to_raw(), time) }
}

/// Convert quarter notes to time using `TimeMap2_QNToTime`.
pub fn qn_to_time(low: &ReaperLow, project: ProjectContext, qn: f64) -> f64 {
    unsafe { low.TimeMap2_QNToTime(project.to_raw(), qn) }
}

/// Result from `TimeMap_QNToMeasures`.
pub struct QnToMeasuresResult {
    pub measure_index: i32,
    pub qn_start: f64,
    pub qn_end: f64,
}

/// Convert quarter notes to measure info.
pub fn qn_to_measures(low: &ReaperLow, project: ProjectContext, qn: f64) -> QnToMeasuresResult {
    let mut qn_start: f64 = 0.0;
    let mut qn_end: f64 = 0.0;
    let measure_index =
        unsafe { low.TimeMap_QNToMeasures(project.to_raw(), qn, &mut qn_start, &mut qn_end) };
    QnToMeasuresResult {
        measure_index,
        qn_start,
        qn_end,
    }
}

/// Result from `TimeMap_GetTimeSigAtTime`.
pub struct TimeSigAtTime {
    pub num: i32,
    pub denom: i32,
    pub tempo: f64,
}

/// Get the time signature and tempo at a given time position.
pub fn get_time_sig_at_time(low: &ReaperLow, project: ProjectContext, time: f64) -> TimeSigAtTime {
    let mut num: i32 = 4;
    let mut denom: i32 = 4;
    let mut tempo: f64 = 120.0;
    unsafe {
        low.TimeMap_GetTimeSigAtTime(project.to_raw(), time, &mut num, &mut denom, &mut tempo);
    }
    TimeSigAtTime { num, denom, tempo }
}

/// Get measure info at a given measure index.
///
/// Returns the time position (seconds) of the measure start.
pub fn get_measure_info(low: &ReaperLow, project: ProjectContext, measure_index: i32) -> f64 {
    unsafe {
        low.TimeMap_GetMeasureInfo(
            project.to_raw(),
            measure_index,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    }
}

/// Standalone `TimeMap_QNToTime` (without project context — uses current).
pub fn qn_to_time_current(low: &ReaperLow, qn: f64) -> f64 {
    low.TimeMap_QNToTime(qn)
}
