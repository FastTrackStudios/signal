//! `impl Midi for Standalone` — note storage in `ProjectState.midi_notes`.
//!
//! Implemented surface:
//! - `create_midi_item` — create an `Item` + active MIDI `Take` on a track
//! - Note CRUD: `add_note`, `add_notes`, `delete_note`, `delete_notes`,
//!   `delete_selected_notes`
//! - Note state setters: pitch / velocity / position / length /
//!   channel / selected / muted
//! - Note batch ops: `select_all_notes`, `transpose_notes`,
//!   `quantize_notes`, `humanize_notes`
//! - Queries: `notes`, `notes_in_range`, `selected_notes`, `note_count`
//!
//! Still `todo!()`: CCs / pitch bends / program changes / sysex
//! (not modeled in `ProjectState` yet — file an issue when needed).
//!
//! Note indices are re-issued on every mutating call so the `index`
//! field on returned `MidiNote`s is consistent with its position in
//! the storage vec. References across mutations aren't stable —
//! callers should re-fetch.

use daw_proto::item::SourceType;
use daw_proto::midi::{
    HumanizeParams, Midi, MidiCC, MidiCCCreate, MidiChannelPressure, MidiChannelPressureCreate,
    MidiNote, MidiNoteCreate, MidiNoteExpression, MidiNoteExpressionCreate, MidiPitchBend,
    MidiPitchBendCreate, MidiPolyPressure, MidiPolyPressureCreate, MidiProgramChange,
    MidiProgramChangeCreate, MidiSysEx, MidiSysExCreate, MidiTakeContent, MidiTakeLocation,
    MidiTakeSnapshot, PpqRange, QuantizeParams, WriteMode,
};
use daw_proto::primitives::{Duration, PositionInSeconds};
use daw_proto::project::ProjectContext;
use daw_proto::{Item, ItemRef, Take, TakeRef, TrackRef};
use uuid::Uuid;

use crate::sync::{Standalone, TakeList};

/// The tick resolution [`Midi::add_notes`] lengths arrive in: REAPER's
/// default, which is what the callers compute them at.
const TICKS_PER_QN: f64 = 960.0;

fn resolve_project(daw: &Standalone, ctx: &ProjectContext) -> Option<String> {
    match ctx {
        ProjectContext::Project(guid) => Some(guid.clone()),
        ProjectContext::Current => {
            let state = daw.state.lock().ok()?;
            state.current_project_guid.clone()
        }
    }
}

/// Resolve `(item, take)` refs in the context of a project to the
/// take's GUID, used to key into `ProjectState.midi_notes`.
fn resolve_take_guid(daw: &Standalone, location: &MidiTakeLocation) -> Option<String> {
    let project_guid = resolve_project(daw, &location.project)?;
    daw.with_project(&project_guid, |p| {
        // Resolve item GUID.
        let item_guid = match &location.item {
            ItemRef::Guid(g) => p.items.contains_key(g).then(|| g.clone()),
            ItemRef::Index(_) | ItemRef::ProjectIndex(_) => {
                // ItemRef::Index = within-track index, ProjectIndex =
                // global. Standalone doesn't index items globally;
                // walk items_by_track to resolve. Caller should
                // generally use Guid.
                None
            }
        }?;
        let takes = p.takes.get(&item_guid)?;
        match &location.take {
            TakeRef::Guid(g) => takes
                .takes
                .iter()
                .find(|t| t.guid == *g)
                .map(|t| t.guid.clone()),
            TakeRef::Index(i) => takes.takes.get(*i as usize).map(|t| t.guid.clone()),
            TakeRef::Active => takes
                .takes
                .get(takes.active_idx as usize)
                .map(|t| t.guid.clone()),
        }
    })
    .ok()
    .flatten()
}

/// Reissue indices so the `index` field matches the vec position.
fn renumber(notes: &mut [MidiNote]) {
    for (i, n) in notes.iter_mut().enumerate() {
        n.index = i as u32;
    }
}

impl Midi for Standalone {
    // ── Bulk take access ───────────────────────────────────────────

    fn read_take(&self, location: MidiTakeLocation) -> MidiTakeSnapshot {
        MidiTakeSnapshot {
            notes: Midi::notes(self, location.clone()),
            ccs: Midi::ccs(self, location.clone(), None),
            pitch_bends: Midi::pitch_bends(self, location.clone()),
            channel_pressures: Midi::channel_pressures(self, location.clone()),
            poly_pressures: Midi::poly_pressures(self, location.clone()),
            note_expressions: Midi::note_expressions(self, location.clone()),
            ppq: 960.0,
            length_ppq: Midi::notes(self, location)
                .iter()
                .map(|n| n.end_ppq())
                .fold(0.0, f64::max),
        }
    }

