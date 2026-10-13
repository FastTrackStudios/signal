//! Zero-overhead sync API for REAPER main-thread access.
//!
//! [`DawMainThread`] provides direct, synchronous access to REAPER APIs
//! without going through vox RPC or `main_thread::query`. This is intended
//! for use from timer callbacks and other code that is **already running on
//! REAPER's main thread**.
//!
//! The struct is `!Send + !Sync` to prevent accidental use from worker threads.

use std::ffi::CString;
use std::marker::PhantomData;

use reaper_high::Reaper;
use reaper_medium::{ProjectContext as ReaperProjectContext, TrackFxLocation};

use crate::project_context::project_guid;
use crate::safe_wrappers::ext_state as sw;

/// Lightweight info about a media item on a track.
#[derive(Clone, Debug)]
pub struct ItemInfo {
    pub position: f64,
    pub length: f64,
    pub take_name: String,
}

/// Synchronous, zero-overhead access to REAPER APIs.
///
/// Must only be used from the REAPER main thread (timer callbacks, etc.).
/// The `PhantomData<*const ()>` marker makes this `!Send + !Sync`.
pub struct DawMainThread {
    _not_send_sync: PhantomData<*const ()>,
}

impl DawMainThread {
    /// Create a new `DawMainThread` handle.
    ///
    /// Returns `None` if the REAPER high-level API is not yet initialized
    /// (i.e., we cannot safely call `Reaper::get()`).
    ///
    /// # Safety contract
    ///
    /// The caller **must** ensure this is called from REAPER's main thread.
    /// The `!Send + !Sync` bound prevents moving it to another thread, but
    /// the initial construction site must be correct.
    pub fn try_new() -> Option<Self> {
        // Probe that the Reaper singleton is available. If the extension
        // hasn't finished bootstrapping, `Reaper::get()` will panic, so we
        // catch that.
        std::panic::catch_unwind(|| {
            let _ = Reaper::get();
        })
        .ok()?;

        Some(Self {
            _not_send_sync: PhantomData,
        })
    }

    // =========================================================================
    // Project
    // =========================================================================

    /// GUID of the current (foreground) project tab.
    pub fn current_project_guid(&self) -> String {
        let reaper = Reaper::get();
        let project = reaper.current_project();
        project_guid(&project)
    }

    // =========================================================================
    // Tracks
    // =========================================================================

    /// All tracks in the current project.
    pub fn track_list(&self) -> Vec<daw_proto::Track> {
        let reaper = Reaper::get();
        let project = reaper.current_project();
        let mut tracks: Vec<daw_proto::Track> =
            project.tracks().map(|t| build_track_info(&t)).collect();
        assign_parent_guids(&mut tracks);
        tracks
    }

    /// Read a per-track ext state value (`P_EXT:section:key`).
    pub fn track_get_ext_state(
        &self,
        track_guid: &str,
        section: &str,
        key: &str,
    ) -> Option<String> {
        let reaper = Reaper::get();
        let project = reaper.current_project();
        let track = find_track_by_guid(&project, track_guid)?;
        let raw = track.raw().ok()?;
        let low = reaper.medium_reaper().low();
        let attr = CString::new(format!("P_EXT:{section}:{key}")).ok()?;
        let mut buf = vec![0u8; 65536];
        let ok = unsafe {
            low.GetSetMediaTrackInfo_String(
                raw.as_ptr(),
                attr.as_ptr(),
                buf.as_mut_ptr() as *mut i8,
                false,
            )
        };
        if !ok {
            return None;
        }
        let val = crate::safe_wrappers::buffer::string_from_buffer(&buf);
        if val.is_empty() { None } else { Some(val) }
    }

    /// Mute or unmute a track by GUID.
    pub fn track_set_muted(&self, track_guid: &str, muted: bool) {
        let reaper = Reaper::get();
        let project = reaper.current_project();
        let Some(track) = find_track_by_guid(&project, track_guid) else {
            return;
        };
        use reaper_high::GroupingBehavior;
        use reaper_medium::GangBehavior;
        if muted {
            track.mute(GangBehavior::DenyGang, GroupingBehavior::PreventGrouping);
        } else {
            track.unmute(GangBehavior::DenyGang, GroupingBehavior::PreventGrouping);
        }
    }

    // =========================================================================
    // FX
    // =========================================================================

