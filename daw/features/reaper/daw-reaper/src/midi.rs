//! `impl Midi for Reaper` — MIDI note/CC/event read+write for takes.

use crate::project_context::resolve_project_context;
use crate::safe_wrappers::item as item_sw;
use crate::safe_wrappers::midi as sw;
use daw_proto::{
    HumanizeParams, ItemRef, Midi, MidiCC, MidiCCCreate, MidiNote, MidiNoteCreate, MidiPitchBend,
    MidiPitchBendCreate, MidiProgramChange, MidiSysEx, MidiTakeContent, MidiTakeLocation,
    MidiTakeSnapshot, PpqRange, ProjectContext, QuantizeParams, TakeRef, TrackRef, WriteMode,
};
use reaper_medium::{MediaItem, MediaItemTake, ProjectContext as ReaperProjectContext};
use tracing::warn;

// =============================================================================
// Public sync helpers — callable directly from the main thread
// =============================================================================

/// Create a new empty MIDI item on a track, returning the active take.
///
/// Must be called from the main thread.
pub fn create_midi_item_on_main_thread(
    track: reaper_medium::MediaTrack,
    start_seconds: f64,
    end_seconds: f64,
) -> Option<MediaItemTake> {
    let low = reaper_high::Reaper::get().medium_reaper().low();
    let item = sw::create_new_midi_item(low, track, start_seconds, end_seconds)?;
    sw::get_active_take(low, item)
}

/// Insert MIDI notes into a take, converting quarter-note positions to PPQ.
///
/// Each `MidiNoteCreate` contains `start_ppq` and `length_ppq`, but here we
/// treat `start_ppq` as a project quarter-note position and convert it to PPQ
/// using `MIDI_GetPPQPosFromProjQN`. This matches the guide_track use-case
/// where note positions are in quarter-notes.
///
/// Must be called from the main thread.
pub fn add_notes_to_take_on_main_thread(take: MediaItemTake, notes: &[MidiNoteCreate]) {
    let low = reaper_high::Reaper::get().medium_reaper().low();

    for note in notes {
        let start_ppq = sw::get_ppq_pos_from_proj_qn(low, take, note.start_ppq);
        let end_ppq = start_ppq + note.length_ppq;

        sw::insert_note(
            low,
            take,
            false,                // selected
            false,                // muted
            start_ppq,            // startppqpos
            end_ppq,              // endppqpos
            note.channel as i32,  // channel
            note.pitch as i32,    // pitch
            note.velocity as i32, // velocity
        );
    }

    // Sort notes after bulk insertion
    sw::sort(low, take);
}

// =============================================================================
// Resolution helpers (used here and by other modules, notably peak.rs)
// =============================================================================

fn get_item_state_chunk(
    medium: &reaper_medium::Reaper,
    item: MediaItem,
    buffer_size: usize,
) -> Option<String> {
    item_sw::get_item_state_chunk(medium.low(), item, buffer_size)
}

fn extract_guid_from_chunk(chunk: &str) -> Option<String> {
    for line in chunk.lines() {
        let trimmed = line.trim();
        // IGUID is the item GUID, GUID is the take GUID
        if let Some(rest) = trimmed.strip_prefix("IGUID ") {
            return Some(rest.trim().to_string());
        }
    }
    None
}

pub fn resolve_item(
    medium: &reaper_medium::Reaper,
    project_ctx: ReaperProjectContext,
    item_ref: &ItemRef,
) -> Option<MediaItem> {
    match item_ref {
        ItemRef::ProjectIndex(index) => medium.get_media_item(project_ctx, *index),
        ItemRef::Index(index) => medium.get_media_item(project_ctx, *index),
        ItemRef::Guid(guid) => {
            let count = medium.count_media_items(project_ctx);
            for i in 0..count {
                let Some(item) = medium.get_media_item(project_ctx, i) else {
                    continue;
                };
                let Some(chunk) = get_item_state_chunk(medium, item, 2048) else {
                    continue;
                };
                if let Some(item_guid) = extract_guid_from_chunk(&chunk)
                    && &item_guid == guid
                {
                    return Some(item);
                }
            }
            None
        }
    }
}

pub fn resolve_take(
    medium: &reaper_medium::Reaper,
    item: MediaItem,
    take_ref: &TakeRef,
) -> Option<MediaItemTake> {
    let low = medium.low();
    match take_ref {
        TakeRef::Active => crate::safe_wrappers::item::get_active_take(medium, item),
        TakeRef::Index(index) => item_sw::get_take(low, item, *index as i32),
        TakeRef::Guid(_) => crate::safe_wrappers::item::get_active_take(medium, item),
    }
}

