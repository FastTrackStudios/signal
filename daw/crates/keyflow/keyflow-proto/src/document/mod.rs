//! Multi-block document support for .kf files
//!
//! Allows .kf files to contain multiple named blocks separated by `--- name ---` delimiters.
//! Without delimiters, a file is treated as a single "keyflow" block (backward compatible).

use serde::{Deserialize, Serialize};

/// A .kf document containing one or more named blocks
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KfDocument {
    /// All blocks in the document (typically in order: keyflow, chordpro, voicings)
    pub blocks: Vec<KfBlock>,
}

impl KfDocument {
    /// Create a new document with a single block
    pub fn new(block: KfBlock) -> Self {
        Self {
            blocks: vec![block],
        }
    }

    /// Find a block by name (case-insensitive)
    pub fn find_block(&self, name: &str) -> Option<&KfBlock> {
        self.blocks
            .iter()
            .find(|b| b.name.eq_ignore_ascii_case(name))
    }

    /// Get all blocks of a specific kind
    pub fn blocks_by_kind(&self, kind: KfBlockKind) -> Vec<&KfBlock> {
        self.blocks.iter().filter(|b| b.kind == kind).collect()
    }

    /// Check if this document has only a single "keyflow" block (plain old format)
    pub fn is_plain_keyflow(&self) -> bool {
        self.blocks.len() == 1 && self.blocks[0].kind == KfBlockKind::Keyflow
    }
}

/// A named block within a .kf document
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KfBlock {
    /// Block name (e.g., "keyflow", "chordpro", "voicings")
    pub name: String,

    /// The kind of block (determines parsing strategy)
    pub kind: KfBlockKind,

    /// Raw block content (lazy-parsed on demand)
    pub content: String,
}

impl KfBlock {
    /// Create a new block with the given name and content
    pub fn new(name: impl Into<String>, kind: KfBlockKind, content: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind,
            content: content.into(),
        }
    }

    /// Create a plain keyflow block (default type)
    pub fn keyflow(content: impl Into<String>) -> Self {
        Self::new("keyflow", KfBlockKind::Keyflow, content)
    }

    /// Create a ChordPro block
    pub fn chordpro(content: impl Into<String>) -> Self {
        Self::new("chordpro", KfBlockKind::ChordPro, content)
    }

    /// Create a voicings block
    pub fn voicings(content: impl Into<String>) -> Self {
        Self::new("voicings", KfBlockKind::Voicings, content)
    }
}

/// Enum describing the semantic type/kind of a block
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KfBlockKind {
    /// Standard Keyflow chart notation
    Keyflow,
    /// ChordPro format (chord-over-lyrics)
    ChordPro,
    /// Voicing notation with exact pitches
    Voicings,
    /// Unknown/custom block type
    Unknown,
}

impl KfBlockKind {
    /// Detect block kind from name (case-insensitive)
    pub fn from_name(name: &str) -> Self {
        match name.to_lowercase().as_str() {
            "keyflow" => KfBlockKind::Keyflow,
            "chordpro" | "chord-pro" | "chord pro" => KfBlockKind::ChordPro,
            "voicings" | "voicing" => KfBlockKind::Voicings,
            _ => KfBlockKind::Unknown,
        }
    }

    /// Get the default name for this kind
    pub fn default_name(&self) -> &'static str {
        match self {
            KfBlockKind::Keyflow => "keyflow",
            KfBlockKind::ChordPro => "chordpro",
            KfBlockKind::Voicings => "voicings",
            KfBlockKind::Unknown => "unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kf_document_creation() {
        let doc = KfDocument::new(KfBlock::keyflow("C //// G ////"));
        assert_eq!(doc.blocks.len(), 1);
        assert!(doc.is_plain_keyflow());
    }

    #[test]
    fn test_find_block() {
        let blocks = vec![
            KfBlock::keyflow("C //// G ////"),
            KfBlock::chordpro("{title: Test}\n[vs]\n[C]Verse"),
        ];
        let doc = KfDocument { blocks };

        assert!(doc.find_block("keyflow").is_some());
        assert!(doc.find_block("ChordPro").is_some());
        assert!(doc.find_block("voicings").is_none());
    }

    #[test]
    fn test_block_kind_detection() {
        assert_eq!(KfBlockKind::from_name("keyflow"), KfBlockKind::Keyflow);
        assert_eq!(KfBlockKind::from_name("ChordPro"), KfBlockKind::ChordPro);
        assert_eq!(KfBlockKind::from_name("VOICINGS"), KfBlockKind::Voicings);
        assert_eq!(KfBlockKind::from_name("custom"), KfBlockKind::Unknown);
    }
}
