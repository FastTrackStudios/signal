//! The active language: one value, project state.
//!
//! `flow.vocals.language.active`: the session has an active language,
//! and every vocal scene follows it. It is project ext-state — the same
//! mechanism `daw_module`'s `CREATE_STATE_SECTION` already uses for
//! per-track create state — read and written through
//! [`daw::service::ExtState`] so the one write works identically over a
//! live REAPER project and over `daw-standalone`, with no backend-
//! specific code on either side of the seam.

use daw::service::{ExtState, ProjectContext};
use daw_proto::DawResult;

use super::language::Language;

/// The ext-state section the active language lives under.
const SECTION: &str = "FTS_LANGUAGE";
/// The one key: the whole point of "one write" is that switching the
/// language never touches more than this.
const KEY: &str = "active";

/// The session's active language, if one has been set.
///
/// `None` when the project has never had a language switch — a session
/// with no vocals, or one nobody has switched yet. Callers that need a
/// language to hide by should treat that as "hide nothing" rather than
/// guessing one.
#[must_use]
pub fn get_active_language<E: ExtState>(ext: &E, project: ProjectContext) -> Option<Language> {
    ext.get_project(project, SECTION, KEY)
        .and_then(|value| Language::parse(&value))
}

/// Switch the active language: one write.
///
/// # Errors
///
/// Returns whatever the backend's `set_project` returns — a project
/// that does not resolve, most likely.
pub fn set_active_language<E: ExtState>(
    ext: &E,
    project: ProjectContext,
    language: Language,
) -> DawResult<()> {
    ext.set_project(project, SECTION, KEY, language.as_str())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use daw_proto::ProjectContext;

    use super::*;

    /// A minimal `ExtState` double: project-scoped keys only, which is
    /// all the active language ever touches — enough to prove the round
    /// trip without standing up a whole `daw-standalone` project.
    #[derive(Default)]
    struct Fake {
        project: Mutex<HashMap<(String, String), String>>,
    }

    impl ExtState for Fake {
        fn get(&self, _section: &str, _key: &str) -> Option<String> {
            None
        }
        fn set(&self, _section: &str, _key: &str, _value: &str, _persist: bool) -> DawResult<()> {
            Ok(())
        }
        fn delete(&self, _section: &str, _key: &str, _persist: bool) -> DawResult<()> {
            Ok(())
        }
        fn has(&self, _section: &str, _key: &str) -> bool {
            false
        }
        fn get_project(
            &self,
            _project: ProjectContext,
            section: &str,
            key: &str,
        ) -> Option<String> {
            self.project
                .lock()
                .expect("lock")
                .get(&(section.to_owned(), key.to_owned()))
                .cloned()
        }
        fn set_project(
            &self,
            _project: ProjectContext,
            section: &str,
            key: &str,
            value: &str,
        ) -> DawResult<()> {
            self.project
                .lock()
                .expect("lock")
                .insert((section.to_owned(), key.to_owned()), value.to_owned());
            Ok(())
        }
        fn delete_project(
            &self,
            _project: ProjectContext,
            section: &str,
            key: &str,
        ) -> DawResult<()> {
            self.project
                .lock()
                .expect("lock")
                .remove(&(section.to_owned(), key.to_owned()));
            Ok(())
        }
        fn has_project(&self, _project: ProjectContext, section: &str, key: &str) -> bool {
            self.project
                .lock()
                .expect("lock")
                .contains_key(&(section.to_owned(), key.to_owned()))
        }
    }

    /// An untouched project has no active language.
    #[test]
    fn no_switch_yet_is_none() {
        let ext = Fake::default();
        assert_eq!(get_active_language(&ext, ProjectContext::Current), None);
    }

    /// The switch writes one key, and reading it back gives the same
    /// language.
    #[test]
    fn the_switch_writes_once_and_reads_back() {
        let ext = Fake::default();
        set_active_language(&ext, ProjectContext::Current, Language::Es).expect("one write");
        assert_eq!(
            get_active_language(&ext, ProjectContext::Current),
            Some(Language::Es)
        );
        assert_eq!(ext.project.lock().expect("lock").len(), 1, "one write");
    }

    /// Switching again is still one key — a later switch overwrites
    /// rather than accumulating state.
    #[test]
    fn switching_again_overwrites_the_one_key() {
        let ext = Fake::default();
        set_active_language(&ext, ProjectContext::Current, Language::En).expect("one write");
        set_active_language(&ext, ProjectContext::Current, Language::Pt).expect("one write");
        assert_eq!(
            get_active_language(&ext, ProjectContext::Current),
            Some(Language::Pt)
        );
        assert_eq!(ext.project.lock().expect("lock").len(), 1, "still one key");
    }

    /// A malformed value (a stale build, a hand-edited project) is not
    /// a language rather than a panic.
    #[test]
    fn an_unparseable_value_reads_as_none() {
        let ext = Fake::default();
        ext.set_project(ProjectContext::Current, SECTION, KEY, "klingon")
            .expect("set");
        assert_eq!(get_active_language(&ext, ProjectContext::Current), None);
    }
}
