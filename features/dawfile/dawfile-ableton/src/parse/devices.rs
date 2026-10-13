//! Device (plugin / built-in effect) parsing.
//!
//! Devices live inside `DeviceChain` elements. The structure is:
//!
//! ```text
//! DeviceChain > DeviceChain (inner) > Devices > [device elements]
//! ```
//!
//! Rack devices (AudioEffectGroupDevice, InstrumentGroupDevice, etc.) contain
//! nested `Branches` with their own `DeviceChain` elements, forming an
//! arbitrarily deep tree. This parser recurses through all levels.
//!
//! Device element names identify built-in vs plugin:
//! - `PluginDevice` — VST2/VST3/AU wrapper
//! - `MxDeviceAudioEffect` / `MxDeviceInstrument` — Max for Live
//! - `*GroupDevice` — Racks (contain nested device chains)
//! - Everything else — Ableton built-in (e.g., `Compressor2`, `Eq8`, `Reverb`)

use super::xml_helpers::*;
use crate::devices;
use crate::types::{AbletonVersion, Device, DeviceCategory, DeviceFormat, DevicePluginId};
use roxmltree::Node;

/// Rack device tag names that contain nested device chains.
const RACK_TAGS: &[&str] = &[
    "AudioEffectGroupDevice",
    "InstrumentGroupDevice",
    "MidiEffectGroupDevice",
    "DrumGroupDevice",
];

/// Known v12 note algorithm / note transform device tags.
const NOTE_ALGORITHM_TAGS: &[&str] = &[
    "MidiArpeggiator",
    "MidiNoteLength",
    "MidiPitcher",
    "MidiRandom",
    "MidiScale",
    "MidiVelocity",
    "MidiChord",
    "NoteTransformArpeggiate",
    "NoteTransformConnect",
    "NoteTransformOrnament",
    "NoteTransformQuantize",
    "NoteTransformRecombine",
    "NoteTransformRhythm",
    "NoteTransformSeed",
    "NoteTransformShape",
    "NoteTransformSpan",
    "NoteTransformStacks",
    "NoteTransformStrum",
    "NoteTransformTimeWarp",
];

/// Parse all devices from a `DeviceChain` node, recursing into racks.
pub fn parse_devices(device_chain: Node<'_, '_>, version: &AbletonVersion) -> Vec<Device> {
    let mut devices = Vec::new();
    collect_devices_recursive(device_chain, version, &mut devices, 0);
    devices
}

/// Recursively collect devices from a device chain, descending into racks.
fn collect_devices_recursive(
    node: Node<'_, '_>,
    version: &AbletonVersion,
    out: &mut Vec<Device>,
    depth: usize,
) {
    // Guard against pathological nesting (Ableton practically limits to ~8 levels)
    if depth > 16 {
        return;
    }

    // Look for Devices in direct children and in nested DeviceChain
    for target in find_device_lists(node) {
        for device_node in target.children() {
            if !device_node.is_element() {
                continue;
            }

            let tag = device_node.tag_name().name();

            if tag == "PluginDevice" {
                if let Some(dev) = parse_plugin_device(device_node) {
                    out.push(dev);
                }
            } else if matches!(
                tag,
                "MxDeviceAudioEffect" | "MxDeviceInstrument" | "MxDeviceMidiEffect"
            ) {
                out.push(parse_max_for_live_device(device_node));
            } else if RACK_TAGS.contains(&tag) {
                // Rack device: add the rack itself, then recurse into its branches
                out.push(parse_builtin_device(device_node, tag));
                recurse_into_rack(device_node, version, out, depth);
            } else if NOTE_ALGORITHM_TAGS.contains(&tag) {
                out.push(parse_note_algorithm_device(device_node, tag));
            } else if !tag.is_empty() {
                out.push(parse_builtin_device(device_node, tag));
            }
        }
    }
}

/// Find all `<Devices>` element nodes reachable from this node.
///
/// Checks multiple paths because Ableton uses different chain element names:
/// - `Devices` (direct child)
/// - `DeviceChain > Devices`
/// - `AudioToAudioDeviceChain > Devices` (inside rack branches)
/// - `MidiToAudioDeviceChain > Devices` (inside instrument rack branches)
/// - `MidiToMidiDeviceChain > Devices` (inside MIDI effect rack branches)
fn find_device_lists<'a, 'input>(node: Node<'a, 'input>) -> Vec<Node<'a, 'input>> {
    let mut lists = Vec::new();

    // Direct: node > Devices
    if let Some(d) = child(node, "Devices") {
        lists.push(d);
    }

    // Check all known chain element names
    for chain_name in &[
        "DeviceChain",
        "AudioToAudioDeviceChain",
        "MidiToAudioDeviceChain",
        "MidiToMidiDeviceChain",
        "AudioToMidiDeviceChain",
    ] {
        if let Some(inner_chain) = child(node, chain_name)
            && let Some(d) = child(inner_chain, "Devices")
        {
            lists.push(d);
        }
    }

    lists
}

