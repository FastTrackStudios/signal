//! Animated mocks of each rig's interface.
//!
//! # These are pictures that move, not the rig
//!
//! Every demo here is markup plus CSS keyframes. There is no engine, no
//! audio graph, no signal, and no timer in Rust driving a re-render. That
//! is deliberate on three counts:
//!
//! - **It cannot fail.** A landing page that boots a DSP graph to show a
//!   meter moving is a landing page that white-screens when the graph
//!   fails to boot. This one renders as long as the browser can render.
//! - **It costs nothing.** CSS animation runs on the compositor. Driving
//!   four stripes from a Rust interval would re-render the whole page tens
//!   of times a second to move some rectangles.
//! - **It is honest about being a mock.** The real thing is one click
//!   away at `/rigs/<slug>`, and [`crate::routes::rig`] says plainly that
//!   it is not wired up yet. A demo that pretends to be live is a demo
//!   someone will file a bug against.
//!
//! The shapes are drawn from the real interfaces — the guitar rig's board
//! of pedals into an amp, the keys rig's layer mixer, the drum rig's pads
//! and mic faders — so the page is showing the product rather than
//! generic dashboard furniture.

mod drums;
mod guitar;
mod keys;

pub use drums::DrumsDemo;
pub use guitar::GuitarDemo;
pub use keys::KeysDemo;
