//! Tiny key/value prefs — one storage API for every surface the app
//! ships on.
//!
//! The implementation moved to `utils::prefs` (#193) so the expression
//! editor can use it too: it lived here, in an app crate, which no
//! library can depend on. This forwards rather than re-exports so the
//! app's existing `prefs::get`/`set`/`remove` call sites are unchanged.
//!
//! Native keeps one file per key under ~/.config/fts; the web build
//! keeps the same keys in localStorage. Typed, versioned preferences go
//! through `utils::prefs::{load, store}` instead.

pub fn get(key: &str) -> Option<String> {
    utils::prefs::get_raw(key)
}

pub fn set(key: &str, value: &str) {
    utils::prefs::set_raw(key, value)
}

pub fn remove(key: &str) {
    utils::prefs::remove(key)
}