    fn write_take(
        &self,
        location: MidiTakeLocation,
        content: MidiTakeContent,
        mode: WriteMode,
    ) -> Vec<u32> {
        if mode == WriteMode::Replace {
            // Clear first so a replace is genuinely a replace; the
            // per-index deletes below would otherwise leave the old
            // notes interleaved with the new.
            let existing: Vec<u32> = (0..Midi::note_count(self, location.clone())).collect();
            Midi::delete_notes(self, location.clone(), existing);
        }
        // Notes as `notes()` reads them, so the round-trip path.
        let indices = Midi::add_notes_ppq(self, location.clone(), content.notes);
        for cc in content.ccs {
            Midi::add_cc(self, location.clone(), cc);
        }
        for pb in content.pitch_bends {
            Midi::add_pitch_bend(self, location.clone(), pb);
        }
        for ne in content.note_expressions {
            Midi::add_note_expression(self, location.clone(), ne);
        }
        for cp in content.channel_pressures {
            Midi::add_channel_pressure(self, location.clone(), cp);
        }
        indices
    }

    fn replace_range(
        &self,
        location: MidiTakeLocation,
        range: PpqRange,
        content: MidiTakeContent,
    ) -> Vec<u32> {
        // Delete high-to-low: every removal shifts the indices above
        // it, so ascending order would delete the wrong notes.
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
        daw_proto::midi::smf::read(&path, track_index as usize)
    }

    fn write_midi_file(&self, path: String, content: MidiTakeContent, ppq: f64) -> bool {
        daw_proto::midi::smf::write(&path, &content, ppq).is_ok()
    }

