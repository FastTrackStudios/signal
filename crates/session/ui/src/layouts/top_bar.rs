//! Shared connection-state enum and version constant for the session UI.
//!
//! The original dock-less `TopBar` layout — with its `ConnectionStatusBadge`,
//! `LatencyBadge`, and nav tabs — was superseded by the dock-based
//! [`crate::shell::SessionShell`] and its `ConnectionBadge`. Only the shared
//! `ConnectionState` / `VERSION` remain; everything else was orphaned.

/// Application version string.
pub const VERSION: &str = "v0.1.0-alpha";

/// Connection state surfaced by the shell's connection badge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ConnectionState {
    #[default]
    Disconnected,
    Connecting,
    Connected,
}
