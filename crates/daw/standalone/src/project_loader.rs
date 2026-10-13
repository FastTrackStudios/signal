//! Load a REAPER `.rpp` project into a [`Standalone`] backend.
//!
//! Parses with `dawfile-reaper` and populates `ProjectState` with
//! tracks, items, takes, markers, regions, tempo points, hardware
//! outputs, and routing edges. Returns a `LoadedProject` summary so
//! callers can sanity-check what made it across.
//!
//! Scope (per current standalone capabilities): everything except
//! FX processing. FX entries on the source RPP are skipped (the
//! standalone FX impl is synthetic; loading real plugin state would
//! require a plugin host). Automation envelopes are also skipped for
//! now — the audio-graph integration will need them but the proto
//! `Automation` trait already works against the runtime envelopes
//! created via `add_point`, so loaded envelopes can land in a follow-up.
//!
//! Feature-gated under `rpp-project` (pulls dawfile-reaper, not WASM-
//! compatible due to rayon). For browser use cases, build the
//! project state directly via the proto trait methods.

#![cfg(any(feature = "rpp-project", feature = "rpp-project-wasm"))]

use std::sync::atomic::{AtomicUsize, Ordering};

use dawfile_reaper::types::item::{SourceType as RppSourceType, Take as RppTake};
use dawfile_reaper::types::project::{DecodeOptions, ReaperProject};
use uuid::Uuid;

use daw_proto::item::{FadeShape, Item, SourceType};
use daw_proto::primitives::{Duration, PositionInSeconds, Tempo, TimeSignature};
use daw_proto::project::ProjectInfo;
use daw_proto::track::Track;
use daw_proto::{Marker, Region, Take, TempoPoint};

use crate::sync::{ItemEntry, Standalone, TakeList, TrackExt};

/// Like [`load_rpp`] but pulls audio bytes through the project's
/// [`MediaBay`](crate::media_bay::MediaBay) resolver instead of an
/// inline closure. Caller must install a [`BayFileResolver`](
/// crate::media_bay::BayFileResolver) on the bay first — native apps
/// use [`FsFileResolver`](crate::media_bay::FsFileResolver), browser
/// apps install a JS-backed one.
///
/// ```ignore
/// use daw_standalone::media_bay::FsFileResolver;
/// use daw_standalone::project_loader::load_rpp_via_bay;
///
/// daw.media_bay().set_file_resolver(Box::new(FsFileResolver));
/// let (proj, audio) = load_rpp_via_bay(&daw, "Song", &path, &rpp_text)?;
/// ```
#[cfg(feature = "decode")]
pub fn load_rpp_via_bay(
    daw: &Standalone,
    project_name: &str,
    project_path: &str,
    rpp_text: &str,
) -> Result<
    (
        LoadedProject,
        crate::audio_engine::materialize::MaterializeReport,
    ),
    String,
> {
    let proj = load_rpp_text(daw, project_name, project_path, rpp_text)?;
    let audio = crate::audio_engine::materialize::materialize_via_bay(daw, &proj.project_guid)?;
    Ok((proj, audio))
}

/// One-shot wrapper that loads structure AND materializes audio.
///
/// Equivalent to `load_rpp_text(...)` followed by
/// `materialize_audio(...)`, returning both reports. Available when
/// the `decode` feature is on (so symphonia is available).
///
/// ```ignore
/// use daw_standalone::project_loader::load_rpp;
///
/// let (proj, audio) = load_rpp(&daw, "Song", "/tmp/song.rpp", &rpp_text, |path| {
///     std::fs::read(path).map_err(|e| e.to_string())
/// })?;
/// eprintln!("loaded {} tracks, {} audio sources", proj.track_count, audio.loaded);
/// ```
#[cfg(feature = "decode")]
pub fn load_rpp<F>(
    daw: &Standalone,
    project_name: &str,
    project_path: &str,
    rpp_text: &str,
    resolver: F,
) -> Result<
    (
        LoadedProject,
        crate::audio_engine::materialize::MaterializeReport,
    ),
    String,
>
where
    F: FnMut(&str) -> Result<Vec<u8>, String>,
{
    let proj = load_rpp_text(daw, project_name, project_path, rpp_text)?;
    let audio =
        crate::audio_engine::materialize::materialize_audio(daw, &proj.project_guid, resolver);
    Ok((proj, audio))
}

/// REAPER native colour (Windows `COLORREF`, `0x..BBGGRR` + the
/// `0x1000000` custom flag) → the canonical `0xRRGGBB` the daw-proto
/// types carry.
pub(crate) fn native_color_to_rgb(c: u32) -> u32 {
    let r = c & 0xff;
    let g = (c >> 8) & 0xff;
    let b = (c >> 16) & 0xff;
    (r << 16) | (g << 8) | b
}

/// A REAPER colour field as the colour it sets, if it sets one: only with
/// the `0x1000000` custom flag, as REAPER reads it. Without it the field
/// is not a colour — `PEAKCOL 16576` is REAPER's "none", drawn in the
/// theme's default; read as a colour it painted those tracks orange. (A
/// field written negative, `-12566400` = `0xFF404080`, has the flag: it
/// is that colour.)
pub(crate) fn custom_color(c: i32) -> Option<u32> {
    let c = c as u32;
    (c & 0x0100_0000 != 0).then(|| native_color_to_rgb(c & 0x00ff_ffff))
}

/// Summary of what was loaded.
#[derive(Debug, Default)]
pub struct LoadedProject {
    pub project_guid: String,
    pub track_count: usize,
    pub item_count: usize,
    pub take_count: usize,
    pub marker_count: usize,
    pub region_count: usize,
    pub tempo_point_count: usize,
    pub hw_output_count: usize,
    /// Warnings emitted during the load (e.g. unsupported FX skipped).
    pub warnings: Vec<String>,
}

/// Parse RPP text and populate a fresh project in `daw`. Returns the
/// seeded project's GUID + a summary of what was loaded.
pub fn load_rpp_text(
    daw: &Standalone,
    project_name: &str,
    project_path: &str,
    rpp_text: &str,
) -> Result<LoadedProject, String> {
    let rpp =
        dawfile_reaper::parse_rpp_file(rpp_text).map_err(|e| format!("rpp parse failed: {e:?}"))?;
    let project = ReaperProject::from_rpp_project_with_options(&rpp, DecodeOptions::full())
        .map_err(|e| format!("rpp decode failed: {e:?}"))?;

    let project_guid = Uuid::new_v4().to_string();
    daw.seed_project(ProjectInfo {
        guid: project_guid.clone(),
        name: project_name.to_string(),
        path: project_path.to_string(),
    });

    let mut summary = LoadedProject {
        project_guid: project_guid.clone(),
        ..Default::default()
    };

    populate_tracks(daw, &project_guid, &project, rpp_text, &mut summary);
    populate_markers_regions(daw, &project_guid, &project, &mut summary);
    populate_tempo(daw, &project_guid, &project, &mut summary);
    populate_routing(daw, &project_guid, &project, &mut summary);
    populate_fx_chains(daw, &project_guid, &project, &mut summary);
    fill_pools(daw, &project_guid, rpp_text);
    // What did not come in, said once: a project mixed with its plugins
    // missing sounds wrong with nothing on screen to say why.
    if !summary.warnings.is_empty() {
        tracing::warn!(
            project.name = project_name,
            load.warnings = summary.warnings.len(),
            load.first = ?summary.warnings.iter().take(8).collect::<Vec<_>>(),
            "the project loaded without some of what it holds"
        );
    }

    Ok(summary)
}

/// Make `project_guid`'s relative take paths absolute against `dir`.
///
/// A project file names its media relative to its own folder
/// (`Media/Bass.wav`), and the media bay resolves through ONE resolver
/// per engine. An engine holding several projects from different folders
/// — a setlist, each song in its own folder with its own `Media/Click.wav`
/// — cannot tell one song's `Media/Click.wav` from another's through a
/// relative resolver, so each project's references are anchored to its
/// folder as it loads, before it materializes. A `.session` save writes
/// paths inside the project's folder back as relative, so nothing about
/// the saved file changes.
///
/// Returns how many take paths were anchored.
pub fn anchor_media(daw: &Standalone, project_guid: &str, dir: &std::path::Path) -> usize {
    // Absolute, whatever it was given: a folder named relative to the
    // working directory (`../sessions/Song`) would anchor nothing, and a
    // save would write those paths out as if they were the project's own.
    let dir = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
    daw.with_project_mut(project_guid, |p| {
        let mut anchored = 0;
        for list in p.takes.values_mut() {
            for take in &mut list.takes {
                if let Some(path) = take.source_file_path.as_mut()
                    && !path.is_empty()
                    && std::path::Path::new(path.as_str()).is_relative()
                {
                    *path = dir.join(path.as_str()).to_string_lossy().into_owned();
                    anchored += 1;
                }
            }
        }
        anchored
    })
    .unwrap_or(0)
}

