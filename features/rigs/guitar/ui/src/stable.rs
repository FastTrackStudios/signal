//! Callbacks that are made in a render but not leaked by it.
//!
//! `Callback::new` in a component body is owned by the component's scope and
//! freed only when the component unmounts — "this should not be called
//! directly in the body of a component" (dioxus-core). A panel that re-renders
//! with every meter tick and never unmounts (the Control view) therefore grew
//! without bound: two such callbacks captured the block list, and a night left
//! open took 100 GB.
//!
//! [`use_stable`] gives a component a table of callbacks keyed by call site
//! (and an optional index, for loops). The first render at a site creates the
//! callback; every later render hands it the new closure (`Callback::replace`)
//! and drops the old one. So a site costs one callback for the component's
//! life, and — unlike `use_callback` — it can be used anywhere in the render:
//! inside `if`, `match`, `Option::map`, or a loop.

use std::any::Any;
use std::collections::HashMap;
use std::panic::Location;

use dioxus::prelude::*;

type Slot = (&'static str, u32, u32, usize);

/// A component's stable callbacks — see the module docs. `Copy`, so it can
/// be captured freely.
#[derive(Clone, Copy)]
pub struct Stable(CopyValue<HashMap<Slot, Box<dyn Any>>>);

/// The component's callback table. A hook: call it unconditionally, at the
/// top of the component.
#[must_use]
pub fn use_stable() -> Stable {
    Stable(use_hook(|| CopyValue::new(HashMap::new())))
}

impl Stable {
    /// A callback for this call site, holding `f` from now on.
    #[track_caller]
    pub fn cb<A: 'static, R: 'static>(self, f: impl FnMut(A) -> R + 'static) -> Callback<A, R> {
        self.keyed(0, f)
    }

    /// As [`cb`](Self::cb), for a call site that makes several (one per
    /// `key` — e.g. a loop index).
    #[track_caller]
    pub fn keyed<A: 'static, R: 'static>(
        self,
        key: usize,
        f: impl FnMut(A) -> R + 'static,
    ) -> Callback<A, R> {
        let at = Location::caller();
        let slot = (at.file(), at.line(), at.column(), key);
        let mut table = self.0;
        let existing = table
            .read()
            .get(&slot)
            .and_then(|b| b.downcast_ref::<Callback<A, R>>().copied());
        match existing {
            Some(mut cb) => {
                cb.replace(Box::new(f));
                cb
            }
            None => {
                let cb = Callback::new(f);
                table.write().insert(slot, Box::new(cb));
                cb
            }
        }
    }

    /// How many callbacks the table holds — bounded by the component's call
    /// sites, however often it renders.
    #[must_use]
    pub fn len(self) -> usize {
        self.0.read().len()
    }

    #[must_use]
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }
}