/// Recurse into rack branches to find nested devices.
///
/// Rack structure:
/// ```text
/// <*GroupDevice>
///   <Branches>
///     <*Branch Id="0">
///       <DeviceChain>
///         <Devices>
///           ... (more devices, possibly more racks)
///         </Devices>
///       </DeviceChain>
///     </*Branch>
///   </Branches>
/// </*GroupDevice>
/// ```
fn recurse_into_rack(
    rack_node: Node<'_, '_>,
    version: &AbletonVersion,
    out: &mut Vec<Device>,
    depth: usize,
) {
    if let Some(branches) = child(rack_node, "Branches") {
        for branch in branches.children() {
            if !branch.is_element() {
                continue;
            }
            // Each branch has a DeviceChain
            if let Some(dc) = child(branch, "DeviceChain") {
                collect_devices_recursive(dc, version, out, depth + 1);
            }
        }
    }

    // DrumGroupDevice has a different structure: DrumBranch > DeviceChain
    // Also check for ReturnBranch nodes (rack return chains)
    for child_name in &["ReturnBranches", "DrumPads"] {
        if let Some(container) = child(rack_node, child_name) {
            for branch in container.children() {
                if !branch.is_element() {
                    continue;
                }
                if let Some(dc) = child(branch, "DeviceChain") {
                    collect_devices_recursive(dc, version, out, depth + 1);
                }
            }
        }
    }
}

fn parse_plugin_device(node: Node<'_, '_>) -> Option<Device> {
    let is_on = child(node, "On")
        .and_then(|on| child_bool(on, "Manual"))
        .unwrap_or(true);

    // Get device ID from SourceContext
    let device_id = descend(node, "SourceContext.Value.BranchSourceContext")
        .and_then(|bsc| child_value(bsc, "BranchDeviceId"))
        .map(|s| s.to_string());

    // Determine format and name from PluginDesc
    let (name, format, processor_state, controller_state) =
        if let Some(plugin_desc) = child(node, "PluginDesc") {
            let (n, f) = parse_plugin_desc(plugin_desc);
            let (ps, cs) = parse_plugin_states(plugin_desc);
            (n, f, ps, cs)
        } else {
            (
                "Unknown Plugin".to_string(),
                DeviceFormat::Unknown,
                None,
                None,
            )
        };

    let parsed_id = device_id.as_deref().map(parse_device_id);

    Some(Device {
        name,
        format,
        device_id,
        is_on,
        parsed_id,
        processor_state,
        controller_state,
        builtin_params: None,
    })
}

fn parse_plugin_desc(desc: Node<'_, '_>) -> (String, DeviceFormat) {
    // VST3: Name is a direct child of Vst3PluginInfo (not inside Vst3Preset)
    if let Some(vst3_info) = child(desc, "Vst3PluginInfo") {
        let name = child_value(vst3_info, "Name")
            .unwrap_or("Unknown VST3")
            .to_string();
        return (name, DeviceFormat::Vst3);
    }

    // VST2: uses PlugName (not Name)
    if let Some(vst_info) = child(desc, "VstPluginInfo") {
        let name = child_value(vst_info, "PlugName")
            .unwrap_or("Unknown VST2")
            .to_string();
        return (name, DeviceFormat::Vst2);
    }

    // AU
    if let Some(au_info) = child(desc, "AuPluginInfo") {
        let name = child_value(au_info, "Name")
            .unwrap_or("Unknown AU")
            .to_string();
        return (name, DeviceFormat::AudioUnit);
    }

    ("Unknown Plugin".to_string(), DeviceFormat::Unknown)
}

fn parse_max_for_live_device(node: Node<'_, '_>) -> Device {
    let is_on = child(node, "On")
        .and_then(|on| child_bool(on, "Manual"))
        .unwrap_or(true);

    let name = child(node, "Name")
        .and_then(|n| {
            child_value(n, "UserName")
                .filter(|s| !s.is_empty())
                .or_else(|| child_value(n, "EffectiveName"))
        })
        .unwrap_or("Max for Live Device")
        .to_string();

    Device {
        name,
        format: DeviceFormat::MaxForLive,
        device_id: None,
        is_on,
        parsed_id: None,
        processor_state: None,
        controller_state: None,
        builtin_params: None,
    }
}