fn populate_tracks(
    daw: &Standalone,
    project_guid: &str,
    project: &ReaperProject,
    rpp_text: &str,
    summary: &mut LoadedProject,
) {
    static ITEM_COUNTER: AtomicUsize = AtomicUsize::new(0);

    let _ = daw.with_project_mut(project_guid, |p| {
        // Default project tempo / time signature from the tempo
        // envelope (or fall back to 120/4-4).
        if let Some((tempo, time_signature)) = transport_tempo_from_rpp(project) {
            p.transport.tempo = tempo;
            p.transport.time_signature = time_signature;
        }

        // Master section: `MASTER_VOLUME vol pan …` + `MASTERMUTESOLO`.
        if let Some((vol, pan, ..)) = project.properties.master_volume {
            p.master_volume = vol.max(0.0);
            p.master_pan = pan.clamp(-1.0, 1.0);
        }
        if let Some(ms) = project.properties.master_mute_solo {
            p.master_muted = ms & 1 != 0;
        }

        // Ours, not REAPER's — see `mcp_widths`.
        let widths = mcp_widths(rpp_text);

        for (idx, rt) in project.tracks.iter().enumerate() {
            let (track, ext) = track_from_rpp(rt, idx, &widths);
            let guid = track.guid.clone();
            let lane_count = track.lane_count;
            p.tracks.push(track);
            p.track_ext.insert(guid.clone(), ext);
            // Its ext-state (`EXT FTS:kind piece`): what the template
            // said the track is.
            for (key, value) in &rt.extension_data {
                p.track_ext_state
                    .insert((guid.clone(), String::new(), key.clone()), value.clone());
            }

            // Track automation envelopes (VOLENV2 / PANENV2 / …).
            for env in &rt.envelopes {
                if let Some((key, data)) = convert_track_envelope(env) {
                    p.envelopes.insert((guid.clone(), key), data);
                }
            }

            // Items on this track.
            let track_items = p.items_by_track.entry(guid.clone()).or_default();
            for (item_idx, ri) in rt.items.iter().enumerate() {
                let item = item_from_rpp(ri, &guid, item_idx, lane_count);
                let item_guid = item.guid.clone();

                ITEM_COUNTER.fetch_add(1, Ordering::Relaxed);
                p.items.insert(item_guid.clone(), ItemEntry { item });
                track_items.push(item_guid.clone());

                // Takes.
                let mut takes_out = Vec::with_capacity(ri.takes.len().max(1));
                for (take_idx, rt_take) in ri.takes.iter().enumerate() {
                    let take = build_take(&item_guid, take_idx as u32, rt_take);
                    // r[impl drums.open.stretch-markers]
                    // `SM` lines are keyed per take; the third token is the
                    // marker's slope (daw-proto's field of the same name).
                    if !rt_take.stretch_markers.is_empty() {
                        p.stretch_markers
                            .insert(take.guid.clone(), stretch_markers_from_rpp(rt_take));
                    }
                    // If this is a MIDI take, decode its event stream
                    // into MidiNote entries on `p.midi_notes`. The
                    // renderer reads from this map to feed VST3i /
                    // CLAPi at playback.
                    if take.is_midi
                        && let Some(src) = rt_take.source.as_ref()
                        && let Some(midi) = src.midi_data.as_ref()
                    {
                        let decoded = decode_midi_source(midi);
                        if !decoded.notes.is_empty() {
                            p.midi_notes.insert(take.guid.clone(), decoded.notes);
                        }
                        if !decoded.ccs.is_empty() {
                            p.midi_ccs.insert(take.guid.clone(), decoded.ccs);
                        }
                        if !decoded.pitch_bends.is_empty() {
                            p.midi_pitch_bends
                                .insert(take.guid.clone(), decoded.pitch_bends);
                        }
                        if !decoded.program_changes.is_empty() {
                            p.midi_program_changes
                                .insert(take.guid.clone(), decoded.program_changes);
                        }
                        if !decoded.sysex.is_empty() {
                            p.midi_sysex.insert(take.guid.clone(), decoded.sysex);
                        }
                        if !decoded.channel_pressures.is_empty() {
                            p.midi_channel_pressures
                                .insert(take.guid.clone(), decoded.channel_pressures);
                        }
                        if !decoded.poly_pressures.is_empty() {
                            p.midi_poly_pressures
                                .insert(take.guid.clone(), decoded.poly_pressures);
                        }
                    }
                    takes_out.push(take);
                }
                // Which take plays — see `active_take_index`.
                let active_idx = active_take_index(ri);
                if !takes_out.is_empty() {
                    p.takes.insert(
                        item_guid.clone(),
                        TakeList {
                            active_idx,
                            takes: takes_out,
                        },
                    );
                    summary.take_count += ri.takes.len();
                }

                summary.item_count += 1;
            }
            summary.track_count += 1;
        }

        // Second pass: resolve parent_guid via folder_depth so the
        // standalone view reflects REAPER's nesting.
        resolve_folder_parents(&mut p.tracks);
    });

    let _ = ITEM_COUNTER; // silence unused-warning when this file is the only consumer
}

/// One `<TRACK>` as this loader reads it: the proto [`Track`] and the
/// extended fields beside it. Shared with the `.session` writer, which
/// compares the engine's state against exactly this view of the original
/// file to decide what it has to write back.
pub(crate) fn track_from_rpp(
    rt: &dawfile_reaper::types::Track,
    idx: usize,
    widths: &std::collections::HashMap<String, u32>,
) -> (Track, TrackExt) {
    // Synthesize a GUID — REAPER's track GUIDs aren't always
    // exposed by dawfile-reaper. Use track_id when available.
    let guid = rt
        .track_id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let (volume, pan) = rt
        .volpan
        .as_ref()
        .map(|v| (v.volume, v.pan))
        .unwrap_or((1.0, 0.0));
    let (muted, soloed) = rt
        .mutesolo
        .as_ref()
        .map(|m| {
            let solo = !matches!(m.solo, dawfile_reaper::types::track::TrackSoloState::NoSolo);
            (m.mute, solo)
        })
        .unwrap_or((false, false));
    let (folder_depth, is_folder) = rt
        .folder
        .as_ref()
        .map(|f| {
            use dawfile_reaper::types::track::FolderState as FS;
            let depth = match f.folder_state {
                FS::FolderParent => 1,
                // ISBUS's second field is the depth DELTA: a
                // last-in-folder track can close several
                // nested folders at once (`ISBUS 2 -3`).
                FS::LastInFolder => f.indentation.min(-1),
                // `ISBUS 0 -1`, which is what REAPER actually
                // writes for the last track in a folder: the
                // first field says "not a folder parent" and
                // the second still closes one. Reading only the
                // state flag left every folder open, so a
                // session's nesting grew by one at each bus and
                // never came back down.
                _ if f.indentation < 0 => f.indentation,
                FS::Regular | FS::Unknown(_) => 0,
            };
            (depth, depth > 0)
        })
        .unwrap_or((0, false));

    // Fixed item lanes (REAPER 7 comping), decoded by the one
    // decoder every loader shares (`FixedLaneFields::decode`).
    let dawfile_reaper::types::track::FixedLaneState {
        lane_count,
        lane_play_mask,
        lane_names,
        lane_display,
    } = rt.fixed_lane_state();

    let grouping = daw_proto::track::TrackGrouping::from_rpp_fields(
        rt.group_flags.as_deref().unwrap_or(&[]),
        rt.group_flags_high.as_deref().unwrap_or(&[]),
    );

    let track = Track {
        guid: guid.clone(),
        // Ours, not REAPER's — see `mcp_widths`.
        width: widths.get(&guid).copied(),
        automation_mode: {
            use daw_proto::primitives::AutomationMode as P;
            use dawfile_reaper::types::track::AutomationMode as R;
            match rt.automation_mode {
                R::TrimRead => P::TrimRead,
                R::Read => P::Read,
                R::Touch => P::Touch,
                R::Write => P::Write,
                R::Latch => P::Latch,
                R::Unknown(_) => P::TrimRead,
            }
        },
        input_monitor: {
            use daw_proto::track::InputMonitoringMode as P;
            use dawfile_reaper::types::track::MonitorMode as R;
            match rt.record.as_ref().map(|r| r.monitor) {
                Some(R::On) => P::Normal,
                Some(R::Auto) => P::NotWhenPlaying,
                _ => P::Off,
            }
        },
        index: idx as u32,
        name: rt.name.clone(),
        color: rt.peak_color.and_then(custom_color),
        muted,
        soloed,
        armed: rt.record.as_ref().map(|r| r.armed).unwrap_or(false),
        phase_inverted: rt.invert_phase,
        selected: rt.selected,
        volume,
        pan,
        parent_guid: None, // resolved in a second pass once we
        // have folder nesting, currently
        // tracked only via folder_depth
        folder_depth,
        is_folder,
        lane_count,
        lane_play_mask,
        lane_names,
        lane_display,
        grouping,
        visible_in_tcp: rt
            .show_in_mixer
            .as_ref()
            .map(|s| s.show_in_track_list)
            .unwrap_or(true),
        visible_in_mixer: rt
            .show_in_mixer
            .as_ref()
            .map(|s| s.show_in_mixer)
            .unwrap_or(true),
        // Filled by `Tracks::all` from the routing maps, which
        // are the authority — a count stored here would go
        // stale the first time a send was added.
        send_count: 0,
        receive_count: 0,
        fx_count: 0, // FX not loaded (synthetic standalone)
        input_fx_count: 0,
        // The project's own `TRACKHEIGHT`. Absent, or zero —
        // REAPER's sentinel for automatic — means the panel
        // picks its own, so it stays `None` rather than becoming
        // a track nought pixels tall.
        height: rt
            .track_height
            .as_ref()
            .map(|h| h.height)
            .filter(|h| *h > 0)
            .map(|h| h as u32),
        // The project file's own answer, not a default: a track
        // muted out of the master bus must not read as sending to
        // it just because nobody asked the routing service.
        parent_send: rt.master_send.as_ref().map(|m| m.enabled).unwrap_or(true),
        record_input: rt
            .record
            .as_ref()
            .map_or(daw_proto::track::RecordInput::None, |r| {
                record_input_from_rpp(r.input)
            }),
    };
    let ext = TrackExt {
        num_channels: rt.channel_count.max(1).min(128),
        record_input: track.record_input,
        parent_send_enabled: track.parent_send,
        tcp_height_pixels: 0,
        comping: rt.lane_comping(),
    };
    (track, ext)
}