    fn notes(&self, location: MidiTakeLocation) -> Vec<MidiNote> {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return Vec::new();
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return Vec::new();
        };
        self.with_project(&project_guid, |p| {
            p.midi_notes.get(&take_guid).cloned().unwrap_or_default()
        })
        .unwrap_or_default()
    }

    fn notes_in_range(&self, location: MidiTakeLocation, range: PpqRange) -> Vec<MidiNote> {
        Midi::notes(self, location)
            .into_iter()
            .filter(|n| n.overlaps(range.start, range.end))
            .collect()
    }

    fn selected_notes(&self, location: MidiTakeLocation) -> Vec<MidiNote> {
        Midi::notes(self, location)
            .into_iter()
            .filter(|n| n.selected)
            .collect()
    }

    fn note_count(&self, location: MidiTakeLocation) -> u32 {
        Midi::notes(self, location).len() as u32
    }

    fn create_midi_item(
        &self,
        project: ProjectContext,
        track: TrackRef,
        start_seconds: f64,
        end_seconds: f64,
    ) -> Option<MidiTakeLocation> {
        let project_guid = resolve_project(self, &project)?;
        let result: Option<(String, String)> = self
            .with_project_mut(&project_guid, |p| {
                // Resolve target track guid.
                let track_guid = match &track {
                    TrackRef::Guid(g) => p
                        .tracks
                        .iter()
                        .find(|t| t.guid == *g)
                        .map(|t| t.guid.clone()),
                    TrackRef::Index(i) => p.tracks.get(*i as usize).map(|t| t.guid.clone()),
                    TrackRef::Master => None, // no items on master
                }?;
                // Synthesize GUIDs.
                let item_guid = Uuid::new_v4().to_string();
                let take_guid = Uuid::new_v4().to_string();
                p.next_item_counter += 1;
                p.next_take_counter += 1;

                // Determine the item index by counting existing
                // items on the track *before* taking the mutable
                // entry handle (avoids overlapping borrows of `p`).
                let item_idx = p
                    .items_by_track
                    .get(&track_guid)
                    .map(|v| v.len() as u32)
                    .unwrap_or(0);

                let mut item = Item::default();
                item.guid = item_guid.clone();
                item.track_guid = track_guid.clone();
                item.index = item_idx;
                item.position = PositionInSeconds::from_seconds(start_seconds);
                item.length = Duration::from_seconds((end_seconds - start_seconds).max(0.0));
                item.take_count = 1;
                item.active_take_index = 0;

                p.items
                    .insert(item_guid.clone(), crate::sync::ItemEntry { item });
                p.items_by_track
                    .entry(track_guid)
                    .or_default()
                    .push(item_guid.clone());

                // Active MIDI take.
                let take = Take {
                    guid: take_guid.clone(),
                    item_guid: item_guid.clone(),
                    index: 0,
                    is_active: true,
                    name: String::new(),
                    is_midi: true,
                    midi_note_count: Some(0),
                    source_type: SourceType::Midi,
                    ..Default::default()
                };
                p.takes.insert(
                    item_guid.clone(),
                    TakeList {
                        active_idx: 0,
                        takes: vec![take],
                    },
                );
                p.midi_notes.insert(take_guid.clone(), Vec::new());
                Some((item_guid, take_guid))
            })
            .ok()
            .flatten();

        let (item_guid, _take_guid) = result?;
        Some(MidiTakeLocation::new(
            ProjectContext::Project(project_guid),
            ItemRef::Guid(item_guid),
            TakeRef::Active,
        ))
    }

    fn add_note(&self, location: MidiTakeLocation, note: MidiNoteCreate) -> u32 {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return u32::MAX;
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return u32::MAX;
        };
        self.with_project_mut(&project_guid, |p| {
            let new_count = {
                let notes = p.midi_notes.entry(take_guid.clone()).or_default();
                let idx = notes.len() as u32;
                notes.push(MidiNote {
                    index: idx,
                    channel: note.channel & 0x0F,
                    pitch: note.pitch & 0x7F,
                    velocity: note.velocity.clamp(1, 127),
                    start_ppq: note.start_ppq,
                    length_ppq: note.length_ppq.max(0.0),
                    selected: false,
                    muted: false,
                });
                (idx, notes.len() as u32)
            };
            update_take_note_count(p, &take_guid, new_count.1);
            new_count.0
        })
        .unwrap_or(u32::MAX)
    }

    /// REAPER's contract, which is what every caller writes to: the
    /// start a project quarter-note position, the LENGTH a raw tick
    /// delta at 960 a quarter (`add_notes_to_take_on_main_thread`
    /// inserts `start + length_ppq` ticks). Standalone keeps both in
    /// quarter notes — the file loader divides by the source's ticks,
    /// the renderer reads both through the tempo map — so the length is
    /// converted here. Stored as given, a generated click's quarter-note
    /// tick sounded for 960 quarter notes.
    fn add_notes(&self, location: MidiTakeLocation, notes: Vec<MidiNoteCreate>) -> Vec<u32> {
        notes
            .into_iter()
            .map(|n| {
                let n = MidiNoteCreate {
                    length_ppq: n.length_ppq / TICKS_PER_QN,
                    ..n
                };
                Midi::add_note(self, location.clone(), n)
            })
            .collect()
    }

    /// In the units [`Midi::notes`] hands back — quarter notes, start
    /// and length both — so stored as given: this is the round trip.
    /// Every note in one pass, the take's count set once: a note at a
    /// time looked the take up across the whole project per note.
    fn add_notes_ppq(&self, location: MidiTakeLocation, notes: Vec<MidiNoteCreate>) -> Vec<u32> {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return Vec::new();
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return Vec::new();
        };
        self.with_project_mut(&project_guid, |p| {
            let (added, count) = {
                let list = p.midi_notes.entry(take_guid.clone()).or_default();
                let added: Vec<u32> = notes
                    .into_iter()
                    .map(|note| {
                        let idx = list.len() as u32;
                        list.push(MidiNote {
                            index: idx,
                            channel: note.channel & 0x0F,
                            pitch: note.pitch & 0x7F,
                            velocity: note.velocity.clamp(1, 127),
                            start_ppq: note.start_ppq,
                            length_ppq: note.length_ppq.max(0.0),
                            selected: false,
                            muted: false,
                        });
                        idx
                    })
                    .collect();
                (added, list.len() as u32)
            };
            update_take_note_count(p, &take_guid, count);
            added
        })
        .unwrap_or_default()
    }

    fn delete_note(&self, location: MidiTakeLocation, index: u32) {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return;
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return;
        };
        let _ = self.with_project_mut(&project_guid, |p| {
            let new_count = p.midi_notes.get_mut(&take_guid).map(|notes| {
                let i = index as usize;
                if i < notes.len() {
                    notes.remove(i);
                    renumber(notes);
                }
                notes.len() as u32
            });
            if let Some(c) = new_count {
                update_take_note_count(p, &take_guid, c);
            }
        });
    }

    fn delete_notes(&self, location: MidiTakeLocation, indices: Vec<u32>) {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return;
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return;
        };
        let _ = self.with_project_mut(&project_guid, |p| {
            let new_count = p.midi_notes.get_mut(&take_guid).map(|notes| {
                let mut sorted = indices;
                sorted.sort_unstable_by(|a, b| b.cmp(a));
                for i in sorted {
                    let u = i as usize;
                    if u < notes.len() {
                        notes.remove(u);
                    }
                }
                renumber(notes);
                notes.len() as u32
            });
            if let Some(c) = new_count {
                update_take_note_count(p, &take_guid, c);
            }
        });
    }

    fn delete_selected_notes(&self, location: MidiTakeLocation) {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return;
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return;
        };
        let _ = self.with_project_mut(&project_guid, |p| {
            let new_count = p.midi_notes.get_mut(&take_guid).map(|notes| {
                notes.retain(|n| !n.selected);
                renumber(notes);
                notes.len() as u32
            });
            if let Some(c) = new_count {
                update_take_note_count(p, &take_guid, c);
            }
        });
    }

    fn set_note_pitch(&self, location: MidiTakeLocation, index: u32, pitch: u8) {
        mutate_note(self, &location, index, |n| n.pitch = pitch & 0x7F);
    }

    fn set_note_velocity(&self, location: MidiTakeLocation, index: u32, velocity: u8) {
        mutate_note(self, &location, index, |n| {
            n.velocity = velocity.clamp(1, 127)
        });
    }

    fn set_note_position(&self, location: MidiTakeLocation, index: u32, start_ppq: f64) {
        mutate_note(self, &location, index, |n| n.start_ppq = start_ppq);
    }

    fn set_note_length(&self, location: MidiTakeLocation, index: u32, length_ppq: f64) {
        mutate_note(self, &location, index, |n| {
            n.length_ppq = length_ppq.max(0.0)
        });
    }

    fn set_note_channel(&self, location: MidiTakeLocation, index: u32, channel: u8) {
        mutate_note(self, &location, index, |n| n.channel = channel & 0x0F);
    }

    fn set_note_selected(&self, location: MidiTakeLocation, index: u32, selected: bool) {
        mutate_note(self, &location, index, |n| n.selected = selected);
    }

    fn set_note_muted(&self, location: MidiTakeLocation, index: u32, muted: bool) {
        mutate_note(self, &location, index, |n| n.muted = muted);
    }

    fn select_all_notes(&self, location: MidiTakeLocation, selected: bool) {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return;
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return;
        };
        let _ = self.with_project_mut(&project_guid, |p| {
            if let Some(notes) = p.midi_notes.get_mut(&take_guid) {
                for n in notes.iter_mut() {
                    n.selected = selected;
                }
            }
        });
    }

    fn transpose_notes(&self, location: MidiTakeLocation, indices: Vec<u32>, semitones: i8) {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return;
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return;
        };
        let _ = self.with_project_mut(&project_guid, |p| {
            if let Some(notes) = p.midi_notes.get_mut(&take_guid) {
                // Per proto doc: empty `indices` means "selected notes only".
                let select_all_selected = indices.is_empty();
                for (i, n) in notes.iter_mut().enumerate() {
                    let touched = if select_all_selected {
                        n.selected
                    } else {
                        indices.contains(&(i as u32))
                    };
                    if touched {
                        let new_pitch = (n.pitch as i16 + semitones as i16).clamp(0, 127);
                        n.pitch = new_pitch as u8;
                    }
                }
            }
        });
    }

    fn quantize_notes(&self, location: MidiTakeLocation, params: QuantizeParams) {
        if params.grid_ppq <= 0.0 {
            return;
        }
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return;
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return;
        };
        let _ = self.with_project_mut(&project_guid, |p| {
            if let Some(notes) = p.midi_notes.get_mut(&take_guid) {
                let strength = params.strength.clamp(0.0, 1.0);
                let select_all_selected = params.indices.is_empty();
                for (i, n) in notes.iter_mut().enumerate() {
                    let touched = if select_all_selected {
                        n.selected
                    } else {
                        params.indices.contains(&(i as u32))
                    };
                    if !touched {
                        continue;
                    }
                    let grid = params.grid_ppq;
                    let nearest = (n.start_ppq / grid).round() * grid;
                    n.start_ppq += (nearest - n.start_ppq) * strength;
                }
            }
        });
    }

    fn humanize_notes(&self, location: MidiTakeLocation, params: HumanizeParams) {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return;
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return;
        };
        let _ = self.with_project_mut(&project_guid, |p| {
            let Some(notes) = p.midi_notes.get_mut(&take_guid) else {
                return;
            };
            // Pick targets: explicit indices, or selected, or all.
            let touched: Vec<usize> = if !params.indices.is_empty() {
                params
                    .indices
                    .iter()
                    .filter_map(|&i| {
                        if (i as usize) < notes.len() {
                            Some(i as usize)
                        } else {
                            None
                        }
                    })
                    .collect()
            } else {
                let any_selected = notes.iter().any(|n| n.selected);
                if any_selected {
                    notes
                        .iter()
                        .enumerate()
                        .filter(|(_, n)| n.selected)
                        .map(|(i, _)| i)
                        .collect()
                } else {
                    (0..notes.len()).collect()
                }
            };
            // Deterministic PRNG seeded from take guid + total note
            // count so the same humanize on the same take produces
            // the same offsets. Splitmix64: tiny, fast, well-studied.
            let mut state = splitmix_seed(&take_guid, notes.len());
            for idx in touched {
                let n = &mut notes[idx];
                if params.timing_range_ppq > 0.0 {
                    // Symmetric range [-range, +range], uniform.
                    let raw = (next_u64(&mut state) as f64) / (u64::MAX as f64);
                    let offset = (raw * 2.0 - 1.0) * params.timing_range_ppq;
                    n.start_ppq = (n.start_ppq + offset).max(0.0);
                }
                if params.velocity_range > 0 {
                    let raw = (next_u64(&mut state) as f64) / (u64::MAX as f64);
                    let offset = ((raw * 2.0 - 1.0) * params.velocity_range as f64).round() as i32;
                    n.velocity = ((n.velocity as i32) + offset).clamp(1, 127) as u8;
                }
            }
            // Resort + renumber since timing shifted.
            notes.sort_by(|a, b| {
                a.start_ppq
                    .partial_cmp(&b.start_ppq)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            renumber(notes);
        });
    }

    // ── CCs ────────────────────────────────────────────────────────

    fn ccs(&self, location: MidiTakeLocation, controller: Option<u8>) -> Vec<MidiCC> {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return Vec::new();
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return Vec::new();
        };
        self.with_project(&project_guid, |p| {
            p.midi_ccs
                .get(&take_guid)
                .map(|v| match controller {
                    Some(c) => v.iter().filter(|cc| cc.controller == c).cloned().collect(),
                    None => v.clone(),
                })
                .unwrap_or_default()
        })
        .unwrap_or_default()
    }

    fn add_cc(&self, location: MidiTakeLocation, cc: MidiCCCreate) -> u32 {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return 0;
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return 0;
        };
        self.with_project_mut(&project_guid, |p| {
            let v = p.midi_ccs.entry(take_guid).or_default();
            let new_index = v.len() as u32;
            v.push(MidiCC {
                index: new_index,
                channel: cc.channel & 0x0F,
                controller: cc.controller & 0x7F,
                value: cc.value & 0x7F,
                position_ppq: cc.position_ppq,
                selected: false,
            });
            sort_by_ppq_cc(v);
            renumber_cc(v);
            new_index
        })
        .ok()
        .unwrap_or(0)
    }

    fn delete_cc(&self, location: MidiTakeLocation, index: u32) {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return;
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return;
        };
        let _ = self.with_project_mut(&project_guid, |p| {
            if let Some(v) = p.midi_ccs.get_mut(&take_guid)
                && (index as usize) < v.len()
            {
                v.remove(index as usize);
                renumber_cc(v);
            }
        });
    }

    fn set_cc_value(&self, location: MidiTakeLocation, index: u32, value: u8) {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return;
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return;
        };
        let _ = self.with_project_mut(&project_guid, |p| {
            if let Some(v) = p.midi_ccs.get_mut(&take_guid)
                && let Some(cc) = v.get_mut(index as usize)
            {
                cc.value = value & 0x7F;
            }
        });
    }

    // ── Pitch bends ────────────────────────────────────────────────

    fn pitch_bends(&self, location: MidiTakeLocation) -> Vec<MidiPitchBend> {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return Vec::new();
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return Vec::new();
        };
        self.with_project(&project_guid, |p| {
            p.midi_pitch_bends
                .get(&take_guid)
                .cloned()
                .unwrap_or_default()
        })
        .unwrap_or_default()
    }

    fn add_pitch_bend(&self, location: MidiTakeLocation, pb: MidiPitchBendCreate) -> u32 {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return 0;
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return 0;
        };
        self.with_project_mut(&project_guid, |p| {
            let v = p.midi_pitch_bends.entry(take_guid).or_default();
            let new_index = v.len() as u32;
            v.push(MidiPitchBend {
                index: new_index,
                channel: pb.channel & 0x0F,
                value: pb.value.clamp(-8192, 8191),
                position_ppq: pb.position_ppq,
                selected: false,
            });
            sort_by_ppq_pb(v);
            renumber_pb(v);
            new_index
        })
        .ok()
        .unwrap_or(0)
    }

    fn delete_pitch_bend(&self, location: MidiTakeLocation, index: u32) {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return;
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return;
        };
        let _ = self.with_project_mut(&project_guid, |p| {
            if let Some(v) = p.midi_pitch_bends.get_mut(&take_guid)
                && (index as usize) < v.len()
            {
                v.remove(index as usize);
                renumber_pb(v);
            }
        });
    }

    fn set_pitch_bend_value(&self, location: MidiTakeLocation, index: u32, value: i16) {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return;
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return;
        };
        let _ = self.with_project_mut(&project_guid, |p| {
            if let Some(v) = p.midi_pitch_bends.get_mut(&take_guid)
                && let Some(pb) = v.get_mut(index as usize)
            {
                pb.value = value.clamp(-8192, 8191);
            }
        });
    }

    // ── Program changes ────────────────────────────────────────────

    fn program_changes(&self, location: MidiTakeLocation) -> Vec<MidiProgramChange> {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return Vec::new();
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return Vec::new();
        };
        self.with_project(&project_guid, |p| {
            p.midi_program_changes
                .get(&take_guid)
                .cloned()
                .unwrap_or_default()
        })
        .unwrap_or_default()
    }

    fn add_program_change(&self, location: MidiTakeLocation, pc: MidiProgramChangeCreate) -> u32 {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return 0;
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return 0;
        };
        self.with_project_mut(&project_guid, |p| {
            let v = p.midi_program_changes.entry(take_guid).or_default();
            let new_index = v.len() as u32;
            v.push(MidiProgramChange {
                index: new_index,
                channel: pc.channel & 0x0F,
                program: pc.program & 0x7F,
                position_ppq: pc.position_ppq,
            });
            sort_by_ppq_pc(v);
            renumber_pc(v);
            new_index
        })
        .ok()
        .unwrap_or(0)
    }

    fn delete_program_change(&self, location: MidiTakeLocation, index: u32) {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return;
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return;
        };
        let _ = self.with_project_mut(&project_guid, |p| {
            if let Some(v) = p.midi_program_changes.get_mut(&take_guid)
                && (index as usize) < v.len()
            {
                v.remove(index as usize);
                renumber_pc(v);
            }
        });
    }

    fn set_program(&self, location: MidiTakeLocation, index: u32, program: u8) {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return;
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return;
        };
        let _ = self.with_project_mut(&project_guid, |p| {
            if let Some(v) = p.midi_program_changes.get_mut(&take_guid)
                && let Some(pc) = v.get_mut(index as usize)
            {
                pc.program = program & 0x7F;
            }
        });
    }

    // ── SysEx ──────────────────────────────────────────────────────

    fn sysex(&self, location: MidiTakeLocation) -> Vec<MidiSysEx> {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return Vec::new();
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return Vec::new();
        };
        self.with_project(&project_guid, |p| {
            p.midi_sysex.get(&take_guid).cloned().unwrap_or_default()
        })
        .unwrap_or_default()
    }

    fn add_sysex(&self, location: MidiTakeLocation, sysex: MidiSysExCreate) -> u32 {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return 0;
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return 0;
        };
        self.with_project_mut(&project_guid, |p| {
            let v = p.midi_sysex.entry(take_guid).or_default();
            let new_index = v.len() as u32;
            v.push(MidiSysEx {
                index: new_index,
                position_ppq: sysex.position_ppq,
                data: sysex.data,
            });
            sort_by_ppq_sysex(v);
            renumber_sysex(v);
            new_index
        })
        .ok()
        .unwrap_or(0)
    }

    fn delete_sysex(&self, location: MidiTakeLocation, index: u32) {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return;
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return;
        };
        let _ = self.with_project_mut(&project_guid, |p| {
            if let Some(v) = p.midi_sysex.get_mut(&take_guid)
                && (index as usize) < v.len()
            {
                v.remove(index as usize);
                renumber_sysex(v);
            }
        });
    }

    // ── Channel pressure (mono aftertouch) ─────────────────────────

    fn channel_pressures(&self, location: MidiTakeLocation) -> Vec<MidiChannelPressure> {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return Vec::new();
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return Vec::new();
        };
        self.with_project(&project_guid, |p| {
            p.midi_channel_pressures
                .get(&take_guid)
                .cloned()
                .unwrap_or_default()
        })
        .unwrap_or_default()
    }

    fn add_channel_pressure(
        &self,
        location: MidiTakeLocation,
        cp: MidiChannelPressureCreate,
    ) -> u32 {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return 0;
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return 0;
        };
        self.with_project_mut(&project_guid, |p| {
            let v = p.midi_channel_pressures.entry(take_guid).or_default();
            let new_index = v.len() as u32;
            v.push(MidiChannelPressure {
                index: new_index,
                channel: cp.channel & 0x0F,
                pressure: cp.pressure & 0x7F,
                position_ppq: cp.position_ppq,
                selected: false,
            });
            sort_by_ppq_cp(v);
            renumber_cp(v);
            new_index
        })
        .ok()
        .unwrap_or(0)
    }

    fn delete_channel_pressure(&self, location: MidiTakeLocation, index: u32) {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return;
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return;
        };
        let _ = self.with_project_mut(&project_guid, |p| {
            if let Some(v) = p.midi_channel_pressures.get_mut(&take_guid)
                && (index as usize) < v.len()
            {
                v.remove(index as usize);
                renumber_cp(v);
            }
        });
    }

    fn set_channel_pressure_value(&self, location: MidiTakeLocation, index: u32, pressure: u8) {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return;
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return;
        };
        let _ = self.with_project_mut(&project_guid, |p| {
            if let Some(v) = p.midi_channel_pressures.get_mut(&take_guid)
                && let Some(cp) = v.get_mut(index as usize)
            {
                cp.pressure = pressure & 0x7F;
            }
        });
    }

    // ── Poly pressure (per-note aftertouch) ────────────────────────

    fn poly_pressures(&self, location: MidiTakeLocation) -> Vec<MidiPolyPressure> {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return Vec::new();
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return Vec::new();
        };
        self.with_project(&project_guid, |p| {
            p.midi_poly_pressures
                .get(&take_guid)
                .cloned()
                .unwrap_or_default()
        })
        .unwrap_or_default()
    }

    fn add_poly_pressure(&self, location: MidiTakeLocation, pp: MidiPolyPressureCreate) -> u32 {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return 0;
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return 0;
        };
        self.with_project_mut(&project_guid, |p| {
            let v = p.midi_poly_pressures.entry(take_guid).or_default();
            let new_index = v.len() as u32;
            v.push(MidiPolyPressure {
                index: new_index,
                channel: pp.channel & 0x0F,
                note: pp.note & 0x7F,
                pressure: pp.pressure & 0x7F,
                position_ppq: pp.position_ppq,
                selected: false,
            });
            sort_by_ppq_pp(v);
            renumber_pp(v);
            new_index
        })
        .ok()
        .unwrap_or(0)
    }

    fn delete_poly_pressure(&self, location: MidiTakeLocation, index: u32) {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return;
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return;
        };
        let _ = self.with_project_mut(&project_guid, |p| {
            if let Some(v) = p.midi_poly_pressures.get_mut(&take_guid)
                && (index as usize) < v.len()
            {
                v.remove(index as usize);
                renumber_pp(v);
            }
        });
    }

    fn set_poly_pressure_value(&self, location: MidiTakeLocation, index: u32, pressure: u8) {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return;
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return;
        };
        let _ = self.with_project_mut(&project_guid, |p| {
            if let Some(v) = p.midi_poly_pressures.get_mut(&take_guid)
                && let Some(pp) = v.get_mut(index as usize)
            {
                pp.pressure = pressure & 0x7F;
            }
        });
    }

    // ── Note expression (MPE / CLAP / VST3) ────────────────────────

    fn note_expressions(&self, location: MidiTakeLocation) -> Vec<MidiNoteExpression> {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return Vec::new();
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return Vec::new();
        };
        self.with_project(&project_guid, |p| {
            p.midi_note_expressions
                .get(&take_guid)
                .cloned()
                .unwrap_or_default()
        })
        .unwrap_or_default()
    }

    fn add_note_expression(&self, location: MidiTakeLocation, ne: MidiNoteExpressionCreate) -> u32 {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return 0;
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return 0;
        };
        self.with_project_mut(&project_guid, |p| {
            let v = p.midi_note_expressions.entry(take_guid).or_default();
            let new_index = v.len() as u32;
            v.push(MidiNoteExpression {
                index: new_index,
                channel: ne.channel & 0x0F,
                note: ne.note,
                dimension: ne.dimension,
                value: ne.value,
                position_ppq: ne.position_ppq,
                selected: false,
            });
            sort_by_ppq_ne(v);
            renumber_ne(v);
            new_index
        })
        .ok()
        .unwrap_or(0)
    }

    fn delete_note_expression(&self, location: MidiTakeLocation, index: u32) {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return;
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return;
        };
        let _ = self.with_project_mut(&project_guid, |p| {
            if let Some(v) = p.midi_note_expressions.get_mut(&take_guid)
                && (index as usize) < v.len()
            {
                v.remove(index as usize);
                renumber_ne(v);
            }
        });
    }

    fn set_note_expression_value(&self, location: MidiTakeLocation, index: u32, value: f64) {
        let Some(take_guid) = resolve_take_guid(self, &location) else {
            return;
        };
        let Some(project_guid) = resolve_project(self, &location.project) else {
            return;
        };
        let _ = self.with_project_mut(&project_guid, |p| {
            if let Some(v) = p.midi_note_expressions.get_mut(&take_guid)
                && let Some(ne) = v.get_mut(index as usize)
            {
                ne.value = value;
            }
        });
    }
}

