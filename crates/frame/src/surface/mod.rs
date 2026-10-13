//! Surfaces: control surfaces — their protocol, runtime, widgets, kit and in-memory host.

pub mod proto;

#[cfg(feature = "runtime")]
pub mod runtime;

#[cfg(feature = "runtime")]
pub mod widgets;

#[cfg(feature = "runtime")]
pub mod kit;

#[cfg(feature = "runtime")]
pub mod memory;