/// One `<ITEM>` as this loader reads it (its takes are [`build_take`]'s).
pub(crate) fn item_from_rpp(
    ri: &dawfile_reaper::types::Item,
    track_guid: &str,
    item_idx: usize,
    lane_count: u32,
) -> Item {
    let item_guid = ri
        .item_guid
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let mut item = Item::default();
    item.guid = item_guid;
    item.track_guid = track_guid.to_string();
    item.index = item_idx as u32;
    item.position = PositionInSeconds::from_seconds(ri.position);
    // REAPER saves the odd item with a hair of negative length (and fades
    // to match) — `LENGTH -0.00316666666667` — and opens it as an empty
    // item. So do we: clamped to nothing, never a panic on open.
    item.length = Duration::from_seconds(ri.length.max(0.0));
    item.snap_offset = Duration::from_seconds(ri.snap_offset.max(0.0));
    item.muted = ri.mute.as_ref().map(|m| m.muted).unwrap_or(false);
    item.selected = ri.selected;
    item.volume = ri.volpan.as_ref().map(|v| v.item_trim).unwrap_or(1.0);
    if let Some(fi) = &ri.fade_in {
        item.fade_in_length = Duration::from_seconds(fi.time.max(0.0));
        item.fade_in_shape = fade_curve_to_shape(fi.curve_type);
    }
    if let Some(fo) = &ri.fade_out {
        item.fade_out_length = Duration::from_seconds(fo.time.max(0.0));
        item.fade_out_shape = fade_curve_to_shape(fo.curve_type);
    }
    item.color = ri.color.and_then(custom_color);
    item.loop_source = ri.loop_source;
    // Fixed-lane membership only matters on lane-enabled
    // tracks (YPOS also appears for free item positioning).
    item.fixed_lane = if lane_count > 0 {
        ri.lane.map(|l| l.max(0) as u32)
    } else {
        None
    };
    item.take_count = ri.takes.len().max(1) as u32;
    item.label = label_from_rpp(ri);
    // proto `Item` doesn't carry `channel_mode` yet — drop.
    item
}

/// An item's label: its `<NOTES>` block (REAPER's `P_NOTES`), one `|`
/// line a line. The parser keeps a nested block it does not model on the
/// take it was read under, which for an item's own notes is take #0.
pub(crate) fn label_from_rpp(ri: &dawfile_reaper::types::Item) -> Option<String> {
    let block = ri
        .takes
        .iter()
        .flat_map(|t| &t.extra_blocks)
        .find(|b| is_notes_block(b))?;
    let text = block
        .lines()
        .skip(1)
        .filter_map(|l| l.trim_start().strip_prefix('|'))
        .collect::<Vec<_>>()
        .join("\n");
    (!text.is_empty()).then_some(text)
}

/// Whether a nested block is an item's `<NOTES>`.
pub(crate) fn is_notes_block(block: &str) -> bool {
    block.trim_start().starts_with("<NOTES")
}

/// A take's `SM` lines, sorted by position. The third token is the
/// marker's slope (daw-proto's field of the same name).
pub(crate) fn stretch_markers_from_rpp(rt_take: &RppTake) -> Vec<daw_proto::StretchMarker> {
    let mut markers: Vec<daw_proto::StretchMarker> = rt_take
        .stretch_markers
        .iter()
        .map(|sm| daw_proto::StretchMarker {
            position: sm.position,
            source_position: sm.source_position,
            slope: sm.rate.unwrap_or(0.0),
        })
        .collect();
    markers.sort_by(|a, b| a.position.total_cmp(&b.position));
    markers
}

/// Which take plays.
///
/// `TAKE SEL` is REAPER's own marker for the active take and
/// is authoritative when present. The GUID match below is the
/// older path and only works for an item whose FIRST take is
/// written inline (so the item carries that take's `GUID`).
///
/// A comped item is not written that way. It opens with
/// `TAKE NULL` — an empty comp-lane slot, which occupies a
/// take index exactly as REAPER counts it — so there is no
/// item-level `GUID` to match, `position` finds nothing, and
/// the old `unwrap_or(0)` landed on a null slot. The item
/// then reported an `Empty` active take and every consumer
/// that keeps only audio dropped it: on a real session
/// (`set in stone`) the tom trigger tracks composed to 1.3%
/// non-zero at -66 dBFS while their source files were full
/// 317s recordings. It presented as "the trigger tracks hold
/// no audio".
///
/// Falling back to the first take that has a source keeps a
/// pathological item (nulls only, no SEL) playing something
/// real rather than silence.
pub(crate) fn active_take_index(ri: &dawfile_reaper::types::Item) -> u32 {
    ri.takes
        .iter()
        .position(|t| t.is_selected)
        .or_else(|| {
            ri.take_guid.as_ref().and_then(|g| {
                ri.takes
                    .iter()
                    .position(|t| t.take_guid.as_ref() == Some(g))
            })
        })
        .or_else(|| ri.takes.iter().position(|t| t.source.is_some()))
        .unwrap_or(0) as u32
}

/// All MIDI event types decoded from an RPP `MidiSource`.
pub(crate) struct DecodedMidiSource {
    pub(crate) notes: Vec<daw_proto::midi::MidiNote>,
    pub(crate) ccs: Vec<daw_proto::midi::MidiCC>,
    pub(crate) pitch_bends: Vec<daw_proto::midi::MidiPitchBend>,
    pub(crate) program_changes: Vec<daw_proto::midi::MidiProgramChange>,
    pub(crate) sysex: Vec<daw_proto::midi::MidiSysEx>,
    pub(crate) channel_pressures: Vec<daw_proto::midi::MidiChannelPressure>,
    pub(crate) poly_pressures: Vec<daw_proto::midi::MidiPolyPressure>,
}

/// Walk a parsed RPP `MidiSource`, demultiplex the delta-tick event
/// stream into the proto's typed per-take collections (notes, CCs,
/// pitch bends, program changes, SysEx). The renderer reads from
/// these collections and feeds them per-block to the track's
/// instrument plugin.
///
/// Time conversion: REAPER's MIDI source stores deltas at
/// `ticks_per_qn` (typically 960). All proto types use *quarter notes*
/// for `start_ppq` / `position_ppq`, so we divide accumulated ticks
/// by `ticks_per_qn`.
///
/// Aftertouch (channel pressure) and poly-pressure are currently
/// dropped — they need their own proto vectors before we can route
/// them. SysEx is preserved verbatim including the leading 0xF0 /
/// trailing 0xF7 framing bytes (REAPER's `E` lines store the raw
/// MIDI bytes per the spec).
pub(crate) fn decode_midi_source(
    midi: &dawfile_reaper::types::item::MidiSource,
) -> DecodedMidiSource {
    use daw_proto::midi::{
        MidiCC, MidiChannelPressure, MidiNote, MidiPitchBend, MidiPolyPressure, MidiProgramChange,
        MidiSysEx,
    };
    let tpq = midi.ticks_per_qn.max(1) as f64;
    let mut notes: Vec<MidiNote> = Vec::new();
    let mut ccs: Vec<MidiCC> = Vec::new();
    let mut pitch_bends: Vec<MidiPitchBend> = Vec::new();
    let mut program_changes: Vec<MidiProgramChange> = Vec::new();
    let mut sysex: Vec<MidiSysEx> = Vec::new();
    let mut channel_pressures: Vec<MidiChannelPressure> = Vec::new();
    let mut poly_pressures: Vec<MidiPolyPressure> = Vec::new();
    // (channel, pitch) → (start_tick, velocity, index_into_notes)
    let mut pending_notes: std::collections::HashMap<(u8, u8), (u64, u8, usize)> =
        std::collections::HashMap::new();
    let mut tick: u64 = 0;
    let mut next_note_idx: u32 = 0;
    let to_ppq = |t: u64| (t as f64) / tpq;

    // Deltas count from the previous event of EITHER kind: an `<X>` block
    // (text, notation, …) between two `E` lines carries part of the time
    // between them. Summing the `E` deltas alone put every event after
    // one early by its delta.
    use dawfile_reaper::types::item::MidiSourceEvent;
    let stream: Vec<(u32, Option<&[u8]>)> = if midi.event_stream.is_empty() {
        midi.events
            .iter()
            .map(|e| (e.delta_ticks, Some(e.bytes.as_slice())))
            .collect()
    } else {
        midi.event_stream
            .iter()
            .map(|ev| match ev {
                MidiSourceEvent::Midi(e) => (e.delta_ticks, Some(e.bytes.as_slice())),
                MidiSourceEvent::Extended(x) => (x.delta_ticks(), None),
            })
            .collect()
    };
    for (delta, bytes) in stream {
        tick = tick.saturating_add(delta as u64);
        let Some(bytes) = bytes else {
            continue;
        };
        let Some(&status) = bytes.first() else {
            continue;
        };
        let typ = status & 0xF0;
        let channel = status & 0x0F;
        match typ {
            0x90 => {
                let pitch = bytes.get(1).copied().unwrap_or(0) & 0x7F;
                let velocity = bytes.get(2).copied().unwrap_or(0) & 0x7F;
                if velocity == 0 {
                    if let Some((start_tick, vel, idx)) = pending_notes.remove(&(channel, pitch))
                        && let Some(n) = notes.get_mut(idx) {
                            n.length_ppq = to_ppq(tick.saturating_sub(start_tick));
                            n.velocity = vel;
                        }
                } else {
                    let idx = notes.len();
                    notes.push(MidiNote {
                        index: next_note_idx,
                        channel,
                        pitch,
                        velocity,
                        start_ppq: to_ppq(tick),
                        length_ppq: 0.0,
                        selected: false,
                        muted: false,
                    });
                    next_note_idx += 1;
                    pending_notes.insert((channel, pitch), (tick, velocity, idx));
                }
            }
            0x80 => {
                let pitch = bytes.get(1).copied().unwrap_or(0) & 0x7F;
                if let Some((start_tick, vel, idx)) = pending_notes.remove(&(channel, pitch))
                    && let Some(n) = notes.get_mut(idx) {
                        n.length_ppq = to_ppq(tick.saturating_sub(start_tick));
                        n.velocity = vel;
                    }
            }
            0xB0 => {
                // Control Change.
                let controller = bytes.get(1).copied().unwrap_or(0) & 0x7F;
                let value = bytes.get(2).copied().unwrap_or(0) & 0x7F;
                let idx = ccs.len() as u32;
                ccs.push(MidiCC {
                    index: idx,
                    channel,
                    controller,
                    value,
                    position_ppq: to_ppq(tick),
                    selected: false,
                });
            }
            0xC0 => {
                // Program Change.
                let program = bytes.get(1).copied().unwrap_or(0) & 0x7F;
                let idx = program_changes.len() as u32;
                program_changes.push(MidiProgramChange {
                    index: idx,
                    channel,
                    program,
                    position_ppq: to_ppq(tick),
                });
            }
            0xE0 => {
                // Pitch Bend: LSB then MSB, both 7-bit, combine to
                // a 14-bit unsigned then subtract 8192 for signed.
                let lsb = bytes.get(1).copied().unwrap_or(0) & 0x7F;
                let msb = bytes.get(2).copied().unwrap_or(0) & 0x7F;
                let unsigned = ((msb as u16) << 7) | lsb as u16;
                let signed = (unsigned as i32 - 8192) as i16;
                let idx = pitch_bends.len() as u32;
                pitch_bends.push(MidiPitchBend {
                    index: idx,
                    channel,
                    value: signed,
                    position_ppq: to_ppq(tick),
                    selected: false,
                });
            }
            0xF0
                // SysEx (status 0xF0) — store the entire frame
                // verbatim including the trailing 0xF7. Other 0xFn
                // realtime / system messages (clock, start, stop,
                // active sensing) aren't currently routed.
                if status == 0xF0 => {
                    let idx = sysex.len() as u32;
                    sysex.push(MidiSysEx {
                        index: idx,
                        position_ppq: to_ppq(tick),
                        data: bytes.to_vec(),
                    });
                }
            0xA0 => {
                // Poly Pressure (per-note aftertouch).
                let note = bytes.get(1).copied().unwrap_or(0) & 0x7F;
                let pressure = bytes.get(2).copied().unwrap_or(0) & 0x7F;
                let idx = poly_pressures.len() as u32;
                poly_pressures.push(MidiPolyPressure {
                    index: idx,
                    channel,
                    note,
                    pressure,
                    position_ppq: to_ppq(tick),
                    selected: false,
                });
            }
            0xD0 => {
                // Channel Pressure (mono aftertouch).
                let pressure = bytes.get(1).copied().unwrap_or(0) & 0x7F;
                let idx = channel_pressures.len() as u32;
                channel_pressures.push(MidiChannelPressure {
                    index: idx,
                    channel,
                    pressure,
                    position_ppq: to_ppq(tick),
                    selected: false,
                });
            }
            _ => {
                // 0xF1-0xFE realtime / system messages — drop.
            }
        }
    }
    DecodedMidiSource {
        notes,
        ccs,
        pitch_bends,
        program_changes,
        sysex,
        channel_pressures,
        poly_pressures,
    }
}