pub(crate) fn resolve_take_for_location(
    medium: &reaper_medium::Reaper,
    location: &MidiTakeLocation,
) -> Option<MediaItemTake> {
    let project_ctx = resolve_project_context(&location.project);
    let item = resolve_item(medium, project_ctx, &location.item)?;
    resolve_take(medium, item, &location.take)
}

pub(crate) fn read_notes(medium: &reaper_medium::Reaper, take: MediaItemTake) -> Vec<MidiNote> {
    let low = medium.low();
    let counts = sw::count_events(low, take);

    let mut notes = Vec::with_capacity(counts.notes.max(0) as usize);
    for index in 0..counts.notes {
        let Some(n) = sw::get_note(low, take, index) else {
            continue;
        };
        notes.push(MidiNote {
            index: index as u32,
            channel: n.channel.clamp(0, 15) as u8,
            pitch: n.pitch.clamp(0, 127) as u8,
            velocity: n.velocity.clamp(1, 127) as u8,
            start_ppq: n.start_ppq,
            length_ppq: (n.end_ppq - n.start_ppq).max(0.0),
            selected: n.selected,
            muted: n.muted,
        });
    }
    notes
}

fn readonly_warn(method: &str) {
    warn!("Midi::{method} is read-only in this pass; skipping mutation");
}

/// Resolve `location` and read note `index` out of it.
fn read_note(location: &MidiTakeLocation, index: u32) -> Option<(MediaItemTake, sw::MidiNoteRaw)> {
    let medium = reaper_high::Reaper::get().medium_reaper();
    let take = resolve_take_for_location(medium, location)?;
    let note = sw::get_note(medium.low(), take, index as i32)?;
    Some((take, note))
}

/// Apply `edit` to note `index` of the take at `location`.
///
/// Sorts afterwards because the note setters accept position and pitch
/// changes, either of which can reorder the take. `MIDI_SetNote` is
/// called with `no_sort`, so without this a moved note would leave the
/// event list unsorted and REAPER's own reads inconsistent. A pure
/// velocity edit doesn't strictly need it, but paying one sort per edit
/// is not worth the class of bug that skipping it invites — and callers
/// touching many notes should use the batch helpers rather than looping
/// here anyway.
fn edit_note(location: &MidiTakeLocation, index: u32, edit: sw::MidiNoteEdit) {
    let medium = reaper_high::Reaper::get().medium_reaper();
    let Some(take) = resolve_take_for_location(medium, location) else {
        warn!("Midi::set_note_*: could not resolve take for {location:?}");
        return;
    };
    let low = medium.low();
    if !sw::set_note(low, take, index as i32, edit) {
        warn!("Midi::set_note_*: REAPER rejected the edit for note {index}");
        return;
    }
    sw::sort(low, take);
}

// =============================================================================
// `impl Midi for Reaper`
// =============================================================================

impl Midi for crate::Reaper {
    // ── Bulk take access ───────────────────────────────────────────

    fn read_take(&self, location: MidiTakeLocation) -> MidiTakeSnapshot {
        let notes = Midi::notes(self, location.clone());
        MidiTakeSnapshot {
            length_ppq: notes.iter().map(|n| n.end_ppq()).fold(0.0, f64::max),
            notes,
            ccs: Midi::ccs(self, location.clone(), None),
            pitch_bends: Midi::pitch_bends(self, location.clone()),
            channel_pressures: Midi::channel_pressures(self, location.clone()),
            poly_pressures: Midi::poly_pressures(self, location.clone()),
            note_expressions: Midi::note_expressions(self, location),
            // REAPER's MIDI API is quarter-note based; 960 is the
            // resolution its own editor reports.
            ppq: 960.0,
        }
    }

    fn write_take(
        &self,
        location: MidiTakeLocation,
        content: MidiTakeContent,
        mode: WriteMode,
    ) -> Vec<u32> {
        if mode == WriteMode::Replace {
            let existing: Vec<u32> = (0..Midi::note_count(self, location.clone())).collect();
            Midi::delete_notes(self, location.clone(), existing);
        }
        let indices = Midi::add_notes(self, location.clone(), content.notes);
        for cc in content.ccs {
            Midi::add_cc(self, location.clone(), cc);
        }
        for pb in content.pitch_bends {
            Midi::add_pitch_bend(self, location.clone(), pb);
        }
        for ne in content.note_expressions {
            Midi::add_note_expression(self, location.clone(), ne);
        }
        indices
    }

