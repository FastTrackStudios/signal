use crate::diff::DocumentDiff;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DocumentHistory {
    pub undo: Vec<DocumentDiff>,
    pub redo: Vec<DocumentDiff>,
}

impl DocumentHistory {
    pub fn push(&mut self, diff: DocumentDiff) {
        self.undo.push(diff);
        self.redo.clear();
    }
}
