//! Serialization logic for REAPER plugin preset files.

use super::types::*;

/// Serialize a `PluginPresets` back to the `.ini` file format.
pub fn serialize(p: &PluginPresets) -> String {
    let mut out = String::new();
    out.push_str("[General]\n");
    out.push_str(&format!("LastDefImpTime={}\n", p.last_def_imp_time));
    out.push_str(&format!("NbPresets={}\n", p.nb_presets));
    for (i, preset) in p.presets.iter().enumerate() {
        out.push('\n');
        out.push_str(&format!("[{i}]\n"));
        out.push_str(&format!("Name={}\n", preset.name));
        out.push_str(&format!("Len={}\n", preset.len));
        out.push_str(&format!("Data={}\n", preset.data));
    }
    out
}
