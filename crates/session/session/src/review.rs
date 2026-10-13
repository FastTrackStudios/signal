//! Rating takes — the `ExtState` half.
//!
//! The model, the format and every rule about what a mark means live in
//! [`session_proto::review`], where the UI can see them without
//! depending on this crate. What is here is the part that needs a DAW:
//! reading and writing a song's review through project ext state.

pub use session_proto::review::*;

/// The song's review, as stored.
#[must_use]
pub fn read<E: daw::service::ExtState>(ext: &E, project: daw::service::ProjectContext) -> Review {
    ext.get_project(project, SECTION, KEY)
        .as_deref()
        .map(Review::from_stored)
        .unwrap_or_default()
}

/// Keep it with the song: one write.
///
/// # Errors
///
/// Whatever the backend's `set_project` returns.
pub fn write<E: daw::service::ExtState>(
    ext: &E,
    project: daw::service::ProjectContext,
    review: &Review,
) -> daw_proto::DawResult<()> {
    ext.set_project(project, SECTION, KEY, &review.stored())
}
