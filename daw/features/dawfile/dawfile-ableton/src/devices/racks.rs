//! Typed parameter models for Ableton rack devices (Drum Rack, Instrument Rack, Audio Effect Rack).

use crate::parse::xml_helpers::*;
use crate::write::xml_writer::AbletonXmlWriter;
use roxmltree::Node;
use std::io::{self, Write};

/// A single macro control knob.
#[derive(Debug, Clone)]
pub struct MacroControl {
    /// Current value (0.0-127.0).
    pub value: f64,
    /// Display name (user-defined label).
    pub name: String,
    /// Default value.
    pub default: f64,
}

impl Default for MacroControl {
    fn default() -> Self {
        Self {
            value: 0.0,
            name: String::new(),
            default: 0.0,
        }
    }
}

/// Shared parameters for all rack device types (Drum Rack, Instrument Rack, Audio Effect Rack).
///
/// Racks share the same macro control structure. The chain/branch structure is
/// handled by the main device parser; these params capture the 16 macro knobs.
#[derive(Debug, Clone, Default)]
pub struct RackParams {
    /// 16 macro control knobs.
    pub macros: [MacroControl; 16],
}

pub fn parse_rack(node: Node<'_, '_>) -> RackParams {
    let mut params = RackParams::default();

    for i in 0..16 {
        // Macro values: <MacroControls.N><Manual Value="X"/></MacroControls.N>
        let value_tag = format!("MacroControls.{i}");
        if let Some(mc) = child(node, &value_tag) {
            params.macros[i].value = child_f64(mc, "Manual").unwrap_or(0.0);
        }

        // Macro display names: <MacroDisplayNames.N Value="Name"/>
        let name_tag = format!("MacroDisplayNames.{i}");
        if let Some(name) = child_value(node, &name_tag) {
            params.macros[i].name = name.to_string();
        }

        // Macro defaults: <MacroDefaults.N Value="X"/>
        let default_tag = format!("MacroDefaults.{i}");
        if let Some(def) = child(node, &default_tag) {
            params.macros[i].default = child_f64(def, "Manual").unwrap_or(0.0);
        }
    }

    params
}

pub fn write_rack<W: Write>(w: &mut AbletonXmlWriter<W>, p: &RackParams) -> io::Result<()> {
    for (i, m) in p.macros.iter().enumerate() {
        let value_tag = format!("MacroControls.{i}");
        w.start(&value_tag)?;
        w.value_float("Manual", m.value)?;
        w.automation_target("AutomationTarget")?;
        w.end(&value_tag)?;

        let name_tag = format!("MacroDisplayNames.{i}");
        w.value_element(&name_tag, &m.name)?;

        let default_tag = format!("MacroDefaults.{i}");
        w.start(&default_tag)?;
        w.value_float("Manual", m.default)?;
        w.automation_target("AutomationTarget")?;
        w.end(&default_tag)?;
    }

    Ok(())
}
