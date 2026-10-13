//! What the editor and the audio side agree on.
//!
//! Two things cross between them: the name of the loaded model (audio writes
//! nothing, the editor reads it to draw a header) and a request to load a new
//! one (the editor writes, the loader thread reads). The model itself never
//! crosses this way — that is [`crate::swap`]'s job, because handing a model
//! over needs the audio thread to neither allocate nor free.

use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use crate::swap::EditorEnd;

/// State shared between a plugin instance and its editor.
pub struct NamUi {
    /// Display name of the model in play. Set when a load is requested rather
    /// than when the audio thread picks it up: a name that only appears after
    /// the swap makes the click feel broken for the block or two in between.
    loaded_name: Mutex<String>,
    /// The path behind that name, so a reopened editor can show what is on.
    loaded_path: Mutex<Option<PathBuf>>,
    /// The requesting end of the swap. `None` before `activate()` — an
    /// editor can be opened on an inactive plugin, and it should still browse.
    swap: Mutex<Option<EditorEnd>>,
}

impl Default for NamUi {
    fn default() -> Self {
        Self::new()
    }
}

impl NamUi {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            loaded_name: Mutex::new(String::new()),
            loaded_path: Mutex::new(None),
            swap: Mutex::new(None),
        }
    }

    /// Hand over the swap's editor end, once audio is active.
    pub fn attach(&self, end: EditorEnd) {
        *self.swap.lock().unwrap_or_else(PoisonError::into_inner) = Some(end);
        // A model chosen before the plugin was active is loaded now, so
        // opening an editor, picking an amp and then pressing play works.
        let pending = self
            .loaded_path
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(path) = pending {
            self.request(path);
        }
    }

    /// The name to show in the header.
    #[must_use]
    pub fn loaded_name(&self) -> String {
        self.loaded_name
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The path in play, if any.
    #[must_use]
    pub fn loaded_path(&self) -> Option<PathBuf> {
        self.loaded_path
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Play `path`, showing it as `name`.
    pub fn load(&self, name: &str, path: &str) {
        let path = PathBuf::from(path);
        {
            let mut current = self
                .loaded_name
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            current.clear();
            current.push_str(name);
        }
        *self
            .loaded_path
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(path.clone());
        self.request(path);
    }

    /// Ask the loader for a path, if the plugin is active.
    fn request(&self, path: PathBuf) {
        let swap = self.swap.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(end) = swap.as_ref() {
            if !end.request(path) {
                tracing::warn!("nam: load request dropped — the loader is behind");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_editor_shows_no_model() {
        let ui = NamUi::new();
        assert_eq!(ui.loaded_name(), "");
        assert!(ui.loaded_path().is_none());
    }

    /// Choosing a model before the plugin is active must not be lost: hosts
    /// let you open an editor on a stopped transport.
    #[test]
    fn a_choice_made_while_inactive_is_remembered_and_replayed() {
        let ui = NamUi::new();
        ui.load("Plexi 51", "/models/plexi.nam");
        assert_eq!(ui.loaded_name(), "Plexi 51");
        assert_eq!(ui.loaded_path(), Some(PathBuf::from("/models/plexi.nam")));

        let (_audio, editor) = crate::swap::start(48_000.0, 512);
        ui.attach(editor);
        assert_eq!(
            ui.loaded_path(),
            Some(PathBuf::from("/models/plexi.nam")),
            "attaching replays the choice rather than clearing it"
        );
    }

    #[test]
    fn loading_replaces_the_previous_choice() {
        let ui = NamUi::new();
        ui.load("First", "/a.nam");
        ui.load("Second", "/b.nam");
        assert_eq!(ui.loaded_name(), "Second");
        assert_eq!(ui.loaded_path(), Some(PathBuf::from("/b.nam")));
    }
}
