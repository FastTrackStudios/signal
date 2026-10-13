//! Compatibility aliases for standalone clients.
pub use expression_editor_host::drum_host::HostLane;
pub type DrumHost<D = daw::standalone::Standalone> = expression_editor_host::drum_host::DrumHost<D>;
pub type SharedDrumHost<D = daw::standalone::Standalone> =
    expression_editor_host::drum_host::SharedDrumHost<D>;
