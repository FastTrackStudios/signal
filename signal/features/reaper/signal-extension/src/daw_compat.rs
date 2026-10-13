//! Temporary compatibility shims for daw APIs removed in daw main.
//!
//! TODO(daw-track-api): daw main removed per-track ext-state (`P_EXT`) and
//! track state-chunk get/set from the async `TrackHandle` surface (only the
//! REAPER-wide `Daw::ext_state()` remains, plus a low-level read helper in
//! `daw-reaper`'s `sync_api`). These stubs keep signal compiling; the
//! underlying behavior is DISABLED at runtime until daw restores the APIs
//! (or signal migrates to a replacement). Each call logs a one-line warning.
#![allow(dead_code, async_fn_in_trait)]

use daw::rpc::{Result, TrackHandle};

/// Drop-in replacement for the removed `TrackHandle` ext-state / chunk methods.
/// Returns `daw::rpc::Result` so existing `?` / `map_err` call sites are
/// unchanged — only a `use` of this trait is added.
pub trait TrackHandleCompat {
    async fn set_ext_state(&self, section: &str, key: &str, value: &str) -> Result<()>;
    async fn get_ext_state(&self, section: &str, key: &str) -> Result<Option<String>>;
    async fn get_chunk(&self) -> Result<String>;
    async fn set_chunk(&self, chunk: String) -> Result<()>;
}

impl TrackHandleCompat for TrackHandle {
    async fn set_ext_state(&self, section: &str, key: &str, _value: &str) -> Result<()> {
        eprintln!(
            "[signal] STUB daw_compat::set_ext_state({section}:{key}) — track P_EXT \
             write unavailable on daw main; no-op (TODO daw-track-api)"
        );
        Ok(())
    }

    async fn get_ext_state(&self, _section: &str, _key: &str) -> Result<Option<String>> {
        // Read path: behave as "key absent" so callers fall back to defaults.
        Ok(None)
    }

    async fn get_chunk(&self) -> Result<String> {
        eprintln!(
            "[signal] STUB daw_compat::get_chunk — track state-chunk read unavailable \
             on daw main; returning empty chunk (TODO daw-track-api)"
        );
        Ok(String::new())
    }

    async fn set_chunk(&self, _chunk: String) -> Result<()> {
        eprintln!(
            "[signal] STUB daw_compat::set_chunk — track state-chunk write unavailable \
             on daw main; no-op (TODO daw-track-api)"
        );
        Ok(())
    }
}
