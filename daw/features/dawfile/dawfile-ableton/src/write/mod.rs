//! Write support for Ableton Live set files.
//!
//! Generates .als files (gzipped XML) from `AbletonLiveSet` types.
//! The output targets Ableton Live 12 format and is compatible with Live 11+.
//!
//! # Approach
//!
//! This writer generates complete .als XML from our domain types rather than
//! doing round-trip modification. This means:
//! - You can create .als files from any source (Pro Tools import, REAPER, etc.)
//! - Unknown/unsupported Ableton elements are not preserved
//! - Ableton fills in defaults for any missing elements when it opens the file
//!
//! # Usage
//!
//! ```no_run
//! use dawfile_ableton::write::write_live_set;
//!
//! let set = dawfile_ableton::read_live_set("input.als").unwrap();
//! // ... modify set ...
//! write_live_set(&set, "output.als").unwrap();
//! ```

pub mod xml_writer;

use crate::devices;
use crate::error::{AbletonError, AbletonResult};
use crate::types::*;
use flate2::Compression;
use flate2::write::GzEncoder;
use std::io::Write;
use std::path::Path;
use xml_writer::AbletonXmlWriter;

/// Write an Ableton Live set to a file (.als).
pub fn write_live_set(set: &AbletonLiveSet, path: impl AsRef<Path>) -> AbletonResult<()> {
    let data = write_live_set_bytes(set)?;
    std::fs::write(path.as_ref(), data)?;
    Ok(())
}

/// Serialize an Ableton Live set to gzipped XML bytes.
pub fn write_live_set_bytes(set: &AbletonLiveSet) -> AbletonResult<Vec<u8>> {
    let xml = serialize_to_xml(set)?;

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(xml.as_bytes())
        .map_err(AbletonError::Io)?;
    encoder.finish().map_err(AbletonError::Io)
}

/// Serialize to raw XML string (useful for debugging).
pub fn serialize_to_xml(set: &AbletonLiveSet) -> AbletonResult<String> {
    let mut buf = Vec::new();
    let mut w = AbletonXmlWriter::new(&mut buf);

    w.write_declaration()
        .map_err(|e| AbletonError::Xml(e.to_string()))?;

    write_root(&mut w, set).map_err(|e| AbletonError::Xml(e.to_string()))?;

    drop(w);
    let xml = String::from_utf8(buf).map_err(|e| AbletonError::Xml(e.to_string()))?;
    // Ableton expects a trailing newline
    Ok(xml + "\n")
}

fn write_root(w: &mut AbletonXmlWriter<&mut Vec<u8>>, set: &AbletonLiveSet) -> std::io::Result<()> {
    let minor_version = format!(
        "{}.{}.{}",
        set.version.major, set.version.minor, set.version.patch
    );
    let creator = if set.version.creator.is_empty() {
        format!("Ableton Live {}.{}", set.version.major, set.version.minor)
    } else {
        set.version.creator.clone()
    };

    w.start_with_attrs(
        "Ableton",
        &[
            ("MajorVersion", "5"),
            ("MinorVersion", &minor_version),
            ("SchemaChangeCount", "3"),
            ("Creator", &creator),
            ("Revision", ""),
        ],
    )?;

    w.start("LiveSet")?;
    write_tracks(w, set)?;
    write_master_track(w, set)?;
    write_pre_hear_track(w, set)?;
    write_locators(w, &set.locators)?;
    write_scenes(w, &set.scenes)?;
    write_transport(w, &set.transport)?;
    write_groove_pool(w, &set.groove_pool)?;
    if let Some(ref tuning) = set.tuning_system {
        write_tuning_system(w, tuning)?;
    }
    w.end("LiveSet")?;

    w.end("Ableton")
}

fn write_tracks(
    w: &mut AbletonXmlWriter<&mut Vec<u8>>,
    set: &AbletonLiveSet,
) -> std::io::Result<()> {
    let num_scenes = set.scenes.len();

    w.start("Tracks")?;

    for track in &set.audio_tracks {
        write_audio_track(w, track, num_scenes)?;
    }
    for track in &set.midi_tracks {
        write_midi_track(w, track, num_scenes)?;
    }
    for track in &set.group_tracks {
        write_group_track(w, track)?;
    }

    w.end("Tracks")?;

    // Return tracks are a separate element
    w.start("ReturnTracks")?;
    for track in &set.return_tracks {
        write_return_track(w, track)?;
    }
    w.end("ReturnTracks")
}

fn write_track_common(
    w: &mut AbletonXmlWriter<&mut Vec<u8>>,
    common: &TrackCommon,
) -> std::io::Result<()> {
    w.value_int("LomId", 0)?;
    w.value_int("LomIdView", 0)?;

    w.start("Name")?;
    w.value_element("EffectiveName", &common.effective_name)?;
    w.value_element("UserName", &common.user_name)?;
    w.value_element("Annotation", &common.annotation)?;
    w.value_element("MemorizedFirstClipName", &common.memorized_first_clip_name)?;
    w.end("Name")?;

    w.value_int("Color", common.color as i64)?;
    w.value_int("TrackGroupId", common.group_id as i64)?;
    w.value_int("LinkedTrackGroupId", common.linked_track_group_id as i64)?;
    w.value_bool("TrackUnfolded", !common.folded)?;

    // Track delay
    if let Some(ref delay) = common.track_delay {
        w.start("TrackDelay")?;
        w.start("Value")?;
        w.value_float("Manual", delay.value)?;
        w.end("Value")?;
        w.start("IsValueSampleBased")?;
        w.value_bool("Manual", delay.is_sample_based)?;
        w.end("IsValueSampleBased")?;
        w.end("TrackDelay")?;
    }

    // View state
    if let Some(ref vs) = common.view_state
        && let Some(ref json) = vs.view_data_json
    {
        w.text_element("ViewData", json)?;
    }

    Ok(())
}

fn write_device_chain_start(
    w: &mut AbletonXmlWriter<&mut Vec<u8>>,
    mixer: &MixerState,
    view_state: Option<&TrackViewState>,
) -> std::io::Result<()> {
    w.start("DeviceChain")?;
    write_mixer(w, mixer, view_state)?;
    Ok(())
}

