//! Parameter curation — defines which parameters appear in a block's custom GUI.
//!
//! Instead of showing all 100+ raw plugin parameters, [`ParamCuration`] specifies
//! an ordered subset of "featured" parameter IDs for the curated knob grid view.

use facet::Facet;
use serde::{Deserialize, Serialize};

/// Defines which parameters appear in the custom GUI for a block.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Facet)]
pub struct ParamCuration {
    /// Ordered list of parameter IDs to show in the curated view.
    pub featured_param_ids: Vec<String>,
}

impl ParamCuration {
    pub fn new(featured: Vec<String>) -> Self {
        Self {
            featured_param_ids: featured,
        }
    }

    /// Whether a parameter ID is featured.
    pub fn is_featured(&self, param_id: &str) -> bool {
        self.featured_param_ids.iter().any(|id| id == param_id)
    }

    /// Number of featured parameters.
    pub fn len(&self) -> usize {
        self.featured_param_ids.len()
    }

    /// Whether the curation list is empty.
    pub fn is_empty(&self) -> bool {
        self.featured_param_ids.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_featured_checks_membership() {
        let curation = ParamCuration::new(vec!["gain".into(), "tone".into()]);
        assert!(curation.is_featured("gain"));
        assert!(curation.is_featured("tone"));
        assert!(!curation.is_featured("volume"));
    }

    #[test]
    fn empty_curation() {
        let curation = ParamCuration::default();
        assert!(curation.is_empty());
        assert_eq!(curation.len(), 0);
    }

    #[test]
    fn serde_round_trip() {
        let curation = ParamCuration::new(vec!["gain".into(), "tone".into(), "level".into()]);
        let json = serde_json::to_string(&curation).unwrap();
        let parsed: ParamCuration = serde_json::from_str(&json).unwrap();
        assert_eq!(curation, parsed);
    }
}
