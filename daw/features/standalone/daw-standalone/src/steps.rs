//! Edit steps as they finish: one listener told each time a project gains
//! an undo step, or undoes or redoes one — for whatever keeps a history
//! beside the engine (the project file, a shared doc) to record that step
//! as one of its own.

use std::sync::{Arc, RwLock};

/// What is told: the project's guid and the step's name.
pub type Listener = Arc<dyn Fn(&str, &str) + Send + Sync>;

static LISTENER: RwLock<Option<Listener>> = RwLock::new(None);

/// Listen for finished steps, in place of any listener before.
pub fn listen(listener: Listener) {
    *LISTENER
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(listener);
}

/// Tell the listener `project` finished the step `label`. Called with no
/// engine lock held: the listener may read the engine.
pub(crate) fn finished(project: &str, label: &str) {
    let listener = LISTENER
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    if let Some(listener) = listener {
        listener(project, label);
    }
}