    fn replace_range(
        &self,
        location: MidiTakeLocation,
        range: PpqRange,
        content: MidiTakeContent,
    ) -> Vec<u32> {
        // Descending, because each delete renumbers everything above it.
        let mut doomed: Vec<u32> = Midi::notes(self, location.clone())
            .iter()
            .filter(|n| n.overlaps(range.start, range.end))
            .map(|n| n.index)
            .collect();
        doomed.sort_unstable_by(|a, b| b.cmp(a));
        Midi::delete_notes(self, location.clone(), doomed);
        Midi::write_take(self, location, content, WriteMode::Merge)
    }

    fn read_midi_file(&self, path: String, track_index: u32) -> Option<MidiTakeSnapshot> {
        // The same pure-Rust reader the standalone backend uses. Going
        // through REAPER would mean importing the file into a project
        // first, which is a side effect a read should not have.
        daw_proto::midi::smf::read(&path, track_index as usize)
    }

    fn write_midi_file(&self, path: String, content: MidiTakeContent, ppq: f64) -> bool {
        daw_proto::midi::smf::write(&path, &content, ppq).is_ok()
    }

    fn notes(&self, location: MidiTakeLocation) -> Vec<MidiNote> {
        let medium = reaper_high::Reaper::get().medium_reaper();
        let Some(take) = resolve_take_for_location(medium, &location) else {
            return Vec::new();
        };
        read_notes(medium, take)
    }

    fn notes_in_range(&self, location: MidiTakeLocation, range: PpqRange) -> Vec<MidiNote> {
        self.notes(location)
            .into_iter()
            .filter(|note| note.overlaps(range.start, range.end))
            .collect()
    }

    fn selected_notes(&self, location: MidiTakeLocation) -> Vec<MidiNote> {
        self.notes(location)
            .into_iter()
            .filter(|note| note.selected)
            .collect()
    }

    fn note_count(&self, location: MidiTakeLocation) -> u32 {
        self.notes(location).len() as u32
    }

    fn create_midi_item(
        &self,
        project: ProjectContext,
        track: TrackRef,
        start_seconds: f64,
        end_seconds: f64,
    ) -> Option<MidiTakeLocation> {
        let reaper = reaper_high::Reaper::get();
        let proj = match &project {
            ProjectContext::Current => reaper.current_project(),
            ProjectContext::Project(guid) => crate::project_context::find_project_by_guid(guid)?,
        };
        let track_obj = crate::track::resolve_track_pub(&proj, &track)?;
        let raw_track = track_obj.raw().ok()?;
        let take = create_midi_item_on_main_thread(raw_track, start_seconds, end_seconds)?;

        let medium = reaper.medium_reaper();
        let low = medium.low();
        let item = item_sw::get_take_item(low, take)?;

        let item_guid = crate::item::item_guid_string(medium, item);
        if item_guid.is_empty() {
            return None;
        }

        Some(MidiTakeLocation::active(project, ItemRef::Guid(item_guid)))
    }

    fn add_note(&self, location: MidiTakeLocation, note: MidiNoteCreate) -> u32 {
        let indices = self.add_notes(location, vec![note]);
        indices.into_iter().next().unwrap_or(0)
    }

    fn add_notes(&self, location: MidiTakeLocation, notes: Vec<MidiNoteCreate>) -> Vec<u32> {
        let medium = reaper_high::Reaper::get().medium_reaper();
        let Some(take) = resolve_take_for_location(medium, &location) else {
            return Vec::new();
        };
        let count_before = read_notes(medium, take).len() as u32;
        add_notes_to_take_on_main_thread(take, &notes);
        (count_before..count_before + notes.len() as u32).collect()
    }

    fn add_notes_ppq(&self, location: MidiTakeLocation, notes: Vec<MidiNoteCreate>) -> Vec<u32> {
        let medium = reaper_high::Reaper::get().medium_reaper();
        let Some(take) = resolve_take_for_location(medium, &location) else {
            return Vec::new();
        };
        let low = medium.low();
        let count_before = read_notes(medium, take).len() as u32;

        for note in &notes {
            // The whole point of this method: positions go in as they
            // came out of `notes()`, with no quarter-note conversion.
            sw::insert_note(
                low,
                take,
                false,
                false,
                note.start_ppq,
                note.start_ppq + note.length_ppq,
                i32::from(note.channel & 0x0F),
                i32::from(note.pitch & 0x7F),
                i32::from(note.velocity.clamp(1, 127)),
            );
        }
        sw::sort(low, take);

        (count_before..count_before + notes.len() as u32).collect()
    }

