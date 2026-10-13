//! Parser and serializer for REAPER FX browser folder files
//! (`reaper-fxfolders.ini`).

mod parse;
mod serialize;
mod types;

pub use parse::ParseError;
pub use types::{FxFolder, FxFolderItem, FxFolders, PluginType};

impl FxFolders {
    /// Parse the contents of a `reaper-fxfolders.ini` file.
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        parse::parse(input)
    }

    /// Return the number of folders.
    pub fn len(&self) -> usize {
        self.folders.len()
    }

    /// Return `true` if there are no folders.
    pub fn is_empty(&self) -> bool {
        self.folders.is_empty()
    }

    /// Iterate over all folders.
    pub fn iter(&self) -> impl Iterator<Item = &FxFolder> {
        self.folders.iter()
    }

    /// Look up a folder by display name (case-sensitive, returns the first match).
    pub fn folder_by_name(&self, name: &str) -> Option<&FxFolder> {
        self.folders.iter().find(|f| f.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
[Folders]\r\n\
NbFolders=2\r\n\
Name0=Favorites\r\n\
ID0=0\r\n\
Name1=SmartEQ\r\n\
ID1=1\r\n\
[Folder0]\r\n\
Nb=2\r\n\
Type0=3\r\n\
Item0=VST: ReaComp (Cockos)\r\n\
Type1=2\r\n\
Item1=JS/delay\r\n\
[Folder1]\r\n\
Nb=1\r\n\
Type0=1048576\r\n\
Item0=EQ OR equalizer\r\n";

    #[test]
    fn round_trip_sample() {
        let fx = FxFolders::parse(SAMPLE).unwrap();
        assert_eq!(fx.serialize(), SAMPLE);
    }

    #[test]
    fn round_trip_empty() {
        let fx = FxFolders::default();
        let out = fx.serialize();
        let fx2 = FxFolders::parse(&out).unwrap();
        assert_eq!(fx, fx2);
    }

    #[test]
    fn convenience_len_and_iter() {
        let fx = FxFolders::parse(SAMPLE).unwrap();
        assert_eq!(fx.len(), 2);
        assert!(!fx.is_empty());
        assert_eq!(fx.iter().count(), 2);
    }

    #[test]
    fn folder_by_name_found() {
        let fx = FxFolders::parse(SAMPLE).unwrap();
        let folder = fx.folder_by_name("Favorites").unwrap();
        assert_eq!(folder.name, "Favorites");
        assert_eq!(folder.items.len(), 2);
    }

    #[test]
    fn folder_by_name_not_found() {
        let fx = FxFolders::parse(SAMPLE).unwrap();
        assert!(fx.folder_by_name("NonExistent").is_none());
    }

    #[test]
    fn parse_lf_line_endings() {
        let input = "[Folders]\nNbFolders=1\nName0=Test\nID0=0\n[Folder0]\nNb=0\n";
        let fx = FxFolders::parse(input).unwrap();
        assert_eq!(fx.len(), 1);
        assert_eq!(fx.folders[0].name, "Test");
    }

    #[test]
    fn smart_folder_items() {
        let fx = FxFolders::parse(SAMPLE).unwrap();
        let smart = fx.folder_by_name("SmartEQ").unwrap();
        assert_eq!(smart.items.len(), 1);
        assert!(matches!(
            &smart.items[0],
            FxFolderItem::SmartFilter { filter } if filter == "EQ OR equalizer"
        ));
    }

    #[test]
    fn plugin_type_round_trips() {
        for (raw, expected) in [
            (0u32, PluginType::DX),
            (2, PluginType::JS),
            (3, PluginType::VST),
            (5, PluginType::AU),
            (42, PluginType::Unknown(42)),
        ] {
            assert_eq!(PluginType::from_raw(raw), expected);
            assert_eq!(expected.to_raw(), raw);
        }
    }
}
