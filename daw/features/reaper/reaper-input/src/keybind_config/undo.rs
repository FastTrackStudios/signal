//! Undo system for config edits — snapshot-based rollback.
//!
//! Since .styx files are small (typically <5KB), we snapshot the entire
//! file content before each edit. Undo restores the previous snapshot.

use std::cell::RefCell;
use std::path::PathBuf;

const MAX_UNDO_DEPTH: usize = 50;

/// A single undo entry.
#[derive(Clone, Debug)]
struct UndoEntry {
    /// Which file was modified.
    path: PathBuf,
    /// The file content before the edit.
    previous_content: String,
    /// Description of what was changed.
    description: String,
}

// Thread-local undo stack.
thread_local! {
    static UNDO_STACK: RefCell<Vec<UndoEntry>> = const { RefCell::new(Vec::new()) };
    static REDO_STACK: RefCell<Vec<UndoEntry>> = const { RefCell::new(Vec::new()) };
}

/// Save a snapshot of a file before editing it.
/// Call this BEFORE making changes.
pub fn snapshot(path: &std::path::Path, description: &str) {
    if let Ok(content) = std::fs::read_to_string(path) {
        UNDO_STACK.with(|stack| {
            let mut stack = stack.borrow_mut();
            stack.push(UndoEntry {
                path: path.to_path_buf(),
                previous_content: content,
                description: description.to_string(),
            });
            // Trim to max depth
            if stack.len() > MAX_UNDO_DEPTH {
                stack.remove(0);
            }
        });
        // Clear redo stack on new edit
        REDO_STACK.with(|stack| stack.borrow_mut().clear());
    }
}

/// Undo the last config edit. Returns the description of what was undone.
pub fn undo() -> Option<String> {
    UNDO_STACK.with(|undo| {
        let mut undo = undo.borrow_mut();
        let entry = undo.pop()?;

        // Save current state for redo
        if let Ok(current) = std::fs::read_to_string(&entry.path) {
            REDO_STACK.with(|redo| {
                redo.borrow_mut().push(UndoEntry {
                    path: entry.path.clone(),
                    previous_content: current,
                    description: entry.description.clone(),
                });
            });
        }

        // Restore previous content
        if let Err(e) = std::fs::write(&entry.path, &entry.previous_content) {
            tracing::error!("Undo failed: {e}");
            return None;
        }

        tracing::info!(
            desc = entry.description.as_str(),
            "Undo: restored previous config"
        );
        Some(entry.description)
    })
}

/// Redo the last undone edit. Returns the description.
pub fn redo() -> Option<String> {
    REDO_STACK.with(|redo| {
        let mut redo = redo.borrow_mut();
        let entry = redo.pop()?;

        // Save current for undo again
        if let Ok(current) = std::fs::read_to_string(&entry.path) {
            UNDO_STACK.with(|undo| {
                undo.borrow_mut().push(UndoEntry {
                    path: entry.path.clone(),
                    previous_content: current,
                    description: entry.description.clone(),
                });
            });
        }

        if let Err(e) = std::fs::write(&entry.path, &entry.previous_content) {
            tracing::error!("Redo failed: {e}");
            return None;
        }

        tracing::info!(desc = entry.description.as_str(), "Redo: restored edit");
        Some(entry.description)
    })
}

/// Check if undo is available.
pub fn can_undo() -> bool {
    UNDO_STACK.with(|stack| !stack.borrow().is_empty())
}

/// Check if redo is available.
pub fn can_redo() -> bool {
    REDO_STACK.with(|stack| !stack.borrow().is_empty())
}

/// Get the description of what would be undone.
pub fn peek_undo() -> Option<String> {
    UNDO_STACK.with(|stack| stack.borrow().last().map(|e| e.description.clone()))
}

/// Get the undo stack depth.
pub fn undo_depth() -> usize {
    UNDO_STACK.with(|stack| stack.borrow().len())
}