    /// List all FX in the normal (output) FX chain of a track.
    pub fn fx_list(&self, track_guid: &str) -> Vec<daw_proto::Fx> {
        let reaper = Reaper::get();
        let project = reaper.current_project();
        let Some(track) = find_track_by_guid(&project, track_guid) else {
            return vec![];
        };
        let chain = track.normal_fx_chain();
        let chain_ref = &chain;
        chain
            .fxs()
            .map(|fx| build_fx_info(&fx, Some(chain_ref)))
            .collect()
    }

    /// Read a single FX parameter value (normalized 0..1).
    pub fn fx_param_get(&self, track_guid: &str, fx_index: u32, param_index: u32) -> Option<f64> {
        let reaper = Reaper::get();
        let project = reaper.current_project();
        let track = find_track_by_guid(&project, track_guid)?;
        let chain = track.normal_fx_chain();
        if fx_index >= chain.fx_count() {
            return None;
        }
        let fx = chain.fx_by_index_untracked(fx_index);
        if param_index >= fx.parameter_count() {
            return None;
        }
        let param = fx.parameter_by_index(param_index);
        Some(param.reaper_normalized_value().get())
    }

    /// Write a single FX parameter value (normalized 0..1).
    pub fn fx_param_set(&self, track_guid: &str, fx_index: u32, param_index: u32, value: f64) {
        let reaper = Reaper::get();
        let project = reaper.current_project();
        let Some(track) = find_track_by_guid(&project, track_guid) else {
            return;
        };
        let chain = track.normal_fx_chain();
        if fx_index >= chain.fx_count() {
            return;
        }
        let Some(fx) = chain.fx_by_index(fx_index) else {
            return;
        };
        let param = fx.parameter_by_index(param_index);
        let norm_val = reaper_medium::ReaperNormalizedFxParamValue::new(value);
        let _ = param.set_reaper_normalized_value(norm_val);
    }