    fn delete_note(&self, location: MidiTakeLocation, index: u32) {
        self.delete_notes(location, vec![index]);
    }

    fn delete_notes(&self, location: MidiTakeLocation, indices: Vec<u32>) {
        let medium = reaper_high::Reaper::get().medium_reaper();
        let Some(take) = resolve_take_for_location(medium, &location) else {
            warn!("Midi::delete_notes: could not resolve take for {location:?}");
            return;
        };
        let low = medium.low();

        // Highest index first. Deleting renumbers everything above the
        // removed note, so ascending order would delete the wrong notes
        // from the second one onward — and silently, since every index
        // stays in range. Dedup because a repeated index would then
        // delete an innocent neighbour.
        let mut indices = indices;
        indices.sort_unstable_by(|a, b| b.cmp(a));
        indices.dedup();

        for index in indices {
            if !sw::delete_note(low, take, index as i32) {
                warn!("Midi::delete_notes: REAPER rejected deleting note {index}");
            }
        }
        sw::sort(low, take);
    }

    fn delete_selected_notes(&self, location: MidiTakeLocation) {
        let selected: Vec<u32> = self
            .selected_notes(location.clone())
            .into_iter()
            .map(|n| n.index)
            .collect();
        self.delete_notes(location, selected);
    }

    fn set_note_pitch(&self, location: MidiTakeLocation, index: u32, pitch: u8) {
        edit_note(
            &location,
            index,
            sw::MidiNoteEdit {
                pitch: Some(i32::from(pitch & 0x7F)),
                ..Default::default()
            },
        );
    }

    fn set_note_velocity(&self, location: MidiTakeLocation, index: u32, velocity: u8) {
        edit_note(
            &location,
            index,
            sw::MidiNoteEdit {
                // Clamped off zero: a note-on with velocity 0 is a
                // note-off, so passing it through would delete the note
                // rather than quieten it.
                velocity: Some(i32::from(velocity.clamp(1, 127))),
                ..Default::default()
            },
        );
    }

    fn set_note_position(&self, location: MidiTakeLocation, index: u32, start_ppq: f64) {
        // Moving the start alone would stretch the note, so the end moves
        // with it — "position" means the note slides, not resizes.
        let Some((take, note)) = read_note(&location, index) else {
            return;
        };
        let length = note.end_ppq - note.start_ppq;
        let _ = take;
        edit_note(
            &location,
            index,
            sw::MidiNoteEdit {
                start_ppq: Some(start_ppq),
                end_ppq: Some(start_ppq + length),
                ..Default::default()
            },
        );
    }

    fn set_note_length(&self, location: MidiTakeLocation, index: u32, length_ppq: f64) {
        let Some((_, note)) = read_note(&location, index) else {
            return;
        };
        edit_note(
            &location,
            index,
            sw::MidiNoteEdit {
                end_ppq: Some(note.start_ppq + length_ppq.max(1.0)),
                ..Default::default()
            },
        );
    }

    fn set_note_channel(&self, location: MidiTakeLocation, index: u32, channel: u8) {
        edit_note(
            &location,
            index,
            sw::MidiNoteEdit {
                channel: Some(i32::from(channel & 0x0F)),
                ..Default::default()
            },
        );
    }

    fn set_note_selected(&self, location: MidiTakeLocation, index: u32, selected: bool) {
        edit_note(
            &location,
            index,
            sw::MidiNoteEdit {
                selected: Some(selected),
                ..Default::default()
            },
        );
    }

    fn set_note_muted(&self, location: MidiTakeLocation, index: u32, muted: bool) {
        edit_note(
            &location,
            index,
            sw::MidiNoteEdit {
                muted: Some(muted),
                ..Default::default()
            },
        );
    }

    fn select_all_notes(&self, _location: MidiTakeLocation, _selected: bool) {
        readonly_warn("select_all_notes");
    }

    fn transpose_notes(&self, _location: MidiTakeLocation, _indices: Vec<u32>, _semitones: i8) {
        readonly_warn("transpose_notes");
    }

    fn quantize_notes(&self, _location: MidiTakeLocation, _params: QuantizeParams) {
        readonly_warn("quantize_notes");
    }

    fn humanize_notes(&self, _location: MidiTakeLocation, _params: HumanizeParams) {
        readonly_warn("humanize_notes");
    }