pub(crate) fn fade_curve_to_shape(curve: dawfile_reaper::types::item::FadeCurveType) -> FadeShape {
    use dawfile_reaper::types::item::FadeCurveType as F;
    match curve {
        F::Linear => FadeShape::Linear,
        F::Square => FadeShape::FastStart, // closest stand-in; proto has no
        // Square fade.
        F::SlowStartEnd => FadeShape::SlowStartEnd,
        F::FastStart => FadeShape::FastStart,
        F::FastEnd => FadeShape::FastEnd,
        F::Bezier => FadeShape::SlowStartEnd, // proto has no Bezier; pick smoothest stand-in
        F::Unknown(_) => FadeShape::Linear,
    }
}

pub(crate) fn build_take(item_guid: &str, index: u32, rt: &RppTake) -> Take {
    let take_guid = rt
        .take_guid
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let (source_type, source_file_path) = match &rt.source {
        Some(src) => {
            let st = match src.source_type {
                // Every compressed/PCM audio source REAPER's `<SOURCE ...>`
                // tag can name — not just `WAVE`. Previously only `Wave`
                // mapped to `Audio`; `Vorbis` (ogg — what session's
                // generated projects use), `Mp3`, `Flac`, and
                // `OfflineWave` all fell into `Unknown`, which silently
                // skips waveform-preview fetching (anything gated on
                // `SourceType::Audio` treated these takes as non-audio).
                RppSourceType::Wave
                | RppSourceType::OfflineWave
                | RppSourceType::Mp3
                | RppSourceType::Flac
                | RppSourceType::Vorbis => SourceType::Audio,
                RppSourceType::Midi => SourceType::Midi,
                RppSourceType::Empty => SourceType::Empty,
                RppSourceType::Video | RppSourceType::Section | RppSourceType::Unknown(_) => {
                    SourceType::Unknown
                }
            };
            let file = if src.file_path.is_empty() {
                None
            } else {
                Some(src.file_path.clone())
            };
            (st, file)
        }
        None => (SourceType::Empty, None),
    };

    Take {
        guid: take_guid,
        item_guid: item_guid.to_string(),
        index,
        is_active: false, // patched up by caller using item.take_guid
        name: rt.name.clone(),
        color: None,
        volume: rt.volpan.as_ref().map(|v| v.take_volume).unwrap_or(1.0),
        play_rate: rt.playrate.as_ref().map(|pr| pr.rate).unwrap_or(1.0),
        pitch: 0.0,
        preserve_pitch: true,
        channel_mode: {
            use dawfile_reaper::types::item::ChannelMode as CM;
            match rt.channel_mode {
                CM::Normal => 0,
                CM::ReverseStereo => 1,
                CM::MonoDownmix => 2,
                CM::MonoLeft => 3,
                CM::MonoRight => 4,
                // dawfile stores the decoded channel; re-encode to
                // REAPER's raw CHANMODE value space.
                CM::MonoChannel(ch) => ch as u32 + 2,
                CM::StereChannel(ch) => ch as u32 + 66,
                CM::Unknown(v) => v.max(0) as u32,
            }
        },
        start_offset: Duration::from_seconds(rt.slip_offset.max(0.0)),
        source_type,
        source_file_path,
        source_length: None,
        source_sample_rate: None,
        source_channels: None,
        is_midi: matches!(source_type, SourceType::Midi),
        midi_note_count: None,
    }
}

pub(crate) fn resolve_folder_parents(tracks: &mut [Track]) {
    // Stack-of-folder-guids walk: when entering a folder we push the
    // track guid onto the stack and mark following tracks until
    // depth decrement.
    let mut stack: Vec<String> = Vec::new();
    for t in tracks.iter_mut() {
        t.parent_guid = stack.last().cloned();
        // Apply depth AFTER setting parent (a folder's own parent is
        // the folder enclosing it, not itself).
        match t.folder_depth.signum() {
            1 => stack.push(t.guid.clone()),
            -1 => {
                for _ in 0..t.folder_depth.unsigned_abs() {
                    stack.pop();
                }
            }
            _ => {}
        }
    }
}

fn populate_markers_regions(
    daw: &Standalone,
    project_guid: &str,
    project: &ReaperProject,
    summary: &mut LoadedProject,
) {
    let _ = daw.with_project_mut(project_guid, |p| {
        // REAPER 7.62+ ruler lanes. A marker carries a lane index and
        // the project names the lanes (`RULERLANE 1 8 SONG 0 -1`), so
        // without the names a lane index is a bare number and the
        // grouping cannot be labelled. Stored where the `Project`
        // service's ruler-lane accessors already read from, so a loaded
        // project answers them the same way a hand-set one does.
        // The file numbers lanes from 1 (`RULERLANE 1 8 "SONG"`); the
        // service surface is REAPER's API, which numbers them from 0.
        for lane in &project.ruler_lanes {
            let Some(index) = file_lane_to_api(Some(lane.index)) else {
                continue;
            };
            p.ruler_lanes.insert(
                index,
                crate::sync::RulerLane {
                    name: lane.name.clone(),
                    flags: lane.flags.max(0) as u32,
                },
            );
        }
        for mr in &project.markers_regions.markers {
            let id = next_id(&mut p.next_marker_id);
            p.markers.insert(id, marker_from_rpp(mr, id));
            summary.marker_count += 1;
        }
        for mr in &project.markers_regions.regions {
            let id = next_id(&mut p.next_region_id);
            p.regions.insert(id, region_from_rpp(mr, id));
            summary.region_count += 1;
        }
    });
}

/// The project tempo and time signature this loader sets on the
/// transport: the tempo envelope's defaults, else the header
/// `TEMPO <bpm> <num> <denom>` — the project's one tempo, which
/// everything that converts time (the grid, the ruler, quantize targets)
/// reads. r[impl drums.group.tempo]
pub(crate) fn transport_tempo_from_rpp(project: &ReaperProject) -> Option<(Tempo, TimeSignature)> {
    if let Some(env) = &project.tempo_envelope {
        let (num, denom) = env.default_time_signature;
        Some((
            Tempo::from_bpm(env.default_tempo.max(1.0)),
            TimeSignature::new(num.max(1) as u32, denom.max(1) as u32),
        ))
    } else {
        project.properties.tempo.map(|(bpm, num, denom, _)| {
            (
                Tempo::from_bpm(bpm.max(1.0)),
                TimeSignature::new(num.max(1) as u32, denom.max(1) as u32),
            )
        })
    }
}