/// Tiny SplitMix64 PRNG. Seeded from the take guid + a counter so
/// repeated humanize calls on the same take with the same params
/// produce identical offsets — important for test reproducibility.
fn splitmix_seed(take_guid: &str, salt: usize) -> u64 {
    // FNV-1a hash for the seed.
    let mut h: u64 = 0xcbf29ce484222325;
    for b in take_guid.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h ^ (salt as u64).rotate_left(17)
}

fn next_u64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E3779B97F4A7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

fn sort_by_ppq_ne(v: &mut [MidiNoteExpression]) {
    v.sort_by(|a, b| {
        a.position_ppq
            .partial_cmp(&b.position_ppq)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}
fn renumber_ne(v: &mut [MidiNoteExpression]) {
    for (i, e) in v.iter_mut().enumerate() {
        e.index = i as u32;
    }
}

fn sort_by_ppq_cp(v: &mut [MidiChannelPressure]) {
    v.sort_by(|a, b| {
        a.position_ppq
            .partial_cmp(&b.position_ppq)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}
fn renumber_cp(v: &mut [MidiChannelPressure]) {
    for (i, e) in v.iter_mut().enumerate() {
        e.index = i as u32;
    }
}
fn sort_by_ppq_pp(v: &mut [MidiPolyPressure]) {
    v.sort_by(|a, b| {
        a.position_ppq
            .partial_cmp(&b.position_ppq)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}
fn renumber_pp(v: &mut [MidiPolyPressure]) {
    for (i, e) in v.iter_mut().enumerate() {
        e.index = i as u32;
    }
}

// ── Sort + renumber helpers (one per event vec type) ──────────────

fn sort_by_ppq_cc(v: &mut [MidiCC]) {
    v.sort_by(|a, b| {
        a.position_ppq
            .partial_cmp(&b.position_ppq)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}
fn renumber_cc(v: &mut [MidiCC]) {
    for (i, e) in v.iter_mut().enumerate() {
        e.index = i as u32;
    }
}
fn sort_by_ppq_pb(v: &mut [MidiPitchBend]) {
    v.sort_by(|a, b| {
        a.position_ppq
            .partial_cmp(&b.position_ppq)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}
fn renumber_pb(v: &mut [MidiPitchBend]) {
    for (i, e) in v.iter_mut().enumerate() {
        e.index = i as u32;
    }
}
fn sort_by_ppq_pc(v: &mut [MidiProgramChange]) {
    v.sort_by(|a, b| {
        a.position_ppq
            .partial_cmp(&b.position_ppq)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}
fn renumber_pc(v: &mut [MidiProgramChange]) {
    for (i, e) in v.iter_mut().enumerate() {
        e.index = i as u32;
    }
}
fn sort_by_ppq_sysex(v: &mut [MidiSysEx]) {
    v.sort_by(|a, b| {
        a.position_ppq
            .partial_cmp(&b.position_ppq)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}
fn renumber_sysex(v: &mut [MidiSysEx]) {
    for (i, e) in v.iter_mut().enumerate() {
        e.index = i as u32;
    }
}

// ── Helpers ────────────────────────────────────────────────────────────

fn mutate_note(
    daw: &Standalone,
    location: &MidiTakeLocation,
    index: u32,
    f: impl FnOnce(&mut MidiNote),
) {
    let Some(take_guid) = resolve_take_guid(daw, location) else {
        return;
    };
    let Some(project_guid) = resolve_project(daw, &location.project) else {
        return;
    };
    let _ = daw.with_project_mut(&project_guid, |p| {
        if let Some(notes) = p.midi_notes.get_mut(&take_guid)
            && let Some(n) = notes.get_mut(index as usize)
        {
            f(n);
        }
    });
}

fn update_take_note_count(p: &mut crate::sync::ProjectState, take_guid: &str, count: u32) {
    if let Some(list) = p
        .takes
        .values_mut()
        .find(|tl| tl.takes.iter().any(|t| t.guid == take_guid))
        && let Some(t) = list.takes.iter_mut().find(|t| t.guid == take_guid)
    {
        t.midi_note_count = Some(count);
    }
}