fn write_devices(
    w: &mut AbletonXmlWriter<&mut Vec<u8>>,
    device_list: &[Device],
) -> std::io::Result<()> {
    if device_list.is_empty() {
        return Ok(());
    }
    w.start("Devices")?;
    for dev in device_list {
        match dev.format {
            DeviceFormat::Vst2 | DeviceFormat::Vst3 | DeviceFormat::AudioUnit => {
                write_plugin_device(w, dev)?;
            }
            DeviceFormat::MaxForLive => {
                write_m4l_device(w, dev)?;
            }
            DeviceFormat::NoteAlgorithm => {
                write_note_algorithm_device(w, dev)?;
            }
            DeviceFormat::Builtin | DeviceFormat::Unknown => {
                if let Some(ref params) = dev.builtin_params {
                    write_builtin_device(w, dev, params)?;
                }
                // Unknown devices without builtin_params are dropped
                // (we don't have enough info to reconstruct them)
            }
        }
    }
    w.end("Devices")
}

fn write_builtin_device(
    w: &mut AbletonXmlWriter<&mut Vec<u8>>,
    dev: &Device,
    params: &devices::BuiltinParams,
) -> std::io::Result<()> {
    let tag = builtin_params_tag(params);
    let id = w.next_id();
    w.start_with_id(tag, id)?;
    w.value_int("LomId", 0)?;
    w.value_int("LomIdView", 0)?;
    w.value_bool("IsExpanded", true)?;

    w.start("On")?;
    w.value_bool("Manual", dev.is_on)?;
    w.automation_target("AutomationTarget")?;
    w.end("On")?;

    devices::write_builtin_params(w, params)?;

    w.end(tag)
}

fn write_plugin_device(
    w: &mut AbletonXmlWriter<&mut Vec<u8>>,
    dev: &Device,
) -> std::io::Result<()> {
    let id = w.next_id();
    w.start_with_id("PluginDevice", id)?;
    w.value_int("LomId", 0)?;
    w.value_int("LomIdView", 0)?;
    w.value_bool("IsExpanded", true)?;

    w.start("On")?;
    w.value_bool("Manual", dev.is_on)?;
    w.automation_target("AutomationTarget")?;
    w.end("On")?;

    // PluginDesc
    w.start("PluginDesc")?;
    match dev.format {
        DeviceFormat::Vst3 => {
            w.start_with_id("Vst3PluginInfo", 0)?;
            w.value_element("Name", &dev.name)?;
            w.start("Preset")?;
            w.start_with_id("Vst3Preset", 0)?;
            if let Some(ref ps) = dev.processor_state {
                w.start("ProcessorState")?;
                w.write_raw(ps)?;
                w.end("ProcessorState")?;
            }
            if let Some(ref cs) = dev.controller_state {
                w.start("ControllerState")?;
                w.write_raw(cs)?;
                w.end("ControllerState")?;
            }
            w.end("Vst3Preset")?;
            w.end("Preset")?;
            w.end("Vst3PluginInfo")?;
        }
        DeviceFormat::Vst2 => {
            w.start_with_id("VstPluginInfo", 0)?;
            w.value_element("PlugName", &dev.name)?;
            w.start("Preset")?;
            w.start_with_id("VstPreset", 0)?;
            if let Some(ref ps) = dev.processor_state {
                w.start("Buffer")?;
                w.write_raw(ps)?;
                w.end("Buffer")?;
            }
            w.end("VstPreset")?;
            w.end("Preset")?;
            w.end("VstPluginInfo")?;
        }
        DeviceFormat::AudioUnit => {
            w.start_with_id("AuPluginInfo", 0)?;
            w.value_element("Name", &dev.name)?;
            w.end("AuPluginInfo")?;
        }
        _ => {}
    }
    w.end("PluginDesc")?;

    // SourceContext with device ID
    if let Some(ref device_id) = dev.device_id {
        w.start("SourceContext")?;
        w.start("Value")?;
        w.start_with_id("BranchSourceContext", 0)?;
        w.value_element("BranchDeviceId", device_id)?;
        w.end("BranchSourceContext")?;
        w.end("Value")?;
        w.end("SourceContext")?;
    } else {
        w.empty("SourceContext")?;
    }

    w.end("PluginDevice")
}

fn write_m4l_device(w: &mut AbletonXmlWriter<&mut Vec<u8>>, dev: &Device) -> std::io::Result<()> {
    // Default to MxDeviceAudioEffect since we don't store the original tag
    let tag = "MxDeviceAudioEffect";
    let id = w.next_id();
    w.start_with_id(tag, id)?;
    w.value_int("LomId", 0)?;
    w.value_int("LomIdView", 0)?;
    w.value_bool("IsExpanded", true)?;

    w.start("On")?;
    w.value_bool("Manual", dev.is_on)?;
    w.automation_target("AutomationTarget")?;
    w.end("On")?;

    w.start("Name")?;
    w.value_element("UserName", &dev.name)?;
    w.value_element("EffectiveName", &dev.name)?;
    w.end("Name")?;

    w.end(tag)
}

fn write_note_algorithm_device(
    w: &mut AbletonXmlWriter<&mut Vec<u8>>,
    dev: &Device,
) -> std::io::Result<()> {
    // The `name` field stores the XML class name (e.g. "NoteTransformArpeggiate")
    let tag = &dev.name;
    let id = w.next_id();
    w.start_with_id(tag, id)?;
    w.value_int("LomId", 0)?;
    w.value_int("LomIdView", 0)?;
    w.value_bool("IsExpanded", true)?;

    w.start("On")?;
    w.value_bool("Manual", dev.is_on)?;
    w.automation_target("AutomationTarget")?;
    w.end("On")?;

    w.end(tag)
}