fn parse_builtin_device(node: Node<'_, '_>, tag: &str) -> Device {
    let is_on = child(node, "On")
        .and_then(|on| child_bool(on, "Manual"))
        .unwrap_or(true);

    let name = child(node, "Name")
        .and_then(|n| {
            child_value(n, "UserName")
                .filter(|s| !s.is_empty())
                .or_else(|| child_value(n, "EffectiveName"))
        })
        .unwrap_or(tag)
        .to_string();

    let builtin_params = devices::parse_builtin_params(tag, node);

    Device {
        name,
        format: DeviceFormat::Builtin,
        device_id: None,
        is_on,
        parsed_id: None,
        processor_state: None,
        controller_state: None,
        builtin_params,
    }
}

fn parse_note_algorithm_device(node: Node<'_, '_>, tag: &str) -> Device {
    let is_on = child(node, "On")
        .and_then(|on| child_bool(on, "Manual"))
        .unwrap_or(true);

    let name = child(node, "Name")
        .and_then(|n| {
            child_value(n, "UserName")
                .filter(|s| !s.is_empty())
                .or_else(|| child_value(n, "EffectiveName"))
        })
        .unwrap_or(tag)
        .to_string();

    Device {
        name,
        format: DeviceFormat::NoteAlgorithm,
        device_id: None,
        is_on,
        parsed_id: None,
        processor_state: None,
        controller_state: None,
        builtin_params: None,
    }
}

/// Parse a device plugin identifier string into structured form.
fn parse_device_id(raw: &str) -> DevicePluginId {
    let parts: Vec<&str> = raw.splitn(4, ':').collect();

    if parts.len() < 3 || parts[0] != "device" {
        return DevicePluginId {
            format: DeviceFormat::Unknown,
            category: DeviceCategory::Unknown,
            uuid: None,
            numeric_id: None,
            name_hint: None,
        };
    }

    let format = match parts[1] {
        "vst3" => DeviceFormat::Vst3,
        "vst" => DeviceFormat::Vst2,
        "au" => DeviceFormat::AudioUnit,
        "m4l" => DeviceFormat::MaxForLive,
        _ => DeviceFormat::Unknown,
    };

    let category = match parts[2] {
        "audiofx" => DeviceCategory::AudioEffect,
        "instr" => DeviceCategory::Instrument,
        "midifx" => DeviceCategory::MidiEffect,
        _ => DeviceCategory::Unknown,
    };

    let remainder = parts.get(3).unwrap_or(&"");

    let (uuid, numeric_id, name_hint) = match format {
        DeviceFormat::Vst3 | DeviceFormat::AudioUnit => {
            // UUID is the remainder (may contain sub-colons for AU)
            let uuid_str = if remainder.is_empty() {
                None
            } else {
                Some(remainder.to_string())
            };
            (uuid_str, None, None)
        }
        DeviceFormat::Vst2 => {
            // Format: <numeric_id>?n=<name>
            if let Some(q_pos) = remainder.find("?n=") {
                let id_str = &remainder[..q_pos];
                let name = &remainder[q_pos + 3..];
                (None, id_str.parse::<i64>().ok(), Some(name.to_string()))
            } else {
                (None, remainder.parse::<i64>().ok(), None)
            }
        }
        _ => (None, None, None),
    };

    DevicePluginId {
        format,
        category,
        uuid,
        numeric_id,
        name_hint,
    }
}

/// Extract processor and controller state from a PluginDesc node.
fn parse_plugin_states(plugin_desc: Node<'_, '_>) -> (Option<String>, Option<String>) {
    // Try VST3 path: Vst3PluginInfo > Preset > Vst3Preset > ProcessorState / ControllerState
    if let Some(vst3_info) = child(plugin_desc, "Vst3PluginInfo")
        && let Some(preset) = descend(vst3_info, "Preset.Vst3Preset")
    {
        let ps = child(preset, "ProcessorState")
            .and_then(|n| n.text())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let cs = child(preset, "ControllerState")
            .and_then(|n| n.text())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        return (ps, cs);
    }

    // Try VST2 path: VstPluginInfo > Preset > VstPreset > ProcessorState
    if let Some(vst_info) = child(plugin_desc, "VstPluginInfo")
        && let Some(preset) = descend(vst_info, "Preset.VstPreset")
    {
        let ps = child(preset, "Buffer")
            .and_then(|n| n.text())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        return (ps, None);
    }

    (None, None)
}
