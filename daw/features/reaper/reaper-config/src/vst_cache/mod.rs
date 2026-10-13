//! Parser and serializer for REAPER VST/DX plugin cache files
//! (`reaper-vstplugins*.ini`).

mod parse;
mod serialize;
mod types;

pub use parse::ParseError;
pub use types::*;

impl VstPluginsCache {
    /// Parse the contents of a `reaper-vstplugins*.ini` file.
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
[vstcache]\n\
C:\\Plugins\\Synth.dll=1700000000,1234ABCD,My Synth!!!vsti\n\
C:\\Plugins\\Effect.dll=1700000001,DEADBEEF,Some Effect\n";

    #[test]
    fn parse_entries() {
        let cache = VstPluginsCache::parse(SAMPLE).unwrap();
        assert_eq!(cache.entries.len(), 2);

        let synth = &cache.entries[0];
        assert_eq!(synth.filename, "C:\\Plugins\\Synth.dll");
        assert_eq!(synth.timestamp, 1700000000);
        assert_eq!(synth.vst_id, "1234ABCD");
        assert_eq!(synth.name, "My Synth");
        assert!(synth.is_instrument);

        let effect = &cache.entries[1];
        assert_eq!(effect.filename, "C:\\Plugins\\Effect.dll");
        assert_eq!(effect.name, "Some Effect");
        assert!(!effect.is_instrument);
    }

    #[test]
    fn round_trip() {
        let cache = VstPluginsCache::parse(SAMPLE).unwrap();
        let out = cache.serialize();
        let cache2 = VstPluginsCache::parse(&out).unwrap();
        assert_eq!(cache.entries.len(), cache2.entries.len());
        assert_eq!(cache.entries[0].name, cache2.entries[0].name);
        assert_eq!(
            cache.entries[0].is_instrument,
            cache2.entries[0].is_instrument
        );
        assert_eq!(cache.entries[1].name, cache2.entries[1].name);
    }

    #[test]
    fn empty_cache() {
        let input = "[vstcache]\n";
        let cache = VstPluginsCache::parse(input).unwrap();
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn serialize_instrument_suffix() {
        let cache = VstPluginsCache {
            entries: vec![VstPlugin {
                filename: "Synth.dll".into(),
                timestamp: 0,
                vst_id: "00000001".into(),
                name: "MySynth".into(),
                is_instrument: true,
            }],
        };
        let out = cache.serialize();
        assert!(out.contains("!!!vsti"));
    }

    #[test]
    fn serialize_no_instrument_suffix() {
        let cache = VstPluginsCache {
            entries: vec![VstPlugin {
                filename: "Effect.dll".into(),
                timestamp: 0,
                vst_id: "00000002".into(),
                name: "MyEffect".into(),
                is_instrument: false,
            }],
        };
        let out = cache.serialize();
        assert!(!out.contains("!!!vsti"));
    }
}
