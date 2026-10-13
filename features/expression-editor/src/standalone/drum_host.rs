//! Compatibility aliases for standalone clients.
pub use crate::host::drum_host::HostLane;
pub type DrumHost<D = daw::standalone::Standalone> = crate::host::drum_host::DrumHost<D>;
pub type SharedDrumHost<D = daw::standalone::Standalone> =
    crate::host::drum_host::SharedDrumHost<D>;
