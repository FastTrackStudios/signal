//! Parser and serializer for REAPER plugin preset files.

mod parse;
mod serialize;
mod types;

pub use parse::ParseError;
pub use types::*;

impl PluginPresets {
    /// Parse the contents of a plugin preset `.ini` file.
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        parse::parse(input)
    }

    /// Serialize back to the `.ini` file format.
    pub fn serialize(&self) -> String {
        serialize::serialize(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
[General]\n\
LastDefImpTime=1700000000\n\
NbPresets=2\n\
\n\
[0]\n\
Name=Clean\n\
Len=4\n\
Data=deadbeef\n\
\n\
[1]\n\
Name=Warm\n\
Len=2\n\
Data=cafe\n";

    #[test]
    fn parse_general_section() {
        let p = PluginPresets::parse(SAMPLE).unwrap();
        assert_eq!(p.last_def_imp_time, 1700000000);
        assert_eq!(p.nb_presets, 2);
    }

    #[test]
    fn parse_preset_entries() {
        let p = PluginPresets::parse(SAMPLE).unwrap();
        assert_eq!(p.presets.len(), 2);
        assert_eq!(p.presets[0].name, "Clean");
        assert_eq!(p.presets[0].len, 4);
        assert_eq!(p.presets[0].data, "deadbeef");
        assert_eq!(p.presets[1].name, "Warm");
        assert_eq!(p.presets[1].data, "cafe");
    }

    #[test]
    fn round_trip() {
        let p = PluginPresets::parse(SAMPLE).unwrap();
        let out = p.serialize();
        let p2 = PluginPresets::parse(&out).unwrap();
        assert_eq!(p.presets.len(), p2.presets.len());
        assert_eq!(p.presets[0].name, p2.presets[0].name);
        assert_eq!(p.presets[1].data, p2.presets[1].data);
    }

    #[test]
    fn empty_presets() {
        let input = "[General]\nLastDefImpTime=0\nNbPresets=0\n";
        let p = PluginPresets::parse(input).unwrap();
        assert!(p.presets.is_empty());
    }

    #[test]
    fn serialize_output_format() {
        let input =
            "[General]\nLastDefImpTime=0\nNbPresets=1\n\n[0]\nName=Init\nLen=2\nData=0000\n";
        let p = PluginPresets::parse(input).unwrap();
        let out = p.serialize();
        assert!(out.contains("[General]"));
        assert!(out.contains("NbPresets=1"));
        assert!(out.contains("[0]"));
        assert!(out.contains("Name=Init"));
        assert!(out.contains("Data=0000"));
    }
}