/// Map a [`BuiltinParams`] variant back to the Ableton XML tag name.
fn builtin_params_tag(params: &devices::BuiltinParams) -> &'static str {
    use devices::BuiltinParams::*;
    match params {
        Eq8(_) => "Eq8",
        Compressor(_) => "Compressor2",
        GlueCompressor(_) => "GlueCompressor",
        Gate(_) => "Gate",
        Limiter(_) => "Limiter",
        MultibandDynamics(_) => "MultibandDynamics",
        Reverb(_) => "Reverb",
        Delay(_) => "Delay",
        Echo(_) => "Echo",
        AutoFilter(_) => "AutoFilter",
        Chorus(_) => "Chorus2",
        ChorusLegacy(_) => "Chorus",
        Phaser(_) => "Phaser",
        PhaserNew(_) => "PhaserNew",
        Flanger(_) => "Flanger",
        Saturator(_) => "Saturator",
        Utility(_) => "StereoGain",
        Tuner(_) => "Tuner",
        Cabinet(_) => "Cabinet",
        Erosion(_) => "Erosion",
        Redux(_) => "Redux2",
        ReduxLegacy(_) => "Redux",
        Vinyl(_) => "Vinyl",
        // Amp & distortion
        Amp(_) => "Amp",
        Overdrive(_) => "Overdrive",
        Pedal(_) => "Pedal",
        DrumBuss(_) => "DrumBuss",
        Tube(_) => "Tube",
        Roar(_) => "Roar",
        // Filter & frequency domain
        ChannelEq(_) => "ChannelEq",
        FilterEq3(_) => "FilterEQ3",
        AutoPan(_) => "AutoPan",
        FrequencyShifter(_) => "FrequencyShifter",
        Shifter(_) => "Shifter",
        Spectral(_) => "Spectral",
        Transmute(_) => "Transmute",
        // Creative
        BeatRepeat(_) => "BeatRepeat",
        Corpus(_) => "Corpus",
        Resonator(_) => "Resonator",
        Vocoder(_) => "Vocoder",
        Looper(_) => "Looper",
        GrainDelay(_) => "GrainDelay",
        FilterDelay(_) => "FilterDelay",
        Hybrid(_) => "Hybrid",
        // Instruments
        Simpler(_) => "OriginalSimpler",
        Sampler(_) => "MultiSampler",
        Operator(_) => "Operator",
        Drift(_) => "Drift",
        Wavetable(_) => "InstrumentVector",
        Meld(_) => "InstrumentMeld",
        Collision(_) => "Collision",
        Tension(_) => "StringStudio",
        Electric(_) => "LoungeLizard",
        Analog(_) => "UltraAnalog",
        Impulse(_) => "InstrumentImpulse",
        // Racks
        DrumRack(_) => "DrumGroupDevice",
        InstrumentRack(_) => "InstrumentGroupDevice",
        AudioEffectRack(_) => "AudioEffectGroupDevice",
    }
}

fn write_mixer(
    w: &mut AbletonXmlWriter<&mut Vec<u8>>,
    mixer: &MixerState,
    view_state: Option<&TrackViewState>,
) -> std::io::Result<()> {
    w.start("Mixer")?;
    w.value_int("LomId", 0)?;
    w.value_int("LomIdView", 0)?;
    w.value_bool("IsExpanded", true)?;

    w.start("On")?;
    w.value_bool("Manual", true)?;
    w.automation_target("AutomationTarget")?;
    w.end("On")?;

    // Volume
    w.start("Volume")?;
    w.value_float("Manual", mixer.volume)?;
    w.automation_target("AutomationTarget")?;
    w.start("MidiControllerRange")?;
    w.value_float("Min", 0.0003162277660168)?;
    w.value_float("Max", 1.99526231496888)?;
    w.end("MidiControllerRange")?;
    w.end("Volume")?;

    // Pan
    w.start("Pan")?;
    w.value_float("Manual", mixer.pan)?;
    w.automation_target("AutomationTarget")?;
    w.start("MidiControllerRange")?;
    w.value_float("Min", -1.0)?;
    w.value_float("Max", 1.0)?;
    w.end("MidiControllerRange")?;
    w.end("Pan")?;

    // Split stereo pan (if present)
    if let Some(pan_l) = mixer.split_stereo_pan_l {
        w.start("SplitStereoPanL")?;
        w.value_float("Manual", pan_l)?;
        w.automation_target("AutomationTarget")?;
        w.start("MidiControllerRange")?;
        w.value_float("Min", -1.0)?;
        w.value_float("Max", 1.0)?;
        w.end("MidiControllerRange")?;
        w.end("SplitStereoPanL")?;
    }
    if let Some(pan_r) = mixer.split_stereo_pan_r {
        w.start("SplitStereoPanR")?;
        w.value_float("Manual", pan_r)?;
        w.automation_target("AutomationTarget")?;
        w.start("MidiControllerRange")?;
        w.value_float("Min", -1.0)?;
        w.value_float("Max", 1.0)?;
        w.end("MidiControllerRange")?;
        w.end("SplitStereoPanR")?;
    }

    // Pan mode
    w.start("PanMode")?;
    w.value_int("Manual", mixer.pan_mode as i64)?;
    w.end("PanMode")?;

    // Speaker
    w.start("Speaker")?;
    w.value_bool("Manual", mixer.speaker_on)?;
    w.automation_target("AutomationTarget")?;
    w.end("Speaker")?;

    // CrossFadeState
    w.start("CrossFadeState")?;
    w.value_int("Manual", mixer.crossfade_state as i64)?;
    w.automation_target("AutomationTarget")?;
    w.end("CrossFadeState")?;

    // Sends
    w.start("Sends")?;
    for (i, send) in mixer.sends.iter().enumerate() {
        w.start_with_id("TrackSendHolder", i as i32)?;
        w.start("Send")?;
        w.value_float("Manual", send.level)?;
        w.automation_target("AutomationTarget")?;
        w.end("Send")?;
        w.start("Active")?;
        w.value_bool("Manual", send.enabled)?;
        w.end("Active")?;
        w.end("TrackSendHolder")?;
    }
    w.end("Sends")?;

    // Session track width (note: "Sesstion" is Ableton's actual typo)
    if let Some(vs) = view_state
        && let Some(width) = vs.session_track_width
    {
        w.value_int("ViewStateSesstionTrackWidth", width as i64)?;
    }

    w.end("Mixer")
}

fn write_routing_target(
    w: &mut AbletonXmlWriter<&mut Vec<u8>>,
    tag: &str,
    routing: &RoutingTarget,
) -> std::io::Result<()> {
    w.start(tag)?;
    w.value_element("Target", &routing.target)?;
    w.value_element("UpperDisplayString", &routing.upper_display_string)?;
    w.value_element("LowerDisplayString", &routing.lower_display_string)?;
    if let Some(ref mpe) = routing.mpe_settings {
        w.start("MpeSettings")?;
        w.value_int("ZoneType", mpe.zone_type as i64)?;
        w.value_int("FirstNoteChannel", mpe.first_note_channel as i64)?;
        w.value_int("LastNoteChannel", mpe.last_note_channel as i64)?;
        w.end("MpeSettings")?;
    } else {
        w.start("MpeSettings")?;
        w.value_int("ZoneType", 0)?;
        w.value_int("FirstNoteChannel", 1)?;
        w.value_int("LastNoteChannel", 15)?;
        w.end("MpeSettings")?;
    }
    w.end(tag)
}

