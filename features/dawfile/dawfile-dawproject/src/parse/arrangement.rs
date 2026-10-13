//! Parse the arrangement timeline, tempo automation, and time signature automation.

use super::{clips, xml_helpers::*};
use crate::types::{
    Arrangement, AutomationPoint, AutomationPoints, AutomationTarget, AutomationUnit,
    ExpressionType, Interpolation, Lane, LaneContent, Marker, TempoPoint, TimeSignaturePoint,
    TimeUnit,
};
use roxmltree::Node;

/// Parse an `<Arrangement>` element.
pub fn parse_arrangement(node: Node<'_, '_>) -> Arrangement {
    let id = attr(node, "id").unwrap_or("").to_string();
    let name = attr(node, "name").map(str::to_string);
    let color = attr(node, "color").map(str::to_string);
    let comment = attr(node, "comment").map(str::to_string);
    let time_unit = attr(node, "timeUnit")
        .map(TimeUnit::from_str)
        .unwrap_or(TimeUnit::Beats);

    let lanes = child(node, "Lanes")
        .map(|n| parse_lanes(n, time_unit))
        .unwrap_or_default();

    let markers = child(node, "Markers")
        .map(|m| children(m, "Marker").map(clips::parse_marker).collect())
        .unwrap_or_default();

    let tempo_automation = child(node, "TempoAutomation")
        .map(parse_tempo_automation)
        .unwrap_or_default();

    let time_sig_automation = child(node, "TimeSignatureAutomation")
        .map(parse_time_sig_automation)
        .unwrap_or_default();

    Arrangement {
        id,
        name,
        color,
        comment,
        time_unit,
        lanes,
        markers,
        tempo_automation,
        time_sig_automation,
    }
}

// ─── Tempo / time-sig automation ─────────────────────────────────────────────

fn parse_tempo_automation(node: Node<'_, '_>) -> Vec<TempoPoint> {
    // TempoAutomation is a <Points> element with RealPoints at bpm unit
    child_elements(node)
        .filter(|n| {
            let tag = n.tag_name().name();
            tag == "RealPoint" || tag == "Points"
        })
        .flat_map(|n| {
            if n.tag_name().name() == "Points" {
                // nested Points element
                child_elements(n)
                    .filter(|p| p.tag_name().name() == "RealPoint")
                    .map(|p| TempoPoint {
                        time: attr_f64(p, "time", 0.0),
                        bpm: attr(p, "value")
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(120.0),
                        interpolation: attr(p, "interpolation")
                            .map(Interpolation::from_str)
                            .unwrap_or(Interpolation::Hold),
                    })
                    .collect::<Vec<_>>()
            } else {
                vec![TempoPoint {
                    time: attr_f64(n, "time", 0.0),
                    bpm: attr(n, "value")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(120.0),
                    interpolation: attr(n, "interpolation")
                        .map(Interpolation::from_str)
                        .unwrap_or(Interpolation::Hold),
                }]
            }
        })
        .collect()
}

fn parse_time_sig_automation(node: Node<'_, '_>) -> Vec<TimeSignaturePoint> {
    child_elements(node)
        .filter(|n| {
            let tag = n.tag_name().name();
            tag == "TimeSignaturePoint" || tag == "Points"
        })
        .flat_map(|n| {
            if n.tag_name().name() == "Points" {
                child_elements(n)
                    .filter(|p| p.tag_name().name() == "TimeSignaturePoint")
                    .map(parse_time_sig_point)
                    .collect::<Vec<_>>()
            } else {
                vec![parse_time_sig_point(n)]
            }
        })
        .collect()
}

fn parse_time_sig_point(node: Node<'_, '_>) -> TimeSignaturePoint {
    TimeSignaturePoint {
        time: attr_f64(node, "time", 0.0),
        numerator: attr_u8(node, "numerator", 4),
        denominator: attr_u8(node, "denominator", 4),
    }
}

// ─── Lanes ───────────────────────────────────────────────────────────────────

fn parse_lanes(node: Node<'_, '_>, parent_unit: TimeUnit) -> Vec<Lane> {
    child_elements(node)
        .filter_map(|child| parse_lane(child, parent_unit))
        .collect()
}

fn parse_lane(node: Node<'_, '_>, parent_unit: TimeUnit) -> Option<Lane> {
    let tag = node.tag_name().name();
    let id = attr(node, "id").unwrap_or("").to_string();
    let track = attr(node, "track").unwrap_or("").to_string();
    let time_unit = attr(node, "timeUnit").map(TimeUnit::from_str);
    let effective_unit = time_unit.unwrap_or(parent_unit);

    let content = match tag {
        // Nested Lanes — contains per-track lane groups; flatten one level
        "Lanes" => {
            let sub_lanes = parse_lanes(node, effective_unit);
            if let Some(first) = sub_lanes.into_iter().next() {
                return Some(Lane {
                    id,
                    track,
                    time_unit,
                    content: first.content,
                });
            }
            return None;
        }
        "Clips" => LaneContent::Clips(clips::parse_clips(node)),
        "Notes" => LaneContent::Notes(clips::parse_notes(node)),
        // The spec calls automation lanes "<Points>" not "<AutomationLane>"
        "Points" => LaneContent::Automation(parse_points(node)),
        "Markers" => {
            let markers = children(node, "Marker").map(clips::parse_marker).collect();
            LaneContent::Markers(markers)
        }
        _ => return None,
    };

    Some(Lane {
        id,
        track,
        time_unit,
        content,
    })
}

// ─── Automation Points ────────────────────────────────────────────────────────

fn parse_points(node: Node<'_, '_>) -> AutomationPoints {
    let id = attr(node, "id").unwrap_or("").to_string();
    let unit = attr(node, "unit").and_then(AutomationUnit::from_str);

    let target = child(node, "Target").map(parse_target).unwrap_or_default();

    let points = child_elements(node)
        .filter_map(parse_automation_point)
        .collect();

    AutomationPoints {
        id,
        target,
        unit,
        points,
    }
}

fn parse_target(node: Node<'_, '_>) -> AutomationTarget {
    let parameter = attr(node, "parameter").map(str::to_string);
    let expression = attr(node, "expression").and_then(ExpressionType::from_str);
    let channel = attr(node, "channel").and_then(|v| v.parse().ok());
    let key = attr(node, "key").and_then(|v| v.parse().ok());
    let controller = attr(node, "controller").and_then(|v| v.parse().ok());
    AutomationTarget {
        parameter,
        expression,
        channel,
        key,
        controller,
    }
}

fn parse_automation_point(node: Node<'_, '_>) -> Option<AutomationPoint> {
    let tag = node.tag_name().name();
    // All point types share time and (optionally) interpolation; value type varies.
    // We normalise everything to f64.
    let time = attr_f64(node, "time", 0.0);
    let interpolation = attr(node, "interpolation")
        .map(Interpolation::from_str)
        .unwrap_or(Interpolation::Hold);

    let value: f64 = match tag {
        "RealPoint" => attr(node, "value")?.parse().ok()?,
        "BoolPoint" => {
            if attr_bool(node, "value", false) {
                1.0
            } else {
                0.0
            }
        }
        "IntegerPoint" | "EnumPoint" => attr(node, "value")?.parse::<i64>().ok()? as f64,
        // TimeSignaturePoint encodes numerator in value for generic use
        "TimeSignaturePoint" => attr_u8(node, "numerator", 4) as f64,
        _ => return None,
    };

    Some(AutomationPoint {
        time,
        value,
        interpolation,
    })
}
