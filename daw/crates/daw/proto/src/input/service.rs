//! Input service — keyboard/mouse interception driving + action exec.
//!
//! The host captures key/mouse events from REAPER's TranslateAccel
//! hook synchronously and applies a `KeyFilter` uploaded by the
//! extension. Eaten keys would stream over the architect-rpc bridge —
//! streaming subscription retires by default; sibling trait when a
//! real consumer needs it.

use super::KeyFilter;

#[architect::rpc]
pub trait Input {
    /// Upload a key filter configuration. Replaces the previous one.
    fn set_key_filter(&self, filter: KeyFilter);

    fn get_key_filter(&self) -> KeyFilter;

    /// When disabled, TranslateAccel passes all keys through.
    fn set_enabled(&self, enabled: bool);
    fn is_enabled(&self) -> bool;

    /// Execute a REAPER action by command name or numeric ID.
    /// Supports named commands (`"FTS_SIGNAL_NEXT_SONG"`), named
    /// with prefix (`"_SWS_ABOUT"`), numeric IDs (`"40044"`).
    fn execute_action(&self, action_id: &str);
}