fn write_audio_track(
    w: &mut AbletonXmlWriter<&mut Vec<u8>>,
    track: &AudioTrack,
    num_scenes: usize,
) -> std::io::Result<()> {
    w.start_with_id("AudioTrack", track.common.id)?;
    write_track_common(w, &track.common)?;

    write_device_chain_start(w, &track.common.mixer, track.common.view_state.as_ref())?;
    write_routing_target(w, "AudioInputRouting", &track.audio_input)?;
    write_routing_target(w, "AudioOutputRouting", &track.audio_output)?;
    write_devices(w, &track.common.devices)?;

    // MainSequencer
    w.start("MainSequencer")?;
    w.value_int("LomId", 0)?;

    // ClipSlotList (session clips)
    w.start("ClipSlotList")?;
    write_session_audio_clips(w, &track.session_clips, num_scenes)?;
    w.end("ClipSlotList")?;

    w.value_int("MonitoringEnum", track.monitoring as i64)?;

    // Is armed
    w.start("IsArmed")?;
    w.value_bool("Manual", track.is_armed)?;
    w.end("IsArmed")?;

    // Recorder with take counter
    w.start("Recorder")?;
    w.value_int("TakeCounter", track.take_counter as i64)?;
    w.end("Recorder")?;

    // Arrangement audio clips
    w.start("Sample")?;
    w.start("ArrangerAutomation")?;
    w.start("Events")?;
    for clip in &track.arrangement_clips {
        write_audio_clip(w, clip)?;
    }
    w.end("Events")?;
    w.end("ArrangerAutomation")?;
    w.end("Sample")?;

    w.end("MainSequencer")?;

    // Take lanes
    write_take_lanes(w, &track.take_lanes)?;

    w.end("DeviceChain")?;

    // Per-track automation envelopes
    write_track_automation(w, &track.common.automation_envelopes)?;

    w.end("AudioTrack")
}

fn write_midi_track(
    w: &mut AbletonXmlWriter<&mut Vec<u8>>,
    track: &MidiTrack,
    num_scenes: usize,
) -> std::io::Result<()> {
    w.start_with_id("MidiTrack", track.common.id)?;
    write_track_common(w, &track.common)?;

    write_device_chain_start(w, &track.common.mixer, track.common.view_state.as_ref())?;
    write_routing_target(w, "MidiInputRouting", &track.midi_input)?;
    write_routing_target(w, "AudioOutputRouting", &track.audio_output)?;
    write_devices(w, &track.common.devices)?;

    // MainSequencer
    w.start("MainSequencer")?;
    w.value_int("LomId", 0)?;

    // ClipSlotList (session clips)
    w.start("ClipSlotList")?;
    write_session_midi_clips(w, &track.session_clips, num_scenes)?;
    w.end("ClipSlotList")?;

    w.value_int("MonitoringEnum", track.monitoring as i64)?;

    // Is armed
    w.start("IsArmed")?;
    w.value_bool("Manual", track.is_armed)?;
    w.end("IsArmed")?;

    // Recorder with take counter
    w.start("Recorder")?;
    w.value_int("TakeCounter", track.take_counter as i64)?;
    w.end("Recorder")?;

    // Arrangement MIDI clips
    w.start("ClipTimeable")?;
    w.start("ArrangerAutomation")?;
    w.start("Events")?;
    for clip in &track.arrangement_clips {
        write_midi_clip(w, clip)?;
    }
    w.end("Events")?;
    w.end("ArrangerAutomation")?;
    w.end("ClipTimeable")?;

    w.end("MainSequencer")?;

    // Take lanes
    write_take_lanes(w, &track.take_lanes)?;

    w.end("DeviceChain")?;

    // Per-track automation envelopes
    write_track_automation(w, &track.common.automation_envelopes)?;

    w.end("MidiTrack")
}

fn write_group_track(
    w: &mut AbletonXmlWriter<&mut Vec<u8>>,
    track: &GroupTrack,
) -> std::io::Result<()> {
    w.start_with_id("GroupTrack", track.common.id)?;
    write_track_common(w, &track.common)?;
    write_device_chain_start(w, &track.common.mixer, track.common.view_state.as_ref())?;
    write_devices(w, &track.common.devices)?;
    w.end("DeviceChain")?;

    // Per-track automation envelopes
    write_track_automation(w, &track.common.automation_envelopes)?;

    w.end("GroupTrack")
}

fn write_return_track(
    w: &mut AbletonXmlWriter<&mut Vec<u8>>,
    track: &ReturnTrack,
) -> std::io::Result<()> {
    w.start_with_id("ReturnTrack", track.common.id)?;
    write_track_common(w, &track.common)?;
    write_device_chain_start(w, &track.common.mixer, track.common.view_state.as_ref())?;
    write_devices(w, &track.common.devices)?;
    w.end("DeviceChain")?;

    // Per-track automation envelopes
    write_track_automation(w, &track.common.automation_envelopes)?;

    w.end("ReturnTrack")
}

// ─── Clips ──────────────────────────────────────────────────────────────────

