//! Parser and serializer for REAPER render preset (`reaper-render.ini`) files.
//!
//! This module can parse and round-trip the four entry types found in REAPER's
//! render preset configuration file:
//!
//! - **`<RENDERPRESET>`** — primary format/quality settings for a named preset
//! - **`<RENDERPRESET2>`** — secondary output format for a named preset
//! - **`RENDERPRESET_OUTPUT`** — output path and bounds settings
//! - **`RENDERPRESET_EXT`** — normalization, fade and silence-trim settings

mod parse;
mod serialize;
mod types;

pub use parse::ParseError;
pub use types::*;

impl RenderPresetFile {
    /// Parse the contents of a `reaper-render.ini` file.
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        parse::parse(input)
    }

    /// Return the number of entries in this file.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Return `true` if this file has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate over all entries.
    pub fn iter(&self) -> impl Iterator<Item = &RenderPresetEntry> {
        self.entries.iter()
    }

    /// Iterate over `<RENDERPRESET>` entries only.
    pub fn presets(&self) -> impl Iterator<Item = &RenderPreset> {
        self.entries.iter().filter_map(|e| match e {
            RenderPresetEntry::Preset(p) => Some(p),
            _ => None,
        })
    }

    /// Iterate over `<RENDERPRESET2>` entries only.
    pub fn presets2(&self) -> impl Iterator<Item = &RenderPreset2> {
        self.entries.iter().filter_map(|e| match e {
            RenderPresetEntry::Preset2(p) => Some(p),
            _ => None,
        })
    }

    /// Iterate over `RENDERPRESET_OUTPUT` entries only.
    pub fn outputs(&self) -> impl Iterator<Item = &RenderPresetOutput> {
        self.entries.iter().filter_map(|e| match e {
            RenderPresetEntry::Output(o) => Some(o),
            _ => None,
        })
    }

    /// Iterate over `RENDERPRESET_EXT` entries only.
    pub fn exts(&self) -> impl Iterator<Item = &RenderPresetExt> {
        self.entries.iter().filter_map(|e| match e {
            RenderPresetEntry::Ext(e) => Some(e),
            _ => None,
        })
    }
}

