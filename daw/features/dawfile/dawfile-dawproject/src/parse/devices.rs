//! Parse device (plugin) chains from channel elements.

use super::{tracks::parse_device_parameters, xml_helpers::*};
use crate::types::{
    BuiltinDeviceContent, CompressorParams, Device, DeviceFormat, DeviceState, EqBand, EqBandType,
    LimiterParams, NoiseGateParams,
};
use roxmltree::Node;

/// Parse all devices from a `<Devices>` element.
pub fn parse_devices(devices_node: Node<'_, '_>) -> Vec<Device> {
    child_elements(devices_node)
        .filter_map(parse_device)
        .collect()
}

fn parse_device(node: Node<'_, '_>) -> Option<Device> {
    let tag = node.tag_name().name();
    let format = match tag {
        "Vst2Plugin" => DeviceFormat::Vst2,
        "Vst3Plugin" => DeviceFormat::Vst3,
        "ClapPlugin" => DeviceFormat::Clap,
        "AuPlugin" => DeviceFormat::Au,
        "BuiltinDevice" => DeviceFormat::Builtin,
        "Equalizer" => DeviceFormat::Equalizer,
        "Compressor" => DeviceFormat::Compressor,
        "Limiter" => DeviceFormat::Limiter,
        "NoiseGate" => DeviceFormat::NoiseGate,
        _ => return None,
    };

    let name = attr(node, "deviceName")
        .or_else(|| attr(node, "name"))
        .unwrap_or("")
        .to_string();

    // Enabled: prefer explicit <Enabled> child element; fall back to inverted `bypass` attr.
    let enabled = child(node, "Enabled")
        .map(|e| attr_bool(e, "value", true))
        .unwrap_or_else(|| !attr_bool(node, "bypass", false));

    let loaded = attr_bool(node, "loaded", true);
    let device_role = super::tracks::parse_device_role(node);

    let plugin_id = attr(node, "pluginId").map(str::to_string);
    let vendor = attr(node, "deviceVendor")
        .or_else(|| attr(node, "vendorName"))
        .map(str::to_string);
    let plugin_version = attr(node, "pluginVersion").map(str::to_string);
    let device_id = attr(node, "deviceID").map(str::to_string);

    let plugin_path = attr(node, "deviceFile").map(std::path::PathBuf::from);

    let parameters = child(node, "Parameters")
        .map(parse_device_parameters)
        .unwrap_or_default();

    let builtin_content = parse_builtin_content(node, &format);

    let state = child(node, "State").and_then(|s| {
        if let Some(file) = attr(s, "file") {
            Some(DeviceState::File(file.to_string()))
        } else if let Some(text) = s.text() {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                Some(DeviceState::Base64(trimmed.to_string()))
            } else {
                None
            }
        } else {
            None
        }
    });

    Some(Device {
        name,
        format,
        device_role,
        plugin_id,
        vendor,
        plugin_version,
        device_id,
        plugin_path,
        enabled,
        loaded,
        parameters,
        builtin_content,
        state,
    })
}

// ─── Built-in device structured content ──────────────────────────────────────

fn parse_builtin_content(node: Node<'_, '_>, format: &DeviceFormat) -> BuiltinDeviceContent {
    match format {
        DeviceFormat::Equalizer => {
            let bands = children(node, "Band").map(parse_eq_band).collect();
            BuiltinDeviceContent::Equalizer(bands)
        }
        DeviceFormat::Compressor => BuiltinDeviceContent::Compressor(CompressorParams {
            threshold: parse_param_value(node, "Threshold"),
            ratio: parse_param_value(node, "Ratio"),
            attack: parse_param_value(node, "Attack"),
            release: parse_param_value(node, "Release"),
            input_gain: parse_param_value(node, "InputGain"),
            output_gain: parse_param_value(node, "OutputGain"),
            auto_makeup: child(node, "AutoMakeup").map(|n| attr_bool(n, "value", false)),
        }),
        DeviceFormat::Limiter => BuiltinDeviceContent::Limiter(LimiterParams {
            threshold: parse_param_value(node, "Threshold"),
            attack: parse_param_value(node, "Attack"),
            release: parse_param_value(node, "Release"),
            input_gain: parse_param_value(node, "InputGain"),
            output_gain: parse_param_value(node, "OutputGain"),
        }),
        DeviceFormat::NoiseGate => BuiltinDeviceContent::NoiseGate(NoiseGateParams {
            threshold: parse_param_value(node, "Threshold"),
            range: parse_param_value(node, "Range"),
            ratio: parse_param_value(node, "Ratio"),
            attack: parse_param_value(node, "Attack"),
            release: parse_param_value(node, "Release"),
        }),
        _ => BuiltinDeviceContent::None,
    }
}

/// Extract the `value` attribute from a named child element.
fn parse_param_value(parent: Node<'_, '_>, child_name: &str) -> Option<f64> {
    child(parent, child_name)
        .and_then(|n| attr(n, "value"))
        .and_then(|v| v.parse().ok())
}

fn parse_eq_band(node: Node<'_, '_>) -> EqBand {
    let id = attr(node, "id").unwrap_or("").to_string();
    let band_type = attr(node, "type")
        .and_then(EqBandType::from_str)
        .unwrap_or(EqBandType::Bell);
    let order = attr(node, "order").and_then(|v| v.parse().ok());
    let freq = parse_param_value(node, "Freq");
    let gain = parse_param_value(node, "Gain");
    let q = parse_param_value(node, "Q");
    let enabled = child(node, "Enabled")
        .map(|n| attr_bool(n, "value", true))
        .unwrap_or(true);
    EqBand {
        id,
        band_type,
        order,
        freq,
        gain,
        q,
        enabled,
    }
}
