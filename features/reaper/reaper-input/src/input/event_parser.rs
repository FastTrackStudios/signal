//! Event Parser
//!
//! Parses REAPER action binding event format strings into structured events.
//! Supports:
//! - midi:XX[:YY] - MIDI events (one or two bytes hex)
//! - [wheel|hwheel|mtvert|mthorz|mtzoom|mtrot|mediakbd]:flags - Mouse/touch events
//! - key:flags:keycode - Key events
//! - osc:/msg[:f=FloatValue|:s=StringValue] - OSC messages
//! - KBD_OnMainActionEx - Action execution

use std::str::FromStr;

/// Represents a parsed input event
#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    /// MIDI event: midi:XX[:YY]
    /// XX is the first byte (required), YY is optional second byte
    Midi { byte1: u8, byte2: Option<u8> },
    /// Mouse/touch wheel event: [wheel|hwheel|mtvert|mthorz|mtzoom|mtrot|mediakbd]:flags
    Wheel { wheel_type: WheelType, flags: u32 },
    /// Key event: key:flags:keycode
    Key { flags: u32, keycode: u32 },
    /// OSC message: osc:/msg[:f=FloatValue|:s=StringValue]
    Osc {
        path: String,
        float_value: Option<f32>,
        string_value: Option<String>,
    },
    /// Action execution: KBD_OnMainActionEx
    Action(String),
}

/// Types of wheel/touch events
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WheelType {
    /// Vertical mouse wheel
    Wheel,
    /// Horizontal mouse wheel
    Hwheel,
    /// Vertical touch/multitouch
    MtVert,
    /// Horizontal touch/multitouch
    MtHorz,
    /// Multitouch zoom
    MtZoom,
    /// Multitouch rotation
    MtRot,
    /// Media keyboard
    MediaKbd,
}

impl FromStr for WheelType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "wheel" => Ok(WheelType::Wheel),
            "hwheel" => Ok(WheelType::Hwheel),
            "mtvert" => Ok(WheelType::MtVert),
            "mthorz" => Ok(WheelType::MtHorz),
            "mtzoom" => Ok(WheelType::MtZoom),
            "mtrot" => Ok(WheelType::MtRot),
            "mediakbd" => Ok(WheelType::MediaKbd),
            _ => Err(format!("Unknown wheel type: {}", s)),
        }
    }
}

/// Parse an input event string into an InputEvent
pub fn parse_event(event_str: &str) -> Result<InputEvent, String> {
    let event_str = event_str.trim();

    // Check for MIDI event: midi:XX[:YY]
    if event_str.starts_with("midi:") {
        return parse_midi_event(event_str);
    }

    // Check for wheel event: [wheel|hwheel|...]:flags
    if let Some(wheel_type) = [
        "wheel", "hwheel", "mtvert", "mthorz", "mtzoom", "mtrot", "mediakbd",
    ]
    .iter()
    .find(|&prefix| event_str.starts_with(prefix))
    {
        return parse_wheel_event(event_str, wheel_type);
    }

    // Check for key event: key:flags:keycode
    if event_str.starts_with("key:") {
        return parse_key_event(event_str);
    }

    // Check for OSC event: osc:/msg[:f=FloatValue|:s=StringValue]
    if event_str.starts_with("osc:") {
        return parse_osc_event(event_str);
    }

    // Otherwise, treat as action name (e.g., "KBD_OnMainActionEx")
    Ok(InputEvent::Action(event_str.to_string()))
}

fn parse_midi_event(event_str: &str) -> Result<InputEvent, String> {
    // Format: midi:XX[:YY]
    let parts: Vec<&str> = event_str.split(':').collect();
    if parts.len() < 2 || parts[0] != "midi" {
        return Err("Invalid MIDI event format".to_string());
    }

    // Parse first byte (required)
    let byte1 = u8::from_str_radix(parts[1], 16)
        .map_err(|_| format!("Invalid MIDI byte1: {}", parts[1]))?;

    // Parse second byte (optional)
    let byte2 = if parts.len() > 2 {
        Some(
            u8::from_str_radix(parts[2], 16)
                .map_err(|_| format!("Invalid MIDI byte2: {}", parts[2]))?,
        )
    } else {
        None
    };

    Ok(InputEvent::Midi { byte1, byte2 })
}