fn write_clip_common(
    w: &mut AbletonXmlWriter<&mut Vec<u8>>,
    common: &ClipCommon,
) -> std::io::Result<()> {
    w.value_int("LomId", 0)?;
    w.value_int("LomIdView", 0)?;
    w.value_float("CurrentStart", common.current_start)?;
    w.value_float("CurrentEnd", common.current_end)?;

    if let Some(ref loop_s) = common.loop_settings {
        w.start("Loop")?;
        w.value_float("LoopStart", loop_s.loop_start)?;
        w.value_float("LoopEnd", loop_s.loop_end)?;
        w.value_float("StartRelative", loop_s.start_relative)?;
        w.value_bool("LoopOn", loop_s.loop_on)?;
        w.value_float("OutMarker", loop_s.loop_end)?;
        w.value_float("HiddenLoopStart", 0.0)?;
        w.value_float("HiddenLoopEnd", 4.0)?;
        w.end("Loop")?;
    }

    w.value_element("Name", &common.name)?;
    w.value_int("Color", common.color as i64)?;
    w.value_bool("Disabled", common.disabled)?;

    if let Some(ref fa) = common.follow_action {
        write_follow_action(w, fa)?;
    }

    w.value_int("LaunchMode", common.launch_mode as i64)?;
    w.value_int("LaunchQuantisation", common.launch_quantisation as i64)?;

    if let Some(ref grid) = common.grid {
        write_clip_grid(w, "Grid", grid)?;
    }

    w.value_bool("Legato", common.legato)?;
    w.value_bool("Ram", common.ram)?;
    w.value_float("VelocityAmount", common.velocity_amount)?;

    w.start("GrooveSettings")?;
    w.value_int("GrooveId", common.groove_id as i64)?;
    w.end("GrooveSettings")?;

    w.value_float("FreezeStart", common.freeze_start)?;
    w.value_float("FreezeEnd", common.freeze_end)?;
    w.value_int("TakeId", common.take_id as i64)?;

    // Clip automation envelopes
    write_clip_envelopes(w, &common.envelopes)?;

    Ok(())
}

fn write_follow_action(
    w: &mut AbletonXmlWriter<&mut Vec<u8>>,
    fa: &FollowAction,
) -> std::io::Result<()> {
    w.start("FollowAction")?;
    w.value_float("FollowTime", fa.follow_time)?;
    w.value_bool("IsLinked", fa.is_linked)?;
    w.value_int("LoopIterations", fa.loop_iterations as i64)?;
    w.value_int("FollowActionA", fa.action_a as i64)?;
    w.value_int("FollowActionB", fa.action_b as i64)?;
    w.value_int("FollowChanceA", fa.chance_a as i64)?;
    w.value_int("FollowChanceB", fa.chance_b as i64)?;
    w.value_bool("FollowActionEnabled", fa.enabled)?;
    w.end("FollowAction")
}

fn write_clip_grid(
    w: &mut AbletonXmlWriter<&mut Vec<u8>>,
    tag: &str,
    grid: &ClipGrid,
) -> std::io::Result<()> {
    w.start(tag)?;
    w.value_int("FixedNumerator", grid.fixed_numerator as i64)?;
    w.value_int("FixedDenominator", grid.fixed_denominator as i64)?;
    w.value_int("GridIntervalPixel", grid.grid_interval_pixel as i64)?;
    w.value_int("Ntoles", grid.ntoles as i64)?;
    w.value_bool("SnapToGrid", grid.snap_to_grid)?;
    w.value_bool("Fixed", grid.fixed)?;
    w.end(tag)
}

fn write_audio_clip(
    w: &mut AbletonXmlWriter<&mut Vec<u8>>,
    clip: &AudioClip,
) -> std::io::Result<()> {
    let time_str = format_float(clip.common.time);
    w.start_with_attrs(
        "AudioClip",
        &[("Id", &clip.common.id.to_string()), ("Time", &time_str)],
    )?;

    write_clip_common(w, &clip.common)?;

    if let Some(ref sr) = clip.sample_ref {
        write_sample_ref(w, sr)?;
    }

    w.value_int("WarpMode", clip.warp_mode as i64)?;
    w.value_bool("IsWarped", clip.is_warped)?;
    w.value_float("PitchCoarse", clip.pitch_coarse)?;
    w.value_float("PitchFine", clip.pitch_fine)?;
    w.value_float("SampleVolume", clip.sample_volume)?;

    // Warp markers
    if !clip.warp_markers.is_empty() {
        w.start("WarpMarkers")?;
        for marker in &clip.warp_markers {
            w.empty_with_attrs(
                "WarpMarker",
                &[
                    ("SecTime", &format_float(marker.sec_time)),
                    ("BeatTime", &format_float(marker.beat_time)),
                ],
            )?;
        }
        w.end("WarpMarkers")?;
    }

    // Fades
    if let Some(ref fades) = clip.fades {
        w.start("Fades")?;
        w.value_float("FadeInLength", fades.fade_in_length)?;
        w.value_float("FadeOutLength", fades.fade_out_length)?;
        w.value_float("FadeInCurveSkew", fades.fade_in_curve_skew)?;
        w.value_float("FadeInCurveSlope", fades.fade_in_curve_slope)?;
        w.value_float("FadeOutCurveSkew", fades.fade_out_curve_skew)?;
        w.value_float("FadeOutCurveSlope", fades.fade_out_curve_slope)?;
        w.end("Fades")?;
    }

    // Warp params
    w.value_float("GranularityTones", clip.granularity_tones)?;
    w.value_float("GranularityTexture", clip.granularity_texture)?;
    w.value_float("FluctuationTexture", clip.fluctuation_texture)?;
    w.value_int("TransientResolution", clip.transient_resolution as i64)?;
    w.value_int("TransientLoopMode", clip.transient_loop_mode as i64)?;
    w.value_float("TransientEnvelope", clip.transient_envelope)?;
    w.value_float("ComplexProFormants", clip.complex_pro_formants)?;
    w.value_float("ComplexProEnvelope", clip.complex_pro_envelope)?;

    w.value_bool("Fade", clip.fade_on)?;
    w.value_bool("HiQ", clip.hiq)?;
    w.value_bool("IsSongTempoLeader", clip.is_song_tempo_leader)?;

    w.end("AudioClip")
}

