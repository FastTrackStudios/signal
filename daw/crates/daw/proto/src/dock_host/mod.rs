//! Dock host — platform-portable dock/window management.

mod service;
mod types;

pub use service::*;
pub use types::{DockEvent, DockHandle, DockKind, PanelPixels, UiEventDto};
