//! Low-level INI file reader/writer.
//!
//! REAPER's ini format uses `[section]` headers with `key=value` lines.
//! This module preserves the original file structure (comments, ordering,
//! blank lines) so round-tripping doesn't destroy user formatting.

use std::collections::HashMap;

/// A parsed INI file that preserves original structure for round-tripping.
#[derive(Debug, Clone)]
pub struct IniFile {
    /// Raw lines, preserving order and formatting.
    lines: Vec<IniLine>,
}

#[derive(Debug, Clone)]
enum IniLine {
    /// A `[section]` header.
    Section(String),
    /// A `key=value` pair (section it belongs to is tracked by position).
    KeyValue {
        section: String,
        key: String,
        value: String,
    },
    /// Comment, blank line, or anything else we don't parse.
    Other(String),
}

impl IniFile {
    /// Parse an INI file from text.
    pub fn parse(text: &str) -> Self {
        let mut lines = Vec::new();
        let mut current_section = String::new();

        for raw_line in text.lines() {
            let trimmed = raw_line.trim();

            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                current_section = trimmed[1..trimmed.len() - 1].to_string();
                lines.push(IniLine::Section(current_section.clone()));
            } else if let Some(eq_pos) = trimmed.find('=') {
                let key = trimmed[..eq_pos].to_string();
                let value = trimmed[eq_pos + 1..].to_string();
                lines.push(IniLine::KeyValue {
                    section: current_section.clone(),
                    key,
                    value,
                });
            } else {
                lines.push(IniLine::Other(raw_line.to_string()));
            }
        }

        Self { lines }
    }

    /// Get a value by section and key.
    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        for line in &self.lines {
            if let IniLine::KeyValue {
                section: s,
                key: k,
                value: v,
            } = line
                && s.eq_ignore_ascii_case(section)
                && k == key
            {
                return Some(v.as_str());
            }
        }
        None
    }

    /// Set a value. If the key exists in the section, update it.
    /// If not, append it after the section header.
    /// If the section doesn't exist, create it at the end.
    pub fn set(&mut self, section: &str, key: &str, value: &str) {
        // Try to update existing key.
        for line in &mut self.lines {
            if let IniLine::KeyValue {
                section: s,
                key: k,
                value: v,
            } = line
                && s.eq_ignore_ascii_case(section)
                && k == key
            {
                *v = value.to_string();
                return;
            }
        }

        // Key doesn't exist — find the section and insert after its header.
        let section_lower = section.to_ascii_lowercase();
        for i in 0..self.lines.len() {
            if let IniLine::Section(s) = &self.lines[i]
                && s.eq_ignore_ascii_case(section)
            {
                self.lines.insert(
                    i + 1,
                    IniLine::KeyValue {
                        section: s.clone(),
                        key: key.to_string(),
                        value: value.to_string(),
                    },
                );
                return;
            }
        }

        // Section doesn't exist — create it.
        self.lines.push(IniLine::Section(section.to_string()));
        self.lines.push(IniLine::KeyValue {
            section: section.to_string(),
            key: key.to_string(),
            value: value.to_string(),
        });
    }

    /// Remove a key from a section. Returns the old value if it existed.
    pub fn remove(&mut self, section: &str, key: &str) -> Option<String> {
        let pos = self.lines.iter().position(|line| {
            matches!(line, IniLine::KeyValue { section: s, key: k, .. }
                if s.eq_ignore_ascii_case(section) && k == key)
        });
        if let Some(i) = pos
            && let IniLine::KeyValue { value, .. } = self.lines.remove(i)
        {
            return Some(value);
        }
        None
    }

    /// Get all key-value pairs in a section.
    pub fn section(&self, section: &str) -> HashMap<&str, &str> {
        let mut map = HashMap::new();
        for line in &self.lines {
            if let IniLine::KeyValue {
                section: s,
                key: k,
                value: v,
            } = line
                && s.eq_ignore_ascii_case(section)
            {
                map.insert(k.as_str(), v.as_str());
            }
        }
        map
    }

    /// Get all key-value pairs in a section as an ordered list.
    ///
    /// Unlike [`section`](Self::section), this preserves insertion order and
    /// includes duplicate keys (callers can apply their own dedup policy).
    pub fn section_entries(&self, section: &str) -> Vec<(&str, &str)> {
        self.lines
            .iter()
            .filter_map(|line| {
                if let IniLine::KeyValue {
                    section: s,
                    key: k,
                    value: v,
                } = line
                    && s.eq_ignore_ascii_case(section)
                {
                    return Some((k.as_str(), v.as_str()));
                }
                None
            })
            .collect()
    }

    /// List all section names in order of first appearance.
    pub fn sections(&self) -> Vec<&str> {
        let mut seen = Vec::new();
        for line in &self.lines {
            if let IniLine::Section(s) = line
                && !seen.contains(&s.as_str())
            {
                seen.push(s.as_str());
            }
        }
        seen
    }

    /// Serialize back to INI text, preserving original structure.
    pub fn serialize(&self) -> String {
        let mut output = String::new();
        for line in &self.lines {
            match line {
                IniLine::Section(s) => {
                    output.push('[');
                    output.push_str(s);
                    output.push(']');
                }
                IniLine::KeyValue { key, value, .. } => {
                    output.push_str(key);
                    output.push('=');
                    output.push_str(value);
                }
                IniLine::Other(s) => {
                    output.push_str(s);
                }
            }
            output.push('\n');
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let input = "[REAPER]\nundomaxmem=256\nlastthemefn5=Default\n\n[reaper_video]\nvisible=1\n";
        let ini = IniFile::parse(input);
        assert_eq!(ini.get("REAPER", "undomaxmem"), Some("256"));
        assert_eq!(ini.get("reaper_video", "visible"), Some("1"));
        assert_eq!(ini.serialize(), input);
    }

    #[test]
    fn set_existing_key() {
        let input = "[REAPER]\nundomaxmem=256\n";
        let mut ini = IniFile::parse(input);
        ini.set("REAPER", "undomaxmem", "512");
        assert_eq!(ini.get("REAPER", "undomaxmem"), Some("512"));
    }

    #[test]
    fn set_new_key_existing_section() {
        let input = "[REAPER]\nundomaxmem=256\n";
        let mut ini = IniFile::parse(input);
        ini.set("REAPER", "autosaveint", "300");
        assert_eq!(ini.get("REAPER", "autosaveint"), Some("300"));
    }

    #[test]
    fn set_new_section() {
        let input = "[REAPER]\nundomaxmem=256\n";
        let mut ini = IniFile::parse(input);
        ini.set("reaper_video", "visible", "1");
        assert_eq!(ini.get("reaper_video", "visible"), Some("1"));
        assert!(ini.serialize().contains("[reaper_video]"));
    }

    #[test]
    fn remove_key() {
        let input = "[REAPER]\nundomaxmem=256\nautosaveint=300\n";
        let mut ini = IniFile::parse(input);
        let old = ini.remove("REAPER", "undomaxmem");
        assert_eq!(old, Some("256".to_string()));
        assert_eq!(ini.get("REAPER", "undomaxmem"), None);
        assert_eq!(ini.get("REAPER", "autosaveint"), Some("300"));
    }
}
