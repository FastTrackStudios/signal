//! Targets baseview has no windowing for — iOS, Android: the platform types
//! exist so the crate (and what is built on it, the plugin UIs' shared
//! widgets) compiles there, and opening a window says it cannot.
//!
//! An app on such a target draws its UI some other way (Session on iOS runs
//! the same Blitz components in its own window); only a plugin editor, which
//! is the one thing that needs a baseview window, is unavailable.

use crate::dpi::Size;
use crate::*;
use raw_window_handle::{DisplayHandle, HasWindowHandle};
use std::fmt;

/// Why a window could not be opened (here: never can) or a handler failed.
#[derive(Debug)]
pub enum PlatformError {
    Handler(HandlerError),
    /// Baseview has no windowing on this target.
    Unsupported,
}

impl fmt::Display for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Handler(e) => e.fmt(f),
            Self::Unsupported => f.write_str("baseview has no windowing on this platform"),
        }
    }
}

impl std::error::Error for PlatformError {}

impl From<HandlerError> for PlatformError {
    fn from(e: HandlerError) -> Self {
        Self::Handler(e)
    }
}

pub(crate) type Result<T> = std::result::Result<T, PlatformError>;

/// A window never opened here.
pub struct WindowHandle {
    _never: std::convert::Infallible,
}

impl WindowHandle {
    pub fn create_window(_init: WindowInitializer) -> Result<Self> {
        Err(PlatformError::Unsupported)
    }

    pub fn run_until_closed(self) -> Result<()> {
        match self._never {}
    }

    pub fn is_open(&self) -> bool {
        match self._never {}
    }

    pub fn is_resizable(&self) -> bool {
        match self._never {}
    }

    pub fn min_size(&self) -> Option<Size> {
        match self._never {}
    }

    pub fn max_size(&self) -> Option<Size> {
        match self._never {}
    }

    pub fn handle_main_thread_callback(&self) {
        match self._never {}
    }

    pub fn size(&self) -> WindowSize {
        match self._never {}
    }

    pub fn resize(&self, _size: Size) -> Result<()> {
        match self._never {}
    }

    pub fn suggest_scale_factor(&self, _scale_factor: f64) -> Result<()> {
        match self._never {}
    }

    pub fn set_parent(&self, _new_parent: ParentWindowHandle) -> Result<()> {
        match self._never {}
    }

    pub fn show(&self) -> Result<()> {
        match self._never {}
    }

    pub fn hide(&self) -> Result<()> {
        match self._never {}
    }
}

/// The context a handler is given — never, as no window opens.
#[derive(Clone)]
pub struct WindowContext {
    _never: std::convert::Infallible,
}

impl WindowContext {
    pub fn request_close(&self) {
        match self._never {}
    }

    pub fn has_focus(&self) -> bool {
        match self._never {}
    }

    pub fn focus(&self) -> Result<()> {
        match self._never {}
    }

    pub fn resize(&self, _size: Size) -> Result<()> {
        match self._never {}
    }

    pub fn set_mouse_cursor(&self, _cursor: MouseCursor) -> Result<()> {
        match self._never {}
    }

    pub fn size(&self) -> WindowSize {
        match self._never {}
    }

    pub fn scale_factor(&self) -> f64 {
        match self._never {}
    }

    pub fn window_handle(&self) -> Option<raw_window_handle::WindowHandle<'_>> {
        match self._never {}
    }

    pub fn display_handle(&self) -> DisplayHandle<'_> {
        match self._never {}
    }

    pub fn platform_handle(&self) -> PlatformHandle {
        match self._never {}
    }
}

impl fmt::Debug for WindowContext {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self._never {}
    }
}

/// A handle to a window — never, as no window opens.
#[derive(Clone)]
pub struct PlatformHandle {
    _never: std::sync::Arc<std::convert::Infallible>,
}

impl PlatformHandle {
    pub fn window_handle(&self) -> Option<raw_window_handle::WindowHandle<'_>> {
        match *self._never {}
    }

    pub fn display_handle(&self) -> DisplayHandle<'_> {
        match *self._never {}
    }
}

impl fmt::Debug for PlatformHandle {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self._never {}
    }
}

/// A parent window a child would open into — there is none to take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParentWindowHandle(());

impl ParentWindowHandle {
    pub fn extract(_window: &impl HasWindowHandle) -> std::result::Result<Self, PlatformError> {
        Err(PlatformError::Unsupported)
    }
}

/// No system clipboard reached from here.
pub fn copy_to_clipboard(_data: &str) {}