fn write_midi_clip(w: &mut AbletonXmlWriter<&mut Vec<u8>>, clip: &MidiClip) -> std::io::Result<()> {
    let time_str = format_float(clip.common.time);
    w.start_with_attrs(
        "MidiClip",
        &[("Id", &clip.common.id.to_string()), ("Time", &time_str)],
    )?;

    write_clip_common(w, &clip.common)?;

    // Notes > KeyTracks
    w.start("Notes")?;
    w.start("KeyTracks")?;
    for (i, kt) in clip.key_tracks.iter().enumerate() {
        w.start_with_id("KeyTrack", i as i32)?;
        w.start("Notes")?;
        for note in &kt.notes {
            let note_id = w.next_id().to_string();
            w.empty_with_attrs(
                "MidiNoteEvent",
                &[
                    ("Time", &format_float(note.time)),
                    ("Duration", &format_float(note.duration)),
                    ("Velocity", &note.velocity.to_string()),
                    ("VelocityDeviation", &note.velocity_deviation.to_string()),
                    ("IsEnabled", if note.is_enabled { "true" } else { "false" }),
                    ("Probability", &format_float(note.probability)),
                    ("NoteId", &note_id),
                ],
            )?;
        }
        w.end("Notes")?;
        w.value_int("MidiKey", kt.midi_key as i64)?;
        w.end("KeyTrack")?;
    }
    w.end("KeyTracks")?;
    w.empty("PerNoteEventStore")?;
    w.end("Notes")?;

    // Scale info
    if let Some(ref ks) = clip.scale_info {
        w.start("ScaleInformation")?;
        w.value_int("RootNote", ks.root_note.to_midi() as i64)?;
        w.value_element("Name", &ks.scale)?;
        w.end("ScaleInformation")?;
        w.value_bool("IsInKey", true)?;
    }

    w.value_int("BankSelectCoarse", clip.bank_select_coarse as i64)?;
    w.value_int("BankSelectFine", clip.bank_select_fine as i64)?;
    w.value_int("ProgramChange", clip.program_change as i64)?;
    w.value_int(
        "NoteSpellingPreference",
        clip.note_spelling_preference as i64,
    )?;

    if let Some(ref grid) = clip.expression_grid {
        write_clip_grid(w, "ExpressionGrid", grid)?;
    }

    w.end("MidiClip")
}

fn write_sample_ref(w: &mut AbletonXmlWriter<&mut Vec<u8>>, sr: &SampleRef) -> std::io::Result<()> {
    w.start("SampleRef")?;
    w.start("FileRef")?;

    w.value_int("RelativePathType", 1)?;
    w.value_element("RelativePath", sr.relative_path.as_deref().unwrap_or(""))?;
    w.value_element(
        "Path",
        &sr.path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
    )?;
    w.value_int("Type", 1)?;
    w.value_element("LivePackName", sr.live_pack_name.as_deref().unwrap_or(""))?;
    w.value_element("LivePackId", sr.live_pack_id.as_deref().unwrap_or(""))?;
    w.value_int("OriginalFileSize", sr.file_size.unwrap_or(0) as i64)?;
    w.value_int("OriginalCrc", sr.crc.unwrap_or(0) as i64)?;

    w.end("FileRef")?;

    w.value_int("LastModDate", sr.last_mod_date.unwrap_or(0) as i64)?;
    w.empty("SourceContext")?;
    w.value_int("SampleUsageHint", 0)?;
    w.value_int("DefaultDuration", sr.default_duration.unwrap_or(0) as i64)?;
    w.value_int(
        "DefaultSampleRate",
        sr.default_sample_rate.unwrap_or(44100) as i64,
    )?;

    w.end("SampleRef")
}

fn write_session_audio_clips(
    w: &mut AbletonXmlWriter<&mut Vec<u8>>,
    clips: &[SessionClip<AudioClip>],
    num_scenes: usize,
) -> std::io::Result<()> {
    // Write one slot per scene (Ableton expects empty slots too)
    let max_clip_slot = clips.iter().map(|c| c.slot_index + 1).max().unwrap_or(0);
    let num_slots = max_clip_slot.max(num_scenes);

    for slot_idx in 0..num_slots {
        w.start_with_id("ClipSlot", slot_idx as i32)?;
        let has_stop = if let Some(sc) = clips.iter().find(|c| c.slot_index == slot_idx) {
            w.start("ClipData")?;
            write_audio_clip(w, &sc.clip)?;
            w.end("ClipData")?;
            sc.has_stop
        } else {
            w.start("ClipData")?;
            w.end("ClipData")?;
            true
        };
        w.value_bool("HasStop", has_stop)?;
        w.value_bool("NeedRefreeze", true)?;
        w.end("ClipSlot")?;
    }
    Ok(())
}

fn write_session_midi_clips(
    w: &mut AbletonXmlWriter<&mut Vec<u8>>,
    clips: &[SessionClip<MidiClip>],
    num_scenes: usize,
) -> std::io::Result<()> {
    // Write one slot per scene (Ableton expects empty slots too)
    let max_clip_slot = clips.iter().map(|c| c.slot_index + 1).max().unwrap_or(0);
    let num_slots = max_clip_slot.max(num_scenes);

    for slot_idx in 0..num_slots {
        w.start_with_id("ClipSlot", slot_idx as i32)?;
        let has_stop = if let Some(sc) = clips.iter().find(|c| c.slot_index == slot_idx) {
            w.start("ClipData")?;
            write_midi_clip(w, &sc.clip)?;
            w.end("ClipData")?;
            sc.has_stop
        } else {
            w.start("ClipData")?;
            w.end("ClipData")?;
            true
        };
        w.value_bool("HasStop", has_stop)?;
        w.value_bool("NeedRefreeze", true)?;
        w.end("ClipSlot")?;
    }
    Ok(())
}

// ─── Master / Transport / Markers ───────────────────────────────────────────