/// One `PT` line of the tempo envelope. The time signature is encoded
/// `num | denom << 16`.
pub(crate) fn tempo_point_from_rpp(
    pt: &dawfile_reaper::types::time_tempo::TempoTimePoint,
) -> TempoPoint {
    let mut tp = TempoPoint::default();
    tp.position = daw_proto::Position::from_time(PositionInSeconds::from_seconds(pt.position));
    tp.bpm = pt.tempo.max(1.0);
    if let Some(enc) = pt.time_signature_encoded {
        let num = (enc & 0xFFFF).max(1) as u32;
        let denom = ((enc >> 16) & 0xFFFF).max(1) as u32;
        tp.time_signature = Some(TimeSignature::new(num, denom));
    }
    tp
}

/// A marker/region colour: `0` is "none", anything else native.
fn marker_color_from_rpp(color: i32) -> Option<u32> {
    (color != 0).then(|| native_color_to_rgb(color as u32))
}

/// One `MARKER` line that is a marker, numbered `id` by this backend.
pub(crate) fn marker_from_rpp(mr: &dawfile_reaper::types::MarkerRegion, id: u32) -> Marker {
    Marker {
        id: Some(id),
        position: daw_proto::Position::from_time(PositionInSeconds::from_seconds(mr.position)),
        name: mr.name.clone(),
        color: marker_color_from_rpp(mr.color),
        guid: (!mr.guid.is_empty()).then(|| mr.guid.clone()),
        lane: file_lane_to_api(mr.lane),
    }
}

/// One region (a `MARKER` start/end pair), numbered `id` by this backend.
pub(crate) fn region_from_rpp(mr: &dawfile_reaper::types::MarkerRegion, id: u32) -> Region {
    let end = mr.end_position.unwrap_or(mr.position);
    Region {
        id: Some(id),
        time_range: daw_proto::primitives::TimeRange::from_seconds(mr.position, end),
        name: mr.name.clone(),
        color: marker_color_from_rpp(mr.color),
        guid: (!mr.guid.is_empty()).then(|| mr.guid.clone()),
        lane: file_lane_to_api(mr.lane),
    }
}

fn next_id(counter: &mut u32) -> u32 {
    let id = *counter;
    *counter = counter.saturating_add(1);
    id
}

fn populate_tempo(
    daw: &Standalone,
    project_guid: &str,
    project: &ReaperProject,
    summary: &mut LoadedProject,
) {
    let Some(env) = &project.tempo_envelope else {
        return;
    };
    let _ = daw.with_project_mut(project_guid, |p| {
        for pt in &env.points {
            p.tempo_points.push(tempo_point_from_rpp(pt));
            summary.tempo_point_count += 1;
        }
    });
}

/// Convert an RPP track envelope block into the runtime envelope map
/// entry. Returns `None` for envelope types the renderer doesn't
/// evaluate yet (width, tempo, FX-param blocks live elsewhere).
///
/// Value conventions translated to the renderer's:
/// - volume: linear gain, used as-is
/// - pan: RPP −1…1 (negative = left) → 0…1 (0.5 = centre)
/// - mute: RPP >0.5 = PLAY → ours >0.5 = MUTED (inverted)
pub(crate) fn convert_track_envelope(
    env: &dawfile_reaper::types::envelope::Envelope,
) -> Option<(crate::sync::EnvelopeKey, crate::sync::EnvelopeData)> {
    use daw_proto::automation::{EnvelopeShape, EnvelopeType};
    use daw_proto::primitives::AutomationMode;
    use dawfile_reaper::types::envelope::EnvelopePointShape as PS;

    enum ValueMap {
        Direct,
        Pan,
        Mute,
    }
    let (ty, map) = match env.envelope_type.as_str() {
        "VOLENV2" => (EnvelopeType::Volume, ValueMap::Direct),
        "VOLENV" => (EnvelopeType::VolumePrefx, ValueMap::Direct),
        "PANENV2" => (EnvelopeType::Pan, ValueMap::Pan),
        "PANENV" => (EnvelopeType::PanPrefx, ValueMap::Pan),
        "MUTEENV" | "MUTEENV2" => (EnvelopeType::Mute, ValueMap::Mute),
        _ => return None,
    };
    let points: Vec<daw_proto::automation::EnvelopePoint> = env
        .points
        .iter()
        .enumerate()
        .map(|(i, pt)| daw_proto::automation::EnvelopePoint {
            index: i as u32,
            time: PositionInSeconds::from_seconds(pt.position),
            value: match map {
                ValueMap::Direct => pt.value,
                ValueMap::Pan => (pt.value.clamp(-1.0, 1.0) + 1.0) / 2.0,
                ValueMap::Mute => 1.0 - pt.value.clamp(0.0, 1.0),
            },
            shape: match pt.shape {
                PS::Square => EnvelopeShape::Square,
                PS::SlowStartEnd => EnvelopeShape::SlowStartEnd,
                PS::FastStart => EnvelopeShape::FastStart,
                PS::FastEnd => EnvelopeShape::FastEnd,
                PS::Bezier => EnvelopeShape::Bezier,
                PS::Linear | PS::Default => EnvelopeShape::Linear,
            },
            tension: pt.bezier_tension.unwrap_or(0.0),
            selected: pt.selected.unwrap_or(false),
        })
        .collect();
    if points.is_empty() && env.automation_items.is_empty() {
        return None;
    }
    let mut data = crate::sync::EnvelopeData::new();
    data.visible = env.visible;
    data.armed = env.armed;
    // A track envelope has no mode of its own in the file (the track's
    // `AUTOMODE` governs it): one that plays opens as a new one is made,
    // Trim/Read, so a save and a reopen agree.
    data.automation_mode = if env.active {
        AutomationMode::TrimRead
    } else {
        AutomationMode::Off
    };
    data.points = points;
    data.automation_items = automation_items(&env.automation_items);
    Some((crate::sync::EnvelopeKey::Track(ty), data))
}

/// An envelope's `POOLEDENVINST`s as the engine's automation items, their
/// curves still empty — the pools they name are project-level and are
/// read in once the whole project is in ([`fill_pools`]).
pub(crate) fn automation_items(
    items: &[dawfile_reaper::types::envelope::AutomationItem],
) -> Vec<(
    daw_proto::automation::AutomationItem,
    Vec<daw_proto::automation::EnvelopePoint>,
)> {
    use daw_proto::primitives::Duration;
    items
        .iter()
        .enumerate()
        .map(|(i, ai)| {
            (
                daw_proto::automation::AutomationItem {
                    index: i as u32,
                    pool_id: ai.pool_index,
                    name: String::new(),
                    position: PositionInSeconds::from_seconds(ai.position),
                    length: Duration::from_seconds(ai.length),
                    start_offset: Duration::from_seconds(ai.start_offset),
                    play_rate: ai.play_rate,
                    baseline: ai.baseline,
                    amplitude: ai.amplitude,
                    loop_source: ai.loop_enabled,
                    selected: ai.selected,
                },
                Vec::new(),
            )
        })
        .collect()
}

/// A file envelope value as the engine's, for an envelope of `key`: pan
/// −1…1 → 0…1, mute's play flag turned over; everything else as it is.
pub(crate) fn engine_value(key: &crate::sync::EnvelopeKey, value: f64) -> f64 {
    use daw_proto::automation::EnvelopeType;
    match key {
        crate::sync::EnvelopeKey::Track(EnvelopeType::Pan | EnvelopeType::PanPrefx) => {
            (value.clamp(-1.0, 1.0) + 1.0) / 2.0
        }
        crate::sync::EnvelopeKey::Track(EnvelopeType::Mute) => 1.0 - value.clamp(0.0, 1.0),
        _ => value,
    }
}

/// A file point's shape as the engine's.
pub(crate) fn engine_shape(
    shape: dawfile_reaper::types::envelope::EnvelopePointShape,
) -> daw_proto::automation::EnvelopeShape {
    use daw_proto::automation::EnvelopeShape;
    use dawfile_reaper::types::envelope::EnvelopePointShape as PS;
    match shape {
        PS::Square => EnvelopeShape::Square,
        PS::SlowStartEnd => EnvelopeShape::SlowStartEnd,
        PS::FastStart => EnvelopeShape::FastStart,
        PS::FastEnd => EnvelopeShape::FastEnd,
        PS::Bezier => EnvelopeShape::Bezier,
        PS::Linear | PS::Default => EnvelopeShape::Linear,
    }
}

/// Give every automation item in the project its pool's curve: the
/// `<POOLEDENV>` of the id its instance names, in the engine's values for
/// its envelope. An instance whose pool is missing keeps an empty curve
/// (it plays nothing; the envelope plays under it).
fn fill_pools(daw: &Standalone, project_guid: &str, rpp_text: &str) {
    let pools = dawfile_reaper::types::envelope::PooledEnvelope::all_in(rpp_text);
    if pools.is_empty() {
        return;
    }
    let _ = daw.with_project_mut(project_guid, |p| {
        for ((_, key), data) in &mut p.envelopes {
            for (item, curve) in &mut data.automation_items {
                let Some(pool) = pools.iter().find(|pool| pool.id == item.pool_id) else {
                    continue;
                };
                item.name.clone_from(&pool.name);
                *curve = pool
                    .points
                    .iter()
                    .enumerate()
                    .map(|(i, pt)| daw_proto::automation::EnvelopePoint {
                        index: i as u32,
                        time: PositionInSeconds::from_seconds(pt.position),
                        value: engine_value(key, pt.value),
                        shape: engine_shape(pt.shape),
                        tension: pt.bezier_tension.unwrap_or(0.0),
                        selected: false,
                    })
                    .collect();
            }
        }
    });
}

