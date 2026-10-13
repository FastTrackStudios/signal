//! Implementation of [`session_proto::key::KeyActions`].
//!
//! Each action places a key change at the edit cursor on the KEY track.
//! The work is in [`crate::key`]; this is the thirty-way fan-out that
//! gives every key its own REAPER action, so they can be bound to keys
//! and put on a toolbar.

use daw::service::{
    Items, ProjectContext, Projects, TempoMap, Tracks, transport::service::Transport,
};
use daw_proto::{DawError, DawResult};
use session_proto::key::{KeyActions, register_key_actions};

use crate::key;

/// Serves the key actions against a DAW backend.
pub struct KeyActionsImpl<D> {
    daw: D,
}

impl<D> KeyActionsImpl<D> {
    pub const fn new(daw: D) -> Self {
        Self { daw }
    }
}

/// What placing a key change needs from a backend.
pub trait KeyDaw: Tracks + Items + Transport + Projects + TempoMap + Send + Sync + 'static {}
impl<T> KeyDaw for T where
    T: Tracks + Items + Transport + Projects + TempoMap + Send + Sync + 'static
{
}

impl<D: KeyDaw> KeyActionsImpl<D> {
    /// Place `root` major or minor at the edit cursor.
    fn set(&self, root: &str, major: bool) -> DawResult<()> {
        let key = key::key_from_name(root, major)
            .ok_or_else(|| DawError::OperationFailed(format!("{root} is not a note")))?;
        let project = ProjectContext::Current;
        let at = Transport::get_position(&self.daw, project.clone());
        key::set_key_at(&self.daw, project, at, &key)
    }
}

impl<D: KeyDaw> KeyActions for KeyActionsImpl<D> {
    fn set_key_c_major(&self) -> DawResult<()> {
        self.set("C", true)
    }

    fn set_key_g_major(&self) -> DawResult<()> {
        self.set("G", true)
    }

    fn set_key_d_major(&self) -> DawResult<()> {
        self.set("D", true)
    }

    fn set_key_a_major(&self) -> DawResult<()> {
        self.set("A", true)
    }

    fn set_key_e_major(&self) -> DawResult<()> {
        self.set("E", true)
    }

    fn set_key_b_major(&self) -> DawResult<()> {
        self.set("B", true)
    }

    fn set_key_f_sharp_major(&self) -> DawResult<()> {
        self.set("F#", true)
    }

    fn set_key_c_sharp_major(&self) -> DawResult<()> {
        self.set("C#", true)
    }

    fn set_key_f_major(&self) -> DawResult<()> {
        self.set("F", true)
    }

    fn set_key_b_flat_major(&self) -> DawResult<()> {
        self.set("Bb", true)
    }

    fn set_key_e_flat_major(&self) -> DawResult<()> {
        self.set("Eb", true)
    }

    fn set_key_a_flat_major(&self) -> DawResult<()> {
        self.set("Ab", true)
    }

    fn set_key_d_flat_major(&self) -> DawResult<()> {
        self.set("Db", true)
    }

    fn set_key_g_flat_major(&self) -> DawResult<()> {
        self.set("Gb", true)
    }

    fn set_key_c_flat_major(&self) -> DawResult<()> {
        self.set("Cb", true)
    }

    fn set_key_a_minor(&self) -> DawResult<()> {
        self.set("A", false)
    }

    fn set_key_e_minor(&self) -> DawResult<()> {
        self.set("E", false)
    }

    fn set_key_b_minor(&self) -> DawResult<()> {
        self.set("B", false)
    }

    fn set_key_f_sharp_minor(&self) -> DawResult<()> {
        self.set("F#", false)
    }

    fn set_key_c_sharp_minor(&self) -> DawResult<()> {
        self.set("C#", false)
    }

    fn set_key_g_sharp_minor(&self) -> DawResult<()> {
        self.set("G#", false)
    }

    fn set_key_d_sharp_minor(&self) -> DawResult<()> {
        self.set("D#", false)
    }

    fn set_key_a_sharp_minor(&self) -> DawResult<()> {
        self.set("A#", false)
    }

    fn set_key_d_minor(&self) -> DawResult<()> {
        self.set("D", false)
    }

    fn set_key_g_minor(&self) -> DawResult<()> {
        self.set("G", false)
    }

    fn set_key_c_minor(&self) -> DawResult<()> {
        self.set("C", false)
    }

    fn set_key_f_minor(&self) -> DawResult<()> {
        self.set("F", false)
    }

    fn set_key_b_flat_minor(&self) -> DawResult<()> {
        self.set("Bb", false)
    }

    fn set_key_e_flat_minor(&self) -> DawResult<()> {
        self.set("Eb", false)
    }

    fn set_key_a_flat_minor(&self) -> DawResult<()> {
        self.set("Ab", false)
    }

    fn bake_key_signatures(&self) -> DawResult<()> {
        let count = key::bake_key_signatures(&self.daw, &ProjectContext::Current)?;
        tracing::info!(
            count,
            "[session] baked key signatures into the project file"
        );
        Ok(())
    }

    fn clear_key_changes(&self) -> DawResult<()> {
        key::clear_key_changes(&self.daw, &ProjectContext::Current)
    }
}

/// Register all thirty-two key actions with `backend`.
pub fn register_actions<D, B>(backend: &B, daw: D)
where
    D: KeyDaw,
    B: architect::action::ActionBackend + ?Sized,
{
    register_key_actions(backend, std::sync::Arc::new(KeyActionsImpl::new(daw)));
}
