//! Backend-agnostic I/O as `#[architect::rpc]` services.
//!
//! `midicore` talks to no OS. It declares the *shape* a MIDI backend has —
//! enumerate ports, stream input, send output — as `#[architect::rpc]` traits.
//! architect + vox turn each sync-looking trait into a matching async client +
//! dispatcher, so a concrete adapter (`midicore-midir`, a daw-backed adapter, a
//! virtual test backend, …) is callable **in-process** *and* **over the wire**
//! with no hand-written transport code. A MIDI device living in another process
//! or on another machine becomes a `subscribe()` returning an `Rx<TimedEvent>`.
//!
//! Each trait lives in its own module: `#[architect::rpc]` emits a `Service`
//! token at the trait's module scope, so co-locating two traits would collide.
//!
//! Gated on the `vox` feature (the macro checks it via
//! `#[cfg_attr(feature = "vox", ::architect::vox::service)]`).
#![cfg(feature = "vox")]

/// Port enumeration.
pub mod ports {
    use crate::port::{Direction, PortInfo};

    #[architect::rpc]
    pub trait MidiPorts {
        /// List the ports available for `direction`.
        fn ports(&self, direction: Direction) -> Vec<PortInfo>;
    }
}

/// Streaming MIDI input.
pub mod input {
    use crate::port::{PortSelector, TimedEvent};
    use vox::Tx;

    #[architect::rpc]
    pub trait MidiInput {
        /// Open the port(s) chosen by `selector` and push every incoming event
        /// into `tx` until the subscription is dropped. The client half is an
        /// `Rx<TimedEvent>` the caller drives like any stream.
        async fn subscribe(&self, selector: PortSelector, tx: Tx<TimedEvent>);
    }
}

/// MIDI output.
pub mod output {
    use crate::event::MidiEvent;
    use crate::port::{MidiIoError, PortSelector};

    #[architect::rpc]
    pub trait MidiOutput {
        /// Send one event to the port(s) chosen by `selector`.
        fn send(&self, selector: PortSelector, event: MidiEvent) -> Result<(), MidiIoError>;
    }
}

pub use input::MidiInput;
pub use output::MidiOutput;
pub use ports::MidiPorts;