fn populate_routing(
    daw: &Standalone,
    project_guid: &str,
    project: &ReaperProject,
    summary: &mut LoadedProject,
) {
    // Track→track sends are stored in RPP on the DESTINATION track as
    // `AUXRECV <source idx> <mode> <vol> <pan> …` — walk every track's
    // receives and register the send on the source. This is the session's
    // actual mix path (e.g. a folder with MAINSEND 0 sending into its bus:
    // Drums → DRUM BUS → MIX BUS → master); dropping them silences any
    // project mixed through busses.
    let _ = daw.with_project_mut(project_guid, |p| {
        // Snapshot track GUIDs by index for AUXRECV resolution.
        let track_guids: Vec<String> = p.tracks.iter().map(|t| t.guid.clone()).collect();
        let track_names: Vec<String> = p.tracks.iter().map(|t| t.name.clone()).collect();
        for (dest_idx, rt) in project.tracks.iter().enumerate() {
            let Some(dest_guid) = track_guids.get(dest_idx).cloned() else {
                continue;
            };
            for recv in &rt.receives {
                let src_idx = recv.source_track_index;
                if src_idx < 0 {
                    continue;
                }
                let Some(src_guid) = track_guids.get(src_idx as usize).cloned() else {
                    continue;
                };
                let mut route = daw_proto::TrackRoute::default();
                route.route_type = daw_proto::RouteType::Send;
                route.source_track_guid = src_guid.clone();
                route.dest_track_guid = Some(dest_guid.clone());
                route.dest_track_name = track_names.get(dest_idx).cloned();
                route.volume = recv.volume;
                route.pan = recv.pan;
                route.muted = recv.mute;
                route.phase_inverted = recv.invert_polarity;
                route.send_mode = send_mode_from_rpp(recv.mode);
                route.source_channels = channels_from_rpp(recv.source_audio_channels);
                route.dest_channels = channels_from_rpp(recv.dest_audio_channels);
                let sends = p.sends.entry(src_guid).or_default();
                route.index = sends.len() as u32;
                sends.push(route);
            }
        }

        for (src_idx, rt) in project.tracks.iter().enumerate() {
            let Some(src_guid) = track_guids.get(src_idx).cloned() else {
                continue;
            };
            for hw in &rt.hardware_outputs {
                let mut route = daw_proto::TrackRoute::default();
                route.route_type = daw_proto::RouteType::HardwareOutput;
                route.source_track_guid = src_guid.clone();
                route.hw_output_index = Some(hw.output_index as u32);
                route.hw_output_name = Some(format!("HW {}", hw.output_index));
                route.volume = hw.volume;
                route.pan = hw.pan;
                route.muted = hw.mute;
                route.phase_inverted = hw.invert_polarity;
                route.source_channels = daw_proto::ChannelMapping {
                    start_channel: 0,
                    num_channels: rt.channel_count.max(1).min(128),
                };
                let outs = p.hw_outputs.entry(src_guid.clone()).or_default();
                let i = outs.len() as u32;
                route.index = i;
                outs.push(route);
                summary.hw_output_count += 1;
            }
        }
    });
}

/// `AUXRECV` field 2 in the backend's terms.
pub(crate) fn send_mode_from_rpp(mode: i32) -> daw_proto::routing::SendMode {
    match mode {
        1 => daw_proto::routing::SendMode::PreFx,
        3 => daw_proto::routing::SendMode::PostFx, // pre-fader
        _ => daw_proto::routing::SendMode::PostFader,
    }
}

// ────────────────────────────────────────────────────────────────────
// FX chain population
// ────────────────────────────────────────────────────────────────────

/// Walk each track's `<FXCHAIN>` and (in REAPER 7+) `<CONTAINER>`
/// nodes, instantiate the plugin through [`daw_proto::fx::Effects`],
/// and apply state via [`crate::rpp_state`] + [`Standalone::apply_plugin_state`].
///
/// Unsupported nodes (JS scripts, AU, video, plugins whose bundle
/// can't be resolved on this host) are recorded as warnings on
/// [`LoadedProject`] and skipped — the track still loads with its
/// audio + automation intact.
fn populate_fx_chains(
    daw: &Standalone,
    project_guid: &str,
    project: &ReaperProject,
    summary: &mut LoadedProject,
) {
    use daw_proto::fx::FxChainContext;
    use daw_proto::project::ProjectContext;

    // Build (track_guid, fx_chain) pairs. Tracks were added in order
    // so we can correlate by index against the source project.
    let track_guids: Vec<String> = daw
        .read_project(project_guid, |p| {
            p.tracks.iter().map(|t| t.guid.clone()).collect()
        })
        .unwrap_or_default();

    let ctx = ProjectContext::Project(project_guid.to_string());

    for (idx, rt) in project.tracks.iter().enumerate() {
        let Some(track_guid) = track_guids.get(idx).cloned() else {
            continue;
        };
        let Some(fxc) = rt.fx_chain.as_ref() else {
            continue;
        };
        for node in &fxc.nodes {
            apply_fx_node(
                daw,
                &ctx,
                FxChainContext::Track(track_guid.clone()),
                node,
                summary,
            );
        }
    }
}

fn apply_fx_node(
    daw: &Standalone,
    ctx: &daw_proto::project::ProjectContext,
    chain_ctx: daw_proto::fx::FxChainContext,
    node: &dawfile_reaper::types::fx_chain::FxChainNode,
    summary: &mut LoadedProject,
) {
    use dawfile_reaper::types::fx_chain::{FxChainNode, PluginType};
    match node {
        FxChainNode::Plugin(p) => {
            // A built-in FX of the injected factory (the guide's click,
            // count and voice instruments, …) has no bundle on disk for
            // the search below to find: ask the factory first. The
            // `.session` writer stores one as a CLAP node whose plugin id
            // (`file`) is the factory's name — see
            // `session_file::builtin_fx_node`.
            if let Some(name) = builtin_fx_name(daw, p) {
                match daw_proto::fx::Effects::add(daw, ctx.clone(), chain_ctx.clone(), &name) {
                    Some(fx_guid) => {
                        // What it was set to, as the session writer saved
                        // it (`session_file::builtin_fx_node`): a CLAP
                        // block's state.
                        if !p.state_data.is_empty() {
                            match crate::rpp_state::reaper_clap_to_state(&p.state_data)
                                .map_err(|e| e.to_string())
                                .and_then(|blob| {
                                    daw.apply_plugin_state(&fx_guid, &blob)
                                        .map_err(|e| e.to_string())
                                }) {
                                Ok(()) => {}
                                Err(e) => summary.warnings.push(format!(
                                    "FX state not restored for built-in '{name}': {e}"
                                )),
                            }
                        }
                        apply_fx_flags(daw, ctx, &chain_ctx, &fx_guid, p);
                    }
                    None => summary
                        .warnings
                        .push(format!("FX add failed: built-in '{name}'")),
                }
                return;
            }
            // Skip plugin formats we don't host yet (or never will,
            // like Video). JS would need a JSFX engine.
            match p.plugin_type {
                PluginType::Vst3 | PluginType::Clap | PluginType::Vst => {}
                _ => {
                    summary.warnings.push(format!(
                        "FX skipped: '{}' (unsupported format {:?})",
                        p.name, p.plugin_type
                    ));
                    return;
                }
            }
            // Resolve the bundle on disk. RPP stores just the
            // filename ("MUtility.vst3") so we walk standard plugin
            // search paths.
            let Some(path) = resolve_plugin_path(&p.file, &p.plugin_type) else {
                summary.warnings.push(format!(
                    "FX skipped: '{}' (bundle '{}' not found in plugin search paths)",
                    p.name, p.file
                ));
                return;
            };
            // Stand-up the plugin via the existing Effects::add
            // path (which dispatches by file extension into the
            // CLAP / VST3 host).
            let Some(fx_guid) =
                daw_proto::fx::Effects::add(daw, ctx.clone(), chain_ctx.clone(), path.as_str())
            else {
                summary.warnings.push(format!(
                    "FX add failed: '{}' (load_plugin returned no instance)",
                    p.name
                ));
                return;
            };
            // Restore state if the RPP carried any.
            if !p.state_data.is_empty() {
                let decode = match p.plugin_type {
                    PluginType::Clap => crate::rpp_state::reaper_clap_to_state(&p.state_data),
                    _ => crate::rpp_state::reaper_vst3_to_daw_state(&p.state_data),
                };
                match decode {
                    Ok(blob) => {
                        if let Err(e) = daw.apply_plugin_state(&fx_guid, &blob) {
                            summary
                                .warnings
                                .push(format!("FX state apply failed for '{}': {e}", p.name));
                        }
                    }
                    Err(e) => summary
                        .warnings
                        .push(format!("FX state decode failed for '{}': {e}", p.name)),
                }
            }
            apply_fx_flags(daw, ctx, &chain_ctx, &fx_guid, p);
        }
        FxChainNode::Container(c) => {
            // REAPER 7 FX containers. The proto layer doesn't have a
            // first-class container yet — flatten children into the
            // parent chain. State + routing within the container is
            // lost; record a warning so users know.
            summary.warnings.push(format!(
                "FX container '{}' flattened (REAPER 7 container layout not yet modeled)",
                c.name
            ));
            for child in &c.children {
                apply_fx_node(daw, ctx, chain_ctx.clone(), child, summary);
            }
        }
    }
}