// ---------------------------------------------------------------------------
// Integration / round-trip tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- Round-trip: single RENDERPRESET block ------------------------------

    #[test]
    fn round_trip_renderpreset_unquoted() {
        let input = "<RENDERPRESET MyPreset 44100 2 0 0 3 0 0\r\n  SGFsbG8=\r\n>\r\n";
        let f = RenderPresetFile::parse(input).unwrap();
        assert_eq!(f.serialize(), input);
    }

    #[test]
    fn round_trip_renderpreset_quoted_name() {
        let input = "<RENDERPRESET \"My Preset\" 48000 1 2 1 0 5 4\r\n  YWJj\r\n>\r\n";
        let f = RenderPresetFile::parse(input).unwrap();
        assert_eq!(f.serialize(), input);
    }

    // --- Round-trip: RENDERPRESET2 block ------------------------------------

    #[test]
    fn round_trip_renderpreset2() {
        let input = "<RENDERPRESET2 Secondary\r\n  bXAz\r\n>\r\n";
        let f = RenderPresetFile::parse(input).unwrap();
        assert_eq!(f.serialize(), input);
    }

    #[test]
    fn round_trip_renderpreset2_quoted() {
        let input = "<RENDERPRESET2 \"My Secondary\"\r\n  bXAz\r\n>\r\n";
        let f = RenderPresetFile::parse(input).unwrap();
        assert_eq!(f.serialize(), input);
    }

    // --- Round-trip: RENDERPRESET_OUTPUT ------------------------------------

    #[test]
    fn round_trip_output_bare() {
        let input = "RENDERPRESET_OUTPUT MyPreset 1 0 0 0 0 $project 0 \"\" 1000\r\n";
        let f = RenderPresetFile::parse(input).unwrap();
        assert_eq!(f.serialize(), input);
    }

    #[test]
    fn round_trip_output_quoted_name_and_filename() {
        let input = "RENDERPRESET_OUTPUT \"My Preset\" 0 1.5 10.25 1 0 \"my output\" 1 /home/user/audio 500\r\n";
        let f = RenderPresetFile::parse(input).unwrap();
        assert_eq!(f.serialize(), input);
    }

    // --- Round-trip: RENDERPRESET_EXT ---------------------------------------

    #[test]
    fn round_trip_ext_basic() {
        // Use exact integer-looking floats so format_f64 is stable.
        let input = "RENDERPRESET_EXT MyPreset 0 1 1 0 0 0 0 0 0 0 0\r\n";
        let f = RenderPresetFile::parse(input).unwrap();
        assert_eq!(f.serialize(), input);
    }

    #[test]
    fn round_trip_ext_quoted_name() {
        let input = "RENDERPRESET_EXT \"My Ext\" 65 1 1 0 0 0 0 0 0 0 0\r\n";
        let f = RenderPresetFile::parse(input).unwrap();
        assert_eq!(f.serialize(), input);
    }

    // --- Round-trip: full "All settings" preset block -----------------------

    #[test]
    fn round_trip_all_settings() {
        let input = concat!(
            "<RENDERPRESET Stereo 44100 2 0 0 3 0 0\r\n",
            "  SGFsbG8=\r\n",
            ">\r\n",
            "<RENDERPRESET2 Stereo\r\n",
            "  bXAz\r\n",
            ">\r\n",
            "RENDERPRESET_OUTPUT Stereo 1 0 0 0 0 $project 0 \"\" 1000\r\n",
            "RENDERPRESET_EXT Stereo 0 1 1 0 0 0 0 0 0 0 0\r\n",
        );
        let f = RenderPresetFile::parse(input).unwrap();
        assert_eq!(f.entries.len(), 4);
        assert_eq!(f.serialize(), input);
    }

    #[test]
    fn round_trip_all_settings_quoted_name() {
        let input = concat!(
            "<RENDERPRESET \"My Stereo\" 44100 2 0 0 3 0 0\r\n",
            "  SGFsbG8=\r\n",
            ">\r\n",
            "<RENDERPRESET2 \"My Stereo\"\r\n",
            "  bXAz\r\n",
            ">\r\n",
            "RENDERPRESET_OUTPUT \"My Stereo\" 1 0 0 0 0 $project 0 \"\" 1000\r\n",
            "RENDERPRESET_EXT \"My Stereo\" 0 1 1 0 0 0 0 0 0 0 0\r\n",
        );
        let f = RenderPresetFile::parse(input).unwrap();
        assert_eq!(f.entries.len(), 4);
        assert_eq!(f.serialize(), input);
    }

    // --- Convenience iterator tests -----------------------------------------

    #[test]
    fn convenience_iterators() {
        let input = concat!(
            "<RENDERPRESET A 44100 2 0 0 3 0 0\r\n",
            "  SGFsbG8=\r\n",
            ">\r\n",
            "<RENDERPRESET B 48000 2 0 0 3 0 0\r\n",
            "  SGFsbG8=\r\n",
            ">\r\n",
            "<RENDERPRESET2 A\r\n",
            "  bXAz\r\n",
            ">\r\n",
            "RENDERPRESET_OUTPUT A 1 0 0 0 0 $project 0 \"\" 1000\r\n",
            "RENDERPRESET_EXT A 0 1 1 0 0 0 0 0 0 0 0\r\n",
        );
        let f = RenderPresetFile::parse(input).unwrap();
        assert_eq!(f.len(), 5);
        assert!(!f.is_empty());
        assert_eq!(f.presets().count(), 2);
        assert_eq!(f.presets2().count(), 1);
        assert_eq!(f.outputs().count(), 1);
        assert_eq!(f.exts().count(), 1);
    }

    // --- Empty / whitespace-only input --------------------------------------

    #[test]
    fn empty_input() {
        let f = RenderPresetFile::parse("").unwrap();
        assert!(f.is_empty());
        assert_eq!(f.serialize(), "");
    }

    #[test]
    fn whitespace_only_input() {
        let f = RenderPresetFile::parse("\r\n\r\n  \r\n").unwrap();
        assert!(f.is_empty());
    }

    // --- Error propagation --------------------------------------------------

    #[test]
    fn error_on_unterminated_block() {
        let input = "<RENDERPRESET MyPreset 44100 2 0 0 3 0 0\r\n  SGFsbG8=\r\n";
        assert!(RenderPresetFile::parse(input).is_err());
    }

    // --- Various flags / field values ---------------------------------------

    #[test]
    fn various_checkboxes_flags() {
        let input = "<RENDERPRESET Flags 44100 2 0 0 0 15 524292\r\n  abc\r\n>\r\n";
        let f = RenderPresetFile::parse(input).unwrap();
        let p = f.presets().next().unwrap();
        // various_checkboxes: 15 = dither master + noise shape master + dither stems + noise shape stems
        assert_eq!(p.various_checkboxes, 15);
        // various_checkboxes2: 524292 = multichannel (4) + parallel render (524288)
        assert_eq!(p.various_checkboxes2, 524292);
    }

    #[test]
    fn offline_mode_values() {
        for mode in 0u32..=4 {
            let input = format!("<RENDERPRESET P 44100 2 {mode} 0 0 0 0\r\n  x\r\n>\r\n");
            let f = RenderPresetFile::parse(&input).unwrap();
            let p = f.presets().next().unwrap();
            assert_eq!(p.offline_mode, mode);
        }
    }

    #[test]
    fn resample_mode_values() {
        for rm in 0u32..=9 {
            let input = format!("<RENDERPRESET P 44100 2 0 0 {rm} 0 0\r\n  x\r\n>\r\n");
            let f = RenderPresetFile::parse(&input).unwrap();
            let p = f.presets().next().unwrap();
            assert_eq!(p.resample_mode, rm);
        }
    }

    #[test]
    fn output_bounds_values() {
        for bounds in [0u32, 1, 2, 3, 5] {
            let input = format!("RENDERPRESET_OUTPUT P {bounds} 0 0 0 0 out 0 \"\" 0\r\n");
            let f = RenderPresetFile::parse(&input).unwrap();
            let o = f.outputs().next().unwrap();
            assert_eq!(o.bounds, bounds);
        }
    }

    #[test]
    fn output_source_values() {
        for source in [0u32, 1, 3, 8, 32, 64, 128, 4096] {
            let input = format!("RENDERPRESET_OUTPUT P 1 0 0 {source} 0 out 0 \"\" 0\r\n");
            let f = RenderPresetFile::parse(&input).unwrap();
            let o = f.outputs().next().unwrap();
            assert_eq!(o.source, source);
        }
    }

    #[test]
    fn multiple_presets_independent() {
        let input = concat!(
            "<RENDERPRESET PresetA 44100 2 0 0 3 0 0\r\n",
            "  AAAA\r\n",
            ">\r\n",
            "<RENDERPRESET PresetB 96000 6 1 0 8 5 0\r\n",
            "  BBBB\r\n",
            ">\r\n",
        );
        let f = RenderPresetFile::parse(input).unwrap();
        let ps: Vec<_> = f.presets().collect();
        assert_eq!(ps.len(), 2);
        assert_eq!(ps[0].name, "PresetA");
        assert_eq!(ps[0].sample_rate, 44100);
        assert_eq!(ps[1].name, "PresetB");
        assert_eq!(ps[1].sample_rate, 96000);
        assert_eq!(ps[1].channels, 6);
    }
}