    /// Get the display name of an FX by index.
    pub fn fx_name(&self, track_guid: &str, fx_index: u32) -> Option<String> {
        let reaper = Reaper::get();
        let project = reaper.current_project();
        let track = find_track_by_guid(&project, track_guid)?;
        let chain = track.normal_fx_chain();
        if fx_index >= chain.fx_count() {
            return None;
        }
        let fx = chain.fx_by_index_untracked(fx_index);
        let name = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            fx.name().to_str().to_string()
        }))
        .ok()?;
        Some(name)
    }

    // =========================================================================
    // Transport
    // =========================================================================

    /// Current transport state for the current project.
    pub fn transport_state(&self) -> Option<daw_proto::Transport> {
        let reaper = Reaper::get();
        let medium = reaper.medium_reaper();
        let project = reaper.current_project();

        let play_state = {
            let state = medium.get_play_state_ex(ReaperProjectContext::CurrentProject);
            if state.is_recording {
                daw_proto::PlayState::Recording
            } else if state.is_playing {
                daw_proto::PlayState::Playing
            } else if state.is_paused {
                daw_proto::PlayState::Paused
            } else {
                daw_proto::PlayState::Stopped
            }
        };

        let looping = medium.get_set_repeat_ex_get(ReaperProjectContext::CurrentProject);
        let tempo_bpm = project.tempo().bpm().get();
        let playrate = project.play_rate().playback_speed_factor().get();

        let pos_seconds = medium
            .get_play_position_ex(ReaperProjectContext::CurrentProject)
            .get();
        let edit_pos = medium
            .get_cursor_position_ex(ReaperProjectContext::CurrentProject)
            .map(|p| p.get())
            .unwrap_or(0.0);

        let ts_result = medium.time_map_2_time_to_beats(
            ReaperProjectContext::CurrentProject,
            medium.get_play_position_ex(ReaperProjectContext::CurrentProject),
        );
        let ts_num = ts_result.time_signature.numerator.get();
        let ts_denom = ts_result.time_signature.denominator.get();

        Some(daw_proto::Transport {
            play_state,
            record_mode: daw_proto::RecordMode::Normal,
            looping,
            metronome: false,
            loop_region: None,
            time_selection: None,
            tempo: daw_proto::primitives::Tempo::from_bpm(tempo_bpm),
            playrate,
            time_signature: daw_proto::TimeSignature::new(ts_num, ts_denom),
            playhead_position: daw_proto::primitives::Position::new(
                None,
                Some(daw_proto::primitives::TimePosition::from_seconds(
                    pos_seconds,
                )),
                None,
            ),
            edit_position: daw_proto::primitives::Position::new(
                None,
                Some(daw_proto::primitives::TimePosition::from_seconds(edit_pos)),
                None,
            ),
        })
    }

    // =========================================================================
    // Last Touched FX
    // =========================================================================

    /// Info about the last-touched FX parameter, if any.
    pub fn last_touched_fx(&self) -> Option<daw_proto::LastTouchedFx> {
        let reaper = Reaper::get();
        let result = reaper.medium_reaper().get_last_touched_fx()?;

        use reaper_medium::GetLastTouchedFxResult::*;
        match result {
            TrackFx {
                track_location,
                fx_location,
                param_index,
            } => {
                let project = reaper.current_project();
                let track = match track_location {
                    reaper_medium::TrackLocation::MasterTrack => project.master_track().ok()?,
                    reaper_medium::TrackLocation::NormalTrack(idx) => {
                        project.track_by_index(idx)?
                    }
                };
                let track_guid = track.guid().to_string_without_braces();
                let (fx_index, is_input_fx) = match fx_location {
                    TrackFxLocation::NormalFxChain(idx) => (idx, false),
                    TrackFxLocation::InputFxChain(idx) => (idx, true),
                    _ => return None,
                };
                Some(daw_proto::LastTouchedFx {
                    track_guid,
                    fx_index,
                    param_index,
                    is_input_fx,
                })
            }
            _ => None,
        }
    }

    // =========================================================================
    // Global ExtState
    // =========================================================================

    /// Read a global ExtState value.
    pub fn ext_state_get(&self, section: &str, key: &str) -> Option<String> {
        let section_c = CString::new(section).ok()?;
        let key_c = CString::new(key).ok()?;
        let low = Reaper::get().medium_reaper().low();
        sw::get_ext_state(low, &section_c, &key_c)
    }

    /// Write a global ExtState value.
    pub fn ext_state_set(&self, section: &str, key: &str, value: &str, persist: bool) {
        let Ok(section_c) = CString::new(section) else {
            return;
        };
        let Ok(key_c) = CString::new(key) else {
            return;
        };
        let Ok(value_c) = CString::new(value) else {
            return;
        };
        let low = Reaper::get().medium_reaper().low();
        sw::set_ext_state(low, &section_c, &key_c, &value_c, persist);
    }

    // =========================================================================
    // Console
    // =========================================================================

    /// Print a message to the REAPER console.
    pub fn show_console_msg(&self, msg: &str) {
        let reaper = Reaper::get();
        reaper.show_console_msg(msg);
    }

    // =========================================================================
    // Items
    // =========================================================================

    /// Number of media items on a track.
    pub fn item_count(&self, track_guid: &str) -> u32 {
        let reaper = Reaper::get();
        let project = reaper.current_project();
        let Some(track) = find_track_by_guid(&project, track_guid) else {
            return 0;
        };
        track.item_count()
    }

    /// Media items on a track with position, length, and active take name.
    pub fn items(&self, track_guid: &str) -> Vec<ItemInfo> {
        use crate::safe_wrappers::item as item_sw;
        use reaper_medium::ItemAttributeKey;

        let reaper = Reaper::get();
        let medium = reaper.medium_reaper();
        let low = medium.low();
        let project = reaper.current_project();
        let Some(track) = find_track_by_guid(&project, track_guid) else {
            return vec![];
        };
        let Some(raw) = track.raw().ok() else {
            return vec![];
        };

        let count = item_sw::count_track_media_items(medium, raw);
        let mut items = Vec::with_capacity(count as usize);

        for i in 0..count {
            let Some(item) = item_sw::get_track_media_item(medium, raw, i) else {
                continue;
            };

            let position = item_sw::get_item_info_value(medium, item, ItemAttributeKey::Position);
            let length = item_sw::get_item_info_value(medium, item, ItemAttributeKey::Length);

            let take_name = item_sw::get_active_take(medium, item)
                .map(|take| item_sw::get_take_name(low, take))
                .unwrap_or_default();

            items.push(ItemInfo {
                position,
                length,
                take_name,
            });
        }

        items
    }

    // ── Routing ──────────────────────────────────────────────────────────

    /// Get the number of sends from a track.
    pub fn send_count(&self, track_guid: &str) -> u32 {
        let reaper = Reaper::get();
        let project = reaper.current_project();
        let Some(track) = find_track_by_guid(&project, track_guid) else {
            return 0;
        };
        let hw_count = track.typed_send_count(reaper_high::SendPartnerType::HardwareOutput);
        let total = track.send_count();
        total.saturating_sub(hw_count)
    }

    /// Get the destination track GUID for a send by index.
    pub fn send_dest_guid(&self, track_guid: &str, send_index: u32) -> Option<String> {
        let reaper = Reaper::get();
        let project = reaper.current_project();
        let track = find_track_by_guid(&project, track_guid)?;
        let hw_count = track.typed_send_count(reaper_high::SendPartnerType::HardwareOutput);
        let route = track.send_by_index(hw_count + send_index)?;
        match route.partner()? {
            reaper_high::TrackRoutePartner::Track(dest) => {
                Some(dest.guid().to_string_without_braces())
            }
            _ => None,
        }
    }

    /// Add a send from source track to destination track. Returns the send index.
    pub fn add_send(&self, source_guid: &str, dest_guid: &str) -> Option<u32> {
        let reaper = Reaper::get();
        let project = reaper.current_project();
        let source = find_track_by_guid(&project, source_guid)?;
        let dest = find_track_by_guid(&project, dest_guid)?;
        let low = reaper.medium_reaper().low();
        let route_idx =
            unsafe { low.CreateTrackSend(source.raw().ok()?.as_ptr(), dest.raw().ok()?.as_ptr()) };
        if route_idx >= 0 {
            Some(route_idx as u32)
        } else {
            None
        }
    }

    /// Mute or unmute a send by index.
    pub fn set_send_muted(&self, track_guid: &str, send_index: u32, muted: bool) {
        let reaper = Reaper::get();
        let project = reaper.current_project();
        let Some(track) = find_track_by_guid(&project, track_guid) else {
            return;
        };
        let hw_count = track.typed_send_count(reaper_high::SendPartnerType::HardwareOutput);
        let Some(route) = track.send_by_index(hw_count + send_index) else {
            return;
        };
        if muted {
            let _ = route.mute();
        } else {
            let _ = route.unmute();
        }
    }

    /// Check if a send is muted.
    pub fn is_send_muted(&self, track_guid: &str, send_index: u32) -> bool {
        let reaper = Reaper::get();
        let project = reaper.current_project();
        let Some(track) = find_track_by_guid(&project, track_guid) else {
            return false;
        };
        let hw_count = track.typed_send_count(reaper_high::SendPartnerType::HardwareOutput);
        let Some(route) = track.send_by_index(hw_count + send_index) else {
            return false;
        };
        route.is_muted().unwrap_or(false)
    }
}