/// The factory name of a built-in FX node, when the injected
/// [`FxFactory`](crate::plugin::FxFactory) provides one: the plugin id
/// (`file`) first — what the `.session` writer stores — then the display
/// name.
fn builtin_fx_name(
    daw: &Standalone,
    p: &dawfile_reaper::types::fx_chain::FxPlugin,
) -> Option<String> {
    let factory = daw.fx_factory()?;
    [p.file.as_str(), p.name.as_str()]
        .into_iter()
        .find(|name| !name.is_empty() && factory.provides(name))
        .map(str::to_string)
}

/// REAPER's `BYPASS <bypassed> <offline>` on a freshly added FX.
/// `Effects::add` starts enabled and online, so only a flip is written.
fn apply_fx_flags(
    daw: &Standalone,
    ctx: &daw_proto::project::ProjectContext,
    chain_ctx: &daw_proto::fx::FxChainContext,
    fx_guid: &str,
    p: &dawfile_reaper::types::fx_chain::FxPlugin,
) {
    let target = || daw_proto::fx::FxTarget {
        context: chain_ctx.clone(),
        fx: daw_proto::fx::FxRef::Guid(fx_guid.to_string()),
    };
    if p.bypassed {
        let _ = daw_proto::fx::Effects::set_enabled(daw, ctx.clone(), target(), false);
    }
    if p.offline {
        let _ = daw_proto::fx::Effects::set_offline(daw, ctx.clone(), target(), true);
    }
    // Its parameters' automation (`<PARMENV>`), as the engine's envelopes.
    if let (
        daw_proto::fx::FxChainContext::Track(track),
        daw_proto::project::ProjectContext::Project(project),
    ) = (chain_ctx, ctx)
        && !p.param_envelopes.is_empty()
    {
        let envelopes: Vec<(crate::sync::EnvelopeKey, crate::sync::EnvelopeData)> = p
            .param_envelopes
            .iter()
            .map(|env| {
                (
                    crate::sync::EnvelopeKey::FxParam {
                        fx_guid: fx_guid.to_owned(),
                        param_index: env.param.index,
                    },
                    param_envelope(env),
                )
            })
            .collect();
        let _ = daw.with_project_mut(project, |state| {
            for (key, data) in envelopes {
                state.envelopes.insert((track.clone(), key), data);
            }
        });
    }
}

/// A `<PARMENV>` as the engine's envelope: its points (seconds, 0..1, the
/// shape REAPER numbers 0–5), its mode (REAPER's 0–5; off when not active),
/// shown and armed as it was.
fn param_envelope(
    env: &dawfile_reaper::types::fx_chain::FxParamEnvelope,
) -> crate::sync::EnvelopeData {
    use daw_proto::automation::{EnvelopePoint, EnvelopeShape};
    use daw_proto::primitives::AutomationMode;
    let shape = |flags: &[f64]| match flags.first().map(|f| f.round() as i64) {
        Some(1) => EnvelopeShape::Square,
        Some(2) => EnvelopeShape::SlowStartEnd,
        Some(3) => EnvelopeShape::FastStart,
        Some(4) => EnvelopeShape::FastEnd,
        Some(5) => EnvelopeShape::Bezier,
        _ => EnvelopeShape::Linear,
    };
    let mut data = crate::sync::EnvelopeData::new();
    data.visible = env.visible;
    data.armed = env.armed;
    data.automation_mode = if env.active {
        match env.mode {
            1 => AutomationMode::Read,
            2 => AutomationMode::Touch,
            3 => AutomationMode::Write,
            4 => AutomationMode::Latch,
            5 => AutomationMode::LatchPreview,
            _ => AutomationMode::TrimRead,
        }
    } else {
        AutomationMode::Off
    };
    data.points = env
        .points
        .iter()
        .enumerate()
        .map(|(i, pt)| EnvelopePoint {
            index: u32::try_from(i).unwrap_or(u32::MAX),
            time: daw_proto::primitives::PositionInSeconds::from_seconds(pt.time),
            value: pt.value,
            shape: shape(&pt.flags),
            tension: pt.flags.get(4).copied().unwrap_or(0.0),
            selected: false,
        })
        .collect();
    data.automation_items = automation_items(&env.automation_items);
    data
}

/// Find a plugin bundle on disk given just a filename. Walks the
/// usual VST3 / CLAP search dirs:
///
/// - `$HOME/.vst3`, `/usr/lib/vst3`, `/usr/local/lib/vst3` (Linux)
/// - `~/Library/Audio/Plug-Ins/VST3`, `/Library/Audio/Plug-Ins/VST3` (macOS)
/// - `$HOME/.clap`, `/usr/lib/clap`, `/usr/local/lib/clap` (Linux)
/// - `~/Library/Audio/Plug-Ins/CLAP`, `/Library/Audio/Plug-Ins/CLAP` (macOS)
/// - the same pattern for VST2 (`.vst`, `/usr/lib/vst`, `Plug-Ins/VST`)
///
/// Each root is searched directly, then through vendor folders (see
/// [`PLUGIN_FOLDER_DEPTH`]).
///
/// Returns the absolute path if found. Bare filenames in `.rpp`
/// files are how REAPER refers to plugins; the host resolves them
/// against the same dirs the OS DAW would.
fn resolve_plugin_path(
    filename: &str,
    plugin_type: &dawfile_reaper::types::fx_chain::PluginType,
) -> Option<String> {
    use dawfile_reaper::types::fx_chain::PluginType;
    use std::path::PathBuf;

    // If the file is already an absolute path that exists, take it.
    let direct = PathBuf::from(filename);
    if direct.is_absolute() && direct.exists() {
        return Some(filename.to_string());
    }

    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut roots: Vec<PathBuf> = Vec::new();
    match plugin_type {
        PluginType::Vst3 => {
            if let Some(h) = &home {
                roots.push(h.join(".vst3"));
                #[cfg(target_os = "macos")]
                roots.push(h.join("Library/Audio/Plug-Ins/VST3"));
            }
            roots.push(PathBuf::from("/usr/lib/vst3"));
            roots.push(PathBuf::from("/usr/local/lib/vst3"));
            #[cfg(target_os = "macos")]
            roots.push(PathBuf::from("/Library/Audio/Plug-Ins/VST3"));
        }
        PluginType::Clap => {
            if let Some(h) = &home {
                roots.push(h.join(".clap"));
                #[cfg(target_os = "macos")]
                roots.push(h.join("Library/Audio/Plug-Ins/CLAP"));
            }
            roots.push(PathBuf::from("/usr/lib/clap"));
            roots.push(PathBuf::from("/usr/local/lib/clap"));
            #[cfg(target_os = "macos")]
            roots.push(PathBuf::from("/Library/Audio/Plug-Ins/CLAP"));
        }
        PluginType::Vst => {
            if let Some(h) = &home {
                roots.push(h.join(".vst"));
                #[cfg(target_os = "macos")]
                roots.push(h.join("Library/Audio/Plug-Ins/VST"));
            }
            roots.push(PathBuf::from("/usr/lib/vst"));
            roots.push(PathBuf::from("/usr/local/lib/vst"));
            #[cfg(target_os = "macos")]
            roots.push(PathBuf::from("/Library/Audio/Plug-Ins/VST"));
        }
        _ => return None,
    }

    // Direct hits first, then vendor folders.
    for depth in [0, PLUGIN_FOLDER_DEPTH] {
        for root in &roots {
            if let Some(path) = find_plugin_in(root, filename, depth) {
                return path.to_str().map(|s| s.to_string());
            }
        }
    }
    None
}

/// How deep [`find_plugin_in`] descends below a search root. macOS
/// installers nest by vendor and category — Universal Audio ships
/// `Plug-Ins/VST3/Universal Audio/Compressors and Limiters/Foo.vst3`.
const PLUGIN_FOLDER_DEPTH: usize = 3;

/// `dir/filename`, or the same inside a subfolder up to `depth` levels
/// down. A plugin bundle is itself a directory, so anything with an
/// extension is a plugin, not a folder, and is not descended into.
fn find_plugin_in(
    dir: &std::path::Path,
    filename: &str,
    depth: usize,
) -> Option<std::path::PathBuf> {
    let candidate = dir.join(filename);
    if candidate.exists() {
        return Some(candidate);
    }
    if depth == 0 {
        return None;
    }
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|sub| sub.is_dir() && sub.extension().is_none())
        .find_map(|sub| find_plugin_in(&sub, filename, depth - 1))
}

/// Per-track mixer widths, from the project's `<EXTSTATE>` block.
///
/// REAPER has no strip width, so there is no field to read: it goes
/// where REAPER keeps everything it does not model, which is the
/// project-level extension block. The shape follows the one FTS already
/// writes there — a named sub-block, then `KEY guid=value` lines:
///
/// ```text
///   <EXTSTATE
///     <FTSMCP
///       WIDTHS {GUID}=120 {GUID}=44
///     >
///   >
/// ```
///
/// A line scan rather than a parse, because the typed project model does
/// not carry `<EXTSTATE>` and growing it one for a single key would be
/// a larger change than the feature. Anything unrecognised is ignored:
/// an extension block is by definition full of other people's data.
pub(crate) fn mcp_widths(rpp_text: &str) -> std::collections::HashMap<String, u32> {
    let mut widths = std::collections::HashMap::new();
    let mut in_ext = false;
    let mut in_ours = false;
    for line in rpp_text.lines() {
        let line = line.trim();
        if line.starts_with("<EXTSTATE") {
            in_ext = true;
        } else if in_ext && line.starts_with("<FTSMCP") {
            in_ours = true;
        } else if line == ">" {
            // Closes whichever is innermost.
            if in_ours {
                in_ours = false;
            } else {
                in_ext = false;
            }
        } else if in_ours {
            if let Some(rest) = line.strip_prefix("WIDTHS ") {
                for pair in rest.split_whitespace() {
                    if let Some((guid, px)) = pair.split_once('=')
                        && let Ok(px) = px.parse()
                    {
                        widths.insert(guid.to_string(), px);
                    }
                }
            }
        }
    }
    widths
}

