//! Tempo and time signature parsing.
//!
//! Tempo lives on the master/main track's mixer:
//! - v10+:  `MasterTrack.DeviceChain.Mixer.Tempo.Manual[@Value]`
//! - v12+:  `MainTrack.DeviceChain.Mixer.Tempo.Manual[@Value]`
//! - v8-9:  Falls back to `Tempo.ArrangerAutomation.Events.FloatEvent[@Value]`
//!
//! Time signature uses Ableton's encoded integer format (0-494):
//!   numerator = (encoded % 99) + 1
//!   denominator = 2^(encoded / 99)

use super::xml_helpers::*;
use crate::types::{AutomationPoint, TimeSignature};
use roxmltree::Node;

/// Parse the master tempo from a MasterTrack or MainTrack node.
pub fn parse_tempo(master_node: Node<'_, '_>) -> f64 {
    // Try: DeviceChain.Mixer.Tempo.Manual[@Value]
    if let Some(tempo_node) = descend(master_node, "DeviceChain.Mixer.Tempo") {
        if let Some(bpm) = child_f64(tempo_node, "Manual")
            && (10.0..=999.0).contains(&bpm)
        {
            return bpm;
        }

        // Fallback: ArrangerAutomation.Events first FloatEvent
        if let Some(events) = descend(tempo_node, "ArrangerAutomation.Events") {
            for event in events.children() {
                if event.has_tag_name("FloatEvent")
                    && let Some(bpm) = event.attribute("Value").and_then(|v| v.parse::<f64>().ok())
                    && (10.0..=999.0).contains(&bpm)
                {
                    return bpm;
                }
            }
        }
    }

    120.0 // Ableton default
}

/// Parse tempo automation events from the master track.
///
/// In v10+, automation lives in `AutomationEnvelopes.Envelopes` and is matched
/// to the tempo parameter via the `PointeeId` / `AutomationTarget.Id` link.
pub fn parse_tempo_automation(master_node: Node<'_, '_>) -> Vec<AutomationPoint> {
    let mut points = Vec::new();

    // Get the tempo automation target ID
    let target_id = descend(master_node, "DeviceChain.Mixer.Tempo.AutomationTarget")
        .and_then(|n| n.attribute("Id"))
        .and_then(|v| v.parse::<i32>().ok());

    // First check inline ArrangerAutomation on the Tempo element itself (v8-9)
    if let Some(events) = descend(
        master_node,
        "DeviceChain.Mixer.Tempo.ArrangerAutomation.Events",
    ) {
        collect_float_events(events, &mut points);
    }

    // Then check envelope-based automation (v10+)
    if let Some(target_id) = target_id
        && let Some(envelopes) = descend(master_node, "AutomationEnvelopes.Envelopes")
    {
        for envelope in envelopes.children() {
            if !envelope.has_tag_name("AutomationEnvelope") {
                continue;
            }
            let pointee_id =
                descend(envelope, "EnvelopeTarget").and_then(|n| child_i32(n, "PointeeId"));

            if pointee_id == Some(target_id) {
                if let Some(events) = descend(envelope, "Automation.Events") {
                    collect_float_events(events, &mut points);
                }
                break;
            }
        }
    }

    // Filter out the sentinel event at -63072000 beats (Ableton internal)
    points.retain(|p| p.time > -60_000_000.0);
    points.sort_by(|a, b| {
        a.time
            .partial_cmp(&b.time)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    points
}

fn collect_float_events(events_node: Node<'_, '_>, points: &mut Vec<AutomationPoint>) {
    for event in events_node.children() {
        if event.has_tag_name("FloatEvent")
            && let (Some(time), Some(value)) = (
                event.attribute("Time").and_then(|v| v.parse::<f64>().ok()),
                event.attribute("Value").and_then(|v| v.parse::<f64>().ok()),
            )
        {
            let curve_control_1 = parse_curve_control(&event, "CurveControl1X", "CurveControl1Y");
            let curve_control_2 = parse_curve_control(&event, "CurveControl2X", "CurveControl2Y");
            points.push(AutomationPoint {
                time,
                value,
                curve_control_1,
                curve_control_2,
            });
        }
    }
}

/// Parse a pair of bezier control handle attributes from a FloatEvent.
fn parse_curve_control(event: &Node<'_, '_>, x_attr: &str, y_attr: &str) -> Option<(f64, f64)> {
    let x = event
        .attribute(x_attr)
        .and_then(|v| v.parse::<f64>().ok())?;
    let y = event
        .attribute(y_attr)
        .and_then(|v| v.parse::<f64>().ok())?;
    Some((x, y))
}

/// Parse the time signature from the master track.
///
/// Location: `MasterTrack.DeviceChain.Mixer.TimeSignature.TimeSignatures
///            .RemoteableTimeSignature.Numerator/Denominator`
///
/// Or from encoded `EnumEvent` values (0-494).
pub fn parse_time_signature(master_node: Node<'_, '_>) -> TimeSignature {
    // Try direct Numerator/Denominator (common in v10+)
    if let Some(ts_node) = descend(
        master_node,
        "DeviceChain.Mixer.TimeSignature.TimeSignatures",
    ) {
        for remote_ts in ts_node.children() {
            if remote_ts.has_tag_name("RemoteableTimeSignature")
                && let (Some(num), Some(den)) = (
                    child_i32(remote_ts, "Numerator"),
                    child_i32(remote_ts, "Denominator"),
                )
                && num > 0
                && den > 0
            {
                return TimeSignature {
                    numerator: num as u8,
                    denominator: den as u8,
                };
            }
        }
    }

    // Try encoded EnumEvent format
    if let Some(events) = descend(
        master_node,
        "DeviceChain.Mixer.TimeSignature.AutomationTarget",
    ) {
        // Look for EnumEvent in automation
        if let Some(encoded) = events
            .parent()
            .and_then(|p| descend(p, "ArrangerAutomation.Events"))
        {
            for event in encoded.children() {
                if event.has_tag_name("EnumEvent")
                    && let Some(v) = event.attribute("Value").and_then(|v| v.parse::<u32>().ok())
                {
                    return decode_time_signature(v);
                }
            }
        }
    }

    TimeSignature::default()
}

/// Decode Ableton's encoded time signature integer (0-494).
///
/// Formula:
///   numerator = (encoded % 99) + 1
///   denominator = 2^(encoded / 99)
fn decode_time_signature(encoded: u32) -> TimeSignature {
    let numerator = (encoded % 99) + 1;
    let denominator = 1u32 << (encoded / 99); // 2^(encoded/99)

    TimeSignature {
        numerator: numerator.min(255) as u8,
        denominator: denominator.min(255) as u8,
    }
}