// =============================================================================
// Private helpers
// =============================================================================

/// Find a track by GUID string within a project (linear scan).
fn find_track_by_guid(project: &reaper_high::Project, guid: &str) -> Option<reaper_high::Track> {
    for i in 0..project.track_count() {
        if let Some(track) = project.track_by_index(i)
            && track.guid().to_string_without_braces() == guid
        {
            return Some(track);
        }
    }
    None
}

/// Convert a reaper-high Track to our daw_proto::Track.
///
/// Mirrors the logic in `track.rs::build_track_info`.
fn build_track_info(track: &reaper_high::Track) -> daw_proto::Track {
    let guid = track.guid().to_string_without_braces();
    let index = track.index().unwrap_or(0);
    let name = track
        .name()
        .map(|n| n.to_str().to_string())
        .unwrap_or_else(|| {
            if track.is_master_track() {
                "MASTER".to_string()
            } else {
                format!("Track {}", index + 1)
            }
        });

    let color = track
        .custom_color()
        .map(|c| ((c.r as u32) << 16) | ((c.g as u32) << 8) | (c.b as u32));

    let volume = track.volume().get();
    let pan = track.pan().reaper_value().get();
    let muted = track.is_muted();
    let soloed = track.is_solo();
    let armed = track.is_armed(false);
    let selected = track.is_selected();
    let folder_depth = track.folder_depth_change();
    let is_folder = folder_depth > 0;
    let fx_count = track.normal_fx_chain().fx_count();
    let input_fx_count = track.input_fx_chain().fx_count();
    let visible_in_tcp = track.is_shown(reaper_medium::TrackArea::Tcp);
    let visible_in_mixer = track.is_shown(reaper_medium::TrackArea::Mcp);
    let (record_input, parent_send) = crate::track::record_input_and_parent_send(track);
    let height = crate::track::tcp_height(track);
    let send_count = track.typed_send_count(reaper_high::SendPartnerType::Track);
    let receive_count = track.receive_count();

    let lanes = track
        .raw()
        .map(crate::lanes::read_lanes)
        .unwrap_or_default();
    daw_proto::Track {
        guid,
        index,
        name,
        color,
        muted,
        soloed,
        armed,
        selected,
        volume,
        pan,
        phase_inverted: track.phase_is_inverted(),
        automation_mode: {
            use daw_proto::primitives::AutomationMode as P;
            use reaper_medium::AutomationMode as R;
            match track.automation_mode() {
                R::TrimRead => P::TrimRead,
                R::Read => P::Read,
                R::Touch => P::Touch,
                R::Write => P::Write,
                R::Latch => P::Latch,
                R::LatchPreview => P::LatchPreview,
                R::Unknown(_) => P::TrimRead,
            }
        },
        input_monitor: {
            use daw_proto::track::InputMonitoringMode as P;
            use reaper_medium::InputMonitoringMode as R;
            match track.input_monitoring_mode() {
                R::Off => P::Off,
                R::Normal => P::Normal,
                R::NotWhenPlaying => P::NotWhenPlaying,
                _ => P::Off,
            }
        },
        parent_guid: None,
        folder_depth,
        is_folder,
        lane_count: lanes.lane_count,
        lane_play_mask: lanes.lane_play_mask,
        lane_names: lanes.lane_names,
        lane_display: lanes.lane_display,
        grouping: daw_proto::track::TrackGrouping::default(),
        visible_in_tcp,
        visible_in_mixer,
        send_count,
        receive_count,
        fx_count,
        input_fx_count,
        height,
        // REAPER has no strip width, so there is nothing to read: the
        // value lives in the project's ext state and reaches the model
        // through the loader.
        width: None,
        record_input,
        parent_send,
    }
}

