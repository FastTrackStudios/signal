//! Parser and serializer for REAPER default preset assignment files
//! (`reaper-defpresets.ini`).

mod parse;
mod serialize;
mod types;

pub use parse::ParseError;
pub use types::*;

impl DefPresets {
    /// Parse the contents of a `reaper-defpresets.ini` file.
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        parse::parse(input)
    }

    /// Serialize back to the `.ini` file format.
    pub fn serialize(&self) -> String {
        serialize::serialize(self)
    }

    /// Look up the default preset name for a plugin identity string.
    pub fn get(&self, plugin_id: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| e.plugin_id == plugin_id)
            .map(|e| e.preset_name.as_str())
    }

    /// Set (or insert) the default preset for a plugin identity string.
    pub fn set(&mut self, plugin_id: &str, preset_name: &str) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.plugin_id == plugin_id) {
            entry.preset_name = preset_name.to_string();
        } else {
            self.entries.push(DefPresetEntry {
                plugin_id: plugin_id.to_string(),
                preset_name: preset_name.to_string(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
[defpresets]\n\
VST3: My Synth (Vendor)=Init Preset\n\
VST: OldEffect.dll=Clean\n\
JS: ReaEQ/reaeq=Flat\n";

    #[test]
    fn parse_entries() {
        let dp = DefPresets::parse(SAMPLE).unwrap();
        assert_eq!(dp.entries.len(), 3);
        assert_eq!(dp.entries[0].plugin_id, "VST3: My Synth (Vendor)");
        assert_eq!(dp.entries[0].preset_name, "Init Preset");
        assert_eq!(dp.entries[1].plugin_id, "VST: OldEffect.dll");
        assert_eq!(dp.entries[2].plugin_id, "JS: ReaEQ/reaeq");
    }

    #[test]
    fn get_helper() {
        let dp = DefPresets::parse(SAMPLE).unwrap();
        assert_eq!(dp.get("VST: OldEffect.dll"), Some("Clean"));
        assert_eq!(dp.get("JS: ReaEQ/reaeq"), Some("Flat"));
        assert_eq!(dp.get("Missing"), None);
    }

    #[test]
    fn set_new_entry() {
        let mut dp = DefPresets::parse(SAMPLE).unwrap();
        dp.set("AU: NewPlugin", "Default");
        assert_eq!(dp.get("AU: NewPlugin"), Some("Default"));
        assert_eq!(dp.entries.len(), 4);
    }

    #[test]
    fn set_existing_entry() {
        let mut dp = DefPresets::parse(SAMPLE).unwrap();
        dp.set("VST: OldEffect.dll", "Heavy");
        assert_eq!(dp.get("VST: OldEffect.dll"), Some("Heavy"));
        assert_eq!(dp.entries.len(), 3);
    }

    #[test]
    fn round_trip() {
        let dp = DefPresets::parse(SAMPLE).unwrap();
        let out = dp.serialize();
        let dp2 = DefPresets::parse(&out).unwrap();
        assert_eq!(dp.entries.len(), dp2.entries.len());
        for (a, b) in dp.entries.iter().zip(dp2.entries.iter()) {
            assert_eq!(a.plugin_id, b.plugin_id);
            assert_eq!(a.preset_name, b.preset_name);
        }
    }

    #[test]
    fn empty_file() {
        let dp = DefPresets::parse("[defpresets]\n").unwrap();
        assert!(dp.entries.is_empty());
    }

    #[test]
    fn preset_name_with_equals() {
        let input = "[defpresets]\nVST: Eq.dll=A=B\n";
        let dp = DefPresets::parse(input).unwrap();
        assert_eq!(dp.get("VST: Eq.dll"), Some("A=B"));
    }
}