fn write_master_track(
    w: &mut AbletonXmlWriter<&mut Vec<u8>>,
    set: &AbletonLiveSet,
) -> std::io::Result<()> {
    // v12+ uses MainTrack
    let tag = if set.version.at_least(12, 0) {
        "MainTrack"
    } else {
        "MasterTrack"
    };

    w.start(tag)?;
    w.value_int("LomId", 0)?;

    w.start("DeviceChain")?;

    // Mixer with tempo (master track has no session track width)
    w.start("Mixer")?;
    w.value_int("LomId", 0)?;

    // Tempo
    w.start("Tempo")?;
    w.value_float("Manual", set.tempo)?;
    let tempo_target_id = w.automation_target("AutomationTarget")?;

    // Inline ArrangerAutomation: only the sentinel event.
    // Actual tempo automation goes in AutomationEnvelopes (v10+ style)
    // to avoid duplication on round-trip.
    w.start("ArrangerAutomation")?;
    w.start("Events")?;
    w.empty_with_attrs(
        "FloatEvent",
        &[
            ("Id", "0"),
            ("Time", "-63072000"),
            ("Value", &format_float(set.tempo)),
        ],
    )?;
    w.end("Events")?;
    w.end("ArrangerAutomation")?;
    w.end("Tempo")?;

    // Time signature
    w.start("TimeSignature")?;
    w.start("TimeSignatures")?;
    w.start_with_id("RemoteableTimeSignature", 0)?;
    w.value_int("Numerator", set.time_signature.numerator as i64)?;
    w.value_int("Denominator", set.time_signature.denominator as i64)?;
    w.value_float("Time", 0.0)?;
    w.end("RemoteableTimeSignature")?;
    w.end("TimeSignatures")?;
    w.end("TimeSignature")?;

    // Master mixer volume/pan
    if let Some(ref master) = set.master_track {
        w.start("Volume")?;
        w.value_float("Manual", master.mixer.volume)?;
        w.automation_target("AutomationTarget")?;
        w.end("Volume")?;

        w.start("Pan")?;
        w.value_float("Manual", master.mixer.pan)?;
        w.automation_target("AutomationTarget")?;
        w.end("Pan")?;
    }

    w.end("Mixer")?;

    // Master output routing
    let default_routing = RoutingTarget {
        target: "AudioOut/External/S0".to_string(),
        ..RoutingTarget::default()
    };
    let output = set
        .master_track
        .as_ref()
        .map(|m| &m.audio_output)
        .unwrap_or(&default_routing);
    write_routing_target(w, "AudioOutputRouting", output)?;

    if let Some(ref master) = set.master_track {
        write_devices(w, &master.devices)?;
    }

    w.end("DeviceChain")?;

    // Tempo automation envelope (v10+ style, cross-reference by pointee ID)
    w.start("AutomationEnvelopes")?;
    w.start("Envelopes")?;
    if !set.tempo_automation.is_empty() {
        w.start_with_id("AutomationEnvelope", 0)?;
        w.start("EnvelopeTarget")?;
        w.value_int("PointeeId", tempo_target_id as i64)?;
        w.end("EnvelopeTarget")?;
        w.start("Automation")?;
        w.start("Events")?;
        for (i, point) in set.tempo_automation.iter().enumerate() {
            write_float_event(w, i as i32, point)?;
        }
        w.end("Events")?;
        w.end("Automation")?;
        w.end("AutomationEnvelope")?;
    }
    w.end("Envelopes")?;
    w.end("AutomationEnvelopes")?;

    w.end(tag)
}

fn write_pre_hear_track(
    w: &mut AbletonXmlWriter<&mut Vec<u8>>,
    set: &AbletonLiveSet,
) -> std::io::Result<()> {
    w.start("PreHearTrack")?;
    w.value_int("LomId", 0)?;
    w.start("DeviceChain")?;
    let default_routing = RoutingTarget {
        target: "AudioOut/External/S1".to_string(),
        ..RoutingTarget::default()
    };
    let routing = set
        .pre_hear_track
        .as_ref()
        .map(|pht| &pht.audio_output)
        .unwrap_or(&default_routing);
    write_routing_target(w, "AudioOutputRouting", routing)?;
    w.end("DeviceChain")?;
    w.end("PreHearTrack")
}

fn write_locators(
    w: &mut AbletonXmlWriter<&mut Vec<u8>>,
    locators: &[Locator],
) -> std::io::Result<()> {
    w.start("Locators")?;
    w.start("Locators")?;
    for loc in locators {
        w.start("Locator")?;
        w.value_float("Time", loc.time)?;
        w.value_element("Name", &loc.name)?;
        w.end("Locator")?;
    }
    w.end("Locators")?;
    w.end("Locators")
}

fn write_scenes(w: &mut AbletonXmlWriter<&mut Vec<u8>>, scenes: &[Scene]) -> std::io::Result<()> {
    w.start("Scenes")?;
    for scene in scenes {
        w.start_with_id("Scene", scene.id)?;
        w.value_element("Name", &scene.name)?;
        w.value_int("Color", scene.color as i64)?;
        w.value_element("Annotation", &scene.annotation)?;

        if let Some(tempo) = scene.tempo {
            w.start("Tempo")?;
            w.value_float("Manual", tempo)?;
            w.end("Tempo")?;
            w.value_bool("IsTempoEnabled", true)?;
        } else {
            w.value_bool("IsTempoEnabled", false)?;
        }

        if let Some(ref fa) = scene.follow_action {
            write_follow_action(w, fa)?;
        }

        w.value_int("TimeSignatureId", scene.time_signature_id as i64)?;
        w.value_bool("IsTimeSignatureEnabled", scene.is_time_signature_enabled)?;

        w.end("Scene")?;
    }
    w.end("Scenes")
}

fn write_transport(
    w: &mut AbletonXmlWriter<&mut Vec<u8>>,
    transport: &TransportState,
) -> std::io::Result<()> {
    w.start("Transport")?;
    w.value_bool("LoopOn", transport.loop_on)?;
    w.value_float("LoopStart", transport.loop_start)?;
    w.value_float("LoopLength", transport.loop_length)?;
    w.value_bool("LoopIsSongStart", transport.loop_is_song_start)?;
    w.value_float("CurrentTime", transport.current_time)?;
    w.value_bool("PunchIn", transport.punch_in)?;
    w.value_bool("PunchOut", transport.punch_out)?;
    w.value_int(
        "MetronomeTickDuration",
        transport.metronome_tick_duration as i64,
    )?;
    w.value_int("DrawMode", transport.draw_mode as i64)?;
    w.end("Transport")
}

// ─── Take Lanes ────────────────────────────────────────────────────────────

fn write_take_lanes(
    w: &mut AbletonXmlWriter<&mut Vec<u8>>,
    take_lanes: &[TakeLane],
) -> std::io::Result<()> {
    if take_lanes.is_empty() {
        return Ok(());
    }
    w.start("TakeLanes")?;
    w.start("TakeLanes")?;
    for lane in take_lanes {
        w.start_with_id("TakeLane", lane.id)?;
        w.value_element("Name", &lane.name)?;
        w.value_bool("IsActive", lane.is_active)?;

        // Audio clips in this take lane
        if !lane.audio_clips.is_empty() {
            w.start("Events")?;
            for clip in &lane.audio_clips {
                write_audio_clip(w, clip)?;
            }
            w.end("Events")?;
        }

        // MIDI clips in this take lane
        if !lane.midi_clips.is_empty() {
            w.start("Events")?;
            for clip in &lane.midi_clips {
                write_midi_clip(w, clip)?;
            }
            w.end("Events")?;
        }

        w.end("TakeLane")?;
    }
    w.end("TakeLanes")?;
    w.end("TakeLanes")
}

// ─── Groove Pool ───────────────────────────────────────────────────────────