/// Walk the flat track list and assign `parent_guid` using folder depth changes.
///
/// Mirrors the logic in `track.rs::assign_parent_guids`.
fn assign_parent_guids(tracks: &mut [daw_proto::Track]) {
    let mut folder_stack: Vec<String> = Vec::new();

    for track in tracks.iter_mut() {
        track.parent_guid = folder_stack.last().cloned();

        let depth = track.folder_depth;
        if depth > 0 {
            folder_stack.push(track.guid.clone());
        } else if depth < 0 {
            for _ in 0..depth.unsigned_abs() {
                folder_stack.pop();
            }
        }
    }
}

/// Build an `Fx` proto struct from a reaper-high Fx.
///
/// Mirrors the logic in `fx.rs::build_fx_info`.
fn build_fx_info(fx: &reaper_high::Fx, chain: Option<&reaper_high::FxChain>) -> daw_proto::Fx {
    let guid = fx
        .get_or_query_guid()
        .map(|g| g.to_string_without_braces())
        .unwrap_or_else(|_| {
            chain
                .and_then(|c| reaper_high::get_fx_guid(c, fx.index()))
                .map(|g| g.to_string_without_braces())
                .unwrap_or_default()
        });
    let name = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        fx.name().to_str().to_string()
    }))
    .unwrap_or_else(|_| "(unknown)".to_string());
    let index = fx.index();
    let enabled =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| fx.is_enabled())).unwrap_or(false);
    let offline =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| !fx.is_online())).unwrap_or(false);
    let window_open =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| fx.window_is_open()))
            .unwrap_or(false);
    let parameter_count =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| fx.parameter_count()))
            .unwrap_or(0);

    let (plugin_name, plugin_type) = match fx.info() {
        Ok(info) => {
            let ptype = parse_fx_type(&info.sub_type_expression);
            (info.effect_name, ptype)
        }
        Err(_) => (name.clone(), daw_proto::FxType::Unknown),
    };

    let preset_name = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        fx.preset_name()
            .map(|rs| rs.to_str().to_string())
            .filter(|s| !s.is_empty())
    }))
    .unwrap_or(None);

    daw_proto::Fx {
        guid,
        index,
        name,
        plugin_name,
        plugin_type,
        enabled,
        offline,
        window_open,
        parameter_count,
        preset_name,
    }
}

/// Parse an FX sub-type expression into an FxType enum.
///
/// Mirrors the logic in `fx.rs::parse_fx_type`.
fn parse_fx_type(sub_type: &str) -> daw_proto::FxType {
    match sub_type {
        "VST" | "VSTi" => daw_proto::FxType::Vst2,
        "VST3" | "VST3i" => daw_proto::FxType::Vst3,
        "CLAP" | "CLAPi" => daw_proto::FxType::Clap,
        "AU" | "AUi" => daw_proto::FxType::Au,
        "JS" => daw_proto::FxType::Js,
        _ => daw_proto::FxType::Unknown,
    }
}
