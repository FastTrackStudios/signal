//! Parser and serializer for REAPER external state persistence files
//! (`reaper-extstate.ini`).

mod parse;
mod serialize;
mod types;

pub use parse::ParseError;
pub use types::*;

impl ExtState {
    /// Parse the contents of a `reaper-extstate.ini` file.
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        parse::parse(input)
    }

    /// Serialize back to the `.ini` file format.
    pub fn serialize(&self) -> String {
        serialize::serialize(self)
    }

    /// Look up a value by section and key. Returns `None` if either is absent.
    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections
            .iter()
            .find(|s| s.name == section)
            .and_then(|s| s.entries.iter().find(|e| e.key == key))
            .map(|e| e.value.as_str())
    }

    /// Set (or insert) a value in the given section, creating the section if
    /// it does not already exist.
    pub fn set(&mut self, section: &str, key: &str, value: &str) {
        if let Some(sec) = self.sections.iter_mut().find(|s| s.name == section) {
            if let Some(entry) = sec.entries.iter_mut().find(|e| e.key == key) {
                entry.value = value.to_string();
            } else {
                sec.entries.push(ExtStateEntry {
                    key: key.to_string(),
                    value: value.to_string(),
                });
            }
        } else {
            self.sections.push(ExtStateSection {
                name: section.to_string(),
                entries: vec![ExtStateEntry {
                    key: key.to_string(),
                    value: value.to_string(),
                }],
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
[MyExtension]\n\
someKey=hello\n\
counter=42\n\
\n\
[AnotherPlugin]\n\
path=C:\\Projects\n";

    #[test]
    fn parse_sections() {
        let state = ExtState::parse(SAMPLE).unwrap();
        assert_eq!(state.sections.len(), 2);
        assert_eq!(state.sections[0].name, "MyExtension");
        assert_eq!(state.sections[1].name, "AnotherPlugin");
    }

    #[test]
    fn parse_entries() {
        let state = ExtState::parse(SAMPLE).unwrap();
        assert_eq!(state.sections[0].entries.len(), 2);
        assert_eq!(state.sections[0].entries[0].key, "someKey");
        assert_eq!(state.sections[0].entries[0].value, "hello");
        assert_eq!(state.sections[0].entries[1].key, "counter");
        assert_eq!(state.sections[0].entries[1].value, "42");
    }

    #[test]
    fn get_helper() {
        let state = ExtState::parse(SAMPLE).unwrap();
        assert_eq!(state.get("MyExtension", "someKey"), Some("hello"));
        assert_eq!(state.get("MyExtension", "counter"), Some("42"));
        assert_eq!(state.get("AnotherPlugin", "path"), Some("C:\\Projects"));
        assert_eq!(state.get("Missing", "key"), None);
        assert_eq!(state.get("MyExtension", "nokey"), None);
    }

    #[test]
    fn set_new_key() {
        let mut state = ExtState::parse(SAMPLE).unwrap();
        state.set("MyExtension", "newKey", "newVal");
        assert_eq!(state.get("MyExtension", "newKey"), Some("newVal"));
    }

    #[test]
    fn set_existing_key() {
        let mut state = ExtState::parse(SAMPLE).unwrap();
        state.set("MyExtension", "someKey", "updated");
        assert_eq!(state.get("MyExtension", "someKey"), Some("updated"));
    }

    #[test]
    fn set_creates_section() {
        let mut state = ExtState::parse(SAMPLE).unwrap();
        state.set("BrandNewSection", "x", "1");
        assert_eq!(state.get("BrandNewSection", "x"), Some("1"));
    }

    #[test]
    fn round_trip() {
        let state = ExtState::parse(SAMPLE).unwrap();
        let out = state.serialize();
        let state2 = ExtState::parse(&out).unwrap();
        assert_eq!(state.sections.len(), state2.sections.len());
        assert_eq!(
            state.get("MyExtension", "counter"),
            state2.get("MyExtension", "counter")
        );
    }

    #[test]
    fn empty_file() {
        let state = ExtState::parse("").unwrap();
        assert!(state.sections.is_empty());
    }

    #[test]
    fn value_containing_equals() {
        let input = "[Sec]\nurl=http://example.com?a=1&b=2\n";
        let state = ExtState::parse(input).unwrap();
        assert_eq!(state.get("Sec", "url"), Some("http://example.com?a=1&b=2"));
    }
}