fn write_groove_pool(
    w: &mut AbletonXmlWriter<&mut Vec<u8>>,
    grooves: &[Groove],
) -> std::io::Result<()> {
    w.start("GroovePool")?;
    w.start("Grooves")?;
    for groove in grooves {
        w.start_with_id("Groove", groove.id)?;
        w.value_element("Name", &groove.name)?;
        w.start("FileRef")?;
        w.value_element("Path", &groove.path)?;
        w.end("FileRef")?;
        w.value_float("Base", groove.base)?;
        w.value_float("QuantizeAmount", groove.quantize_amount)?;
        w.value_float("TimingAmount", groove.timing_amount)?;
        w.value_float("RandomAmount", groove.random_amount)?;
        w.value_float("VelocityAmount", groove.velocity_amount)?;
        w.end("Groove")?;
    }
    w.end("Grooves")?;
    w.end("GroovePool")
}

// ─── Tuning System ─────────────────────────────────────────────────────────

fn write_tuning_system(
    w: &mut AbletonXmlWriter<&mut Vec<u8>>,
    tuning: &TuningSystem,
) -> std::io::Result<()> {
    w.start("TuningSystems")?;
    // Emit the raw XML directly — the tuning system structure is complex
    // and we preserve it verbatim from parsing.
    w.write_raw(&tuning.raw_xml)?;
    w.end("TuningSystems")
}

// ─── Track & Clip Automation ──────────────────────────────────────────────

fn write_track_automation(
    w: &mut AbletonXmlWriter<&mut Vec<u8>>,
    envelopes: &[AutomationEnvelope],
) -> std::io::Result<()> {
    if envelopes.is_empty() {
        return Ok(());
    }

    w.start("AutomationEnvelopes")?;
    w.start("Envelopes")?;
    for (i, env) in envelopes.iter().enumerate() {
        w.start_with_id("AutomationEnvelope", i as i32)?;
        w.start("EnvelopeTarget")?;
        w.value_int("PointeeId", env.pointee_id as i64)?;
        w.end("EnvelopeTarget")?;
        w.start("Automation")?;
        w.start("Events")?;
        for (j, event) in env.events.iter().enumerate() {
            write_automation_event(w, j as i32, event)?;
        }
        w.end("Events")?;
        w.end("Automation")?;
        w.end("AutomationEnvelope")?;
    }
    w.end("Envelopes")?;
    w.end("AutomationEnvelopes")
}

fn write_clip_envelopes(
    w: &mut AbletonXmlWriter<&mut Vec<u8>>,
    envelopes: &[ClipEnvelope],
) -> std::io::Result<()> {
    if envelopes.is_empty() {
        return Ok(());
    }

    w.start("Envelopes")?;
    w.start("Envelopes")?;
    for (i, env) in envelopes.iter().enumerate() {
        w.start_with_id("ClipEnvelope", i as i32)?;
        w.start("EnvelopeTarget")?;
        w.value_int("PointeeId", env.pointee_id as i64)?;
        w.end("EnvelopeTarget")?;
        w.start("Automation")?;
        w.start("Events")?;
        for (j, event) in env.events.iter().enumerate() {
            write_automation_event(w, j as i32, event)?;
        }
        w.end("Events")?;
        w.end("Automation")?;
        w.end("ClipEnvelope")?;
    }
    w.end("Envelopes")?;
    w.end("Envelopes")
}

fn write_automation_event(
    w: &mut AbletonXmlWriter<&mut Vec<u8>>,
    id: i32,
    event: &AutomationEvent,
) -> std::io::Result<()> {
    match event {
        AutomationEvent::Float {
            time,
            value,
            curve_control_1,
            curve_control_2,
        } => {
            let id_str = id.to_string();
            let time_str = format_float(*time);
            let value_str = format_float(*value);

            let mut attrs: Vec<(&str, String)> =
                vec![("Id", id_str), ("Time", time_str), ("Value", value_str)];

            if let Some((x, y)) = curve_control_1 {
                attrs.push(("CurveControl1X", format_float(*x)));
                attrs.push(("CurveControl1Y", format_float(*y)));
            }
            if let Some((x, y)) = curve_control_2 {
                attrs.push(("CurveControl2X", format_float(*x)));
                attrs.push(("CurveControl2Y", format_float(*y)));
            }

            let attr_refs: Vec<(&str, &str)> =
                attrs.iter().map(|(k, v)| (*k, v.as_str())).collect();
            w.empty_with_attrs("FloatEvent", &attr_refs)
        }
        AutomationEvent::Bool { time, value } => {
            let id_str = id.to_string();
            let time_str = format_float(*time);
            w.empty_with_attrs(
                "BoolEvent",
                &[
                    ("Id", id_str.as_str()),
                    ("Time", time_str.as_str()),
                    ("Value", if *value { "true" } else { "false" }),
                ],
            )
        }
        AutomationEvent::Enum { time, value } => {
            let id_str = id.to_string();
            let time_str = format_float(*time);
            let value_str = value.to_string();
            w.empty_with_attrs(
                "EnumEvent",
                &[
                    ("Id", id_str.as_str()),
                    ("Time", time_str.as_str()),
                    ("Value", value_str.as_str()),
                ],
            )
        }
    }
}

// ─── Float Events with Bezier Handles ──────────────────────────────────────

fn write_float_event(
    w: &mut AbletonXmlWriter<&mut Vec<u8>>,
    id: i32,
    point: &AutomationPoint,
) -> std::io::Result<()> {
    let id_str = id.to_string();
    let time_str = format_float(point.time);
    let value_str = format_float(point.value);

    // Build attributes, including optional bezier curve controls
    let mut attrs: Vec<(&str, String)> =
        vec![("Id", id_str), ("Time", time_str), ("Value", value_str)];

    if let Some((x, y)) = point.curve_control_1 {
        attrs.push(("CurveControl1X", format_float(x)));
        attrs.push(("CurveControl1Y", format_float(y)));
    }
    if let Some((x, y)) = point.curve_control_2 {
        attrs.push(("CurveControl2X", format_float(x)));
        attrs.push(("CurveControl2Y", format_float(y)));
    }

    let attr_refs: Vec<(&str, &str)> = attrs.iter().map(|(k, v)| (*k, v.as_str())).collect();
    w.empty_with_attrs("FloatEvent", &attr_refs)
}

/// Format a float value the way Ableton expects.
fn format_float(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        let raw = format!("{value}");
        raw.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}
