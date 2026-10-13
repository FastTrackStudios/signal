//! Where every part of a daw-standalone project lives in the doc.
//!
//! Each engine type below is taken apart field by field, with no `..`:
//! a field added to any of them stops this compiling until someone
//! writes down where it lives — a doc container, or how it is derived
//! again from what the doc holds, or why it is one person's (presence)
//! rather than the project's.

#![cfg(feature = "engine")]

use daw_proto::automation::{AutomationItem, EnvelopePoint};
use daw_proto::fx::Fx;
use daw_proto::transport::transport::Transport;
use daw_proto::{Item, Take, Track, TrackRoute};
use daw_standalone::sync::{
    EnvelopeData, FxEntry, ItemEntry, ProjectState, RulerLane, TakeList, TrackExt,
};

/// `Type { field: "where it lives", … }`, every field named: the pattern
/// a closure that is never called matches is exhaustive, so the table
/// cannot fall behind the type.
macro_rules! homes {
    ($ty:ident { $($field:ident: $home:literal),* $(,)? }) => {{
        let _exhaustive = |v: &$ty| {
            let $ty { $($field: _),* } = v;
        };
        vec![$((stringify!($ty), stringify!($field), $home)),*]
    }};
}

fn table() -> Vec<(&'static str, &'static str, &'static str)> {
    let mut all = homes!(ProjectState {
        info: "re-derived: the engine project the bridge is bound to (guid and path are each machine's own)",
        transport: "see Transport",
        regions: "regions (by guid; ids are engine bookkeeping)",
        markers: "markers (by guid; ids are engine bookkeeping)",
        ruler_lanes: "project.ruler_lanes",
        tempo_points: "meta.tempo",
        tracks: "tracks (the tree); see Track",
        track_ext: "see TrackExt",
        midi_notes: "items: take.notes, take.more.notes_muted; takes[].notes",
        midi_ccs: "take.more.midi.cc; takes[].midi",
        midi_pitch_bends: "take.more.midi.pb; takes[].midi",
        midi_program_changes: "take.more.midi.pc; takes[].midi",
        midi_sysex: "take.more.midi.sysex; takes[].midi",
        stretch_markers: "take.stretch; takes[].stretch",
        stretch_modes: "take.more.stretch_mode; takes[].stretch_mode (absent = project default)",
        take_markers: "take_markers (by recording; a fileless take's as take:<item>:<place>)",
        midi_channel_pressures: "take.more.midi.chp; takes[].midi",
        midi_poly_pressures: "take.more.midi.pp; takes[].midi",
        midi_note_expressions: "take.more.midi.nx; takes[].midi",
        envelopes: "envelopes (track, master, FX param, send, named); a take's: take.volume, take.more.envelopes; see EnvelopeData",
        global_automation_override: "project.automation_override",
        bay_state: "project.bay (the engine's own serialization)",
        audio_sources: "re-derived: decoded from each take's source file when the engine next plays or reads it",
        next_region_id: "bookkeeping: engine-minted ids",
        next_marker_id: "bookkeeping: engine-minted ids",
        track_ext_state: "tracks: ext (the unnamed section), ext_sections",
        project_ext_state: "project.ext",
        fx_chains: "fx (chain: track, input, monitoring; the master's under track master); see FxEntry",
        fx_preset_index: "fx.preset",
        fx_named_config: "fx.config",
        items: "items; see ItemEntry",
        items_by_track: "re-derived: each item's track (order: the order items are added)",
        takes: "see TakeList",
        sends: "sends; see TrackRoute",
        receives: "re-derived: mirrored from the sends, in track order",
        hw_outputs: "hw_outputs (a track's; the master's under track master); see TrackRoute",
        next_item_counter: "bookkeeping: engine-minted ids",
        next_fx_counter: "bookkeeping: engine-minted ids",
        next_take_counter: "bookkeeping: engine-minted ids",
        revision: "bookkeeping: bumped by every write",
        master_volume: "project.master.volume",
        master_pan: "project.master.pan",
        master_muted: "project.master.muted",
    });
    all.extend(homes!(Transport {
        play_state: "presence: the shared transport (crate::transport), not the project",
        record_mode: "project.record_mode",
        looping: "project.looping",
        metronome: "project.metronome",
        loop_region: "presence: one person's time selection (the engine pairs it with the loop)",
        time_selection: "presence: one person's time selection",
        tempo: "re-derived: the tempo map (meta.tempo)",
        playrate: "project.playrate",
        time_signature: "re-derived: the tempo map (meta.tempo)",
        playhead_position: "presence: the shared transport",
        edit_position: "presence: one person's edit cursor",
    }));
    all.extend(homes!(Track {
        guid: "tracks node: guid",
        index: "re-derived: the tree's order",
        name: "name",
        color: "color",
        muted: "muted",
        soloed: "soloed",
        armed: "armed",
        selected: "presence: one person's selection",
        volume: "volume",
        pan: "pan",
        phase_inverted: "phase_inverted",
        automation_mode: "automation_mode",
        input_monitor: "monitor",
        record_input: "record_input",
        parent_send: "parent_send",
        parent_guid: "the tree: the node's parent",
        folder_depth: "re-derived: the tree",
        is_folder: "re-derived: the tree",
        lane_count: "lanes.count",
        lane_play_mask: "lanes.play",
        lane_names: "lanes.names",
        lane_display: "lane_display",
        grouping: "grouping (every family, lead, follow, reverse)",
        visible_in_tcp: "visible_in_tcp",
        visible_in_mixer: "visible_in_mixer",
        send_count: "re-derived: the sends",
        receive_count: "re-derived: the sends",
        fx_count: "re-derived: the FX",
        input_fx_count: "re-derived: the FX",
        height: "height",
        width: "width",
    }));
    all.extend(homes!(TrackExt {
        num_channels: "channels",
        record_input: "record_input (the same as the track's)",
        parent_send_enabled: "parent_send (the same as the track's)",
        tcp_height_pixels: "height (0: none)",
        comping: "comp.record, comp.lane, comp.last, comp.areas",
    }));
    all.extend(homes!(ItemEntry { item: "see Item" }));
    all.extend(homes!(Item {
        guid: "items key (a doc id, mapped to the engine's)",
        track_guid: "track",
        index: "re-derived: the order on its track",
        position: "position",
        length: "length",
        snap_offset: "snap_offset",
        muted: "muted",
        selected: "presence: one person's selection",
        locked: "locked",
        volume: "volume",
        fade_in_length: "fade_in",
        fade_out_length: "fade_out",
        fade_in_shape: "fade_in_shape",
        fade_out_shape: "fade_out_shape",
        beat_attach_mode: "beat_attach",
        loop_source: "loop",
        auto_stretch: "auto_stretch",
        color: "color",
        group_id: "group",
        label: "label",
        fixed_lane: "fixed_lane",
        start_offset: "start_offset",
        take_count: "re-derived: the takes",
        active_take_index: "active_take",
    }));
    all.extend(homes!(TakeList {
        active_idx: "active_take",
        takes: "take.* (the active one), takes (the others, in order); see Take",
    }));
    all.extend(homes!(Take {
        guid: "bookkeeping: engine-minted; the doc keeps a take by its item and place",
        item_guid: "re-derived: its item",
        index: "re-derived: its place",
        is_active: "re-derived: active_take",
        name: "name",
        color: "color",
        volume: "gain",
        play_rate: "playrate",
        pitch: "pitch",
        preserve_pitch: "preserve_pitch",
        channel_mode: "channel_mode",
        start_offset: "start_offset",
        source_type: "source_type",
        source_file_path: "source (relative to the session folder)",
        source_length: "source_length",
        source_sample_rate: "source_rate",
        source_channels: "source_channels",
        is_midi: "midi",
        midi_note_count: "note_count",
    }));
    all.extend(homes!(TrackRoute {
        index: "re-derived: its place",
        route_type: "which container: sends or hw_outputs (receives are re-derived)",
        source_track_guid: "track",
        dest_track_guid: "dest",
        dest_track_name: "label",
        hw_output_index: "dest (a hardware output's)",
        hw_output_name: "label (a hardware output's)",
        volume: "volume",
        pan: "pan",
        muted: "muted",
        mono: "mono",
        phase_inverted: "phase_inverted",
        send_mode: "mode",
        automation_mode: "automation_mode",
        source_channels: "from, from_count",
        dest_channels: "to, to_count",
        midi_channel_mapping: "midi",
    }));
    all.extend(homes!(FxEntry {
        fx: "see Fx",
        state_chunk: "state (a blob's name in the project's store)",
        params: "params",
        ranges: "re-derived: the plugin instance, made again from plugin and state",
    }));
    all.extend(homes!(Fx {
        guid: "fx key (a doc id, mapped to the engine's)",
        index: "index",
        name: "name",
        plugin_name: "plugin",
        plugin_type: "re-derived: the plugin it is added by",
        enabled: "enabled",
        offline: "offline",
        window_open: "presence: one person's open window",
        parameter_count: "param_count",
        preset_name: "preset_name",
    }));
    all.extend(homes!(EnvelopeData {
        visible: "visible",
        armed: "armed",
        automation_mode: "mode",
        in_own_lane: "own_lane",
        lane_height: "lane_height",
        points: "points; see EnvelopePoint",
        automation_items: "items; see AutomationItem",
    }));
    all.extend(homes!(EnvelopePoint {
        index: "re-derived: its place",
        time: "points[at]",
        value: "points[value]",
        shape: "points[shape]",
        tension: "points[tension]",
        selected: "presence: one person's selection",
    }));
    all.extend(homes!(AutomationItem {
        index: "re-derived: its place",
        pool_id: "pool",
        name: "name",
        position: "position",
        length: "length",
        start_offset: "start_offset",
        play_rate: "playrate",
        baseline: "baseline",
        amplitude: "amplitude",
        loop_source: "looped",
        selected: "presence: one person's selection",
    }));
    all.extend(homes!(RulerLane {
        name: "project.ruler_lanes[name]",
        flags: "project.ruler_lanes[flags]",
    }));
    all
}

#[test]
fn every_field_of_a_project_has_a_home_in_the_doc() {
    let table = table();
    for (ty, field, home) in &table {
        assert!(!home.is_empty(), "{ty}.{field} has no home");
    }
    // Nothing named twice.
    let mut names: Vec<(&str, &str)> = table.iter().map(|(t, f, _)| (*t, *f)).collect();
    names.sort_unstable();
    let before = names.len();
    names.dedup();
    assert_eq!(names.len(), before);
}