    fn ccs(&self, _location: MidiTakeLocation, _controller: Option<u8>) -> Vec<MidiCC> {
        Vec::new()
    }

    fn add_cc(&self, _location: MidiTakeLocation, _cc: MidiCCCreate) -> u32 {
        readonly_warn("add_cc");
        0
    }

    fn delete_cc(&self, _location: MidiTakeLocation, _index: u32) {
        readonly_warn("delete_cc");
    }

    fn set_cc_value(&self, _location: MidiTakeLocation, _index: u32, _value: u8) {
        readonly_warn("set_cc_value");
    }

    fn pitch_bends(&self, _location: MidiTakeLocation) -> Vec<MidiPitchBend> {
        Vec::new()
    }

    fn add_pitch_bend(&self, _location: MidiTakeLocation, _pb: MidiPitchBendCreate) -> u32 {
        readonly_warn("add_pitch_bend");
        0
    }

    fn program_changes(&self, _location: MidiTakeLocation) -> Vec<MidiProgramChange> {
        Vec::new()
    }

    fn sysex(&self, _location: MidiTakeLocation) -> Vec<MidiSysEx> {
        Vec::new()
    }

    // Read-only stubs for newer mutation methods in the Midi trait.
    fn delete_pitch_bend(&self, _: MidiTakeLocation, _: u32) {
        readonly_warn("delete_pitch_bend");
    }
    fn set_pitch_bend_value(&self, _: MidiTakeLocation, _: u32, _: i16) {
        readonly_warn("set_pitch_bend_value");
    }
    fn add_program_change(
        &self,
        _: MidiTakeLocation,
        _: daw_proto::MidiProgramChangeCreate,
    ) -> u32 {
        readonly_warn("add_program_change");
        0
    }
    fn delete_program_change(&self, _: MidiTakeLocation, _: u32) {
        readonly_warn("delete_program_change");
    }
    fn set_program(&self, _: MidiTakeLocation, _: u32, _: u8) {
        readonly_warn("set_program");
    }
    fn add_sysex(&self, _: MidiTakeLocation, _: daw_proto::MidiSysExCreate) -> u32 {
        readonly_warn("add_sysex");
        0
    }
    fn delete_sysex(&self, _: MidiTakeLocation, _: u32) {
        readonly_warn("delete_sysex");
    }

    // Channel-pressure + poly-pressure read-only stubs, mirroring the
    // pattern above for CC/pitch-bend/program-change. REAPER's MIDI take
    // accessors expose these via `MIDI_GetCC` etc.; wiring them up is
    // tracked separately from this WindowManager work.
    fn channel_pressures(&self, _: MidiTakeLocation) -> Vec<daw_proto::MidiChannelPressure> {
        Vec::new()
    }
    fn add_channel_pressure(
        &self,
        _: MidiTakeLocation,
        _: daw_proto::MidiChannelPressureCreate,
    ) -> u32 {
        readonly_warn("add_channel_pressure");
        0
    }
    fn delete_channel_pressure(&self, _: MidiTakeLocation, _: u32) {
        readonly_warn("delete_channel_pressure");
    }
    fn set_channel_pressure_value(&self, _: MidiTakeLocation, _: u32, _: u8) {
        readonly_warn("set_channel_pressure_value");
    }
    fn poly_pressures(&self, _: MidiTakeLocation) -> Vec<daw_proto::MidiPolyPressure> {
        Vec::new()
    }
    fn add_poly_pressure(&self, _: MidiTakeLocation, _: daw_proto::MidiPolyPressureCreate) -> u32 {
        readonly_warn("add_poly_pressure");
        0
    }
    fn delete_poly_pressure(&self, _: MidiTakeLocation, _: u32) {
        readonly_warn("delete_poly_pressure");
    }
    fn set_poly_pressure_value(&self, _: MidiTakeLocation, _: u32, _: u8) {
        readonly_warn("set_poly_pressure_value");
    }
    fn note_expressions(&self, _: MidiTakeLocation) -> Vec<daw_proto::MidiNoteExpression> {
        Vec::new()
    }
    fn add_note_expression(
        &self,
        _: MidiTakeLocation,
        _: daw_proto::MidiNoteExpressionCreate,
    ) -> u32 {
        readonly_warn("add_note_expression");
        0
    }
    fn delete_note_expression(&self, _: MidiTakeLocation, _: u32) {
        readonly_warn("delete_note_expression");
    }
    fn set_note_expression_value(&self, _: MidiTakeLocation, _: u32, _: f64) {
        readonly_warn("set_note_expression_value");
    }
}
