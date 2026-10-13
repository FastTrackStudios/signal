//! Backend-agnostic DAW action modules.
//!
//! `#[architect::actions]` traits plus the logic behind them, for
//! capabilities that are plain DAW operations rather than any one
//! product's domain: pre-roll, record control, track grouping, take
//! ranking. (Auto-colouring moved on to `session::color` — colour stopped
//! being generic once it needed song structure.) These used to live in `session`, which meant
//! reaching into a *session* crate for things with nothing to do with
//! setlists or songs.
//!
//! Why a separate crate and not `daw-proto`: these drive a live backend
//! (`daw::reaper::Reaper`) rather than merely defining the contract, and
//! `daw-reaper` already depends on `daw-proto` — putting them there would
//! be a dependency cycle. `daw-proto` stays the backend-agnostic
//! *definition* layer (service traits, `TracksExt`, `TrackTree`); this
//! crate is the layer above it that actually drives a backend.
//!
//! Registration follows the usual shape: the `#[architect::actions]`
//! macro emits `register_<name>_actions(backend, imp)`, and the caller
//! composes any namespace nesting by wrapping its `ActionBackend` in
//! `architect::action::ScopedActionBackend`.

pub mod group_manager;
pub mod groups;
pub mod preroll;
pub mod record;
pub mod take_ranking;

/// Register every action this crate declares against `backend`.
///
/// One call instead of five: a module added here reaches every host that
/// already calls this, rather than silently not registering until each
/// call site remembers to add a line. `daw` is the backend the handlers
/// drive (`daw::reaper::Reaper` in production).
pub fn register_all_actions<D, B>(backend: &B, daw: D)
where
    D: daw::service::Projects + daw::service::ActionRegistration + Send + Sync + 'static,
    B: ::architect::action::ActionBackend + ?Sized,
{
    groups::register_actions(backend);
    preroll::register_actions(backend, daw);
    record::register_actions(backend);
    take_ranking::register_actions(backend);
}
