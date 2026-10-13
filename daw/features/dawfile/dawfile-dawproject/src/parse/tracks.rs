//! Parse the track hierarchy from the `<Structure>` element.

use super::{devices, xml_helpers::*};
use crate::types::{
    AutomationUnit, Channel, ChannelRole, ContentType, DeviceParameter, DeviceParameterValue,
    DeviceRole, Send, Track,
};
use roxmltree::Node;

/// Parse all top-level tracks from a `<Structure>` element.
pub fn parse_tracks(structure: Node<'_, '_>) -> Vec<Track> {
    children(structure, "Track").map(parse_track).collect()
}

pub fn parse_track(node: Node<'_, '_>) -> Track {
    let id = attr(node, "id").unwrap_or("").to_string();
    let name = attr(node, "name").unwrap_or("").to_string();
    let color = attr(node, "color").map(str::to_string);
    let comment = attr(node, "comment").map(str::to_string);
    let content_types = attr(node, "contentType")
        .map(ContentType::parse_list)
        .unwrap_or_default();
    let loaded = attr_bool(node, "loaded", true);

    let channel = child(node, "Channel").map(parse_channel);
    let children = children(node, "Track").map(parse_track).collect();

    Track {
        id,
        name,
        color,
        comment,
        content_types,
        loaded,
        channel,
        children,
    }
}

fn parse_channel(node: Node<'_, '_>) -> Channel {
    let id = attr(node, "id").unwrap_or("").to_string();
    let role = attr(node, "role")
        .map(ChannelRole::from_str)
        .unwrap_or(ChannelRole::Regular);
    let audio_channels = attr_u32(node, "audioChannels", 2);
    let destination = attr(node, "destination").map(str::to_string);
    let blend_mode = attr(node, "blendMode").map(str::to_string);
    let solo = attr_bool(node, "solo", false);

    let volume = child(node, "Volume")
        .map(|v| attr_f64(v, "value", 1.0))
        .unwrap_or(1.0);
    let pan = child(node, "Pan")
        .map(|p| attr_f64(p, "value", 0.0))
        .unwrap_or(0.0);
    let muted = child(node, "Mute")
        .map(|m| attr_bool(m, "value", false))
        .unwrap_or(false);

    let sends = child(node, "Sends").map(parse_sends).unwrap_or_default();
    let devices = child(node, "Devices")
        .map(devices::parse_devices)
        .unwrap_or_default();

    Channel {
        id,
        role,
        audio_channels,
        destination,
        blend_mode,
        volume,
        pan,
        muted,
        solo,
        sends,
        devices,
    }
}

fn parse_sends(sends_node: Node<'_, '_>) -> Vec<Send> {
    children(sends_node, "Send")
        .map(|send| {
            // Spec uses "destination" (IDREF); we also accept legacy "target"
            let destination = attr(send, "destination")
                .or_else(|| attr(send, "target"))
                .unwrap_or("")
                .to_string();
            let volume = child(send, "Volume")
                .map(|v| attr_f64(v, "value", 1.0))
                .unwrap_or_else(|| attr_f64(send, "volume", 1.0));
            let pan = child(send, "Pan")
                .map(|p| attr_f64(p, "value", 0.0))
                .unwrap_or(0.0);
            let enabled = child(send, "Enable")
                .map(|e| attr_bool(e, "value", true))
                .unwrap_or(true);
            let pre_fader = matches!(attr(send, "type"), Some("pre"));
            Send {
                destination,
                volume,
                pan,
                enabled,
                pre_fader,
            }
        })
        .collect()
}

// ─── Device parameters ───────────────────────────────────────────────────────

pub fn parse_device_parameters(params_node: Node<'_, '_>) -> Vec<DeviceParameter> {
    child_elements(params_node)
        .filter_map(parse_device_parameter)
        .collect()
}

fn parse_device_parameter(node: Node<'_, '_>) -> Option<DeviceParameter> {
    let id = attr(node, "parameterID")?.to_string();
    let name = attr(node, "name").map(str::to_string);

    let value = match node.tag_name().name() {
        "RealParameter" => {
            let value = attr_f64(node, "value", 0.0);
            let min = attr(node, "min").and_then(|v| v.parse().ok());
            let max = attr(node, "max").and_then(|v| v.parse().ok());
            let unit = attr(node, "unit").and_then(AutomationUnit::from_str);
            DeviceParameterValue::Real {
                value,
                min,
                max,
                unit,
            }
        }
        "BoolParameter" => DeviceParameterValue::Bool(attr_bool(node, "value", false)),
        "IntegerParameter" => {
            let value = attr(node, "value")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let min = attr(node, "min").and_then(|v| v.parse().ok());
            let max = attr(node, "max").and_then(|v| v.parse().ok());
            DeviceParameterValue::Integer { value, min, max }
        }
        "EnumParameter" => {
            let value = attr_u32(node, "value", 0);
            let count = attr_u32(node, "count", 0);
            let labels = children(node, "label")
                .filter_map(|l| l.text().map(str::to_string))
                .collect();
            DeviceParameterValue::Enum {
                value,
                count,
                labels,
            }
        }
        "TimeSignatureParameter" => {
            let numerator = attr_u8(node, "numerator", 4);
            let denominator = attr_u8(node, "denominator", 4);
            DeviceParameterValue::TimeSignature {
                numerator,
                denominator,
            }
        }
        _ => return None,
    };

    Some(DeviceParameter { id, name, value })
}

pub fn parse_device_role(node: Node<'_, '_>) -> Option<DeviceRole> {
    attr(node, "deviceRole").and_then(DeviceRole::from_str)
}
