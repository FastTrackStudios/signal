//! The studio's shared pieces: the project as one immutable snapshot
//! ([`project`]), the handles that compare it by pointer ([`ProjectRef`],
//! [`RowsRef`]), how a shut folder shows what it holds ([`folded`]), and
//! the colours and panel geometry the DOM around the painted arrangement
//! agrees with ([`lanes::Colors`], [`panel::TINT_W`]).
//!
//! The studio window this module was — a DOM arrangement, rails, ruler,
//! transport and track panel for a WRY WebView — is retired. The
//! arrangement is painted by ONE thing, `session_daw::widget::
//! ArrangementWidget`, which the Session app mounts; see that crate.
//!
//! # Nothing is copied that could be shared
//!
//! Dioxus hands a component its props by value on every render, so
//! project data travels behind [`ProjectRef`]/[`RowsRef`], which compare
//! by pointer — a plain `Arc<Project>` would deep-compare the whole
//! session every time dioxus checked whether to memoise a child.

pub mod folded;
pub mod lanes;
pub mod panel;
pub mod project;

use std::ops::Deref;
use std::sync::Arc;

use daw_proto::Track;

pub use project::Project;

/// The project, compared by pointer.
///
/// This exists because dioxus memoises a child by `PartialEq` on its
/// props, and `Arc<T>`'s `PartialEq` compares the *contents* unless `T:
/// Eq` — which `Project` is not, holding floats. A bare `Arc<Project>`
/// prop therefore deep-compares an entire session on every render of the
/// parent, which is precisely the work the `Arc` was reached for to
/// avoid. Two handles on the same snapshot are the same snapshot; a new
/// snapshot is a new allocation.
#[derive(Clone)]
pub struct ProjectRef(pub Arc<Project>);

impl PartialEq for ProjectRef {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Deref for ProjectRef {
    type Target = Project;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// The visible track list with each track's folder depth, compared by
/// pointer for the same reason as [`ProjectRef`]. Rebuilt only when the
/// project lands or a folder is opened or closed.
#[derive(Clone)]
pub struct RowsRef(pub Arc<Vec<(Track, u32)>>);

impl PartialEq for RowsRef {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Deref for RowsRef {
    type Target = Vec<(Track, u32)>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