fn parse_wheel_event(event_str: &str, _wheel_type_str: &str) -> Result<InputEvent, String> {
    // Format: [wheel|hwheel|...]:flags
    let parts: Vec<&str> = event_str.split(':').collect();
    if parts.len() < 2 {
        return Err("Invalid wheel event format".to_string());
    }

    let wheel_type = WheelType::from_str(parts[0])?;
    let flags = u32::from_str_radix(parts[1], 16)
        .or_else(|_| parts[1].parse::<u32>())
        .map_err(|_| format!("Invalid wheel flags: {}", parts[1]))?;

    Ok(InputEvent::Wheel { wheel_type, flags })
}

fn parse_key_event(event_str: &str) -> Result<InputEvent, String> {
    // Format: key:flags:keycode
    let parts: Vec<&str> = event_str.split(':').collect();
    if parts.len() < 3 || parts[0] != "key" {
        return Err("Invalid key event format".to_string());
    }

    let flags = u32::from_str_radix(parts[1], 16)
        .or_else(|_| parts[1].parse::<u32>())
        .map_err(|_| format!("Invalid key flags: {}", parts[1]))?;

    let keycode = u32::from_str_radix(parts[2], 16)
        .or_else(|_| parts[2].parse::<u32>())
        .map_err(|_| format!("Invalid keycode: {}", parts[2]))?;

    Ok(InputEvent::Key { flags, keycode })
}

fn parse_osc_event(event_str: &str) -> Result<InputEvent, String> {
    // Format: osc:/msg[:f=FloatValue|:s=StringValue]
    let parts: Vec<&str> = event_str.split(':').collect();
    if parts.len() < 2 || parts[0] != "osc" {
        return Err("Invalid OSC event format".to_string());
    }

    let path = parts[1].to_string();

    let mut float_value = None;
    let mut string_value = None;

    // Parse optional parameters
    for part in parts.iter().skip(2) {
        if part.starts_with("f=") {
            let value = part[2..]
                .parse::<f32>()
                .map_err(|_| format!("Invalid OSC float value: {}", part))?;
            float_value = Some(value);
        } else if part.starts_with("s=") {
            string_value = Some(part[2..].to_string());
        }
    }

    Ok(InputEvent::Osc {
        path,
        float_value,
        string_value,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_midi_event() {
        assert_eq!(
            parse_event("midi:90").unwrap(),
            InputEvent::Midi {
                byte1: 0x90,
                byte2: None
            }
        );
        assert_eq!(
            parse_event("midi:90:7F").unwrap(),
            InputEvent::Midi {
                byte1: 0x90,
                byte2: Some(0x7F)
            }
        );
    }

    #[test]
    fn test_parse_wheel_event() {
        assert_eq!(
            parse_event("wheel:0").unwrap(),
            InputEvent::Wheel {
                wheel_type: WheelType::Wheel,
                flags: 0
            }
        );
        assert_eq!(
            parse_event("hwheel:8").unwrap(),
            InputEvent::Wheel {
                wheel_type: WheelType::Hwheel,
                flags: 8
            }
        );
    }

    #[test]
    fn test_parse_key_event() {
        // Test with hex values (0x43 = 67 decimal)
        assert_eq!(
            parse_event("key:8:43").unwrap(),
            InputEvent::Key {
                flags: 8,
                keycode: 67
            }
        );
        // Test with hex values (0x64 = 100 decimal)
        assert_eq!(
            parse_event("key:8:64").unwrap(),
            InputEvent::Key {
                flags: 8,
                keycode: 100
            }
        );
    }

    #[test]
    fn test_parse_osc_event() {
        assert_eq!(
            parse_event("osc:/test").unwrap(),
            InputEvent::Osc {
                path: "/test".to_string(),
                float_value: None,
                string_value: None,
            }
        );
        assert_eq!(
            parse_event("osc:/test:f=1.5").unwrap(),
            InputEvent::Osc {
                path: "/test".to_string(),
                float_value: Some(1.5),
                string_value: None,
            }
        );
        assert_eq!(
            parse_event("osc:/test:s=hello").unwrap(),
            InputEvent::Osc {
                path: "/test".to_string(),
                float_value: None,
                string_value: Some("hello".to_string()),
            }
        );
    }

    #[test]
    fn test_parse_action() {
        assert_eq!(
            parse_event("KBD_OnMainActionEx").unwrap(),
            InputEvent::Action("KBD_OnMainActionEx".to_string())
        );
    }
}
