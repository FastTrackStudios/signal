//! The rig demos on the landing page.
//!
//! # Guitar and keys are the real thing; drums is a picture
//!
//! [`GuitarDemo`] and [`KeysDemo`] mount the SHIPPED control surfaces —
//! `signal-guitar-ui::ControlView` with `PerformGrid`, and
//! `signal-keys-ui::ControlView` — the same components the desktop and
//! browser remotes render. They are not mocks, so they cannot drift from
//! the product: if the landing page looks wrong, the product is wrong.
//!
//! Both run with **no engine**. Each reads its rig through
//! `try_consume_context`, so an absent client means a read-only surface
//! rather than a broken one — the crates are built for this. What the
//! demos supply is the *state* a rig would have published, and a feed that
//! updates it the way an event stream would: a pick pattern for the
//! guitar, a chord progression and meters for the keys. Nothing is
//! sampled and nothing is random; both are closed-form functions of a
//! frame counter, so every machine and every screenshot sees the same
//! thing.
//!
//! [`DrumsDemo`] is still CSS keyframes. That is fine for what it shows,
//! and it is the one left to convert.
//!
//! Both live surfaces are `inert` and `pointer-events: none`: with no
//! client in context a drag would move a control locally and reach
//! nothing, which reads as broken rather than as a demo.

mod drums;
mod guitar;
mod keys;

pub use drums::DrumsDemo;
pub use guitar::GuitarDemo;
pub use keys::KeysDemo;
