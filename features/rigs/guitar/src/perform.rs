//! The guitar rig's side of the shared performance surface
//! ([`RigPerform`]): the same calls the guitar's own `Rig` answers, so a
//! screen built on the shared surface plays every rig. Mounted under the
//! rig's scope (`merge_router_scoped("guitar", …)`).

use architect::PubSub;
use signal_guitar_proto::rig::Rig;
use signal_guitar_proto::{Applied, CompositionModel, LibraryModel, PatchInfo, PerformanceModel};
use signal_rigs_proto::perform::PerformState;
use signal_rigs_proto::perform::service::{PerformEvent, RigPerform, RigPerformStreamSource};

use super::GuitarRigBackend;

/// The shared state, from the guitar's model and the patch playing.
pub(super) fn perform_state(m: &PerformanceModel, active_patch: Option<String>) -> PerformState {
    PerformState {
        mode: m.perform_mode,
        profile_name: m.profile_name.clone(),
        stacks: m.stacks.clone(),
        active_patch: active_patch.unwrap_or_default(),
        songs: m.songs.clone(),
        song_index: m.song_index,
        setlists: m.setlists.clone(),
        setlist_index: m.setlist_index,
        library_songs: m.library_songs.clone(),
        parts: m.parts.clone(),
        part_index: m.part_index,
        song_profile: m.song_profile.clone(),
        start_part: m.start_part.clone(),
        start_patch: m.start_patch.clone(),
        song_changes: m.song_changes.clone(),
        tempo_bpm: m.tempo_bpm,
        undo_depth: m.undo_depth,
        revision: m.revision,
    }
}

impl RigPerform for GuitarRigBackend {
    fn state(&self) -> PerformState {
        perform_state(&Rig::perf(self), self.active_patch_name())
    }
    fn set_perform_mode(&self, mode: u32) {
        Rig::set_perform_mode(self, mode);
    }
    fn patches(&self) -> Vec<PatchInfo> {
        Rig::patches(self)
    }
    fn select_patch(&self, index: u32) -> Applied {
        Rig::select_patch(self, index)
    }
    fn compositions(&self) -> CompositionModel {
        Rig::compositions(self)
    }
    fn choose_preset(&self, preset: String, variation: String) {
        Rig::choose_preset(self, preset, variation);
    }
    fn save_variation(&self, preset: String, variation: String) {
        Rig::save_core_snapshot(self, preset, variation);
    }
    fn rename_preset(&self, old: String, new_name: String) {
        Rig::rename_rig_preset(self, old, new_name);
    }
    fn duplicate_preset(&self, name: String, new_name: String) {
        Rig::duplicate_rig_preset(self, name, new_name);
    }
    fn delete_preset(&self, name: String) {
        Rig::delete_rig_preset(self, name);
    }
    fn library(&self) -> LibraryModel {
        Rig::library(self)
    }
    fn select_profile(&self, name: String) -> Applied {
        Rig::select_profile(self, name)
    }
    fn add_profile(&self, name: String, from: String) {
        Rig::add_profile(self, name, from);
    }
    fn rename_profile(&self, old: String, new_name: String) {
        Rig::rename_profile(self, old, new_name);
    }
    fn delete_profile(&self, name: String) {
        Rig::delete_profile(self, name);
    }
    fn press_stack(&self, index: u32) {
        Rig::press_stack(self, index);
    }
    fn play_stack_patch(&self, index: u32, position: u32) -> Applied {
        Rig::play_stack_patch(self, index, position)
    }
    fn set_stack_patch(&self, stack: String, patch: String, on: bool) {
        Rig::set_stack_patch(self, stack, patch, on);
    }
    fn move_stack_patch(&self, stack: String, from: u32, to: u32) {
        Rig::move_stack_patch(self, stack, from, to);
    }
    fn move_stack(&self, from: u32, to: u32) {
        Rig::move_stack(self, from, to);
    }
    fn rename_stack(&self, old: String, new_name: String) {
        Rig::rename_stack(self, old, new_name);
    }
    fn rename_patch(&self, old: String, new_name: String) {
        Rig::rename_patch(self, old, new_name);
    }
    fn select_song(&self, index: u32) -> Applied {
        Rig::select_song(self, index)
    }
    fn select_part(&self, index: u32) -> Applied {
        Rig::select_part(self, index)
    }
    fn add_part(&self, name: String) {
        Rig::add_part(self, name);
    }
    fn rename_part(&self, old: String, new_name: String) {
        Rig::rename_part(self, old, new_name);
    }
    fn remove_part(&self, name: String) {
        Rig::remove_part(self, name);
    }
    fn move_part(&self, from: u32, to: u32) {
        Rig::move_part(self, from, to);
    }
    fn set_part_patch(&self, part: String, patch: String) {
        Rig::set_part_patch(self, part, patch);
    }
    fn set_part_section(&self, part: String, section: String) {
        Rig::set_part_section(self, part, section);
    }
    fn set_part_profile(&self, part: String, profile: String) {
        Rig::set_part_profile(self, part, profile);
    }
    fn edit_song_part(&self, song: String, op: String, part: String, value: String) {
        Rig::edit_song_part(self, song, op, part, value);
    }
    fn add_song(&self, name: String, key: String, bpm: u32) {
        Rig::add_song(self, name, key, bpm);
    }
    fn set_song_profile(&self, song: String, profile: String) {
        Rig::set_song_profile(self, song, profile);
    }
    fn set_song_colour(&self, song: String, colour: String) {
        Rig::set_song_colour(self, song, colour);
    }
    fn select_setlist(&self, index: u32) -> Applied {
        Rig::select_setlist(self, index)
    }
    fn add_setlist(&self, name: String) {
        Rig::add_setlist(self, name);
    }
    fn rename_setlist(&self, index: u32, new_name: String) {
        Rig::rename_setlist(self, index, new_name);
    }
    fn duplicate_setlist(&self, index: u32, new_name: String) {
        Rig::duplicate_setlist(self, index, new_name);
    }
    fn set_setlist_details(&self, index: u32, event: String, date: String, title: String) {
        Rig::set_setlist_details(self, index, event, date, title);
    }
    fn set_setlist_profile(&self, index: u32, profile: String) {
        Rig::set_setlist_profile(self, index, profile);
    }
    fn add_setlist_entry(&self, setlist: u32, song: String) {
        Rig::add_setlist_entry(self, setlist, song);
    }
    fn set_setlist_entry(&self, entry: u32, key: String, bpm: u32) {
        Rig::set_setlist_entry(self, entry, key, bpm);
    }
    fn set_setlist_entry_start(&self, entry: u32, patch: String) {
        Rig::set_setlist_entry_start(self, entry, patch);
    }
    fn move_setlist_entry(&self, setlist: u32, from: u32, to: u32) {
        Rig::move_setlist_entry(self, setlist, from, to);
    }
    fn remove_setlist_entry(&self, setlist: u32, entry: u32) {
        Rig::remove_setlist_entry(self, setlist, entry);
    }
    fn delete_setlist(&self, index: u32) {
        Rig::delete_setlist(self, index);
    }
}

impl RigPerformStreamSource for GuitarRigBackend {
    fn events_hub(&self) -> &PubSub<PerformEvent> {
        &self.perform
    }
}