/// REAPER's `I_RECINPUT`, decoded.
///
/// One integer carrying three different things, which is why it is here
/// rather than inline:
///
/// ```text
///   < 0            nothing selected
///   0..1024        a mono hardware input, by index
///   1024 + n       the stereo pair starting at input n
///   4096 + d*32+c  MIDI: device d, channel c, where c = 0 is "all
///                  channels" and a device of 63 is "all devices"
/// ```
///
/// The standalone loader used to report `None` for every track whatever
/// the file said, so a strip's input field read "No input" on a track
/// that was plainly recording something.
pub(crate) fn record_input_from_rpp(raw: i32) -> daw_proto::track::RecordInput {
    use daw_proto::track::RecordInput;

    if raw < 0 {
        return RecordInput::None;
    }
    if raw >= 4096 {
        let bits = raw - 4096;
        let device = bits >> 5;
        let channel = bits & 0x1F;
        return RecordInput::Midi {
            // 63 is REAPER's "all devices"; a channel of zero is "all
            // channels", and the rest are counted from one in the file
            // and from zero in the model.
            device_id: (device != 63).then(|| device.clamp(0, 255) as u8),
            channel: (channel != 0).then(|| (channel - 1).clamp(0, 255) as u8),
        };
    }
    // A stereo pair is named by the input it starts on, which is the
    // same number a mono input would use — the 1024 only says how many
    // channels follow it.
    let channel = if raw >= 1024 { raw - 1024 } else { raw };
    RecordInput::Audio {
        channel: channel.max(0) as u32,
    }
}

#[cfg(test)]
mod plugin_search_tests {
    use super::find_plugin_in;

    #[test]
    fn finds_a_plugin_in_nested_vendor_folders_but_not_inside_bundles() {
        let root = tempfile::tempdir().unwrap();
        let nested = root.path().join("Universal Audio/Compressors and Limiters");
        std::fs::create_dir_all(nested.join("UAD API 2500.vst3/Contents")).unwrap();
        // A same-named file inside another bundle must not be found.
        std::fs::create_dir_all(root.path().join("Other.vst3/Hidden.vst3")).unwrap();

        let hit = find_plugin_in(root.path(), "UAD API 2500.vst3", 3).unwrap();
        assert_eq!(hit, nested.join("UAD API 2500.vst3"));
        assert!(find_plugin_in(root.path(), "UAD API 2500.vst3", 1).is_none());
        assert!(find_plugin_in(root.path(), "Hidden.vst3", 3).is_none());
    }
}

/// A lane number as the `.rpp` writes it (1-based; 0 or absent = no lane)
/// to the 0-based index REAPER's API — and so this backend — uses.
pub(crate) fn file_lane_to_api(lane: Option<i32>) -> Option<u32> {
    lane.filter(|l| *l >= 1).map(|l| (l - 1) as u32)
}

#[cfg(test)]
mod anchor_tests {
    use super::{anchor_media, load_rpp_text};
    use crate::sync::Standalone;

    const PROJECT: &str = r#"<REAPER_PROJECT 0.1 "7.0/test" 0
  <TRACK {00000000-0000-0000-0000-000000000001}
    NAME Click
    <ITEM
      POSITION 0
      LENGTH 4
      IGUID {00000000-0000-0000-0000-00000000000A}
      <SOURCE WAVE
        FILE "Media/Click.wav"
      >
    >
  >
>
"#;

    /// Two songs with the same relative media name point at their own
    /// folders once anchored — and an absolute path is left alone.
    #[test]
    fn each_project_s_media_is_anchored_to_its_own_folder() {
        let daw = Standalone::new();
        let one = load_rpp_text(&daw, "One", "/set/One/One.RPP", PROJECT).unwrap();
        let two = load_rpp_text(&daw, "Two", "/set/Two/Two.RPP", PROJECT).unwrap();
        assert_eq!(
            anchor_media(&daw, &one.project_guid, std::path::Path::new("/set/One")),
            1
        );
        assert_eq!(
            anchor_media(&daw, &two.project_guid, std::path::Path::new("/set/Two")),
            1
        );
        let path = |guid: &str| {
            daw.read_project(guid, |p| {
                p.takes
                    .values()
                    .flat_map(|l| l.takes.iter())
                    .find_map(|t| t.source_file_path.clone())
            })
            .flatten()
        };
        assert_eq!(
            path(&one.project_guid).as_deref(),
            Some("/set/One/Media/Click.wav")
        );
        assert_eq!(
            path(&two.project_guid).as_deref(),
            Some("/set/Two/Media/Click.wav")
        );
        assert_eq!(
            anchor_media(&daw, &one.project_guid, std::path::Path::new("/elsewhere")),
            0
        );
    }

    /// A folder given relative to the working directory still anchors to
    /// an absolute path.
    #[test]
    fn a_relative_folder_anchors_absolutely() {
        let daw = Standalone::new();
        let song = load_rpp_text(&daw, "One", "../set/One/One.RPP", PROJECT).unwrap();
        anchor_media(&daw, &song.project_guid, std::path::Path::new("../set/One"));
        let path = daw
            .read_project(&song.project_guid, |p| {
                p.takes
                    .values()
                    .flat_map(|l| l.takes.iter())
                    .find_map(|t| t.source_file_path.clone())
            })
            .flatten()
            .expect("a path");
        assert!(std::path::Path::new(&path).is_absolute(), "{path}");
        assert!(path.ends_with("set/One/Media/Click.wav"), "{path}");
    }
}

#[cfg(test)]
mod negative_length_tests {
    use super::load_rpp_text;
    use crate::sync::Standalone;

    /// REAPER saves the odd item with a hair of negative length, and fades
    /// to match (a real session, Crescendum's "it knows my name", has two).
    /// It opens: the item is empty, not a panic.
    #[test]
    fn an_item_saved_with_a_negative_length_opens_empty() {
        const PROJECT: &str = r#"<REAPER_PROJECT 0.1 "7.0/test" 0
  <TRACK {00000000-0000-0000-0000-000000000001}
    NAME Vocals
    <ITEM
      POSITION 12
      LENGTH -0.00316666666667
      FADEIN 1 -0.00316666666667 0 1 0 0 0
      FADEOUT 1 -0.00316666666667 0 1 0 0 0
      IGUID {00000000-0000-0000-0000-00000000000A}
      <SOURCE WAVE
        FILE "Media/Vocals.wav"
      >
    >
  >
>
"#;
        let daw = Standalone::new();
        let loaded = load_rpp_text(&daw, "Song", "/set/Song/Song.RPP", PROJECT).expect("it opens");
        let lengths = daw
            .read_project(&loaded.project_guid, |p| {
                p.items
                    .values()
                    .map(|e| {
                        (
                            e.item.length.as_seconds(),
                            e.item.fade_in_length.as_seconds(),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .expect("the project");
        assert_eq!(lengths, vec![(0.0, 0.0)]);
    }
}

#[cfg(test)]
mod color_tests {
    use super::custom_color;

    /// Only a flagged field is a colour: REAPER's "none" (16576) sets
    /// nothing; a flagged one — written positive or negative — is its
    /// colour, `0xBBGGRR` read as `0xRRGGBB`.
    #[test]
    fn only_a_flagged_colour_field_is_a_colour() {
        assert_eq!(custom_color(16576), None);
        assert_eq!(custom_color(0), None);
        assert_eq!(custom_color(-12_566_400), Some(0x80_40_40));
        assert_eq!(custom_color(25_184_576), Some(0x40_49_80));
    }
}

/// A receive's channel field as REAPER writes it: the first channel
/// (0 = 1/2, 2 = 3/4), `| 1024` for mono, more pairs above bit 10 —
/// `-1` for none, read as the default pair.
pub(crate) fn channels_from_rpp(field: i32) -> daw_proto::ChannelMapping {
    let Ok(field) = u32::try_from(field) else {
        return daw_proto::ChannelMapping::default();
    };
    let pairs = (field >> 11) & 0x1f;
    daw_proto::ChannelMapping {
        start_channel: field & 0x3ff,
        num_channels: if field & 1024 != 0 {
            1
        } else {
            2 * pairs.max(1)
        },
    }
}

/// [`channels_from_rpp`] the other way: the field REAPER writes for
/// `channels` — the default pair (none named) is `0`.
pub(crate) fn channels_to_rpp(channels: &daw_proto::ChannelMapping) -> i32 {
    let start = i32::try_from(channels.start_channel & 0x3ff).unwrap_or(0);
    match channels.num_channels {
        1 => start | 1024,
        0 | 2 => start,
        n => start | (i32::try_from((n / 2) & 0x1f).unwrap_or(1) << 11),
    }
}

#[cfg(test)]
mod channel_tests {
    use super::channels_from_rpp;

    /// REAPER's receive channel fields: 0 the main pair, 2 a
    /// sidechain on 3/4 (`AUXRECV 10 1 1 0 0 0 0 0 2` — a kick's
    /// trigger keying its gate), mono flagged, none.
    #[test]
    fn a_receive_lands_on_the_channels_it_names() {
        let at = |field| {
            let c = channels_from_rpp(field);
            (c.start_channel, c.num_channels)
        };
        assert_eq!(at(0), (0, 2));
        assert_eq!(at(2), (2, 2));
        assert_eq!(at(1024 | 3), (3, 1));
        assert_eq!(at(-1), (0, 0));
        for field in [0, 2, 1024 | 3] {
            assert_eq!(
                super::channels_to_rpp(&channels_from_rpp(field)),
                field,
                "{field} round trips"
            );
        }
    }
}
